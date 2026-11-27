//! Generators and properties of `try` over steps whose failures differ from
//! the failure of the function: where the failure of a step converts to the
//! function's, the function returns the converted failure, and where it does
//! not, the program is refused.
//!
//! Every generator works out the answer itself, so it shares nothing with the
//! compiler.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;

pub const ONCE: &[Test] = &[
    (
        "try_returns_the_converted_failure_of_the_first_step_that_fails_part_1_of_3",
        || try_returns_the_converted_failure_of_the_first_step_that_fails(0),
    ),
    (
        "try_returns_the_converted_failure_of_the_first_step_that_fails_part_2_of_3",
        || try_returns_the_converted_failure_of_the_first_step_that_fails(1),
    ),
    (
        "try_returns_the_converted_failure_of_the_first_step_that_fails_part_3_of_3",
        || try_returns_the_converted_failure_of_the_first_step_that_fails(2),
    ),
    (
        "try_without_a_conversion_of_a_failure_is_refused_with_both_results_named",
        try_without_a_conversion_of_a_failure_is_refused_with_both_results_named,
    ),
];

#[derive(Debug)]
struct Chain {
    source: String,
    /// What `main` prints.
    prints: String,
    /// The type of the failure that has no conversion, if one is left out.
    lacking: Option<usize>,
}

/// Steps that fail with a struct each, a function that takes them with `try`
/// and fails with an enum that holds each struct in a case of its own, or with
/// the one struct there is, and a `main` that calls it with some numbers.
/// A step fails when the number is at least its limit.
fn chain(g: &mut Gen, leave_out: bool) -> Chain {
    let steps = 1 + g.below(3);
    let identity = steps == 1 && g.one_in(2);
    let limits: Vec<usize> = (0..steps).map(|_| 1 + g.below(9)).collect();
    let lacking = (leave_out && !identity).then(|| g.below(steps));
    let mut source = String::new();
    for i in 0..steps {
        source.push_str(&format!("struct E{i} {{\n  at: Nat\n}}\n\n"));
    }
    let failure = if identity { "E0" } else { "F" };
    if !identity {
        source.push_str("enum F {\n");
        for i in 0..steps {
            if lacking != Some(i) {
                source.push_str(&format!("  f{i}: E{i}\n"));
            }
        }
        // An enum with no case that holds a failure has no conversion for it.
        source.push_str("  other\n}\n\n");
    }
    for (i, limit) in limits.iter().enumerate() {
        source.push_str(&format!(
            "fn s{i}(n: Nat) -> Result[Nat, E{i}] {{\n  if n < {limit} {{ .ok(n) }} else {{ .err(E{i}(at: n)) }}\n}}\n\n"
        ));
    }
    source.push_str(&format!("fn chain(n: Nat) -> Result[Nat, {failure}] {{\n"));
    for i in 0..steps {
        source.push_str(&format!("  v{i} = try s{i}(n)\n"));
    }
    source.push_str(&format!("  .ok(v{})\n}}\n\n", steps - 1));
    source.push_str("fn main() <console, alloc> {\n");
    let numbers: Vec<usize> = (0..4).map(|_| g.below(11)).collect();
    let mut prints = String::new();
    for n in &numbers {
        source.push_str(&format!("  print(\"{{chain({n})}}\")\n"));
        match limits.iter().position(|l| n >= l) {
            None => prints.push_str(&format!(".ok({n})\n")),
            Some(i) if identity => prints.push_str(&format!(".err(E{i}(at: {n}))\n")),
            Some(i) => prints.push_str(&format!(".err(.f{i}(E{i}(at: {n})))\n")),
        }
    }
    source.push_str("}\n");
    Chain {
        source: with_header(source),
        prints,
        lacking,
    }
}

/// For every function that takes steps with `try` and returns `Result[U, F]`,
/// where each step gives a `Result` whose failure converts to `F` or is `F`,
/// the function gives what the steps give while they succeed and the first
/// failure converted when one does not.
fn try_returns_the_converted_failure_of_the_first_step_that_fails(part: u64) {
    generate::check_part(230, 60, part, 3,|g| chain(g, false),
        |c| {
            let program = lower(c.source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(
                scalar::run(&program),
                Outcome {
                    stdout: c.prints.clone().into_bytes(),
                    status: 0
                }
            );
        },
    );
}

/// Where the failure of one step has no conversion to the function's, the
/// program is refused at that `try`, and the error names both results.
fn try_without_a_conversion_of_a_failure_is_refused_with_both_results_named() {
    generate::check(
        231,
        60,
        |g| chain(g, true),
        |c| {
            let Some(left_out) = c.lacking else {
                assert!(check(c.source.as_bytes()).is_ok());
                return;
            };
            let errors = check(c.source.as_bytes()).expect_err("refused");
            let line = c
                .source
                .lines()
                .position(|l| l.trim() == format!("v{left_out} = try s{left_out}(n)"))
                .expect("the step")
                + 1;
            assert!(
                errors.iter().any(|e| e.pos.line == line
                    && e.message.contains("no implementation of `Try`")
                    && e.message.contains(&format!("Result[Nat, E{left_out}]"))
                    && e.message.contains("Result[Nat, F]")),
                "{errors:?} is not at line {line}"
            );
        },
    );
}
