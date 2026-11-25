//! The range of a `var` with no stated type is the smallest that holds every
//! value it is given, solved in the order the body is read.
//!
//! Every generator knows the values it gives each `var`, so what it expects
//! comes from there and shares nothing with the compiler.

use smith::scalar::{self, Outcome};
use smith::target::Target;
use smith::{check, compile, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;

pub const ONCE: &[Test] = &[
    (
        "a_var_with_no_range_written_is_what_stating_the_range_its_values_need_is",
        a_var_with_no_range_written_is_what_stating_the_range_its_values_need_is,
    ),
    (
        "a_use_that_needs_less_than_the_inferred_range_says_what_was_inferred_from_what_and_where",
        a_use_that_needs_less_than_the_inferred_range_says_what_was_inferred_from_what_and_where,
    ),
    (
        "a_var_a_loop_changes_by_itself_is_refused_with_the_range_to_state",
        a_var_a_loop_changes_by_itself_is_refused_with_the_range_to_state,
    ),
    (
        "a_var_given_a_loops_counter_is_the_range_of_the_counter_and_a_use_of_it_that_fits_is_accepted",
        a_var_given_a_loops_counter_is_the_range_of_the_counter_and_a_use_of_it_that_fits_is_accepted,
    ),
    (
        "a_var_that_a_loop_gives_itself_plus_its_counter_is_refused_at_the_assignment_with_the_range_to_state",
        a_var_that_a_loop_gives_itself_plus_its_counter_is_refused_at_the_assignment_with_the_range_to_state,
    ),
    (
        "a_var_changed_by_itself_in_a_loop_is_refused_beside_every_other_error_of_the_program",
        a_var_changed_by_itself_in_a_loop_is_refused_beside_every_other_error_of_the_program,
    ),
    (
        "every_generated_body_that_gives_a_var_loop_counters_and_numbers_is_accepted_and_prints_what_the_steps_give",
        every_generated_body_that_gives_a_var_loop_counters_and_numbers_is_accepted_and_prints_what_the_steps_give,
    ),
    (
        "a_list_or_an_option_written_with_nothing_is_what_stating_the_type_its_uses_need_is",
        a_list_or_an_option_written_with_nothing_is_what_stating_the_type_its_uses_need_is,
    ),
    (
        "a_list_and_an_option_written_with_nothing_are_settled_by_what_they_are_given_and_used_for",
        a_list_and_an_option_written_with_nothing_are_settled_by_what_they_are_given_and_used_for,
    ),
    (
        "a_list_written_with_nothing_that_nothing_settles_is_refused_with_where_to_state_it",
        a_list_written_with_nothing_that_nothing_settles_is_refused_with_where_to_state_it,
    ),
    (
        "the_time_to_check_a_body_grows_with_its_size_and_no_faster",
        the_time_to_check_a_body_grows_with_its_size_and_no_faster,
    ),
];

/// One line of the body of a function.
#[derive(Clone, Debug)]
enum Step {
    /// `v = a`.
    Set(usize, i128),
    /// `if c < k { v = a }`.
    IfSet(usize, i128, i128),
    /// `if c < k { v = a } else { v = b }`.
    Either(usize, i128, i128, i128),
    /// `v = v + d`, which is written only where nothing lies between it and
    /// the line that last gave `v` a value.
    Grow(usize, i128),
    /// `need(v)`, a call of a function whose parameter is `0..30`.
    Use(usize),
}

#[derive(Debug)]
struct Program {
    /// What the `var`s are first given.
    start: [i128; 2],
    steps: Vec<Step>,
}

const NAMES: [&str; 2] = ["n", "m"];

fn program(g: &mut Gen) -> Program {
    let count = 1 + g.below(6);
    let mut exact = [true, true];
    let mut steps = Vec::new();
    let start = [g.below(6) as i128, g.below(6) as i128];
    for _ in 0..count {
        let v = g.below(2);
        let value = g.below(40) as i128;
        let step = match g.below(5) {
            0 => {
                exact[v] = true;
                Step::Set(v, value)
            }
            1 => {
                exact[v] = false;
                Step::IfSet(v, 1 + g.below(4) as i128, value)
            }
            2 => {
                exact[v] = false;
                Step::Either(v, 1 + g.below(4) as i128, value, g.below(40) as i128)
            }
            3 if exact[v] => Step::Grow(v, 1 + g.below(3) as i128),
            3 => {
                exact[v] = true;
                Step::Set(v, value)
            }
            _ => Step::Use(v),
        };
        steps.push(step);
    }
    Program { start, steps }
}

/// The range each `var` has, from what the steps give it, read in order:
/// the least and greatest value of each, with the greatest not included.
fn ranges(p: &Program) -> [(i128, i128); 2] {
    let mut total = [(p.start[0], p.start[0]), (p.start[1], p.start[1])];
    let mut now = p.start;
    let grow = |v: usize, at: i128, total: &mut [(i128, i128); 2]| {
        total[v] = (total[v].0.min(at), total[v].1.max(at));
    };
    for s in &p.steps {
        match s {
            Step::Set(v, a) => {
                now[*v] = *a;
                grow(*v, *a, &mut total);
            }
            Step::IfSet(v, _, a) => grow(*v, *a, &mut total),
            Step::Either(v, _, a, b) => {
                grow(*v, *a, &mut total);
                grow(*v, *b, &mut total);
            }
            Step::Grow(v, d) => {
                now[*v] += d;
                grow(*v, now[*v], &mut total);
            }
            Step::Use(_) => {}
        }
    }
    total.map(|(lo, hi)| (lo, hi + 1))
}

/// The program, with each `var` written as the range `stated` gives it, or
/// with no range written.
fn source(p: &Program, stated: Option<[(i128, i128); 2]>) -> String {
    let mut out = String::from("fn need(x: 0..30) -> 0..30 { x }\n\nfn f(c: Byte) {\n");
    for v in 0..2 {
        match stated {
            Some(r) => out.push_str(&format!(
                "  var {}: {}..{} = {}\n",
                NAMES[v], r[v].0, r[v].1, p.start[v]
            )),
            None => out.push_str(&format!("  var {} = {}\n", NAMES[v], p.start[v])),
        }
    }
    for s in &p.steps {
        let line = match s {
            Step::Set(v, a) => format!("{} = {a}", NAMES[*v]),
            Step::IfSet(v, k, a) => format!("if c < {k} {{ {} = {a} }}", NAMES[*v]),
            Step::Either(v, k, a, b) => format!(
                "if c < {k} {{ {0} = {a} }} else {{ {0} = {b} }}",
                NAMES[*v]
            ),
            Step::Grow(v, d) => format!("{0} = {0} + {d}", NAMES[*v]),
            Step::Use(v) => format!("need({})", NAMES[*v]),
        };
        out.push_str(&format!("  {line}\n"));
    }
    out.push_str("}\n\nfn main() {\n  f(1)\n}\n");
    with_header(out)
}

/// A program with a `var` that no range is written for is accepted exactly
/// when the same program, with the range its values need written, is, and is
/// the same program when it is: the same executable, and the same errors.
fn a_var_with_no_range_written_is_what_stating_the_range_its_values_need_is() {
    generate::check(61, 40, program, |p| {
        let (open, closed) = (source(p, None), source(p, Some(ranges(p))));
        for target in [Target::ALL[0], Target::ALL[Target::ALL.len() - 1]] {
            let (a, b) = (compile(open.as_bytes(), target), compile(closed.as_bytes(), target));
            assert_eq!(
                a.is_ok(),
                b.is_ok(),
                "{a:?} for\n{open}\nbut {b:?} for\n{closed}"
            );
            if let (Ok(a), Ok(b)) = (&a, &b) {
                assert!(a == b, "the executables differ for\n{open}\nand\n{closed}");
            }
        }
    });
}

/// The line of a statement in the source the generator wrote, which is the
/// line of its text.
fn line_of(source: &str, text: &str) -> usize {
    source
        .lines()
        .position(|l| l.trim() == text)
        .unwrap_or_else(|| panic!("`{text}` in\n{source}"))
        + 1
}

#[derive(Debug)]
struct Narrow {
    /// What the `var` is first given, then in each arm of an `if`.
    start: i128,
    then: i128,
    otherwise: i128,
    /// What the use needs, which is at most the greater of the arms.
    limit: i128,
}

fn narrow(g: &mut Gen) -> Narrow {
    let start = g.below(5) as i128;
    let then = start + 1 + g.below(5) as i128;
    let otherwise = then + 1 + g.below(5) as i128;
    Narrow {
        start,
        then,
        otherwise,
        limit: 1 + g.below(otherwise as usize) as i128,
    }
}

/// A `var` given a value in each arm of an `if` has the range of the least
/// to the greatest of them, and passing it to a parameter whose range is below
/// the greatest is refused with the range inferred, the assignment that set its
/// top, and the use that needs less, each with its line.
fn a_use_that_needs_less_than_the_inferred_range_says_what_was_inferred_from_what_and_where() {
    generate::check(62, 40, narrow, |n| {
        let top = n.otherwise;
        let limit = n.limit;
        let source = with_header(format!(
            "fn need(x: 0..{limit}) -> 0..{limit} {{ x }}\n\n\
             fn f(c: Byte) {{\n  var x = {}\n  if c < 2 {{ x = {} }} else {{ x = {} }}\n  need(x)\n}}\n\n\
             fn main() {{\n  f(1)\n}}\n",
            n.start, n.then, n.otherwise
        ));
        let errors = lower(source.as_bytes()).expect_err("a use that needs less");
        let top_line = line_of(&source, &format!("if c < 2 {{ x = {} }} else {{ x = {} }}", n.then, n.otherwise));
        let use_line = line_of(&source, "need(x)");
        let start_line = line_of(&source, &format!("var x = {}", n.start));
        let said = errors
            .iter()
            .find(|e| e.pos.line == use_line)
            .unwrap_or_else(|| panic!("{errors:?} for\n{source}"));
        let range = format!("{}..{}", n.start.min(n.then), top + 1);
        for part in [
            "inferred".to_string(),
            format!("`{range}`"),
            format!("`x = {top}` at line {top_line}"),
            format!("at line {start_line}"),
            format!("the use at line {use_line}"),
        ] {
            assert!(said.message.contains(&part), "no `{part}` in {said:?} for\n{source}");
        }
    });
}

/// A `var` that a loop gives a value that depends on the `var` itself has no
/// range inferred yet: the error says that it is changed by itself in the loop
/// and that its range is not inferred, and the fix states a range.
fn a_var_a_loop_changes_by_itself_is_refused_with_the_range_to_state() {
    for step in ["acc = acc + i", "acc = acc + 1"] {
        let source = with_header(format!(
            "fn sum(n: 0..11) -> 0..101 {{\n  var acc = 0\n  loop i in 0..n {{\n    {step}\n  }}\n  acc\n}}\n\n\
             fn main() {{\n  x = sum(3)\n}}\n"
        ));
        let errors = lower(source.as_bytes()).expect_err("a var changed by itself");
        let at = line_of(&source, step);
        let said = errors
            .iter()
            .find(|e| e.pos.line == at)
            .unwrap_or_else(|| panic!("{errors:?} for\n{source}"));
        assert!(
            said.message.contains("`acc`")
                && said.message.contains("changed by itself in this loop")
                && said.message.contains("not inferred"),
            "{said:?}"
        );
        let fix = said.fix.as_deref().expect("a fix");
        assert!(fix.contains("var acc: ") && fix.contains("= 0"), "{fix}");
    }
}

/// A body of `lines` lines, each pair of which gives one of a few `var`s a value
/// and reads it.
fn long_body(lines: usize) -> String {
    let mut out = String::from("fn need(x: 0..60) -> 0..60 { x }\n\nfn f(c: Byte) {\n");
    for v in 0..8 {
        out.push_str(&format!("  var v{v} = {v}\n"));
    }
    for k in 0..lines / 2 {
        let v = k % 8;
        if k % 5 == 0 {
            out.push_str(&format!("  if c < {} {{ v{v} = {} }}\n", k % 7 + 1, k % 50));
        } else {
            out.push_str(&format!("  v{v} = {}\n", k % 50));
        }
        out.push_str(&format!("  need(v{v})\n"));
    }
    out.push_str("}\n\nfn main() {\n  f(1)\n}\n");
    with_header(out)
}

/// What a body of `lines` lines costs to check, which the thread that checks it
/// computed for.
fn cost(lines: usize) -> f64 {
    let source = long_body(lines);
    let start = crate::clock::thread_time();
    let result = check(source.as_bytes());
    let spent = crate::clock::thread_time() - start;
    assert!(result.is_ok(), "{result:?}");
    spent.as_secs_f64()
}

/// Checking a body that has such `var`s takes time in proportion to its size:
/// five times the lines take about five times as long, far less than the
/// twenty-five times that looking at the body again for each line would. The
/// bound leaves room for what a busy machine adds to a longer body.
fn the_time_to_check_a_body_grows_with_its_size_and_no_faster() {
    let small = cost(2000);
    let large = cost(10000);
    assert!(
        large < small * 14.0,
        "2000 lines took {small}s and 10000 took {large}s, which is more than linear"
    );
}

fn run(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// `var last = 0` that a `loop` gives the counter, `last = i`, is the `0` and
/// each `i`, so `small(last)` of a parameter that holds them is accepted and
/// prints the last counter.
fn a_var_given_a_loops_counter_is_the_range_of_the_counter_and_a_use_of_it_that_fits_is_accepted() {
    let source = with_header(
        "fn small(x: 0..10) -> 0..10 { x }\n\n\
         fn main() <console, alloc> {\n  var last = 0\n  loop i in 0..5 { last = i }\n  print(\"{small(last)}\")\n}\n"
            .to_string(),
    );
    assert_eq!(
        run(&source),
        Outcome {
            stdout: b"4\n".to_vec(),
            status: 0
        },
        "{source}"
    );
}

/// `var sum = 0` that a `loop` gives itself plus the counter has no range
/// inferred yet, and is refused at the assignment, with the range to state.
fn a_var_that_a_loop_gives_itself_plus_its_counter_is_refused_at_the_assignment_with_the_range_to_state() {
    let source = with_header(
        "fn main() <console, alloc> {\n  var sum = 0\n  loop i in 0..5 { sum = sum + i }\n  print(\"{sum}\")\n}\n"
            .to_string(),
    );
    let errors = lower(source.as_bytes()).expect_err("a var changed by itself");
    let at = line_of(&source, "loop i in 0..5 { sum = sum + i }");
    let said = errors
        .iter()
        .find(|e| e.pos.line == at)
        .unwrap_or_else(|| panic!("{errors:?} for\n{source}"));
    assert!(
        said.message.contains("`sum`")
            && said.message.contains("changed by itself in this loop")
            && said.message.contains("not inferred"),
        "{said:?}"
    );
    assert!(said.fix.as_deref().is_some_and(|f| f.contains("var sum: ")), "{said:?}");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

/// One step of a body that gives the `var` `v` a value and prints it.
#[derive(Clone, Debug)]
enum Turn {
    /// `loop i in 0..k { v = i }`.
    Count(usize),
    /// `v = a`.
    Set(i128),
    /// `if c < k { v = i }` in a loop of `k` rounds, after which `v` is read.
    Print,
}

fn turns(g: &mut Gen) -> Vec<Turn> {
    let count = 1 + g.below(8);
    (0..count)
        .map(|_| match g.below(3) {
            0 => Turn::Count(1 + g.below(9)),
            1 => Turn::Set(g.below(40) as i128),
            _ => Turn::Print,
        })
        .collect()
}

/// A `var` given loop counters and numbers, and never itself, never makes a
/// program that is refused, and the program prints what the steps give.
fn every_generated_body_that_gives_a_var_loop_counters_and_numbers_is_accepted_and_prints_what_the_steps_give() {
    generate::check(63, 60, turns, |steps| {
        let mut source = String::from("fn main() <console, alloc> {\n  var v = 0\n");
        let (mut held, mut expected) = (0i128, String::new());
        for step in steps {
            match step {
                Turn::Count(k) => {
                    source.push_str(&format!("  loop i in 0..{k} {{ v = i }}\n"));
                    held = *k as i128 - 1;
                }
                Turn::Set(a) => {
                    source.push_str(&format!("  v = {a}\n"));
                    held = *a;
                }
                Turn::Print => {
                    source.push_str("  print(\"{v}\")\n");
                    expected.push_str(&format!("{held}\n"));
                }
            }
        }
        source.push_str("}\n");
        let source = with_header(source);
        assert_eq!(
            run(&source),
            Outcome {
                stdout: expected.into_bytes(),
                status: 0
            },
            "{source}"
        );
    });
}

/// A program that has another mistake, here a missing `alloc`, is refused for
/// the `var` that a loop changes by itself and for the mistake, and the range
/// the `var` has so far is said in neither.
fn a_var_changed_by_itself_in_a_loop_is_refused_beside_every_other_error_of_the_program() {
    let source = with_header(
        "fn main() <console> {\n  var sum = 0\n  loop i in 0..5 { sum = sum + i }\n  print(\"{sum}\")\n}\n"
            .to_string(),
    );
    let errors = lower(source.as_bytes()).expect_err("two mistakes");
    let at = line_of(&source, "loop i in 0..5 { sum = sum + i }");
    let print = line_of(&source, "print(\"{sum}\")");
    assert!(
        errors.iter().any(|e| e.pos.line == at
            && e.message.contains("changed by itself in this loop")
            && e.message.contains("not inferred")),
        "{errors:?} for\n{source}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.pos.line == print && e.message.contains("`alloc`")),
        "{errors:?} for\n{source}"
    );
    assert!(
        errors.iter().all(|e| !e.message.contains("does not fit")),
        "{errors:?} for\n{source}"
    );
}

/// A list that numbers are pushed to, one after another.
#[derive(Debug)]
struct Pushes {
    numbers: Vec<i128>,
}

fn pushes(g: &mut Gen) -> Pushes {
    let count = 1 + g.below(5);
    Pushes {
        numbers: (0..count).map(|_| g.below(300) as i128).collect(),
    }
}

/// `var xs = []` that numbers are pushed to has the element the smallest range
/// that holds them, so the program is the one that states `List` of it, in the
/// same executable on two targets.
fn a_list_or_an_option_written_with_nothing_is_what_stating_the_type_its_uses_need_is() {
    generate::check(64, 12, pushes, |p| {
        let (low, high) = (
            *p.numbers.iter().min().expect("a number"),
            *p.numbers.iter().max().expect("a number"),
        );
        let body = |declared: &str| {
            let mut out = format!("fn main() <console, alloc> {{\n  var xs{declared} = []\n");
            for n in &p.numbers {
                out.push_str(&format!("  xs = push(xs, {n})\n"));
            }
            out.push_str("  print(\"{xs.size}\")\n}\n");
            with_header(format!("use std.list (push)\n\n{out}"))
        };
        let open = body("");
        let closed = body(&format!(": List[{low}..{}]", high + 1));
        for target in [Target::ALL[0], Target::ALL[Target::ALL.len() - 1]] {
            let (a, b) = (
                compile(open.as_bytes(), target),
                compile(closed.as_bytes(), target),
            );
            assert!(a.is_ok(), "{a:?} for\n{open}");
            assert!(
                a == b,
                "the executables differ, or {b:?}, for\n{open}\nand\n{closed}"
            );
        }
    });
}

/// A list used as an argument, a list pushed to, and an option that is given
/// a value in one branch, each written with nothing.
fn a_list_and_an_option_written_with_nothing_are_settled_by_what_they_are_given_and_used_for() {
    let source = with_header(
        "use std.list (push)\n\n\
         fn total(xs: List[0..256]) -> Nat { xs.size }\n\n\
         fn used() -> Nat {\n  xs = []\n  total(xs)\n}\n\n\
         fn pick(c: bool) -> Option[Nat] {\n  var o = .none\n  if c { o = .some(7) }\n  o\n}\n\n\
         fn main() <console, alloc> {\n  var ys = []\n  ys = push(ys, 3)\n  ys = push(ys, 200)\n  \
         print(\"{used()} {ys.size} {pick(true)} {pick(false)}\")\n}\n"
            .to_string(),
    );
    assert_eq!(
        run(&source),
        Outcome {
            stdout: b"0 2 .some(7) .none\n".to_vec(),
            status: 0
        },
        "{source}"
    );
}

/// A list of nothing that nothing in the body gives or uses is refused at the
/// list, with where to state its type.
fn a_list_written_with_nothing_that_nothing_settles_is_refused_with_where_to_state_it() {
    let source = with_header("fn main() {\n  xs = []\n}\n".to_string());
    let errors = lower(source.as_bytes()).expect_err("a list of nothing");
    let at = line_of(&source, "xs = []");
    let said = errors
        .iter()
        .find(|e| e.pos.line == at)
        .unwrap_or_else(|| panic!("{errors:?} for\n{source}"));
    assert!(said.message.contains("an array of nothing"), "{said:?}");
    assert!(said.fix.as_deref().is_some_and(|f| f.contains("xs: List[")), "{said:?}");
}
