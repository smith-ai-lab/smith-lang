//! A handle of `net` that a task owns when it is asked to end is closed by the
//! handler in force, once: one that a function holds, one in a struct of two, and
//! one that a child gave its parent by `join`. A function that holds a handle
//! while it may wait and does not carry `net` is refused where it is compiled.

use smith::{lower, scalar};

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "a_cancelled_task_closes_the_connection_it_holds_through_the_handler_in_force_once",
        a_cancelled_task_closes_the_connection_it_holds_through_the_handler_in_force_once,
    ),
    (
        "a_cancelled_task_closes_both_connections_of_a_struct_it_holds",
        a_cancelled_task_closes_both_connections_of_a_struct_it_holds,
    ),
    (
        "a_connection_given_to_a_parent_by_join_is_closed_once_by_the_parent_and_not_by_the_child_that_ended",
        a_connection_given_to_a_parent_by_join_is_closed_once_by_the_parent_and_not_by_the_child_that_ended,
    ),
    (
        "a_function_that_holds_a_connection_while_it_may_wait_and_does_not_carry_net_is_refused",
        a_function_that_holds_a_connection_while_it_may_wait_and_does_not_carry_net_is_refused,
    ),
    (
        "a_connection_that_is_closed_by_its_function_is_not_closed_again_when_no_task_is_asked_to_end",
        a_connection_that_is_closed_by_its_function_is_not_closed_again_when_no_task_is_asked_to_end,
    ),
];

/// A handler of `net` that prints the number of each connection it is asked to
/// close and of each it is asked to write to, so the program says what the
/// handler was asked.
fn program(functions: &str, tasks: &str, seed: u64) -> String {
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.int (wrapping_add)
use std.net (close, connection, handle, net, release, write, Connection)
use std.list (freeze)
use std.task (join, spawn, task, test_ready, test_ready_by, test_scheduler_with_io)

{functions}
fn run() -> u64 ! net, diverge, alloc {{
  with test_scheduler_with_io({seed}, [])
{tasks}
  late = test_ready_by(99, 10)
  1
}}

fn main() ! console, alloc, diverge {{
  r = net(
    listen_on: fn(host, port) {{ .err(.refused) }},
    accept: fn(l) {{ .err(.closed) }},
    stop: fn(l) {{ n = release(l) }},
    connect_to: fn(host, port) {{ .err(.refused) }},
    read: fn(c, most) {{ .err(.closed) }},
    write: fn(c, bytes) {{
      print(\"wrote {{handle(c)}}\")
      .ok(())
    }},
    close: fn(c) {{
      print(\"closed {{handle(c)}}\")
      n = release(c)
    }},
    bind_to: fn(host, port) {{ .err(.refused) }},
    send: fn(s, to, bytes) {{ .err(.closed) }},
    receive: fn(s, most) {{ .err(.closed) }},
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
    assert_eq!(scalar::check(&program), Ok(()), "for {source}");
    let (outcome, _) = scalar::run_counting(&scalar::passes(&program), usize::MAX);
    let text = String::from_utf8(outcome.stdout).expect("text");
    assert_eq!(outcome.status, 0, "{text} for {source}");
    text.lines().map(str::to_string).collect()
}

fn sorted(mut lines: Vec<String>) -> Vec<String> {
    lines.sort();
    lines
}

const HOLD: &str = "fn hold(n: u64) -> u64 ! net, task {
  c = connection(n)
  test_ready(n)
  close(c)
  n
}
";

fn a_cancelled_task_closes_the_connection_it_holds_through_the_handler_in_force_once() {
    for seed in 0..8 {
        let source = program(HOLD, "  spawn { hold(5) }\n  spawn { hold(6) }", seed);
        assert_eq!(
            sorted(said(&source)),
            ["1", "closed 5", "closed 6"],
            "seed {seed}"
        );
    }
}

const PAIR: &str = "struct Pair {
  a: Connection
  b: Connection
}

fn hold_two(n: u64) -> u64 ! net, task {
  p = Pair(a: connection(n), b: connection(wrapping_add(n, 1)))
  test_ready(n)
  close(p.a)
  close(p.b)
  n
}
";

fn a_cancelled_task_closes_both_connections_of_a_struct_it_holds() {
    for seed in 0..8 {
        let source = program(PAIR, "  spawn { hold_two(10) }", seed);
        assert_eq!(
            sorted(said(&source)),
            ["1", "closed 10", "closed 11"],
            "seed {seed}"
        );
    }
}

const GIVEN: &str = "fn parent(n: u64) -> u64 ! net, task {
  child = spawn { connection(n) }
  c = join(child)
  test_ready(n)
  close(c)
  n
}
";

/// The child opens a connection and ends, giving it to its parent, which is
/// cancelled while it waits, and a sibling that holds one is cancelled too. Each
/// of the two is closed once, and the child that ended closes nothing.
fn a_connection_given_to_a_parent_by_join_is_closed_once_by_the_parent_and_not_by_the_child_that_ended()
 {
    for seed in 0..8 {
        let source = program(
            &format!("{GIVEN}\n{HOLD}"),
            "  spawn { parent(7) }\n  spawn { hold(8) }",
            seed,
        );
        assert_eq!(
            sorted(said(&source)),
            ["1", "closed 7", "closed 8"],
            "seed {seed}"
        );
    }
}

fn a_function_that_holds_a_connection_while_it_may_wait_and_does_not_carry_net_is_refused() {
    let source = "use std.net (Connection, close, connection, net)
use std.time (sleep, time)

fn relay(c: take Connection) -> Connection ! time {
  sleep(10)
  c
}

fn main() ! time, net {
  close(relay(connection(3)))
}
";
    let errors = match lower(source.as_bytes()) {
        Ok(_) => panic!("a program that is refused was built"),
        Err(errors) => format!("{errors:?}"),
    };
    assert!(
        errors.contains("`relay` holds a `Connection` while it may wait, so it must carry `net`"),
        "{errors}"
    );
    assert!(errors.contains("add `net` to its effects"), "{errors}");
}

const FINISH: &str = "fn finish(n: u64) -> u64 ! net, task {
  c = connection(n)
  test_ready(n)
  close(c)
  n
}
";

/// A task that is never asked to end, which waits for its handle and then closes
/// it itself, closes it once: the handler is asked once, and by the function.
fn a_connection_that_is_closed_by_its_function_is_not_closed_again_when_no_task_is_asked_to_end() {
    let source = program(FINISH, "  spawn { finish(4) }", 0)
        .replace("test_scheduler_with_io(0, [])", "test_scheduler_with_io(0, [4])");
    assert_eq!(sorted(said(&source)), ["1", "closed 4"]);
}
