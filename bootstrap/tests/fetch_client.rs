//! `std.fetch` asks another server over `net`, as a handler of `net` that a test
//! writes plays the server: it gives the bytes of a response in the pieces the
//! test chose, and keeps what the client wrote and how many times the connection
//! was closed. With no network, the response a body of any length and framing
//! makes is read whole up to the limit, and past it is `.too_big` with no more
//! than the limit held; every failure is a value; and the request on the wire
//! says the type and the length of its payload.

use smith::scalar;
use smith::lower;

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_body_past_the_limit_is_refused_as_soon_as_the_bytes_or_the_head_say_so_and_no_more_is_read",
        a_body_past_the_limit_is_refused_as_soon_as_the_bytes_or_the_head_say_so_and_no_more_is_read,
    ),
    (
        "a_response_of_any_framing_in_any_pieces_is_read_whole_up_to_the_limit_and_too_big_past_it",
        a_response_of_any_framing_in_any_pieces_is_read_whole_up_to_the_limit_and_too_big_past_it,
    ),
    (
        "a_request_says_the_type_and_the_length_of_its_payload_and_the_headers_it_was_given_part_1_of_3",
        || a_request_says_the_type_and_the_length_of_its_payload_and_the_headers_it_was_given(0),
    ),
    (
        "a_request_says_the_type_and_the_length_of_its_payload_and_the_headers_it_was_given_part_2_of_3",
        || a_request_says_the_type_and_the_length_of_its_payload_and_the_headers_it_was_given(1),
    ),
    (
        "a_request_says_the_type_and_the_length_of_its_payload_and_the_headers_it_was_given_part_3_of_3",
        || a_request_says_the_type_and_the_length_of_its_payload_and_the_headers_it_was_given(2),
    ),
    (
        "a_handler_of_a_test_answers_each_method_with_no_network_and_a_body_past_the_limit_is_too_big",
        a_handler_of_a_test_answers_each_method_with_no_network_and_a_body_past_the_limit_is_too_big,
    ),
    (
        "a_request_that_is_wrong_is_refused_where_it_is_written_with_a_message_that_names_the_method_part_1_of_3",
        || a_request_that_is_wrong_is_refused_where_it_is_written_with_a_message_that_names_the_method(0),
    ),
    (
        "a_request_that_is_wrong_is_refused_where_it_is_written_with_a_message_that_names_the_method_part_2_of_3",
        || a_request_that_is_wrong_is_refused_where_it_is_written_with_a_message_that_names_the_method(1),
    ),
    (
        "a_request_that_is_wrong_is_refused_where_it_is_written_with_a_message_that_names_the_method_part_3_of_3",
        || a_request_that_is_wrong_is_refused_where_it_is_written_with_a_message_that_names_the_method(2),
    ),
    (
        "an_address_that_is_not_one_a_server_not_found_and_https_are_values_and_nothing_is_written",
        an_address_that_is_not_one_a_server_not_found_and_https_are_values_and_nothing_is_written,
    ),
];

/// The program that asks, under a handler of `net` that plays the server. Each
/// connection the program makes is played the next of `connections`, each the
/// pieces of what the server says, each piece a list of bytes, and the connection
/// says how many bytes were asked of it when it is closed. `connect` is the body
/// of `connect_to` that answers when it is not `.ok`, and `call` the calls of
/// `std.fetch` the program makes, each printing what it gets.
fn program(connections: &[Vec<Vec<u8>>], connect: &str, call: &str) -> String {
    let each: Vec<String> = connections
        .iter()
        .map(|script| {
            let pieces: Vec<String> = script
                .iter()
                .map(|p| format!("[{}]", p.iter().map(u8::to_string).collect::<Vec<_>>().join(", ")))
                .collect();
            format!("[{}]", pieces.join(", "))
        })
        .collect();
    let listed = each.join(", ");
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.fetch (client, get, head, post, put, patch, delete)
use std.int (saturating_add)
use std.list (copy, freeze, push)
use std.message (bytes, form, header_name, header_value, text)
use std.net (Chunk, connection, net, release)
use std.utf8 (utf8)

fn shown(b: take List[Byte]) -> str <alloc> {{
  match utf8(freeze(b)) {{
    .ok(t) {{ t }}
    .err(e) {{ \"not text\" }}
  }}
}}

fn playing[T](body: fn() -> T <net>) -> T <console, alloc> {{
  var connections: List[List[List[Byte]]] = [{listed}]
  var script: List[List[Byte]] = []
  var left: List[Byte] = []
  var written: List[Byte] = []
  var asked: u64 = 0
  result = net(
    listen_on: fn(host, port) {{ .err(.refused(\"{{host}}:{{port}}\")) }},
    accept: fn(l) {{ .err(.refused(\"{{l.host}}:{{l.port}}\")) }},
    stop: fn(l) {{ n = release(l) }},
    connect_to: fn(host, port) {{
      if connections.size > 0 {{
        script = connections[0]
        var tail: List[List[List[Byte]]] = []
        loop i in 1..connections.size {{ tail = push(tail, connections[i]) }}
        connections = tail
        left = []
        asked = 0
        .ok(connection(2))
      }} else {{
        {connect}
      }}
    }},
    read: fn(c, most) {{
      if left.size == 0 {{
        if script.size > 0 {{
          left = copy(script[0])
          var tail: List[List[Byte]] = []
          loop i in 1..script.size {{ tail = push(tail, script[i]) }}
          script = tail
        }}
      }}
      if left.size == 0 {{
        .ok(.ended)
      }} else {{
        piece = if most < left.size {{ most }} else {{ left.size }}
        var got: List[Byte] = []
        var rest: List[Byte] = []
        loop i in 0..left.size {{
          if i < piece {{ got = push(got, left[i]) }} else {{ rest = push(rest, left[i]) }}
        }}
        left = rest
        asked = saturating_add(asked, got.size)
        if got.size >= 1 {{ .ok(.bytes(Chunk(bytes: got))) }} else {{ .ok(.ended) }}
      }}
    }},
    write: fn(c, data) {{
      var made: List[Byte] = []
      loop i in 0..data.size {{ made = push(made, data[i]) }}
      written = made
      print(\"request\")
      print(shown(copy(written)))
      .ok(())
    }},
    close: fn(c) {{
      print(\"closed {{asked}}\")
      n = release(c)
    }},
    bind_to: fn(host, port) {{ .err(.refused(\"{{host}}:{{port}}\")) }},
    send: fn(s, to, data) {{ .err(.closed(\"{{to.host}}:{{to.port}}\")) }},
    receive: fn(s, most) {{ .err(.closed(\"{{s.host}}:{{s.port}}\")) }},
    unbind: fn(s) {{ n = release(s) }},
    body: body,
  )
  print(\"wrote\")
  print(shown(written))
  result
}}

fn main() <console, alloc> {{
  with playing()
  with client()
{call}}}
"
    )
}

/// The status line and the lines of the headers of a response.
fn head(status: u16, reason: &str, headers: &[(String, String)], framing: &str) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status} {reason}\r\n");
    for (n, v) in headers {
        out += &format!("{n}: {v}\r\n");
    }
    out += framing;
    out += "\r\n";
    out.into_bytes()
}

/// How a response says where its body ends.
#[derive(Clone, Copy, Debug)]
enum Framing {
    Length,
    Chunked,
    Close,
}

/// A response the server plays, and what the client must make of it.
#[derive(Debug)]
struct Played {
    framing: Framing,
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    /// The places the bytes of the message are cut, in pieces.
    cuts: Vec<usize>,
    /// Whether the head and the first bytes of the body come in one piece.
    together: bool,
    limit: usize,
}

fn played(g: &mut Gen) -> Played {
    let framing = *g.pick(&[Framing::Length, Framing::Chunked, Framing::Close]);
    let status = *g.pick(&[200u16, 201, 301, 404, 500]);
    let names = ["x-id", "server", "location", "x-a-b"];
    let headers = (0..g.below(3))
        .map(|i| (names[(i + g.below(2)) % names.len()].to_string(), format!("v{}", g.below(100))))
        .collect();
    let size = *g.pick(&[0usize, 1, 2, 17, 100, 301]);
    let body: Vec<u8> = (0..size).map(|i| b'a' + ((i + g.below(26)) % 26) as u8).collect();
    let limit = match g.below(4) {
        0 => body.len().saturating_sub(1 + g.below(3)),
        1 => body.len(),
        2 => body.len() + 1,
        _ => body.len() + g.below(50),
    };
    let cuts = (0..g.below(5)).map(|_| g.below(400)).collect();
    Played { framing, status, headers, body, cuts, together: g.one_in(3), limit }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        301 => "Moved Permanently",
        404 => "Not Found",
        _ => "Internal Server Error",
    }
}

/// The pieces of what the server says: the head, then the body as the framing
/// writes it, cut where the generator chose.
fn pieces(p: &Played) -> Vec<Vec<u8>> {
    let (framing, body): (String, Vec<u8>) = match p.framing {
        Framing::Length => (format!("content-length: {}\r\n", p.body.len()), p.body.clone()),
        Framing::Chunked => {
            let mut wire = Vec::new();
            for chunk in p.body.chunks(7) {
                wire.extend(format!("{:x}\r\n", chunk.len()).into_bytes());
                wire.extend(chunk);
                wire.extend(b"\r\n");
            }
            wire.extend(b"0\r\n\r\n");
            ("transfer-encoding: chunked\r\n".to_string(), wire)
        }
        Framing::Close => (String::new(), p.body.clone()),
    };
    let mut head = head(p.status, reason(p.status), &p.headers, &framing);
    let mut rest = body;
    if p.together && !rest.is_empty() {
        let n = rest.len().min(3);
        head.extend(rest.drain(..n));
    }
    let mut out = vec![head];
    let mut at = 0;
    let mut cuts: Vec<usize> = p.cuts.iter().map(|c| c % (rest.len() + 1)).collect();
    cuts.sort();
    for cut in cuts {
        if cut > at && cut < rest.len() {
            out.push(rest[at..cut].to_vec());
            at = cut;
        }
    }
    if at < rest.len() {
        out.push(rest[at..].to_vec());
    }
    out
}

fn printed(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    String::from_utf8(scalar::run(&program).stdout).expect("text")
}

/// The calls of the program for the cases: each asks the address numbered like
/// it, with the limit it has, and prints one line of what it got.
fn calls(batch: &[Played]) -> String {
    let mut out = String::new();
    for (i, p) in batch.iter().enumerate() {
        out += &format!(
            "  match get[{limit}](\"http://example.com/x{i}\") {{\n    .ok(r) {{\n      var line = \"\"\n      loop i in 0..r.headers.size {{\n        (n, v) = r.headers[i]\n        line = \"{{line}}{{n}}={{v}},\"\n      }}\n      print(\"{{r.status}} {{shown(copy(r.body.bytes))}} {{line}}\")\n    }}\n    .err(e) {{ print(\"{{e}}\") }}\n  }}\n",
            limit = p.limit
        );
    }
    out
}

/// A server that answers a request with a body of any length, framed by its
/// length, in chunks, or by the end of the connection, in pieces cut anywhere,
/// is read whole when the body is no longer than the limit, with each header it
/// said, and is `.too_big` when it is longer, with the connection closed once
/// for each request, and no more asked of it than the head, one byte past the
/// limit, and what came with the head.
fn a_response_of_any_framing_in_any_pieces_is_read_whole_up_to_the_limit_and_too_big_past_it() {
    generate::check(
        1123,
        1,
        |g| (0..8).map(|_| played(g)).collect::<Vec<Played>>(),
        |batch| {
            let connections: Vec<Vec<Vec<u8>>> = batch.iter().map(pieces).collect();
            let source = program(&connections, ".err(.refused(\"{host}:{port}\"))", &calls(batch));
            let said = printed(&source);
            let lines: Vec<&str> = said.lines().collect();
            // Each request prints what it wrote, its closing, and its result.
            let mut at = 0;
            for (i, p) in batch.iter().enumerate() {
                let url = format!("http://example.com/x{i}");
                assert_eq!(lines[at], "request", "{said} for\n{source}");
                while !lines[at].starts_with("closed ") {
                    at += 1;
                }
                let asked: usize = lines[at].strip_prefix("closed ").unwrap().parse().unwrap();
                let first = lines[at + 1];
                at += 2;
                let head_len = connections[i][0].len();
                if p.body.len() <= p.limit {
                    let body = String::from_utf8_lossy(&p.body).to_string();
                    assert!(first.starts_with(&format!("{} {body} ", p.status)), "{first:?} for\n{source}");
                    for (n, v) in &p.headers {
                        assert!(first.contains(&format!("{n}={v},")), "{first:?} lacks {n} for\n{source}");
                    }
                } else {
                    assert_eq!(first, format!("{url}: body longer than {} bytes", p.limit), "for\n{source}");
                    // No more of the body than the limit and one byte, whatever
                    // the framing adds, was asked of the server, and a body that
                    // the head says is too long was not asked for at all.
                    let framing_adds = match p.framing {
                        Framing::Chunked => 16 * (p.limit / 7 + 3),
                        _ => 0,
                    };
                    assert!(asked <= head_len + 3 + p.limit + 1 + framing_adds, "asked {asked} of {i}: {p:?}");
                    if matches!(p.framing, Framing::Length) {
                        assert!(asked <= head_len + 3, "asked {asked} for a body that is too long: {p:?}");
                    }
                }
            }
        },
    );
}

/// A request writes its line, `host`, `connection: close`, the headers the
/// program gave, and for a request with a body the type and the length of its
/// payload and then the payload, and nothing else. Each part is one of three
/// requests, a `post` with headers, a `get`, and a `head` with an address of the
/// form `[::1]:9`, each in a program of its own.
fn a_request_says_the_type_and_the_length_of_its_payload_and_the_headers_it_was_given(part: usize) {
    let reply = head(200, "OK", &[], "content-length: 0\r\n");
    let (call, written) = [
        (
            "  match post[16](\"http://example.com:8080/items?x=1#frag\", text(\"hi there\"), headers: [(header_name(\"Accept\"), header_value(\"text/plain\"))]) {\n    .ok(r) { print(\"{r.status}\") }\n    .err(e) { print(\"{e}\") }\n  }\n",
            "POST /items?x=1 HTTP/1.1\nhost: example.com:8080\nconnection: close\naccept: text/plain\ncontent-type: text/plain; charset=utf-8\ncontent-length: 8\n\nhi there",
        ),
        (
            "  match get[16](\"http://example.com\") {\n    .ok(r) { print(\"{r.status}\") }\n    .err(e) { print(\"{e}\") }\n  }\n",
            "GET / HTTP/1.1\nhost: example.com\nconnection: close\n\n",
        ),
        (
            "  match head(\"http://[::1]:9/a\") {\n    .ok(r) { print(\"{r.status}\") }\n    .err(e) { print(\"{e}\") }\n  }\n",
            "HEAD /a HTTP/1.1\nhost: [::1]:9\nconnection: close\n\n",
        ),
    ][part];
    let said = printed(&program(&[vec![reply]], ".err(.refused(\"{host}:{port}\"))", call)).replace('\r', "");
    // The request, the closing with the 38 bytes of the reply asked for, the
    // status, and the last request written, which is the only one.
    assert_eq!(said, format!("request\n{written}\nclosed 38\n200\nwrote\n{written}\n"));
}

/// An address that is not one, a server that is not there, and `https` are each
/// a value, `.address`, `.unreachable`, and `.failed`, with the address and what
/// was said, and none of them writes a byte.
fn an_address_that_is_not_one_a_server_not_found_and_https_are_values_and_nothing_is_written() {
    let urls = [
        "example.com/keys",
        "ftp://example.com/",
        "http:///x",
        "http://example.com:99999/x",
        "http://user@example.com/x",
        "https://example.com/keys",
        "http://unknown.test/keys",
        "http://refused.test/keys",
        "http://other.test/keys",
    ];
    let mut call = String::new();
    for url in urls {
        call += &format!("  match get[16](\"{url}\") {{\n    .ok(r) {{ print(\"{{r.status}}\") }}\n    .err(e) {{ print(\"{{e}}\") }}\n  }}\n");
    }
    let refuse = "if host == \"unknown.test\" { .err(.unknown_host(\"{host}:{port}\")) } else if host == \"refused.test\" { .err(.refused(\"{host}:{port}\")) } else { .err(.failed((\"{host}:{port}\", \"connect: error 110\"))) }";
    let said = printed(&program(&[], refuse, &call));
    let wanted = "example.com/keys: not an address
ftp://example.com/: not an address
http:///x: not an address
http://example.com:99999/x: not an address
http://user@example.com/x: not an address
https://example.com/keys: https is not built yet
http://unknown.test/keys: server not reached
http://refused.test/keys: server not reached
http://other.test/keys: other.test:80: connect: error 110
wrote

";
    assert_eq!(said, wanted);
}

/// A handler of `fetch` that a test writes gives the replies it wants, to a
/// program that asks other servers, with no network and no `net`: it matches each
/// case and has exactly what that method has, and a reply with a body past the
/// limit the request carries is `.too_big` whatever the handler meant.
fn a_handler_of_a_test_answers_each_method_with_no_network_and_a_body_past_the_limit_is_too_big() {
    let source = "use std.bits (u64)
use std.console (console, print)
use std.fetch (Reply, delete, fetch, get, head, patch, post, put)
use std.list (freeze, push)
use std.message (content, content_type, form, json, size, text)
use std.utf8 (utf8)

struct Item {
  id: u64
}

fn shown(b: take List[Byte]) -> str <alloc> {
  match utf8(freeze(b)) {
    .ok(t) { t }
    .err(e) { \"not text\" }
  }
}

fn listed(t: str) -> List[Byte] <alloc> {
  var made: List[Byte] = []
  loop i in 0..t.size { made = push(made, t[i]) }
  made
}

fn main() <console, alloc> {
  fetch(
    send: fn(request) {
      match request {
        .get(u) {
          if u.url == \"http://big/\" {
            .ok(Reply(status: 200, headers: [], body: listed(\"0123456789\")))
          } else {
            .ok(Reply(status: 200, headers: [(\"x-url\", u.url)], body: listed(\"{u.most}\")))
          }
        }
        .head(u) { .ok(Reply(status: 204, headers: [], body: [])) }
        .delete(u) { .ok(Reply(status: 202, headers: [], body: [])) }
        .post(u) { .ok(Reply(status: 201, headers: [], body: listed(\"{content_type(u.payload)}|{size(u.payload)}|{shown(content(u.payload))}\"))) }
        .put(u) { .ok(Reply(status: 200, headers: [], body: [])) }
        .patch(u) { .ok(Reply(status: 200, headers: [], body: [])) }
      }
    },
    body: {
      match get(\"http://one/\") {
        .ok(r) { print(\"get {r.status} {shown(r.body.bytes)} {r.headers.size}\") }
        .err(e) { print(\"{e}\") }
      }
      match get[3](\"http://big/\") {
        .ok(r) { print(\"small {r.status}\") }
        .err(e) { print(\"{e}\") }
      }
      match get[100](\"http://big/\") {
        .ok(r) { print(\"big {r.status} {shown(r.body.bytes)}\") }
        .err(e) { print(\"{e}\") }
      }
      match post[64](\"http://p/\", json(Item(id: 4))) {
        .ok(r) { print(\"post {r.status} {shown(r.body.bytes)}\") }
        .err(e) { print(\"{e}\") }
      }
      match put[8](\"http://p/\", form([(\"a\", \"b\")])) {
        .ok(r) { print(\"put {r.status}\") }
        .err(e) { print(\"{e}\") }
      }
      match patch[8](\"http://p/\", text(\"x\")) {
        .ok(r) { print(\"patch {r.status}\") }
        .err(e) { print(\"{e}\") }
      }
      match delete[8](\"http://p/\") {
        .ok(r) { print(\"delete {r.status}\") }
        .err(e) { print(\"{e}\") }
      }
      match head(\"http://p/\") {
        .ok(r) { print(\"head {r.status} {r.headers.size}\") }
        .err(e) { print(\"{e}\") }
      }
    },
  )
}
";
    let said = printed(source);
    let wanted = "get 200 1048576 1
http://big/: body longer than 3 bytes
big 200 0123456789
post 201 application/json|8|{\"id\":4}
put 200
patch 200
delete 202
head 204 0
";
    assert_eq!(said, wanted);
}

/// A call that does not fit its method is refused where it is written, with a
/// message that names the method: a body to `get`, no body to `post`, a limit to
/// `head`, a second body to `post`, and the body of what `head` gives. Each part
/// checks two of the five.
fn a_request_that_is_wrong_is_refused_where_it_is_written_with_a_message_that_names_the_method(part: usize) {
    let in_main = |line: &str| {
        format!(
            "use std.console (console, print)\nuse std.fetch (fetch, get, head, post)\nuse std.message (text)\n\nfn main() <console, fetch, alloc> {{\n  {line}\n  print(\"x\")\n}}\n"
        )
    };
    let wrong = [
        (in_main("a = get(\"http://x/\", body: text(\"x\"))"), "`get", "has no parameter named `body`"),
        (in_main("b = post(\"http://x/\")"), "`post", "needs an argument for `payload`"),
        (in_main("c = post(\"http://x/\", text(\"y\"), body: text(\"z\"))"), "`post", "has no parameter named `body`"),
        (in_main("e = head(\"http://x/\", limit: 4)"), "`head", "has no parameter named `limit`"),
        (in_main("f = head(\"http://x/\", text(\"y\"))"), "`head`", "is given by name"),
        (
            "use std.console (console, print)\nuse std.fetch (fetch, head)\n\nfn main() <console, fetch, alloc> {\n  match head(\"http://x/\") {\n    .ok(seen) { print(\"{seen.body.bytes.size}\") }\n    .err(e) { print(\"{e}\") }\n  }\n}\n".to_string(),
            "HeadResponse",
            "body",
        ),
    ];
    for (source, method, said) in wrong.iter().skip(part).step_by(3) {
        let errors: Vec<String> = smith::check(source.as_bytes())
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert!(
            errors.iter().any(|m| m.contains(method) && m.contains(said)),
            "{errors:?} for {source}"
        );
    }
}

/// What the client asked of the server for each request, which the server answers
/// in the pieces of its case, with the limit of its case: the bytes it asked, which
/// the connection says when it is closed, and the line of the result.
fn asked_of(cases: &[(Vec<Vec<u8>>, usize)]) -> Vec<(usize, String)> {
    let mut call = String::new();
    for (i, (_, limit)) in cases.iter().enumerate() {
        call += &format!(
            "  match get[{limit}](\"http://example.com/x{i}\") {{\n    .ok(r) {{ print(\"ok {{r.status}} {{r.body.bytes.size}}\") }}\n    .err(e) {{ print(\"{{e}}\") }}\n  }}\n"
        );
    }
    let connections: Vec<Vec<Vec<u8>>> = cases.iter().map(|(p, _)| p.clone()).collect();
    let source = program(&connections, ".err(.refused(\"{host}:{port}\"))", &call);
    let said = printed(&source);
    let lines: Vec<&str> = said.lines().collect();
    let mut out = Vec::new();
    let mut at = 0;
    for _ in cases {
        while !lines[at].starts_with("closed ") {
            at += 1;
        }
        out.push((lines[at].strip_prefix("closed ").unwrap().parse().unwrap(), lines[at + 1].to_string()));
        at += 2;
    }
    out
}

/// A response whose body is one byte past the limit is `.too_big` and the client
/// asks no more of the server than it needs to know so. A head that says the body
/// is longer than the limit is enough: a server that sends the head and nothing
/// after it is answered at once, with not a byte of the body asked for. A body to
/// the end of the connection is refused when it has the limit and one byte, and a
/// chunk that takes the body past the limit is refused when it is whole, and in
/// neither is a byte asked that the server had not said was there: the server has
/// more to send, and what is asked says the client did not wait for it. Each
/// check fails when its limit is one more.
fn a_body_past_the_limit_is_refused_as_soon_as_the_bytes_or_the_head_say_so_and_no_more_is_read() {
    let mut cases = Vec::new();
    let mut wanted = Vec::new();
    for (i, limit) in [0usize, 5].into_iter().enumerate() {
        let body = vec![b'a'; limit + 1];
        let said = format!("http://example.com/x{}: body longer than {limit} bytes", 3 * i);
        // By its length: the head alone, which is the whole of what is asked.
        let by_length = head(200, "OK", &[], &format!("content-length: {}\r\n", limit + 1));
        wanted.push((by_length.len(), said.clone()));
        cases.push((vec![by_length], limit));
        // To the end of the connection: the head, the limit, and one byte.
        let to_close = head_of_close();
        wanted.push((to_close.len() + limit + 1, said.replace(&format!("x{}", 3 * i), &format!("x{}", 3 * i + 1))));
        cases.push((vec![to_close, body.clone(), b"zzzzzzzzzz".to_vec()], limit));
        // In chunks: the head and the chunk that passes the limit, whole.
        let chunked = head_of_chunked();
        let mut chunk = format!("{:x}\r\n", limit + 1).into_bytes();
        chunk.extend(&body);
        chunk.extend(b"\r\n");
        wanted.push((chunked.len() + chunk.len(), said.replace(&format!("x{}", 3 * i), &format!("x{}", 3 * i + 2))));
        cases.push((vec![chunked, chunk, b"0\r\n\r\n".to_vec()], limit));
    }
    let got: Vec<(usize, String)> = asked_of(&cases);
    assert_eq!(got, wanted);
}

fn head_of_close() -> Vec<u8> {
    head(200, "OK", &[], "")
}

fn head_of_chunked() -> Vec<u8> {
    head(200, "OK", &[], "transfer-encoding: chunked\r\n")
}
