//! A task group, `spawn`, and `join`: the programs of `tasks.md`,
//! `task-sharing.md`, `task-handlers.md`, and `task-arenas.md` give what the
//! pages say, each child runs once if it is joined, nothing of a child runs
//! after its group's block, each wrong use of the values around a child is
//! refused, and every executable gives what the evaluator gives.

use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "fetch_one_and_fetch_both_give_what_the_page_says_under_a_handler_of_fetch",
        fetch_one_and_fetch_both_give_what_the_page_says_under_a_handler_of_fetch,
    ),
    (
        "each_child_runs_once_if_it_is_joined_and_none_runs_after_the_block_ends",
        each_child_runs_once_if_it_is_joined_and_none_runs_after_the_block_ends,
    ),
    (
        "an_ordinary_value_is_shared_and_a_var_is_read_as_it_was_at_the_spawn",
        an_ordinary_value_is_shared_and_a_var_is_read_as_it_was_at_the_spawn,
    ),
    (
        "a_task_holds_its_value_where_only_join_reads_it",
        a_task_holds_its_value_where_only_join_reads_it,
    ),
    (
        "the_programs_of_the_pages_build_as_the_pages_write_them_and_give_what_they_say",
        the_programs_of_the_pages_build_as_the_pages_write_them_and_give_what_they_say,
    ),
    (
        "a_child_that_assigns_a_var_is_refused",
        a_child_that_assigns_a_var_is_refused,
    ),
    (
        "a_value_used_once_moves_into_the_child_and_a_second_use_is_refused",
        a_value_used_once_moves_into_the_child_and_a_second_use_is_refused,
    ),
    (
        "a_function_that_spawns_with_no_group_carries_spawn_and_its_caller_that_opens_one_does_not",
        a_function_that_spawns_with_no_group_carries_spawn_and_its_caller_that_opens_one_does_not,
    ),
    (
        "a_child_runs_under_the_handlers_in_force_where_it_was_spawned_and_its_operations_run_one_at_a_time",
        a_child_runs_under_the_handlers_in_force_where_it_was_spawned_and_its_operations_run_one_at_a_time,
    ),
    (
        "a_child_gives_its_memory_back_when_it_ends_and_its_result_is_built_in_its_parents",
        a_child_gives_its_memory_back_when_it_ends_and_its_result_is_built_in_its_parents,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_prints_what_the_evaluator_says_when_children_are_joined",
    every_executable_prints_what_the_evaluator_says_when_children_are_joined,
)];

const USES: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\nuse std.list (copy, push)\nuse std.net (NetError)\nuse std.random (Key, bits, seeded, split)\nuse std.task (join, spawn, tasks)\n\n";

/// The text of a program that reads the pages' effects, with the imports.
fn program(items: &str) -> String {
    format!("{USES}{items}")
}

fn run(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(scalar::check(&program), Ok(()), "for {source}");
    let outcome = scalar::run(&program);
    assert_eq!(outcome.status, 0, "for {source}");
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

/// `fetch`, declared as an effect that the test handles, and the two functions
/// of `tasks.md`.
const FETCH: &str = "effect web {\n  fn fetch(url: str) -> Result[List[Byte], NetError]\n}\n\nfn fetch_one(url: str) -> Result[List[Byte], NetError] ! web, alloc {\n  with tasks()\n  t = spawn { fetch(url) }\n  join(t)\n}\n\nfn fetch_both(a: str, b: str) -> Result[(List[Byte], List[Byte]), NetError] ! web, alloc {\n  with tasks()\n  ta = spawn { fetch(a) }\n  tb = spawn { fetch(b) }\n  pa = try join(ta)\n  pb = try join(tb)\n  .ok((pa, pb))\n}\n\nfn served[T](body: fn() -> T ! web) -> T ! alloc {\n  web(fetch: fn(url) -> Result[List[Byte], NetError] { if url.size == 0 { .err(.refused) } else { .ok([7, 8]) } }, body: body)\n}\n\n";

fn fetch_one_and_fetch_both_give_what_the_page_says_under_a_handler_of_fetch() {
    let main = "fn main() ! console, alloc {\n  with served()\n  one = fetch_one(\"a\")\n  match one {\n    .ok(page) { print(\"one {page.size}\") }\n    .err(e) { print(\"one failed\") }\n  }\n  both = fetch_both(\"a\", \"bb\")\n  match both {\n    .ok(pages) {\n      (x, y) = pages\n      print(\"both {x.size} {y.size}\")\n    }\n    .err(e) { print(\"both failed\") }\n  }\n  failed = fetch_both(\"a\", \"\")\n  match failed {\n    .ok(pages) { print(\"no\") }\n    .err(e) { print(\"second failed\") }\n  }\n}\n";
    let said = run(&program(&format!("{FETCH}{main}")));
    assert_eq!(said, "one 2\nboth 2 2\nsecond failed\n");
}

/// A program that spawns `spawned` children in a loop of calls, each of which
/// asks a counter, joins the first `joined` of them, and says what the
/// counter held when the block ended and after it.
fn counting(spawned: usize, joined: usize) -> String {
    let mut s = String::from("effect count {\n  fn tick() -> u64\n}\n\nfn counter[T](body: fn() -> T ! count) -> T {\n  var n: u64 = 0\n  count(\n    tick: {\n      n = wrapping_add(n, 1)\n      n\n    },\n    body: body,\n  )\n}\n\nfn group() -> u64 ! count {\n  with tasks()\n");
    for k in 0..spawned {
        s += &format!("  t{k} = spawn {{ tick() }}\n");
    }
    s += "  var total: u64 = 0\n";
    for k in 0..joined {
        s += &format!("  total = wrapping_add(total, wrapping_mul(join(t{k}), {}))\n", 1000u64.pow(k as u32 % 3));
    }
    s += "  total\n}\n\nfn main() ! console, alloc {\n  with counter()\n  total = group()\n  after = tick()\n  print(\"{total} {after}\")\n}\n";
    program(&s)
}

/// A child that is joined runs exactly once, so the counter has at least one
/// tick for each of them and at most one for each child, and nothing runs
/// after the block, so the first tick the program asks for itself comes right
/// after the ticks of the children.
fn each_child_runs_once_if_it_is_joined_and_none_runs_after_the_block_ends() {
    generate::check(330, 24, |g| {
        let spawned = 1 + g.below(5);
        (spawned, g.below(spawned + 1))
    }, |&(spawned, joined)| {
        let said = run(&counting(spawned, joined));
        let (total, after) = said.trim().split_once(' ').expect("two numbers");
        let (total, after): (u64, u64) = (total.parse().unwrap(), after.parse().unwrap());
        // `after` is one more than the number of children that ran, which is
        // every child that was joined, and no more than every child.
        assert!(after > joined as u64 && after <= spawned as u64 + 1, "{said} for {spawned} {joined}");
        // The values the joined children gave are each a different tick.
        let mut ticks: Vec<u64> = (0..joined as u32)
            .map(|k| (total / 1000u64.pow(k % 3)) % 1000)
            .collect();
        if joined <= 3 {
            ticks.sort();
            ticks.dedup();
            assert_eq!(ticks.len(), joined, "{said} for {spawned} {joined}");
        }
    });
}

/// `fetch_twice`, `snapshot`, and a function that gives what a child read.
fn an_ordinary_value_is_shared_and_a_var_is_read_as_it_was_at_the_spawn() {
    generate::check(331, 20, |g| (g.below(1000) as u64, 1 + g.below(1000) as u64), |&(a, b)| {
        let source = program(&format!(
            "fn snapshot() -> u64 {{\n  var n: u64 = {a}\n  with tasks()\n  t = spawn {{ wrapping_add(n, 1) }}\n  n = {b}\n  wrapping_add(n, join(t))\n}}\n\nfn twice(x: str) -> (u64, u64) {{\n  with tasks()\n  ta = spawn {{ wrapping_add(x.size, 1) }}\n  tb = spawn {{ wrapping_mul(x.size, 2) }}\n  (join(ta), join(tb))\n}}\n\nfn main() ! console, alloc {{\n  (p, q) = twice(\"abc\")\n  print(\"{{snapshot()}} {{p}} {{q}}\")\n}}\n"
        ));
        assert_eq!(run(&source), format!("{} 4 6\n", b + a + 1));
    });
}

/// The code block of a page of the language that holds `needle`.
fn page_block(page: &str, needle: &str) -> String {
    let path = format!("{}/../knowledge/language/{page}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let blocks: Vec<&str> = text.split("```\n").collect();
    // The blocks are the odd pieces, between the fences.
    let found: Vec<&&str> = blocks
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|b| b.contains(needle))
        .collect();
    assert_eq!(found.len(), 1, "{page} has one block with `{needle}`");
    found[0].to_string()
}

/// `snapshot` and `start_two`, read from the pages and run as they are
/// written there, with a `main` that prints what they give.
fn the_programs_of_the_pages_build_as_the_pages_write_them_and_give_what_they_say() {
    let snapshot = page_block("task-sharing.md", "fn snapshot()");
    let said = run(&format!(
        "use std.console (console, print)\n{snapshot}\nfn main() ! console, alloc {{\n  print(\"{{snapshot()}}\")\n}}\n"
    ));
    assert_eq!(said, "7\n");
    let start = page_block("tasks.md", "fn start_two()");
    let said = run(&format!(
        "use std.console (console, print)\n{start}\nfn main() ! console, alloc {{\n  start_two()\n  print(\"done\")\n}}\n"
    ));
    assert_eq!(said, "done\n");
}

/// A program cannot read the value of a child without joining it.
fn a_task_holds_its_value_where_only_join_reads_it() {
    let source = program("fn peek() -> u64 {\n  with tasks()\n  t = spawn { 1 }\n  t.value\n}\n\nfn main() ! console, alloc {\n  print(\"{peek()}\")\n}\n");
    let errors = refused(&source);
    assert!(errors.contains("private"), "{errors}");
}

fn a_child_that_assigns_a_var_is_refused() {
    let source = program("fn counted() -> u64 {\n  var n: u64 = 0\n  with tasks()\n  t = spawn { n = 7 }\n  join(t)\n  n\n}\n\nfn main() ! console, alloc {\n  print(\"{counted()}\")\n}\n");
    let errors = refused(&source);
    assert!(errors.contains("this child assigns `n`"), "{errors}");
}

fn a_value_used_once_moves_into_the_child_and_a_second_use_is_refused() {
    let items = |first: &str, second: &str| {
        program(&format!(
            "fn two_draws(key: take Key) -> u64 {{\n  (k1, k2) = split(key)\n  with tasks()\n  ta = spawn {{ bits({first}) }}\n  tb = spawn {{ bits({second}) }}\n  wrapping_add(join(ta), join(tb))\n}}\n\nfn main() ! console, alloc {{\n  print(\"{{two_draws(seeded(1))}}\")\n}}\n"
        ))
    };
    let said = run(&items("k1", "k2"));
    assert_eq!(said.trim().parse::<u64>().map(|_| ()), Ok(()));
    let errors = refused(&items("k1", "k1"));
    assert!(errors.contains("`k1` is used here, after it was given away"), "{errors}");
    // The parent that spawned with the key has none left.
    let after = program("fn after(key: take Key) -> u64 {\n  with tasks()\n  ta = spawn { bits(key) }\n  wrapping_add(join(ta), bits(key))\n}\n\nfn main() ! console, alloc {\n  print(\"{after(seeded(1))}\")\n}\n");
    let errors = refused(&after);
    assert!(errors.contains("`key` is used here, after it was given away"), "{errors}");
}

fn a_function_that_spawns_with_no_group_carries_spawn_and_its_caller_that_opens_one_does_not() {
    let source = |effect: &str| {
        program(&format!(
            "fn start(n: u64){effect} {{\n  spawn {{ wrapping_mul(n, 2) }}\n  ()\n}}\n\nfn start_two() {{\n  with tasks()\n  start(1)\n  start(2)\n}}\n\nfn main() ! console, alloc {{\n  start_two()\n  print(\"done\")\n}}\n"
        ))
    };
    assert_eq!(run(&source(" ! spawn")), "done\n");
    let errors = refused(&source(""));
    assert!(errors.contains("`spawn` needs the `spawn` effect, which `start` does not declare"), "{errors}");
    // A caller with no group of its own carries it too.
    let caller = program("fn start(n: u64) ! spawn {\n  spawn { n }\n  ()\n}\n\nfn start_two() {\n  start(1)\n}\n\nfn main() ! console, alloc {\n  start_two()\n}\n");
    let errors = refused(&caller);
    assert!(errors.contains("`start` needs the `spawn` effect"), "{errors}");
}

const CLOCK: &str = "effect clock {\n  fn now() -> u64\n}\n\nfn fake_clock[T](body: fn() -> T ! clock) -> T {\n  var ticks: u64 = 0\n  clock(\n    now: {\n      ticks = wrapping_add(ticks, 1)\n      ticks\n    },\n    body: body,\n  )\n}\n\n";

/// A child reads the clock that was in force where it was spawned, and no
/// two children read the same tick, whatever the order they read in.
fn a_child_runs_under_the_handlers_in_force_where_it_was_spawned_and_its_operations_run_one_at_a_time()
 {
    generate::check(332, 20, |g| (1 + g.below(4), 1 + g.below(4)), |&(children, reads)| {
        let mut s = format!("{CLOCK}fn readings() -> List[u64] ! alloc {{\n  with fake_clock()\n  with tasks()\n");
        let pushes = "    l = push(l, now())\n".repeat(reads);
        for k in 0..children {
            s += &format!("  t{k} = spawn {{\n    var l: List[u64] = []\n{pushes}    l\n  }}\n");
        }
        s += "  var all: List[u64] = []\n";
        for k in 0..children {
            s += &format!("  r{k} = join(t{k})\n  loop i in 0..r{k}.size {{ all = push(all, r{k}[i]) }}\n");
        }
        s += "  all\n}\n\nfn main() ! console, alloc {\n  xs = readings()\n  loop i in 0..xs.size { print(\"{xs[i]}\") }\n}\n";
        let said = run(&program(&s));
        let mut ticks: Vec<u64> = said.lines().map(|l| l.parse().unwrap()).collect();
        ticks.sort();
        let expected: Vec<u64> = (1..=(children * reads) as u64).collect();
        assert_eq!(ticks, expected, "for {children} children reading {reads}");
    });
}

/// The system is asked for the same memory however many children a program
/// spawns that each build a list and give back a number, and a list that a
/// child gives back is built where its parent keeps it.
fn a_child_gives_its_memory_back_when_it_ends_and_its_result_is_built_in_its_parents() {
    let asked = |rounds: u64| {
        let source = program(&format!(
            "fn build(n: u64) -> List[u64] ! alloc {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, wrapping_mul(i, i)) }}\n  acc\n}}\n\nfn total(xs: List[u64]) -> u64 {{\n  var s: u64 = 0\n  loop i in 0..xs.size {{ s = wrapping_add(s, xs[i]) }}\n  s\n}}\n\nfn child_sum(n: u64) -> u64 ! alloc {{\n  with tasks()\n  t = spawn {{ total(build(n)) }}\n  join(t)\n}}\n\nfn child_list(n: u64) -> List[u64] ! alloc {{\n  with tasks()\n  t = spawn {{ build(n) }}\n  join(t)\n}}\n\nfn main() ! console, alloc {{\n  var sum: u64 = 0\n  loop j in 0..{rounds} {{\n    sum = wrapping_add(sum, child_sum(50))\n  }}\n  xs = child_list(10)\n  pad = build(300)\n  print(\"{{sum}} {{total(xs)}} {{xs.size}} {{total(pad)}}\")\n}}\n"
        ));
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let (outcome, requests) = scalar::run_counting(&program, usize::MAX);
        assert_eq!(outcome.status, 0);
        let one: u64 = (0..50u64).map(|i| i * i).sum();
        let list: u64 = (0..10u64).map(|i| i * i).sum();
        let pad: u64 = (0..300u64).map(|i| i * i).sum();
        assert_eq!(
            outcome.stdout,
            format!("{} {list} 10 {pad}\n", one * rounds).into_bytes()
        );
        requests.len()
    };
    let counts: Vec<usize> = [3, 30, 90].iter().map(|r| asked(*r)).collect();
    assert_eq!(counts[0], counts[1], "{counts:?}");
    assert_eq!(counts[1], counts[2], "{counts:?}");
}

fn every_executable_prints_what_the_evaluator_says_when_children_are_joined() {
    let targets = runnable();
    generate::check(333, 6, |g: &mut Gen| (1 + g.below(5), 1 + g.below(5)), |&(children, reads)| {
        let mut s = format!("{CLOCK}fn build(n: u64) -> List[u64] ! alloc {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, wrapping_mul(i, 3)) }}\n  acc\n}}\n\nfn total(xs: List[u64]) -> u64 {{\n  var s: u64 = 0\n  loop i in 0..xs.size {{ s = wrapping_add(s, xs[i]) }}\n  s\n}}\n\nfn readings() -> u64 ! alloc {{\n  with fake_clock()\n  with tasks()\n  var sum: u64 = 0\n");
        for k in 0..children {
            s += &format!("  t{k} = spawn {{ wrapping_add(now(), total(build({}))) }}\n", reads + k);
        }
        for k in 0..children {
            s += &format!("  sum = wrapping_add(sum, wrapping_mul(join(t{k}), {}))\n", k + 1);
        }
        s += "  sum\n}\n\nfn main() ! console, alloc {\n  print(\"{readings()}\")\n}\n";
        let source = program(&s);
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let said = scalar::run(&scalar::passes(&program));
        assert_eq!(said.status, 0);
        for &target in &targets {
            runs_as(source.as_bytes(), target, &said);
        }
    });
}
