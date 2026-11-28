//! A group of tasks that no scheduler is in force of waits for its connections
//! in `epoll`, on the one thread of the program: a connection that says nothing
//! holds up no other, a child that opens a group of its own is run by the
//! scheduler in force and not by a loop of its own, and a task that waits for
//! the system gives its place to the others. The programs are real servers that
//! the tests connect to, so the tests are of the kind of machine that runs them.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use smith::target::Target;

use crate::Test;
use crate::ports;
use crate::shared;
use crate::targets;

pub const MACHINE: &[Test] = &[
    (
        "a_connection_that_says_nothing_holds_up_no_other_and_the_program_has_one_thread",
        a_connection_that_says_nothing_holds_up_no_other_and_the_program_has_one_thread,
    ),
    (
        "a_child_that_opens_a_group_waits_in_the_scheduler_in_force_and_holds_up_no_task",
        a_child_that_opens_a_group_waits_in_the_scheduler_in_force_and_holds_up_no_task,
    ),
];

/// A server that accepts two connections, runs a task for each that reads once
/// and writes what it read, and prints the bytes it echoed.
const PAIR: &str = r#"use std.bits (u64)
use std.console (console, print)
use std.deserialize (ReadError)
use std.fromtext (from_text)
use std.int (wrapping_add)
use std.list (freeze)
use std.net (Connection, NetError, accept, close, listen, net, read, write)
use std.task (join, spawn, tasks)

fn turn(c: take Connection) -> u64 <net, alloc> {
  mut size: u64 = 0
  match read(c, 64) {
    .ok(.bytes(chunk)) {
      size = chunk.bytes.size
      _ = write(c, freeze(chunk.bytes))
    }
    .ok(.ended) { }
    .err(e) { }
  }
  close(c)
  size
}

fn pair(port: 1..65536) -> Result[u64, NetError] <console, net, alloc> {
  with tasks()
  with l = listen("127.0.0.1", .at(port))
  print("ready")
  a = try accept(l)
  ta = spawn { turn(a) }
  b = try accept(l)
  tb = spawn { turn(b) }
  .ok(wrapping_add(join(ta), join(tb)))
}

fn main(args: List[str]) <console, net, alloc> {
  if args.size == 0 { return }
  port_word = args[0]
  number: Result[1..65536, ReadError] = from_text("port", port_word)
  match number {
    .ok(p) {
      match pair(p) {
        .ok(n) { print("{n}") }
        .err(e) { print("failed") }
      }
    }
    .err(e) { print("bad port") }
  }
}
"#;

/// A server that accepts three connections. A task of a group of its own holds
/// two of them, and another task of the first group holds the third.
const NESTED: &str = r#"use std.bits (u64)
use std.console (console, print)
use std.deserialize (ReadError)
use std.fromtext (from_text)
use std.int (wrapping_add)
use std.list (freeze)
use std.net (Connection, NetError, accept, close, listen, net, read, write)
use std.task (join, spawn, task, tasks)

fn turn(c: take Connection) -> u64 <net, alloc> {
  mut size: u64 = 0
  match read(c, 64) {
    .ok(.bytes(chunk)) {
      size = chunk.bytes.size
      _ = write(c, freeze(chunk.bytes))
    }
    .ok(.ended) { }
    .err(e) { }
  }
  close(c)
  size
}

fn inner(a: take Connection, b: take Connection) -> u64 <net, alloc, task> {
  with tasks()
  ta = spawn { turn(a) }
  tb = spawn { turn(b) }
  wrapping_add(join(ta), join(tb))
}

fn pair(port: 1..65536) -> Result[u64, NetError] <console, net, alloc> {
  with tasks()
  with l = listen("127.0.0.1", .at(port))
  print("ready")
  a = try accept(l)
  b = match accept(l) {
    .ok(x) { x }
    .err(e) {
      close(a)
      return .err(e)
    }
  }
  c = match accept(l) {
    .ok(x) { x }
    .err(e) {
      close(a)
      close(b)
      return .err(e)
    }
  }
  outer = spawn { inner(a, b) }
  other = spawn { turn(c) }
  .ok(wrapping_add(join(outer), join(other)))
}

fn main(args: List[str]) <console, net, alloc> {
  if args.size == 0 { return }
  port_word = args[0]
  number: Result[1..65536, ReadError] = from_text("port", port_word)
  match number {
    .ok(p) {
      match pair(p) {
        .ok(n) { print("{n}") }
        .err(e) { print("failed") }
      }
    }
    .err(e) { print("bad port") }
  }
}
"#;

/// The targets that run the program: those of Linux, which waits in `epoll`, and
/// those of macOS, which waits in `kqueue`.
fn linux() -> Vec<Target> {
    targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux") || t.name().starts_with("macos"))
        .collect()
}

fn program(source: &'static str, target: Target) -> Vec<u8> {
    static BUILT: shared::Keyed<(usize, &str), Vec<u8>> = shared::Keyed::new();
    BUILT.get((source.as_ptr() as usize, target.name()), || {
        targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"))
    })
}

/// A server that is started and is killed when the test ends.
struct Started {
    child: Child,
    out: BufReader<ChildStdout>,
    port: u16,
}

impl Drop for Started {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Started {
    fn new(source: &'static str, target: Target) -> Started {
        let dir = crate::scratch::path("epoll");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, &program(source, target));
        let started = ports::serve(ports::all(), |port| {
            let mut child = Command::new(&path)
                .arg(port.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut out = BufReader::new(child.stdout.take().unwrap());
            let mut line = String::new();
            out.read_line(&mut line).unwrap();
            if line.trim() == "ready" {
                return ports::Start::Up(Started { child, out, port });
            }
            let said = ports::said(&mut child);
            let _ = child.wait();
            if said.contains("in use") {
                ports::Start::Taken
            } else {
                ports::Start::Failed(format!("the server wrote {line:?} and {said:?}"))
            }
        });
        std::fs::remove_dir_all(&dir).ok();
        started
    }

    fn connect(&self) -> TcpStream {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
        stream
    }

    /// What the server printed after it was ready.
    fn result(&mut self) -> String {
        let mut line = String::new();
        self.out.read_line(&mut line).unwrap();
        line.trim().to_string()
    }

    /// The threads the server has.
    fn threads(&self) -> usize {
        if let Ok(status) = std::fs::read_to_string(format!("/proc/{}/status", self.child.id())) {
            let line = status.lines().find(|l| l.starts_with("Threads:")).expect("a line of threads");
            return line["Threads:".len()..].trim().parse().unwrap();
        }
        // macOS has no `/proc`: `ps -M` lists a thread to a line after its heading.
        let listed = Command::new("ps").args(["-M", "-p", &self.child.id().to_string()]).output().unwrap();
        String::from_utf8_lossy(&listed.stdout).lines().count() - 1
    }
}

/// What the connection gives next, and how long it took.
fn answer(stream: &mut TcpStream, bytes: &[u8]) -> (Vec<u8>, Duration) {
    let from = Instant::now();
    stream.write_all(bytes).unwrap();
    let mut got = vec![0u8; bytes.len()];
    stream.read_exact(&mut got).unwrap();
    (got, from.elapsed())
}

fn a_connection_that_says_nothing_holds_up_no_other_and_the_program_has_one_thread() {
    for target in linux() {
        if !runs(PAIR, target) {
            continue;
        }
        let mut server = Started::new(PAIR, target);
        let mut quiet = server.connect();
        std::thread::sleep(Duration::from_millis(100));
        let mut quick = server.connect();
        // The second is answered while the first has said nothing.
        let (got, took) = answer(&mut quick, b"bee");
        assert_eq!(got, b"bee", "{}", target.name());
        assert!(took < Duration::from_millis(900), "{} took {took:?}", target.name());
        assert_eq!(server.threads(), 1, "{}", target.name());
        std::thread::sleep(Duration::from_millis(1000));
        let (got, _) = answer(&mut quiet, b"ay");
        assert_eq!(got, b"ay", "{}", target.name());
        assert_eq!(server.result(), "5", "{}", target.name());
        let mut rest = Vec::new();
        quiet.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty());
    }
}

fn a_child_that_opens_a_group_waits_in_the_scheduler_in_force_and_holds_up_no_task() {
    for target in linux() {
        if !runs(NESTED, target) {
            continue;
        }
        let mut server = Started::new(NESTED, target);
        let mut a = server.connect();
        let mut b = server.connect();
        let mut c = server.connect();
        // The task of the first group is answered while the two of the inner
        // group wait, which they do in the scheduler of the first group.
        let (got, took) = answer(&mut c, b"cee");
        assert_eq!(got, b"cee", "{}", target.name());
        assert!(took < Duration::from_millis(900), "{} took {took:?}", target.name());
        assert_eq!(server.threads(), 1, "{}", target.name());
        std::thread::sleep(Duration::from_millis(1000));
        assert_eq!(answer(&mut a, b"ay").0, b"ay", "{}", target.name());
        std::thread::sleep(Duration::from_millis(1000));
        assert_eq!(answer(&mut b, b"bee!").0, b"bee!", "{}", target.name());
        assert_eq!(server.result(), "9", "{}", target.name());
    }
}

/// Notes the program for each target a fingerprint is made for, and says
/// whether this machine runs it.
fn runs(source: &'static str, target: Target) -> bool {
    targets::record(target, &[&program(source, target)]);
    !targets::fingerprinting()
}
