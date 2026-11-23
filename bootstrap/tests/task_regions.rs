//! The memory of a task is regions of its own, taken from the pool: a task that
//! waits with nothing built holds a few KB, a task that waits inside an arena in
//! a loop asks the system for the same memory after 20 rounds as after 1,000,
//! two tasks that each wait inside their own arena never see each other's bytes
//! change, and a task whose result holds no address gives its memory back to the
//! pool when it is joined.

use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_thousand_tasks_that_wait_with_nothing_built_hold_under_eight_megabytes",
        a_thousand_tasks_that_wait_with_nothing_built_hold_under_eight_megabytes,
    ),
    (
        "a_task_that_waits_inside_an_arena_in_a_loop_asks_the_system_for_the_same_memory_after_20_rounds_as_after_1000",
        a_task_that_waits_inside_an_arena_in_a_loop_asks_the_system_for_the_same_memory_after_20_rounds_as_after_1000,
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
    (
        "a_child_whose_handle_is_dropped_gives_its_memory_back_when_it_ends_whether_it_is_dropped_as_a_statement_or_bound_and_never_read",
        a_child_whose_handle_is_dropped_gives_its_memory_back_when_it_ends_whether_it_is_dropped_as_a_statement_or_bound_and_never_read,
    ),
    (
        "a_child_whose_handle_is_joined_on_one_branch_and_dropped_on_the_other_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_is_joined_on_one_branch_and_dropped_on_the_other_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_child_whose_handle_is_dropped_in_a_tuple_a_list_or_an_enum_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_is_dropped_in_a_tuple_a_list_or_an_enum_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_child_whose_handle_is_dropped_in_a_struct_or_as_what_the_last_statement_of_an_arm_gives_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_is_dropped_in_a_struct_or_as_what_the_last_statement_of_an_arm_gives_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_child_whose_handle_is_in_a_var_a_destructured_name_or_a_parameter_that_is_never_read_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_is_in_a_var_a_destructured_name_or_a_parameter_that_is_never_read_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_child_whose_handle_a_var_is_assigned_over_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_a_var_is_assigned_over_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_child_whose_handle_is_in_a_list_in_a_list_or_in_a_recursive_enum_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_is_in_a_list_in_a_list_or_in_a_recursive_enum_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_child_whose_handle_a_path_through_a_loop_joins_or_drops_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_a_path_through_a_loop_joins_or_drops_gives_its_memory_back_when_it_ends,
    ),
    (
        "every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_1_of_8",
        every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_1_of_8,
    ),
    (
        "every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_2_of_8",
        every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_2_of_8,
    ),
    (
        "every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_3_of_8",
        every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_3_of_8,
    ),
    (
        "every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_4_of_8",
        every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_4_of_8,
    ),
    (
        "every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_5_of_8",
        every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_5_of_8,
    ),
    (
        "every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_6_of_8",
        every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_6_of_8,
    ),
    (
        "every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_7_of_8",
        every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_7_of_8,
    ),
    (
        "every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_8_of_8",
        every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_8_of_8,
    ),
    (
        "a_child_whose_handle_a_pattern_binds_or_a_function_reads_on_one_path_only_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_a_pattern_binds_or_a_function_reads_on_one_path_only_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_child_whose_handle_is_a_field_of_a_struct_that_is_joined_or_assigned_over_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_is_a_field_of_a_struct_that_is_joined_or_assigned_over_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_child_whose_handle_a_loop_over_a_list_joins_before_it_breaks_gives_its_memory_back_when_it_ends",
        a_child_whose_handle_a_loop_over_a_list_joins_before_it_breaks_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_child_whose_handle_is_read_is_not_given_back_before_it_is_joined",
        a_child_whose_handle_is_read_is_not_given_back_before_it_is_joined,
    ),
    (
        "a_child_that_gives_a_text_of_twenty_bytes_holds_a_piece_of_64_bytes_for_it_and_no_more",
        a_child_that_gives_a_text_of_twenty_bytes_holds_a_piece_of_64_bytes_for_it_and_no_more,
    ),
    (
        "a_task_that_ends_while_it_waits_asks_the_children_it_holds_to_end_and_the_block_ends_each_task_by_its_exit",
        a_task_that_ends_while_it_waits_asks_the_children_it_holds_to_end_and_the_block_ends_each_task_by_its_exit,
    ),
    (
        "a_task_that_calls_what_waits_under_a_scheduler_looks_for_the_request_to_end_after_the_call_and_a_program_with_no_task_never_does",
        a_task_that_calls_what_waits_under_a_scheduler_looks_for_the_request_to_end_after_the_call_and_a_program_with_no_task_never_does,
    ),
    (
        "children_that_wait_when_the_block_that_started_them_ends_give_their_memory_back_for_20_rounds_as_for_1000",
        children_that_wait_when_the_block_that_started_them_ends_give_their_memory_back_for_20_rounds_as_for_1000,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "every_executable_gives_back_the_memory_of_children_whose_handles_are_dropped",
        every_executable_gives_back_the_memory_of_children_whose_handles_are_dropped,
    ),
    (
    "every_executable_keeps_the_regions_of_tasks_apart_and_gives_the_memory_of_a_joined_task_back",
    every_executable_keeps_the_regions_of_tasks_apart_and_gives_the_memory_of_a_joined_task_back,
)];

const USES_WITH_TASK: &str = "use std.alloc (arena)\nuse std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\nuse std.list (freeze, push)\nuse std.task (Task, join, spawn, task, test_scheduler)\n";

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

fn a_task_that_waits_inside_an_arena_in_a_loop_asks_the_system_for_the_same_memory_after_20_rounds_as_after_1000() {
    let (few, few_bytes) = asked(&arena_rounds(20));
    let (many, many_bytes) = asked(&arena_rounds(1000));
    let each = (0..200u64).sum::<u64>();
    let wanted = |n: u64| (0..n).map(|r| each + r + 1).sum::<u64>();
    assert_eq!(few, format!("{}\n", wanted(20)));
    assert_eq!(many, format!("{}\n", wanted(1000)));
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
    // that all exist at once, 2,000 rounds of an arena in a task, and 10,000 children one after another.
    let big = [
        (arena_rounds(2000), format!("{}\n", (0..2000u64).map(|r| (0..200u64).sum::<u64>() + r + 1).sum::<u64>())),
        (many(1000), "500500\n".to_string()),
        (one_after_another(10_000), format!("{}\n", 4950u64 * 10_000)),
        (lists_one_after_another(10_000), format!("{}\n", 4950u64 * 10_000)),
        (texts(10_000), format!("{}\n", (0..10_000u64).map(|n| n.to_string().len() as u64 + 17).sum::<u64>())),
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

/// A recursion that starts a child at each level, each giving a text of twenty
/// bytes, and joins them on the way back, so that all of them exist at once.
fn texts(count: u64) -> String {
    format!(
        "{USES}fn build(n: u64) -> List[u64] ! alloc {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, i) }}\n  acc\n}}\n\nfn down(xs: Slice[u64], at: Nat) -> u64 ! task, alloc\n  decreases xs.size - at\n{{\n  if at < xs.size {{\n    first = xs[at]\n    g = spawn {{ \"{{first}}-abcdefghijklmnop\" }}\n    v = down(xs, at + 1)\n    t = join(g)\n    wrapping_add(t.size, v)\n  }} else {{\n    0\n  }}\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler(3)\n  a = spawn {{ down(freeze(build({count})), 0) }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

/// The text a child gives is built in the first piece of its stack, which is 64
/// bytes with its header and trailer, so a task that gives a text of twenty bytes
/// costs its frame and 64 bytes, and no more.
fn a_child_that_gives_a_text_of_twenty_bytes_holds_a_piece_of_64_bytes_for_it_and_no_more() {
    let (few_said, few) = asked(&texts(100));
    let (many_said, more) = asked(&texts(200));
    let length = |count: u64| (0..count).map(|n| n.to_string().len() as u64 + 17).sum::<u64>();
    assert_eq!(few_said, format!("{}\n", length(100)));
    assert_eq!(many_said, format!("{}\n", length(200)));
    // The system gives memory in chunks of 256 KB, so a hundred children that
    // each held a piece of 4 KB would ask for two chunks more, and a hundred that
    // hold a frame and 64 bytes ask for no more than one.
    assert!(more.saturating_sub(few) <= scalar::pool::CHUNK as usize, "{few} bytes for 100 children, {more} for 200");
}

/// A loop that starts a child in each round, each building a list of 1,000 numbers
/// and adding them up, and does not keep the handle: as a statement of its own,
/// or bound to a name that nothing reads. The handle is `how`.
fn dropped_children(count: u64, seed: u64, how: &str) -> String {
    format!(
        "{USES_WITH_TASK}\n\nstruct Both {{\n  first: Task[u64]\n  second: Task[u64]\n}}\n\nenum Held {{\n  none\n  one: Task[u64]\n}}\n\nenum Deep {{\n  leaf: Task[u64]\n  more: List[Deep]\n}}\n\n{LISTS}fn nest() -> List[List[Task[u64]]] ! task, alloc {{\n  [[spawn {{ total(build(250)) }}], [spawn {{ total(build(250)) }}, spawn {{ total(build(250)) }}]]\n}}\n\nfn ignore(t: take Task[u64]) {{\n  ()\n}}\n\nfn maybe(t: take Task[u64], yes: bool) -> u64 ! task {{\n  if yes {{ join(t) }} else {{ 0 }}\n}}\n\nfn handles() -> List[Task[u64]] ! task, alloc {{\n  var hs: List[Task[u64]] = []\n  loop q in 0..3 {{\n    hs = push(hs, spawn {{ total(build(250)) }})\n  }}\n  hs\n}}\n\nfn rounds(n: u64) -> u64 ! task, alloc {{\n  var sum: u64 = 0\n  loop r in 0..n {{\n    {how}\n    var more: u64 = 0\n    loop q in 0..4 {{\n      j = spawn {{ wrapping_add(r, 1) }}\n      more = wrapping_add(more, join(j))\n    }}\n    sum = wrapping_add(sum, wrapping_add(1, wrapping_mul(more, 0)))\n  }}\n  sum\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler({seed})\n  a = spawn {{ rounds({count}) }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    )
}

/// Each of a hundred children would keep 4 KB of list and more, and a child that is never
/// joined used to keep it until the program ended. A handle that is dropped gives
/// the child's memory back when the child has ended, whether it was dropped before
/// the child ended or after, which the seeds of the scheduler pick between.
fn a_child_whose_handle_is_dropped_gives_its_memory_back_when_it_ends_whether_it_is_dropped_as_a_statement_or_bound_and_never_read() {
    kept_by_drops(&DROPS[..3]);
}

fn a_child_whose_handle_is_joined_on_one_branch_and_dropped_on_the_other_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[3..4]);
}

fn a_child_whose_handle_is_dropped_in_a_struct_or_as_what_the_last_statement_of_an_arm_gives_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[7..9]);
}

fn a_child_whose_handle_is_in_a_var_a_destructured_name_or_a_parameter_that_is_never_read_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[9..12]);
}

fn a_child_whose_handle_a_var_is_assigned_over_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[12..13]);
}

fn a_child_whose_handle_is_in_a_list_in_a_list_or_in_a_recursive_enum_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[13..15]);
}

fn a_child_whose_handle_a_path_through_a_loop_joins_or_drops_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[15..17]);
}

fn a_child_whose_handle_a_pattern_binds_or_a_function_reads_on_one_path_only_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[17..20]);
}

fn a_child_whose_handle_is_a_field_of_a_struct_that_is_joined_or_assigned_over_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[20..22]);
}

fn a_child_whose_handle_a_loop_over_a_list_joins_before_it_breaks_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[22..]);
}

fn a_child_whose_handle_is_dropped_in_a_tuple_a_list_or_an_enum_gives_its_memory_back_when_it_ends() {
    kept_by_drops(&DROPS[4..7]);
}

fn kept_by_drops(hows: &[&str]) {
    let kept = |how: &str, seed: u64| {
        let (few_said, few) = asked(&dropped_children(8, seed, how));
        let (many_said, more) = asked(&dropped_children(56, seed, how));
        assert_eq!((few_said, many_said), ("8\n".to_string(), "56\n".to_string()), "{how}");
        more.saturating_sub(few)
    };
    for seed in [1, 2] {
        for how in hows {
            let more = kept(how, seed);
            // 48 more children, each of 4 KB of list and the pieces it grew in, would
            // be 400 KB of chunks, which is more than one.
            assert!(more <= scalar::pool::CHUNK as usize, "{more} bytes more for 48 children, `{how}`, seed {seed}");
        }
    }
}

/// The places a handle is dropped, each in a loop that starts a child in each
/// round: as a statement, as a name nothing reads, joined in some rounds and
/// dropped in the others, in a tuple, in a struct, and as the value that the last
/// statement of an arm gives to a `match` whose value is thrown away.
const DROPS: [&str; 23] = [
    "spawn { total(build(500)) }",
    "t = spawn { total(build(500)) }",
    "t = spawn { total(build(500)) }\n    if r < 5 { sum = wrapping_add(sum, wrapping_mul(join(t), 0)) } else { }",
    "t = spawn { total(build(500)) }\n    if r < 5 { } else { sum = wrapping_add(sum, wrapping_mul(join(t), 0)) }",
    "pair = (spawn { total(build(250)) }, spawn { total(build(250)) })",
    "hs = handles()",
    "held = Held.one(spawn { total(build(500)) })",
    "both = Both(first: spawn { total(build(250)) }, second: spawn { total(build(250)) })",
    "match r < 1000 {\n      .true { spawn { total(build(500)) } }\n      .false { spawn { total(build(500)) } }\n    }",
    "var t = spawn { total(build(500)) }",
    "(a, b) = (spawn { total(build(250)) }, spawn { total(build(250)) })",
    "ignore(spawn { total(build(500)) })",
    "var t = spawn { total(build(500)) }\n    t = spawn { total(build(500)) }\n    t = spawn { total(build(500)) }",
    "ns = nest()",
    "d = Deep.more([Deep.more([Deep.more([Deep.more([Deep.more([Deep.more([Deep.more([Deep.more([Deep.more([Deep.more([Deep.leaf(spawn { total(build(250)) })])])])])])])])])])])",
    "t = spawn { total(build(500)) }\n    loop i in 0..3 {\n      if r < 5 {\n        sum = wrapping_add(sum, wrapping_mul(join(t), 0))\n        break\n      } else { }\n    }",
    "var t = spawn { total(build(500)) }\n    loop i in 0..2 {\n      sum = wrapping_add(sum, wrapping_mul(join(t), 0))\n      t = spawn { total(build(500)) }\n    }",
    "match Held.one(spawn { total(build(500)) }) {\n      .none { }\n      .one(t) { }\n    }",
    "t = spawn { total(build(500)) }\n    sum = wrapping_add(sum, wrapping_mul(maybe(t, r < 5), 0))",
    "(a, b) = (spawn { total(build(250)) }, spawn { total(build(250)) })\n    sum = wrapping_add(sum, wrapping_mul(join(a), 0))",
    "both = Both(first: spawn { total(build(250)) }, second: spawn { total(build(250)) })\n    sum = wrapping_add(sum, wrapping_mul(join(both.first), 0))",
    "var both = Both(first: spawn { total(build(250)) }, second: spawn { total(build(250)) })\n    both.first = spawn { total(build(250)) }\n    both.second = spawn { total(build(250)) }",
    "t = spawn { total(build(500)) }\n    xs = build(3)\n    loop x in xs {\n      if r < 5 {\n        sum = wrapping_add(sum, wrapping_mul(join(t), 0))\n        break\n      } else { }\n    }",
];

/// A handle that is read is the program's to join, so what it holds is not given
/// back until it is joined: the same loop, which joins each child after the next
/// one starts, gives the right sums.
fn a_child_whose_handle_is_read_is_not_given_back_before_it_is_joined() {
    let source = format!(
        "{USES}{LISTS}fn run() -> u64 ! alloc, diverge {{\n  with test_scheduler(3)\n  a = spawn {{ total(build(100)) }}\n  b = spawn {{ total(build(200)) }}\n  c = spawn {{ total(build(300)) }}\n  x = join(c)\n  y = join(a)\n  z = join(b)\n  wrapping_add(x, wrapping_add(y, z))\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    );
    let (said, _) = asked(&source);
    let sum = |n: u64| n * (n - 1) / 2;
    assert_eq!(said, format!("{}\n", sum(100) + sum(200) + sum(300)));
}

fn every_executable_gives_back_the_memory_of_children_whose_handles_are_dropped() {
    let targets = runnable();
    for (source, wanted) in [
        (dropped_children(3_000, 1, "spawn { total(build(100)) }"), "3000\n"),
    ]
    .into_iter()
    .chain(DROPS.iter().map(|how| (dropped_children(30, 2, how), "30\n")))
    {
        let said = scalar::Outcome {
            status: 0,
            stdout: wanted.as_bytes().to_vec(),
        };
        for &target in &targets {
            runs_as(source.as_bytes(), target, &said);
        }
    }
}

/// How a generated program holds the children one item of it starts.
#[derive(Clone, Copy, Debug)]
enum Hold {
    Name,
    Var,
    /// A `var` that is assigned over before it is read.
    Assigned,
    Tuple,
    Struct,
    List,
    /// A list in a list.
    Nested,
    Enum,
    /// An enum that holds a list of itself, this deep.
    Deep(usize),
    /// A parameter of a function.
    Param,
    /// A `var` that a loop joins and starts again.
    Reborn,
    /// A name that a loop joins and leaves on one path, and that ends it where
    /// the range does on the other.
    Looped,
    /// A list that is made where the loop over it is written.
    Over,
    /// A `var` struct whose fields are assigned over before they are read.
    Fielded,
    /// A name that a loop over a list joins and leaves on one path, and that
    /// ends it where the list does on the other.
    OverBreak,
}

/// What a generated program does with the children of an item in a round.
#[derive(Clone, Copy, Debug)]
enum End {
    Join,
    Drop,
    /// Joins them in the rounds below the number, and drops them in the others.
    Below(u64),
    /// Joins the first of two, and drops the other.
    Half,
}

#[derive(Clone, Copy, Debug)]
struct Item {
    hold: Hold,
    end: End,
    size: u64,
}

/// What `total(build(n))` gives.
fn sum_to(n: u64) -> u64 {
    n * n.saturating_sub(1) / 2
}

impl Item {
    /// The work of a child of the item, the `which`th, which gives a number of its own.
    fn work(&self, which: u64) -> String {
        format!("spawn {{ total(build({})) }}", self.size + which)
    }

    fn value(&self, which: u64) -> u64 {
        sum_to(self.size + which)
    }

    /// The children the item starts, as the sizes that tell them apart.
    fn children(&self) -> Vec<u64> {
        match self.hold {
            Hold::Name
            | Hold::Var
            | Hold::Assigned
            | Hold::Param
            | Hold::Looped
            | Hold::OverBreak
            | Hold::Enum => vec![0],
            Hold::Tuple | Hold::Struct | Hold::Fielded | Hold::List => vec![0, 1],
            Hold::Nested => vec![0, 1, 2],
            Hold::Deep(_) => vec![0],
            Hold::Reborn => vec![0],
            Hold::Over => vec![0, 1, 2],
        }
    }

    /// What the item adds to the sum in the round `r`.
    fn adds(&self, r: u64) -> u64 {
        let joined = match self.end {
            End::Join | End::Half => true,
            End::Drop => false,
            End::Below(c) => r < c,
        };
        let all: u64 = self.children().iter().map(|&k| self.value(k)).sum();
        let once = match (self.hold, self.end) {
            (Hold::Tuple | Hold::Struct | Hold::Fielded, End::Half) => self.value(0),
            _ => all,
        };
        // A `var` that a loop joins and starts again joins twice before it ends.
        let before = match self.hold {
            Hold::Reborn => 2 * self.value(0),
            _ => 0,
        };
        // A loop that joins on one path joins only when the end does.
        match self.hold {
            Hold::Looped | Hold::OverBreak => {
                if joined {
                    self.value(0)
                } else {
                    0
                }
            }
            _ => before + if joined { once } else { 0 },
        }
    }

    /// The statements of the item in a round, each of which is one line, with
    /// the names it defines ended by `id`.
    fn source(&self, id: usize) -> String {
        let add = |what: &str| format!("sum = wrapping_add(sum, {what})");
        // The statements that end the children of the name, as `join` of what
        // the closure makes the join of one.
        let ends = |join: String, drop: String| -> String {
            match self.end {
                End::Join | End::Half => join,
                End::Drop => drop,
                End::Below(c) => format!("if r < {c} {{\n{join}\n}} else {{ }}"),
            }
        };
        let w = |k: u64| self.work(k);
        match self.hold {
            Hold::Name | Hold::Var => {
                let bind = if matches!(self.hold, Hold::Var) { "var " } else { "" };
                format!(
                    "{bind}h{id} = {}\n{}",
                    w(0),
                    ends(add(&format!("join(h{id})")), String::new())
                )
            }
            Hold::Assigned => format!(
                "var h{id} = {}\nh{id} = {}\n{}",
                self.work(9),
                w(0),
                ends(add(&format!("join(h{id})")), String::new())
            ),
            Hold::Tuple => {
                let join = match self.end {
                    End::Half => add(&format!("join(a{id})")),
                    _ => format!("{}\n{}", add(&format!("join(a{id})")), add(&format!("join(b{id})"))),
                };
                format!("(a{id}, b{id}) = ({}, {})\n{}", w(0), w(1), ends(join, String::new()))
            }
            Hold::Struct | Hold::Fielded => {
                let join = match self.end {
                    End::Half => add(&format!("join(p{id}.first)")),
                    _ => format!(
                        "{}\n{}",
                        add(&format!("join(p{id}.first)")),
                        add(&format!("join(p{id}.second)"))
                    ),
                };
                if matches!(self.hold, Hold::Fielded) {
                    format!(
                        "var p{id} = Both(first: {}, second: {})\np{id}.first = {}\np{id}.second = {}\n{}",
                        w(8),
                        w(9),
                        w(0),
                        w(1),
                        ends(join, String::new())
                    )
                } else {
                    format!("p{id} = Both(first: {}, second: {})\n{}", w(0), w(1), ends(join, String::new()))
                }
            }
            Hold::List => format!("hs{id}: List[Task[u64]] = [{}, {}]", w(0), w(1)),
            Hold::Nested => format!("ns{id}: List[List[Task[u64]]] = [[{}], [{}, {}]]", w(0), w(1), w(2)),
            Hold::Enum => format!(
                "e{id} = Held.one({})\n{}",
                w(0),
                ends(
                    format!(
                        "match e{id} {{\n.none {{ }}\n.one(t{id}) {{\n{}\n}}\n}}",
                        add(&format!("join(t{id})"))
                    ),
                    String::new()
                )
            ),
            Hold::Deep(depth) => {
                let mut made = format!("Deep.leaf({})", w(0));
                for _ in 0..depth {
                    made = format!("Deep.more([{made}])");
                }
                format!("d{id} = {made}")
            }
            Hold::Param => {
                let bind = format!("h{id} = {}\n", w(0));
                match self.end {
                    End::Join | End::Half => format!("{bind}{}", add(&format!("finish(h{id})"))),
                    End::Drop => format!("{bind}ignore(h{id})"),
                    End::Below(c) => format!("{bind}{}", add(&format!("maybe(h{id}, r < {c})"))),
                }
            }
            Hold::Reborn => format!(
                "var h{id} = {}\nloop i{id} in 0..2 {{\n{}\nh{id} = {}\n}}\n{}",
                w(0),
                add(&format!("join(h{id})")),
                w(0),
                ends(add(&format!("join(h{id})")), String::new())
            ),
            Hold::Looped => {
                let c = match self.end {
                    End::Below(c) => c,
                    End::Join | End::Half => u64::MAX,
                    End::Drop => 0,
                };
                format!(
                    "h{id} = {}\nloop i{id} in 0..3 {{\nif r < {c} {{\n{}\nbreak\n}} else {{ }}\n}}",
                    w(0),
                    add(&format!("join(h{id})"))
                )
            }
            Hold::OverBreak => {
                let c = match self.end {
                    End::Below(c) => c,
                    End::Join | End::Half => u64::MAX,
                    End::Drop => 0,
                };
                format!(
                    "t{id} = {}\nxs{id} = build(3)\nloop x{id} in xs{id} {{\nif r < {c} {{\n{}\nbreak\n}} else {{ }}\n}}",
                    w(0),
                    add(&format!("join(t{id})"))
                )
            }
            Hold::Over => format!("handles({})", self.size),
        }
    }
}

/// A generated program: the items it runs in each round of a loop.
#[derive(Clone, Debug)]
struct Held_ {
    items: Vec<Item>,
    seed: u64,
}

fn holds_a_list(hold: Hold) -> bool {
    matches!(hold, Hold::List | Hold::Nested | Hold::Deep(_) | Hold::Over)
}

fn an_item(g: &mut Gen) -> Item {
    let hold = match g.below(15) {
        0 => Hold::Name,
        1 => Hold::Var,
        2 => Hold::Assigned,
        3 => Hold::Tuple,
        4 => Hold::Struct,
        5 => Hold::List,
        6 => Hold::Nested,
        7 => Hold::Enum,
        8 => Hold::Deep(1 + g.below(11)),
        9 => Hold::Param,
        10 => Hold::Reborn,
        11 => Hold::Looped,
        12 => Hold::Fielded,
        13 => Hold::OverBreak,
        _ => Hold::Over,
    };
    let end = match g.below(4) {
        0 => End::Join,
        1 => End::Drop,
        2 => End::Below(1 + g.below(6) as u64),
        _ => End::Half,
    };
    // A loop lends the handles of a list, and a handle is given away to be
    // joined, so the handles of a list are dropped and never joined.
    let end = if holds_a_list(hold) { End::Drop } else { end };
    let size = 10 + 5 * g.below(7) as u64;
    Item { hold, end, size }
}

fn generated_program(g: &mut Gen) -> Held_ {
    // The evaluator enters 3 million blocks at most, and a child costs thousands
    // of them in the scheduler of a test, so a round starts four children at most.
    let count = 1 + g.below(3);
    let mut items: Vec<Item> = Vec::new();
    let mut children = 0;
    for _ in 0..count {
        let item = an_item(g);
        children += item.children().len();
        if children > 4 && !items.is_empty() {
            break;
        }
        items.push(item);
    }
    Held_ {
        items,
        seed: 1 + g.below(3) as u64,
    }
}

impl Held_ {
    fn source(&self, rounds: u64) -> String {
        let body: String = (self.items.iter().enumerate())
            .map(|(id, item)| {
                item.source(id)
                    .lines()
                    .map(|line| format!("    {line}\n"))
                    .collect::<String>()
            })
            .collect();
        format!(
            "{USES_WITH_TASK}\n\nstruct Both {{\n  first: Task[u64]\n  second: Task[u64]\n}}\n\nenum Held {{\n  none\n  one: Task[u64]\n}}\n\nenum Deep {{\n  leaf: Task[u64]\n  more: List[Deep]\n}}\n\n{LISTS}fn ignore(t: take Task[u64]) {{\n  ()\n}}\n\nfn finish(t: take Task[u64]) -> u64 ! task {{\n  join(t)\n}}\n\nfn maybe(t: take Task[u64], yes: bool) -> u64 ! task {{\n  if yes {{ join(t) }} else {{ 0 }}\n}}\n\nfn handles(n: u64) -> List[Task[u64]] ! task, alloc {{\n  [spawn {{ total(build(n)) }}, spawn {{ total(build(wrapping_add(n, 1))) }}, spawn {{ total(build(wrapping_add(n, 2))) }}]\n}}\n\nfn rounds(n: u64) -> u64 ! task, alloc {{\n  var sum: u64 = 0\n  loop r in 0..n {{\n{body}    var more: u64 = 0\n    loop q in 0..2 {{\n      j = spawn {{ wrapping_add(r, 1) }}\n      more = wrapping_add(more, join(j))\n    }}\n    sum = wrapping_add(sum, wrapping_mul(more, 0))\n  }}\n  sum\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler({})\n  a = spawn {{ rounds({rounds}) }}\n  join(a)\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n",
            self.seed
        )
    }

    /// What the program prints.
    fn prints(&self, rounds: u64) -> u64 {
        (0..rounds)
            .map(|r| self.items.iter().map(|item| item.adds(r)).sum::<u64>())
            .sum()
    }
}

/// A program that holds children in names, `var`s, patterns, parameters, structs,
/// lists in lists, enums, and loops, and joins them or drops them on any path,
/// gives each child's value once, and asks the system for the same memory for 100
/// rounds as for 10: a child that was freed before its join would give another
/// number, and one that was never freed would ask for more.
fn generated_children(part: u64) {
    generate::check_part(9601, 40, part, 8, generated_program, |program| {
        let (few_said, few) = asked(&program.source(10));
        let (many_said, many) = asked(&program.source(100));
        assert_eq!(few_said, format!("{}\n", program.prints(10)), "{}", program.source(10));
        assert_eq!(many_said, format!("{}\n", program.prints(100)), "{}", program.source(100));
        assert!(
            many.saturating_sub(few) <= scalar::pool::CHUNK as usize,
            "{few} bytes for 10 rounds and {many} for 100 of\n{}",
            program.source(100)
        );
    });
}

fn every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_1_of_8() {
    generated_children(0);
}

fn every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_2_of_8() {
    generated_children(1);
}

fn every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_3_of_8() {
    generated_children(2);
}

fn every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_4_of_8() {
    generated_children(3);
}

fn every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_5_of_8() {
    generated_children(4);
}

fn every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_6_of_8() {
    generated_children(5);
}

fn every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_7_of_8() {
    generated_children(6);
}

fn every_child_of_a_generated_program_is_joined_or_freed_once_and_the_memory_asked_for_is_the_same_for_10_and_100_rounds_part_8_of_8() {
    generated_children(7);
}

/// Rounds of a block that starts children that wait for a handle that is never
/// ready, one of them with a child of its own, and ends without joining any.
fn left_waiting(rounds: u64) -> String {
    format!(
        "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\nuse std.task (spawn, task, test_ready, test_scheduler_with_io)\n\nfn waits(n: u64) -> u64 ! task {{\n  test_ready(n)\n  n\n}}\n\nfn parent(n: u64) -> u64 ! task {{\n  spawn {{ waits(n) }}\n  test_ready(n)\n  n\n}}\n\nfn once(r: u64) -> u64 ! diverge {{\n  with test_scheduler_with_io(r, [])\n  spawn {{ waits(r) }}\n  spawn {{ parent(r) }}\n  spawn {{ waits(r) }}\n  r\n}}\n\nfn rounds(n: u64) -> u64 ! diverge {{\n  var sum: u64 = 0\n  loop r in 0..n {{ sum = wrapping_add(sum, once(r)) }}\n  sum\n}}\n\nfn main() ! console, diverge, alloc {{\n  print(\"{{rounds({rounds})}}\")\n}}\n"
    )
}

/// The block of a scheduler ends when its first task does, and what is still
/// waiting then is ended, and its memory given back to the pool: the system is
/// asked for the same memory after 20 rounds as after 1,000.
fn children_that_wait_when_the_block_that_started_them_ends_give_their_memory_back_for_20_rounds_as_for_1000() {
    let (few, few_bytes) = asked(&left_waiting(20));
    let (many, many_bytes) = asked(&left_waiting(1000));
    assert_eq!(few, format!("{}\n", (0..20u64).sum::<u64>()));
    assert_eq!(many, format!("{}\n", (0..1000u64).sum::<u64>()));
    assert_eq!(few_bytes, many_bytes);
}

/// Whether a function of the program reads the bit of its frame that asks the
/// task to end: it masks the word of the header with the bit and shifts the bit
/// down to a 0 or a 1, which is what a function that looks for the request does
/// and what the poll of a caller, which only copies the bit to its callee, does
/// not.
fn reads_the_request(program: &scalar::Program) -> bool {
    use scalar::{Binary, Constant, Op};
    program.functions.iter().any(|f| {
        let all = || f.blocks.iter().flat_map(|b| &b.instructions);
        let bits: std::collections::HashSet<_> = all()
            .filter(|i| matches!(i.op, Op::Constant(Constant::Int(n)) if n == scalar::CANCEL))
            .flat_map(|i| i.results.iter().copied())
            .collect();
        let masked: std::collections::HashSet<_> = all()
            .filter(|i| matches!(i.op, Op::Binary { op: Binary::And, b, .. } if bits.contains(&b)))
            .flat_map(|i| i.results.iter().copied())
            .collect();
        all().any(|i| {
            matches!(i.op, Op::Binary { op: Binary::ShiftRightU, a, .. } if masked.contains(&a))
        })
    })
}

/// A call that may suspend the task is followed by a look at whether the task
/// has been asked to end, which leaves the function, so a task that is polled
/// when it has been asked to leaves by it. A program with no task has no such
/// look in it: nothing asks it, so it is built as it was.
fn a_task_that_calls_what_waits_under_a_scheduler_looks_for_the_request_to_end_after_the_call_and_a_program_with_no_task_never_does()
{
    let with_task = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\nuse std.task (join, spawn, task, test_ready, test_scheduler_with_io)\n\nfn waits(n: u64) -> u64 ! task {\n  test_ready(n)\n  n\n}\n\nfn calls(n: u64) -> u64 ! task {\n  wrapping_add(waits(n), 1)\n}\n\nfn run() -> u64 ! diverge, alloc {\n  with test_scheduler_with_io(3, [5])\n  a = spawn { calls(5) }\n  join(a)\n}\n\nfn main() ! console, alloc, diverge {\n  print(\"{run()}\")\n}\n";
    let program = lower(with_task.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(scalar::check(&program), Ok(()));
    assert!(reads_the_request(&program));
    let (said, _) = asked(with_task);
    assert_eq!(said, "6\n");
    let none = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\n\nfn calls(n: u64) -> u64 {\n  wrapping_add(n, 1)\n}\n\nfn main() ! console, alloc {\n  print(\"{calls(5)}\")\n}\n";
    let program = lower(none.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(!reads_the_request(&program));
}

/// Whether a function of the program stores the two bits that drop a child of a
/// task that ends: that its handle is dropped, and that it is asked to end.
fn asks_a_child_to_end(program: &scalar::Program) -> bool {
    use scalar::{Constant, Op};
    let both = 8 | scalar::CANCEL;
    program.functions.iter().any(|f| {
        (f.blocks.iter().flat_map(|b| &b.instructions))
            .any(|i| matches!(i.op, Op::Constant(Constant::Int(n)) if n == both))
    })
}

/// A task that holds the handle of a child that waits, and waits itself, is
/// ended when the block of the scheduler ends: it leaves by its exit, which asks
/// the child to end, and the child leaves by its own. Nothing is run after the
/// wait that each is in, whatever the order the scheduler ran them in, and the
/// block gives what its first task gave. A group of a program that holds no
/// child that waits has no such mark.
fn a_task_that_ends_while_it_waits_asks_the_children_it_holds_to_end_and_the_block_ends_each_task_by_its_exit()
{
    let source = |seed: u64| {
        format!(
            "use std.bits (u64)\nuse std.console (console, print)\nuse std.task (spawn, task, test_ready, test_scheduler_with_io)\n\nfn waits(n: u64) -> u64 ! task, console, alloc {{\n  test_ready(n)\n  print(\"{{n}} woke\")\n  n\n}}\n\nfn parent(n: u64) -> u64 ! task, console, alloc {{\n  spawn {{ waits(n) }}\n  spawn {{ parent_of(n) }}\n  test_ready(n)\n  print(\"parent woke\")\n  n\n}}\n\nfn parent_of(n: u64) -> u64 ! task, console, alloc {{\n  c = spawn {{ waits(n) }}\n  test_ready(n)\n  print(\"nested woke\")\n  n\n}}\n\nfn run() -> u64 ! diverge, console, alloc {{\n  with test_scheduler_with_io({seed}, [])\n  spawn {{ parent(7) }}\n  spawn {{ waits(8) }}\n  3\n}}\n\nfn main() ! console, diverge, alloc {{\n  print(\"{{run()}}\")\n}}\n"
        )
    };
    for seed in 0..8u64 {
        let text = source(seed);
        let program = lower(text.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {text}"));
        assert_eq!(scalar::check(&program), Ok(()), "for {text}");
        assert!(asks_a_child_to_end(&program), "for {text}");
        let (said, _) = asked(&text);
        assert_eq!(said, "3\n", "seed {seed}");
    }
    let none = "use std.bits (u64)\nuse std.console (console, print)\n\nfn main() ! console, alloc {\n  print(\"{3}\")\n}\n";
    let program = lower(none.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(!asks_a_child_to_end(&program));
}
