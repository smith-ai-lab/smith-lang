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
        "a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed_part_1_of_3",
        || a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed(0),
    ),
    (
        "a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed_part_2_of_3",
        || a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed(1),
    ),
    (
        "a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed_part_3_of_3",
        || a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed(2),
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
/// what it gave and one more, and the parent adds what the two give, for each of
/// the cases, as one program with a function for each, so that the library of
/// tasks is lowered once.
fn nested_all(cases: &[(u64, u64, u64)]) -> String {
    let mut s = format!("{USES}fn child(n: u64) -> u64 <task> {{\n  g = spawn {{ wrapping_mul(n, 3) }}\n  v = join(take g)\n  wrapping_add(v, 1)\n}}\n\n");
    for (i, &(n, m, seed)) in cases.iter().enumerate() {
        s += &format!("fn run{i}() -> u64 <alloc, diverge> {{\n  with test_scheduler({seed})\n  a = spawn {{ child({n}) }}\n  b = spawn {{ child({m}) }}\n  wrapping_add(join(take a), join(take b))\n}}\n\n");
    }
    s += "fn main() <console, alloc, diverge> {\n";
    for i in 0..cases.len() {
        s += &format!("  print(\"{{run{i}()}}\")\n");
    }
    s + "}\n"
}

/// A function in which two children say which they are, and the parent joins both,
/// for each of the seeds, each run twice, as one program.
fn two_say_all(seeds: std::ops::Range<u64>) -> String {
    let mut s = USES.to_string();
    for seed in seeds.clone() {
        s += &format!("fn run{seed}() -> u64 <console, alloc, diverge> {{\n  with test_scheduler({seed})\n  a = spawn {{\n    print(\"a\")\n    v: u64 = 1\n    v\n  }}\n  b = spawn {{\n    print(\"b\")\n    v: u64 = 2\n    v\n  }}\n  wrapping_add(join(take a), join(take b))\n}}\n\n");
    }
    s += "fn main() <console, alloc, diverge> {\n";
    for seed in seeds {
        s += &format!("  print(\"{{run{seed}()}}\")\n  print(\"{{run{seed}()}}\")\n");
    }
    s + "}\n"
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
        1,
        |g| {
            (0..16)
                .map(|_| (g.below(1000) as u64, g.below(1000) as u64, g.below(1 << 20) as u64))
                .collect::<Vec<_>>()
        },
        |cases| {
            let said = run(&nested_all(cases));
            let lines: Vec<&str> = said.lines().collect();
            assert_eq!(lines.len(), cases.len(), "{said}");
            for (line, &(n, m, seed)) in lines.iter().zip(cases) {
                assert_eq!(*line, (n * 3 + 1 + m * 3 + 1).to_string(), "seed {seed}");
            }
        },
    );
}

fn one_seed_gives_one_order_and_some_seed_shows_each_order_of_two_tasks() {
    let mut orders: BTreeSet<String> = BTreeSet::new();
    let said = run(&two_say_all(0..10));
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 10 * 2 * 3, "{said}");
    for (seed, runs) in lines.chunks(6).enumerate() {
        let (first, second) = (format!("{}\n", runs[..3].join("\n")), format!("{}\n", runs[3..].join("\n")));
        assert_eq!(first, second, "seed {seed}");
        orders.insert(first);
    }
    let expected: BTreeSet<String> = ["a\nb\n3\n", "b\na\n3\n"].iter().map(|s| s.to_string()).collect();
    assert_eq!(orders, expected);
}

/// A child that joins a grandchild says so before and after, and the grandchild
/// says its own line between: whatever the seed, the line of the grandchild is
/// after the first of the child and before the second.
fn a_task_that_waits_for_another_runs_again_after_that_one_has_ended() {
    // The seeds are one program, which is lowered once.
    let mut source = format!(
        "{USES}fn child() -> u64 <console, task> {{\n  print(\"before\")\n  g = spawn {{\n    print(\"inside\")\n    v: u64 = 7\n    v\n  }}\n  v = join(take g)\n  print(\"after\")\n  v\n}}\n\n"
    );
    for seed in 0..24u64 {
        source += &format!("fn run{seed}() -> u64 <console, alloc, diverge> {{\n  with test_scheduler({seed})\n  a = spawn {{ child() }}\n  join(take a)\n}}\n\n");
    }
    source += "fn main() <console, alloc, diverge> {\n";
    for seed in 0..24u64 {
        source += &format!("  print(\"{{run{seed}()}}\")\n");
    }
    source += "}\n";
    let said = run(&source);
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 24 * 4, "{said}");
    for (seed, chunk) in lines.chunks(4).enumerate() {
        assert_eq!(format!("{}\n", chunk.join("\n")), "before\ninside\nafter\n7\n", "seed {seed}");
    }
}

/// A function that carries the effect of a handler and not the scheduler's
/// is built under that handler alone, where its loop takes the state of that
/// handler, which is no second scheduler's, and gives what it does under any group.
fn a_function_that_carries_only_the_effect_of_a_handler_that_holds_state_and_loops_runs_under_a_scheduler_of_a_test() {
    for seed in [0u64, 1, 2, 5] {
        let source = format!(
            "{USES}effect clock {{\n  fn now() -> u64\n}}\n\nfn fake_clock[T](body: fn() -> T <clock>) -> T {{\n  mut ticks: u64 = 0\n  clock(\n    now: {{\n      ticks = wrapping_add(ticks, 1)\n      ticks\n    }},\n    body: body,\n  )\n}}\n\nfn many() -> u64 <clock> {{\n  mut last: u64 = 0\n  loop i in 0..20 {{ last = now() }}\n  last\n}}\n\nfn run() -> u64 <alloc, diverge> {{\n  with fake_clock()\n  with test_scheduler({seed})\n  a = spawn {{ many() }}\n  join(take a)\n}}\n\nfn main() <console, alloc, diverge> {{\n  print(\"{{run()}}\")\n}}\n"
        );
        assert_eq!(run(&source), "20\n", "for seed {seed}");
    }
}

fn a_handler_that_holds_state_installed_under_the_scheduler_of_a_child_is_refused_as_not_built_yet() {
    let source = format!(
        "{USES}effect clock {{\n  fn now() -> u64\n}}\n\nfn fake_clock[T](body: fn() -> T <clock>) -> T {{\n  mut ticks: u64 = 0\n  clock(\n    now: {{\n      ticks = wrapping_add(ticks, 1)\n      ticks\n    }},\n    body: body,\n  )\n}}\n\nfn run() -> u64 <alloc, diverge> {{\n  with test_scheduler(1)\n  with fake_clock()\n  a = spawn {{ now() }}\n  join(take a)\n}}\n\nfn main() <console, alloc, diverge> {{\n  print(\"{{run()}}\")\n}}\n"
    );
    let errors = refused(&source);
    assert!(errors.contains("installed under its scheduler"), "{errors}");
}

/// For each of the seeds, a function in which two children say which they are and
/// the parent joins both, and the function of `nested_all` for the numbers 4 and
/// 9, as one program, so that the library of tasks is lowered and built once.
fn every_seed(seeds: &[u64]) -> String {
    let mut s = format!("{USES}fn child(n: u64) -> u64 <task> {{\n  g = spawn {{ wrapping_mul(n, 3) }}\n  v = join(take g)\n  wrapping_add(v, 1)\n}}\n\n");
    let mut main = String::new();
    for seed in seeds {
        s += &format!("fn say{seed}() -> u64 <console, alloc, diverge> {{\n  with test_scheduler({seed})\n  a = spawn {{\n    print(\"a\")\n    v: u64 = 1\n    v\n  }}\n  b = spawn {{\n    print(\"b\")\n    v: u64 = 2\n    v\n  }}\n  wrapping_add(join(take a), join(take b))\n}}\n\n");
        s += &format!("fn nest{seed}() -> u64 <alloc, diverge> {{\n  with test_scheduler({seed})\n  a = spawn {{ child(4) }}\n  b = spawn {{ child(9) }}\n  wrapping_add(join(take a), join(take b))\n}}\n\n");
        main += &format!("  print(\"{{say{seed}()}}\")\n  print(\"{{nest{seed}()}}\")\n");
    }
    format!("{s}fn main() <console, alloc, diverge> {{\n{main}}}\n")
}

fn every_executable_runs_the_tasks_of_a_seed_in_the_order_the_evaluator_does() {
    let source = every_seed(&[0, 1, 2, 5, 11]);
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let said = scalar::run(&scalar::passes(&program));
    assert_eq!(said.status, 0);
    for target in runnable() {
        runs_as(source.as_bytes(), target, &said);
    }
}

/// Functions that take memory, which tasks run between the waits of one that holds
/// a list, a list it pushes to on both sides of a wait, and an arena around one.
const MEMORY: &str = "use std.alloc (arena)\nuse std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\nuse std.list (push)\nuse std.task (join, spawn, task, test_scheduler)\n\nfn build(n: u64, k: u64) -> List[u64] <alloc> {\n  mut acc: List[u64] = []\n  loop i in 0..n { acc = push(take acc, wrapping_add(wrapping_mul(i, i), k)) }\n  acc\n}\n\nfn total(xs: List[u64]) -> u64 {\n  mut s: u64 = 0\n  loop i in 0..xs.size { s = wrapping_add(s, xs[i]) }\n  s\n}\n\nfn noise(k: u64) -> u64 <alloc> {\n  total(build(300, k))\n}\n\nfn holder(n: u64) -> u64 <task, alloc> {\n  xs = build(40, n)\n  mut ys: List[u64] = build(10, n)\n  a = spawn { noise(1) }\n  b = spawn { noise(2) }\n  c = spawn { noise(3) }\n  ys = push(take ys, 1000)\n  x = join(take a)\n  y = join(take b)\n  ys = push(take ys, wrapping_add(x, y))\n  z = join(take c)\n  wrapping_add(total(xs), wrapping_add(total(ys), z))\n}\n\nfn inside(n: u64) -> u64 <task> {\n  with arena()\n  xs = build(n, 7)\n  g = spawn { noise(5) }\n  v = join(take g)\n  wrapping_add(total(xs), v)\n}\n\nfn run(seed: u64) -> u64 <alloc, diverge> {\n  with test_scheduler(seed)\n  a = spawn { holder(5) }\n  b = spawn { holder(9) }\n  c = spawn { inside(20) }\n  d = spawn { inside(25) }\n  wrapping_add(join(take a), wrapping_add(join(take b), wrapping_add(join(take c), join(take d))))\n}\n\nfn main() <console, alloc, diverge> {\n  print(\"{run(SEED)}\")\n}\n";

fn memory_answer() -> u64 {
    let t = |n: u64, k: u64| (0..n).map(|i| i * i + k).sum::<u64>();
    let holder = |n: u64| t(40, n) + t(10, n) + 1000 + (t(300, 1) + t(300, 2)) + t(300, 3);
    let inside = |n: u64| t(n, 7) + t(300, 5);
    holder(5) + holder(9) + inside(20) + inside(25)
}

fn what_a_task_holds_across_a_wait_lives_until_it_ends_while_other_tasks_take_and_release_memory() {
    // The seeds are one program, which is lowered once.
    let calls: String = (0..5u64).map(|seed| format!("  print(\"{{run({seed})}}\")\n")).collect();
    let said = run(&MEMORY.replace("  print(\"{run(SEED)}\")\n", &calls));
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 5, "{said}");
    for (seed, line) in lines.iter().enumerate() {
        assert_eq!(*line, memory_answer().to_string(), "seed {seed}");
    }
}

/// `down` goes twenty-five levels, and at each spawns a task and waits for it.
const DEEP: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\nuse std.list (freeze, push)\nuse std.task (join, spawn, task, test_scheduler)\n\nfn build(n: u64) -> List[u64] <alloc> {\n  mut acc: List[u64] = []\n  loop i in 0..n { acc = push(take acc, i) }\n  acc\n}\n\nfn down(xs: Slice[u64], at: Nat) -> u64 <task, alloc>\n  decreases xs.size - at\n{\n  if at < xs.size {\n    first = xs[at]\n    g = spawn { wrapping_add(first, 1) }\n    v = join(take g)\n    wrapping_add(v, down(xs, at + 1))\n  } else {\n    0\n  }\n}\n\nfn run(seed: u64) -> u64 <alloc, diverge> {\n  with test_scheduler(seed)\n  a = spawn { down(freeze(build(25)), 0) }\n  join(take a)\n}\n\nfn main() <console, alloc, diverge> {\n  print(\"{run(SEED)}\")\n}\n";

fn a_recursion_twenty_five_deep_with_a_wait_at_each_level_gives_the_same_sum_for_every_seed(part: u64) {
    for seed in 2 * part..2 * part + 2 {
        let said = run(&DEEP.replace("SEED", &seed.to_string()));
        assert_eq!(said, "325\n", "seed {seed}");
    }
}

/// A block under a scheduler is a task of its own, which gives back what it
/// assigns of the function around it, so the function goes on with it: a `mut`
/// assigned in the block, in a loop, holds what the block gave it each round,
/// whichever seed orders the tasks.
fn a_block_that_assigns_a_var_of_its_function_gives_it_back_to_the_function() {
    for seed in 0..6u64 {
        let source = format!(
            "{USES}fn run() -> u64 <alloc, diverge> {{\n  mut n: u64 = 1\n  loop i in 0..3 {{\n    with test_scheduler({seed})\n    a = spawn {{ wrapping_mul(n, 2) }}\n    n = wrapping_add(n, join(take a))\n  }}\n  n\n}}\n\nfn main() <console, alloc, diverge> {{\n  print(\"{{run()}}\")\n}}\n"
        );
        // n is 1, then 1 + 2, then 3 + 6, then 9 + 18.
        assert_eq!(run(&source), "27\n", "seed {seed}");
    }
}

/// The program of `MEMORY` run under the scheduler of a test of each of two seeds
/// and under the group of `tasks()`, which waits as the scheduler of a program
/// does and runs one task after another, and the program of `DEEP`, as the
/// functions of one program, so that the library of tasks is lowered and built
/// once, and the executable prints the four numbers the evaluator prints.
fn holding_and_deep() -> String {
    let (memory, main) = MEMORY.split_once("fn main()").expect("a main");
    assert!(main.contains("run(SEED)"));
    let memory = memory
        .replace("use std.list (push)", "use std.list (freeze, push)")
        .replace("use std.task (join, spawn, task, test_scheduler)", "use std.task (join, spawn, task, tasks, test_scheduler)");
    let (_, run) = memory.split_once("fn run(seed: u64)").expect("a run");
    let under_the_group = format!("fn run(seed: u64){run}")
        .replace("fn run(seed: u64) -> u64 <alloc, diverge> {\n  with test_scheduler(seed)", "fn run_tasks(seed: u64) -> u64 <alloc> {\n  with tasks()");
    let (_, deep) = DEEP.split_once("fn build(n: u64)").expect("a build");
    let (deep, _) = deep.split_once("fn main()").expect("a main");
    let deep = format!("fn build(n: u64){deep}")
        .replace("fn build(n: u64)", "fn build_up(n: u64)")
        .replace("build(25)", "build_up(25)")
        .replace("fn run(seed: u64)", "fn deep(seed: u64)");
    format!(
        "{memory}{under_the_group}{deep}fn main() <console, alloc, diverge> {{\n  print(\"{{run(3)}}\")\n  print(\"{{run(11)}}\")\n  print(\"{{deep(2)}}\")\n  print(\"{{run_tasks(3)}}\")\n}}\n"
    )
}

fn every_executable_keeps_what_a_task_holds_across_a_wait_and_goes_twenty_five_levels_down() {
    let source = holding_and_deep();
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let said = scalar::run(&scalar::passes(&program));
    assert_eq!(said.status, 0);
    assert_eq!(String::from_utf8_lossy(&said.stdout), format!("{0}\n{0}\n325\n{0}\n", memory_answer()));
    for target in runnable() {
        runs_as(source.as_bytes(), target, &said);
    }
}

/// A task whose loop waits in one branch and leaves in the other, which leaves
/// at its first poll: the scheduler goes on with the state the poll gave.
fn a_task_that_may_wait_in_a_loop_and_does_not_gives_what_it_holds_and_the_scheduler_its_state() {
    let source = |n: u64| {
        format!(
            "{USES}fn probe(n: u64) -> u64 <task> {{\n  mut got: u64 = 7\n  loop attempt in 0..10 {{\n    if n == 99 {{\n      g = spawn {{ wrapping_mul(n, 2) }}\n      got = join(take g)\n    }} else {{\n      break\n    }}\n  }}\n  got\n}}\n\nfn run(n: u64) -> u64 <alloc, diverge> {{\n  with test_scheduler(3)\n  a = spawn {{ probe(n) }}\n  join(take a)\n}}\n\nfn main() <console, alloc, diverge> {{\n  print(\"{{run({n})}}\")\n}}\n"
        )
    };
    assert_eq!(run(&source(5)), "7\n");
    assert_eq!(run(&source(99)), "198\n");
}

/// Three tasks each wait for a handle of their own and say which they are when
/// it is ready. The test says which handles are ready and in what order, so
/// the order of the three is that, for every seed, and a task that waits for a
/// handle the test never makes ready is waited for by none that is run.
fn a_program_with_handles(cases: &[(&str, u64)]) -> String {
    let mut s = String::from("use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\nuse std.task (Descriptor, join, spawn, task, test_ready, test_scheduler_with_io)\n\nfn say(handle: Descriptor) -> u64 <task, console, alloc> {\n  test_ready(handle)\n  print(\"{handle}\")\n  handle\n}\n\n");
    for (i, (ready, seed)) in cases.iter().enumerate() {
        s += &format!("fn run{i}() -> u64 <console, alloc, diverge> {{\n  with test_scheduler_with_io({seed}, {ready})\n  a = spawn {{ say(1) }}\n  b = spawn {{ say(2) }}\n  c = spawn {{ say(3) }}\n  wrapping_add(join(take a), wrapping_add(join(take b), join(take c)))\n}}\n\n");
    }
    s += "fn main() <console, alloc, diverge> {\n";
    for i in 0..cases.len() {
        s += &format!("  print(\"{{run{i}()}}\")\n");
    }
    s + "}\n"
}

/// The twelve seeds and the two orders are run as one program, which is lowered
/// once, as lowering the library of tasks takes the time of a case.
fn tasks_that_wait_for_handles_run_in_the_order_the_test_makes_the_handles_ready_for_every_seed() {
    let mut cases = Vec::new();
    let mut wanted = Vec::new();
    for seed in 0..12u64 {
        cases.push(("[3, 1, 2]", seed));
        wanted.push("3\n1\n2\n6\n");
        cases.push(("[2, 3, 1]", seed));
        wanted.push("2\n3\n1\n6\n");
    }
    let said = run(&a_program_with_handles(&cases));
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 4 * cases.len(), "{said}");
    for (i, chunk) in lines.chunks(4).enumerate() {
        assert_eq!(format!("{}\n", chunk.join("\n")), wanted[i], "ready {} seed {}", cases[i].0, cases[i].1);
    }
}

/// A recursion in which each level starts a child of the group, joins it, and
/// calls the level below, so each level waits.
fn down(depth: u64) -> String {
    DEEP.replace("use std.task (join, spawn, task, test_scheduler)", "use std.task (join, spawn, task, tasks)")
        .replace("fn run(seed: u64) -> u64 <alloc, diverge> {\n  with test_scheduler(seed)", "fn run() -> u64 <alloc> {\n  with tasks()")
        .replace("build(25)", &format!("build({depth})"))
        .replace("<console, alloc, diverge>", "<console, alloc>")
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
