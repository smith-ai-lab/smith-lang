//! A whole number bound to a name with no type keeps its own range, wherever
//! the name is used beside a value that has a type, so a program is accepted
//! or refused exactly as the one that writes that range for the name is. A
//! number written in place, with no name, is still decided by its use.

use smith::scalar;
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_number_bound_to_a_name_keeps_its_range_beside_a_typed_value_in_a_function_a_var_and_a_lambda",
        a_number_bound_to_a_name_keeps_its_range_beside_a_typed_value_in_a_function_a_var_and_a_lambda,
    ),
    (
        "a_number_written_in_place_beside_a_typed_value_is_still_decided_by_its_use",
        a_number_written_in_place_beside_a_typed_value_is_still_decided_by_its_use,
    ),
    (
        "a_bound_number_beside_a_typed_value_is_accepted_and_refused_as_the_range_written_is_part_1_of_3",
        || a_bound_number_beside_a_typed_value_is_accepted_and_refused_as_the_range_written_is(0),
    ),
    (
        "a_bound_number_beside_a_typed_value_is_accepted_and_refused_as_the_range_written_is_part_2_of_3",
        || a_bound_number_beside_a_typed_value_is_accepted_and_refused_as_the_range_written_is(1),
    ),
    (
        "a_bound_number_beside_a_typed_value_is_accepted_and_refused_as_the_range_written_is_part_3_of_3",
        || a_bound_number_beside_a_typed_value_is_accepted_and_refused_as_the_range_written_is(2),
    ),
    (
        "a_comparison_that_the_range_of_a_bound_number_decides_is_refused_as_with_the_range_written",
        a_comparison_that_the_range_of_a_bound_number_decides_is_refused_as_with_the_range_written,
    ),
];

fn printed(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    String::from_utf8(scalar::run(&program).stdout).expect("text")
}

fn a_number_bound_to_a_name_keeps_its_range_beside_a_typed_value_in_a_function_a_var_and_a_lambda() {
    let head = "use std.console (console, print)\n\n";
    let sources = [
        format!("{head}fn g(a: 1..3) -> 11..13 {{\n  k = 10\n  a + k\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{g(2)}}\")\n}}\n"),
        format!("{head}fn g(a: 1..3) -> 11..13 {{\n  var k = 10\n  a + k\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{g(2)}}\")\n}}\n"),
    ];
    for source in sources {
        assert_eq!(printed(&source), "12\n", "for\n{source}");
    }
    let lambda = format!(
        "{head}fn main() <console, alloc> {{\n  k = 10\n  f = fn(a) {{ a + k }}\n  print(\"{{f(1)}} {{f(2)}}\")\n}}\n"
    );
    assert_eq!(printed(&lambda), "11 12\n");
}

fn a_number_written_in_place_beside_a_typed_value_is_still_decided_by_its_use() {
    let source = "use std.console (console, print)\n\nfn g(a: 1..3) -> 11..13 { a + 10 }\n\n\
                  fn main() <console, alloc> {\n  print(\"{g(2)}\")\n}\n";
    assert_eq!(printed(source), "12\n");
}

fn a_comparison_that_the_range_of_a_bound_number_decides_is_refused_as_with_the_range_written() {
    let program = |binding: &str| {
        format!(
            "use std.console (console, print)\n\nfn main() <console> {{\n  a: -15..-13 = -15\n  {binding}\n  \
             if a < k {{ print(\"less\") }} else {{ print(\"not\") }}\n}}\n"
        )
    };
    for binding in ["k = -15", "k: -15..-14 = -15"] {
        let source = program(binding);
        let errors = check(source.as_bytes()).expect_err(&source);
        assert!(
            errors.iter().any(|e| e.message.contains("this comparison is always `false`")),
            "{errors:?} for\n{source}"
        );
    }
}

/// A binding of `n`, and how the name is used beside `a`, a value of the range
/// `low..high`.
#[derive(Clone, Debug)]
struct Use {
    n: i128,
    low: i128,
    high: i128,
    how: usize,
}

fn a_bound_number_beside_a_typed_value_is_accepted_and_refused_as_the_range_written_is(part: u64) {
    generate::check_part(
        4601,
        120,
        part,
        3,
        |g: &mut Gen| {
            let low = g.below(40) as i128 - 20;
            Use {
                n: g.below(400) as i128 - 100,
                low,
                high: low + 1 + g.below(30) as i128,
                how: g.below(5),
            }
        },
        |u| {
            let body = match u.how {
                0 => "  y = a + k\n  print(\"{y}\")\n",
                1 => "  y = a - k\n  print(\"{y}\")\n",
                2 => "  y = a * k\n  print(\"{y}\")\n",
                3 => "  if a < k { print(\"less\") } else { print(\"not\") }\n",
                _ => "  y = takes(k)\n  print(\"{y}\")\n",
            };
            let program = |binding: String| {
                format!(
                    "use std.console (console, print)\n\n\
                     fn takes(b: {}..{}) -> {}..{} {{ b }}\n\n\
                     fn main() <console, alloc> {{\n  a: {}..{} = {}\n  {binding}\n{body}}}\n",
                    u.low, u.high, u.low, u.high, u.low, u.high, u.low
                )
            };
            let bound = program(format!("k = {}", u.n));
            let written = program(format!("k: {}..{} = {}", u.n, u.n + 1, u.n));
            match (check(written.as_bytes()), check(bound.as_bytes())) {
                (Ok(_), Ok(_)) => assert_eq!(printed(&bound), printed(&written), "for\n{bound}"),
                (Err(_), Err(_)) => {}
                (written_says, bound_says) => panic!(
                    "the range written gives {written_says:?}, the number gives {bound_says:?} for\n{bound}"
                ),
            }
        },
    );
}
