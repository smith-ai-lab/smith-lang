//! What `listen` of `std.http` writes for a response is the status line, the
//! headers of the response, the `content-type` and the `content-length` of its
//! body when it has one and a `content-length` of 0 when it has none, and the
//! body. A handler of `net` that a test writes gives one request for each kind
//! of route and keeps what `write` was given, so that every `Respond` of the
//! library, and the answers the routes give themselves, are compared as whole
//! responses, with no network.

use crate::Test;
use crate::targets::{self, fingerprinting, record, run as execute};

pub const MACHINE: &[Test] = &[
    (
        "every_kind_of_route_is_answered_with_the_whole_response_its_body_and_its_status_say",
        every_kind_of_route_is_answered_with_the_whole_response_its_body_and_its_status_say,
    ),
];

/// The kinds of route, each with the path that asks it and the response it
/// must be answered with, as a status line, the lines of headers before the
/// length, and the body.
struct Route {
    path: &'static str,
    status: &'static str,
    headers: Vec<String>,
    body: String,
}

fn json_text(name: &str) -> String {
    name.replace('\\', "\\\\").replace('"', "\\\"")
}

fn routes_for(name: &str) -> Vec<Route> {
    let json = "content-type: application/json".to_string();
    vec![
        Route {
            path: "/user",
            status: "200 OK",
            headers: vec![json.clone()],
            body: format!("{{\"id\":7,\"name\":\"{}\"}}", json_text(name)),
        },
        Route {
            path: "/page",
            status: "200 OK",
            headers: vec!["content-type: text/html; charset=utf-8".into()],
            body: format!("<p>{}</p>", name.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")),
        },
        Route {
            path: "/text",
            status: "200 OK",
            headers: vec!["content-type: text/plain; charset=utf-8".into()],
            body: name.to_string(),
        },
        Route {
            path: "/go",
            status: "302 Found",
            headers: vec!["location: /login".into()],
            body: String::new(),
        },
        Route { path: "/none", status: "204 No Content", headers: vec![], body: String::new() },
        Route {
            path: "/missing",
            status: "404 Not Found",
            headers: vec![json],
            body: "{\"message\":\"no user\"}".into(),
        },
        Route { path: "/nowhere", status: "404 Not Found", headers: vec![], body: String::new() },
    ]
}

/// The bytes of a request for the path, written as a list.
fn request(path: &str) -> String {
    let text = format!("GET {path} HTTP/1.1\r\nhost: x\r\n\r\n");
    let bytes: Vec<String> = text.bytes().map(|b| b.to_string()).collect();
    format!("[{}]", bytes.join(", "))
}

/// A program that serves one connection for each route, each a request for its
/// path, under `listen`, with a handler that keeps what was written, and prints
/// each write as the number of its connection and the numbers of its bytes.
fn program(name: &str, routes: &[Route]) -> String {
    let mut reads = String::new();
    for (i, r) in routes.iter().enumerate() {
        reads += &format!("        if handle(c) == {} {{\n          .ok(.bytes(Chunk(bytes: {})))\n        }} else ", i + 2, request(r.path));
    }
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.html (Html, p, render, text)
use std.http (NotFound, Redirect, Text, get, http, listen, routes)
use std.int (wrapping_add)
use std.list (push)
use std.net (Chunk, connection, handle, listener, net, release)

struct User {{
  id: u64
  name: str
}}

fn user() -> User {{
  User(id: 7, name: \"{name}\")
}}

fn page() -> Html <alloc> {{
  render() {{ p {{ text(\"{name}\") }} }}
}}

fn plain() -> Text {{
  Text(text: \"{name}\")
}}

fn go() -> Redirect {{
  Redirect(to: \"/login\")
}}

fn none() {{ }}

fn missing() -> Result[User, NotFound] {{
  Result.err(NotFound(message: \"no user\"))
}}

fn app() <http, alloc> {{
  with routes()
  get(\"/user\", user)
  get(\"/page\", page)
  get(\"/text\", plain)
  get(\"/go\", go)
  get(\"/none\", none)
  get(\"/missing\", missing)
}}

fn recording[T](body: fn() -> T <net>) -> (T, List[(u64, List[Byte])]) <alloc> {{
  var accepted: u64 = 0
  var asked: List[u64] = []
  var kept: List[(u64, List[Byte])] = []
  result = net(
    listen_on: fn(host, port) {{ .ok(listener(1)) }},
    accept: fn(l) {{
      accepted = wrapping_add(accepted, 1)
      if accepted > {count} {{
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
{reads}{{
          .ok(.ended)
        }}
      }}
    }},
    write: fn(c, bytes) {{
      var copied: List[Byte] = []
      loop i in 0..bytes.size {{ copied = push(copied, bytes[i]) }}
      kept = push(kept, (handle(c), copied))
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

fn run() -> ((), List[(u64, List[Byte])]) <alloc, diverge> {{
  with recording()
  with listen(port: 80)
  app()
}}

fn main() <console, alloc, diverge> {{
  (done, writes) = run()
  loop i in 0..writes.size {{
    (who, inner) = writes[i]
    var line = \"{{who}}:\"
    loop j in 0..inner.size {{ line = \"{{line}} {{inner[j]}}\" }}
    print(line)
  }}
}}
",
        count = routes.len(),
    )
}

/// The whole response the page says for the route.
fn wire(r: &Route) -> String {
    let mut out = format!("HTTP/1.1 {}\r\n", r.status);
    for h in &r.headers {
        out += &format!("{h}\r\n");
    }
    out += &format!("content-length: {}\r\n\r\n{}", r.body.len(), r.body);
    out
}

/// A name with each mark that JSON, a page, and a text treat in their own way.
const NAME: &str = "Ana <b> & \\\\ caf\u{e9} it's";

/// Every kind of route is answered, on each target that serves, with the whole
/// response the status, the body, and its type say, which `content-length` is
/// the size of the body of, and a `content-type` is written for only a response
/// that has a body.
fn every_kind_of_route_is_answered_with_the_whole_response_its_body_and_its_status_say() {
    let routes = routes_for(NAME);
    let source = program(&NAME.replace('\\', "\\\\"), &routes);
    for target in targets::runnable().into_iter().filter(|t| t.name().starts_with("linux") || t.name().starts_with("macos")) {
        let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
        let expected: Vec<String> = routes.iter().map(wire).collect();
        record(target, &[&bytes, expected.join("\n").as_bytes()]);
        if fingerprinting() {
            continue;
        }
        let said = execute(&bytes);
        assert_eq!(said.status, 0, "on {}", target.name());
        let printed = String::from_utf8(said.stdout).expect("text");
        let mut found: Vec<(u64, String)> = printed
            .lines()
            .map(|l| {
                let (who, rest) = l.split_once(':').expect("a connection");
                let bytes: Vec<u8> = rest.split_whitespace().map(|n| n.parse().expect("a byte")).collect();
                (who.parse().expect("a number"), String::from_utf8(bytes).expect("text"))
            })
            .collect();
        found.sort();
        assert_eq!(found.len(), routes.len(), "{printed}");
        for (i, r) in routes.iter().enumerate() {
            assert_eq!(found[i].0, i as u64 + 2, "{printed}");
            assert_eq!(found[i].1, wire(r), "for the route {} on {}", r.path, target.name());
        }
    }
}
