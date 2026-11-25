//! What `std.http` writes cannot be wrong: a status is one of `100..600`, a
//! header's name and value are types that only a name that is a token the
//! library does not write and a value with no line break are made into, and a
//! request's method is a `Method`. Each wrong program is refused where it is
//! written, with a message that names what is wrong, and the same text known
//! only when the program runs is a `Result`.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_header_the_library_writes_is_refused_where_it_is_written_in_any_case_of_its_letters",
        a_header_the_library_writes_is_refused_where_it_is_written_in_any_case_of_its_letters,
    ),
    (
        "a_name_that_is_no_token_and_a_value_with_a_line_break_are_refused_where_they_are_written",
        a_name_that_is_no_token_and_a_value_with_a_line_break_are_refused_where_they_are_written,
    ),
    (
        "a_status_outside_the_range_of_statuses_is_refused_where_it_is_written",
        a_status_outside_the_range_of_statuses_is_refused_where_it_is_written,
    ),
    (
        "a_method_that_is_a_text_is_refused_for_a_request",
        a_method_that_is_a_text_is_refused_for_a_request,
    ),
    (
        "a_name_and_a_value_known_when_the_program_runs_are_made_by_check_or_say_why_not",
        a_name_and_a_value_known_when_the_program_runs_are_made_by_check_or_say_why_not,
    ),
];

fn messages(source: &str) -> Vec<String> {
    check(source.as_bytes())
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.message)
        .collect()
}

fn outcome(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// A program with one statement in a function that has `alloc`.
fn program(uses: &str, line: &str) -> String {
    format!("use std.http ({uses})\n\nfn main() <alloc> {{\n  {line}\n}}\n")
}

/// The text with the case of each letter chosen by the generator.
fn cased(g: &mut Gen, text: &str) -> String {
    text.chars()
        .map(|c| if g.below(2) == 0 { c.to_ascii_uppercase() } else { c })
        .collect()
}

const RESERVED: &[&str] = &["content-length", "connection", "transfer-encoding"];

fn a_header_the_library_writes_is_refused_where_it_is_written_in_any_case_of_its_letters() {
    let names = |g: &mut Gen| {
        let name = *g.pick(RESERVED);
        cased(g, name)
    };
    generate::check(9101, 12, names, |name| {
        let source = program("header_name", &format!("x = header_name(\"{name}\")"));
        let said = messages(&source);
        assert!(
            said.iter().any(|m| m.contains("not_written_by_the_library") && m.contains(name.as_str())),
            "{said:?} for\n{source}"
        );
    });
}

fn a_name_that_is_no_token_and_a_value_with_a_line_break_are_refused_where_they_are_written() {
    let bad = |g: &mut Gen| {
        let at = g.below(3);
        let mark = *g.pick(&[" ", ":", "(", "\\\"", "/"]);
        format!("a{mark}b{at}")
    };
    generate::check(9102, 8, bad, |name| {
        let source = program("header_name", &format!("x = header_name(\"{name}\")"));
        let said = messages(&source);
        assert!(said.iter().any(|m| m.contains("a_token")), "{said:?} for\n{source}");
    });
    let source = program("header_value", "x = header_value(\"a\\nb\")");
    let said = messages(&source);
    assert!(said.iter().any(|m| m.contains("one_line")), "{said:?} for\n{source}");
    let source = program("header_name", "x = header_name(\"\")");
    assert!(messages(&source).iter().any(|m| m.contains("a_token")));
}

fn a_status_outside_the_range_of_statuses_is_refused_where_it_is_written() {
    let statuses = |g: &mut Gen| if g.below(2) == 0 { g.below(100) } else { 600 + g.below(400) };
    generate::check(9103, 10, statuses, |status| {
        let source = program("Response", &format!("r = Response(status: {status}, headers: [], body: \"\")"));
        let said = messages(&source);
        assert!(said.iter().any(|m| m.contains("100..600")), "{said:?} for\n{source}");
    });
    let inside = |g: &mut Gen| 100 + g.below(500);
    generate::check(9104, 10, inside, |status| {
        let source = program("Response", &format!("r = Response(status: {status}, headers: [], body: \"\")"));
        assert!(messages(&source).is_empty(), "{:?} for\n{source}", messages(&source));
    });
}

fn a_method_that_is_a_text_is_refused_for_a_request() {
    let source = program(
        "Request",
        "r = Request(method: \"GET\", path: \"/\", query: \"\", headers: [], body: \"\")",
    );
    let said = messages(&source);
    assert!(said.iter().any(|m| m.contains("Method")), "{said:?} for\n{source}");
    let source = "use std.fetch (Method)\nuse std.http (Request)\n\nfn main() <alloc> {\n  r = Request(method: .get, path: \"/\", query: \"\", headers: [], body: \"\")\n}\n";
    assert!(messages(source).is_empty(), "{:?}", messages(source));
}

fn a_name_and_a_value_known_when_the_program_runs_are_made_by_check_or_say_why_not() {
    let names = |g: &mut Gen| {
        let name = *g.pick(&["x-id", "accept", "content-type", "x-a.b"]);
        cased(g, name)
    };
    generate::check(9105, 8, names, |name| {
        let source = format!(
            "use std.console (console, print)\nuse std.http (check_name)\n\nfn main() <console, alloc> {{\n  match check_name(\"{name}\") {{\n    .ok(n) {{ print(\"{{n}}\") }}\n    .err(e) {{ print(\"no\") }}\n  }}\n}}\n"
        );
        let said = String::from_utf8(outcome(&source).stdout).unwrap();
        assert_eq!(said, format!("{}\n", name.to_ascii_lowercase()), "for\n{source}");
    });
    let refused = |g: &mut Gen| {
        let reserved = *g.pick(RESERVED);
        cased(g, reserved)
    };
    generate::check(9106, 6, refused, |name| {
        let source = format!(
            "use std.console (console, print)\nuse std.http (check_name, check_value)\n\nfn main() <console, alloc> {{\n  match check_name(\"{name}\") {{\n    .ok(n) {{ print(\"made\") }}\n    .err(e) {{ print(\"{{e}}\") }}\n  }}\n  match check_value(\"a\\nb\") {{\n    .ok(v) {{ print(\"made\") }}\n    .err(e) {{ print(\"line break\") }}\n  }}\n}}\n"
        );
        let said = String::from_utf8(outcome(&source).stdout).unwrap();
        assert!(said.starts_with(&format!("{name}: ")), "{said} for\n{source}");
        assert!(said.ends_with("line break\n"), "{said}");
    });
}
