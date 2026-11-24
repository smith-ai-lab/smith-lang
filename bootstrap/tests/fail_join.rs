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
];

/// A parent that starts a child that asks for more memory than the limit gives,
/// and says what it did after `joins`, which is what a test puts there.
fn program(joins: &str) -> String {
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.list (push)
use std.task (join, spawn, test_ready_by, test_scheduler)

fn eat() -> u64 ! alloc {{
  var xs: List[u64] = []
  loop i in 0..9223372036854775807 {{
    xs = push(xs, i)
  }}
  0
}}

fn run() ! console, alloc, diverge {{
  with test_scheduler(3)
  child = spawn {{ eat() }}
{joins}  print(\"the parent went on\")
}}

fn main() ! console, alloc, diverge {{
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
    let (status, text) = said(&program("  n = join(child)\n"));
    assert_eq!((status, text.as_str()), (1, ""));
}

fn a_child_that_ran_out_of_memory_and_is_never_joined_leaves_its_parent_to_go_on() {
    let (status, text) = said(&program("  late = test_ready_by(99, 10)\n"));
    assert_eq!((status, text.as_str()), (0, "the parent went on\n"));
}
