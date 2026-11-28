//! A handler of `net` that a test writes makes a `Listener`, a `Connection`,
//! and a `Socket` from a number it chooses and reads the number back, so a
//! server is tested with no socket; and the system's handler, given a value
//! that it did not open, answers with `failed` and makes no system call, so a
//! number made up in a program reaches nothing the program does not own.

use smith::{lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::lowered_once::{SEED, Written};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "a_handler_makes_a_listener_a_connection_and_a_socket_from_numbers_and_reads_them_back",
        a_handler_makes_a_listener_a_connection_and_a_socket_from_numbers_and_reads_them_back,
    ),
    (
        "a_server_of_messages_whose_tool_asks_the_network_is_answered_over_a_handler_of_net_that_chooses_the_connection",
        a_server_of_messages_whose_tool_asks_the_network_is_answered_over_a_handler_of_net_that_chooses_the_connection,
    ),
];

pub const MACHINE: &[Test] = &[(
    "the_systems_handler_refuses_a_number_made_up_in_a_program_and_makes_no_system_call_with_it",
    the_systems_handler_refuses_a_number_made_up_in_a_program_and_makes_no_system_call_with_it,
)];

/// The numbers a handler chooses for the three, which differ.
fn numbers(g: &mut Gen) -> (u64, u64, u64) {
    let listener = 1 + g.below(1000) as u64;
    (listener, listener + 1 + g.below(1000) as u64, listener + 1002 + g.below(1000) as u64)
}

fn a_handler_makes_a_listener_a_connection_and_a_socket_from_numbers_and_reads_them_back() {
    generate::check_cases(7, 0, 3, numbers, |&(l, c, s)| {
        let source = format!(
            "use std.console (console, print)
use std.net (Address, accept, bind_to, close, connection, handle, listen_on, listener, net, release, send, socket, stop, unbind, write)

fn faked[T](body: fn() -> T <net>) -> T <console, alloc> {{
  net(
    listen_on: fn(host, port) {{ .ok(listener({l})) }},
    accept: fn(l) {{
      print(\"{{handle(l)}}\")
      .ok(connection({c}))
    }},
    stop: fn(l) {{ n = release(take l) }},
    connect_to: fn(host, port) {{ .err(.refused(host)) }},
    read: fn(c, most) {{ .err(.closed(c.host)) }},
    write: fn(c, bytes) {{
      print(\"{{handle(c)}}\")
      .ok(())
    }},
    close: fn(c) {{ n = release(take c) }},
    bind_to: fn(host, port) {{ .ok(socket({s})) }},
    send: fn(s, to, bytes) {{
      print(\"{{handle(s)}}\")
      .ok(())
    }},
    receive: fn(s, most) {{ .err(.closed(s.host)) }},
    unbind: fn(s) {{ n = release(take s) }},
    body: body,
  )
}}

fn use_all() <net, alloc> {{
  match listen_on(\"h\", .at(1)) {{
    .ok(l) {{
      match accept(l) {{
        .ok(c) {{
          match write(c, \"x\".bytes) {{
            .ok(u) {{ }}
            .err(e) {{ }}
          }}
          close(take c)
        }}
        .err(e) {{ }}
      }}
      stop(take l)
    }}
    .err(e) {{ }}
  }}
  match bind_to(\"h\", .at(1)) {{
    .ok(s) {{
      match send(s, Address(host: \"h\", port: 1), \"x\".bytes) {{
        .ok(u) {{ }}
        .err(e) {{ }}
      }}
      unbind(take s)
    }}
    .err(e) {{ }}
  }}
}}

fn main() <console, alloc> {{
  with faked()
  use_all()
}}
"
        );
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
        assert!(!program.asks_the_kernel());
        let said = String::from_utf8(scalar::run(&program).stdout).unwrap();
        assert_eq!(said, format!("{l}\n{c}\n{s}\n"), "{source}");
    });
}

/// A server of the page, whose one tool asks a peer a word, under `stdio`, with
/// a handler of `net` that is the peer: it answers with the number it chose for
/// the connection and how many bytes it was asked for.
fn server(handle: u64) -> String {
    format!(
        "use std.console (console)
use std.list (copy, freeze)
use std.mcp (messages, mcp, stdio, tool)
use std.net (Chunk, NetError, connect, connection, handle, net, read, release, write)
use std.utf8 (utf8)

// Asks the peer a word, and gives what it answers.
fn ask(word: str) -> Result[str, NetError] <net, alloc> {{
  with c = connect(\"peer.test\", 7)
  try write(c, word.bytes)
  match try read(c, 64) {{
    .bytes(chunk) {{
      match utf8(freeze(take chunk.bytes)) {{
        .ok(text) {{ .ok(text) }}
        .err(e) {{ .err(.failed((\"peer.test:7\", \"not text\"))) }}
      }}
    }}
    .ended {{ .err(.closed(\"peer.test:7\")) }}
  }}
}}

fn serve() <messages, net, alloc> {{
  with mcp(name: \"peer\", version: \"1\")
  tool(ask)
}}

fn peer[T](body: fn() -> T <net>) -> T <alloc> {{
  net(
    listen_on: fn(host, port) {{ .err(.refused(host)) }},
    accept: fn(l) {{ .err(.refused(l.host)) }},
    stop: fn(l) {{ n = release(take l) }},
    connect_to: fn(host, port) {{ .ok(connection({handle})) }},
    read: fn(c, most) {{
      said = copy(\"{{handle(c)}} read {{most}}\".bytes)
      if said.size >= 1 {{ .ok(.bytes(Chunk(bytes: said))) }} else {{ .ok(.ended) }}
    }},
    write: fn(c, bytes) {{ .ok(()) }},
    close: fn(c) {{ n = release(take c) }},
    bind_to: fn(host, port) {{ .err(.refused(host)) }},
    send: fn(s, to, bytes) {{ .err(.refused(s.host)) }},
    receive: fn(s, most) {{ .err(.refused(s.host)) }},
    unbind: fn(s) {{ n = release(take s) }},
    body: body,
  )
}}

fn main() <console, alloc> {{
  with peer()
  with stdio()
  serve()
}}
"
    )
}

fn a_server_of_messages_whose_tool_asks_the_network_is_answered_over_a_handler_of_net_that_chooses_the_connection()
{
    // The text is the same for every handle but the handle, so it is lowered once.
    let written = Written::new(server(SEED));
    assert!(!written.program.asks_the_kernel());
    generate::check_cases(11, 0, 3, |g: &mut Gen| (1 + g.below(5000) as u64, 1 + g.below(12)), |&(handle, size)| {
        let word = "w".repeat(size);
        let call = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{{\"name\":\"ask\",\"arguments\":{{\"word\":\"{word}\"}},\"_meta\":{{\"io.modelcontextprotocol/protocolVersion\":\"2026-07-28\",\"io.modelcontextprotocol/clientCapabilities\":{{}}}}}}}}\n"
        );
        let said = String::from_utf8(scalar::run_with_input(&written.with(&[(SEED, handle)]), call.as_bytes()).stdout).unwrap();
        assert!(said.contains(&format!("\"structuredContent\":\"{handle} read 64\"")), "{said}");
    });
}

/// What the system's handler is given that it did not open: a number that is
/// open in the process, standard output, in each of the three values.
const MADE_UP: &str = "use std.console (console, print)
use std.net (Address, NetError, accept, close, connection, listener, net, read, send, receive, socket, stop, unbind, write)

fn said(e: NetError) -> str <alloc> {
  match e {
    .failed((address, m)) { m }
    .unknown_host(h) { \"unknown host\" }
    .refused(h) { \"refused\" }
    .in_use(h) { \"in use\" }
    .closed(h) { \"closed\" }
  }
}

fn main() <console, net, alloc> {
  c = connection(1)
  match write(c, \"SHOULD NOT APPEAR\\n\".bytes) {
    .ok(u) { print(\"wrote\") }
    .err(e) { print(said(e)) }
  }
  match read(c, 8) {
    .ok(b) { print(\"read\") }
    .err(e) { print(said(e)) }
  }
  close(take c)
  l = listener(1)
  match accept(l) {
    .ok(k) {
      print(\"accepted\")
      close(take k)
    }
    .err(e) { print(said(e)) }
  }
  stop(take l)
  s = socket(1)
  match send(s, Address(host: \"127.0.0.1\", port: 9), \"SHOULD NOT APPEAR\\n\".bytes) {
    .ok(u) { print(\"sent\") }
    .err(e) { print(said(e)) }
  }
  match receive(s, 8) {
    .ok(p) { print(\"received\") }
    .err(e) { print(said(e)) }
  }
  unbind(take s)
  print(\"standard output is open\")
}
";

fn the_systems_handler_refuses_a_number_made_up_in_a_program_and_makes_no_system_call_with_it() {
    for target in targets::runnable() {
        if !(target.name().starts_with("linux")
            || target.name().starts_with("macos")
            || target.name().starts_with("windows"))
        {
            continue;
        }
        let bytes = targets::build(MADE_UP.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let outcome = targets::run(&bytes);
        let said = String::from_utf8(outcome.stdout).unwrap();
        let lines: Vec<&str> = said.lines().collect();
        // Each of the five operations is refused, and nothing was written to
        // standard output by them, nor was standard output closed by the ends.
        assert_eq!(lines.len(), 6, "{}: {said}", target.name());
        for line in &lines[..5] {
            assert!(line.contains("not one the system opened"), "{}: {said}", target.name());
        }
        assert_eq!(lines[5], "standard output is open", "{}", target.name());
        assert!(!said.contains("SHOULD NOT APPEAR"), "{}", target.name());
    }
}
