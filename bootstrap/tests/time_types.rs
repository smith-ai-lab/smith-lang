//! A point in time and a span of time are types of their own, each holding one
//! unit in a private field, so that a time that is the wrong thing cannot be
//! written. Each wrong program is refused where it is written, with a message
//! that says what is wrong, and a right one is accepted and gives the right
//! answer when it runs.

use smith::diag::{Error, Pos};
use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_wrong_use_of_a_point_or_a_span_is_refused_where_it_is_written_and_says_what_is_wrong",
        a_wrong_use_of_a_point_or_a_span_is_refused_where_it_is_written_and_says_what_is_wrong,
    ),
    (
        "a_number_is_refused_where_a_point_or_a_span_is_needed_and_the_field_of_each_is_private",
        a_number_is_refused_where_a_point_or_a_span_is_needed_and_the_field_of_each_is_private,
    ),
    (
        "a_point_and_a_span_add_to_a_point_and_two_points_between_them_are_a_span",
        a_point_and_a_span_add_to_a_point_and_two_points_between_them_are_a_span,
    ),
    (
        "a_point_is_compared_with_a_point_and_a_span_with_a_span_and_a_second_is_a_thousand_milliseconds",
        a_point_is_compared_with_a_point_and_a_span_with_a_span_and_a_second_is_a_thousand_milliseconds,
    ),
];

fn errors(source: &str) -> Vec<Error> {
    check(source.as_bytes()).err().unwrap_or_default()
}

fn outcome(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// The place of the first `needle` at or after `from`.
fn place(source: &str, from: &str, needle: &str) -> Pos {
    let at = source.find(from).unwrap() + source[source.find(from).unwrap()..].find(needle).unwrap();
    Pos {
        line: source[..at].matches('\n').count() + 1,
        col: at - source[..at].rfind('\n').map_or(0, |n| n + 1) + 1,
    }
}

/// A program whose `main` holds `line`, with the time of the library in reach.
fn program(line: &str) -> String {
    format!(
        "use std.instant (Duration, Instant, between, ms, seconds, until)\nuse std.time (now, sleep, time)\n\n\
         fn main() <time, alloc> {{\n  a = now()\n  d = ms(5)\n  {line}\n}}\n"
    )
}

/// Each wrong line, the text of it that is the wrong part, and a word the
/// message holds.
const WRONG: &[(&str, &str, &str)] = &[
    ("sleep(now())", "now()", "Instant"),
    ("x = a + a", "a + a", "Add"),
    ("x = a - a", "a - a", "Sub"),
    ("x = d + a", "d + a", "Add"),
    ("x = a < d", "d\n", "Duration"),
    ("x = a == d", "d\n", "Duration"),
    ("x = d + d", "d + d", "Add"),
    ("x = between(now(), a)", "between(now(), a)", "earlier"),
];

fn a_wrong_use_of_a_point_or_a_span_is_refused_where_it_is_written_and_says_what_is_wrong() {
    generate::check(
        9201,
        (WRONG.len() * 2) as u64,
        |g: &mut Gen| *g.pick(WRONG),
        |(line, wrong, word)| {
            let source = program(line);
            let said = errors(&source);
            let at = place(&source, line, wrong);
            assert!(
                said.iter().any(|e| e.message.contains(word) && e.pos.line == at.line),
                "{said:?} for\n{source}"
            );
        },
    );
}

fn a_number_is_refused_where_a_point_or_a_span_is_needed_and_the_field_of_each_is_private() {
    let lines = ["sleep(5)", "x = ms(a)", "x = a < 5", "x = d.ms", "x = a.ms", "x = Duration(ms: 5)", "x = Instant(ms: 5)"];
    for line in lines {
        let source = program(line);
        let said = errors(&source);
        let at = place(&source, line, line);
        assert!(said.iter().any(|e| e.pos.line == at.line), "{said:?} for\n{source}");
    }
    let private = errors(&program("x = d.ms"));
    assert!(private.iter().any(|e| e.message.contains("private")), "{private:?}");
}

fn a_point_and_a_span_add_to_a_point_and_two_points_between_them_are_a_span() {
    let spans = |g: &mut Gen| (g.below(1000), g.below(1000));
    generate::check(9202, 8, spans, |(x, y)| {
        let earlier = x.min(y);
        let later = x.max(y);
        let source = format!(
            "use std.console (console, print)\nuse std.instant (Duration, Instant, between, instant, ms, since_1970, until)\n\n\
             fn later(a: Instant, d: Duration) -> Instant\n  requires Add.valid(a, d)\n{{\n  a + d\n}}\n\n\
             fn gap(a: Instant, b: Instant) -> Duration\n  requires a <= b\n{{\n  between(a, b)\n}}\n\n\
             fn main() <console, alloc> {{\n  a = instant({earlier})\n  b = instant({later})\n  \
             print(\"{{since_1970(later(a, ms({later})))}} {{gap(a, b) == ms({later} - {earlier})}} {{until(a, b) == ms(0)}} {{until(b, a) == ms({later} - {earlier})}}\")\n}}\n"
        );
        let said = String::from_utf8(outcome(&source).stdout).unwrap();
        assert_eq!(said, format!("{} true true true\n", earlier + later), "for\n{source}");
    });
}

fn a_point_is_compared_with_a_point_and_a_span_with_a_span_and_a_second_is_a_thousand_milliseconds() {
    let source = "use std.console (console, print)\nuse std.instant (instant, ms, seconds)\n\n\
                  fn main() <console, alloc> {\n  \
                  print(\"{instant(1) < instant(2)} {instant(2) <= instant(2)} {ms(1000) == seconds(1)} {ms(999) < seconds(1)}\")\n}\n";
    let said = String::from_utf8(outcome(source).stdout).unwrap();
    assert_eq!(said, "true true true true\n");
}
