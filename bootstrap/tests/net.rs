//! A program that carries `net` or `time` is answered by the system's own
//! calls on Linux, with a call that waits blocking, and by a handler a test
//! writes with no call at all.
//!
//! The program the machine tests run is `net_program.smith`, which does what
//! the page's `echo_once` and `ask` do, one thing for each word it is started
//! with. A test that is the other side of a connection is a program of this
//! test's own, which is why a test starts no thread: it starts the program,
//! waits for the line that says its listener or socket is open, and then
//! talks to it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, UdpSocket};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;

use smith::check;
use smith::diag::Error;
use smith::target::Target;
use smith::{lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::ports;
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "a_listener_a_connection_and_a_socket_that_is_not_ended_or_is_ended_twice_or_is_used_after_does_not_compile",
        a_listener_a_connection_and_a_socket_that_is_not_ended_or_is_ended_twice_or_is_used_after_does_not_compile,
    ),
    (
        "a_handler_a_test_writes_answers_net_and_time_with_no_system_call",
        a_handler_a_test_writes_answers_net_and_time_with_no_system_call,
    ),
    (
        "a_target_that_has_no_network_and_no_clock_refuses_a_program_that_carries_them_naming_itself",
        a_target_that_has_no_network_and_no_clock_refuses_a_program_that_carries_them_naming_itself,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "a_server_whose_first_port_is_held_by_another_socket_is_started_again_on_another_and_listens",
        a_server_whose_first_port_is_held_by_another_socket_is_started_again_on_another_and_listens,
    ),
    (
        "echo_once_takes_a_connection_and_sends_back_what_it_was_sent",
        echo_once_takes_a_connection_and_sends_back_what_it_was_sent,
    ),
    (
        "ask_gets_the_bytes_a_listener_sends_it",
        ask_gets_the_bytes_a_listener_sends_it,
    ),
    (
        "what_one_side_writes_the_other_reads_in_order_and_whole_and_a_read_after_the_close_gives_no_bytes",
        what_one_side_writes_the_other_reads_in_order_and_whole_and_a_read_after_the_close_gives_no_bytes,
    ),
    (
        "a_datagram_sent_to_a_bound_socket_is_received_with_the_address_it_came_from",
        a_datagram_sent_to_a_bound_socket_is_received_with_the_address_it_came_from,
    ),
    (
        "a_port_nothing_listens_at_a_port_in_use_and_a_write_after_the_close_are_each_their_case",
        a_port_nothing_listens_at_a_port_in_use_and_a_write_after_the_close_are_each_their_case,
    ),
    (
        "a_failed_call_holds_its_name_and_the_systems_error_number",
        a_failed_call_holds_its_name_and_the_systems_error_number,
    ),
    (
        "now_is_the_milliseconds_since_1970_and_sleep_returns_after_at_least_as_many_as_it_is_given",
        now_is_the_milliseconds_since_1970_and_sleep_returns_after_at_least_as_many_as_it_is_given,
    ),
];

/// The targets that build the program: those that talk to the kernel.
fn kernel() -> Vec<Target> {
    targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux") || t.name().starts_with("macos"))
        .collect()
}

/// The program, built once for `target`.
fn program(target: Target) -> &'static [u8] {
    static X64: OnceLock<Vec<u8>> = OnceLock::new();
    static ARM64: OnceLock<Vec<u8>> = OnceLock::new();
    let cell = if target.name().ends_with("arm64") { &ARM64 } else { &X64 };
    cell.get_or_init(|| {
        let source = include_str!("net_program.smith");
        targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"))
    })
}

/// A program of the test that has been started, which is killed when the test
/// ends, however it ends.
struct Started {
    child: Child,
    lines: BufReader<std::process::ChildStdout>,
}

impl Drop for Started {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Started {
    /// Starts the program for `target` with the words, as the test's own
    /// process. Its standard output is a pipe the test reads a line at a time.
    fn new(target: Target, words: &[&str]) -> Started {
        let dir = crate::scratch::path("net");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, program(target));
        let mut child = Command::new(&path)
            .args(words)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let lines = BufReader::new(child.stdout.take().unwrap());
        std::fs::remove_dir_all(&dir).ok();
        Started { child, lines }
    }

    /// Starts the program for `target` in `mode`, which listens at the port it
    /// is given, on the first of `candidates` it can bind, and waits for the
    /// line that says it listens, as `ports.rs` says.
    fn serving(
        target: Target,
        mode: &str,
        candidates: impl IntoIterator<Item = u16>,
    ) -> (Started, u16) {
        ports::serve(candidates, |port| {
            let mut run = Started::new(target, &[mode, &port.to_string()]);
            match run.line().as_deref() {
                Some("ready") => ports::Start::Up((run, port)),
                Some("in_use") => ports::Start::Taken,
                other => {
                    let _ = run.child.kill();
                    let status = run.child.wait().unwrap();
                    ports::Start::Failed(format!(
                        "the program ended with {status} before it listened, and wrote {other:?} to standard output and {:?} to standard error",
                        ports::said(&mut run.child)
                    ))
                }
            }
        })
    }

    /// The next line the program writes, and none when it has ended.
    fn line(&mut self) -> Option<String> {
        let mut line = String::new();
        match self.lines.read_line(&mut line).unwrap() {
            0 => None,
            _ => Some(line.trim_end().to_string()),
        }
    }
}

/// Notes the program for each target a fingerprint is made for, and says
/// whether this machine runs it.
fn runs(target: Target) -> bool {
    targets::record(target, &[program(target)]);
    !targets::fingerprinting()
}

fn echo_once_takes_a_connection_and_sends_back_what_it_was_sent() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let (mut run, port) = Started::serving(target, "once", ports::all());
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(b"hello over the network").unwrap();
        let mut back = [0u8; 22];
        stream.read_exact(&mut back).unwrap();
        assert_eq!(&back, b"hello over the network", "{}", target.name());
        assert_eq!(run.line().as_deref(), Some("ok"));
    }
}

fn a_server_whose_first_port_is_held_by_another_socket_is_started_again_on_another_and_listens() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let held = TcpListener::bind("127.0.0.1:0").unwrap();
        let taken = held.local_addr().unwrap().port();
        let (mut run, port) =
            Started::serving(target, "once", std::iter::once(taken).chain(ports::all()));
        assert_ne!(port, taken, "{}", target.name());
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(b"hello").unwrap();
        let mut back = [0u8; 5];
        stream.read_exact(&mut back).unwrap();
        assert_eq!(&back, b"hello", "{}", target.name());
        assert_eq!(run.line().as_deref(), Some("ok"));
    }
}

fn ask_gets_the_bytes_a_listener_sends_it() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut run = Started::new(target, &["ask", &port.to_string(), "what is seven"]);
        let (mut stream, _) = listener.accept().unwrap();
        let mut asked = [0u8; 13];
        stream.read_exact(&mut asked).unwrap();
        assert_eq!(&asked, b"what is seven", "{}", target.name());
        stream.write_all(b"seven, of course").unwrap();
        assert_eq!(
            run.line().as_deref(),
            Some("seven, of course"),
            "{}",
            target.name()
        );
    }
}

/// The lengths of the lists of bytes a test sends: the edges of the reads the
/// program makes, and others the run's seed gives, up to a megabyte.
fn lengths(g: &mut Gen) -> Vec<usize> {
    let mut lengths = vec![0, 1, 4095, 4096, 4097, 1 << 20];
    lengths.extend((0..3).map(|_| g.below(1 << 20)));
    lengths
}

fn what_one_side_writes_the_other_reads_in_order_and_whole_and_a_read_after_the_close_gives_no_bytes()
{
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        generate::check_cases(
            97,
            0,
            1,
            lengths,
            |lengths| {
                for &length in lengths {
                    let bytes: Vec<u8> = (0..length).map(|i| (i * 31 + i / 251) as u8).collect();
                    let (mut run, port) = Started::serving(target, "everything", ports::all());
                    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                    // The program reads until the end of what is sent, which
                    // is a read that gives no bytes, so it needs the close.
                    stream.write_all(&bytes).unwrap();
                    stream.shutdown(Shutdown::Write).unwrap();
                    let mut back = Vec::new();
                    stream.read_to_end(&mut back).unwrap();
                    assert!(back == bytes, "{length} bytes, {}", target.name());
                    assert_eq!(run.line(), Some(length.to_string()));
                }
            },
        );
    }
}

fn a_datagram_sent_to_a_bound_socket_is_received_with_the_address_it_came_from() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let from = UdpSocket::bind("127.0.0.1:0").unwrap();
        let (mut run, port) = Started::serving(target, "datagram", ports::all());
        from.send_to(b"a datagram", ("127.0.0.1", port)).unwrap();
        let said = format!("127.0.0.1 {} a datagram", from.local_addr().unwrap().port());
        assert_eq!(run.line(), Some(said), "{}", target.name());
    }
}

fn a_port_nothing_listens_at_a_port_in_use_and_a_write_after_the_close_are_each_their_case() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let nothing = ports::next();
        let mut run = Started::new(target, &["refused", &nothing.to_string()]);
        assert_eq!(run.line().as_deref(), Some("refused"), "{}", target.name());

        let held = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        let mut run = Started::new(target, &["in_use", &port.to_string()]);
        assert_eq!(run.line().as_deref(), Some("in_use"), "{}", target.name());

        // The other side closes with what was sent still unread, so the
        // system tells the program, and a write after it is a connection that
        // was closed.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut run = Started::new(target, &["closed", &port.to_string()]);
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
        assert_eq!(run.line().as_deref(), Some("closed"), "{}", target.name());
    }
}

/// A port of an address the machine does not hold is refused by the system with
/// "the address is not available", which is 99 on Linux and 49 on macOS.
fn a_failed_call_holds_its_name_and_the_systems_error_number() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let number = if target.name().starts_with("macos") { 49 } else { 99 };
        let mut run = Started::new(target, &["elsewhere", "9"]);
        assert_eq!(
            run.line(),
            Some(format!("bind: error {number}")),
            "{}",
            target.name()
        );
    }
}

fn now_is_the_milliseconds_since_1970_and_sleep_returns_after_at_least_as_many_as_it_is_given() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let mut run = Started::new(target, &["clock"]);
        let line = run.line().unwrap();
        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let read: Vec<i64> = line.split(' ').map(|n| n.parse().unwrap()).collect();
        assert!(
            read[0] >= before - 1000 && read[1] <= after + 1000,
            "{line}: {before} to {after}"
        );
        assert!(read[1] - read[0] >= 150, "{line}");
    }
}

// What a test writes of a program is built by the checker alone.

fn errors(source: &str) -> Vec<Error> {
    match check(source.as_bytes()) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

/// A program that opens a resource of `std.net` with `open`, which is `listen_on`,
/// `connect_to`, or `bind_to`, and then runs the lines, which the test says
/// how to end it in.
fn opened(ty: &str, open: &str, end: &str, lines: &str) -> String {
    let mut names = vec![
        "Address", ty, "NetError", "accept", "close", end, open, "net", "send", "write",
    ];
    names.sort();
    names.dedup();
    let names = names.join(", ");
    format!(
        "use std.list (freeze)\nuse std.net ({names})\n\n\
         fn run() -> Result[(), NetError] ! net, alloc {{\n  \
         value = try {open}(\"127.0.0.1\", 0)\n{lines}  .ok(())\n}}\n\n\
         fn main() ! net, alloc {{\n  match run() {{\n    .ok(()) {{ }}\n    .err(e) {{ }}\n  }}\n}}\n"
    )
}

fn a_listener_a_connection_and_a_socket_that_is_not_ended_or_is_ended_twice_or_is_used_after_does_not_compile()
{
    // A line that lends the resource, so that it is used and not ended.
    for (ty, open, end, uses) in [
        (
            "Listener",
            "listen_on",
            "stop",
            "  match accept(value) {\n    .ok(c) { close(c) }\n    .err(e) { }\n  }\n",
        ),
        (
            "Connection",
            "connect_to",
            "close",
            "  match write(value, freeze([])) {\n    .ok(()) { }\n    .err(e) { }\n  }\n",
        ),
        (
            "Socket",
            "bind_to",
            "unbind",
            "  match send(value, Address(host: \"127.0.0.1\", port: 1), freeze([])) {\n    .ok(()) { }\n    .err(e) { }\n  }\n",
        ),
    ] {
        let end = if open == "bind_to" { "unbind" } else { end };
        let ended = format!("  {end}(value)\n");
        let source = |lines: &str| opened(ty, open, end, lines);
        // Ended once, after it was used: it compiles.
        let once = errors(&source(&format!("{uses}{ended}")));
        assert!(once.is_empty(), "{ty} ended once: {once:?}");
        // Not ended: it is dropped.
        let not_ended = errors(&source(uses));
        assert!(
            not_ended.iter().any(|e| e.message.contains(ty)),
            "{ty} not ended: {not_ended:?}"
        );
        // Ended twice.
        let twice = errors(&source(&format!("{ended}{ended}")));
        assert!(!twice.is_empty(), "{ty} ended twice");
        // Used after it is ended.
        let after = errors(&source(&format!("{ended}{uses}")));
        assert!(!after.is_empty(), "{ty} used after it is ended");
    }
}

fn a_handler_a_test_writes_answers_net_and_time_with_no_system_call() {
    let source = HANDLED;
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(!program.asks_the_kernel());
    let outcome = scalar::run(&program);
    assert_eq!(String::from_utf8(outcome.stdout).unwrap(), "refused 11\n");
    // The program under the handler is the program with none, which is
    // refused by a target that has no call to answer it with.
    for &target in Target::ALL.iter() {
        let bytes = targets::build(source.as_bytes(), target);
        assert!(bytes.is_ok(), "{}", target.name());
    }
}

/// A handler answers `net` and `time`, and the code under it is what a
/// program with no handler would have.
const HANDLED: &str = "use std.console (console, print)
use std.list (freeze)
use std.net (Connection, NetError, close, connect, net, read, release_connection, release_listener, release_socket, write)
use std.time (now, time)

fn ask(question: str) -> Result[List[Byte], NetError] ! net, alloc {
  with c = connect(\"example.test\", 7)
  try write(c, question.bytes)
  read(c, 16)
}

fn main() ! console, alloc {
  net(
    listen_on: fn(host, port) { .err(.refused) },
    accept: fn(l) { .err(.refused) },
    stop: fn(l) { n = release_listener(l) },
    connect_to: fn(host, port) { .err(.refused) },
    read: fn(c, most) { .ok([104, 101, 108, 108, 111]) },
    write: fn(c, bytes) { .ok(()) },
    close: fn(c) { n = release_connection(c) },
    bind_to: fn(host, port) { .err(.refused) },
    send: fn(s, to, bytes) { .ok(()) },
    receive: fn(s, most) { .err(.refused) },
    unbind: fn(s) { n = release_socket(s) }
  ) {
    time(now: { 11 }, sleep: fn(ms) { }) {
      match ask(\"x\") {
        .ok(bytes) { print(\"answered\") }
        .err(.refused) { print(\"refused {now()}\") }
        .err(e) { print(\"another\") }
      }
    }
  }
}
";

fn a_target_that_has_no_network_and_no_clock_refuses_a_program_that_carries_them_naming_itself() {
    let program = lower(include_str!("net_program.smith").as_bytes()).unwrap();
    assert!(program.asks_the_kernel());
    for &target in Target::ALL.iter() {
        if target.name().starts_with("linux") || target.name().starts_with("macos") {
            continue;
        }
        let refused = std::panic::catch_unwind(|| target.emit(&program));
        let message = match refused {
            Ok(_) => panic!("{} built a program that asks the kernel", target.name()),
            Err(why) => match why.downcast_ref::<String>() {
                Some(text) => text.clone(),
                None => String::new(),
            },
        };
        assert!(message.contains(target.name()), "{message}");
    }
}
