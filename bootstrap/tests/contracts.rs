//! Generators and properties of `requires` and `ensures`, which `arith`
//! checks when the program is built: a call compiles exactly when the
//! arguments satisfy what the function requires, a function compiles exactly
//! when what it returns satisfies what it ensures, and neither adds an
//! instruction to a program.
//!
//! Every generator works out the answer by trying the values or with the
//! arithmetic of Rust on 128 bits, so it shares nothing with the compiler.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::{INTS, Int, compare, with_header};
use crate::programs::pos_after;

pub const ONCE: &[Test] = &[
    (
        "a_call_compiles_exactly_when_its_arguments_satisfy_every_requires_part_1_of_2",
        || a_call_compiles_exactly_when_its_arguments_satisfy_every_requires(0),
    ),
    (
        "a_call_compiles_exactly_when_its_arguments_satisfy_every_requires_part_2_of_2",
        || a_call_compiles_exactly_when_its_arguments_satisfy_every_requires(1),
    ),
    (
        "a_requires_that_calls_a_predicate_is_owed_exactly_as_the_expression_it_returns_is_part_1_of_2",
        || a_requires_that_calls_a_predicate_is_owed_exactly_as_the_expression_it_returns_is(0),
    ),
    (
        "a_requires_that_calls_a_predicate_is_owed_exactly_as_the_expression_it_returns_is_part_2_of_2",
        || a_requires_that_calls_a_predicate_is_owed_exactly_as_the_expression_it_returns_is(1),
    ),
    (
        "inside_a_function_its_requires_are_facts",
        inside_a_function_its_requires_are_facts,
    ),
    (
        "a_function_compiles_exactly_when_what_it_returns_satisfies_every_ensures",
        a_function_compiles_exactly_when_what_it_returns_satisfies_every_ensures,
    ),
    (
        "after_a_call_what_it_ensures_is_a_fact_so_calls_chain_exactly_as_far_as_the_numbers_allow_part_1_of_2",
        || after_a_call_what_it_ensures_is_a_fact_so_calls_chain_exactly_as_far_as_the_numbers_allow(0),
    ),
    (
        "after_a_call_what_it_ensures_is_a_fact_so_calls_chain_exactly_as_far_as_the_numbers_allow_part_2_of_2",
        || after_a_call_what_it_ensures_is_a_fact_so_calls_chain_exactly_as_far_as_the_numbers_allow(1),
    ),
    (
        "an_ensures_on_a_function_that_does_not_name_its_result_is_an_error_that_says_so",
        an_ensures_on_a_function_that_does_not_name_its_result_is_an_error_that_says_so,
    ),
    (
        "a_function_with_a_requires_is_refused_wherever_it_is_used_as_a_value",
        a_function_with_a_requires_is_refused_wherever_it_is_used_as_a_value,
    ),
    (
        "contracts_add_no_instruction_to_a_program_that_compiles_without_them",
        contracts_add_no_instruction_to_a_program_that_compiles_without_them,
    ),
    (
        "a_function_that_changes_a_parameter_compiles_exactly_when_what_it_does_to_it_is_what_it_ensures_of_old",
        a_function_that_changes_a_parameter_compiles_exactly_when_what_it_does_to_it_is_what_it_ensures_of_old,
    ),
    (
        "after_a_call_that_changes_a_place_what_it_ensures_of_old_is_known_so_calls_chain_exactly_as_far_as_the_requires_allows",
        after_a_call_that_changes_a_place_what_it_ensures_of_old_is_known_so_calls_chain_exactly_as_far_as_the_requires_allows,
    ),
    (
        "old_is_refused_outside_an_ensures_and_of_anything_but_a_parameter_marked_mut",
        old_is_refused_outside_an_ensures_and_of_anything_but_a_parameter_marked_mut,
    ),
];

pub const COMPARISONS: [&str; 6] = ["<", "<=", ">", ">=", "==", "!="];

fn run(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    scalar::run(&program)
}

/// A value of the type, at one of its limits or next to one, or between.
pub fn near(g: &mut Gen, ty: Int) -> i128 {
    let d = g.below(5) as i128 - 2;
    match g.below(3) {
        0 => (ty.min() + d.abs()).min(ty.max()),
        1 => (ty.max() - d.abs()).max(ty.min()),
        _ => (d + 3).clamp(ty.min(), ty.max()),
    }
}

/// A statement about the variable `x`, written out, with the function that
/// says for which values it holds.
pub type Statement = (String, Box<dyn Fn(i128) -> bool>);

pub fn statement(g: &mut Gen, ty: Int, x: &str, depth: usize) -> Statement {
    if depth > 0 && g.one_in(2) {
        let (a, f) = statement(g, ty, x, depth - 1);
        return match g.below(3) {
            0 => (format!("not ({a})"), Box::new(move |v| !f(v))),
            1 => {
                let (b, h) = statement(g, ty, x, depth - 1);
                (format!("({a}) and ({b})"), Box::new(move |v| f(v) && h(v)))
            }
            _ => {
                let (b, h) = statement(g, ty, x, depth - 1);
                (format!("({a}) or ({b})"), Box::new(move |v| f(v) || h(v)))
            }
        };
    }
    let mut at = near(g, ty);
    let op = *g.pick(&COMPARISONS);
    let variable_first = g.one_in(2);
    // The compiler refuses a comparison that the type decides, as one with
    // the least number of the type that nothing is below, so the number is
    // moved in from the limit it is at.
    let mirrored = match op {
        "<" => ">",
        "<=" => ">=",
        ">" => "<",
        ">=" => "<=",
        other => other,
    };
    let as_written = if variable_first { op } else { mirrored };
    let decided = (at == ty.min() && matches!(as_written, ">=" | "<"))
        || (at == ty.max() && matches!(as_written, "<=" | ">"));
    if decided {
        at = if at == ty.min() { at + 1 } else { at - 1 };
    }
    if variable_first {
        (
            format!("{x} {op} {at}"),
            Box::new(move |v| compare(op, v, at)),
        )
    } else {
        // The number on the left, as the variable on the right.
        (
            format!("{at} {op} {x}"),
            Box::new(move |v| compare(op, at, v)),
        )
    }
}

#[derive(Debug)]
struct Call {
    source: String,
    /// Whether the argument satisfies every `requires`.
    holds: bool,
    arg: i128,
}

/// A function with several `requires` over its parameter, and a `main` that
/// calls it with a number, written as a literal or as a name.
fn call(g: &mut Gen) -> Call {
    let ty = *g.pick(INTS);
    let arg = near(g, ty);
    let mut holds = true;
    let mut lines = String::new();
    for _ in 0..1 + g.below(3) {
        let (text, f) = statement(g, ty, "i", 2);
        holds &= f(arg);
        lines.push_str(&format!("  requires {text}\n"));
    }
    let t = ty.name;
    let passed = if g.one_in(2) {
        arg.to_string()
    } else {
        "x".to_string()
    };
    let bind = if passed == "x" {
        format!("  x: {t} = {arg}\n")
    } else {
        String::new()
    };
    Call {
        source: with_header(format!(
            "fn needs(i: {t}) -> {t}\n{lines}{{\n  i\n}}\n\nfn main() <console, alloc> {{\n{bind}  print(\"{{needs({passed})}}\")\n}}\n"
        )),
        holds,
        arg,
    }
}

/// For every function with several `requires`, each a comparison of whole
/// numbers joined with `and`, `or`, and `not`, a call compiles exactly when
/// `arith` proves them of the call's arguments, and prints what it was given.
fn a_call_compiles_exactly_when_its_arguments_satisfy_every_requires(part: u64) {
    generate::check_part(110, 150, part, 2, call, |c| match check(c.source.as_bytes()) {
        Ok(_) => {
            assert!(c.holds, "accepted, though a `requires` does not hold");
            assert_eq!(
                run(&c.source),
                Outcome {
                    stdout: format!("{}\n", c.arg).into_bytes(),
                    status: 0
                }
            );
        }
        Err(errors) => {
            assert!(
                !c.holds,
                "refused, though every `requires` holds: {errors:?}"
            );
            assert!(
                errors.iter().any(|e| e.message.contains("needs")),
                "{errors:?} does not name the function"
            );
        }
    });
}

/// The same as `call`, with each statement written once as a function that
/// returns `bool` and named in the `requires`, some of them through a second
/// function that calls the first.
fn predicates(g: &mut Gen) -> Call {
    let ty = *g.pick(INTS);
    let arg = near(g, ty);
    let t = ty.name;
    let mut holds = true;
    let mut defs = String::new();
    let mut lines = String::new();
    for k in 0..1 + g.below(3) {
        let (text, f) = statement(g, ty, "i", 2);
        holds &= f(arg);
        defs.push_str(&format!("fn p{k}(i: {t}) -> bool {{ {text} }}\n\n"));
        if g.one_in(2) {
            defs.push_str(&format!("fn q{k}(j: {t}) -> bool {{ p{k}(j) }}\n\n"));
            lines.push_str(&format!("  requires q{k}(i)\n"));
        } else {
            lines.push_str(&format!("  requires p{k}(i)\n"));
        }
    }
    let passed = if g.one_in(2) {
        arg.to_string()
    } else {
        "x".to_string()
    };
    let bind = if passed == "x" {
        format!("  x: {t} = {arg}\n")
    } else {
        String::new()
    };
    Call {
        source: with_header(format!(
            "{defs}fn needs(i: {t}) -> {t}\n{lines}{{\n  i\n}}\n\nfn main() <console, alloc> {{\n{bind}  print(\"{{needs({passed})}}\")\n}}\n"
        )),
        holds,
        arg,
    }
}

/// For every function whose `requires` calls predicates, functions with no
/// effect that return a comparison, or a call of another, a call compiles
/// exactly when the comparisons hold of its arguments, as with the
/// expressions written in place, and the program prints the same.
fn a_requires_that_calls_a_predicate_is_owed_exactly_as_the_expression_it_returns_is(part: u64) {
    generate::check_part(111, 150, part, 2, predicates, |c| match check(c.source.as_bytes()) {
        Ok(_) => {
            assert!(c.holds, "accepted, though a predicate does not hold");
            assert_eq!(
                run(&c.source),
                Outcome {
                    stdout: format!("{}\n", c.arg).into_bytes(),
                    status: 0
                }
            );
        }
        Err(errors) => {
            assert!(
                !c.holds,
                "refused, though every predicate holds: {errors:?}"
            );
            assert!(
                errors.iter().any(|e| e.message.contains("needs")),
                "{errors:?} does not name the function"
            );
        }
    });
}

#[derive(Debug)]
struct Inside {
    source: String,
    /// Whether the operation fits for every value the `requires` allows.
    fits: bool,
    /// How the operation is written, for the error.
    written: String,
}

/// A function that requires its parameter to lie between two numbers, and
/// adds a number to it or takes one from it.
fn inside(g: &mut Gen) -> Inside {
    let ty = *g.pick(INTS);
    let (a, b) = (near(g, ty), near(g, ty));
    let (lo, hi) = (a.min(b), a.max(b));
    let k = g.below(4) as i128;
    let add = g.one_in(2);
    let (written, low, high) = if add {
        (format!("i + {k}"), lo + k, hi + k)
    } else {
        (format!("i - {k}"), lo - k, hi - k)
    };
    let fits = low >= ty.min() && high <= ty.max();
    let t = ty.name;
    let upper = if g.one_in(2) {
        format!("i <= {hi}")
    } else {
        format!("not (i > {hi})")
    };
    // A side that the type keeps already is not said.
    let lower_line = if lo > ty.min() {
        format!("  requires {lo} <= i\n")
    } else {
        String::new()
    };
    let upper_line = if hi < ty.max() {
        format!("  requires {upper}\n")
    } else {
        String::new()
    };
    Inside {
        source: with_header(format!(
            "fn f(i: {t}) -> {t}\n{lower_line}{upper_line}{{\n  {written}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{f({lo})}}\")\n}}\n"
        )),
        fits,
        written,
    }
}

/// Inside a function the `requires` are facts, so an operation on the
/// parameter compiles exactly when it fits for every value they allow.
fn inside_a_function_its_requires_are_facts() {
    generate::check(111, 80, inside, |c| match check(c.source.as_bytes()) {
        Ok(_) => assert!(c.fits, "accepted, though the result may not fit"),
        Err(errors) => {
            assert!(!c.fits, "refused, though every result fits: {errors:?}");
            assert!(
                errors.iter().any(|e| e.message.contains(&c.written)),
                "{errors:?} does not state the goal about `{}`",
                c.written
            );
        }
    });
}

#[derive(Debug)]
struct Gives {
    source: String,
    /// Whether every `ensures` holds at both places the function returns.
    holds: bool,
}

/// A function that adds one number in one block of an `if` and another in
/// the other, under a `requires` that lets both fit, with several `ensures`
/// about the result and the parameter.
fn gives(g: &mut Gen) -> Gives {
    let ty = *g.pick(INTS);
    let (c1, c2) = (g.below(4) as i128, g.below(4) as i128);
    let t = ty.name;
    let top = ty.max() - 3;
    // The branch is on a comparison of the parameter, whichever way it goes.
    let split = near(g, ty).min(top).max(ty.min() + 1);
    let mut holds = true;
    let mut lines = String::new();
    for _ in 0..1 + g.below(3) {
        let d = g.below(5) as i128 - 2;
        let op = *g.pick(&COMPARISONS);
        let sign = if d < 0 { "-" } else { "+" };
        lines.push_str(&format!("  ensures r {op} i {sign} {}\n", d.abs()));
        // The block for values below the split is never run when none are.
        holds &= (split <= ty.min() || compare(op, c1, d)) && compare(op, c2, d);
    }
    Gives {
        source: with_header(format!(
            "fn lift(i: {t}) -> r: {t}\n  requires i <= {top}\n{lines}{{\n  if i < {split} {{\n    i + {c1}\n  }} else {{\n    i + {c2}\n  }}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{lift({})}}\")\n}}\n",
            ty.min()
        )),
        holds,
    }
}

/// A function compiles exactly when `arith` proves every `ensures` of what
/// it returns, at each place it returns.
fn a_function_compiles_exactly_when_what_it_returns_satisfies_every_ensures() {
    generate::check(112, 150, gives, |c| match check(c.source.as_bytes()) {
        Ok(_) => assert!(c.holds, "accepted, though an `ensures` does not hold"),
        Err(errors) => {
            assert!(
                !c.holds,
                "refused, though every `ensures` holds: {errors:?}"
            );
            assert!(
                errors.iter().any(|e| e.message.contains("lift")),
                "{errors:?} does not name the function"
            );
        }
    });
}

#[derive(Debug)]
struct Chain {
    source: String,
    /// Whether the numbers allow the calls.
    fits: bool,
    ty: Int,
    start: i128,
    depth: i128,
}

/// A function whose `ensures` says what it adds, called inside another that
/// calls it `depth` times, one on the result of the other, under a `requires`
/// that is next to the limit.
fn chain(g: &mut Gen) -> Chain {
    let ty = *g.pick(INTS);
    let depth = 1 + g.below(3) as i128;
    let bound = ty.max() - depth + g.below(3) as i128 - 1;
    let start = bound.min(ty.max());
    let t = ty.name;
    let mut call = "i".to_string();
    for _ in 0..depth {
        call = format!("next({call})");
    }
    Chain {
        source: with_header(format!(
            "fn next(i: {t}) -> r: {t}\n  requires i < {max}\n  ensures r == i + 1\n{{\n  i + 1\n}}\n\nfn many(i: {t}) -> {t}\n  requires i <= {bound}\n{{\n  {call}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{many({start})}}\")\n}}\n",
            max = ty.max()
        )),
        fits: bound + depth <= ty.max(),
        ty,
        start,
        depth,
    }
}

/// After a call, what the function ensures of its arguments and its result
/// is a fact, so `depth` calls one on the result of another compile exactly
/// when the `requires` of the caller leaves room for them, and then add up.
fn after_a_call_what_it_ensures_is_a_fact_so_calls_chain_exactly_as_far_as_the_numbers_allow(part: u64) {
    generate::check_part(113, 150, part, 2, chain, |c| match check(c.source.as_bytes()) {
        Ok(_) => {
            assert!(c.fits, "accepted, though the calls can pass the limit");
            assert_eq!(
                run(&c.source),
                Outcome {
                    stdout: format!("{}\n", c.start + c.depth).into_bytes(),
                    status: 0
                }
            );
        }
        Err(errors) => {
            assert!(!c.fits, "refused, though the calls fit: {errors:?}");
            assert!(
                errors.iter().any(|e| e.message.contains("next")),
                "{errors:?} does not name the function {:?}",
                c.ty
            );
        }
    });
}

/// An `ensures` on a function whose signature gives its result no name is
/// an error that says so, for every type, and naming the result mends it.
fn an_ensures_on_a_function_that_does_not_name_its_result_is_an_error_that_says_so() {
    for ty in INTS {
        let t = ty.name;
        let unnamed = with_header(format!(
            "fn same(i: {t}) -> {t}\n  ensures i == i\n{{\n  i\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{same(1)}}\")\n}}\n"
        ));
        let errors = check(unnamed.as_bytes()).expect_err("a result with no name");
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("same") && e.message.contains("name")),
            "{errors:?} does not say the result has no name"
        );
        let named = unnamed.replace("-> ", "-> r: ").replace("i == i", "r == i");
        assert!(check(named.as_bytes()).is_ok(), "{named}");
    }
}

/// A function that never needs its contracts to compile, written with and
/// without them.
fn guarded(g: &mut Gen) -> (String, String) {
    let ty = *g.pick(INTS);
    let t = ty.name;
    let at = near(g, ty);
    let (requires, ensures) = match g.below(3) {
        0 => (
            "  requires i >= i\n".to_string(),
            "  ensures r >= i\n".to_string(),
        ),
        1 => (
            "  requires i <= i and i >= i\n".to_string(),
            "  ensures r >= i and r <= i + 1\n".to_string(),
        ),
        _ => (
            String::new(),
            format!("  ensures r > i or r == {}\n", ty.max()),
        ),
    };
    let body = format!(
        "  if i < {} {{\n    i + 1\n  }} else {{\n    i\n  }}\n",
        ty.max()
    );
    let main = format!("fn main() <console, alloc> {{\n  print(\"{{f({at})}}\")\n}}\n");
    let with = format!("fn f(i: {t}) -> r: {t}\n{requires}{ensures}{{\n{body}}}\n\n{main}");
    let without = format!("fn f(i: {t}) -> {t} {{\n{body}}}\n\n{main}");
    let (with, without) = (with_header(with), with_header(without));
    (with, without)
}

/// For every program that compiles with its contracts and without them, the
/// scalar form is the same, so nothing is added for a `requires` or an
/// `ensures`.
fn contracts_add_no_instruction_to_a_program_that_compiles_without_them() {
    generate::check(114, 60, guarded, |(with, without)| {
        let lowered = |source: &str| {
            lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"))
        };
        assert_eq!(lowered(with), lowered(without));
    });
}

/// A function type states no contract, so a function with a `requires` is
/// only called by its name: in every way a function name is passed as a value, for
/// every type, it is refused and the error names it, and with only an
/// `ensures` it is accepted.
fn a_function_with_a_requires_is_refused_wherever_it_is_used_as_a_value() {
    for ty in INTS {
        let t = ty.name;
        let max = ty.max();
        // How the function is used as a value, in `main`.
        let uses = ["apply(1, f: lift)", "through(lift, 1)"];
        for usage in uses {
            for requires in [true, false] {
                let line = if requires {
                    format!("  requires i < {max}\n")
                } else {
                    String::new()
                };
                let source = with_header(format!(
                    "fn lift(i: {t}) -> r: {t}\n{line}  ensures r >= i\n{{\n  if i < {max} {{ i + 1 }} else {{ i }}\n}}\n\n\
                     fn apply(x: {t}; f: fn({t}) -> {t}) -> {t} {{\n  f(x)\n}}\n\n\
                     fn through(f: fn({t}) -> {t}, x: {t}) -> {t} {{\n  f(x)\n}}\n\n\
                     fn main() <console, alloc> {{\n  print(\"{{{usage}}}\")\n}}\n"
                ));
                match check(source.as_bytes()) {
                    Ok(_) => assert!(
                        !requires,
                        "accepted, though `lift` has a `requires`: {source}"
                    ),
                    Err(errors) => {
                        assert!(
                            requires,
                            "refused, though `lift` has no `requires`: {errors:?}"
                        );
                        assert!(
                            errors.iter().any(|e| e.message.contains("lift")),
                            "{errors:?} does not name the function for {usage}"
                        );
                    }
                }
            }
        }
    }
}

/// The struct the functions that change a value in place change.
const CURSOR: &str = "struct Cursor {\n  at: Nat\n  line: Nat\n}\n\nstruct Doc {\n  cursor: Cursor\n  lines: Nat\n}\n\n";

#[derive(Debug)]
struct Changes {
    source: String,
    /// Whether what the function adds is what it ensures, on every way out.
    holds: bool,
    /// What it ensures it adds.
    ensured: i128,
}

/// A function that adds to a parameter marked `mut`, a `Cursor` or a `Nat`,
/// maybe leaving early by `return` between two additions, and ensures it
/// adds a number that may or may not be what it adds on each way out.
fn changes(g: &mut Gen) -> Changes {
    let (first, then, ensured) = (g.below(3) as i128, g.below(3) as i128, g.below(4) as i128);
    let early = g.one_in(2);
    let cursor = g.one_in(2);
    let (param, x, left) = match cursor {
        true => ("c: mut Cursor", "c.at", "c.line == 0"),
        false => ("n: mut Nat", "n", "n == 7"),
    };
    let old = if cursor { "old(c).at" } else { "old(n)" };
    let add = |k: i128| match k {
        0 => String::new(),
        k => format!("  {x} = {x} + {k}\n"),
    };
    let mut body = add(first);
    if early {
        body.push_str(&format!("  if {left} {{\n    return\n  }}\n"));
        body.push_str(&add(then));
    }
    let holds = first == ensured && (!early || first + then == ensured);
    let main = match cursor {
        true => format!(
            "  mut c = Cursor(at: 0, line: {})\n  if c.at < 1000 {{\n    step(mut c)\n  }}\n  print(\"{{c.at}}\")\n",
            g.below(2)
        ),
        false => "  mut n: Nat = 0\n  if n < 1000 {\n    step(mut n)\n  }\n  print(\"{n}\")\n".to_string(),
    };
    Changes {
        source: with_header(format!(
            "{CURSOR}fn step({param})\n  requires {x} < 1000\n  ensures {x} == {old} + {ensured}\n{{\n{body}}}\n\nfn main() <console, alloc> {{\n{main}}}\n"
        )),
        holds,
        ensured,
    }
}

/// A function that changes a parameter marked `mut` compiles exactly when
/// what it ensures of it and of `old` of it holds at each place it returns,
/// and the caller then sees what it ensured.
fn a_function_that_changes_a_parameter_compiles_exactly_when_what_it_does_to_it_is_what_it_ensures_of_old()
 {
    generate::check(1234, 40, changes, |c| match check(c.source.as_bytes()) {
        Ok(_) => {
            assert!(c.holds, "accepted, though an `ensures` does not hold");
            let printed = String::from_utf8(run(&c.source).stdout).expect("text");
            assert_eq!(printed, format!("{}\n", c.ensured));
        }
        Err(errors) => {
            assert!(!c.holds, "refused, though every `ensures` holds: {errors:?}");
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("step") && e.message.contains("ensures")),
                "{errors:?} does not name the `ensures` of the function"
            );
        }
    });
}

#[derive(Debug)]
struct Walk {
    source: String,
    /// Whether every `ensures` holds and every call's `requires` is proven.
    fits: bool,
    /// How many times the place is stepped.
    steps: i128,
}

/// `step`, which ensures it adds one, called `steps` times by `walk`, which
/// ensures it adds a number that may or may not be that, and `main`, which
/// walks a `mut` or a field of one under a bound and steps it once more, so
/// that each call proves its `requires` only from what the calls before it
/// ensured.
fn walk(g: &mut Gen) -> Walk {
    let steps = 1 + g.below(3) as i128;
    let ensured = steps - 1 + g.below(3) as i128;
    let limit = 50 + g.below(5) as i128;
    let bound = limit - steps + g.below(3) as i128;
    let place = *g.pick(&["c", "d.cursor"]);
    let calls = "  step(mut v)\n".repeat(steps as usize);
    Walk {
        source: with_header(format!(
            "{CURSOR}fn step(c: mut Cursor)\n  requires c.at < {limit}\n  ensures c.at == old(c).at + 1\n{{\n  c.at = c.at + 1\n}}\n\n\
             fn walk(v: mut Cursor)\n  requires v.at < {bound}\n  ensures v.at == old(v).at + {ensured}\n{{\n{calls}}}\n\n\
             fn main() <console, alloc> {{\n  mut c = Cursor(at: 0, line: 0)\n  mut d = Doc(cursor: Cursor(at: 0, line: 0), lines: 1)\n  if {place}.at < {bound} {{\n    walk(mut {place})\n    step(mut {place})\n  }}\n  print(\"{{{place}.at}} {{c.line}} {{d.lines}}\")\n}}\n"
        )),
        fits: ensured == steps && bound + steps - 1 < limit,
        steps,
    }
}

/// After a call that changes a place, the caller knows what the function
/// ensures, with `old` of the parameter what the place held before the call,
/// so calls that each need room under a limit compile exactly as far as the
/// numbers allow, and the place holds what they added.
fn after_a_call_that_changes_a_place_what_it_ensures_of_old_is_known_so_calls_chain_exactly_as_far_as_the_requires_allows()
 {
    generate::check(1235, 40, walk, |c| match check(c.source.as_bytes()) {
        Ok(_) => {
            assert!(c.fits, "accepted, though a call can pass the limit");
            let printed = String::from_utf8(run(&c.source).stdout).expect("text");
            assert_eq!(printed, format!("{} 0 1\n", c.steps + 1));
        }
        Err(errors) => {
            assert!(!c.fits, "refused, though the calls fit: {errors:?}");
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("walk") || e.message.contains("step")),
                "{errors:?} names neither function"
            );
        }
    });
}

/// `old` written in a `requires`, in a body, of a parameter not marked `mut`,
/// of a field, of a number, or in a function that changes nothing, is refused
/// where it is written, each case with the `old` it is refused for, and the same function with `old` of its parameter marked `mut`
/// in its `ensures` is not.
fn old_is_refused_outside_an_ensures_and_of_anything_but_a_parameter_marked_mut() {
    let accepted = "fn f(c: mut Cursor, k: Nat)\n  requires c.at < 9\n  ensures c.at == old(c).at + 1\n{\n  c.at = c.at + 1\n}\n";
    let refused = [
        (
            "fn f(c: mut Cursor, k: Nat)\n  requires old(c).at < 9\n  ensures c.at == old(c).at + 1\n{\n  c.at = c.at + 1\n}\n",
            "old(c).at < 9",
        ),
        (
            "fn f(c: mut Cursor, k: Nat)\n  requires c.at < 9\n{\n  c.at = old(c).at + 1\n}\n",
            "old(c).at + 1\n}",
        ),
        (
            "fn f(c: mut Cursor, k: Nat)\n  requires c.at < 9\n  ensures c.at == old(k)\n{\n  c.at = k\n}\n",
            "old(k)",
        ),
        (
            "fn f(c: mut Cursor, k: Nat)\n  requires c.at < 9\n  ensures c.at == old(c.at) + 1\n{\n  c.at = c.at + 1\n}\n",
            "old(c.at)",
        ),
        (
            "fn f(c: mut Cursor, k: Nat)\n  requires c.at < 9\n  ensures c.at == old(c).at + old(1)\n{\n  c.at = c.at + 1\n}\n",
            "old(1)",
        ),
        (
            "fn g(k: Nat) -> r: Nat\n  ensures r == old(k)\n{\n  k\n}\n\nfn f(c: mut Cursor, k: Nat)\n  requires c.at < 9\n{\n  c.at = g(k)\n}\n",
            "old(k)",
        ),
    ];
    let program = |f: &str| {
        with_header(format!(
            "{CURSOR}{f}\nfn main() <console, alloc> {{\n  mut d = Doc(cursor: Cursor(at: 0, line: 0), lines: 1)\n  if d.cursor.at < 9 {{\n    f(mut d.cursor, 3)\n  }}\n  print(\"{{d.cursor.at}} {{d.lines}} {{d.cursor.line}}\")\n}}\n"
        ))
    };
    let source = program(accepted);
    assert!(check(source.as_bytes()).is_ok(), "{source}");
    for (f, refused_at) in refused {
        let source = program(f);
        let at = pos_after(&source[..source.find(refused_at).expect("the `old`")]);
        let errors = check(source.as_bytes()).expect_err(&source);
        assert!(
            errors
                .iter()
                .any(|e| e.pos == at && e.message.contains("old")),
            "nothing about `old` at {at:?} among {errors:?} for {source}"
        );
    }
}
