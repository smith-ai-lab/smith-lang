//! A function may install the handlers of a server one after the other with `with`
//! lines: a handler of `net`, a scheduler, an arena, `listen`, and `routes`. A
//! program that does is built, or refused with a place in its source, and is
//! never a crash of the compiler, and it answers as the program does that
//! installs the same handlers in two functions, one that calls the other.

use smith::lower;
use smith::target::Target;

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets::{self, fingerprinting, record, run as execute};

pub const ONCE: &[Test] = &[
    (
        "a_server_that_installs_listen_and_routes_in_one_function_is_built_whatever_else_is_installed_with_them",
        a_server_that_installs_listen_and_routes_in_one_function_is_built_whatever_else_is_installed_with_them,
    ),
    (
        "a_server_with_no_net_handler_of_its_own_that_installs_listen_and_routes_in_main_is_built",
        a_server_with_no_net_handler_of_its_own_that_installs_listen_and_routes_in_main_is_built,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_1_of_8",
        a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_1,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_2_of_8",
        a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_2,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_3_of_8",
        a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_3,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_4_of_8",
        a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_4,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_5_of_8",
        a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_5,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_6_of_8",
        a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_6,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_7_of_8",
        a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_7,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_8_of_8",
        a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_8,
    ),
];

/// The handlers a function installs, in the order they are written.
#[derive(Debug)]
struct Chain {
    /// How many connections the handler of `net` gives before it says `.closed`.
    connections: u64,
    /// The seed of a scheduler of a test, written after the handler of `net`.
    scheduler: Option<u64>,
    /// Whether `with arena()` is written before `listen`.
    arena_before_listen: bool,
    /// Whether `with arena()` is written between `listen` and `routes`.
    arena_before_routes: bool,
}

fn chain(g: &mut Gen) -> Chain {
    let scheduler = g.one_in(2).then(|| g.below(8) as u64);
    Chain {
        connections: g.below(4) as u64,
        scheduler,
        arena_before_listen: g.one_in(2),
        arena_before_routes: g.one_in(2),
    }
}

/// The bytes of a request for the user with the number, written as a list.
fn request(id: u64) -> String {
    let text = format!("GET /users/{id} HTTP/1.1\r\nhost: x\r\n\r\n");
    let bytes: Vec<String> = text.bytes().map(|b| b.to_string()).collect();
    format!("[{}]", bytes.join(", "))
}

/// The program that installs the chain. When `split` is true the handlers from
/// `routes` on are installed by a function that `run` calls, and when it is false
/// every handler is installed by `run`.
fn program(chain: &Chain, split: bool) -> String {
    let scheduler = chain
        .scheduler
        .map_or((String::new(), String::new()), |s| {
            ("use std.task (test_scheduler)\n".to_string(), format!("  with test_scheduler({s})\n"))
        });
    let arena = |on: bool| if on { "  with arena()\n" } else { "" };
    let routes = format!("{}  with routes()\n  get(\"/users/:id\", user)\n", arena(chain.arena_before_routes));
    let (app, inline) = if split {
        (format!("fn app() <http, alloc> {{\n{routes}}}\n\n"), "  app()\n".to_string())
    } else {
        (String::new(), routes)
    };
    format!(
        "use std.alloc (arena)
use std.bits (u64)
use std.console (console, print)
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

{app}fn recording[T](body: fn() -> T <net>) -> (T, List[List[Byte]]) <alloc> {{
  var accepted: u64 = 0
  var asked: List[u64] = []
  var kept: List[List[Byte]] = []
  result = net(
    listen_on: fn(host, port) {{ .ok(listener(1)) }},
    accept: fn(l) {{
      accepted = wrapping_add(accepted, 1)
      if accepted > {connections} {{
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

fn run() -> ((), List[List[Byte]]) <alloc, diverge> {{
  with recording()
{scheduler}{before_listen}  with listen(port: 80)
{inline}}}

fn main() <console, alloc, diverge> {{
  (done, writes) = run()
  print(\"{{writes.size}}\")
  loop i in 0..writes.size {{
    inner = writes[i]
    var line = \"\"
    loop j in 0..inner.size {{ line = \"{{line}}{{inner[j]}} \" }}
    print(line)
  }}
}}
",
        uses = scheduler.0,
        scheduler = scheduler.1,
        before_listen = arena(chain.arena_before_listen),
        connections = chain.connections,
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

/// The responses that the connections of the chain are given, which the handler of
/// `net` reads a request from with the number of its handle.
fn expected(chain: &Chain) -> Vec<String> {
    let mut found: Vec<String> = (0..chain.connections).map(|k| response(7 + k)).collect();
    found.sort();
    found
}

fn answers_alike(part: u64) {
    generate::check_cases(1207, part, 1, chain, |chain| {
        let expected = expected(chain);
        for target in kernel() {
            let inline = program(chain, false);
            let split = program(chain, true);
            let built = |source: &str| {
                targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?} for {source}"))
            };
            let (a, b) = (built(&inline), built(&split));
            record(target, &[&a, expected.join("\n").as_bytes()]);
            record(target, &[&b, expected.join("\n").as_bytes()]);
            if !fingerprinting() {
                for (bytes, source) in [(&a, &inline), (&b, &split)] {
                    let said = execute(bytes);
                    assert_eq!(said.status, 0, "for {source} on {}", target.name());
                    let found = writes(&String::from_utf8(said.stdout).expect("text"));
                    assert_eq!(found, expected, "for {source} on {}", target.name());
                }
            }
        }
    });
}

/// The handlers are written one after the other in one function, with an arena
/// and a scheduler of a test written or not, and the executable answers with the
/// responses of the routes, as the one does that installs them in two functions.
fn a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_1() {
    answers_alike(0);
}

fn a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_2() {
    answers_alike(1);
}

fn a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_3() {
    answers_alike(2);
}

fn a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_4() {
    answers_alike(3);
}

fn a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_5() {
    answers_alike(4);
}

fn a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_6() {
    answers_alike(5);
}

fn a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_7() {
    answers_alike(6);
}

fn a_server_that_installs_listen_and_routes_in_one_function_answers_as_one_that_splits_them_part_8() {
    answers_alike(7);
}

/// Every chain is built, as the compiler gives an executable or an error and never
/// stops, whichever handlers are written between `listen` and `routes`.
fn a_server_that_installs_listen_and_routes_in_one_function_is_built_whatever_else_is_installed_with_them() {
    generate::check(1208, 2, chain, |chain| {
        let source = program(chain, false);
        if let Err(errors) = lower(source.as_bytes()) {
            panic!("{errors:?} for {source}");
        }
    });
}

/// The server of the page, whose `main` installs `listen` and `routes` itself on
/// the handler of `net` that the system gives, is built.
fn a_server_with_no_net_handler_of_its_own_that_installs_listen_and_routes_in_main_is_built() {
    let source = "use std.http (listen, routes, get, Text)
use std.net (net)

fn hello() -> Text {
  Text(text: \"Hello, world\")
}

fn main() <net, alloc, diverge> {
  with listen(port: 8080)
  with routes()
  get(\"/\", hello)
}
";
    if let Err(errors) = lower(source.as_bytes()) {
        panic!("{errors:?}");
    }
}
