//! A function may install the handlers of a server one after the other with `with`
//! lines: a handler of `net`, a scheduler, an arena, `listen`, and `routes`. A
//! program that does is built, or refused with a place in its source, and is
//! never a crash of the compiler. Whether it installs them in one function or in
//! two, one that calls the other, it writes to each connection the response its
//! route gives, byte for byte, in the evaluator and in each executable, for a
//! route whose function runs under a handler that keeps state, as `render` of
//! `std.html` does, and for requests of 0 to 20 headers.

use smith::{lower, scalar};
use smith::target::Target;

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets::{self, fingerprinting, record, run as execute};

pub const ONCE: &[Test] = &[
    (
        "the_page_of_the_issue_is_written_whole_to_a_request_of_three_headers_in_the_evaluator",
        the_page_of_the_issue_is_written_whole_to_a_request_of_three_headers_in_the_evaluator,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_is_built_whatever_else_is_installed_with_them_part_1_of_2",
        || a_server_that_installs_listen_and_routes_in_one_function_is_built_whatever_else_is_installed_with_them(0),
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_is_built_whatever_else_is_installed_with_them_part_2_of_2",
        || a_server_that_installs_listen_and_routes_in_one_function_is_built_whatever_else_is_installed_with_them(1),
    ),
    (
        "a_server_with_no_net_handler_of_its_own_that_installs_listen_and_routes_in_main_is_built",
        a_server_with_no_net_handler_of_its_own_that_installs_listen_and_routes_in_main_is_built,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_1_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_1,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_2_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_2,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_3_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_3,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_4_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_4,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_5_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_5,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_6_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_6,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_7_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_7,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_8_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_8,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_9_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_9,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_10_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_10,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_11_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_11,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_12_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_12,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_13_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_13,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_14_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_14,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_15_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_15,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_16_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_16,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_17_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_17,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_18_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_18,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_19_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_19,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_20_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_20,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_21_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_21,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_22_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_22,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_23_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_23,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_24_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_24,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "the_page_of_the_issue_is_written_whole_to_a_request_of_three_headers_by_the_executable",
        the_page_of_the_issue_is_written_whole_to_a_request_of_three_headers_by_the_executable,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_1_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_1,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_2_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_2,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_3_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_3,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_4_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_4,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_5_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_5,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_6_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_6,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_7_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_7,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_8_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_8,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_9_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_9,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_10_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_10,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_11_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_11,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_12_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_12,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_13_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_13,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_14_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_14,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_15_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_15,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_16_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_16,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_17_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_17,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_18_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_18,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_19_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_19,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_20_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_20,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_21_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_21,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_22_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_22,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_23_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_23,
    ),
    (
        "a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_24_of_24",
        a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_24,
    ),
];

/// The handlers a function installs, in the order they are written, the route
/// `routes` is given, and the request each connection sends.
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
    route: Route,
    /// The headers of each request, in order. The first is the host, when there
    /// is one, which a request of HTTP/1.1 is refused without.
    headers: Vec<(String, String)>,
}

/// What the function of the route gives.
#[derive(Debug)]
enum Route {
    /// A user, by the number in the path, which is written as JSON, and which no
    /// handler keeps state for.
    User,
    /// The page that `render` of `std.html` makes, whose handler keeps the text
    /// it writes in its state, of a heading with the word.
    Page(String),
    /// The words a handler of the test keeps in a `mut` as they are said, one
    /// after another, given as text.
    Noted(Vec<String>),
}

/// A word of small letters, of 1 to `most` of them.
fn word(g: &mut Gen, most: usize) -> String {
    let n = 1 + g.below(most);
    (0..n).map(|_| char::from(b'a' + g.below(26) as u8)).collect()
}

/// A text of 0 to `most` bytes that a header may hold.
fn header_value(g: &mut Gen, most: usize) -> String {
    const BYTES: &[u8] = b"abcxyz019*/;=.-_ ";
    let n = g.below(most + 1);
    let mut text: String = (0..n).map(|_| char::from(*g.pick(BYTES))).collect();
    // A space at either end is not part of the value.
    text = text.trim().to_string();
    text
}

fn route(g: &mut Gen) -> Route {
    match g.below(3) {
        0 => Route::User,
        1 => Route::Page(word(g, 30)),
        _ => Route::Noted((0..g.below(5)).map(|_| word(g, 12)).collect()),
    }
}

fn chain(g: &mut Gen) -> Chain {
    let scheduler = g.one_in(2).then(|| g.below(8) as u64);
    let connections = g.below(4) as u64;
    let arena_before_listen = g.one_in(2);
    let arena_before_routes = g.one_in(2);
    let route = route(g);
    // Most requests have few headers, where the memory of the request is small
    // enough that a piece given back too early is the next one taken.
    let count = if g.one_in(4) { g.below(21) } else { 1 + g.below(4) };
    let mut headers = Vec::new();
    if count > 0 {
        headers.push(("host".to_string(), "x".to_string()));
    }
    while headers.len() < count {
        let name = format!("x-{}", word(g, 20));
        let value = header_value(g, 40);
        headers.push((name, value));
    }
    Chain {
        connections,
        scheduler,
        arena_before_listen,
        arena_before_routes,
        route,
        headers,
    }
}

/// The path of the request that the connection with the number sends.
fn path(chain: &Chain, id: u64) -> String {
    match chain.route {
        Route::User => format!("/users/{id}"),
        Route::Page(_) | Route::Noted(_) => "/page".to_string(),
    }
}

/// The request that the connection with the number sends, as a string of the
/// program, whose line breaks `listed` makes the two bytes of HTTP.
fn request(chain: &Chain, id: u64) -> String {
    let mut text = format!("\"GET {} HTTP/1.1\\n", path(chain, id));
    for (name, value) in &chain.headers {
        text += &format!("{name}: {value}\\n");
    }
    text + "\\n\""
}

/// The functions of the route, with what they need of the library, and the call
/// that adds the route.
fn route_source(route: &Route) -> (String, String) {
    match route {
        Route::User => (
            "struct User {
  id: u64
  name: str
}

fn user(id: u64) -> User {
  User(id: id, name: \"Ana\")
}

"
            .to_string(),
            "get(\"/users/:id\", user)".to_string(),
        ),
        Route::Page(word) => (
            format!(
                "use std.html (Html, body, h1, html, render, text)

fn page() -> Html <alloc> {{
  render {{
    html {{
      body {{
        h1 {{ text(\"{word}\") }}
      }}
    }}
  }}
}}

"
            ),
            "get(\"/page\", page)".to_string(),
        ),
        Route::Noted(words) => {
            let said: String = words.iter().map(|w| format!("    say(\"{w}\")\n")).collect();
            (
                format!(
                    "effect note {{
  fn say(t: str)
}}

fn noted(body: fn() <note>) -> str <alloc> {{
  mut said = \"\"
  note(say: fn(t) {{ said = \"{{said}}{{t}}\" }}, body: body)
  said
}}

fn page() -> Text <alloc> {{
  Text(text: noted {{
{said}  }})
}}

"
                ),
                "get(\"/page\", page)".to_string(),
            )
        }
    }
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
    let (functions, add) = route_source(&chain.route);
    let routes = format!("{}  with routes()\n  {add}\n", arena(chain.arena_before_routes));
    let (app, inline) = if split {
        (format!("fn app() <http, alloc> {{\n{routes}}}\n\n"), "  app()\n".to_string())
    } else {
        (String::new(), routes)
    };
    format!(
        "use std.alloc (arena)
use std.bits (u64)
use std.console (console, print)
use std.http ({http})
use std.int (wrapping_add)
use std.list (push)
use std.net (Chunk, connection, handle, listener, net, release)
{uses}
{functions}{app}// The bytes of the text, with each line ended as HTTP ends one.
fn listed(t: str) -> List[Byte] <alloc> {{
  mut made: List[Byte] = []
  loop i in 0..t.size {{
    if t.bytes[i] == 10 {{ made = push(take made, 13) }}
    made = push(take made, t.bytes[i])
  }}
  made
}}

fn recording[T](body: fn() -> T <net>) -> (T, List[List[Byte]]) <alloc> {{
  mut accepted: u64 = 0
  mut asked: List[u64] = []
  mut kept: List[List[Byte]] = []
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
    stop: fn(l) {{ n = release(take l) }},
    connect_to: fn(host, port) {{ .err(.refused(host)) }},
    read: fn(c, most) {{
      mut seen = false
      loop i in 0..asked.size {{ if asked[i] == handle(c) {{ seen = true }} }}
      if seen {{
        .ok(.ended)
      }} else {{
        asked = push(take asked, handle(c))
        made = if handle(c) == 2 {{
          listed({seven})
        }} else if handle(c) == 3 {{
          listed({eight})
        }} else {{
          listed({nine})
        }}
        if made.size == 0 {{ .ok(.ended) }} else {{ .ok(.bytes(Chunk(bytes: made))) }}
      }}
    }},
    write: fn(c, bytes) {{
      mut copied: List[Byte] = []
      loop i in 0..bytes.size {{ copied = push(take copied, bytes[i]) }}
      kept = push(take kept, take copied)
      .ok(())
    }},
    close: fn(c) {{ n = release(take c) }},
    bind_to: fn(host, port) {{ .err(.refused(host)) }},
    send: fn(s, to, bytes) {{ .err(.closed(s.host)) }},
    receive: fn(s, most) {{ .err(.closed(s.host)) }},
    unbind: fn(s) {{ n = release(take s) }},
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
    mut line = \"\"
    loop j in 0..inner.size {{ line = \"{{line}}{{inner[j]}} \" }}
    print(line)
  }}
}}
",
        http = match chain.route {
            Route::Noted(_) => "Text, get, http, listen, routes",
            Route::User | Route::Page(_) => "get, http, listen, routes",
        },
        uses = scheduler.0,
        scheduler = scheduler.1,
        before_listen = arena(chain.arena_before_listen),
        connections = chain.connections,
        seven = request(chain, 7),
        eight = request(chain, 8),
        nine = request(chain, 9),
    )
}

/// The response to a request the route answers with the body, given as the kind
/// of text it is.
fn answered(kind: &str, body: &str) -> String {
    format!("HTTP/1.1 200 OK\r\ncontent-type: {kind}\r\ncontent-length: {}\r\n\r\n{body}", body.len())
}

/// What `listen` writes for the request that the connection with the number
/// sends: the response of the route, or the refusal of a request with no host.
fn response(chain: &Chain, id: u64) -> String {
    if chain.headers.is_empty() {
        return "HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string();
    }
    match &chain.route {
        Route::User => answered("application/json", &format!("{{\"id\":{id},\"name\":\"Ana\"}}")),
        Route::Page(word) => answered(
            "text/html; charset=utf-8",
            &format!("<html><body><h1>{word}</h1></body></html>"),
        ),
        Route::Noted(words) => answered("text/plain; charset=utf-8", &words.concat()),
    }
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
    let mut found: Vec<String> = (0..chain.connections).map(|k| response(chain, 7 + k)).collect();
    found.sort();
    found
}

/// The case of the chains, and whether the chain is written in two functions, of
/// the part of a property: the parts take each case written in one function and
/// then in two.
fn case_of(part: u64) -> (u64, bool) {
    (part / 2, part % 2 == 1)
}

/// For the chain written in one function or in two, the evaluator writes the
/// response of the route to each connection, byte for byte.
fn answers_in_the_evaluator(part: u64) {
    let (case, split) = case_of(part);
    generate::check_cases(1207, case, 1, chain, |chain| {
        let source = program(chain, split);
        let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
        let said = scalar::run(&lowered);
        assert_eq!(said.status, 0, "for {source}");
        let found = writes(&String::from_utf8(said.stdout).expect("text"));
        assert_eq!(found, expected(chain), "for {source}");
    });
}

/// For the same chains as `answers_in_the_evaluator`, so that each executable
/// answers as the evaluator does, the executable for each target whose kernel
/// `listen` is built for writes the response of the route to each connection,
/// byte for byte.
fn answers_alike(part: u64) {
    let (case, split) = case_of(part);
    generate::check_cases(1207, case, 1, chain, |chain| {
        let expected = expected(chain);
        let source = program(chain, split);
        for target in kernel() {
            let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?} for {source}"));
            record(target, &[&bytes, expected.join("\n").as_bytes()]);
            if !fingerprinting() {
                let said = execute(&bytes);
                assert_eq!(said.status, 0, "for {source} on {}", target.name());
                let found = writes(&String::from_utf8(said.stdout).expect("text"));
                assert_eq!(found, expected, "for {source} on {}", target.name());
            }
        }
    });
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_1() {
    answers_in_the_evaluator(0);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_1() {
    answers_alike(0);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_2() {
    answers_in_the_evaluator(1);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_2() {
    answers_alike(1);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_3() {
    answers_in_the_evaluator(2);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_3() {
    answers_alike(2);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_4() {
    answers_in_the_evaluator(3);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_4() {
    answers_alike(3);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_5() {
    answers_in_the_evaluator(4);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_5() {
    answers_alike(4);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_6() {
    answers_in_the_evaluator(5);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_6() {
    answers_alike(5);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_7() {
    answers_in_the_evaluator(6);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_7() {
    answers_alike(6);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_8() {
    answers_in_the_evaluator(7);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_8() {
    answers_alike(7);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_9() {
    answers_in_the_evaluator(8);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_9() {
    answers_alike(8);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_10() {
    answers_in_the_evaluator(9);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_10() {
    answers_alike(9);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_11() {
    answers_in_the_evaluator(10);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_11() {
    answers_alike(10);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_12() {
    answers_in_the_evaluator(11);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_12() {
    answers_alike(11);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_13() {
    answers_in_the_evaluator(12);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_13() {
    answers_alike(12);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_14() {
    answers_in_the_evaluator(13);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_14() {
    answers_alike(13);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_15() {
    answers_in_the_evaluator(14);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_15() {
    answers_alike(14);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_16() {
    answers_in_the_evaluator(15);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_16() {
    answers_alike(15);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_17() {
    answers_in_the_evaluator(16);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_17() {
    answers_alike(16);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_18() {
    answers_in_the_evaluator(17);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_18() {
    answers_alike(17);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_19() {
    answers_in_the_evaluator(18);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_19() {
    answers_alike(18);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_20() {
    answers_in_the_evaluator(19);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_20() {
    answers_alike(19);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_21() {
    answers_in_the_evaluator(20);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_21() {
    answers_alike(20);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_22() {
    answers_in_the_evaluator(21);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_22() {
    answers_alike(21);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_23() {
    answers_in_the_evaluator(22);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_23() {
    answers_alike(22);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_in_the_evaluator_as_its_route_does_part_24() {
    answers_in_the_evaluator(23);
}

fn a_server_that_installs_listen_and_routes_in_one_function_or_two_answers_as_its_route_does_part_24() {
    answers_alike(23);
}

/// The server of #1225, with `listen` and `routes` in one function and a route
/// that renders a page, asked for the page with the headers `curl` sends.
fn the_page_of_the_issue() -> Chain {
    let header = |name: &str, value: &str| (name.to_string(), value.to_string());
    Chain {
        connections: 1,
        scheduler: None,
        arena_before_listen: false,
        arena_before_routes: false,
        route: Route::Page("Hi".to_string()),
        headers: vec![
            header("Host", "localhost:8095"),
            header("User-Agent", "curl/8.5.0"),
            header("Accept", "*/*"),
        ],
    }
}

/// What #1225 says the server writes.
const THE_PAGE: &str =
    "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: 37\r\n\r\n<html><body><h1>Hi</h1></body></html>";

fn the_page_of_the_issue_is_written_whole_to_a_request_of_three_headers_in_the_evaluator() {
    let source = program(&the_page_of_the_issue(), false);
    let said = scalar::run(&lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}")));
    assert_eq!(said.status, 0);
    assert_eq!(writes(&String::from_utf8(said.stdout).expect("text")), [THE_PAGE]);
}

fn the_page_of_the_issue_is_written_whole_to_a_request_of_three_headers_by_the_executable() {
    let source = program(&the_page_of_the_issue(), false);
    for target in kernel() {
        let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        record(target, &[&bytes, THE_PAGE.as_bytes()]);
        if !fingerprinting() {
            let said = execute(&bytes);
            assert_eq!(said.status, 0, "on {}", target.name());
            let found = writes(&String::from_utf8(said.stdout).expect("text"));
            assert_eq!(found, [THE_PAGE], "on {}", target.name());
        }
    }
}

/// Every chain is built, as the compiler gives an executable or an error and never
/// stops, whichever handlers are written between `listen` and `routes`.
fn a_server_that_installs_listen_and_routes_in_one_function_is_built_whatever_else_is_installed_with_them(part: u64) {
    generate::check_cases(1208, part, 1, chain, |chain| {
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

