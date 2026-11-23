//! The examples of `handlers.md`, each as the page gives it except where the
//! page uses `clamp`, which is not built, and `time`, which the library does
//! not declare yet: what each gives, and the refusals the page names, with a
//! fix that repairs the program when it is applied.

use smith::diag::Error;
use smith::scalar;
use smith::{check, lower};

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "the_clock_that_gives_42_and_the_function_under_it_carry_no_effect",
        the_clock_that_gives_42_and_the_function_under_it_carry_no_effect,
    ),
    (
        "the_clock_that_counts_gives_one_and_then_two_and_carries_no_effect",
        the_clock_that_counts_gives_one_and_then_two_and_carries_no_effect,
    ),
    (
        "the_doubled_clock_asks_the_clock_outside_it_and_is_refused_without_the_effect_it_asks",
        the_doubled_clock_asks_the_clock_outside_it_and_is_refused_without_the_effect_it_asks,
    ),
    (
        "the_loud_clock_prints_when_asked_and_is_refused_without_console_and_the_fix_repairs_it",
        the_loud_clock_prints_when_asked_and_is_refused_without_console_and_the_fix_repairs_it,
    ),
    (
        "two_readings_of_two_children_under_the_clock_that_counts_are_one_and_two_in_either_order",
        two_readings_of_two_children_under_the_clock_that_counts_are_one_and_two_in_either_order,
    ),
    (
        "two_pairs_of_two_children_under_the_clock_that_counts_are_the_four_ticks_each_once_for_every_seed",
        two_pairs_of_two_children_under_the_clock_that_counts_are_the_four_ticks_each_once_for_every_seed,
    ),
    (
        "one_function_that_asks_is_answered_by_each_handler_it_runs_under",
        one_function_that_asks_is_answered_by_each_handler_it_runs_under,
    ),
];

/// What every example starts with: the effect, which the page takes from the
/// library.
const HEAD: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\n\neffect time {\n  fn now() -> u64\n}\n\n";

const FIXED: &str =
    "fn fixed_clock[T](body: fn() -> T ! time) -> T {\n  time(now: { 42 }, body: body)\n}\n\n";

const FAKE: &str = "fn fake_clock[T](body: fn() -> T ! time) -> T {\n  var ticks: u64 = 0\n  time(\n    now: {\n      ticks = wrapping_add(ticks, 1)\n      ticks\n    },\n    body: body,\n  )\n}\n\n";

const DOUBLED: &str = "fn doubled_clock[T](body: fn() -> T ! time) -> T ! time {\n  time(now: { wrapping_mul(now(), 2) }, body: body)\n}\n\n";

const LOUD: &str = "fn loud_clock[T](body: fn() -> T ! time) -> T ! console {\n  time(\n    now: {\n      print(\"asked\")\n      7\n    },\n    body: body,\n  )\n}\n\n";

fn errors(source: &str) -> Vec<Error> {
    check(source.as_bytes()).err().unwrap_or_default()
}

fn prints(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    String::from_utf8(scalar::run(&program).stdout).expect("text")
}

/// `fixed_clock` and `reading`: `reading()` is `42` and carries no effect, so
/// `main` lists none for it.
fn the_clock_that_gives_42_and_the_function_under_it_carry_no_effect() {
    let source = format!(
        "{HEAD}{FIXED}fn reading() -> u64 {{\n  with fixed_clock()\n  now()\n}}\n\n\
         fn main() ! console, alloc {{\n  print(\"{{reading()}}\")\n}}\n"
    );
    assert_eq!(prints(&source), "42\n");
}

/// `fake_clock` and `two_readings`: each reading is one tick later.
fn the_clock_that_counts_gives_one_and_then_two_and_carries_no_effect() {
    let source = format!(
        "{HEAD}{FAKE}fn two_readings() -> (u64, u64) {{\n  with fake_clock()\n  (now(), now())\n}}\n\n\
         fn main() ! console, alloc {{\n  print(\"{{two_readings()}}\")\n}}\n"
    );
    assert_eq!(prints(&source), "(1, 2)\n");
}

/// `doubled_clock` asks the clock outside it, so `doubled()` is `84`, and
/// without `! time` it is refused, naming `now`, `time`, and `doubled_clock`.
fn the_doubled_clock_asks_the_clock_outside_it_and_is_refused_without_the_effect_it_asks() {
    let program = |doubled: &str| {
        format!(
            "{HEAD}{FIXED}{doubled}fn doubled() -> u64 {{\n  with fixed_clock()\n  with doubled_clock()\n  now()\n}}\n\n\
             fn main() ! console, alloc {{\n  print(\"{{doubled()}}\")\n}}\n"
        )
    };
    assert_eq!(prints(&program(DOUBLED)), "84\n");
    let source = program(&DOUBLED.replace(" -> T ! time {", " -> T {"));
    let refused = errors(&source);
    assert_eq!(refused.len(), 1, "{refused:?}");
    for name in ["`now`", "`time`", "`doubled_clock`"] {
        assert!(refused[0].message.contains(name), "{name} in {refused:?}");
    }
}

/// `loud_clock` prints each time it is asked, and without `! console` it is
/// refused where it prints, naming `print`, `console`, and `loud_clock`; the fix
/// it gives, written in place of the signature, makes the program accepted.
fn the_loud_clock_prints_when_asked_and_is_refused_without_console_and_the_fix_repairs_it() {
    let program = |loud: &str| {
        format!(
            "{HEAD}{loud}fn main() ! console {{\n  with loud_clock()\n  x = now()\n}}\n"
        )
    };
    assert_eq!(prints(&program(LOUD)), "asked\n");
    let without = LOUD.replace(" -> T ! console {", " -> T {");
    let source = program(&without);
    let refused = errors(&source);
    assert_eq!(refused.len(), 1, "{refused:?}");
    for name in ["`print`", "`console`", "`loud_clock`"] {
        assert!(refused[0].message.contains(name), "{name} in {refused:?}");
    }
    // The fix is written in backticks after "write", and is the signature.
    let fix = refused[0].fix.as_deref().expect("a fix");
    let written = fix
        .strip_prefix("write `")
        .and_then(|f| f.strip_suffix('`'))
        .expect("write and a signature");
    assert!(written.contains("! time"), "the fix keeps what the parameter carries: {fix}");
    let head = "fn loud_clock[T](body: fn() -> T ! time) -> T {";
    let repaired = source.replace(head, &format!("{written} {{"));
    let found = errors(&repaired);
    assert!(found.is_empty(), "{found:?} for\n{repaired}");
}

/// `report` runs under `fixed(1)` and under `fixed(2)`, and is answered by each.
fn one_function_that_asks_is_answered_by_each_handler_it_runs_under() {
    let source = format!(
        "{HEAD}fn fixed[T](tick: u64, body: fn() -> T ! time) -> T {{\n  time(now: {{ tick }}, body: body)\n}}\n\n\
         fn report() -> u64 ! time {{ now() }}\n\n\
         fn at_one() -> u64 {{\n  with fixed(1)\n  report()\n}}\n\n\
         fn at_two() -> u64 {{\n  with fixed(2)\n  report()\n}}\n\n\
         fn main() ! console, alloc {{\n  print(\"{{at_one()}}\")\n  print(\"{{at_two()}}\")\n}}\n"
    );
    assert_eq!(prints(&source), "1\n2\n");
}

/// What `task-handlers.md` gives, with `test_scheduler` for the scheduler so that a
/// seed picks the order the children run in.
const TASKS: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\nuse std.task (join, spawn, test_scheduler)\n\neffect time {\n  fn now() -> u64\n}\n\n";

/// `two_readings` of `task-handlers.md`: each child asks the clock that counts,
/// and the two ticks are one and two, whichever child runs first.
fn two_readings_of_two_children_under_the_clock_that_counts_are_one_and_two_in_either_order() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=12u64 {
        let source = format!(
            "{TASKS}{FAKE}fn two_readings() -> (u64, u64) ! alloc, diverge {{\n  with fake_clock()\n  with test_scheduler({seed})\n  ta = spawn {{ now() }}\n  tb = spawn {{ now() }}\n  (join(ta), join(tb))\n}}\n\n\
             fn main() ! console, alloc, diverge {{\n  print(\"{{two_readings()}}\")\n}}\n"
        );
        let said = prints(&source);
        assert!(said == "(1, 2)\n" || said == "(2, 1)\n", "seed {seed}: {said}");
        seen.insert(said);
    }
    assert_eq!(seen.len(), 2, "some seed runs each child first: {seen:?}");
}

/// `two_pairs` of `task-handlers.md`: the four readings are the ticks one to four,
/// each once, whatever way the two children's readings interleave.
fn two_pairs_of_two_children_under_the_clock_that_counts_are_the_four_ticks_each_once_for_every_seed() {
    for seed in 1..=12u64 {
        let source = format!(
            "{TASKS}{FAKE}fn two_pairs() -> ((u64, u64), (u64, u64)) ! alloc, diverge {{\n  with fake_clock()\n  with test_scheduler({seed})\n  ta = spawn {{ (now(), now()) }}\n  tb = spawn {{ (now(), now()) }}\n  (join(ta), join(tb))\n}}\n\n\
             fn main() ! console, alloc, diverge {{\n  ((a, b), (c, d)) = two_pairs()\n  print(\"{{wrapping_add(wrapping_mul(a, 1000), wrapping_add(wrapping_mul(b, 100), wrapping_add(wrapping_mul(c, 10), d)))}}\")\n}}\n"
        );
        let said = prints(&source);
        let digits: Vec<char> = said.trim().chars().collect();
        let mut sorted = digits.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec!['1', '2', '3', '4'], "seed {seed}: {said}");
        // Each child's own readings come in order.
        assert!(digits[0] < digits[1] && digits[2] < digits[3], "seed {seed}: {said}");
    }
}
