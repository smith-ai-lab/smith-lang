//! A comment on the line above a `requires` is the message of a call that
//! fails it, and the goal that failed is shown under it. It is for the one
//! `requires` after it. A `requires` with none, or with a blank line between
//! it and the comment, is refused as it was. The expectation comes from the
//! generator that wrote the comments and the numbers.

use std::collections::BTreeSet;

use smith::{check, diag};

use crate::Test;
use crate::contracts::{near, statement};
use crate::generate::{self, Gen};
use crate::numbers::{INTS, with_header};

pub const ONCE: &[Test] = &[
    (
        "a_failed_call_is_told_the_comment_above_the_requires_it_fails_and_no_other",
        a_failed_call_is_told_the_comment_above_the_requires_it_fails_and_no_other,
    ),
    (
        "a_comment_above_a_requires_of_a_trait_function_is_told_at_a_call_through_an_impl",
        a_comment_above_a_requires_of_a_trait_function_is_told_at_a_call_through_an_impl,
    ),
    (
        "a_comment_above_a_requires_that_the_kernel_works_out_is_told_with_what_it_found",
        a_comment_above_a_requires_that_the_kernel_works_out_is_told_with_what_it_found,
    ),
    (
        "alike_requires_that_fail_are_one_error_and_a_comment_apart_from_its_requires_is_not_told",
        alike_requires_that_fail_are_one_error_and_a_comment_apart_from_its_requires_is_not_told,
    ),
    (
        "the_goal_is_written_under_the_comment",
        the_goal_is_written_under_the_comment,
    ),
];

const WORDS: &[&str] = &[
    "The", "first", "number", "must", "not", "be", "below", "the", "second", "Swap", "them",
    "check", "again", "too", "big",
];

/// The lines of a comment, as the text after `// `.
fn words(g: &mut Gen) -> Vec<String> {
    (0..1 + g.below(2))
        .map(|_| {
            let n = 1 + g.below(4);
            (0..n).map(|_| *g.pick(WORDS)).collect::<Vec<_>>().join(" ")
        })
        .collect()
}

/// What stands above a `requires`.
#[derive(Clone, Debug)]
enum Above {
    Nothing,
    /// A comment directly above.
    Comment(Vec<String>),
    /// A comment with a blank line between it and the `requires`.
    Apart(Vec<String>),
}

impl Above {
    fn written(&self, indent: &str) -> String {
        let lines = |lines: &[String]| -> String {
            lines.iter().map(|l| format!("{indent}// {l}\n")).collect()
        };
        match self {
            Above::Nothing => String::new(),
            Above::Comment(l) => lines(l),
            Above::Apart(l) => format!("{}\n", lines(l)),
        }
    }

    /// What a caller is told, if it is the comment's.
    fn told(&self) -> Option<String> {
        match self {
            Above::Comment(l) => Some(l.join("\n")),
            _ => None,
        }
    }
}

fn above(g: &mut Gen) -> Above {
    match g.below(3) {
        0 => Above::Nothing,
        1 => Above::Comment(words(g)),
        _ => Above::Apart(words(g)),
    }
}

#[derive(Debug)]
struct Call {
    source: String,
    /// For each `requires`, what is above it, how it is written, and whether
    /// the argument meets it.
    requires: Vec<(Above, String, bool)>,
}

fn call(g: &mut Gen) -> Call {
    let ty = *g.pick(INTS);
    let arg = near(g, ty);
    let mut requires = Vec::new();
    let mut lines = String::new();
    for _ in 0..1 + g.below(3) {
        let (text, f) = statement(g, ty, "i", 1);
        let over = above(g);
        lines.push_str(&over.written("  "));
        lines.push_str(&format!("  requires {text}\n"));
        requires.push((over, text, f(arg)));
    }
    let t = ty.name;
    Call {
        source: with_header(format!(
            "fn needs(i: {t}) -> {t}\n{lines}{{\n  i\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{needs({arg})}}\")\n}}\n"
        )),
        requires,
    }
}

/// For every function with several `requires`, each with a comment above it, a
/// blank line and a comment, or nothing, a call that fails some of them is
/// refused once for each it fails, with the comment of the one it fails as the
/// message and the goal under it, and with no comment of another.
fn a_failed_call_is_told_the_comment_above_the_requires_it_fails_and_no_other() {
    generate::check(7911, 150, call, |c| {
        let failed: Vec<(&Above, &String)> = (c.requires.iter())
            .filter(|(_, _, holds)| !holds)
            .map(|(over, text, _)| (over, text))
            .collect();
        let errors = match check(c.source.as_bytes()) {
            Ok(_) => {
                assert!(failed.is_empty(), "accepted, though a `requires` fails");
                return;
            }
            Err(errors) => errors,
        };
        // Errors that are the same, in the message and the goal, are reported
        // once, so `requires` that are written alike and fail are one error.
        let told: BTreeSet<(String, &String)> = (failed.iter())
            .filter_map(|(over, text)| over.told().map(|t| (t, *text)))
            .collect();
        let plain: BTreeSet<&String> = (failed.iter())
            .filter(|(over, _)| over.told().is_none())
            .map(|(_, text)| *text)
            .collect();
        let with_goal = errors.iter().filter(|e| e.goal.is_some());
        let without = errors.iter().filter(|e| e.goal.is_none());
        assert_eq!(with_goal.clone().count(), told.len(), "{errors:?}");
        assert_eq!(without.clone().count(), plain.len(), "{errors:?}");
        // The comment of each is the message of an error with a goal, and no
        // other comment is.
        let messages: BTreeSet<&String> = with_goal.map(|e| &e.message).collect();
        assert_eq!(
            messages,
            told.iter().map(|(t, _)| t).collect::<BTreeSet<_>>(),
            "{errors:?}"
        );
        assert!(
            errors.iter().filter(|e| e.goal.is_none()).all(|e| e.message.contains("needs")),
            "{errors:?}"
        );
    });
}

/// A comment above the `requires` of a function of a trait is the message of a
/// call that fails it, whichever `impl` the call goes through.
fn a_comment_above_a_requires_of_a_trait_function_is_told_at_a_call_through_an_impl() {
    generate::check(7912, 30, words, |lines| {
        let comment: String = lines.iter().map(|l| format!("    // {l}\n")).collect();
        let source = with_header(format!(
            "trait Plus[A, B = A, out C = A] {{\n  fn valid(a: A, b: B) -> bool\n  fn plus(a: A, b: B) -> C\n{comment}    requires valid(a, b)\n}}\n\n\
             impl Plus[u8] {{\n  fn valid(a: u8, b: u8) -> bool {{ a <= 255 - b }}\n  fn plus(a: u8, b: u8) -> u8 {{ a + b }}\n}}\n\n\
             fn main() <console, alloc> {{\n  n: u8 = 200\n  print(\"{{Plus.plus(n, n)}}\")\n}}\n"
        ));
        let errors = check(source.as_bytes()).expect_err("a call that fails its `requires`");
        assert!(
            errors
                .iter()
                .any(|e| e.message == lines.join("\n") && e.goal.is_some()),
            "{errors:?} for\n{source}"
        );
    });
}

/// A `requires` that calls a predicate which the kernel works out is told as
/// the comment above it, with what the kernel found under it.
fn a_comment_above_a_requires_that_the_kernel_works_out_is_told_with_what_it_found() {
    generate::check(7913, 20, words, |lines| {
        let comment: String = lines.iter().map(|l| format!("  // {l}\n")).collect();
        let source = with_header(format!(
            "fn has(xs: List[i32], k: i32) -> bool {{\n  var found = false\n  loop v in xs {{\n    if v == k {{\n      found = true\n      break\n    }}\n  }}\n  found\n}}\n\n\
             fn pick(xs: List[i32], k: i32) -> i32\n{comment}  requires has(xs, k)\n{{\n  k\n}}\n\n\
             fn main() <console, alloc> {{ print(\"{{pick([1, 2, 3], 5)}}\") }}\n"
        ));
        let errors = check(source.as_bytes()).expect_err("a call that fails its `requires`");
        assert!(
            errors.iter().any(|e| e.message == lines.join("\n")
                && e.goal.as_ref().is_some_and(|g| g.contains("has"))),
            "{errors:?} for\n{source}"
        );
    });
}

/// The case the merge queue found: two `requires` written alike that fail are
/// one error, and a comment with a blank line between it and a `requires` is
/// the comment of none, as [Functions](functions.md#comments) says of every
/// item, so that `requires` is refused with no comment.
fn alike_requires_that_fail_are_one_error_and_a_comment_apart_from_its_requires_is_not_told() {
    let source = with_header(
        "fn needs(i: Byte) -> Byte\n  requires 2 < i\n  // The\n\n  requires 2 >= i\n  requires 2 >= i\n{\n  i\n}\n\nfn main() <console, alloc> {\n  print(\"{needs(253)}\")\n}\n"
            .to_string(),
    );
    let errors = check(source.as_bytes()).expect_err("a call that fails its `requires`");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].goal.is_none(), "{errors:?}");
    assert!(errors[0].message.contains("needs"), "{errors:?}");
}

/// The comment is written first, and the goal after it, before the line of
/// source.
fn the_goal_is_written_under_the_comment() {
    generate::check(7914, 20, words, |lines| {
        let comment: String = lines.iter().map(|l| format!("  // {l}\n")).collect();
        let source = with_header(format!(
            "fn gap(a: u8, b: u8) -> u8\n{comment}  requires a >= b\n{{\n  a - b\n}}\n\nfn main() <console, alloc> {{ print(\"{{gap(3, 5)}}\") }}\n"
        ));
        let errors = check(source.as_bytes()).expect_err("a call that fails its `requires`");
        let shown = diag::render("main.smith", source.as_bytes(), &errors);
        let told = shown.find(&lines.join("\n")).expect("the comment is shown");
        let goal = shown.find("3 >= 5").expect("the goal is shown");
        let line = shown.find("gap(3, 5)").expect("the line is shown");
        assert!(told < goal && goal < line, "{shown}");
    });
}
