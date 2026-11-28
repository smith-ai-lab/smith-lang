//! A program reads a line of standard input with `input` of `console`: the
//! prompt is written first, the line comes back without its line break, a line
//! feed or a carriage return and a line feed ends it, a line of any length is
//! whole, and `.none` says that standard input has ended.

use smith::scalar::{self, Op, System};
use smith::{lower, target::Target};

use crate::Test;
use crate::aarch64;
use crate::generate::{self, Gen};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "greet_prints_the_name_it_reads_and_prints_hello_alone_at_the_end_of_input",
        greet_prints_the_name_it_reads_and_prints_hello_alone_at_the_end_of_input,
    ),
    (
        "a_body_under_a_handler_that_gives_input_is_answered_by_the_console_outside_it",
        a_body_under_a_handler_that_gives_input_is_answered_by_the_console_outside_it,
    ),
    (
        "every_list_of_lines_is_read_back_whole_on_the_evaluator",
        every_list_of_lines_is_read_back_whole_on_the_evaluator,
    ),
    (
        "a_line_longer_than_any_buffer_is_read_whole_on_the_evaluator",
        a_line_longer_than_any_buffer_is_read_whole_on_the_evaluator,
    ),
    (
        "input_writes_the_prompt_before_it_reads",
        input_writes_the_prompt_before_it_reads,
    ),
    (
        "an_arm64_executable_reads_lines_as_the_evaluator_does",
        an_arm64_executable_reads_lines_as_the_evaluator_does,
    ),
    (
        "a_module_that_reads_standard_input_is_refused_with_one_message",
        a_module_that_reads_standard_input_is_refused_with_one_message,
    ),
];

/// Every executable reads what the evaluator reads, from the same input.
pub const MACHINE: &[Test] = &[(
    "every_executable_reads_lines_as_the_evaluator_does",
    every_executable_reads_lines_as_the_evaluator_does,
)];

const GREET: &str = "use std.console (console, input, print)\n\nfn greet() <console, alloc> {\n  match input(\"name: \") {\n    .some(name) { print(\"hello {name}\") }\n    .none { print(\"hello\") }\n  }\n}\n\nfn main() <console, alloc> {\n  greet()\n}\n";

/// Prints each line it reads, and the mark `|` and a line break after it,
/// until there are no more.
const EVERY_LINE: &str = "use std.console (console, input, print)\n\nfn main() <console, alloc, diverge> {\n  loop {\n    match input() {\n      .some(line) { print(line, end: \"|\\n\") }\n      .none { break }\n    }\n  }\n}\n";

const CAPTURE: &str = "use std.console (console, input, print)\nuse std.list (push)\n\nfn capture[T](body: fn() -> T <console>) -> (T, List[str]) <console, alloc> {\n  mut seen: List[str] = []\n  r = console(\n    print: fn(text, end, to) {\n      match to {\n        .stdout { push(mut seen, \"{text}{end}\") }\n        .stderr { print(text, end: end, to: .stderr) }\n      }\n    },\n    input: fn(prompt) { input(prompt) },\n    body: body,\n  )\n  (r, seen)\n}\n\nfn greet() <console, alloc> {\n  match input(\"name: \") {\n    .some(name) { print(\"hello {name}\") }\n    .none { print(\"hello\") }\n  }\n}\n\nfn main() <console, alloc> {\n  (u, lines) = capture(greet)\n  loop i in 0..lines.size {\n    print(lines[i], end: \"#\")\n  }\n}\n";

fn run(source: &str, input: &[u8]) -> scalar::Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run_with_input(&program, input)
}

fn greet_prints_the_name_it_reads_and_prints_hello_alone_at_the_end_of_input() {
    let said = run(GREET, b"ada\n");
    assert_eq!(said.stdout, b"name: hello ada\n");
    let said = run(GREET, b"");
    assert_eq!(said.stdout, b"name: hello\n");
}

fn a_body_under_a_handler_that_gives_input_is_answered_by_the_console_outside_it() {
    // The prompt is written by the console outside the handler, which is the
    // one that reads, and the line the body prints is kept by the handler.
    let said = run(CAPTURE, b"ada\n");
    assert_eq!(said.stdout, b"name: hello ada\n#");
    let said = run(CAPTURE, b"");
    assert_eq!(said.stdout, b"name: hello\n#");
}

/// A list of lines, how each ends, and whether the last has no end.
#[derive(Clone, Debug)]
struct Lines {
    lines: Vec<String>,
    crlf: Vec<bool>,
    unended: bool,
}

const LETTERS: &[char] = &['a', 'b', ' ', '\u{e9}', '{', '"', '\\', '\t'];
/// Lengths at and beside the sizes a buffer may start with or double to.
const LENGTHS: &[usize] = &[0, 1, 2, 3, 63, 64, 65, 127, 128, 129, 200];

fn lines(g: &mut Gen) -> Lines {
    let count = g.below(5);
    let mut all = Vec::new();
    let mut crlf = Vec::new();
    for _ in 0..count {
        let length = if g.one_in(2) {
            *g.pick(LENGTHS)
        } else {
            g.below(6)
        };
        let mut line: Vec<char> = (0..length).map(|_| *g.pick(LETTERS)).collect();
        // A carriage return that no line feed follows is part of the line.
        if line.len() >= 2 && g.one_in(4) {
            let at = g.below(line.len() - 1);
            line[at] = '\r';
        }
        all.push(line.into_iter().collect());
        crlf.push(g.one_in(2));
    }
    // A last line with no break after it cannot be empty, as that is no line.
    let unended = all.last().is_some_and(|l: &String| !l.is_empty()) && g.one_in(2);
    Lines {
        lines: all,
        crlf,
        unended,
    }
}

/// What standard input holds for the lines, and what `EVERY_LINE` prints.
fn given(l: &Lines) -> (Vec<u8>, Vec<u8>) {
    let (mut input, mut expected) = (Vec::new(), Vec::new());
    for (i, line) in l.lines.iter().enumerate() {
        input.extend_from_slice(line.as_bytes());
        if !(l.unended && i + 1 == l.lines.len()) {
            input.extend_from_slice(if l.crlf[i] { b"\r\n" } else { b"\n" });
        }
        expected.extend_from_slice(line.as_bytes());
        expected.extend_from_slice(b"|\n");
    }
    (input, expected)
}

fn every_list_of_lines_is_read_back_whole_on_the_evaluator() {
    let program = lower(EVERY_LINE.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    generate::check(9101, 40, lines, |l| {
        let (input, expected) = given(l);
        let said = scalar::run_with_input(&program, &input);
        assert_eq!(said.stdout, expected, "for the input {input:?}");
    });
}

fn a_line_longer_than_any_buffer_is_read_whole_on_the_evaluator() {
    let long = "x".repeat(20_000);
    let input = format!("{long}\nshort");
    let said = run(EVERY_LINE, input.as_bytes());
    assert_eq!(said.stdout, format!("{long}|\nshort|\n").as_bytes());
}

/// `input` writes its prompt before it reads: every function that reads has a
/// first block that writes, and no read is in it, so the write comes first
/// whichever way the rest goes.
fn input_writes_the_prompt_before_it_reads() {
    let program = lower(GREET.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let reads = |op: &Op| {
        matches!(
            op,
            Op::System {
                call: System::Read,
                ..
            }
        )
    };
    let writes = |op: &Op| {
        matches!(
            op,
            Op::System {
                call: System::Write(scalar::Stream::Stdout),
                ..
            }
        )
    };
    let mut found = false;
    for function in &program.functions {
        let in_block = |k: usize, test: &dyn Fn(&Op) -> bool| {
            function.blocks[k].instructions.iter().any(|i| test(&i.op))
        };
        if (0..function.blocks.len()).any(|k| in_block(k, &reads)) {
            found = true;
            assert!(in_block(0, &writes), "the first block writes the prompt");
            assert!(!in_block(0, &reads), "no read is in the first block");
        }
    }
    assert!(found, "the program reads");
}

fn a_module_that_reads_standard_input_is_refused_with_one_message() {
    let program = lower(GREET.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let module = *Target::ALL
        .iter()
        .find(|t| !t.is_machine_code())
        .expect("a target that is a module");
    let said = std::panic::catch_unwind(|| module.emit(&program))
        .err()
        .map(|p| match p.downcast::<String>() {
            Ok(message) => *message,
            Err(p) => p.downcast_ref::<&str>().map(|m| m.to_string()).unwrap_or_default(),
        })
        .expect("the module is refused");
    assert!(said.contains("input"), "{said}");
}

/// The executable for arm64 on Linux is run by the interpreter of the tests,
/// which gives it the same input, and it prints what the evaluator prints.
fn an_arm64_executable_reads_lines_as_the_evaluator_does() {
    let long = "w".repeat(700);
    let mut inputs: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"ada".to_vec(),
        b"a\r\n\r\nb\n".to_vec(),
        format!("{long}\nz").into_bytes(),
    ];
    inputs.push(given(&generate::sample(9103, 0, lines)).0);
    // The targets whose executable the interpreter runs, which are the
    // arm64 ones of Linux.
    let executables = |source: &str| -> Vec<Vec<u8>> {
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        Target::ALL
            .iter()
            .filter(|t| t.is_machine_code())
            .map(|t| t.emit(&program))
            .filter(|bytes| aarch64::is_executable(bytes))
            .collect()
    };
    for source in [EVERY_LINE, GREET] {
        let all = executables(source);
        assert!(!all.is_empty(), "no target is built for arm64");
        for bytes in &all {
            for input in &inputs {
                let said = run(source, input);
                let ran = aarch64::run_with_input(bytes, input);
                assert_eq!((ran.stdout, ran.status), (said.stdout, said.status));
            }
        }
    }
}

/// Reads the first line as `GREET` does, and then every line after it as
/// `EVERY_LINE` does, so that one run of an executable covers both.
const GREET_THEN_EVERY_LINE: &str = "use std.console (console, input, print)\n\nfn main() <console, alloc, diverge> {\n  match input(\"name: \") {\n    .some(name) { print(\"hello {name}\") }\n    .none { print(\"hello\") }\n  }\n  loop {\n    match input() {\n      .some(line) { print(line, end: \"|\\n\") }\n      .none { break }\n    }\n  }\n}\n";

/// What is run on every target this machine runs. A run of an executable
/// costs a process, which is most of what this test spends on a machine where
/// that is slow, so there are few runs, each of which covers several things:
/// no input, a name alone, a first line that ends the way a name ends and then
/// lines that end with `\n` and with `\r\n`, empty lines, a line longer than
/// any buffer, and a last line with no end; a lone carriage return at the end;
/// and a generated list of lines.
fn every_executable_reads_lines_as_the_evaluator_does() {
    let long = "y".repeat(5000);
    let mut inputs: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"ada\n".to_vec(),
        format!("first\r\n\r\nb\r\n\n{long}\nlast").into_bytes(),
        b"tail\r".to_vec(),
    ];
    inputs.push(given(&generate::sample(9102, 0, lines)).0);
    // The program is lowered once and run on the evaluator once for each
    // input, and built once for each target.
    let source = GREET_THEN_EVERY_LINE;
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let cases: Vec<(&[u8], scalar::Outcome)> = inputs
        .iter()
        .map(|input| (&input[..], scalar::run_with_input(&program, input)))
        .collect();
    for target in targets::runnable()
        .into_iter()
        .filter(|t| t.is_machine_code())
    {
        targets::runs_as_each_given(source.as_bytes(), target, &cases);
    }
}
