//! A task that runs out of memory ends by a failure that its poll catches, and
//! the program goes on: a parent that does not wait for it is never told, and a
//! parent that joins it never gets a value, since the child never made one, so
//! the program ends with the status 1 at the join. The evaluator refuses the
//! request that is over its limit, as the system does.

use smith::{lower, scalar};

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "a_join_of_a_child_that_ran_out_of_memory_gives_no_value_and_ends_the_program_with_the_status_1",
        a_join_of_a_child_that_ran_out_of_memory_gives_no_value_and_ends_the_program_with_the_status_1,
    ),
    (
        "a_child_that_ran_out_of_memory_and_is_never_joined_leaves_its_parent_to_go_on",
        a_child_that_ran_out_of_memory_and_is_never_joined_leaves_its_parent_to_go_on,
    ),
    (
        "the_outcome_of_a_child_that_ran_out_of_memory_says_so_and_its_parent_goes_on",
        the_outcome_of_a_child_that_ran_out_of_memory_says_so_and_its_parent_goes_on,
    ),
    (
        "the_outcome_of_a_child_that_returns_a_value_is_that_value",
        the_outcome_of_a_child_that_returns_a_value_is_that_value,
    ),
    (
        "a_task_that_joins_a_child_that_ran_out_of_memory_ends_by_the_same_failure_which_its_own_parent_sees",
        a_task_that_joins_a_child_that_ran_out_of_memory_ends_by_the_same_failure_which_its_own_parent_sees,
    ),
];

/// A parent that starts a child that asks for more memory than the limit gives,
/// and says what it did after `joins`, which is what a test puts there.
fn program(joins: &str) -> String {
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.instant (ms)
use std.list (push)
use std.task (join, spawn, test_ready_by, test_scheduler)

fn eat() -> u64 <alloc> {{
  mut xs: List[u64] = []
  loop i in 0..9223372036854775807 {{
    push(mut xs, i)
  }}
  0
}}

fn run() <console, alloc, diverge> {{
  with test_scheduler(3)
  child = spawn {{ eat() }}
{joins}  print(\"the parent went on\")
}}

fn main() <console, alloc, diverge> {{
  run()
}}
"
    )
}

fn said(source: &str) -> (i32, String) {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(scalar::check(&program), Ok(()), "for {source}");
    let (outcome, _) = scalar::run_counting(&scalar::passes(&program), 1 << 20);
    (outcome.status, String::from_utf8(outcome.stdout).expect("text"))
}

fn a_join_of_a_child_that_ran_out_of_memory_gives_no_value_and_ends_the_program_with_the_status_1() {
    let (status, text) = said(&program("  n = join(take child)\n"));
    assert_eq!((status, text.as_str()), (1, ""));
}

fn a_child_that_ran_out_of_memory_and_is_never_joined_leaves_its_parent_to_go_on() {
    let (status, text) = said(&program("  late = test_ready_by(99, ms(10))\n"));
    assert_eq!((status, text.as_str()), (0, "the parent went on\n"));
}

fn the_outcome_of_a_child_that_ran_out_of_memory_says_so_and_its_parent_goes_on() {
    let source = program("  match outcome(take child) {\n    .ok(n) { print(\"a value\") }\n    .err(.out_of_memory) { print(\"out of memory\") }\n    .err(.refused_write) { print(\"refused write\") }\n  }\n")
        .replace("use std.task (join, spawn, test_ready_by, test_scheduler)", "use std.task (join, outcome, spawn, test_ready_by, test_scheduler)");
    let (status, text) = said(&source);
    assert_eq!((status, text.as_str()), (0, "out of memory\nthe parent went on\n"));
}

fn the_outcome_of_a_child_that_returns_a_value_is_that_value() {
    let source = "use std.bits (u64)
use std.console (console, print)
use std.task (outcome, spawn, test_scheduler)

fn run() <console, alloc, diverge> {
  with test_scheduler(3)
  child = spawn { 41 }
  match outcome(take child) {
    .ok(n) { print(\"{n}\") }
    .err(.out_of_memory) { print(\"out of memory\") }
    .err(.refused_write) { print(\"refused write\") }
  }
}

fn main() <console, alloc, diverge> {
  run()
}
";
    let (status, text) = said(source);
    assert_eq!((status, text.as_str()), (0, "41\n"));
}

/// The middle task joins a child that runs out of memory, so it ends by the same
/// failure, and the root, which asks for its outcome, is told that one.
fn a_task_that_joins_a_child_that_ran_out_of_memory_ends_by_the_same_failure_which_its_own_parent_sees() {
    let source = "use std.bits (u64)
use std.console (console, print)
use std.list (push)
use std.task (join, outcome, spawn, task, test_scheduler)

fn eat() -> u64 <alloc> {
  mut xs: List[u64] = []
  loop i in 0..9223372036854775807 {
    push(mut xs, i)
  }
  0
}

fn middle() -> u64 <alloc, task, console> {
  child = spawn { eat() }
  n = join(take child)
  print(\"the middle task went on\")
  n
}

fn run() <console, alloc, diverge> {
  with test_scheduler(3)
  child = spawn { middle() }
  match outcome(take child) {
    .ok(n) { print(\"a value\") }
    .err(.out_of_memory) { print(\"out of memory\") }
    .err(.refused_write) { print(\"refused write\") }
  }
}

fn main() <console, alloc, diverge> {
  run()
}
";
    let (status, text) = said(source);
    assert_eq!((status, text.as_str()), (0, "out of memory\n"));
}
