//! A task that fails ends what it owns at the call the failure went up through,
//! as a task that is asked to end does: a connection it opened is closed, once,
//! by the handler in force, and the parent that asks for its outcome is told the
//! failure. The evaluator runs the landing of each call that the function owns
//! something across.

use smith::{lower, scalar};

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "a_task_that_runs_out_of_memory_closes_the_connection_it_opened_once_and_its_parent_is_told",
        a_task_that_runs_out_of_memory_closes_the_connection_it_opened_once_and_its_parent_is_told,
    ),
    (
        "a_task_that_runs_out_of_memory_two_calls_deep_closes_the_connection_of_each_function_once",
        a_task_that_runs_out_of_memory_two_calls_deep_closes_the_connection_of_each_function_once,
    ),
    (
        "a_failure_in_a_function_that_owns_nothing_and_is_inlined_ends_what_its_caller_owns",
        a_failure_in_a_function_that_owns_nothing_and_is_inlined_ends_what_its_caller_owns,
    ),
    (
        "a_failure_in_an_inlined_function_that_owns_ends_what_it_owns_and_then_what_its_caller_owns",
        a_failure_in_an_inlined_function_that_owns_ends_what_it_owns_and_then_what_its_caller_owns,
    ),
    (
        "a_function_that_holds_a_connection_and_calls_a_small_allocating_helper_that_is_inlined_closes_it_when_the_helper_runs_out_of_memory",
        a_function_that_holds_a_connection_and_calls_a_small_allocating_helper_that_is_inlined_closes_it_when_the_helper_runs_out_of_memory,
    ),
    (
        "a_task_that_does_not_fail_closes_its_connection_where_it_closes_it_and_nowhere_else",
        a_task_that_does_not_fail_closes_its_connection_where_it_closes_it_and_nowhere_else,
    ),
];

/// A program whose handler of `net` prints the number of each connection it is
/// asked to close, and that runs `tasks` in a scheduler of a test, with the
/// functions it is given.
fn program(functions: &str, tasks: &str) -> String {
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.list (push)
use std.net (close, connection, handle, net, release, Connection)
use std.task (join, outcome, spawn, task, test_scheduler)

// Asks for more memory than the evaluator gives, so that the task ends.
fn eat() -> u64 <alloc> {{
  mut xs: List[u64] = []
  loop i in 0..9223372036854775807 {{
    xs = push(xs, i)
  }}
  0
}}

{functions}
fn run() -> u64 <net, diverge, alloc, console> {{
  with test_scheduler(3)
{tasks}
  1
}}

fn main() <console, alloc, diverge> {{
  r = net(
    listen_on: fn(host, port) {{ .err(.refused(host)) }},
    accept: fn(l) {{ .err(.closed(l.host)) }},
    stop: fn(l) {{ n = release(l) }},
    connect_to: fn(host, port) {{ .err(.refused(host)) }},
    read: fn(c, most) {{ .err(.closed(c.host)) }},
    write: fn(c, bytes) {{ .ok(()) }},
    close: fn(c) {{
      print(\"closed {{handle(c)}}\")
      n = release(c)
    }},
    bind_to: fn(host, port) {{ .err(.refused(host)) }},
    send: fn(s, to, bytes) {{ .err(.closed(s.host)) }},
    receive: fn(s, most) {{ .err(.closed(s.host)) }},
    unbind: fn(s) {{ n = release(s) }},
    body: {{ run() }},
  )
  print(\"{{r}}\")
}}
"
    )
}

fn said(source: &str) -> Vec<String> {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    said_by(source, program)
}

/// What the program prints, which `source` is the text of.
fn said_by(source: &str, program: scalar::Program) -> Vec<String> {
    assert_eq!(scalar::check(&program), Ok(()), "for {source}");
    let (outcome, _) = scalar::run_counting(&scalar::passes(&program), 1 << 20);
    let text = String::from_utf8(outcome.stdout).expect("text");
    assert_eq!(outcome.status, 0, "{text} for {source}");
    text.lines().map(str::to_string).collect()
}

const OWNER: &str = "fn owner(n: u64) -> u64 <net, alloc> {
  c = connection(n)
  x = eat()
  close(c)
  x
}
";

const REPORT: &str = "  child = spawn { owner(5) }
  match outcome(child) {
    .ok(n) { print(\"a value\") }
    .err(.out_of_memory) { print(\"out of memory\") }
    .err(.refused_write) { print(\"refused write\") }
  }
";

fn a_task_that_runs_out_of_memory_closes_the_connection_it_opened_once_and_its_parent_is_told() {
    let lines = said(&program(OWNER, REPORT));
    assert_eq!(lines, ["closed 5", "out of memory", "1"]);
}

const TWO_DEEP: &str = "fn inner(n: u64) -> u64 <net, alloc> {
  c = connection(n)
  x = eat()
  close(c)
  x
}

fn outer(n: u64) -> u64 <net, alloc> {
  c = connection(n)
  x = inner(6)
  close(c)
  x
}

";

fn a_task_that_runs_out_of_memory_two_calls_deep_closes_the_connection_of_each_function_once() {
    let source = program(TWO_DEEP, REPORT).replace("owner(5)", "outer(5)");
    let lines = said(&source);
    // The function that failed ends what it owns first, and then its caller.
    assert_eq!(lines, ["closed 6", "closed 5", "out of memory", "1"]);
}

const THROUGH_A_CALL_INLINED: &str = "fn middle() -> u64 <alloc> {
  eat()
}

fn owner(n: u64) -> u64 <net, alloc> {
  c = connection(n)
  x = middle()
  close(c)
  x
}
";

fn a_failure_in_a_function_that_owns_nothing_and_is_inlined_ends_what_its_caller_owns() {
    let lines = said(&program(THROUGH_A_CALL_INLINED, REPORT));
    assert_eq!(lines, ["closed 5", "out of memory", "1"]);
}

const OWNS_AND_INLINED: &str = "fn middle(n: u64) -> u64 <net, alloc> {
  c = connection(n)
  x = eat()
  close(c)
  x
}

fn owner(n: u64) -> u64 <net, alloc> {
  c = connection(n)
  x = middle(6)
  close(c)
  x
}
";

fn a_failure_in_an_inlined_function_that_owns_ends_what_it_owns_and_then_what_its_caller_owns() {
    let lines = said(&program(OWNS_AND_INLINED, REPORT));
    assert_eq!(lines, ["closed 6", "closed 5", "out of memory", "1"]);
}

/// A function whose one request for memory is the literal of a list, which asks
/// for what it holds all at once.
fn a_big_literal() -> String {
    let items = vec!["i"; 4000].join(", ");
    format!(
        "fn big(i: u64) -> List[u64] <alloc> {{
  [{items}]
}}

fn owner(n: u64) -> u64 <net, alloc> {{
  c = connection(n)
  mut kept: List[List[u64]] = []
  loop i in 0..9223372036854775807 {{
    kept = push(kept, big(i))
  }}
  close(c)
  0
}}
"
    )
}

fn a_function_that_holds_a_connection_and_calls_a_small_allocating_helper_that_is_inlined_closes_it_when_the_helper_runs_out_of_memory()
 {
    let source = program(&a_big_literal(), REPORT);
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    // The helper is no function of the program once its call is replaced, though
    // a landing follows the call, and its request for memory is in the owner.
    let inlined = scalar::inline(program.clone());
    assert!(
        !inlined.functions.iter().any(|f| f.name.contains("big")),
        "the helper is still called"
    );
    let lines = said_by(&source, program);
    assert_eq!(lines, ["closed 5", "out of memory", "1"]);
}

fn a_task_that_does_not_fail_closes_its_connection_where_it_closes_it_and_nowhere_else() {
    let source = program(
        "fn owner(n: u64) -> u64 <net, alloc> {
  c = connection(n)
  close(c)
  n
}
",
        REPORT,
    );
    let lines = said(&source);
    assert_eq!(lines, ["closed 5", "a value", "1"]);
}
