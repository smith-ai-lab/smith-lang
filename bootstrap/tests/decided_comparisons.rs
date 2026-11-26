//! A comparison of two whole numbers that the two ranges decide, whatever the
//! values are, is refused, as `knowledge/language/literals.md` says.

use smith::check;

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;

pub const ONCE: &[Test] = &[
    (
        "a_number_that_the_other_side_cannot_hold_is_a_comparison_refused_with_the_fix_to_delete_it",
        a_number_that_the_other_side_cannot_hold_is_a_comparison_refused_with_the_fix_to_delete_it,
    ),
    (
        "a_comparison_of_two_ranges_is_refused_exactly_when_every_pair_of_numbers_answers_alike",
        a_comparison_of_two_ranges_is_refused_exactly_when_every_pair_of_numbers_answers_alike,
    ),
    (
        "a_comparison_is_refused_in_a_requires_an_ensures_and_a_proof_as_it_is_in_a_body",
        a_comparison_is_refused_in_a_requires_an_ensures_and_a_proof_as_it_is_in_a_body,
    ),
    (
        "a_comparison_that_only_a_fact_decides_is_not_refused",
        a_comparison_that_only_a_fact_decides_is_not_refused,
    ),
];

/// The messages and the fixes of the errors that say a comparison is decided,
/// or none when the program compiles.
fn decided(source: &str) -> Vec<(String, Option<String>)> {
    let Err(errors) = check(source.as_bytes()) else {
        return Vec::new();
    };
    errors
        .into_iter()
        .filter(|e| e.message.contains("this comparison is always"))
        .map(|e| (e.message, e.fix))
        .collect()
}

/// A program that prints the comparison, of the name `x` of a range, with the
/// fact the body does not need.
fn printing(declarations: &str, comparison: &str) -> String {
    with_header(format!(
        "fn main() <console, alloc> {{\n{declarations}  print(\"{{{comparison}}}\")\n}}\n"
    ))
}

/// `x: 0..256 = 7` is a number below 256, so `x == 300` is false for every
/// value, and `x < 300` and `x >= 0` are true for every value.
fn a_number_that_the_other_side_cannot_hold_is_a_comparison_refused_with_the_fix_to_delete_it() {
    let x = "  x: 0..256 = 7\n";
    for (comparison, answer) in [
        ("x == 300", "false"),
        ("300 == x", "false"),
        ("x != 300", "true"),
        ("x < 300", "true"),
        ("x >= 0", "true"),
        ("x > 255", "false"),
    ] {
        let found = decided(&printing(x, comparison));
        assert_eq!(found.len(), 1, "{comparison}: {found:?}");
        assert!(
            found[0].0.contains(&format!("always `{answer}`")),
            "{comparison}: {found:?}"
        );
        assert!(found[0].0.contains("`x`"), "{comparison}: {found:?}");
        assert_eq!(
            found[0].1.as_deref(),
            Some("delete the comparison"),
            "{comparison}"
        );
    }
    // The numbers it can be are not decided by what it is.
    for comparison in ["x == 5", "x != 255", "x < 255", "x <= 0", "x > 0", "x >= 255"] {
        let found = decided(&printing(x, comparison));
        assert!(found.is_empty(), "{comparison}: {found:?}");
    }
}

/// One side of a comparison, the numbers it holds.
#[derive(Clone, Copy, Debug)]
struct Side {
    low: i64,
    high: i64,
    /// Whether it is written as the number itself and not as a name.
    written: bool,
}

#[derive(Debug)]
struct Pair {
    left: Side,
    right: Side,
    op: &'static str,
}

fn side(g: &mut Gen, may_be_written: bool) -> Side {
    let low = g.below(8) as i64 - 3;
    let written = may_be_written && g.one_in(3);
    let high = if written {
        low
    } else {
        low + g.below(5) as i64
    };
    Side {
        low,
        high,
        written,
    }
}

fn pair(g: &mut Gen) -> Pair {
    let left = side(g, true);
    let right = side(g, !left.written);
    let op = *g.pick(&["==", "!=", "<", "<=", ">", ">="]);
    Pair { left, right, op }
}

fn answers(op: &str, a: i64, b: i64) -> bool {
    match op {
        "==" => a == b,
        "!=" => a != b,
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        _ => a >= b,
    }
}

/// For every comparison of two whole numbers, each a name of a range or the
/// number itself, it is refused exactly when the answer is the same for every
/// pair of numbers the two ranges hold, which is found by trying every pair,
/// and it says that answer.
fn a_comparison_of_two_ranges_is_refused_exactly_when_every_pair_of_numbers_answers_alike() {
    generate::check(5801, 200, pair, |p| {
        let mut declarations = String::new();
        let mut write = |side: &Side, name: &str| {
            if side.written {
                side.low.to_string()
            } else {
                declarations.push_str(&format!(
                    "  {name}: {}..{} = {}\n",
                    side.low,
                    side.high + 1,
                    side.low
                ));
                name.to_string()
            }
        };
        let (left, right) = (write(&p.left, "x"), write(&p.right, "y"));
        let comparison = format!("{left} {} {right}", p.op);
        let mut every = Vec::new();
        for a in p.left.low..=p.left.high {
            for b in p.right.low..=p.right.high {
                every.push(answers(p.op, a, b));
            }
        }
        let alike = every.iter().all(|a| *a == every[0]);
        let found = decided(&printing(&declarations, &comparison));
        assert_eq!(found.len(), usize::from(alike), "{comparison}: {found:?}");
        if alike {
            assert!(
                found[0].0.contains(&format!("always `{}`", every[0])),
                "{comparison}: {found:?}"
            );
        }
    });
}

/// A comparison in a `requires`, in an `ensures`, and in the statement of a
/// proof is refused as one in a body is, and the fix is the same.
fn a_comparison_is_refused_in_a_requires_an_ensures_and_a_proof_as_it_is_in_a_body() {
    // `main` reaches the function, as a function that nothing reaches is not
    // checked, though a proof is.
    let main = "fn main() <console, alloc> { print(\"{f(1)}\") }\n";
    let proof = "fn main() <console, alloc> { print(\"{1}\") }\n";
    for source in [
        format!("fn f(x: 0..256) -> 0..256\n  requires x < 300\n{{\n  x\n}}\n\n{main}"),
        format!("fn f(x: 0..256) -> r: 0..256\n  ensures r >= 0\n{{\n  x\n}}\n\n{main}"),
        format!("fn p(x: 0..256) -> x == 300 {{ same }}\n\n{proof}"),
        format!("fn p(n: u64) -> n >= 0 {{ same }}\n\n{proof}"),
    ] {
        let found = decided(&source);
        assert_eq!(found.len(), 1, "{source}: {found:?}");
        assert_eq!(found[0].1.as_deref(), Some("delete the comparison"), "{source}");
    }
}

/// What `arith` decides only from a fact, and not from the two types, is not
/// refused: where `x` is below 10 by a `requires`, `x < 20` is true, and the
/// two types say nothing of it.
fn a_comparison_that_only_a_fact_decides_is_not_refused() {
    let main = "fn main() <console, alloc> { print(\"{f(1)}\") }\n";
    for source in [
        format!("fn f(x: 0..256) -> bool\n  requires x < 10\n{{\n  x < 20\n}}\n\n{main}"),
        format!("fn f(x: 0..256) -> 0..256\n  requires x < 10 and x < 20\n{{\n  x\n}}\n\n{main}"),
        format!("fn f(x: 0..256) -> r: 0..256\n  requires x < 10\n  ensures r < 20\n{{\n  x\n}}\n\n{main}"),
    ] {
        let source = with_header(source);
        let found = decided(&source);
        assert!(found.is_empty(), "{source}: {found:?}");
        assert!(check(source.as_bytes()).is_ok(), "{source}");
    }
}
