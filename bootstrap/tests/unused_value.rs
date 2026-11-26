//! A value that nothing uses is refused, and `_ = e` throws one away on
//! purpose. Where a statement is not the last of its block, it compiles exactly
//! when its value is `()` or it is `_ = e`, and the same holds for the end of
//! a block that is not read: the block of an `if` with no `else`, of a `match`
//! written as a statement, and of a `loop`.
//!
//! The generator picks what a function gives and where its call is written,
//! and the expectation comes from that choice alone.

use smith::check;
use smith::diag::Error;
use smith::{lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;

pub const ONCE: &[Test] = &[
    (
        "a_statement_compiles_exactly_when_its_value_is_nothing_or_it_is_thrown_away",
        a_statement_compiles_exactly_when_its_value_is_nothing_or_it_is_thrown_away,
    ),
    (
        "a_result_that_nothing_uses_says_that_a_failure_would_be_dropped",
        a_result_that_nothing_uses_says_that_a_failure_would_be_dropped,
    ),
    (
        "throwing_a_value_away_prints_what_binding_it_prints",
        throwing_a_value_away_prints_what_binding_it_prints,
    ),
    (
        "an_underscore_defines_no_name_and_any_number_may_stand_in_a_block",
        an_underscore_defines_no_name_and_any_number_may_stand_in_a_block,
    ),
    (
        "throwing_away_a_value_without_discard_is_refused_as_any_drop_is",
        throwing_away_a_value_without_discard_is_refused_as_any_drop_is,
    ),
];

/// What a function may give, as its type and a value of it. `()` has none.
const GIVES: &[(&str, &str)] = &[
    ("str", "\"hey\""),
    ("u64", "7"),
    ("bool", "true"),
    ("Result[str, str]", ".ok(\"hey\")"),
    ("()", ""),
];

/// Where the statement under test is written.
const PLACES: &[&str] = &["not_last", "if", "match", "loop"];

/// How the statement under test is written.
const FORMS: &[&str] = &["alone", "thrown_away", "bound"];

fn errors(source: &str) -> Vec<Error> {
    check(source.as_bytes()).err().unwrap_or_default()
}

/// A program whose `main` holds `statement` in `place`, calling `make`, which
/// gives `ty`.
fn program(ty: &str, value: &str, place: &str, form: &str) -> String {
    let make = if ty == "()" {
        "fn make() { }\n".to_string()
    } else {
        format!("fn make() -> {ty} {{ {value} }}\n")
    };
    let statement = match form {
        "alone" => "make()",
        "thrown_away" => "_ = make()",
        _ => "kept = make()",
    };
    let body = match place {
        "not_last" => format!("  {statement}\n  print(\"done\")\n"),
        "if" => format!("  if true {{ {statement} }}\n  print(\"done\")\n"),
        "match" => format!(
            "  match Light.red {{\n    .red {{ {statement} }}\n    .green {{ }}\n  }}\n  print(\"done\")\n"
        ),
        _ => format!("  loop i in 0..1 {{ {statement} }}\n  print(\"done\")\n"),
    };
    with_header(format!(
        "enum Light {{\n  red\n  green\n}}\n\n{make}\nfn main() <console> {{\n{body}}}\n"
    ))
}

/// A statement is refused for its value exactly when it is written alone and
/// its value is not `()`. A block that ends in it is the same, and so is one
/// that is bound, which gives `()`.
fn a_statement_compiles_exactly_when_its_value_is_nothing_or_it_is_thrown_away() {
    generate::check(
        8101,
        60,
        |g: &mut Gen| (g.below(GIVES.len()), g.below(PLACES.len()), g.below(FORMS.len())),
        |(gives, place, form)| {
            let (ty, value) = GIVES[*gives];
            let source = program(ty, value, PLACES[*place], FORMS[*form]);
            let found = errors(&source);
            let refused = FORMS[*form] == "alone" && ty != "()";
            assert_eq!(!found.is_empty(), refused, "{found:?}\n{source}");
            if refused {
                assert_eq!(found.len(), 1, "{found:?}\n{source}");
                assert!(found[0].message.contains("which nothing uses"), "{found:?}");
                assert!(found[0].message.contains(&format!("`{ty}`")), "{found:?}");
                let fix = found[0].fix.as_deref().expect("a fix");
                assert!(fix.contains("`_ =`"), "{fix}");
            }
        },
    );
}

fn a_result_that_nothing_uses_says_that_a_failure_would_be_dropped() {
    for place in PLACES {
        let source = program("Result[str, str]", ".ok(\"hey\")", place, "alone");
        let found = errors(&source);
        assert_eq!(found.len(), 1, "{found:?}\n{source}");
        assert!(found[0].message.contains("a failure would be dropped"), "{found:?}");
        let fix = found[0].fix.as_deref().expect("a fix");
        assert!(fix.contains("`try`") && fix.contains("`_ =`"), "{fix}");
    }
}

fn run(source: &str) -> scalar::Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    scalar::run(&program)
}

/// `shout()` alone is refused, and `_ = shout()` prints what `x = shout()`
/// does, which is what the program printed before the value was refused.
fn throwing_a_value_away_prints_what_binding_it_prints() {
    for (ty, value) in GIVES.iter().filter(|(t, _)| *t != "()") {
        let thrown = program(ty, value, "not_last", "thrown_away");
        let bound = program(ty, value, "not_last", "bound");
        assert_eq!(errors(&thrown), Vec::new(), "{thrown}");
        assert_eq!(run(&thrown), run(&bound), "{thrown}");
        assert_eq!(run(&thrown).stdout, b"done\n".to_vec(), "{thrown}");
    }
}

fn an_underscore_defines_no_name_and_any_number_may_stand_in_a_block() {
    generate::check(
        8102,
        12,
        |g: &mut Gen| 1 + g.below(5),
        |count| {
            let lines = "  _ = make()\n".repeat(*count);
            let source = with_header(format!(
                "fn make() -> str {{ \"hey\" }}\n\nfn main() <console> {{\n{lines}  print(\"done\")\n}}\n"
            ));
            assert_eq!(errors(&source), Vec::new(), "{source}");
            assert_eq!(run(&source).stdout, b"done\n".to_vec(), "{source}");
            // The name `_` cannot be read.
            let read = with_header(format!(
                "fn make() -> str {{ \"hey\" }}\n\nfn main() <console> {{\n{lines}  print(_)\n}}\n"
            ));
            assert!(!errors(&read).is_empty(), "{read}");
        },
    );
}

fn throwing_away_a_value_without_discard_is_refused_as_any_drop_is() {
    let source = with_header(
        "struct Ticket {\n  id: u64\n}\n\nwithout Discard for Ticket\n\nfn open() -> Ticket { Ticket(id: 1) }\n\nfn main() <console> {\n  _ = open()\n  print(\"done\")\n}\n"
            .to_string(),
    );
    let found = errors(&source);
    assert_eq!(found.len(), 1, "{found:?}\n{source}");
    assert!(found[0].message.contains("dropped"), "{found:?}");
}
