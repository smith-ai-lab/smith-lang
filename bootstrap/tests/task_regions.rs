//! The memory of a task is regions of its own, taken from the pool: a task that
//! waits with nothing built holds a few KB, a task that waits inside an arena in
//! a loop asks the system for the same memory after 20 rounds as after 2,000,
//! two tasks that each wait inside their own arena never see each other's bytes
//! change, and a task whose result holds no address gives its memory back to the
//! pool when it is joined.

use smith::lower;
use smith::scalar;

use crate::Test;
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_thousand_tasks_that_wait_with_nothing_built_hold_under_eight_megabytes",
        a_thousand_tasks_that_wait_with_nothing_built_hold_under_eight_megabytes,
    ),
    (
        "a_task_that_waits_inside_an_arena_in_a_loop_asks_the_system_for_the_same_memory_after_20_rounds_as_after_2000",
        a_task_that_waits_inside_an_arena_in_a_loop_asks_the_system_for_the_same_memory_after_20_rounds_as_after_2000,
    ),
    (
        "two_tasks_that_each_wait_inside_their_own_arena_never_see_each_others_bytes_change",
        two_tasks_that_each_wait_inside_their_own_arena_never_see_each_others_bytes_change,
    ),
    (
        "ten_thousand_children_that_give_a_list_joined_one_after_another_ask_the_system_for_the_memory_of_one",
        ten_thousand_children_that_give_a_list_joined_one_after_another_ask_the_system_for_the_memory_of_one,
    ),
    (
        "a_list_of_a_million_bytes_is_joined_in_the_instructions_a_list_of_ten_is",
        a_list_of_a_million_bytes_is_joined_in_the_instructions_a_list_of_ten_is,
    ),
    (
        "a_result_read_after_the_parent_asks_for_more_memory_is_the_same_bytes",
        a_result_read_after_the_parent_asks_for_more_memory_is_the_same_bytes,
    ),
    (
        "a_list_a_function_gives_up_to_main_is_read_there_after_more_calls_as_it_was_built",
        a_list_a_function_gives_up_to_main_is_read_there_after_more_calls_as_it_was_built,
    ),
    (
        "a_program_that_starts_no_task_has_no_pool_and_its_take_is_the_one_it_had",
        a_program_that_starts_no_task_has_no_pool_and_its_take_is_the_one_it_had,
    ),
    (
        "a_child_started_where_a_joined_one_was_is_a_new_task_with_a_value_of_its_own",
        a_child_started_where_a_joined_one_was_is_a_new_task_with_a_value_of_its_own,
    ),
    (
        "a_child_left_running_when_the_arena_that_started_it_ends_runs_on_and_the_scheduler_is_whole",
        a_child_left_running_when_the_arena_that_started_it_ends_runs_on_and_the_scheduler_is_whole,
    ),
    (
        "ten_thousand_children_joined_one_after_another_ask_the_system_for_the_memory_of_one",
        ten_thousand_children_joined_one_after_another_ask_the_system_for_the_memory_of_one,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_keeps_the_regions_of_tasks_apart_and_gives_the_memory_of_a_joined_task_back",
    every_executable_keeps_the_regions_of_tasks_apart_and_gives_the_memory_of_a_joined_task_back,
)];

const USES: &str = "use std.alloc (arena)\nuse std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\nuse std.list (freeze, push)\nuse std.task (join, spawn, task, test_scheduler)\n\n";

/// Runs the program, and gives what it printed and how many bytes the system was
/// asked for in all.
fn asked(source: &str) -> (String, usize) {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(scalar::check(&program), Ok(()), "for {source}");
    let (outcome, requests) = scalar::run_counting(&scalar::passes(&program), usize::MAX);
    assert_eq!(outcome.status, 0, "{} for {source}", String::from_utf8_lossy(&outcome.stdout));
    (String::from_utf8(outcome.stdout).expect("text"), requests.iter().sum())
}

/// A recursion that starts a child at each level and joins them on the way back,
/// so that all of them exist at once, each holding the region it was given.
fn many(count: u64) -> String {
    format!(
        "{USES}fn build(n: u64) -> List[u64] ! alloc {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, i) }}\n  acc\n}}\n\nfn down(xs: Slice[u64], at: Nat) -> u64 ! task, alloc\n  decreases xs.size - at\n{{\n  if at < xs.size {{\n    first = xs[at]\n    g = spawn {{ wrapping_add(first, 1) }}\n    v = down(xs, at + 1)\n    wrapping_add(join(g), v)\n  }} else {{\n    0\n  }}\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler(3)\n  a = spawn {{ down(freeze(build({count})), 0) }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

/// The scheduler of a test looks through its tasks at each poll, which the
/// evaluator cannot do a thousand times over in the time a test has, so the
/// evaluator runs 100 and 200 tasks and holds each of the hundred more to what
/// 1,000 may take of 8 MB, which is 8 KB, apart from the memory the system gives
/// in chunks, and the executables run 1,000.
fn a_thousand_tasks_that_wait_with_nothing_built_hold_under_eight_megabytes() {
    let (few_said, few) = asked(&many(100));
    let (many_said, more) = asked(&many(200));
    assert_eq!(few_said, "5050\n");
    assert_eq!(many_said, "20100\n");
    let each = (more.saturating_sub(few)) as u64 / 100;
    assert!(each <= 8 * 1024 * 1024 / 1000, "{each} bytes for each task: {few}, {more}");
}

/// A task that waits inside an arena in each round of a loop.
fn arena_rounds(rounds: u64) -> String {
    format!(
        "{USES}fn build(n: u64) -> List[u64] ! alloc {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, i) }}\n  acc\n}}\n\nfn total(xs: List[u64]) -> u64 {{\n  var s: u64 = 0\n  loop i in 0..xs.size {{ s = wrapping_add(s, xs[i]) }}\n  s\n}}\n\nfn rounds(n: u64) -> u64 ! task, alloc {{\n  var sum: u64 = 0\n  loop r in 0..n {{\n    with arena()\n    xs = build(200)\n    g = spawn {{ wrapping_add(r, 1) }}\n    sum = wrapping_add(sum, wrapping_add(total(xs), join(g)))\n  }}\n  sum\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler(5)\n  a = spawn {{ rounds({rounds}) }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

fn a_task_that_waits_inside_an_arena_in_a_loop_asks_the_system_for_the_same_memory_after_20_rounds_as_after_2000() {
    let (few, few_bytes) = asked(&arena_rounds(20));
    let (many, many_bytes) = asked(&arena_rounds(2000));
    let each = (0..200u64).sum::<u64>();
    let wanted = |n: u64| (0..n).map(|r| each + r + 1).sum::<u64>();
    assert_eq!(few, format!("{}\n", wanted(20)));
    assert_eq!(many, format!("{}\n", wanted(2000)));
    assert_eq!(few_bytes, many_bytes);
}

/// Two tasks that each build a list inside an arena, wait, and add up the list
/// they built after the wait, in an order the seed picks.
fn two_in_arenas(seed: u64) -> String {
    format!(
        "{USES}fn build(n: u64, k: u64) -> List[u64] ! alloc {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, wrapping_add(i, k)) }}\n  acc\n}}\n\nfn total(xs: List[u64]) -> u64 {{\n  var s: u64 = 0\n  loop i in 0..xs.size {{ s = wrapping_add(s, xs[i]) }}\n  s\n}}\n\nfn mine(n: u64, k: u64) -> u64 ! task, alloc {{\n  var sum: u64 = 0\n  rounds: u64 = 5\n  loop r in 0..rounds {{\n    with arena()\n    xs = build(n, k)\n    g = spawn {{ wrapping_add(r, 1) }}\n    v = join(g)\n    sum = wrapping_add(sum, wrapping_add(total(xs), v))\n  }}\n  sum\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler({seed})\n  a = spawn {{ mine(50, 1000) }}\n  b = spawn {{ mine(80, 7) }}\n  wrapping_add(join(a), join(b))\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

fn two_in_arenas_answer() -> u64 {
    let mine = |n: u64, k: u64| (0..5u64).map(|r| (0..n).map(|i| i + k).sum::<u64>() + r + 1).sum::<u64>();
    mine(50, 1000) + mine(80, 7)
}

fn two_tasks_that_each_wait_inside_their_own_arena_never_see_each_others_bytes_change() {
    for seed in 0..12u64 {
        let (said, _) = asked(&two_in_arenas(seed));
        assert_eq!(said, format!("{}\n", two_in_arenas_answer()), "seed {seed}");
    }
}

/// Children that each build a list and give back its sum, spawned and joined one
/// after another.
fn one_after_another(count: u64) -> String {
    format!(
        "{USES}fn build(n: u64) -> List[u64] ! alloc {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, i) }}\n  acc\n}}\n\nfn total(xs: List[u64]) -> u64 {{\n  var s: u64 = 0\n  loop i in 0..xs.size {{ s = wrapping_add(s, xs[i]) }}\n  s\n}}\n\nfn child() -> u64 ! alloc, diverge {{\n  with test_scheduler(1)\n  g = spawn {{ total(build(100)) }}\n  join(g)\n}}\n\nfn main() ! console, alloc, diverge {{\n  var sum: u64 = 0\n  loop j in 0..{count} {{\n    sum = wrapping_add(sum, child())\n  }}\n  print(\"{{sum}}\")\n}}\n"
    )
}

fn ten_thousand_children_joined_one_after_another_ask_the_system_for_the_memory_of_one() {
    // The evaluator runs 10 and 300, which ask for the same, and the executables
    // run 10,000.
    let (few, few_bytes) = asked(&one_after_another(10));
    let (many, many_bytes) = asked(&one_after_another(300));
    assert_eq!(few, format!("{}\n", 4950 * 10));
    assert_eq!(many, format!("{}\n", 4950 * 300));
    assert_eq!(few_bytes, many_bytes);
}

fn every_executable_keeps_the_regions_of_tasks_apart_and_gives_the_memory_of_a_joined_task_back() {
    let targets = runnable();
    for source in [
        many(200),
        arena_rounds(300),
        two_in_arenas(4),
        one_after_another(500),
        lists_one_after_another(200),
        list_up_to_main(),
    ] {
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let said = scalar::run(&scalar::passes(&program));
        assert_eq!(said.status, 0);
        for &target in &targets {
            runs_as(source.as_bytes(), target, &said);
        }
    }
    // The executables run what the evaluator has not the time for: 1,000 tasks
    // that all exist at once, and 10,000 children one after another.
    let big = [
        (many(1000), "500500\n".to_string()),
        (one_after_another(10_000), format!("{}\n", 4950u64 * 10_000)),
        (lists_one_after_another(10_000), format!("{}\n", 4950u64 * 10_000)),
    ];
    for (source, wanted) in big {
        let said = scalar::Outcome {
            status: 0,
            stdout: wanted.into_bytes(),
        };
        for &target in &targets {
            runs_as(source.as_bytes(), target, &said);
        }
    }
}

/// Children that are started one after another, each joined before the next, are
/// where the one before was, and each gives a value of its own: a scheduler that
/// took the one that is there for the one that was would give the first value
/// again, as the scheduler that numbered a task by where its frame is did.
fn one_after_another_with_values() -> String {
    format!(
        "{USES}fn run() -> u64 ! alloc, diverge {{\n  with test_scheduler(2)\n  var sum: u64 = 0\n  var round: u64 = 0\n  loop r in 0..9 {{\n    g = spawn {{ wrapping_mul(r, r) }}\n    sum = wrapping_add(wrapping_mul(sum, 10), join(g))\n    round = r\n  }}\n  wrapping_add(sum, round)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

fn a_child_started_where_a_joined_one_was_is_a_new_task_with_a_value_of_its_own() {
    // Each square is one digit or two, so the sum is written in the order they
    // are joined: 0, 1, 4, 9, 16, 25, 36, 49, 64, with each step a tenfold.
    let mut wanted = 0u64;
    for r in 0..9u64 {
        wanted = wanted * 10 + r * r;
    }
    let (said, _) = asked(&one_after_another_with_values());
    assert_eq!(said, format!("{}\n", wanted + 8));
}

/// A task that starts a child inside an arena, and leaves the arena without
/// waiting for it, goes on to wait for another, which is when the child runs,
/// after the arena it was started in has ended.
fn left_running(seed: u64) -> String {
    format!(
        "{USES}fn build(n: u64) -> List[u64] ! alloc {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, i) }}\n  acc\n}}\n\nfn total(xs: List[u64]) -> u64 {{\n  var s: u64 = 0\n  loop i in 0..xs.size {{ s = wrapping_add(s, xs[i]) }}\n  s\n}}\n\nfn leave() ! task, alloc, console {{\n  with arena()\n  xs = build(100)\n  spawn {{\n    print(\"sibling\")\n    total(build(50))\n  }}\n  spawn {{\n    print(\"other\")\n    total(build(60))\n  }}\n  print(\"{{total(xs)}}\")\n}}\n\nfn parent() -> u64 ! task, alloc, console {{\n  leave()\n  a = spawn {{ total(build(10)) }}\n  b = spawn {{ total(build(20)) }}\n  wrapping_add(join(a), join(b))\n}}\n\nfn run() -> u64 ! alloc, console, diverge {{\n  with test_scheduler({seed})\n  p = spawn {{ parent() }}\n  join(p)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

fn a_child_left_running_when_the_arena_that_started_it_ends_runs_on_and_the_scheduler_is_whole() {
    for seed in 0..16u64 {
        let (said, _) = asked(&left_running(seed));
        let lines: Vec<&str> = said.lines().collect();
        // The arena's total is printed first, in the task that started the
        // children, and the answer last. The children that were left run once
        // or not at all, since a child that is not joined is cancelled when
        // its group ends, and none of them runs twice or gives a wrong answer.
        assert_eq!(lines.first(), Some(&"4950"), "seed {seed}: {said}");
        assert_eq!(lines.last(), Some(&"235"), "seed {seed}: {said}");
        for name in ["sibling", "other"] {
            let seen = lines.iter().filter(|l| **l == name).count();
            assert!(seen <= 1, "{name} ran {seen} times for seed {seed}: {said}");
        }
    }
}

/// Every case of `tests/cases` that builds and starts no task is built with the
/// stacks it had: no function of the pool, none that makes or frees a task, and
/// a `take` of three parameters, which is the one that asks the system for 64 KB.
fn a_program_that_starts_no_task_has_no_pool_and_its_take_is_the_one_it_had() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let mut checked = 0;
    let mut files: Vec<_> = std::fs::read_dir(folder).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("smith") {
            continue;
        }
        let source = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&source);
        if text.contains("std.task") || text.contains("spawn") || text.contains("tasks") {
            continue;
        }
        let Ok(program) = lower(&source) else { continue };
        for f in &program.functions {
            for name in ["<pool_", "<stack_claim>", "<stack_free>"] {
                assert!(!f.name.starts_with(name), "{} has {} in {:?}", f.name, name, path);
            }
            if f.name == "<stack_take>" {
                assert_eq!(f.blocks[0].params.len(), 3, "the take of {path:?}");
            }
        }
        checked += 1;
    }
    assert!(checked > 50, "{checked} cases");
}

const LISTS: &str = "fn build(n: u64) -> List[u64] ! alloc {\n  var acc: List[u64] = []\n  loop i in 0..n { acc = push(acc, i) }\n  acc\n}\n\nfn total(xs: List[u64]) -> u64 {\n  var s: u64 = 0\n  loop i in 0..xs.size { s = wrapping_add(s, xs[i]) }\n  s\n}\n\n";

/// Children that each build a list and give it, joined one after another, by a
/// function that gives back what is in the list.
fn lists_one_after_another(count: u64) -> String {
    format!(
        "{USES}{LISTS}fn child() -> u64 ! alloc, diverge {{\n  with test_scheduler(1)\n  g = spawn {{ build(100) }}\n  xs = join(g)\n  total(xs)\n}}\n\nfn main() ! console, alloc, diverge {{\n  var sum: u64 = 0\n  loop j in 0..{count} {{\n    sum = wrapping_add(sum, child())\n  }}\n  print(\"{{sum}}\")\n}}\n"
    )
}

fn ten_thousand_children_that_give_a_list_joined_one_after_another_ask_the_system_for_the_memory_of_one()
 {
    let (few, few_bytes) = asked(&lists_one_after_another(10));
    let (many, many_bytes) = asked(&lists_one_after_another(300));
    assert_eq!(few, format!("{}\n", 4950 * 10));
    assert_eq!(many, format!("{}\n", 4950 * 300));
    assert_eq!(few_bytes, many_bytes);
}

/// A child that builds a list of `n` numbers and gives the list, or only its
/// size, to a parent that joins it.
fn join_of(n: u64, whole_list: bool) -> String {
    let gives = if whole_list { "build(n)" } else { "scalar(n)" };
    format!(
        "{USES}{LISTS}fn scalar(n: u64) -> u64 ! alloc {{\n  xs = build(n)\n  xs.size\n}}\n\nfn run(n: u64) -> u64 ! alloc, diverge {{\n  with test_scheduler(1)\n  g = spawn {{ {gives} }}\n  v = join(g)\n  1\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run({n})}}\")\n}}\n"
    )
}

/// What a join costs is what the program takes with a list given, less what it
/// takes with a number given, since the child builds the same list in both.
fn instructions_of_a_join(n: u64) -> i64 {
    let ran = |source: String| {
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        scalar::run_measured(&scalar::passes(&program)).1 as i64
    };
    ran(join_of(n, true)) - ran(join_of(n, false))
}

fn a_list_of_a_million_bytes_is_joined_in_the_instructions_a_list_of_ten_is() {
    let small = instructions_of_a_join(10);
    let large = instructions_of_a_join(125_000);
    assert!((large - small).abs() <= 200, "{small} instructions for 10 numbers, {large} for 125,000");
}

fn a_result_read_after_the_parent_asks_for_more_memory_is_the_same_bytes() {
    let source = format!(
        "{USES}{LISTS}fn run() -> u64 ! alloc, diverge {{\n  with test_scheduler(4)\n  g = spawn {{ build(1000) }}\n  xs = join(g)\n  var more: u64 = 0\n  loop r in 0..50 {{\n    ys = build(2000)\n    more = wrapping_add(more, total(ys))\n  }}\n  wrapping_add(total(xs), more)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    );
    let (said, _) = asked(&source);
    let one = (0..1000u64).sum::<u64>();
    let two = (0..2000u64).sum::<u64>();
    assert_eq!(said, format!("{}\n", one + 50 * two));
}

/// A function that gives the list a child gave up to `main`, which reads it after
/// more calls of the same function.
fn list_up_to_main() -> String {
    format!(
        "{USES}{LISTS}fn child(n: u64) -> List[u64] ! alloc, diverge {{\n  with test_scheduler(2)\n  g = spawn {{ build(n) }}\n  join(g)\n}}\n\nfn main() ! console, alloc, diverge {{\n  first = child(30)\n  second = child(40)\n  third = child(50)\n  print(\"{{total(first)}} {{total(second)}} {{total(third)}}\")\n}}\n"
    )
}

fn a_list_a_function_gives_up_to_main_is_read_there_after_more_calls_as_it_was_built() {
    let (said, _) = asked(&list_up_to_main());
    let sum = |n: u64| (0..n).sum::<u64>();
    assert_eq!(said, format!("{} {} {}\n", sum(30), sum(40), sum(50)));
}
