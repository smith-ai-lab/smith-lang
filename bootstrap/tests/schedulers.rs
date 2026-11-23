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
        "what_a_task_holds_across_a_wait_lives_until_it_ends_while_other_tasks_take_and_release_memory",
        what_a_task_holds_across_a_wait_lives_until_it_ends_while_other_tasks_take_and_release_memory,
    ),
    (
        "a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed",
        a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed,
    ),
    (
        "a_block_that_assigns_a_var_of_its_function_gives_it_back_to_the_function",
        a_block_that_assigns_a_var_of_its_function_gives_it_back_to_the_function,
    ),
    (
        "a_task_that_may_wait_in_a_loop_and_does_not_gives_what_it_holds_and_the_scheduler_its_state",
        a_task_that_may_wait_in_a_loop_and_does_not_gives_what_it_holds_and_the_scheduler_its_state,
    ),
    (
        "tasks_that_wait_for_handles_run_in_the_order_the_test_makes_the_handles_ready_for_every_seed",
        tasks_that_wait_for_handles_run_in_the_order_the_test_makes_the_handles_ready_for_every_seed,
    ),
    (
        "a_function_that_carries_only_the_effect_of_a_handler_that_holds_state_and_loops_runs_under_a_scheduler_of_a_test",
        a_function_that_carries_only_the_effect_of_a_handler_that_holds_state_and_loops_runs_under_a_scheduler_of_a_test,
    ),
    (
        "a_handler_that_holds_state_installed_under_the_scheduler_of_a_child_is_refused_as_not_built_yet",
        a_handler_that_holds_state_installed_under_the_scheduler_of_a_child_is_refused_as_not_built_yet,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "a_recursion_a_thousand_deep_with_a_wait_at_each_level_runs_as_an_executable_under_the_group_as_the_evaluator_does_to_twenty_five",
        a_recursion_a_thousand_deep_with_a_wait_at_each_level_runs_as_an_executable_under_the_group_as_the_evaluator_does_to_twenty_five,
    ),
    (
        "every_executable_runs_the_tasks_of_a_seed_in_the_order_the_evaluator_does",
        every_executable_runs_the_tasks_of_a_seed_in_the_order_the_evaluator_does,
    ),
    (
        "every_executable_keeps_what_a_task_holds_across_a_wait_and_goes_twenty_five_levels_down",
        every_executable_keeps_what_a_task_holds_across_a_wait_and_goes_twenty_five_levels_down,
    ),
];

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
        16,
        |g| (g.below(1000) as u64, g.below(1000) as u64, g.below(1 << 20) as u64),
        |&(n, m, seed)| {
            let said = run(&nested(seed, n, m));
            assert_eq!(said, format!("{}\n", n * 3 + 1 + m * 3 + 1), "seed {seed}");
        },
    );
}

fn one_seed_gives_one_order_and_some_seed_shows_each_order_of_two_tasks() {
    let mut orders: BTreeSet<String> = BTreeSet::new();
    for seed in 0..10u64 {
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

/// A function that carries the effect of a handler and not the scheduler's
/// is built under that handler alone, where its loop takes the state of that
/// handler, which is no second scheduler's, and gives what it does under any group.
fn a_function_that_carries_only_the_effect_of_a_handler_that_holds_state_and_loops_runs_under_a_scheduler_of_a_test() {
    for seed in [0u64, 1, 2, 5] {
        let source = format!(
            "{USES}effect clock {{\n  fn now() -> u64\n}}\n\nfn fake_clock[T](body: fn() -> T ! clock) -> T {{\n  var ticks: u64 = 0\n  clock(\n    now: {{\n      ticks = wrapping_add(ticks, 1)\n      ticks\n    }},\n    body: body,\n  )\n}}\n\nfn many() -> u64 ! clock {{\n  var last: u64 = 0\n  loop i in 0..20 {{ last = now() }}\n  last\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with fake_clock()\n  with test_scheduler({seed})\n  a = spawn {{ many() }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
        );
        assert_eq!(run(&source), "20\n", "for seed {seed}");
    }
}

fn a_handler_that_holds_state_installed_under_the_scheduler_of_a_child_is_refused_as_not_built_yet() {
    let source = format!(
        "{USES}effect clock {{\n  fn now() -> u64\n}}\n\nfn fake_clock[T](body: fn() -> T ! clock) -> T {{\n  var ticks: u64 = 0\n  clock(\n    now: {{\n      ticks = wrapping_add(ticks, 1)\n      ticks\n    }},\n    body: body,\n  )\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler(1)\n  with fake_clock()\n  a = spawn {{ now() }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    );
    let errors = refused(&source);
    assert!(errors.contains("installed under its scheduler"), "{errors}");
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

/// Functions that take memory, which tasks run between the waits of one that holds
/// a list, a list it pushes to on both sides of a wait, and an arena around one.
const MEMORY: &str = "use std.alloc (arena)\nuse std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\nuse std.list (push)\nuse std.task (join, spawn, task, test_scheduler)\n\nfn build(n: u64, k: u64) -> List[u64] ! alloc {\n  var acc: List[u64] = []\n  loop i in 0..n { acc = push(acc, wrapping_add(wrapping_mul(i, i), k)) }\n  acc\n}\n\nfn total(xs: List[u64]) -> u64 {\n  var s: u64 = 0\n  loop i in 0..xs.size { s = wrapping_add(s, xs[i]) }\n  s\n}\n\nfn noise(k: u64) -> u64 ! alloc {\n  total(build(300, k))\n}\n\nfn holder(n: u64) -> u64 ! task, alloc {\n  xs = build(40, n)\n  var ys: List[u64] = build(10, n)\n  a = spawn { noise(1) }\n  b = spawn { noise(2) }\n  c = spawn { noise(3) }\n  ys = push(ys, 1000)\n  x = join(a)\n  y = join(b)\n  ys = push(ys, wrapping_add(x, y))\n  z = join(c)\n  wrapping_add(total(xs), wrapping_add(total(ys), z))\n}\n\nfn inside(n: u64) -> u64 ! task {\n  with arena()\n  xs = build(n, 7)\n  g = spawn { noise(5) }\n  v = join(g)\n  wrapping_add(total(xs), v)\n}\n\nfn run(seed: u64) -> u64 ! alloc, diverge {\n  with test_scheduler(seed)\n  a = spawn { holder(5) }\n  b = spawn { holder(9) }\n  c = spawn { inside(20) }\n  d = spawn { inside(25) }\n  wrapping_add(join(a), wrapping_add(join(b), wrapping_add(join(c), join(d))))\n}\n\nfn main() ! console, alloc, diverge {\n  print(\"{run(SEED)}\")\n}\n";

fn memory_answer() -> u64 {
    let t = |n: u64, k: u64| (0..n).map(|i| i * i + k).sum::<u64>();
    let holder = |n: u64| t(40, n) + t(10, n) + 1000 + (t(300, 1) + t(300, 2)) + t(300, 3);
    let inside = |n: u64| t(n, 7) + t(300, 5);
    holder(5) + holder(9) + inside(20) + inside(25)
}

fn what_a_task_holds_across_a_wait_lives_until_it_ends_while_other_tasks_take_and_release_memory() {
    for seed in 0..12u64 {
        let said = run(&MEMORY.replace("SEED", &seed.to_string()));
        assert_eq!(said, format!("{}\n", memory_answer()), "seed {seed}");
    }
}

/// `down` goes twenty-five levels, and at each spawns a task and waits for it.
const DEEP: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\nuse std.list (freeze, push)\nuse std.task (join, spawn, task, test_scheduler)\n\nfn build(n: u64) -> List[u64] ! alloc {\n  var acc: List[u64] = []\n  loop i in 0..n { acc = push(acc, i) }\n  acc\n}\n\nfn down(xs: Slice[u64], at: Nat) -> u64 ! task, alloc\n  decreases xs.size - at\n{\n  if at < xs.size {\n    first = xs[at]\n    g = spawn { wrapping_add(first, 1) }\n    v = join(g)\n    wrapping_add(v, down(xs, at + 1))\n  } else {\n    0\n  }\n}\n\nfn run(seed: u64) -> u64 ! alloc, diverge {\n  with test_scheduler(seed)\n  a = spawn { down(freeze(build(25)), 0) }\n  join(a)\n}\n\nfn main() ! console, alloc, diverge {\n  print(\"{run(SEED)}\")\n}\n";

fn a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed() {
    for seed in 0..6u64 {
        let said = run(&DEEP.replace("SEED", &seed.to_string()));
        assert_eq!(said, "325\n", "seed {seed}");
    }
}

/// A block under a scheduler is a task of its own, which gives back what it
/// assigns of the function around it, so the function goes on with it: a `var`
/// assigned in the block, in a loop, holds what the block gave it each round,
/// whichever seed orders the tasks.
fn a_block_that_assigns_a_var_of_its_function_gives_it_back_to_the_function() {
    for seed in 0..6u64 {
        let source = format!(
            "{USES}fn run() -> u64 ! alloc, diverge {{\n  var n: u64 = 1\n  loop i in 0..3 {{\n    with test_scheduler({seed})\n    a = spawn {{ wrapping_mul(n, 2) }}\n    n = wrapping_add(n, join(a))\n  }}\n  n\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
        );
        // n is 1, then 1 + 2, then 3 + 6, then 9 + 18.
        assert_eq!(run(&source), "27\n", "seed {seed}");
    }
}

fn every_executable_keeps_what_a_task_holds_across_a_wait_and_goes_twenty_five_levels_down() {
    let targets = runnable();
    for source in [
        MEMORY.replace("SEED", "3"),
        MEMORY.replace("SEED", "11"),
        DEEP.replace("SEED", "2"),
        // The same under the group of `tasks()`, which waits as the scheduler of a
        // program does and runs one task after another.
        MEMORY
            .replace("test_scheduler)", "tasks)")
            .replace("fn run(seed: u64) -> u64 ! alloc, diverge {\n  with test_scheduler(seed)", "fn run(seed: u64) -> u64 ! alloc {\n  with tasks()")
            .replace("! console, alloc, diverge", "! console, alloc")
            .replace("SEED", "3"),
    ] {
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let said = scalar::run(&scalar::passes(&program));
        assert_eq!(said.status, 0);
        for &target in &targets {
            runs_as(source.as_bytes(), target, &said);
        }
    }
}

/// A task whose loop waits in one branch and leaves in the other, which leaves
/// at its first poll: the scheduler goes on with the state the poll gave.
fn a_task_that_may_wait_in_a_loop_and_does_not_gives_what_it_holds_and_the_scheduler_its_state() {
    let source = |n: u64| {
        format!(
            "{USES}fn probe(n: u64) -> u64 ! task {{\n  var got: u64 = 7\n  loop attempt in 0..10 {{\n    if n == 99 {{\n      g = spawn {{ wrapping_mul(n, 2) }}\n      got = join(g)\n    }} else {{\n      break\n    }}\n  }}\n  got\n}}\n\nfn run(n: u64) -> u64 ! alloc, diverge {{\n  with test_scheduler(3)\n  a = spawn {{ probe(n) }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run({n})}}\")\n}}\n"
        )
    };
    assert_eq!(run(&source(5)), "7\n");
    assert_eq!(run(&source(99)), "198\n");
}

/// Three tasks each wait for a handle of their own and say which they are when
/// it is ready. The test says which handles are ready and in what order, so
/// the order of the three is that, for every seed, and a task that waits for a
/// handle the test never makes ready is waited for by none that is run.
fn a_program_with_handles(ready: &str, seed: u64) -> String {
    format!(
        "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\nuse std.task (join, spawn, task, test_ready, test_scheduler_with_io)\n\nfn say(handle: u64) -> u64 ! task, console, alloc {{\n  test_ready(handle)\n  print(\"{{handle}}\")\n  handle\n}}\n\nfn run() -> u64 ! console, alloc, diverge {{\n  with test_scheduler_with_io({seed}, {ready})\n  a = spawn {{ say(1) }}\n  b = spawn {{ say(2) }}\n  c = spawn {{ say(3) }}\n  wrapping_add(join(a), wrapping_add(join(b), join(c)))\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

fn tasks_that_wait_for_handles_run_in_the_order_the_test_makes_the_handles_ready_for_every_seed() {
    for seed in 0..12u64 {
        assert_eq!(run(&a_program_with_handles("[3, 1, 2]", seed)), "3\n1\n2\n6\n", "seed {seed}");
        assert_eq!(run(&a_program_with_handles("[2, 3, 1]", seed)), "2\n3\n1\n6\n", "seed {seed}");
    }
}

/// A recursion in which each level starts a child of the group, joins it, and
/// calls the level below, so each level waits.
fn down(depth: u64) -> String {
    DEEP.replace("use std.task (join, spawn, task, test_scheduler)", "use std.task (join, spawn, task, tasks)")
        .replace("fn run(seed: u64) -> u64 ! alloc, diverge {\n  with test_scheduler(seed)", "fn run() -> u64 ! alloc {\n  with tasks()")
        .replace("build(25)", &format!("build({depth})"))
        .replace("! console, alloc, diverge", "! console, alloc")
        .replace("run(SEED)", "run()")
}

/// The group is run by the evaluator at twenty-five levels, whose waits are
/// all for tasks, and the executable gives the same, and at a thousand levels
/// gives the sum, which the evaluator is too slow to run.
fn a_recursion_a_thousand_deep_with_a_wait_at_each_level_runs_as_an_executable_under_the_group_as_the_evaluator_does_to_twenty_five() {
    let small = down(25);
    let program = lower(small.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let said = scalar::run(&scalar::passes(&program));
    assert_eq!(said.stdout, b"325\n");
    let big = down(1000);
    let large = lower(big.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(scalar::check(&large), Ok(()));
    for target in runnable() {
        runs_as(small.as_bytes(), target, &said);
        let wanted = scalar::Outcome { status: 0, stdout: b"500500\n".to_vec() };
        runs_as(big.as_bytes(), target, &wanted);
    }
}
