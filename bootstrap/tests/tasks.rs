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
        "two_children_of_different_result_types_joined_in_either_order_give_their_own_values",
        two_children_of_different_result_types_joined_in_either_order_give_their_own_values,
    ),
    (
        "a_program_with_no_group_has_the_system_run_each_child_to_its_end_where_it_starts",
        a_program_with_no_group_has_the_system_run_each_child_to_its_end_where_it_starts,
    ),
    (
        "a_handler_of_task_written_in_a_program_is_refused",
        a_handler_of_task_written_in_a_program_is_refused,
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
        "a_function_that_spawns_with_no_group_carries_task_and_its_caller_that_opens_one_does_not",
        a_function_that_spawns_with_no_group_carries_task_and_its_caller_that_opens_one_does_not,
    ),
    (
        "a_child_runs_under_the_handlers_in_force_where_it_was_spawned",
        a_child_runs_under_the_handlers_in_force_where_it_was_spawned,
    ),
    (
        "a_child_under_a_handler_that_holds_state_reads_and_changes_it",
        a_child_under_a_handler_that_holds_state_reads_and_changes_it,
    ),
    (
        "a_child_gives_its_memory_back_when_it_ends_and_its_result_is_built_in_its_parents",
        a_child_gives_its_memory_back_when_it_ends_and_its_result_is_built_in_its_parents,
    ),
    (
        "a_child_is_joined_once_and_a_handle_is_given_away_only_where_it_is_taken",
        a_child_is_joined_once_and_a_handle_is_given_away_only_where_it_is_taken,
    ),
    (
        "a_spawn_written_in_an_argument_or_an_expression_is_joined_as_one_bound_first_is",
        a_spawn_written_in_an_argument_or_an_expression_is_joined_as_one_bound_first_is,
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
const FETCH: &str = "effect web {\n  fn fetch(url: str) -> Result[List[Byte], NetError]\n}\n\nfn fetch_one(url: str) -> Result[List[Byte], NetError] <web, alloc> {\n  with tasks()\n  t = spawn { fetch(url) }\n  join(t)\n}\n\nfn fetch_both(a: str, b: str) -> Result[(List[Byte], List[Byte]), NetError] <web, alloc> {\n  with tasks()\n  ta = spawn { fetch(a) }\n  tb = spawn { fetch(b) }\n  pa = try join(ta)\n  pb = try join(tb)\n  .ok((pa, pb))\n}\n\nfn served[T](body: fn() -> T <web>) -> T <alloc> {\n  web(fetch: fn(url) -> Result[List[Byte], NetError] { if url.size == 0 { .err(.refused) } else { .ok([7, 8]) } }, body: body)\n}\n\n";

fn fetch_one_and_fetch_both_give_what_the_page_says_under_a_handler_of_fetch() {
    let main = "fn main() <console, alloc> {\n  with served()\n  one = fetch_one(\"a\")\n  match one {\n    .ok(page) { print(\"one {page.size}\") }\n    .err(e) { print(\"one failed\") }\n  }\n  both = fetch_both(\"a\", \"bb\")\n  match both {\n    .ok(pages) {\n      (x, y) = pages\n      print(\"both {x.size} {y.size}\")\n    }\n    .err(e) { print(\"both failed\") }\n  }\n  failed = fetch_both(\"a\", \"\")\n  match failed {\n    .ok(pages) { print(\"no\") }\n    .err(e) { print(\"second failed\") }\n  }\n}\n";
    let said = run(&program(&format!("{FETCH}{main}")));
    assert_eq!(said, "one 2\nboth 2 2\nsecond failed\n");
}

/// The text of a program with one function for each `(spawned, joined)`: it
/// spawns `spawned` children, each of which says its number and gives a
/// different value, joins the first `joined` of them, and gives what they gave.
/// Its `main` calls the functions in order and says `end` and what each gave
/// after what its children said.
fn counting(cases: &[(usize, usize)]) -> String {
    let mut s = String::new();
    for (i, &(spawned, joined)) in cases.iter().enumerate() {
        s += &format!("fn group{i}() -> u64 <console, alloc> {{\n  with tasks()\n");
        for k in 0..spawned {
            s += &format!("  t{k} = spawn {{\n    print(\"c{k}\")\n    v: u64 = {}\n    v\n  }}\n", k + 1);
        }
        s += "  var total: u64 = 0\n";
        for k in 0..joined {
            s += &format!("  total = wrapping_add(total, wrapping_mul(join(t{k}), {}))\n", 1000u64.pow(k as u32 % 3));
        }
        s += "  total\n}\n\n";
    }
    s += "fn main() <console, alloc> {\n";
    for i in 0..cases.len() {
        s += &format!("  print(\"end {{group{i}()}}\")\n");
    }
    s += "}\n";
    program(&s)
}

/// What each of the cases says, as `counting` makes them and runs them as one
/// program, which is lowered once: what it says, cut after each `end` line.
fn counted(cases: &[(usize, usize)]) -> Vec<String> {
    let said = run(&counting(cases));
    let mut each: Vec<String> = said.split_inclusive('\n').fold(vec![String::new()], |mut each, line| {
        each.last_mut().unwrap().push_str(line);
        if line.starts_with("end ") {
            each.push(String::new());
        }
        each
    });
    assert_eq!(each.pop().as_deref(), Some(""), "{said}");
    assert_eq!(each.len(), cases.len(), "{said}");
    each
}

/// A child that is joined runs exactly once, no child runs twice, and nothing
/// runs after the block, so the last line is the one the program says after it.
/// The cases are lowered as one program, as lowering the library of tasks takes
/// the time of a case, and a case that shrinking tries is lowered alone.
fn each_child_runs_once_if_it_is_joined_and_none_runs_after_the_block_ends() {
    let draw = |g: &mut Gen| {
        let spawned = 1 + g.below(5);
        (spawned, g.below(spawned + 1))
    };
    let mut distinct: Vec<(usize, usize)> = Vec::new();
    for case in 0..24 {
        let input = generate::sample(330, case, draw);
        if !distinct.contains(&input) {
            distinct.push(input);
        }
    }
    let known: Vec<String> = counted(&distinct);
    generate::check(330, 24, draw, |&(spawned, joined)| {
        let said = match distinct.iter().position(|d| *d == (spawned, joined)) {
            Some(at) => known[at].clone(),
            None => counted(&[(spawned, joined)]).remove(0),
        };
        let lines: Vec<&str> = said.lines().collect();
        let (last, children) = lines.split_last().expect("lines");
        assert!(last.starts_with("end "), "{said} for {spawned} {joined}");
        for k in 0..spawned {
            let ran = children.iter().filter(|l| **l == format!("c{k}")).count();
            assert!(ran <= 1, "{said} for {spawned} {joined}");
            if k < joined {
                assert_eq!(ran, 1, "{said} for {spawned} {joined}");
            }
        }
        assert!(children.iter().all(|l| l.starts_with('c')), "{said}");
        let total: u64 = last["end ".len()..].parse().unwrap();
        let expected: u64 = (0..joined as u32).map(|k| (k as u64 + 1) * 1000u64.pow(k % 3)).sum();
        assert_eq!(total, expected, "{said} for {spawned} {joined}");
    });
}

/// The text of a program with `snapshot` for each pair, and `twice`, whose
/// `main` says what each `snapshot` gives and what `twice` gives, one line each.
fn sharing(cases: &[(u64, u64)]) -> String {
    let mut s = String::new();
    for (i, (a, b)) in cases.iter().enumerate() {
        s += &format!(
            "fn snapshot{i}() -> u64 {{\n  var n: u64 = {a}\n  with tasks()\n  t = spawn {{ wrapping_add(n, 1) }}\n  n = {b}\n  wrapping_add(n, join(t))\n}}\n\n"
        );
    }
    s += "fn twice(x: str) -> (u64, u64) {\n  with tasks()\n  ta = spawn { wrapping_add(x.size, 1) }\n  tb = spawn { wrapping_mul(x.size, 2) }\n  (join(ta), join(tb))\n}\n\nfn main() <console, alloc> {\n  (p, q) = twice(\"abc\")\n";
    for i in 0..cases.len() {
        s += &format!("  print(\"{{snapshot{i}()}} {{p}} {{q}}\")\n");
    }
    s += "}\n";
    program(&s)
}

/// `snapshot`, `twice`, and a function that gives what a child read. The cases
/// are run as one program, and a case that shrinking tries is run alone.
fn an_ordinary_value_is_shared_and_a_var_is_read_as_it_was_at_the_spawn() {
    let draw = |g: &mut Gen| (g.below(1000) as u64, 1 + g.below(1000) as u64);
    let all: Vec<(u64, u64)> = (0..20).map(|case| generate::sample(331, case, draw)).collect();
    let known = run(&sharing(&all));
    let known: Vec<&str> = known.lines().collect();
    assert_eq!(known.len(), all.len(), "{known:?}");
    generate::check(331, 20, draw, |&(a, b)| {
        let alone;
        let said = match all.iter().position(|c| *c == (a, b)) {
            Some(at) => known[at],
            None => {
                alone = run(&sharing(&[(a, b)]));
                alone.trim_end()
            }
        };
        assert_eq!(said, format!("{} 4 6", b + a + 1));
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
        "use std.console (console, print)\n{snapshot}\nfn main() <console, alloc> {{\n  print(\"{{snapshot()}}\")\n}}\n"
    ));
    assert_eq!(said, "7\n");
    let start = page_block("tasks.md", "fn start_two()");
    let said = run(&format!(
        "use std.console (console, print)\n{start}\nfn main() <console, alloc> {{\n  start_two()\n  print(\"done\")\n}}\n"
    ));
    assert_eq!(said, "done\n");
}

/// A program cannot read the value of a child without joining it.
fn a_task_holds_its_value_where_only_join_reads_it() {
    let source = program("fn peek() -> u64 {\n  with tasks()\n  t = spawn { 1 }\n  t.number\n}\n\nfn main() <console, alloc> {\n  print(\"{peek()}\")\n}\n");
    let errors = refused(&source);
    assert!(errors.contains("private"), "{errors}");
}

/// A `Task[T]` is read at its own `T`, however the children are joined.
fn two_children_of_different_result_types_joined_in_either_order_give_their_own_values() {
    generate::check(334, 16, |g| (g.below(1000) as u64, g.below(2) == 0), |&(n, first)| {
        let (a, b) = if first { ("ta", "tb") } else { ("tb", "ta") };
        let source = program(&format!(
            "fn both(n: u64) -> str <alloc> {{\n  with tasks()\n  ta = spawn {{ wrapping_add(n, 1) }}\n  tb = spawn {{ \"word\" }}\n  x = join({a})\n  y = join({b})\n  \"{{x}} {{y}}\"\n}}\n\nfn main() <console, alloc> {{\n  print(both({n}))\n}}\n"
        ));
        let said = run(&source);
        let (number, word) = (n + 1, "word");
        let expected = if first { format!("{number} {word}\n") } else { format!("{word} {number}\n") };
        assert_eq!(said, expected, "for {n}");
    });
}

/// A program that opens no group has `main` carry `task`, and the system's
/// scheduler runs each child to its end where it starts.
fn a_program_with_no_group_has_the_system_run_each_child_to_its_end_where_it_starts() {
    let source = program("fn main() <console, alloc, task> {\n  a = spawn {\n    print(\"a\")\n    v: u64 = 1\n    v\n  }\n  b = spawn {\n    print(\"b\")\n    v: u64 = 2\n    v\n  }\n  y: u64 = join(b)\n  x: u64 = join(a)\n  print(\"{x} {y}\")\n}\n")
        .replace("use std.task (join, spawn, tasks)", "use std.task (join, spawn, task)");
    assert_eq!(run(&source), "a\nb\n1 2\n");
}

/// A handler of `task` that a program writes could give `finish` a number that
/// no `start` gave, so that `join` reads memory that holds no frame: this program
/// built and ended with status 1 and nothing printed before it was refused.
fn a_handler_of_task_written_in_a_program_is_refused() {
    let source = program("fn shift[T](body: fn() -> T <task>) -> T <task> {\n  task(\n    start: fn(work: Frame) -> u64 { 4096 },\n    finish: fn(child: u64) -> Frame { finish(child) },\n    body: body,\n  )\n}\n\nfn main() <console, alloc> {\n  with tasks()\n  with shift()\n  b = spawn { \"seven\" }\n  s: str = join(b)\n  print(s)\n}\n")
        .replace("use std.task (join, spawn, tasks)", "use std.task (Frame, finish, join, spawn, task, tasks)");
    let errors = refused(&source);
    assert!(errors.contains("`task` is handled only by the schedulers of `std.task`"), "{errors}");
}

fn a_child_that_assigns_a_var_is_refused() {
    let source = program("fn counted() -> u64 {\n  var n: u64 = 0\n  with tasks()\n  t = spawn { n = 7 }\n  join(t)\n  n\n}\n\nfn main() <console, alloc> {\n  print(\"{counted()}\")\n}\n");
    let errors = refused(&source);
    assert!(errors.contains("this child assigns `n`"), "{errors}");
}

fn a_value_used_once_moves_into_the_child_and_a_second_use_is_refused() {
    let items = |first: &str, second: &str| {
        program(&format!(
            "fn two_draws(key: take Key) -> u64 {{\n  (k1, k2) = split(key)\n  with tasks()\n  ta = spawn {{ bits({first}) }}\n  tb = spawn {{ bits({second}) }}\n  wrapping_add(join(ta), join(tb))\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{two_draws(seeded(1))}}\")\n}}\n"
        ))
    };
    let said = run(&items("k1", "k2"));
    assert_eq!(said.trim().parse::<u64>().map(|_| ()), Ok(()));
    let errors = refused(&items("k1", "k1"));
    assert!(errors.contains("`k1` is used here, after it was given away"), "{errors}");
    // The parent that spawned with the key has none left.
    let after = program("fn after(key: take Key) -> u64 {\n  with tasks()\n  ta = spawn { bits(key) }\n  wrapping_add(join(ta), bits(key))\n}\n\nfn main() <console, alloc> {\n  print(\"{after(seeded(1))}\")\n}\n");
    let errors = refused(&after);
    assert!(errors.contains("`key` is used here, after it was given away"), "{errors}");
}

fn a_function_that_spawns_with_no_group_carries_task_and_its_caller_that_opens_one_does_not() {
    let source = |effect: &str| {
        program(&format!(
            "fn start(n: u64){effect} {{\n  spawn {{ wrapping_mul(n, 2) }}\n  ()\n}}\n\nfn start_two() {{\n  with tasks()\n  start(1)\n  start(2)\n}}\n\nfn main() <console, alloc> {{\n  start_two()\n  print(\"done\")\n}}\n"
        ))
        .replace("use std.task (join, spawn, tasks)", "use std.task (join, spawn, task, tasks)")
    };
    assert_eq!(run(&source(" <task>")), "done\n");
    let errors = refused(&source(""));
    assert!(errors.contains("`spawn` needs the `task` effect, which `start` does not declare"), "{errors}");
    // A caller with no group of its own carries it too.
    let caller = program("fn start(n: u64) <task> {\n  spawn { n }\n  ()\n}\n\nfn start_two() {\n  start(1)\n}\n\nfn main() <console, alloc> {\n  start_two()\n}\n")
        .replace("use std.task (join, spawn, tasks)", "use std.task (join, spawn, task, tasks)");
    let errors = refused(&caller);
    assert!(errors.contains("`start` needs the `task` effect"), "{errors}");
}

const CLOCK: &str = "effect clock {\n  fn now() -> u64\n}\n\nfn fake_clock[T](body: fn() -> T <clock>) -> T {\n  var ticks: u64 = 0\n  clock(\n    now: {\n      ticks = wrapping_add(ticks, 1)\n      ticks\n    },\n    body: body,\n  )\n}\n\nfn fixed[T](tick: u64, body: fn() -> T <clock>) -> T {\n  clock(now: { tick }, body: body)\n}\n\n";

/// A child reads the clock that was in force where it was spawned, which a
/// handler that holds no state gives it.
fn a_child_runs_under_the_handlers_in_force_where_it_was_spawned() {
    generate::check(332, 20, |g| (1 + g.below(4), g.below(1000) as u64), |&(children, tick)| {
        let mut s = format!("{CLOCK}fn readings() -> List[u64] <alloc> {{\n  with fixed({tick})\n  with tasks()\n");
        for k in 0..children {
            s += &format!("  t{k} = spawn {{ now() }}\n");
        }
        s += "  var all: List[u64] = []\n";
        for k in 0..children {
            s += &format!("  all = push(all, join(t{k}))\n");
        }
        s += "  all\n}\n\nfn main() <console, alloc> {\n  xs = readings()\n  loop i in 0..xs.size { print(\"{xs[i]}\") }\n}\n";
        let said = run(&program(&s));
        let ticks: Vec<u64> = said.lines().map(|l| l.parse().unwrap()).collect();
        assert_eq!(ticks, vec![tick; children], "for {children} children");
    });
}

/// A child under a handler that holds state asks it, and changes what the next
/// asks: the state goes through the poll, and is never copied into the child.
fn a_child_under_a_handler_that_holds_state_reads_and_changes_it() {
    let source = program(&format!(
        "{CLOCK}fn readings() -> u64 {{\n  with fake_clock()\n  with tasks()\n  t = spawn {{ now() }}\n  u = spawn {{ now() }}\n  wrapping_add(wrapping_mul(join(t), 10), join(u))\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{readings()}}\")\n}}\n"
    ));
    assert_eq!(run(&source), "12\n");
}

/// The system is asked for the same memory however many children a program
/// spawns that each build a list and give back a number, and a list that a
/// child gives back is built where its parent keeps it.
fn a_child_gives_its_memory_back_when_it_ends_and_its_result_is_built_in_its_parents() {
    let asked = |rounds: u64| {
        let source = program(&format!(
            "fn build(n: u64) -> List[u64] <alloc> {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, wrapping_mul(i, i)) }}\n  acc\n}}\n\nfn total(xs: List[u64]) -> u64 {{\n  var s: u64 = 0\n  loop i in 0..xs.size {{ s = wrapping_add(s, xs[i]) }}\n  s\n}}\n\nfn child_sum(n: u64) -> u64 <alloc> {{\n  with tasks()\n  t = spawn {{ total(build(n)) }}\n  join(t)\n}}\n\nfn child_list(n: u64) -> List[u64] <alloc> {{\n  with tasks()\n  t = spawn {{ build(n) }}\n  join(t)\n}}\n\nfn main() <console, alloc> {{\n  var sum: u64 = 0\n  loop j in 0..{rounds} {{\n    sum = wrapping_add(sum, child_sum(50))\n  }}\n  xs = child_list(10)\n  pad = build(300)\n  print(\"{{sum}} {{total(xs)}} {{xs.size}} {{total(pad)}}\")\n}}\n"
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
        let mut s = format!("{CLOCK}fn build(n: u64) -> List[u64] <alloc> {{\n  var acc: List[u64] = []\n  loop i in 0..n {{ acc = push(acc, wrapping_mul(i, 3)) }}\n  acc\n}}\n\nfn total(xs: List[u64]) -> u64 {{\n  var s: u64 = 0\n  loop i in 0..xs.size {{ s = wrapping_add(s, xs[i]) }}\n  s\n}}\n\nfn readings() -> u64 <alloc> {{\n  with fixed(7)\n  with tasks()\n  var sum: u64 = 0\n");
        for k in 0..children {
            s += &format!("  t{k} = spawn {{ wrapping_add(now(), total(build({}))) }}\n", reads + k);
        }
        for k in 0..children {
            s += &format!("  sum = wrapping_add(sum, wrapping_mul(join(t{k}), {}))\n", k + 1);
        }
        s += "  sum\n}\n\nfn main() <console, alloc> {\n  print(\"{readings()}\")\n}\n";
        let source = program(&s);
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let said = scalar::run(&scalar::passes(&program));
        assert_eq!(said.status, 0);
        for &target in &targets {
            runs_as(source.as_bytes(), target, &said);
        }
    });
}

/// A `Task` has no `Copy`, so a second `join` of one handle is a use after it
/// was given away, which this program did not say before: it read the frame of
/// a child that had ended. A function that lends the handle may not join it,
/// and one that takes it may.
fn a_child_is_joined_once_and_a_handle_is_given_away_only_where_it_is_taken() {
    let twice = program("fn main() <console, alloc> {\n  with tasks()\n  t = spawn { \"seven\" }\n  a = join(t)\n  b = join(t)\n  print(\"{a} {b}\")\n}\n");
    let errors = refused(&twice);
    assert!(errors.contains("`t` is used here, after it was given away"), "{errors}");
    let once = |param: &str| {
        program(&format!(
            "fn wait(t: {param}Task[str]) -> str <task> {{\n  join(t)\n}}\n\nfn main() <console, alloc> {{\n  with tasks()\n  t = spawn {{ \"seven\" }}\n  print(wait(t))\n}}\n"
        ))
        .replace("use std.task (join, spawn, tasks)", "use std.task (Task, join, spawn, task, tasks)")
    };
    assert_eq!(run(&once("take ")), "seven\n");
    let errors = refused(&once(""));
    assert!(errors.contains("`t` is lent to `wait`"), "{errors}");
}

/// `join(spawn { e })` gave "every task waits, and none can run": the state a
/// scheduler holds, which `spawn` changes, was read for the call of `join`
/// before its argument was worked out, so `join` was given the state without
/// the child. A child started where it is joined, as an argument of any call or
/// a part of any expression, gives what the same program gives with the handle
/// bound first, under a group and under a scheduler of a test of any seed.
fn a_spawn_written_in_an_argument_or_an_expression_is_joined_as_one_bound_first_is() {
    generate::check(1030, 3, |g: &mut Gen| (g.below(1000) as u64, g.below(1000) as u64, g.below(6) as u64), |&(a, b, seed)| {
        let sum = a + b;
        let shapes = [
            ("fn pair(x: take Task[u64], y: take Task[u64]) -> u64 <task> {{\n  wrapping_add(join(x), join(y))\n}}\n\n", format!("pair(spawn {{ num({a}) }}, spawn {{ num({b}) }})"), sum),
            ("", format!("wrapping_add(join(spawn {{ num({a}) }}), join(spawn {{ num({b}) }}))"), sum),
            ("", format!("join(spawn {{ num({a}) }})"), a),
            ("fn twice(x: take Task[u64]) -> u64 <task> {{\n  wrapping_add(join(x), 1)\n}}\n\n", format!("twice(spawn {{ join(spawn {{ num({b}) }}) }})"), b + 1),
        ];
        for (items, call, expected) in shapes {
            for opening in ["with tasks()".to_string(), format!("with test_scheduler({seed})")] {
                let source = program(&format!(
                    "fn num(n: u64) -> u64 {{\n  n\n}}\n\n{}fn run() -> u64 <alloc, diverge> {{\n  {opening}\n  v: u64 = {call}\n  v\n}}\n\nfn main() <console, alloc, diverge> {{\n  print(\"{{run()}}\")\n}}\n",
                    items.replace("{{", "{").replace("}}", "}")
                ))
                .replace("use std.task (join, spawn, tasks)", "use std.task (Task, join, spawn, task, tasks, test_scheduler)");
                assert_eq!(run(&source), format!("{expected}\n"), "for {source}");
            }
        }
    });
}
