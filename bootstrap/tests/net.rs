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
        "a_number_that_is_not_a_port_or_not_a_size_to_read_is_refused_where_it_is_written_and_the_message_names_it",
        a_number_that_is_not_a_port_or_not_a_size_to_read_is_refused_where_it_is_written_and_the_message_names_it,
    ),
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
    (
        "each_case_of_a_net_error_is_written_as_its_address_and_what_went_wrong",
        each_case_of_a_net_error_is_written_as_its_address_and_what_went_wrong,
    ),
    (
        "a_read_that_gives_bytes_gives_a_chunk_of_at_least_one_and_a_handler_that_gives_none_is_refused",
        a_read_that_gives_bytes_gives_a_chunk_of_at_least_one_and_a_handler_that_gives_none_is_refused,
    ),
    (
        "a_caller_that_matches_bytes_indexes_the_first_with_no_check",
        a_caller_that_matches_bytes_indexes_the_first_with_no_check,
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
        "what_one_side_writes_the_other_reads_in_order_and_whole_and_a_read_after_the_close_says_the_connection_ended",
        what_one_side_writes_the_other_reads_in_order_and_whole_and_a_read_after_the_close_says_the_connection_ended,
    ),
    (
        "a_listener_and_a_socket_at_any_port_are_given_one_the_system_has_free",
        a_listener_and_a_socket_at_any_port_are_given_one_the_system_has_free,
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
        "a_refused_connection_and_a_write_to_a_closed_one_say_the_host_and_the_port_they_were_opened_with",
        a_refused_connection_and_a_write_to_a_closed_one_say_the_host_and_the_port_they_were_opened_with,
    ),
    (
        "now_is_the_milliseconds_since_1970_and_sleep_returns_after_at_least_as_many_as_it_is_given",
        now_is_the_milliseconds_since_1970_and_sleep_returns_after_at_least_as_many_as_it_is_given,
    ),
];

/// The targets that build the program: those that talk to the kernel, which
/// on Windows is the network through `WS2_32.dll`, and not the clock.
fn kernel() -> Vec<Target> {
    targets::runnable()
        .into_iter()
        .filter(|t| {
            t.name().starts_with("linux")
                || t.name().starts_with("macos")
                || t.name().starts_with("windows")
        })
        .collect()
}

/// The targets that build the clock: those of Linux and macOS.
fn clock() -> Vec<Target> {
    kernel().into_iter().filter(|t| !t.name().starts_with("windows")).collect()
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

/// The program that tells the time, built once for `target`.
fn time_program(target: Target) -> &'static [u8] {
    static X64: OnceLock<Vec<u8>> = OnceLock::new();
    static ARM64: OnceLock<Vec<u8>> = OnceLock::new();
    let cell = if target.name().ends_with("arm64") { &ARM64 } else { &X64 };
    cell.get_or_init(|| {
        let source = include_str!("time_program.smith");
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
        Started::of(program(target), words)
    }

    /// Starts the executable `bytes` with the words.
    fn of(bytes: &[u8], words: &[&str]) -> Started {
        let dir = crate::scratch::path("net");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, bytes);
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

fn what_one_side_writes_the_other_reads_in_order_and_whole_and_a_read_after_the_close_says_the_connection_ended()
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

fn a_listener_and_a_socket_at_any_port_are_given_one_the_system_has_free() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let mut run = Started::new(target, &["any", "1"]);
        assert_eq!(run.line().as_deref(), Some("ok"), "{}", target.name());
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
/// "the address is not available", which is 99 on Linux, 49 on macOS, and 10049
/// on Windows.
fn a_failed_call_holds_its_name_and_the_systems_error_number() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let number = if target.name().starts_with("macos") {
            49
        } else if target.name().starts_with("windows") {
            10049
        } else {
            99
        };
        let mut run = Started::new(target, &["elsewhere", "9"]);
        assert_eq!(
            run.line(),
            Some(format!("bind: error {number}")),
            "{}",
            target.name()
        );
    }
}

/// A connection that is refused names the host and the port it was asked for,
/// and a write to a connection the other side closed names those it was opened
/// with, which only the connection holds.
fn a_refused_connection_and_a_write_to_a_closed_one_say_the_host_and_the_port_they_were_opened_with() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let nothing = ports::next();
        let mut run = Started::new(target, &["refused_says", &nothing.to_string()]);
        assert_eq!(
            run.line(),
            Some(format!("127.0.0.1:{nothing}: connection refused")),
            "{}",
            target.name()
        );

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut run = Started::new(target, &["closed_says", &port.to_string()]);
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
        assert_eq!(
            run.line(),
            Some(format!("127.0.0.1:{port}: connection closed")),
            "{}",
            target.name()
        );
    }
}

fn now_is_the_milliseconds_since_1970_and_sleep_returns_after_at_least_as_many_as_it_is_given() {
    for target in clock() {
        targets::record(target, &[time_program(target)]);
        if targets::fingerprinting() {
            continue;
        }
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let mut run = Started::of(time_program(target), &[]);
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

/// A number written where a connection's port, a listener's port, or the size
/// of a read is asked for, and is not one the type holds: where it is written
/// is where the program is refused, and the message names the number.
fn a_number_that_is_not_a_port_or_not_a_size_to_read_is_refused_where_it_is_written_and_the_message_names_it()
{
    // A number below the range, at its end, or far beyond it.
    let wrong = |g: &mut Gen| match g.below(3) {
        0 => 0,
        1 => 65536,
        _ => 65536 + g.below(1_000_000_000) as u64,
    };
    generate::check_cases(41, 0, 2, wrong, |&wrong| {
        // The three places. A read has no end that a literal could pass, so
        // the size it is asked for is the one that is below.
        for place in 0..3 {
            let (before, number, after) = match place {
                0 => ("fn run() -> Result[(), NetError] <net, alloc> {\n  with c = connect(\"h\", ", wrong, ")\n  .ok(())\n}\n"),
                1 => (
                    "fn run() -> Result[(), NetError] <net, alloc> {\n  with c = connect(\"h\", 1)\n  match read(c, ",
                    0,
                    ") {\n    .ok(g) { .ok(()) }\n    .err(e) { .err(e) }\n  }\n}\n",
                ),
                _ => ("fn run() <net, alloc, diverge> {\n  with listen(port: ", wrong, ")\n}\n"),
            };
            let call = if place == 2 {
                "  run()\n"
            } else {
                "  match run() {\n    .ok(()) { }\n    .err(e) { }\n  }\n"
            };
            let head = if place == 2 {
                "use std.http (listen)\nuse std.net (net)\n\n"
            } else {
                "use std.net (NetError, connect, net, read)\n\n"
            };
            let source = format!("{head}{before}{number}{after}\nfn main() <net, alloc, diverge> {{\n{call}}}\n");
            let at = crate::programs::pos_after(&format!("{head}{before}"));
            let found = lower(source.as_bytes()).err().unwrap_or_default();
            assert_eq!(found.len(), 1, "{found:?} for\n{source}");
            assert_eq!(found[0].pos, at, "{found:?} for\n{source}");
            assert!(found[0].message.contains(&number.to_string()), "{found:?}");
        }
    });
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
    let port = if open == "connect_to" { "1" } else { ".any" };
    format!(
        "use std.list (freeze)\nuse std.net ({names})\n\n\
         fn run() -> Result[(), NetError] <net, alloc> {{\n  \
         value = try {open}(\"127.0.0.1\", {port})\n{lines}  .ok(())\n}}\n\n\
         fn main() <net, alloc> {{\n  match run() {{\n    .ok(()) {{ }}\n    .err(e) {{ }}\n  }}\n}}\n"
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
    assert_eq!(String::from_utf8(outcome.stdout).unwrap(), "refused example.test:7 11\n");
    // The program under the handler is the program with none, which is
    // refused by a target that has no call to answer it with.
    for &target in Target::ALL.iter() {
        let bytes = targets::build(source.as_bytes(), target);
        assert!(bytes.is_ok(), "{}", target.name());
    }
}

/// Each case written by its `Fmt`: the address, and then what went wrong.
fn each_case_of_a_net_error_is_written_as_its_address_and_what_went_wrong() {
    let source = "use std.console (console, print)
use std.net (NetError)

fn main() <console, alloc> {
  errors: List[NetError] = [
    .unknown_host(\"example.com:80\"),
    .refused(\"example.com:80\"),
    .in_use(\"0.0.0.0:80\"),
    .closed(\"example.com:80\"),
    .failed((\"example.com:80\", \"connect: error 110\")),
  ]
  loop i in 0..errors.size { print(\"{errors[i]}\") }
}
";
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let outcome = scalar::run(&program);
    assert_eq!(
        String::from_utf8(outcome.stdout).unwrap(),
        "example.com:80: unknown host\nexample.com:80: connection refused\n0.0.0.0:80: address in use\n\
         example.com:80: connection closed\nexample.com:80: connect: error 110\n"
    );
}

/// A handler answers `net` and `time`, and the code under it is what a
/// program with no handler would have.
const HANDLED: &str = "use std.console (console, print)
use std.instant (instant, since_1970)
use std.list (freeze)
use std.net (Chunk, Connection, Got, NetError, close, connect, net, read, release, write)
use std.time (now, time)

fn ask(question: str) -> Result[Got, NetError] <net, alloc> {
  with c = connect(\"example.test\", 7)
  try write(c, question.bytes)
  read(c, 16)
}

fn main() <console, alloc> {
  net(
    listen_on: fn(host, port) { .err(.refused(host)) },
    accept: fn(l) { .err(.refused(l.host)) },
    stop: fn(l) { n = release(l) },
    connect_to: fn(host, port) { .err(.refused(\"{host}:{port}\")) },
    read: fn(c, most) { .ok(.bytes(Chunk(bytes: [104, 101, 108, 108, 111]))) },
    write: fn(c, bytes) { .ok(()) },
    close: fn(c) { n = release(c) },
    bind_to: fn(host, port) { .err(.refused(host)) },
    send: fn(s, to, bytes) { .ok(()) },
    receive: fn(s, most) { .err(.refused(s.host)) },
    unbind: fn(s) { n = release(s) }
  ) {
    time(now: { instant(11) }, sleep: fn(span) { }) {
      match ask(\"x\") {
        .ok(bytes) { print(\"answered\") }
        .err(.refused(address)) { print(\"refused {address} {since_1970(now())}\") }
        .err(e) { print(\"another\") }
      }
    }
  }
}
";

/// A target that has no network refuses a program that carries `net`, and a
/// target that has no clock, which is Windows too, one that carries `time`, each
/// naming itself.
fn a_target_that_has_no_network_and_no_clock_refuses_a_program_that_carries_them_naming_itself() {
    let program = lower(include_str!("net_program.smith").as_bytes()).unwrap();
    assert!(program.asks_the_network() && !program.asks_the_clock());
    let timed = lower(include_str!("time_program.smith").as_bytes()).unwrap();
    assert!(timed.asks_the_clock() && !timed.asks_the_network());
    for &target in Target::ALL.iter() {
        if target.name().starts_with("windows") {
            let refused = std::panic::catch_unwind(|| target.emit(&timed));
            let message = match refused {
                Ok(_) => panic!("{} built a program that carries time", target.name()),
                Err(why) => why.downcast_ref::<String>().cloned().unwrap_or_default(),
            };
            assert!(message.contains(target.name()), "{message}");
            continue;
        }
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

/// A program that asks a handler of `net` for one read, whose `read` is the
/// body given, and whose caller is the lines given.
fn reading(read: &str, caller: &str) -> String {
    format!(
        "use std.bits (u8)
use std.console (console, print)
use std.list (copy)
use std.net (Chunk, NetError, connect, connection, net, read, release)

fn ask() -> Result[u8, NetError] <net, alloc> {{
  with c = connect(\"example.test\", 7)
{caller}
}}

fn main() <console, alloc> {{
  net(
    listen_on: fn(host, port) {{ .err(.refused(host)) }},
    accept: fn(l) {{ .err(.refused(l.host)) }},
    stop: fn(l) {{ n = release(l) }},
    connect_to: fn(host, port) {{ .ok(connection(2)) }},
    read: fn(c, most) {{ {read} }},
    write: fn(c, bytes) {{ .ok(()) }},
    close: fn(c) {{ n = release(c) }},
    bind_to: fn(host, port) {{ .err(.refused(host)) }},
    send: fn(s, to, bytes) {{ .ok(()) }},
    receive: fn(s, most) {{ .err(.refused(s.host)) }},
    unbind: fn(s) {{ n = release(s) }}
  ) {{
    match ask() {{
      .ok(b) {{ print(\"{{b}}\") }}
      .err(e) {{ print(\"another\") }}
    }}
  }}
}}
"
    )
}

/// A caller that gives what it was given, as every test below needs one.
const GIVES: &str = "  match try read(c, 16) {
    .bytes(chunk) { .ok(0) }
    .ended { .ok(0) }
  }";

/// A handler whose `read` gives `.bytes` of a chunk is built when the bytes are
/// shown to be at least one, and refused, with what could not be proven stated,
/// when they are none.
fn a_read_that_gives_bytes_gives_a_chunk_of_at_least_one_and_a_handler_that_gives_none_is_refused() {
    // The number of bytes a literal list gives.
    generate::check_cases(53, 0, 2, |g: &mut Gen| g.below(40), |&n| {
        let list = format!("[{}]", vec!["7"; n].join(", "));
        let source = reading(&format!(".ok(.bytes(Chunk(bytes: {list})))"), GIVES);
        let found = errors(&source);
        if n >= 1 {
            assert!(found.is_empty(), "{found:?} for\n{source}");
        } else {
            assert_eq!(found.len(), 1, "{found:?} for\n{source}");
            assert!(found[0].message.contains("[].size >= 1"), "{found:?}");
        }
    });
    // A list that is known only when the program runs is built after the test.
    let tested = "
      got = copy(\"{most}\".bytes)
      if got.size >= 1 { .ok(.bytes(Chunk(bytes: got))) } else { .ok(.ended) }
    ";
    let found = errors(&reading(tested, GIVES));
    assert!(found.is_empty(), "{found:?}");
    let untested = "
      got = copy(\"{most}\".bytes)
      .ok(.bytes(Chunk(bytes: got)))
    ";
    let found = errors(&reading(untested, GIVES));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].message.contains("size >= 1"), "{found:?}");
}

/// A caller that matches `.bytes(c)` has `c.bytes.size >= 1` as a fact, so
/// `c.bytes[0]` is built with no check, and gives the first byte read.
fn a_caller_that_matches_bytes_indexes_the_first_with_no_check() {
    let unchecked = "  match try read(c, 16) {
    .bytes(chunk) { .ok(chunk.bytes[0]) }
    .ended { .ok(0) }
  }";
    let program = lower(reading(".ok(.bytes(Chunk(bytes: [9, 8])))", unchecked).as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let outcome = scalar::run(&program);
    assert_eq!(String::from_utf8(outcome.stdout).unwrap(), "9\n");
}
