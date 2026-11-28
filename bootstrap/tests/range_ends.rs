//! A whole number of any range is an end of a range wherever a `Nat` is: the
//! end of a piece, the range of a `loop`, and a range written as a value. A
//! range from 0 or above is taken as a `Nat` with nothing written, and gives
//! what the same number made a `Nat` gives, with `arith` knowing the bounds of
//! its type. A range that holds a number below 0 is taken only where the
//! number is shown not to be.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_number_of_a_range_is_the_end_of_a_piece_as_the_issue_shows",
        a_number_of_a_range_is_the_end_of_a_piece_as_the_issue_shows,
    ),
    (
        "a_range_from_zero_is_an_end_wherever_a_nat_is_and_gives_what_the_nat_gives",
        a_range_from_zero_is_an_end_wherever_a_nat_is_and_gives_what_the_nat_gives,
    ),
    (
        "a_range_below_zero_is_an_end_only_where_it_is_shown_not_below_zero",
        a_range_below_zero_is_an_end_only_where_it_is_shown_not_below_zero,
    ),
    (
        "a_loop_from_a_number_of_a_byte_to_a_number_past_it_runs_as_many_passes_as_a_nat_counter",
        a_loop_from_a_number_of_a_byte_to_a_number_past_it_runs_as_many_passes_as_a_nat_counter,
    ),
];

pub const MACHINE: &[Test] = &[(
    "a_loop_from_a_number_of_a_byte_to_a_number_past_it_runs_as_many_passes_on_every_executable",
    a_loop_from_a_number_of_a_byte_to_a_number_past_it_runs_as_many_passes_on_every_executable,
)];

fn output_of(source: &str) -> Vec<u8> {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    scalar::run(&program).stdout
}

fn a_number_of_a_range_is_the_end_of_a_piece_as_the_issue_shows() {
    let source = "use std.console (console, print)

fn main() <console, alloc> {
  xs = \"abc\"
  j: 1..3 = 1
  if xs.size == 3 {
    print(\"{xs.bytes[j..3].size}\")
  }
}
";
    assert_eq!(output_of(source), b"2\n".to_vec());
}

/// A program that uses `end`, a name of the range `low..high`, as each kind of
/// end of a range, in a function given a slice of `high` bytes. Where `nat`
/// is true, `end` is the number made a `Nat` first.
fn every_end(low: usize, high: usize, value: usize, nat: bool) -> String {
    let (convert, end) = match nat {
        true => ("  n: Nat = j\n", "n"),
        false => ("", "j"),
    };
    let above = high + 1;
    let text = "a".repeat(high);
    format!(
        "use std.console (console, print)

fn cut(xs: Slice[Byte], j: {low}..{high}) <console, alloc>
  requires xs.size == {high}
{{
{convert}  print(\"{{xs[{end}..{high}].size}} {{xs[..{end}].size}} {{xs[{end}..].size}}\")
  print(\"{{xs[{low}..{end}].size}} {{xs[..{end} + 1].size}}\")
  loop i in {end}..{above} {{ print(\"a{{i}}\") }}
  loop i in {end}..xs.size {{ print(\"b{{i}}\") }}
  loop i in {low}..{end} {{ print(\"c{{i}}\") }}
  r = {end}..{high}
  print(\"{{r.start}} {{r.end}}\")
}}

fn main() <console, alloc> {{
  s = \"{text}\"
  if s.bytes.size == {high} {{
    cut(s.bytes, {value})
  }}
}}
"
    )
}

fn a_range_from_zero_is_an_end_wherever_a_nat_is_and_gives_what_the_nat_gives() {
    generate::check(
        1261,
        8,
        |g: &mut Gen| {
            // Ranges kept in a byte and ranges kept wider.
            let low = match g.one_in(2) {
                true => g.below(6),
                false => 250 + g.below(20),
            };
            let high = low + 1 + g.below(5);
            (low, high, low + g.below(high - low))
        },
        |&(low, high, value)| {
            let ranged = every_end(low, high, value, false);
            let nat = every_end(low, high, value, true);
            let mut expected = format!("{} {} {}\n", high - value, value, high - value);
            expected += &format!("{} {}\n", value - low, value + 1);
            expected.extend((value..=high).map(|i| format!("a{i}\n")));
            expected.extend((value..high).map(|i| format!("b{i}\n")));
            expected.extend((low..value).map(|i| format!("c{i}\n")));
            expected += &format!("{value} {high}\n");
            assert_eq!(
                String::from_utf8(output_of(&ranged)).unwrap(),
                expected,
                "for {ranged}"
            );
            assert_eq!(output_of(&ranged), output_of(&nat), "for {nat}");
        },
    );
}

fn a_range_below_zero_is_an_end_only_where_it_is_shown_not_below_zero() {
    const USES: [&str; 5] = [
        "n = xs[j..3].size",
        "n = xs[..j].size",
        "r = j..3",
        "loop i in j..xs.size { }",
        "loop i in j..3 { }",
    ];
    generate::check(
        1262,
        6,
        |g: &mut Gen| (1 + g.below(5), 1 + g.below(3), g.below(USES.len())),
        |&(below, high, at)| {
            let program = |checked: bool| {
                let used = match checked {
                    true => format!("if j >= 0 {{ {} }}", USES[at]),
                    false => USES[at].to_string(),
                };
                format!(
                    "fn cut(xs: Slice[Byte], j: -{below}..{high})
  requires xs.size == 3
{{
  {used}
}}

fn main() {{
  s = \"abc\"
  if s.bytes.size == 3 {{
    cut(s.bytes, 0)
  }}
}}
"
                )
            };
            let unchecked = program(false);
            let errors = check(unchecked.as_bytes()).expect_err(&unchecked);
            let line = 1 + unchecked.lines().position(|l| l.contains(USES[at])).unwrap();
            assert!(
                errors.iter().any(|e| e.message.contains("`j >= 0`") && e.pos.line == line),
                "{errors:?} for {unchecked}"
            );
            let checked = program(true);
            assert!(check(checked.as_bytes()).is_ok(), "{:?} for {checked}", check(checked.as_bytes()));
        },
    );
}

/// A program that counts the passes of a `loop` from `value`, of the range
/// `low..high` that a byte holds, up to `past`, which no byte holds, and
/// prints them, with what it should print. A counter kept in a byte would
/// wrap and never reach `past`, so the loop leaves once it has counted every
/// pass the range allows, which a counter that counts right never reaches
/// before its last pass, and the program ends either way.
fn passes_past_a_byte(g: &mut Gen) -> (String, String) {
    let low = g.below(250);
    let high = low + 2 + g.below(256 - low - 1);
    let value = low + 1 + g.below(high - low - 1);
    let past = 256 + g.below(200);
    let most = past - low;
    let source = format!(
        "use std.console (console, print)

fn passes(b: {low}..{high}) -> Nat {{
  mut n = 0
  loop i in b..{past} {{
    if n == {most} {{ break }}
    n = n + 1
  }}
  n
}}

fn main() <console, alloc> {{
  print(\"{{passes({value})}}\")
}}
"
    );
    (source, format!("{}\n", past - value))
}

fn a_loop_from_a_number_of_a_byte_to_a_number_past_it_runs_as_many_passes_as_a_nat_counter() {
    generate::check(1263, 4, passes_past_a_byte, |(source, said)| {
        assert_eq!(String::from_utf8(output_of(source)).unwrap(), *said, "for {source}");
    });
}

fn a_loop_from_a_number_of_a_byte_to_a_number_past_it_runs_as_many_passes_on_every_executable() {
    let (source, said) = generate::sample(1263, 0, passes_past_a_byte);
    let said = Outcome {
        stdout: said.into_bytes(),
        status: 0,
    };
    for target in runnable() {
        runs_as(source.as_bytes(), target, &said);
    }
}
