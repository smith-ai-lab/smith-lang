//! The scheduler of `std.task` that a test installs, `test_scheduler(seed)`: it
//! holds state, a list of the tasks it started and a list of those that ended,
//! and that state goes through each poll and is saved in no frame, so a child
//! that starts a grandchild and joins it gives the same value for every seed,
//! one seed gives one order, and every order of two tasks is some seed's. A
//! child under another handler that holds state is refused as not built yet.

use std::collections::BTreeSet;

use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate;
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_child_that_starts_a_grandchild_and_joins_it_gives_the_same_value_for_every_seed",
        a_child_that_starts_a_grandchild_and_joins_it_gives_the_same_value_for_every_seed,
    ),
    (
        "one_seed_gives_one_order_and_some_seed_shows_each_order_of_two_tasks",
        one_seed_gives_one_order_and_some_seed_shows_each_order_of_two_tasks,
    ),
    (
        "a_task_that_waits_for_another_runs_again_after_that_one_has_ended",
        a_task_that_waits_for_another_runs_again_after_that_one_has_ended,
    ),
    (
        "a_child_under_the_scheduler_and_another_handler_that_holds_state_is_refused_as_not_built_yet",
        a_child_under_the_scheduler_and_another_handler_that_holds_state_is_refused_as_not_built_yet,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_runs_the_tasks_of_a_seed_in_the_order_the_evaluator_does",
    every_executable_runs_the_tasks_of_a_seed_in_the_order_the_evaluator_does,
)];

const USES: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\nuse std.task (join, spawn, task, test_scheduler)\n\n";

/// A program in which two children each start a grandchild, join it, and give
/// what it gave and one more, and the parent adds what the two give.
fn nested(seed: u64, n: u64, m: u64) -> String {
    format!(
        "{USES}fn child(n: u64) -> u64 ! task {{\n  g = spawn {{ wrapping_mul(n, 3) }}\n  v = join(g)\n  wrapping_add(v, 1)\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler({seed})\n  a = spawn {{ child({n}) }}\n  b = spawn {{ child({m}) }}\n  wrapping_add(join(a), join(b))\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

/// A program in which two children say which they are, and the parent joins both.
fn two_say(seed: u64) -> String {
    format!(
        "{USES}fn run() -> u64 ! console, alloc, diverge {{\n  with test_scheduler({seed})\n  a = spawn {{\n    print(\"a\")\n    v: u64 = 1\n    v\n  }}\n  b = spawn {{\n    print(\"b\")\n    v: u64 = 2\n    v\n  }}\n  wrapping_add(join(a), join(b))\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

fn run(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(scalar::check(&program), Ok(()), "for {source}");
    let outcome = scalar::run(&program);
    assert_eq!(outcome.status, 0, "{} for {source}", String::from_utf8_lossy(&outcome.stdout));
    let passed = scalar::run(&scalar::passes(&program));
    assert_eq!(outcome.stdout, passed.stdout, "for {source}");
    String::from_utf8(outcome.stdout).expect("text")
}

fn refused(source: &str) -> String {
    match lower(source.as_bytes()) {
        Ok(_) => panic!("a program that is refused was built: {source}"),
        Err(errors) => format!("{errors:?}"),
    }
}

/// The parent's lists of tasks are changed by a child's `spawn`, which the
/// scheduler sees after the poll that ran the child: if a frame saved the state
/// it was made with, the grandchild would never be in the scheduler's list and
/// the tasks would wait for it for ever, which ends the program with status 1.
fn a_child_that_starts_a_grandchild_and_joins_it_gives_the_same_value_for_every_seed() {
    generate::check(
        400,
        40,
        |g| (g.below(1000) as u64, g.below(1000) as u64, g.below(1 << 20) as u64),
        |&(n, m, seed)| {
            let said = run(&nested(seed, n, m));
            assert_eq!(said, format!("{}\n", n * 3 + 1 + m * 3 + 1), "seed {seed}");
        },
    );
}

fn one_seed_gives_one_order_and_some_seed_shows_each_order_of_two_tasks() {
    let mut orders: BTreeSet<String> = BTreeSet::new();
    for seed in 0..32u64 {
        let first = run(&two_say(seed));
        assert_eq!(first, run(&two_say(seed)), "seed {seed}");
        orders.insert(first);
    }
    let expected: BTreeSet<String> = ["a\nb\n3\n", "b\na\n3\n"].iter().map(|s| s.to_string()).collect();
    assert_eq!(orders, expected);
}

/// A child that joins a grandchild says so before and after, and the grandchild
/// says its own line between: whatever the seed, the line of the grandchild is
/// after the first of the child and before the second.
fn a_task_that_waits_for_another_runs_again_after_that_one_has_ended() {
    for seed in 0..24u64 {
        let source = format!(
            "{USES}fn child() -> u64 ! console, task {{\n  print(\"before\")\n  g = spawn {{\n    print(\"inside\")\n    v: u64 = 7\n    v\n  }}\n  v = join(g)\n  print(\"after\")\n  v\n}}\n\nfn run() -> u64 ! console, alloc, diverge {{\n  with test_scheduler({seed})\n  a = spawn {{ child() }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
        );
        assert_eq!(run(&source), "before\ninside\nafter\n7\n", "seed {seed}");
    }
}

fn a_child_under_the_scheduler_and_another_handler_that_holds_state_is_refused_as_not_built_yet() {
    let source = format!(
        "{USES}effect clock {{\n  fn now() -> u64\n}}\n\nfn fake_clock[T](body: fn() -> T ! clock) -> T {{\n  var ticks: u64 = 0\n  clock(\n    now: {{\n      ticks = wrapping_add(ticks, 1)\n      ticks\n    }},\n    body: body,\n  )\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler(1)\n  with fake_clock()\n  a = spawn {{ now() }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    );
    let errors = refused(&source);
    assert!(errors.contains("under a handler that holds state"), "{errors}");
}

fn every_executable_runs_the_tasks_of_a_seed_in_the_order_the_evaluator_does() {
    let targets = runnable();
    for seed in [0u64, 1, 2, 5, 11] {
        for source in [two_say(seed), nested(seed, 4, 9)] {
            let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
            let said = scalar::run(&scalar::passes(&program));
            assert_eq!(said.status, 0);
            for &target in &targets {
                runs_as(source.as_bytes(), target, &said);
            }
        }
    }
}
