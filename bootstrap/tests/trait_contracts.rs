//! Generators and properties of a `requires` on a trait's function, and of a
//! `requires` that names a trait's function, which `arith` reads through the
//! implementation the types choose: a call compiles exactly when the `valid`
//! of the implementation holds of its arguments.
//!
//! The generator works out the answer by trying the values, so it shares
//! nothing with the compiler.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::contracts::{near, statement};
use crate::generate::{self, Gen};
use crate::numbers::{INTS, with_header};

pub const ONCE: &[Test] = &[
    (
        "a_call_of_a_trait_function_compiles_exactly_when_the_valid_of_its_implementation_holds",
        a_call_of_a_trait_function_compiles_exactly_when_the_valid_of_its_implementation_holds,
    ),
    (
        "a_generic_function_without_the_requires_is_refused_at_the_call_in_its_body",
        a_generic_function_without_the_requires_is_refused_at_the_call_in_its_body,
    ),
];

#[derive(Debug)]
struct Case {
    source: String,
    /// Whether the implementation's `valid` holds of the arguments.
    holds: bool,
    /// What the program prints when it compiles.
    prints: String,
    /// Whether the program compiles whatever the arguments are.
    always: bool,
}

const TRAIT: &str = "trait Plus[A, B = A, out C = A] {
  fn valid(a: A, b: B) -> bool
  fn plus(a: A, b: B) -> C
    requires valid(a, b)
}
";

/// A trait with a function that requires its `valid`, an `impl` for a whole
/// number type that says `valid` with a statement about each argument, and a
/// `main` that calls the function directly, through a generic function that
/// states the same `requires`, or after asking `valid` in an `if`.
fn case(g: &mut Gen) -> Case {
    let ty = *g.pick(INTS);
    let t = ty.name;
    let (a, b) = (near(g, ty), near(g, ty));
    let (left, f) = statement(g, ty, "a", 1);
    let (right, h) = statement(g, ty, "b", 1);
    let joined_by_and = g.one_in(2);
    let (valid, holds) = if joined_by_and {
        (format!("({left}) and ({right})"), f(a) && h(b))
    } else {
        (format!("({left}) or ({right})"), f(a) || h(b))
    };
    let imp = format!(
        "impl Plus[{t}] {{\n  fn valid(a: {t}, b: {t}) -> bool {{ {valid} }}\n  fn plus(a: {t}, b: {t}) -> {t} {{ a }}\n}}\n"
    );
    let bind = format!("  x: {t} = {a}\n  y: {t} = {b}\n");
    let (rest, always) = match g.below(3) {
        0 => (
            format!("fn main() <console, alloc> {{\n{bind}  print(\"{{Plus.plus(x, y)}}\")\n}}\n"),
            false,
        ),
        1 => (
            format!(
                "fn both[T: Plus](x: T, y: T) -> T\n  requires Plus.valid(x, y)\n{{\n  Plus.plus(x, y)\n}}\n\nfn main() <console, alloc> {{\n{bind}  print(\"{{both(x, y)}}\")\n}}\n"
            ),
            false,
        ),
        _ => (
            format!(
                "fn main() <console, alloc> {{\n{bind}  if Plus.valid(x, y) {{\n    print(\"{{Plus.plus(x, y)}}\")\n  }} else {{\n    print(\"no\")\n  }}\n}}\n"
            ),
            true,
        ),
    };
    let guarded = rest.contains("if Plus.valid");
    let prints = if guarded && !holds {
        "no".to_string()
    } else {
        a.to_string()
    };
    Case {
        source: with_header(format!("{TRAIT}\n{imp}\n{rest}")),
        holds,
        prints,
        always,
    }
}

/// For every trait whose function requires its `valid`, every `impl` that
/// says `valid` for a type, and every call, direct or through a generic
/// function that states the same `requires`, the call compiles exactly when
/// the `valid` of the `impl` its type picks holds of its arguments, and a
/// call after `valid` was asked in an `if` always does.
fn a_call_of_a_trait_function_compiles_exactly_when_the_valid_of_its_implementation_holds() {
    generate::check(210, 150, case, |c| match check(c.source.as_bytes()) {
        Ok(_) => {
            assert!(
                c.holds || c.always,
                "accepted, though `valid` does not hold"
            );
            let program = lower(c.source.as_bytes()).expect("lowers");
            assert_eq!(
                scalar::run(&program),
                Outcome {
                    stdout: format!("{}\n", c.prints).into_bytes(),
                    status: 0
                }
            );
        }
        Err(errors) => {
            assert!(
                !c.holds && !c.always,
                "refused, though `valid` holds: {errors:?}"
            );
            assert!(
                errors.iter().any(|e| e.message.contains("cannot prove")),
                "{errors:?} does not say what could not be proven"
            );
        }
    });
}

/// For every implementation, a generic function that calls a trait's function
/// and does not state the `requires` of it is refused where it calls, and the
/// same function with the `requires` is not.
fn a_generic_function_without_the_requires_is_refused_at_the_call_in_its_body() {
    generate::check(211, 40, case, |c| {
        let stated = c.source.contains("requires Plus.valid");
        if !stated {
            return;
        }
        let without = c.source.replace("  requires Plus.valid(x, y)\n", "");
        let errors = check(without.as_bytes()).expect_err("refused without the requires");
        let at = without.find("  Plus.plus(x, y)\n").expect("in the body");
        let line = without[..at].matches('\n').count() + 1;
        assert!(
            errors.iter().any(|e| e.pos.line == line),
            "{errors:?} is not at line {line}"
        );
    });
}
