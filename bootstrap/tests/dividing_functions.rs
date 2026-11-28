//! `wrapping_div` and `wrapping_rem` keep the low bits of the quotient and
//! need the divisor proven not to be zero, and `arith` knows what a quotient
//! and what is left over cannot be.

use smith::lower;
use smith::scalar;

use crate::generate::{self, Gen};
use crate::numbers::{INTS, Int, value, with_header};
use crate::targets::{runnable, runs_as};
use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "wrapping_div_and_wrapping_rem_keep_the_low_bits_of_the_quotient",
        wrapping_div_and_wrapping_rem_keep_the_low_bits_of_the_quotient,
    ),
    (
        "wrapping_div_and_wrapping_rem_are_refused_where_the_divisor_may_be_zero",
        wrapping_div_and_wrapping_rem_are_refused_where_the_divisor_may_be_zero,
    ),
    (
        "arith_knows_what_a_quotient_and_what_is_left_over_cannot_be",
        arith_knows_what_a_quotient_and_what_is_left_over_cannot_be,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_of_the_functions_that_divide_gives_what_the_evaluator_gives",
    every_executable_of_the_functions_that_divide_gives_what_the_evaluator_gives,
)];

/// The low bits of the answer: what the type holds that it is congruent to.
fn wrapped(ty: Int, a: i128, b: i128, rem: bool) -> i128 {
    let n = if rem { a % b } else { a / b };
    let span = 1i128 << ty.bits.min(120);
    let low = n.rem_euclid(span);
    if ty.signed && low > ty.max() {
        low - span
    } else {
        low
    }
}

/// A program that prints each of the functions of each pair whose divisor is
/// not zero, a constant.
fn program(ty: Int, pairs: &[(i128, i128)]) -> String {
    let t = ty.name;
    let mut text = "use std.console (console, print)\nuse std.int (wrapping_div, wrapping_rem)\n\nfn main() <console, alloc> {\n".to_string();
    for (i, (a, b)) in pairs.iter().enumerate() {
        text.push_str(&format!("  a{i}: {t} = {a}\n"));
        if *b != 0 {
            text.push_str(&format!(
                "  print(\"{{wrapping_div(a{i}, {b})}} {{wrapping_rem(a{i}, {b})}}\")\n"
            ));
        }
    }
    text.push_str("}\n");
    with_header(text)
}

fn pairs(g: &mut Gen, ty: Int) -> Vec<(i128, i128)> {
    let mut out = Vec::new();
    for _ in 0..g.below(5) + 1 {
        let (a, b) = if g.one_in(3) {
            (ty.min(), *g.pick(&[-1, 0, 1, 2]))
        } else {
            (value(g, ty), value(g, ty))
        };
        if !ty.signed && b < 0 || b < ty.min() || b > ty.max() {
            continue;
        }
        out.push((a, b));
    }
    out
}

fn expected(ty: Int, pairs: &[(i128, i128)]) -> String {
    let mut out = String::new();
    for (a, b) in pairs {
        if *b != 0 {
            out.push_str(&format!(
                "{} {}\n",
                wrapped(ty, *a, *b, false),
                wrapped(ty, *a, *b, true)
            ));
        }
    }
    out
}

fn wrapping_div_and_wrapping_rem_keep_the_low_bits_of_the_quotient() {
    for ty in INTS.iter().filter(|t| t.signed) {
        let pairs = [(ty.min(), -1), (ty.min(), 1), (ty.max(), -1), (-7i128.max(ty.min()), 2)];
        let source = program(*ty, &pairs);
        let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
        let said = String::from_utf8(scalar::run(&lowered).stdout).unwrap();
        assert_eq!(said, expected(*ty, &pairs), "{}", ty.name);
        // The least number divided by -1 is itself, and leaves nothing.
        assert!(said.lines().any(|l| l == format!("{} 0", ty.min())), "{said}");
    }
}

/// What the checker says of a function `f` of two numbers that is called.
fn refused(function: &str) -> Result<(), String> {
    let source = with_header(format!(
        "use std.bits (i8)\nuse std.console (console, print)\nuse std.int (wrapping_div, wrapping_rem)\n\n{function}\n\nfn main() <console, alloc> {{\n  print(\"{{f(1, 2)}}\")\n}}\n"
    ));
    lower(source.as_bytes()).map(|_| ()).map_err(|e| format!("{e:?}"))
}

fn wrapping_div_and_wrapping_rem_are_refused_where_the_divisor_may_be_zero() {
    // The trait's function and the library's, called as `std.int` has it.
    for call in ["Wrapping.", ""] {
        for f in ["wrapping_div", "wrapping_rem"] {
            let write = |args: &str, requires: &str, guard: bool| {
                let call = format!("{call}{f}({args})");
                let body = if guard {
                    format!("if b > 0 {{ {call} }} else {{ 0 }}")
                } else {
                    call
                };
                refused(&format!("fn f(a: i8, b: i8) -> i8 {requires}{{ {body} }}"))
            };
            let error = write("a, b", "", false).unwrap_err();
            assert!(error.contains("cannot prove that `b != 0`"), "{error}");
            assert!(error.contains(&format!("`{f}(a, b)` may divide by zero")), "{error}");
            let error = write("a, 0", "", false).unwrap_err();
            assert!(error.contains("divides by zero"), "{error}");
            assert_eq!(write("a, b", "requires b != 0 ", false), Ok(()));
            assert_eq!(write("a, b", "", true), Ok(()));
        }
    }
}

fn arith_knows_what_a_quotient_and_what_is_left_over_cannot_be() {
    let proven = [
        // The quotient is not above the number, and neither is what is left.
        "fn f(n: Nat, m: Nat) -> Nat {\n  h: Nat = n / 2\n  n - h\n}",
        "fn f(n: Nat, m: Nat) -> Nat {\n  r: Nat = n % 7\n  n - r\n}",
        // What is left over is below the divisor.
        "fn f(a: Nat, b: Nat) -> Nat requires b != 0 {\n  r: Nat = a % b\n  b - 1 - r\n}",
        // A quotient by a number, times it, is not above the number.
        "fn f(n: Nat, m: Nat) -> Nat {\n  q: Nat = n / 8\n  r: Nat = q * 8\n  n - r\n}",
    ];
    for function in proven {
        let source = with_header(format!(
            "use std.console (console, print)\n\n{function}\n\nfn main() <console, alloc> {{\n  print(\"{{f(25, 7)}}\")\n}}\n"
        ));
        lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    }
    // Nothing is claimed that some numbers make false: a quotient of a number
    // that may be negative is below a number only by its size, and the least
    // number divided by -1 is not a number the type holds.
    let refused = [
        "fn f(n: Int, m: Nat) -> Nat {\n  h: Nat = n / 2\n  n - h\n}",
        "fn f(a: Nat, b: Nat) -> Nat requires b != 0 {\n  r: Nat = a % b\n  b - 2 - r\n}",
    ];
    for function in refused {
        let source = with_header(format!(
            "use std.console (console, print)\n\n{function}\n\nfn main() <console, alloc> {{\n  print(\"{{f(25, 7)}}\")\n}}\n"
        ));
        assert!(lower(source.as_bytes()).is_err(), "{function}");
    }
}

fn every_executable_of_the_functions_that_divide_gives_what_the_evaluator_gives() {
    generate::check(
        5202,
        12,
        |g| {
            let ty = *g.pick(INTS);
            (ty, pairs(g, ty))
        },
        |(ty, pairs)| {
            let source = program(*ty, pairs);
            let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
            let said = scalar::run(&scalar::passes(&lowered));
            for target in runnable() {
                runs_as(source.as_bytes(), target, &said);
            }
        },
    );
}
