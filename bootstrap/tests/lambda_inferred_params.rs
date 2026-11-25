//! A lambda that is bound to a name and leaves the types of its parameters out
//! has them from the calls of it in the same body: each parameter is the
//! smallest range that holds every argument, joined before the lambda is
//! checked once with it, and the result is what its body gives. It builds to the
//! bytes of the lambda that writes those types. One that no call gives the types
//! of is refused with where to write them.
//!
//! The generator builds the program that leaves the types out and the program
//! that writes them from the same choices, and says what the first prints.

use smith::diag::Error;
use smith::target::Target;
use smith::{check, compile, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_bound_lambda_takes_the_range_that_holds_every_argument_of_its_calls_and_prints_what_they_give",
        a_bound_lambda_takes_the_range_that_holds_every_argument_of_its_calls_and_prints_what_they_give,
    ),
    (
        "a_bound_lambda_that_leaves_its_types_out_builds_to_the_bytes_of_the_one_that_writes_them",
        a_bound_lambda_that_leaves_its_types_out_builds_to_the_bytes_of_the_one_that_writes_them,
    ),
    (
        "a_use_in_a_bound_lambda_that_needs_less_than_its_calls_pass_is_refused_naming_the_calls",
        a_use_in_a_bound_lambda_that_needs_less_than_its_calls_pass_is_refused_naming_the_calls,
    ),
    (
        "a_bound_lambda_that_no_call_gives_the_types_of_is_refused_where_it_is_written",
        a_bound_lambda_that_no_call_gives_the_types_of_is_refused_where_it_is_written,
    ),
    (
        "a_bound_lambda_called_with_what_its_own_calls_give_so_that_its_range_grows_is_refused",
        a_bound_lambda_called_with_what_its_own_calls_give_so_that_its_range_grows_is_refused,
    ),
    (
        "a_bound_lambda_called_with_what_its_own_calls_give_that_settles_is_inferred",
        a_bound_lambda_called_with_what_its_own_calls_give_that_settles_is_inferred,
    ),
];

fn errors(source: &str) -> Vec<Error> {
    check(source.as_bytes()).err().unwrap_or_default()
}

/// One call of the lambda: the range of its argument, and a value in it.
#[derive(Clone, Debug)]
struct Arg {
    low: i128,
    high: i128,
    value: i128,
    /// Whether the argument is a number written out, and not a parameter.
    literal: bool,
}

/// A function that binds `f = fn(v) { v * m + c }` and calls it once for each
/// argument.
#[derive(Clone, Debug)]
struct Case {
    m: i128,
    c: i128,
    args: Vec<Arg>,
}

fn case(g: &mut Gen) -> Case {
    let args = (0..1 + g.below(3))
        .map(|_| {
            let low = g.below(6) as i128;
            let high = low + g.below(30) as i128;
            let literal = g.one_in(3);
            let value = low + g.below((high - low + 1) as usize) as i128;
            let (low, high) = if literal { (value, value) } else { (low, high) };
            Arg {
                low,
                high,
                value,
                literal,
            }
        })
        .collect();
    Case {
        m: 1 + g.below(4) as i128,
        c: g.below(6) as i128,
        args,
    }
}

impl Case {
    /// The range that holds every argument.
    fn joined(&self) -> (i128, i128) {
        (
            self.args.iter().map(|a| a.low).min().expect("a call"),
            self.args.iter().map(|a| a.high).max().expect("a call"),
        )
    }

    /// The range the lambda gives.
    fn gives(&self) -> (i128, i128) {
        let (low, high) = self.joined();
        (low * self.m + self.c, high * self.m + self.c)
    }

    /// What the function gives: the sum of one result for each call.
    fn sum(&self) -> (i128, i128) {
        let (low, high) = self.gives();
        let n = self.args.len() as i128;
        (low * n, high * n)
    }

    fn expected(&self) -> i128 {
        self.args.iter().map(|a| a.value * self.m + self.c).sum()
    }

    /// The source, with the lambda written as `lambda`.
    fn source(&self, lambda: &str) -> String {
        let params: Vec<String> = (self.args.iter().enumerate())
            .filter(|(_, a)| !a.literal)
            .map(|(i, a)| format!("a{i}: {}..{}", a.low, a.high + 1))
            .collect();
        let calls: Vec<String> = (self.args.iter().enumerate())
            .map(|(i, a)| match a.literal {
                true => format!("f({})", a.value),
                false => format!("f(a{i})"),
            })
            .collect();
        let values: Vec<String> = (self.args.iter())
            .filter(|a| !a.literal)
            .map(|a| a.value.to_string())
            .collect();
        let (low, high) = self.sum();
        format!(
            "use std.console (console, print)\n\n\
             fn go({}) -> {low}..{} {{\n  f = {lambda}\n  {}\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{go({})}}\")\n}}\n",
            params.join(", "),
            high + 1,
            calls.join(" + "),
            values.join(", ")
        )
    }

    fn left_out(&self) -> String {
        self.source(&format!("fn(v) {{ v * {} + {} }}", self.m, self.c))
    }

    fn written(&self) -> String {
        let (low, high) = self.joined();
        let (from, to) = self.gives();
        self.source(&format!(
            "fn(v: {low}..{}) -> {from}..{} {{ v * {} + {} }}",
            high + 1,
            to + 1,
            self.m,
            self.c
        ))
    }
}

/// Each parameter is the smallest range that holds the arguments, so the
/// function builds with the result written as the range that range gives, and
/// prints what the calls give.
fn a_bound_lambda_takes_the_range_that_holds_every_argument_of_its_calls_and_prints_what_they_give() {
    generate::check(4501, 40, case, |p| {
        let source = p.left_out();
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
        let ran = scalar::run(&program);
        assert_eq!(ran.stdout, format!("{}\n", p.expected()).into_bytes(), "for\n{source}");
    });
}

fn a_bound_lambda_that_leaves_its_types_out_builds_to_the_bytes_of_the_one_that_writes_them() {
    generate::check(4502, 3, case, |p| {
        let (left_out, written) = (p.left_out(), p.written());
        if let Err(found) = check(written.as_bytes()) {
            panic!("{found:?} for\n{written}");
        }
        for &target in Target::ALL {
            let inferred = compile(left_out.as_bytes(), target)
                .unwrap_or_else(|e| panic!("{e:?} for\n{left_out}"));
            let said = compile(written.as_bytes(), target).expect("the types written build");
            assert_eq!(inferred, said, "for\n{left_out}\nand\n{written}");
        }
    });
}

/// Two calls of `go`, one that fits what `takes` needs and one that does not.
#[derive(Clone, Debug)]
struct Needs {
    limit: i128,
    fits: i128,
    over: i128,
    /// Whether the call that does not fit comes first.
    over_first: bool,
}

fn a_use_in_a_bound_lambda_that_needs_less_than_its_calls_pass_is_refused_naming_the_calls() {
    generate::check(
        4503,
        20,
        |g: &mut Gen| {
            let limit = 2 + g.below(250) as i128;
            Needs {
                limit,
                fits: g.below(limit as usize) as i128,
                over: limit + g.below(500) as i128,
                over_first: g.one_in(2),
            }
        },
        |p| {
            let (first, second) = match p.over_first {
                true => (p.over, p.fits),
                false => (p.fits, p.over),
            };
            let source = format!(
                "fn takes(b: 0..{}) -> 0..{} {{ b }}\n\n\
                 fn pass() -> 0..{} {{\n  go = fn(v) {{ takes(v) }}\n  x = go({first})\n  go({second})\n}}\n\n\
                 fn main() {{\n  x = pass()\n}}\n",
                p.limit, p.limit, p.limit
            );
            let line_of = |text: &str| {
                source.lines().position(|l| l.trim().contains(text)).expect(text) + 1
            };
            let found = errors(&source);
            let said = found
                .iter()
                .find(|e| e.pos.line == line_of("takes(v)") && e.message.contains("inferred"))
                .unwrap_or_else(|| panic!("{found:?} for\n{source}"));
            let (low, high) = (p.fits.min(p.over), p.fits.max(p.over));
            for part in [
                format!("`v` is a `{low}..{}`", high + 1),
                format!("the call `go({first})` at line {}", line_of(&format!("x = go({first})"))),
                format!("the call `go({second})`"),
                format!("`takes` at line {}", line_of("takes(v)")),
                format!("needs a `0..{}`", p.limit),
            ] {
                assert!(said.message.contains(&part), "no `{part}` in {said:?} for\n{source}");
            }
        },
    );
}

/// A lambda bound to a name that no call of it in the same body gives the types
/// of writes them, wherever it goes: it is refused at the lambda.
fn a_bound_lambda_that_no_call_gives_the_types_of_is_refused_where_it_is_written() {
    let head = "struct Pricing {\n  discount: fn(f32) -> f32\n}\n\n\
                fn apply(f: fn(f32) -> f32, x: f32) -> f32 { f(x) }\n\n";
    for (result, last) in [
        ("f32", "1.0"),
        ("fn(f32) -> f32", "f"),
        ("f32", "apply(f, 2.0)"),
        ("Pricing", "Pricing(discount: f)"),
    ] {
        let source = format!(
            "{head}fn make() -> {result} {{\n  f = fn(v) {{ v * 2.0 }}\n  {last}\n}}\n\n\
             fn main() {{\n  m = make()\n}}\n"
        );
        let at = source.lines().position(|l| l.contains("f = fn(v)")).expect("the lambda") + 1;
        let found = errors(&source);
        assert!(
            found
                .iter()
                .any(|e| e.pos.line == at && e.message.contains("writes the type of each parameter")),
            "{found:?} for\n{source}"
        );
    }
}

fn a_bound_lambda_called_with_what_its_own_calls_give_so_that_its_range_grows_is_refused() {
    generate::check(
        4504,
        10,
        |g: &mut Gen| (1 + g.below(5), 1 + g.below(9)),
        |&(step, top)| {
            let source = format!(
                "fn chain(a: 0..{}) -> 0..1000 {{\n  inc = fn(v) {{ v + {step} }}\n  y = inc(a)\n  inc(y)\n}}\n\n\
                 fn main() {{\n  x = chain(1)\n}}\n",
                top + 1
            );
            let at = source.lines().position(|l| l.contains("inc = fn(v)")).expect("the lambda") + 1;
            let found = errors(&source);
            assert!(
                found.iter().any(|e| e.pos.line == at && e.message.contains("written")),
                "{found:?} for\n{source}"
            );
        },
    );
}

fn a_bound_lambda_called_with_what_its_own_calls_give_that_settles_is_inferred() {
    let source = "use std.console (console, print)\n\n\
                  fn chain(a: 0..40) -> 0..100 {\n  half = fn(v) { v / 2 }\n  y = half(a)\n  z = half(y)\n  half(z) + half(10)\n}\n\n\
                  fn main() <console, alloc> {\n  print(\"{chain(3)}\")\n}\n";
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    assert_eq!(scalar::run(&program).stdout, b"5\n");
}
