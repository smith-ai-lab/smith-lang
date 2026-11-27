//! `listen` of `std.http` serves until its listener is closed: when `accept` gives
//! `.closed` it stops accepting, waits for the connections it already took to be
//! served, and returns. A handler of `net` that a test writes gives some
//! connections, reads each a request, keeps what `write` was given, and then says
//! `.closed`, so that `listen` is tested with no socket opened and the test reads
//! what was written after `listen` has returned. A server whose `accept` never
//! gives `.closed` serves for ever, as it did.

use std::sync::{Arc, Mutex};

use smith::lower;
use smith::target::Target;

use crate::Test;
use crate::targets::{self, fingerprinting, record, run_with_words};

pub const ONCE: &[Test] = &[(
    "a_program_that_calls_quiet_is_refused",
    a_program_that_calls_quiet_is_refused,
)];

pub const MACHINE: &[Test] = &[
    (
        "listen_returns_when_it_is_closed_under_the_scheduler_of_the_system_after_no_connection_one_two_and_three",
        listen_returns_when_it_is_closed_under_the_scheduler_of_the_system_after_no_connection_one_two_and_three,
    ),
    (
        "listen_returns_when_it_is_closed_at_once_after_no_connection",
        listen_returns_when_it_is_closed_at_once_after_no_connection,
    ),
    (
        "listen_returns_when_it_is_closed_after_one_connection",
        listen_returns_when_it_is_closed_after_one_connection,
    ),
    (
        "listen_returns_when_it_is_closed_after_two_connections_part_1_of_3",
        listen_returns_when_it_is_closed_after_two_connections_part_1_of_3,
    ),
    (
        "listen_returns_when_it_is_closed_after_two_connections_part_2_of_3",
        listen_returns_when_it_is_closed_after_two_connections_part_2_of_3,
    ),
    (
        "listen_returns_when_it_is_closed_after_two_connections_part_3_of_3",
        listen_returns_when_it_is_closed_after_two_connections_part_3_of_3,
    ),
    (
        "listen_returns_when_it_is_closed_after_three_connections_part_1_of_3",
        listen_returns_when_it_is_closed_after_three_connections_part_1_of_3,
    ),
    (
        "listen_returns_when_it_is_closed_after_three_connections_part_2_of_3",
        listen_returns_when_it_is_closed_after_three_connections_part_2_of_3,
    ),
    (
        "listen_returns_when_it_is_closed_after_three_connections_part_3_of_3",
        listen_returns_when_it_is_closed_after_three_connections_part_3_of_3,
    ),
];

/// The bytes of a request for the user with the number, written as a list.
fn request(id: u64) -> String {
    let text = format!("GET /users/{id} HTTP/1.1\r\nhost: x\r\n\r\n");
    let bytes: Vec<String> = text.bytes().map(|b| b.to_string()).collect();
    format!("[{}]", bytes.join(", "))
}

/// A program that serves as many connections as its first word says, each a
/// request of the number of its handle, under `listen`, with the handler that
/// keeps what was written, and prints what each write was, a line of numbers each.
/// When `tested` it serves them under the scheduler of a test, whose seed is its
/// second word, and when not under the system's. The words are read when the
/// program runs, so that one executable is every case of the kind.
fn program(tested: bool) -> String {
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.deserialize (ReadError)
use std.fromtext (ReadText, from_text)
use std.http (get, http, listen, routes)
use std.int (wrapping_add)
use std.list (push)
use std.net (Chunk, connection, handle, listener, net, release)
{uses}
struct User {{
  id: u64
  name: str
}}

fn user(id: u64) -> User {{
  User(id: id, name: \"Ana\")
}}

fn app() <http, alloc> {{
  with routes()
  get(\"/users/:id\", user)
}}

fn recording[T](connections: u64, body: fn() -> T <net>) -> (T, List[List[Byte]]) <alloc> {{
  var accepted: u64 = 0
  var asked: List[u64] = []
  var kept: List[List[Byte]] = []
  result = net(
    listen_on: fn(host, port) {{ .ok(listener(1)) }},
    accept: fn(l) {{
      accepted = wrapping_add(accepted, 1)
      if accepted > connections {{
        .err(.closed(l.host))
      }} else {{
        .ok(connection(wrapping_add(accepted, 1)))
      }}
    }},
    stop: fn(l) {{ n = release(l) }},
    connect_to: fn(host, port) {{ .err(.refused(host)) }},
    read: fn(c, most) {{
      var seen = false
      loop i in 0..asked.size {{ if asked[i] == handle(c) {{ seen = true }} }}
      if seen {{
        .ok(.ended)
      }} else {{
        asked = push(asked, handle(c))
        if handle(c) == 2 {{
          .ok(.bytes(Chunk(bytes: {seven})))
        }} else if handle(c) == 3 {{
          .ok(.bytes(Chunk(bytes: {eight})))
        }} else {{
          .ok(.bytes(Chunk(bytes: {nine})))
        }}
      }}
    }},
    write: fn(c, bytes) {{
      var copied: List[Byte] = []
      loop i in 0..bytes.size {{ copied = push(copied, bytes[i]) }}
      kept = push(kept, copied)
      .ok(())
    }},
    close: fn(c) {{ n = release(c) }},
    bind_to: fn(host, port) {{ .err(.refused(host)) }},
    send: fn(s, to, bytes) {{ .err(.closed(s.host)) }},
    receive: fn(s, most) {{ .err(.closed(s.host)) }},
    unbind: fn(s) {{ n = release(s) }},
    body: body,
  )
  (result, kept)
}}

fn run(connections: u64, seed: u64) -> ((), List[List[Byte]]) <alloc, diverge> {{
  with recording(connections)
{scheduler}  with listen(port: 80)
  app()
}}

fn number(text: str) -> u64 <alloc> {{
  got: Result[u64, ReadError] = from_text(\"n\", text)
  match got {{
    .ok(v) {{ v }}
    .err(e) {{ 0 }}
  }}
}}

fn show(writes: List[List[Byte]]) <console, alloc> {{
  print(\"{{writes.size}}\")
  loop i in 0..writes.size {{
    inner = writes[i]
    var line = \"\"
    loop j in 0..inner.size {{ line = \"{{line}}{{inner[j]}} \" }}
    print(line)
  }}
}}

fn main(args: List[str]) <console, alloc, diverge> {{
  if args.size == 2 {{
    (done, writes) = run(number(args[0]), number(args[1]))
    show(writes)
  }}
}}
",
        uses = if tested {
            "use std.task (test_scheduler)\n"
        } else {
            ""
        },
        scheduler = if tested {
            "  with test_scheduler(seed)\n"
        } else {
            ""
        },
        seven = request(7),
        eight = request(8),
        nine = request(9),
    )
}

/// What `listen` writes for a request for the user with the number.
fn response(id: u64) -> String {
    let body = format!("{{\"id\":{id},\"name\":\"Ana\"}}");
    format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len())
}

/// The writes a program printed, as text, in the order of their text.
fn writes(printed: &str) -> Vec<String> {
    let mut lines = printed.lines();
    let count: usize = lines.next().expect("a count").parse().expect("a number");
    let mut found: Vec<String> = lines
        .map(|l| String::from_utf8(l.split_whitespace().map(|n| n.parse::<u8>().expect("a byte")).collect()).expect("text"))
        .collect();
    assert_eq!(found.len(), count, "{printed}");
    found.sort();
    found
}

/// The targets whose kernel `listen` is built for.
fn kernel() -> Vec<Target> {
    targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux") || t.name().starts_with("macos"))
        .collect()
}

/// The executable of `program` for the target. It is built by the first test that
/// asks for it and kept for the others, since every case of a kind is the same
/// executable started with other words, and the program of `std.http` is most of
/// what it costs to build. A fingerprint is made of the executables the compiler it
/// is given builds, so it builds each time.
fn executable(target: Target, tested: bool) -> Arc<Vec<u8>> {
    /// The target, whether it is the program under the scheduler of a test, and its executable.
    type Built = (String, bool, Arc<Vec<u8>>);
    static BUILT: Mutex<Vec<Built>> = Mutex::new(Vec::new());
    let build = || {
        Arc::new(targets::build(program(tested).as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}")))
    };
    if fingerprinting() {
        return build();
    }
    let mut built = BUILT.lock().unwrap_or_else(|e| e.into_inner());
    match built.iter().find(|(name, kind, _)| name == target.name() && *kind == tested) {
        Some((_, _, bytes)) => bytes.clone(),
        None => {
            let bytes = build();
            built.push((target.name().to_string(), tested, bytes.clone()));
            bytes
        }
    }
}

/// `listen` returns where the listener says it is closed, whatever order the
/// scheduler of a test of each seed runs the connections in, after the
/// connections it took have been served: when it was closed at once, after one,
/// and after several, each of which was a task that had not run when the
/// closing came, since a handler of `net` does not wait. Under a group of its own
/// it does the same. Each executable does what is said, which is that the
/// responses are the ones the routes give, in the order of their text.
fn listens(connections: u64, schedulers: &[Option<u64>]) {
    let mut expected: Vec<String> = (0..connections).map(|k| response(7 + k)).collect();
    expected.sort();
    for target in kernel() {
        for &seed in schedulers {
            let bytes = executable(target, seed.is_some());
            let words = [connections.to_string(), seed.unwrap_or(0).to_string()];
            record(target, &[&bytes, words.concat().as_bytes(), expected.join("\n").as_bytes()]);
            if !fingerprinting() {
                let said = run_with_words(&bytes, &words);
                assert_eq!(said.status, 0, "for {connections} connections and {seed:?} on {}", target.name());
                let found = writes(&String::from_utf8(said.stdout).expect("text"));
                assert_eq!(found, expected, "for {connections} connections and {seed:?} on {}", target.name());
            }
        }
    }
}

/// The same under the scheduler of the system, which is a program of its own, since
/// a program has one scheduler.
fn listen_returns_when_it_is_closed_under_the_scheduler_of_the_system_after_no_connection_one_two_and_three() {
    for connections in 0..4 {
        listens(connections, &[None]);
    }
}

fn listen_returns_when_it_is_closed_at_once_after_no_connection() {
    listens(0, &[Some(0)]);
}

fn listen_returns_when_it_is_closed_after_one_connection() {
    listens(1, &[Some(0), Some(4)]);
}

fn listen_returns_when_it_is_closed_after_two_connections_part_1_of_3() {
    listens(2, &[Some(0), Some(1)]);
}

fn listen_returns_when_it_is_closed_after_two_connections_part_2_of_3() {
    listens(2, &[Some(2), Some(3), Some(5)]);
}

fn listen_returns_when_it_is_closed_after_two_connections_part_3_of_3() {
    listens(2, &[Some(6), Some(7)]);
}

fn listen_returns_when_it_is_closed_after_three_connections_part_1_of_3() {
    listens(3, &[Some(0), Some(1)]);
}

fn listen_returns_when_it_is_closed_after_three_connections_part_2_of_3() {
    listens(3, &[Some(2), Some(3), Some(4)]);
}

fn listen_returns_when_it_is_closed_after_three_connections_part_3_of_3() {
    listens(3, &[Some(5), Some(7)]);
}

/// `quiet` waits for tasks that no handle names, which a program is not given: it
/// joins the handle of each task it waits for, or drops it.
fn a_program_that_calls_quiet_is_refused() {
    let source = "use std.task (quiet, spawn, task, tasks)\n\nfn main() <alloc, task> {\n  quiet()\n}\n";
    match lower(source.as_bytes()) {
        Ok(_) => panic!("a program that calls `quiet` was built"),
        Err(errors) => {
            let errors = format!("{errors:?}");
            assert!(errors.contains("`quiet` is for the library"), "{errors}");
        }
    }
}
