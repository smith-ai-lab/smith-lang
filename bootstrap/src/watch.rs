//! `smith --watch`: the command again each time a file it read changes, and the
//! runs of a program that it starts, stops, and starts again, with the sockets
//! the program listens on kept from one run to the next.
//!
//! A run is started with a socket at descriptor 3 that only `smith --watch`
//! holds the other end of. A run that listens asks for the address on it, and is
//! given the descriptor of a socket that this file keeps, so the next run is
//! given the same one and a connection that comes between two runs waits in it.
//! A run is told to stop by closing its end of that socket, and the listener of
//! the run then gives `.closed` from `accept`, as [the library](../../std/net.smith)
//! says.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitCode};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// What one build or check of the command gave.
pub struct Outcome {
    /// The executable to run, when the command runs one and it was built.
    pub executable: Option<PathBuf>,
    /// The files the command read, those that were found.
    pub read: Vec<PathBuf>,
    /// Whether the command succeeded. A failing check or build is shown and the
    /// run before goes on.
    pub ok: bool,
}

/// What `smith --watch` does with a command.
pub enum Kind {
    /// A command that runs nothing: `check` and `build`.
    Report,
    /// A command that runs the program it built, with the words it is given. A
    /// task that carries `diverge` is stopped when the next run is built, and
    /// any other run is let end.
    Run { diverge: bool, words: Vec<OsString> },
}

/// How long a run that was told to stop has to end before it is killed.
const GRACE: Duration = Duration::from_secs(10);
/// How often the files are looked at.
const POLL: Duration = Duration::from_millis(20);
/// How long the files must be as they are before they are built again, so that
/// a save in several writes is built once.
const QUIET: Duration = Duration::from_millis(60);

unsafe extern "C" {
    fn sendmsg(fd: i32, message: *const MsgHdr, flags: i32) -> isize;
    fn signal(number: i32, handler: usize) -> usize;
}

const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

/// How a run is started: the program, the words it is started with, and the end
/// of the control socket that it holds, which the library finds at descriptor 3.
pub type Starter<'a> = dyn FnMut(&Path, &[OsString], &UnixStream) -> std::io::Result<Child> + 'a;

#[repr(C)]
struct IoVec {
    base: *const u8,
    len: usize,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct MsgHdr {
    name: *const u8,
    name_length: u32,
    iov: *const IoVec,
    iov_length: i32,
    control: *const u8,
    control_length: u32,
    flags: i32,
}

#[cfg(not(target_os = "macos"))]
#[repr(C)]
struct MsgHdr {
    name: *const u8,
    name_length: u32,
    iov: *const IoVec,
    iov_length: usize,
    control: *const u8,
    control_length: usize,
    flags: i32,
}

/// The signal that interrupted `smith --watch`, or 0.
static INTERRUPTED: AtomicI32 = AtomicI32::new(0);

extern "C" fn interrupted(number: i32) {
    INTERRUPTED.store(number, Ordering::SeqCst);
}

/// Sends the bytes with a descriptor, which the other side is given as its own.
fn send_with_descriptor(
    stream: &UnixStream,
    bytes: &[u8],
    descriptor: Option<i32>,
) -> std::io::Result<()> {
    let iov = IoVec {
        base: bytes.as_ptr(),
        len: bytes.len(),
    };
    // The header of a message of control, which holds the descriptor: its
    // length, the level and the kind that say a descriptor follows, and the
    // descriptor. The layout is the system's, and macOS makes it shorter.
    let mut control = [0u8; 32];
    let used = match descriptor {
        None => 0,
        Some(fd) => {
            #[cfg(target_os = "macos")]
            {
                control[0..4].copy_from_slice(&16u32.to_ne_bytes());
                control[4..8].copy_from_slice(&0xffffi32.to_ne_bytes());
                control[8..12].copy_from_slice(&1i32.to_ne_bytes());
                control[12..16].copy_from_slice(&fd.to_ne_bytes());
                16
            }
            #[cfg(not(target_os = "macos"))]
            {
                control[0..8].copy_from_slice(&20usize.to_ne_bytes());
                control[8..12].copy_from_slice(&1i32.to_ne_bytes());
                control[12..16].copy_from_slice(&1i32.to_ne_bytes());
                control[16..20].copy_from_slice(&fd.to_ne_bytes());
                24
            }
        }
    };
    let message = MsgHdr {
        name: std::ptr::null(),
        name_length: 0,
        iov: &iov,
        iov_length: 1,
        control: control.as_ptr(),
        control_length: used,
        flags: 0,
    };
    loop {
        // SAFETY: the message points at `iov` and `control`, which live until
        // the call ends, and `iov` points at `bytes`.
        let sent = unsafe { sendmsg(stream.as_raw_fd(), &message, 0) };
        if sent >= 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// The sockets that listen, which are kept for as long as `smith --watch` lives.
#[derive(Default)]
struct Kept {
    listeners: Mutex<HashMap<(String, u32), Arc<TcpListener>>>,
}

impl Kept {
    /// The socket that listens at the address, made the first time it is asked
    /// for.
    fn listener(&self, host: &str, port: u32) -> std::io::Result<Arc<TcpListener>> {
        let mut listeners = self.listeners.lock().expect("the table of listeners");
        let key = (host.to_string(), port);
        if let Some(found) = listeners.get(&key) {
            return Ok(found.clone());
        }
        let port16 = u16::try_from(port).map_err(|_| std::io::ErrorKind::InvalidInput)?;
        let made = Arc::new(TcpListener::bind((host, port16))?);
        listeners.insert(key, made.clone());
        Ok(made)
    }
}

/// The error number of an address that is in use.
#[cfg(target_os = "macos")]
const IN_USE: i32 = 48;
#[cfg(not(target_os = "macos"))]
const IN_USE: i32 = 98;

/// What a run has asked for.
type Attached = Arc<Mutex<Vec<Arc<TcpListener>>>>;

/// Answers the lines a run writes to its end of the control socket. Each is
/// `listen <host> <port>`, answered with `o` and a descriptor of the socket
/// that listens there, or with `e` and the number of the error.
fn serve(stream: UnixStream, kept: Arc<Kept>, attached: Attached) {
    let Ok(reader) = stream.try_clone() else {
        return;
    };
    for line in BufReader::new(reader).lines() {
        let Ok(line) = line else {
            return;
        };
        let words: Vec<&str> = line.split(' ').collect();
        let ["listen", host, port] = words[..] else {
            continue;
        };
        let Ok(port) = port.parse::<u32>() else {
            continue;
        };
        let given = kept.listener(host, port).and_then(|listener| {
            let mut mine = attached.lock().expect("the listeners of a run");
            if mine.iter().any(|l| Arc::ptr_eq(l, &listener)) {
                return Err(std::io::Error::from_raw_os_error(IN_USE));
            }
            mine.push(listener.clone());
            Ok(listener)
        });
        let sent = match given {
            Ok(listener) => send_with_descriptor(&stream, b"o", Some(listener.as_raw_fd())),
            Err(error) => {
                let code = error.raw_os_error().unwrap_or(22);
                send_with_descriptor(&stream, format!("e{code}\n").as_bytes(), None)
            }
        };
        if sent.is_err() {
            return;
        }
    }
}

/// A program that is running, and what it listens on.
struct Run {
    child: Child,
    control: UnixStream,
    attached: Attached,
}

impl Run {
    fn start(
        start: &mut Starter,
        executable: &Path,
        words: &[OsString],
        kept: &Arc<Kept>,
    ) -> std::io::Result<Run> {
        let (mine, theirs) = UnixStream::pair()?;
        let child = start(executable, words, &theirs)?;
        drop(theirs);
        let attached: Attached = Arc::default();
        let (stream, kept, listeners) = (mine.try_clone()?, kept.clone(), attached.clone());
        std::thread::spawn(move || serve(stream, kept, listeners));
        Ok(Run {
            child,
            control: mine,
            attached,
        })
    }

    /// Whether the run has ended.
    fn ended(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }

    /// Tells the run to stop and waits for it: its listeners give `.closed`, it
    /// serves the connections it took, and ends. A connection made to its
    /// sockets meanwhile is made to the kept sockets, which wait for the next
    /// run. A run that listens on nothing, or has not ended by the time the
    /// grace is over, is killed.
    fn stop(&mut self) {
        let listeners: Vec<Arc<TcpListener>> = self
            .attached
            .lock()
            .expect("the listeners of a run")
            .clone();
        let _ = self.control.shutdown(Shutdown::Both);
        if !listeners.is_empty() {
            // A run that waits for a connection is woken by one: it looks at
            // its control socket before it takes it, finds it closed, and
            // leaves the connection where it is.
            for listener in &listeners {
                poke(listener);
            }
            let until = Instant::now() + GRACE;
            while !self.ended() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(5));
            }
            if !self.ended() {
                eprintln!(
                    "smith --watch: the run did not end within {} seconds of being told to stop, so it is killed",
                    GRACE.as_secs()
                );
            }
        }
        self.kill();
    }

    /// Ends the run now, and waits for it.
    fn kill(&mut self) {
        if !self.ended() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

/// Makes a connection to the socket and ends it at once.
fn poke(listener: &TcpListener) {
    let Ok(mut address) = listener.local_addr() else {
        return;
    };
    if address.ip().is_unspecified() {
        let loopback: IpAddr = if address.is_ipv6() {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        };
        address = SocketAddr::new(loopback, address.port());
    }
    let second = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), address.port());
    if TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_err() {
        let _ = TcpStream::connect_timeout(&second, Duration::from_secs(1));
    }
}

/// What a file was when it was looked at: when it was written, how long it is,
/// and what it says.
#[derive(PartialEq, Eq, Clone)]
struct Stamp {
    written: Option<SystemTime>,
    bytes: Vec<u8>,
}

fn stamp(path: &Path) -> Stamp {
    Stamp {
        written: std::fs::metadata(path).and_then(|m| m.modified()).ok(),
        bytes: std::fs::read(path).unwrap_or_default(),
    }
}

/// Whether any of the files is not as it was.
fn changed(files: &[(PathBuf, Stamp)]) -> bool {
    files.iter().any(|(path, was)| stamp(path) != *was)
}

/// Runs the command, and again each time a file it read changes, until `smith
/// --watch` is interrupted.
pub fn watch(kind: Kind, start: &mut Starter, mut command: impl FnMut() -> Outcome) -> ExitCode {
    // SAFETY: the handler only stores a number.
    unsafe {
        signal(SIGINT, interrupted as extern "C" fn(i32) as usize);
        signal(SIGTERM, interrupted as extern "C" fn(i32) as usize);
    }
    let kept = Arc::new(Kept::default());
    let mut run: Option<Run> = None;
    let mut files: Vec<(PathBuf, Stamp)> = Vec::new();
    let mut first = true;
    let code = loop {
        // The files that were known are looked at before the build, so that a
        // save during the build is built again.
        for (path, was) in files.iter_mut() {
            *was = stamp(path);
        }
        let outcome = command();
        for path in &outcome.read {
            if !files.iter().any(|(known, _)| known == path) {
                files.push((path.clone(), stamp(path)));
            }
        }
        if first && files.is_empty() {
            // Nothing was read that a change could fix.
            break ExitCode::FAILURE;
        }
        first = false;
        if INTERRUPTED.load(Ordering::SeqCst) != 0 {
            break interrupted_code();
        }
        if let (true, Kind::Run { diverge, words }, Some(executable)) =
            (outcome.ok, &kind, &outcome.executable)
        {
            if let Some(old) = run.as_mut() {
                if *diverge {
                    old.stop();
                } else {
                    // Let it end.
                    while !old.ended() {
                        if INTERRUPTED.load(Ordering::SeqCst) != 0 {
                            break;
                        }
                        std::thread::sleep(POLL);
                    }
                    old.kill();
                }
            }
            if INTERRUPTED.load(Ordering::SeqCst) != 0 {
                break interrupted_code();
            }
            match Run::start(start, executable, words, &kept) {
                Ok(started) => run = Some(started),
                Err(error) => {
                    eprintln!("{}: {error}", executable.display());
                    run = None;
                }
            }
        }
        // Wait for a file to change and stay as it is.
        let mut quiet_since: Option<Instant> = None;
        let mut seen: Vec<(PathBuf, Stamp)> = Vec::new();
        let interrupted = loop {
            if INTERRUPTED.load(Ordering::SeqCst) != 0 {
                break true;
            }
            if let Some(r) = run.as_mut()
                && r.ended()
            {
                let _ = r.child.wait();
                run = None;
            }
            match quiet_since {
                None => {
                    if changed(&files) {
                        seen = files.iter().map(|(p, _)| (p.clone(), stamp(p))).collect();
                        quiet_since = Some(Instant::now());
                    }
                }
                Some(since) => {
                    let now: Vec<(PathBuf, Stamp)> =
                        files.iter().map(|(p, _)| (p.clone(), stamp(p))).collect();
                    if now != seen {
                        seen = now;
                        quiet_since = Some(Instant::now());
                    } else if since.elapsed() >= QUIET {
                        break false;
                    }
                }
            }
            std::thread::sleep(POLL);
        };
        if interrupted {
            break interrupted_code();
        }
    };
    if let Some(r) = run.as_mut() {
        r.kill();
    }
    code
}

fn interrupted_code() -> ExitCode {
    let number = INTERRUPTED.load(Ordering::SeqCst);
    ExitCode::from(u8::try_from(128 + number).unwrap_or(130))
}
