//! A server of `std.mcp` is tested with no standard input and output: a handler
//! of `console` that a test writes gives the lines the client would send, as the
//! effect that `stdio` reads, and keeps what the server printed, which the test
//! reads after `stdio` has ended with the input. The handler holds state, the
//! place among the lines and the list of what was printed, as a handler of `net`
//! does under `listen`.

use smith::{lower, scalar};

use crate::Test;
use crate::standard::{Json, lit, parse};

pub const ONCE: &[Test] = &[(
    "a_scripted_handler_of_console_that_keeps_what_the_server_printed_is_read_after_stdio_ends",
    a_scripted_handler_of_console_that_keeps_what_the_server_printed_is_read_after_stdio_ends,
)];

const META: &str = "\"_meta\":{\"io.modelcontextprotocol/protocolVersion\":\"2026-07-28\",\"io.modelcontextprotocol/clientCapabilities\":{}}";

fn request(id: u32, method: &str, params: &str) -> String {
    let sep = if params.is_empty() { "" } else { "," };
    format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"{method}\",\"params\":{{{params}{sep}{META}}}}}")
}

fn call(id: u32, name: &str, arguments: &str) -> String {
    request(id, "tools/call", &format!("\"name\":{},\"arguments\":{arguments}", lit(name)))
}

/// The server of the page, and a handler of `console` whose lines are given to
/// it, which keeps what is printed.
const SERVER: &str = "use std.bits (u64)
use std.console (console, print)
use std.int (checked_div, wrapping_add)
use std.list (push)
use std.mcp (mcp, messages, stdio, tool)

// Divides one whole number by another.
fn divide(
  // The number that is divided.
  a: Int,
  // The number it is divided by.
  b: Int,
) -> Result[Int, str] {
  if b == 0 {
    .err(\"b is zero\")
  } else {
    match checked_div(a, b) {
      .some(q) { .ok(q) }
      .none { .err(\"the quotient does not fit\") }
    }
  }
}

fn serve() <messages, alloc> {
  with mcp(name: \"calc\", version: \"1\")
  tool(divide)
}

fn scripted[T](lines: List[str], body: fn() -> T <console>) -> (T, List[str]) <alloc> {
  var next: u64 = 0
  var printed: List[str] = []
  result = console(
    print: fn(text, end, to) {
      printed = push(printed, text)
    },
    input: fn(prompt) {
      var found: Option[str] = .none
      loop i in 0..lines.size {
        if i == next { found = .some(lines[i]) }
      }
      next = wrapping_add(next, 1)
      found
    },
    body: body,
  )
  (result, printed)
}

fn run(lines: List[str]) -> ((), List[str]) <alloc> {
  with scripted(lines)
  with stdio()
  serve()
}

";

fn a_scripted_handler_of_console_that_keeps_what_the_server_printed_is_read_after_stdio_ends() {
    let messages = [
        request(1, "tools/list", ""),
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}".to_string(),
        call(2, "divide", "{\"a\":6,\"b\":3}"),
        call(3, "divide", "{\"a\":1,\"b\":0}"),
    ];
    let lines: Vec<String> = messages.iter().map(|m| lit(m)).collect();
    let source = format!(
        "{SERVER}fn main() <console, alloc> {{\n  (done, printed) = run([{}])\n  print(\"{{printed.size}}\")\n  loop i in 0..printed.size {{\n    line = printed[i]\n    print(line)\n  }}\n}}\n",
        lines.join(", ")
    );
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(scalar::check(&program), Ok(()));
    let said = scalar::run(&program);
    assert_eq!(said.status, 0, "{}", String::from_utf8_lossy(&said.stdout));
    let printed = String::from_utf8(said.stdout).unwrap();
    let mut printed = printed.lines();
    // One answer for each request and none for the notification.
    assert_eq!(printed.next(), Some("3"));
    let answers: Vec<&str> = printed.collect();
    assert_eq!(answers.len(), 3, "{answers:?}");
    let listed = parse(answers[0]).get("result").cloned().expect("a result");
    let Some(Json::Array(tools)) = listed.get("tools") else { panic!("{}", answers[0]) };
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].get("name").and_then(Json::text), Some("divide"));
    let divided = parse(answers[1]).get("result").cloned().expect("a result");
    assert_eq!(divided.get("structuredContent"), Some(&Json::Number("2".into())), "{}", answers[1]);
    let refused = parse(answers[2]).get("result").cloned().expect("a result");
    assert_eq!(refused.get("isError"), Some(&Json::Bool(true)), "{}", answers[2]);
}
