//! A range written as a value is a `Range`, a struct of the prelude with two
//! `Nat`, `start` and `end`. It is still a type where a type is expected, the
//! range of a `loop`, and a piece in an index, and the arithmetic prover keeps
//! what it knows of the ends.

use smith::scalar;
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_range_written_as_a_value_has_the_ends_it_was_written_with_and_is_given_to_a_function_that_takes_one",
        a_range_written_as_a_value_has_the_ends_it_was_written_with_and_is_given_to_a_function_that_takes_one,
    ),
    (
        "a_range_is_still_a_loop_head_a_piece_and_a_type_where_they_were",
        a_range_is_still_a_loop_head_a_piece_and_a_type_where_they_were,
    ),
    (
        "what_arith_knows_of_the_ends_a_range_was_written_with_is_kept_and_nothing_more",
        what_arith_knows_of_the_ends_a_range_was_written_with_is_kept_and_nothing_more,
    ),
    (
        "an_end_that_is_not_a_whole_number_with_no_sign_is_refused_where_the_range_is_written",
        an_end_that_is_not_a_whole_number_with_no_sign_is_refused_where_the_range_is_written,
    ),
    (
        "a_range_is_copied_freely_and_asks_for_no_effect",
        a_range_is_copied_freely_and_asks_for_no_effect,
    ),
];

fn output_of(source: &str) -> Vec<u8> {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    scalar::run(&program).stdout
}

fn a_range_written_as_a_value_has_the_ends_it_was_written_with_and_is_given_to_a_function_that_takes_one() {
    generate::check(
        4417,
        40,
        |g: &mut Gen| (g.below(1000), g.below(1000)),
        |&(a, b)| {
            let source = format!(
                "use std.console (console, print)

fn ends(r: Range) -> (Nat, Nat) {{ (r.start, r.end) }}

fn main() <console, alloc> {{
  r = {a}..{b}
  print(\"{{r.start}} {{r.end}}\")
  (x, y) = ends({a}..{b})
  print(\"{{x}} {{y}}\")
}}
"
            );
            // A range is written only where its ends are proven in order.
            if a > b {
                let errors = check(source.as_bytes()).expect_err("a range written backward");
                assert!(
                    errors.iter().any(|e| e.message.contains(&format!("{a} <= {b}"))),
                    "{errors:?} for {source}"
                );
                return;
            }
            let line = format!("{a} {b}\n");
            assert_eq!(output_of(&source), format!("{line}{line}").into_bytes(), "for {source}");
        },
    );
}

fn a_range_is_still_a_loop_head_a_piece_and_a_type_where_they_were() {
    let source = "use std.console (console, print)
use std.list (freeze)

fn main() <console, alloc> {
  loop i in 2..5 { print(\"{i}\") }
  bytes: List[Byte] = [10, 20, 30, 40]
  xs = freeze(bytes)
  print(\"{xs[1..3].size} {xs[..2].size} {xs[2..].size}\")
  small: 0..10 = 7
  print(\"{small}\")
}
";
    assert_eq!(output_of(source), b"2\n3\n4\n2 2 2\n7\n".to_vec());
}

fn what_arith_knows_of_the_ends_a_range_was_written_with_is_kept_and_nothing_more() {
    let program = |contract: &str, written: &str| {
        format!(
            "fn cut(xs: Slice[Byte], from: Nat, to: Nat) -> Nat
{contract}{{
  r = {written}
  xs[r.start..r.end].size
}}

fn main() {{
  xs: Slice[Byte] = [1, 2, 3]
  n = cut(xs, 0, 3)
}}
"
        )
    };
    let proven = program("  requires from <= to and to <= xs.size\n", "from..to");
    assert!(check(proven.as_bytes()).is_ok(), "{:?}", check(proven.as_bytes()));
    let unproven = program("", "from..to");
    let errors = check(unproven.as_bytes()).expect_err("a piece that is not proven inside");
    assert!(errors.iter().any(|e| e.message.contains("cannot prove")), "{errors:?}");
    // The ends are the ones written, not the ends of another range.
    let swapped = program("  requires from <= to and to <= xs.size\n", "to..from");
    let errors = check(swapped.as_bytes()).expect_err("ends the other way round");
    assert!(errors.iter().any(|e| e.message.contains("cannot prove")), "{errors:?}");
}

fn an_end_that_is_not_a_whole_number_with_no_sign_is_refused_where_the_range_is_written() {
    for written in ["-1..3", "0..-3", "0.5..3"] {
        let source = format!("fn main() {{\n  r = {written}\n}}\n");
        let errors = check(source.as_bytes()).expect_err(&source);
        assert!(!errors.is_empty(), "{written} was accepted");
        assert!(
            errors.iter().all(|e| e.pos.line == 2),
            "{written} was refused somewhere else: {errors:?}"
        );
    }
}

fn a_range_is_copied_freely_and_asks_for_no_effect() {
    let source = "fn width(r: Range) -> Nat\n  requires r.start <= r.end\n{ r.end - r.start }

fn twice(r: Range) -> (Nat, Nat) { (r.start, r.end) }

fn main() {
  r = 1..4
  a = twice(r)
  b = twice(r)
}
";
    assert!(check(source.as_bytes()).is_ok(), "{:?}", check(source.as_bytes()));
}
