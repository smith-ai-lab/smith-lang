//! A program handles `console`: a handler given for `print` receives the
//! text, its `end`, and its `to`, as written and as defaulted, and with no
//! handler the target answers, writing the text and `end` to the stream.

use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "a_handler_of_console_receives_the_text_the_end_and_the_stream_as_written_and_as_defaulted",
        a_handler_of_console_receives_the_text_the_end_and_the_stream_as_written_and_as_defaulted,
    ),
    (
        "with_no_handler_print_writes_the_text_and_the_end_to_the_stream_it_is_given",
        with_no_handler_print_writes_the_text_and_the_end_to_the_stream_it_is_given,
    ),
    (
        "a_handler_that_sends_everything_to_standard_error_leaves_standard_output_empty",
        a_handler_that_sends_everything_to_standard_error_leaves_standard_output_empty,
    ),
];

/// The executables of the programs are checked against the evaluator.
pub const MACHINE: &[Test] = &[(
    "every_executable_answers_print_as_the_evaluator_does_with_and_without_a_handler",
    every_executable_answers_print_as_the_evaluator_does_with_and_without_a_handler,
)];

fn run(source: &str) -> scalar::Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// One call of `print`: its text, and the `end` and the `to` it writes, each
/// written or left out.
#[derive(Clone, Debug)]
struct Print {
    text: &'static str,
    end: Option<&'static str>,
    to: Option<bool>,
}

const WORDS: &[&str] = &["a", "hello world", "x1", "", "caf\u{e9}"];
const ENDS: &[&str] = &["", "!", "\\n", " "];

fn prints(g: &mut Gen) -> Vec<Print> {
    (0..1 + g.below(5))
        .map(|_| Print {
            text: g.pick(WORDS),
            end: g.one_in(2).then(|| *g.pick(ENDS)),
            to: g.one_in(2).then(|| g.one_in(2)),
        })
        .collect()
}

/// The call as written.
fn written(p: &Print) -> String {
    let mut s = format!("print(\"{}\"", p.text);
    if let Some(end) = p.end {
        s += &format!(", end: \"{end}\"");
    }
    if let Some(to) = p.to {
        s += if to { ", to: .stderr" } else { ", to: .stdout" };
    }
    s + ")"
}

/// The bytes of the end, with the escape written in the source meaning a line
/// break.
fn end_of(p: &Print) -> String {
    match p.end {
        None => "\n".into(),
        Some(e) => e.replace("\\n", "\n"),
    }
}

fn is_stderr(p: &Print) -> bool {
    p.to == Some(true)
}

const USES: &str = "use std.console (console, input, print)\nuse std.list (push)\n\n";

/// A function that writes the calls, which carries `console`.
fn body(all: &[Print]) -> String {
    let mut s = String::from("fn writes() <console> {\n");
    for p in all {
        s += &format!("  {}\n", written(p));
    }
    s + "}\n\n"
}

fn a_handler_of_console_receives_the_text_the_end_and_the_stream_as_written_and_as_defaulted() {
    generate::check(8901, 6, prints, |all| {
        let source = format!(
            "{USES}{}fn capture[T](body: fn() -> T <console>) -> (T, List[str]) <console, alloc> {{\n  mut seen: List[str] = []\n  r = console(\n    print: fn(text, end, to) {{\n      which = match to {{\n        .stdout {{ \"o\" }}\n        .stderr {{ \"e\" }}\n      }}\n      seen = push(seen, \"{{which}}|{{text}}|{{end}}\")\n    }},\n    input: fn(prompt) {{ input(prompt) }},\n    body: body,\n  )\n  (r, seen)\n}}\n\nfn main() <console, alloc> {{\n  (u, lines) = capture(writes)\n  loop i in 0..lines.size {{\n    print(lines[i], end: \"#\")\n  }}\n}}\n",
            body(all)
        );
        let mut expected = String::new();
        for p in all {
            let which = if is_stderr(p) { "e" } else { "o" };
            expected += &format!("{which}|{}|{}#", p.text, end_of(p));
        }
        let said = String::from_utf8(run(&source).stdout).unwrap();
        assert_eq!(said, expected, "for\n{source}");
    });
}

fn with_no_handler_print_writes_the_text_and_the_end_to_the_stream_it_is_given() {
    generate::check(8902, 6, prints, |all| {
        let source = format!(
            "{USES}{}fn main() <console> {{\n  writes()\n}}\n",
            body(all)
        );
        let mut expected = String::new();
        for p in all {
            if !is_stderr(p) {
                expected += &format!("{}{}", p.text, end_of(p));
            }
        }
        let said = String::from_utf8(run(&source).stdout).unwrap();
        assert_eq!(said, expected, "for\n{source}");
    });
}

fn a_handler_that_sends_everything_to_standard_error_leaves_standard_output_empty() {
    generate::check(8903, 4, prints, |all| {
        let source = format!(
            "{USES}{}fn to_stderr[T](body: fn() -> T <console>) -> T <console, alloc> {{\n  console(\n    print: fn(text, end, to) {{ print(text, end: end, to: .stderr) }},\n    input: fn(prompt) {{ input(prompt) }},\n    body: body,\n  )\n}}\n\nfn main() <console, alloc> {{\n  to_stderr(writes)\n  print(\"done\")\n}}\n",
            body(all)
        );
        let said = String::from_utf8(run(&source).stdout).unwrap();
        assert_eq!(said, "done\n", "for\n{source}");
    });
}

fn every_executable_answers_print_as_the_evaluator_does_with_and_without_a_handler() {
    let all = vec![
        Print { text: "a", end: None, to: None },
        Print { text: "b", end: Some("!"), to: Some(false) },
        Print { text: "c", end: Some(""), to: Some(true) },
        Print { text: "d", end: Some("\\n"), to: None },
    ];
    let plain = format!("{USES}{}fn main() <console> {{\n  writes()\n}}\n", body(&all));
    let handled = format!(
        "{USES}{}fn to_stderr[T](body: fn() -> T <console>) -> T <console, alloc> {{\n  console(\n    print: fn(text, end, to) {{ print(text, end: end, to: .stdout) }},\n    input: fn(prompt) {{ input(prompt) }},\n    body: body,\n  )\n}}\n\nfn main() <console, alloc> {{\n  to_stderr(writes)\n}}\n",
        body(&all)
    );
    // What goes to standard error is not seen, unless the handler sends it to
    // standard output.
    for (source, expected) in [(plain, "a\nb!d\n"), (handled, "a\nb!cd\n")] {
        let said = run(&source);
        assert_eq!(String::from_utf8(said.stdout.clone()).unwrap(), expected);
        for target in targets::runnable() {
            targets::runs_as(source.as_bytes(), target, &said);
        }
    }
}
