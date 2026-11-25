//! Properties of `convert` that hold for every pair of integer types: when it
//! compiles, and what it gives. What it gives for the values a generator
//! draws is in the generator of `numbers`.

use smith::check;

use crate::Test;
use crate::numbers::{INTS, header_for};

pub const ONCE: &[Test] = &[
    (
        "convert_between_whole_numbers_is_refused_and_a_wider_range_takes_a_narrower_one_as_it_is",
        convert_between_whole_numbers_is_refused_and_a_wider_range_takes_a_narrower_one_as_it_is,
    ),
    (
        "a_conversion_of_a_conversion_is_refused_for_every_type",
        a_conversion_of_a_conversion_is_refused_for_every_type,
    ),
];

/// A program with a function that takes an `x` of type `from` and gives what
/// `body` says, of type `to`.
fn program(from: &str, to: &str, body: &str) -> String {
    let program = format!(
        "fn f(x: {from}) -> {to} {{\n  {body}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{f(1)}}\")\n}}\n"
    );
    header_for(&program) + &program
}

/// `convert` from one whole number type to another is refused for every pair,
/// and where the second holds every value of the first, the error says that a
/// wider range takes a narrower one with nothing written. The function that
/// returns the value as it is compiles for exactly those pairs.
fn convert_between_whole_numbers_is_refused_and_a_wider_range_takes_a_narrower_one_as_it_is() {
    for from in INTS {
        for to in INTS {
            let source = program(from.name, to.name, "convert(x)");
            let errors = check(source.as_bytes()).expect_err("refused");
            let says = errors
                .iter()
                .any(|e| e.message.contains("nothing written"));
            assert_eq!(
                says,
                from != to && to.holds_all(*from),
                "convert from {} to {}: {errors:?}",
                from.name,
                to.name
            );
            let plain = program(from.name, to.name, "x");
            assert_eq!(
                check(plain.as_bytes()).is_ok(),
                to.holds_all(*from),
                "{} as {}",
                from.name,
                to.name
            );
        }
    }
}

/// Where no type is expected, `convert` is refused and the error says so, and
/// a conversion that would need two steps is refused too.
fn a_conversion_of_a_conversion_is_refused_for_every_type() {
    for from in INTS {
        for to in INTS.iter().filter(|t| t.holds_all(*from) && *t != from) {
            let source = program(from.name, to.name, "convert(convert(x))");
            let errors = check(source.as_bytes()).expect_err("an error");
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("expected") && e.message.contains("convert")),
                "convert of convert from {} to {}",
                from.name,
                to.name
            );
        }
    }
}
