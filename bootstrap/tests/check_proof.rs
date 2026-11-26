//! `match check(e) { .some(p) { a } .none { b } }`, which tests `e` when the
//! program runs, takes the first arm exactly when it holds, and in that arm
//! gives `arith` what `if e` does.
//!
//! The statements are the conditions `arith.rs` generates, whose truth is
//! worked out by trying values, so nothing here shares a line with the
//! compiler.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::arith::{Condition, condition, linear};
use crate::generate::{self, Gen};
use crate::numbers::{INTS, Int, with_header};

pub const ONCE: &[Test] = &[
    (
        "a_check_takes_the_first_arm_exactly_when_the_statement_holds",
        a_check_takes_the_first_arm_exactly_when_the_statement_holds,
    ),
    (
        "a_check_gives_arith_what_an_if_gives_and_is_the_same_program_after_the_passes",
        a_check_gives_arith_what_an_if_gives_and_is_the_same_program_after_the_passes,
    ),
    (
        "a_check_of_a_statement_that_names_a_var_or_carries_an_effect_is_refused",
        a_check_of_a_statement_that_names_a_var_or_carries_an_effect_is_refused,
    ),
    (
        "a_check_of_a_product_of_two_sizes_proves_what_a_call_requires",
        a_check_of_a_product_of_two_sizes_proves_what_a_call_requires,
    ),
];

/// How a test of a statement is written.
#[derive(Clone, Copy, Debug)]
enum Written {
    If,
    Check,
}

impl Written {
    /// The text that opens a test of `c` whose first block starts on the next
    /// line.
    fn open(self, c: &str) -> String {
        match self {
            Written::If => format!("if {c} {{\n"),
            Written::Check => format!("match check({c}) {{\n.some(p) {{\n"),
        }
    }

    /// The text between the first block and the second, which starts on the
    /// next line.
    fn between(self) -> &'static str {
        match self {
            Written::If => "} else {\n",
            Written::Check => "}\n.none {\n",
        }
    }

    /// The text that ends the test.
    fn close(self) -> &'static str {
        match self {
            Written::If => "}\n",
            Written::Check => "}\n}\n",
        }
    }

    /// A test of `c` that gives `then` where it holds and `otherwise` where it
    /// does not.
    fn test(self, c: &str, then: &str, otherwise: &str) -> String {
        format!(
            "{}{then}\n{}{otherwise}\n{}",
            self.open(c),
            self.between(),
            self.close()
        )
    }
}

/// A signed type with room for the numbers of the conditions, whose sums are
/// each in the 64 bits that an exact range is kept in.
fn room(g: &mut Gen) -> Int {
    let wide: Vec<Int> = INTS
        .iter()
        .filter(|t| t.signed && (16..=32).contains(&t.bits))
        .copied()
        .collect();
    *g.pick(&wide)
}

#[derive(Debug)]
struct Tested {
    source: String,
    /// What `f` prints, line by line, for each pair of values.
    stdout: String,
}

/// A statement, the function that says for which values it holds, and the
/// predicate it calls, if it calls one.
type Statement = (String, Box<dyn Fn(i128, i128) -> bool>, String);

/// A function of two variables that is `1` where a statement holds and `0`
/// where it does not, printed for the values around zero.
fn tested(g: &mut Gen) -> Tested {
    let ty = room(g);
    let t = ty.name;
    // A statement is also a call of a predicate, a function of one comparison.
    let (text, holds, predicate): Statement =
        if g.one_in(3) {
            let op = *g.pick(&["<", "<=", ">", ">=", "==", "!="]);
            (
                "ordered(x, y)".to_string(),
                Box::new(move |x, y| crate::numbers::compare(op, x, y)),
                format!("fn ordered(a: {t}, b: {t}) -> bool {{ a {op} b }}\n\n"),
            )
        } else {
            let (text, holds): Condition = condition(g, 2, true);
            (text, holds, String::new())
        };
    let body = Written::Check.test(&text, "1", "0");
    let mut calls = String::new();
    let mut stdout = String::new();
    for x in -2..=2 {
        for y in -2..=2 {
            calls.push_str(&format!("  print(\"{{f({x}, {y})}}\")\n"));
            stdout.push_str(&format!("{}\n", u8::from(holds(x, y))));
        }
    }
    Tested {
        source: with_header(format!(
            "{predicate}fn f(x: {t}, y: {t}) -> {t} {{\n{body}}}\n\nfn main() <console, alloc> {{\n{calls}}}\n"
        )),
        stdout,
    }
}

/// For every statement of comparisons of whole numbers, the first arm of a
/// `check` runs exactly for the values that make it true.
fn a_check_takes_the_first_arm_exactly_when_the_statement_holds() {
    generate::check(121, 40, tested, |t| {
        let program = lower(t.source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {}", t.source));
        assert_eq!(
            scalar::run(&program),
            Outcome {
                stdout: t.stdout.clone().into_bytes(),
                status: 0
            },
            "for\n{}",
            t.source
        );
    });
}

#[derive(Debug)]
struct Goal {
    /// The program, with a test written as `if` and as `check`.
    sources: [String; 2],
    /// Whether the result fits whenever the conditions hold.
    fits: bool,
}

/// A function of two variables that lie between -8 and 8, whose goal is a sum
/// of them, inside nested tests of conditions that are linear in the
/// variables, each with the first or the second block taking the sum.
fn goal(g: &mut Gen) -> Goal {
    let ty = room(g);
    let scale = (ty.max() / 16).max(1);
    let (a, b) = (
        (g.below(7) as i128 - 3) * scale,
        (g.below(7) as i128 - 3) * scale,
    );
    let k = *g.pick(&[0, 1, -1, ty.max() / 2, -(ty.max() / 2), ty.max() / 4]);
    let mut conds: Vec<(Condition, bool)> = Vec::new();
    for _ in 0..1 + g.below(3) {
        conds.push((condition(g, 1, ty.bits >= 16), g.one_in(2)));
    }
    let holds = |x: i128, y: i128| conds.iter().all(|(c, then)| c.1(x, y) == *then);
    let sum = linear(a, b, k);
    let t = ty.name;
    let source = |written: Written| {
        let mut body = String::from("  if x >= -8 and x <= 8 and y >= -8 and y <= 8 {\n");
        for (c, then) in &conds {
            body.push_str(&written.open(&c.0));
            if !*then {
                body.push_str("0\n");
                body.push_str(written.between());
            }
        }
        body.push_str(&format!("{sum}\n"));
        for (_, then) in conds.iter().rev() {
            if *then {
                body.push_str(written.between());
                body.push_str("0\n");
            }
            body.push_str(written.close());
        }
        body.push_str("  } else {\n    0\n  }\n");
        with_header(format!(
            "fn f(x: {t}, y: {t}) -> {t} {{\n{body}}}\n\nfn main() <console, alloc> {{\n  print(\"{{f(1, 2)}}\")\n}}\n"
        ))
    };
    let mut fits = true;
    for x in -8..=8 {
        for y in -8..=8 {
            if holds(x, y) && !(ty.min()..=ty.max()).contains(&(a * x + b * y + k)) {
                fits = false;
            }
        }
    }
    Goal {
        sources: [source(Written::If), source(Written::Check)],
        fits,
    }
}

/// For every set of conditions and every sum, a program that tests the
/// conditions with `check` is accepted exactly when the one that tests them
/// with `if` is, which is when the sum fits wherever they hold, and then both
/// are one program once the passes have run, with no instruction for the proof.
fn a_check_gives_arith_what_an_if_gives_and_is_the_same_program_after_the_passes() {
    generate::check(122, 80, goal, |goal| {
        let [with_if, with_check] = &goal.sources;
        let accepted = |source: &str| check(source.as_bytes()).is_ok();
        assert_eq!(accepted(with_if), goal.fits, "for\n{with_if}");
        assert_eq!(accepted(with_check), goal.fits, "for\n{with_check}");
        if goal.fits {
            let lowered = |source: &str| {
                lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"))
            };
            assert_eq!(
                lowered(with_check),
                lowered(with_if),
                "for\n{with_check}\nand\n{with_if}"
            );
        }
    });
}

#[derive(Debug)]
struct Refused {
    source: String,
    /// What the error must name.
    names: &'static str,
}

/// A `check` of a statement that names a `var`, or calls a function that
/// carries an effect, written as the generator picks, and the same program
/// with the statement that is allowed.
fn refused(g: &mut Gen) -> (Refused, String) {
    let op = *g.pick(&["<", "<=", ">", ">=", "==", "!="]);
    let n = 1 + g.below(9);
    let by_var = g.one_in(2);
    let (names, bound, statement, effect) = if by_var {
        ("count", "var count = a", format!("count {op} {n}"), "")
    } else {
        (
            "noisy",
            "limit = a",
            format!("noisy(limit) {op} {n}"),
            "fn noisy(n: Nat) -> Nat <console, alloc> {\n  print(\"{n}\")\n  n\n}\n\n",
        )
    };
    let program = |bound: &str, statement: &str, effect: &str, listed: &str| {
        with_header(format!(
            "{effect}fn f(a: Nat) -> Nat {listed}{{\n  {bound}\n  match check({statement}) {{\n    .some(p) {{ 1 }}\n    .none {{ 0 }}\n  }}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{f(1)}}\")\n}}\n"
        ))
    };
    let listed = if by_var { "" } else { "<console, alloc> " };
    let allowed = if by_var {
        program("count = a", &format!("count {op} {n}"), effect, listed)
    } else {
        program("limit = a", &format!("limit {op} {n}"), effect, listed)
    };
    (
        Refused {
            source: program(bound, &statement, effect, listed),
            names,
        },
        allowed,
    )
}

/// For every statement that names a `var` or carries an effect, a `check` of
/// it is refused with the name, and the program that tests a statement that is
/// allowed in its place is accepted.
fn a_check_of_a_statement_that_names_a_var_or_carries_an_effect_is_refused() {
    generate::check(123, 40, refused, |(refused, allowed)| {
        let errors = check(refused.source.as_bytes()).expect_err(&refused.source);
        assert!(
            errors.iter().any(|e| e.message.contains(refused.names)),
            "{errors:?} does not name `{}` for\n{}",
            refused.names,
            refused.source
        );
        if let Err(errors) = check(allowed.as_bytes()) {
            panic!("{errors:?} for\n{allowed}");
        }
    });
}

/// `fill_checked` of the page `check.md`: the product of two sizes that are
/// known only when the program runs is the statement a call requires, and a
/// `check` proves it in its first arm.
fn a_check_of_a_product_of_two_sizes_proves_what_a_call_requires() {
    let source = with_header(
        "fn fill(rows: Nat, cols: Nat, limit: Nat) -> Nat\n  requires rows * cols <= limit\n{\n  limit - rows * cols\n}\n\nfn fill_checked(rows: Nat, cols: Nat, limit: Nat) -> Option[Nat] {\n  match check(rows * cols <= limit) {\n    .some(p) { .some(fill(rows, cols, limit)) }\n    .none { .none }\n  }\n}\n\nfn main() <console, alloc> {\n  print(\"{fill_checked(2, 3, 10)}\")\n  print(\"{fill_checked(4, 4, 10)}\")\n}\n"
            .to_string(),
    );
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(
        scalar::run(&program),
        Outcome {
            stdout: b".some(4)\n.none\n".to_vec(),
            status: 0
        }
    );
}
