//! A task that is cancelled closes the connection it holds, and a peer on a real
//! socket sees the end of the file while the program goes on: for a child that
//! waits for a read when the block of its group ends, for one that has not run
//! yet, and for a thousand of them, one after another, under a limit of 256
//! descriptors, which a connection left open would pass. The program the tests
//! start is `cancel_peer.smith`, and it goes on for 2.5 seconds after the last
//! group ends, so that the end of the file is the group's and not the program's.

use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use smith::target::Target;

use crate::Test;
use crate::shared;
use crate::cli::{folder, smith, stderr};
use crate::ports;
use crate::targets;

pub const MACHINE: &[Test] = &[
    (
        "a_task_cancelled_while_it_waits_for_a_read_closes_its_connection_and_the_peer_sees_the_end_of_the_file",
        a_task_cancelled_while_it_waits_for_a_read_closes_its_connection_and_the_peer_sees_the_end_of_the_file,
    ),
    (
        "a_task_cancelled_before_it_ran_closes_the_connection_it_was_given_and_the_peer_sees_the_end_of_the_file",
        a_task_cancelled_before_it_ran_closes_the_connection_it_was_given_and_the_peer_sees_the_end_of_the_file,
    ),
    (
        "a_thousand_connections_of_tasks_cancelled_before_they_ran_leave_no_descriptor_open_under_a_limit_of_256",
        a_thousand_connections_of_tasks_cancelled_before_they_ran_leave_no_descriptor_open_under_a_limit_of_256,
    ),
    (
        "a_thousand_connections_of_tasks_cancelled_while_they_wait_leave_no_descriptor_open_under_a_limit_of_256",
        a_thousand_connections_of_tasks_cancelled_while_they_wait_leave_no_descriptor_open_under_a_limit_of_256,
    ),
];

const PROGRAM: &str = include_str!("cancel_peer.smith");

/// The targets that talk to the kernel and run tasks that wait for the system:
/// those of Linux and macOS.
fn kernel() -> Vec<Target> {
    targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux") || t.name().starts_with("macos"))
        .collect()
}

/// The program, built once for each kind of chip, by the compiler's executable.
fn program(target: Target) -> &'static [u8] {
    static X64: shared::Once<Vec<u8>> = shared::Once::new();
    static ARM64: shared::Once<Vec<u8>> = shared::Once::new();
    let cell = if target.name().ends_with("arm64") { &ARM64 } else { &X64 };
    cell.get(|| {
        if targets::fingerprinting() {
            targets::build(PROGRAM.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"))
        } else {
            let dir = folder(&format!("cancel_peer_{}", target.name()));
            std::fs::write(dir.join("server.smith"), PROGRAM).unwrap();
            let built = smith(&dir, &["build", "--target", target.name(), "server.smith"]);
            assert!(built.status.success(), "{}", stderr(&built));
            std::fs::read(dir.join(target.executable_name("server"))).unwrap()
        }
    })
}

/// Notes the program for each target a fingerprint is made for, and says whether
/// this machine runs it.
fn runs(target: Target) -> bool {
    targets::record(target, &[program(target)]);
    !targets::fingerprinting()
}

struct Started {
    child: Child,
    port: u16,
    path: PathBuf,
}

impl Drop for Started {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(dir) = self.path.parent() {
            std::fs::remove_dir_all(dir).ok();
        }
    }
}

/// Starts the program on the first port of `ports.rs` it can bind, for `rounds`
/// connections, each of whose group ends after `pause` milliseconds, with at most
/// `descriptors` open when that is given.
fn start(target: Target, pause: u64, rounds: u64, descriptors: Option<u64>) -> Started {
    let dir = crate::scratch::path(&format!("cancel_peer_{pause}_{rounds}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("program");
    crate::executable::write(&path, program(target));
    ports::serve(ports::all(), |port| {
        let mut command = match descriptors {
            Some(n) => {
                let mut c = Command::new("sh");
                c.arg("-c").arg(format!("ulimit -n {n}; exec \"$0\" \"$@\"")).arg(&path);
                c
            }
            None => Command::new(&path),
        };
        command.args([port.to_string(), pause.to_string(), rounds.to_string()]);
        let mut child = command.stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
        // A program that cannot bind the port ends at once, and says so.
        std::thread::sleep(Duration::from_millis(300));
        match child.try_wait().unwrap() {
            None => ports::Start::Up(Started { child, port, path: path.clone() }),
            Some(status) => {
                let said = ports::said(&mut child);
                if said.contains("in use") {
                    ports::Start::Taken
                } else {
                    ports::Start::Failed(format!("the program ended with {status}: {said:?}"))
                }
            }
        }
    })
}

impl Started {
    /// A connection to the program, tried again until it takes one.
    fn connect(&self) -> TcpStream {
        let end = Instant::now() + Duration::from_secs(8);
        loop {
            match TcpStream::connect(("127.0.0.1", self.port)) {
                Ok(stream) => return stream,
                Err(e) => {
                    assert!(Instant::now() < end, "no connection to the program: {e}");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    fn alive(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }
}

/// What the peer reads within `within`: the program never writes, so the end of
/// the file is the only read that is right.
fn end_of_file(stream: &mut TcpStream, within: Duration) -> Result<(), String> {
    stream.set_read_timeout(Some(within)).unwrap();
    loop {
        // A signal that reaches the test's thread interrupts the read, which says
        // nothing of the connection, so the read is made again.
        match stream.read(&mut [0u8; 1]) {
            Ok(0) => return Ok(()),
            Ok(n) => return Err(format!("{n} bytes were read")),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("{:?}: {e}", e.kind())),
        }
    }
}

fn once(pause: u64) {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let mut server = start(target, pause, 1, None);
        let mut peer = server.connect();
        let read = end_of_file(&mut peer, Duration::from_millis(1800));
        assert_eq!(read, Ok(()), "pause {pause}, {}", target.name());
        // The program was still running, so the group closed it and the program's
        // own end did not.
        assert!(server.alive(), "the program ended first, {}", target.name());
    }
}

fn a_task_cancelled_while_it_waits_for_a_read_closes_its_connection_and_the_peer_sees_the_end_of_the_file() {
    once(300);
}

fn a_task_cancelled_before_it_ran_closes_the_connection_it_was_given_and_the_peer_sees_the_end_of_the_file() {
    once(0);
}

fn many(pause: u64) {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let mut server = start(target, pause, 1000, Some(256));
        for round in 0..1000 {
            let mut peer = server.connect();
            let read = end_of_file(&mut peer, Duration::from_secs(5));
            assert_eq!(read, Ok(()), "round {round}, pause {pause}, {}", target.name());
        }
        assert!(server.alive(), "the program ended first, {}", target.name());
    }
}

fn a_thousand_connections_of_tasks_cancelled_before_they_ran_leave_no_descriptor_open_under_a_limit_of_256() {
    many(0);
}

/// A pause of 1 millisecond is long enough for the child to run and wait for its
/// read, since the group's wait lets the scheduler run it.
fn a_thousand_connections_of_tasks_cancelled_while_they_wait_leave_no_descriptor_open_under_a_limit_of_256() {
    many(1);
}
