//! A `mut` given the result of a call, where an argument of the call is an
//! `if` or a `match` whose block defines and assigns another `mut`: the
//! checker accepts it exactly when it accepts the program with that argument
//! bound to a name first, and the two print the same.

use smith::scalar;
use smith::{check, lower};

use crate::Test;

pub const ONCE: &[Test] = &[(
    "a_call_with_a_block_that_assigns_a_mut_among_its_arguments_is_checked_as_with_the_block_bound_first",
    a_call_with_a_block_that_assigns_a_mut_among_its_arguments_is_checked_as_with_the_block_bound_first,
)];

/// The ways a `mut` is declared: with its type written and without.
const DECLARED: [&str; 2] = ["", ": u64"];

/// The block of an argument that is the `if` or the `match` of the call, as
/// the value it gives.
const BLOCKS: [fn(&str, &str) -> String; 2] = [
    |inner, other| {
        format!("if b {{ 2 }} else {{\n    mut t{inner} = 0\n    t = {other}(t, 1)\n    t\n  }}")
    },
    |inner, other| {
        format!(
            "match b {{\n    true {{ 2 }}\n    false {{\n      mut t{inner} = 0\n      t = {other}(t, 1)\n      t\n    }}\n  }}"
        )
    },
];

/// The functions the call can be to: one that takes its second argument and one
/// that a number written out cannot be given to.
const CALLS: [&str; 2] = ["pick", "wrapping_add"];

fn program(outer: &str, inner: &str, call: &str, block: usize, bound: bool) -> String {
    let argument = BLOCKS[block](inner, call);
    let body = if bound {
        format!("x = {argument}\n  s = {call}(s, x)")
    } else {
        format!("s = {call}(s, {argument})")
    };
    format!(
        "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\n\n\
         fn pick(a: u64, b: u64) -> u64 {{ b }}\n\n\
         fn f(b: bool) -> u64 {{\n  mut s{outer} = 0\n  {body}\n  s\n}}\n\n\
         fn main() <console, alloc> {{\n  print(\"{{f(true)}} {{f(false)}}\")\n}}\n"
    )
}

fn prints(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    String::from_utf8(scalar::run(&program).stdout).unwrap()
}

fn a_call_with_a_block_that_assigns_a_mut_among_its_arguments_is_checked_as_with_the_block_bound_first()
 {
    for outer in DECLARED {
        for inner in DECLARED {
            for call in CALLS {
                for block in 0..BLOCKS.len() {
                    let nested = program(outer, inner, call, block, false);
                    let bound = program(outer, inner, call, block, true);
                    let accepted = check(nested.as_bytes()).is_ok();
                    assert_eq!(
                        accepted,
                        check(bound.as_bytes()).is_ok(),
                        "the program and the one with the block bound first differ in what the checker says:\n{nested}\n{bound}"
                    );
                    if accepted {
                        assert_eq!(prints(&nested), prints(&bound), "{nested}\n{bound}");
                    }
                }
            }
        }
    }
}
