//! A function that changes a parameter marked `mut` can also give a result,
//! as `knowledge/language/mut-parameters.md` says. It means taking the values
//! and giving them back with the result, so `r = f(mut v)` gives what
//! `(v, r) = g(v)` gives, and is built to the same code, where `g` takes `v`
//! and gives it back before what the body gives.
//!
//! A call that passes `mut` stands on a line of its own or as the whole
//! right-hand side of a binding or an assignment, and on a line of its own
//! what it gives is refused as any value that nothing uses is.

use smith::check;
use smith::diag::Error;
use smith::target::Target;
use smith::{compile, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::pos_after;

pub const ONCE: &[Test] = &[
    (
        "a_function_that_changes_a_cursor_gives_a_result_as_the_issue_says",
        a_function_that_changes_a_cursor_gives_a_result_as_the_issue_says,
    ),
    (
        "changing_places_with_mut_and_keeping_the_result_gives_what_taking_them_and_binding_what_comes_back_gives_and_the_same_code",
        changing_places_with_mut_and_keeping_the_result_gives_what_taking_them_and_binding_what_comes_back_gives_and_the_same_code,
    ),
    (
        "a_call_that_passes_mut_stands_on_a_line_of_its_own_or_as_the_whole_right_hand_side",
        a_call_that_passes_mut_stands_on_a_line_of_its_own_or_as_the_whole_right_hand_side,
    ),
    (
        "a_result_of_a_call_that_passes_mut_that_nothing_uses_is_refused_as_any_such_value_is",
        a_result_of_a_call_that_passes_mut_that_nothing_uses_is_refused_as_any_such_value_is,
    ),
];

/// The errors a program gives, which are none when it is accepted.
fn errors(source: &str) -> Vec<Error> {
    match check(source.as_bytes()) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

fn output_of(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    String::from_utf8(scalar::run(&program).stdout).expect("text")
}

const CURSOR: &str = "use std.console (console, print)
use std.bits (u8)

struct Cursor {
  at: u8
  line: u8
}

struct Doc {
  cursor: Cursor
  last: u8
}
";

fn a_function_that_changes_a_cursor_gives_a_result_as_the_issue_says() {
    let source = "use std.console (console, print)
use std.bits (u8)

struct Cursor {
  at: u8
}

fn next(c: mut Cursor) -> u8 {
  was = c.at
  if c.at < 9 { c.at = c.at + 1 }
  was
}

fn main() <console, alloc> {
  mut c = Cursor(at: 0)
  _ = next(mut c)
  t = next(mut c)
  print(\"{c.at} {t}\")
}
";
    assert_eq!(output_of(source), "2 1\n", "for {source}");
}

/// A line of the body of a function that changes the cursors `x0`, `x1`, ...
#[derive(Clone, Copy, Debug)]
enum Step {
    /// Adds a number below 50 to `at` of a cursor when it is below 200.
    AddAt(usize, u8),
    /// `line` of a cursor becomes `at` of another.
    Copy(usize, usize),
    /// A cursor is replaced by a new one.
    Replace(usize, u8, u8),
    /// The function returns `line` of the first cursor when its `at` is the
    /// number.
    ReturnAt(u8),
}

/// What the body ends with, which the function gives.
#[derive(Clone, Copy, Debug)]
enum Tail {
    At(usize),
    Line(usize),
    Number(u8),
}

impl Step {
    fn text(self) -> String {
        match self {
            Step::AddAt(i, k) => format!("if x{i}.at < 200 {{ x{i}.at = x{i}.at + {k} }}"),
            Step::Copy(i, j) => format!("x{i}.line = x{j}.at"),
            Step::Replace(i, a, l) => format!("x{i} = Cursor(at: {a}, line: {l})"),
            Step::ReturnAt(k) => format!("if x0.at == {k} {{ return x0.line }}"),
        }
    }
}

impl Tail {
    fn text(self) -> String {
        match self {
            Tail::At(i) => format!("x{i}.at"),
            Tail::Line(i) => format!("x{i}.line"),
            Tail::Number(k) => k.to_string(),
        }
    }
}

#[derive(Debug)]
struct Change {
    /// The cursors the function changes, by `at` and `line`.
    starts: Vec<(u8, u8)>,
    lines: Vec<Step>,
    tail: Tail,
    calls: usize,
    /// Whether the result is bound with `mut`.
    var: bool,
}

fn change(g: &mut Gen) -> Change {
    let n = 1 + g.below(2);
    let starts = (0..n)
        .map(|_| (g.below(10) as u8, g.below(10) as u8))
        .collect();
    let lines = (0..1 + g.below(5))
        .map(|_| match g.below(4) {
            0 => Step::AddAt(g.below(n), g.below(50) as u8),
            1 => Step::Copy(g.below(n), g.below(n)),
            2 => Step::Replace(g.below(n), g.below(10) as u8, g.below(10) as u8),
            _ => Step::ReturnAt(g.below(10) as u8),
        })
        .collect();
    let tail = match g.below(3) {
        0 => Tail::At(g.below(n)),
        1 => Tail::Line(g.below(n)),
        _ => Tail::Number(g.below(10) as u8),
    };
    Change {
        starts,
        lines,
        tail,
        calls: 1 + g.below(3),
        var: g.one_in(2),
    }
}

/// The cursors after the lines run, and what the function gives.
fn run(c: &Change, cursors: &mut [(u8, u8)]) -> u8 {
    for l in &c.lines {
        match *l {
            Step::AddAt(i, k) if cursors[i].0 < 200 => cursors[i].0 += k,
            Step::AddAt(..) => {}
            Step::Copy(i, j) => cursors[i].1 = cursors[j].0,
            Step::Replace(i, a, l) => cursors[i] = (a, l),
            Step::ReturnAt(k) if cursors[0].0 == k => return cursors[0].1,
            Step::ReturnAt(_) => {}
        }
    }
    match c.tail {
        Tail::At(i) => cursors[i].0,
        Tail::Line(i) => cursors[i].1,
        Tail::Number(k) => k,
    }
}

/// The program that changes the cursors `calls` times and keeps what each
/// call gives, with `mut` when `mutable` is true, and by taking them and
/// binding what comes back otherwise.
fn change_program(c: &Change, mutable: bool) -> String {
    let n = c.starts.len();
    let body: Vec<String> = c.lines.iter().map(|l| format!("  {}", l.text())).collect();
    let body = body.join("\n");
    let tail = c.tail.text();
    let cursors = |each: &dyn Fn(usize) -> String| (0..n).map(each).collect::<Vec<_>>();
    let function = if mutable {
        let params = cursors(&|i| format!("x{i}: mut Cursor"));
        format!("fn change({}) -> u8 {{\n{body}\n  {tail}\n}}", params.join(", "))
    } else {
        let params = cursors(&|i| format!("given{i}: take Cursor"));
        let types = vec!["Cursor"; n].join(", ");
        let rebind: String = cursors(&|i| format!("  mut x{i} = given{i}\n")).concat();
        let back = cursors(&|i| format!("x{i}")).join(", ");
        let body = body.replace("{ return x0.line }", &format!("{{ return ({back}, x0.line) }}"));
        format!(
            "fn change({}) -> ({types}, u8) {{\n{rebind}{body}\n  ({back}, {tail})\n}}",
            params.join(", ")
        )
    };
    let made: String = (c.starts.iter().enumerate())
        .map(|(i, (at, line))| format!("  mut c{i} = Cursor(at: {at}, line: {line})\n"))
        .collect();
    let var = if c.var { "mut " } else { "" };
    let calls: String = (0..c.calls)
        .map(|k| {
            if mutable {
                let args = cursors(&|i| format!("mut c{i}")).join(", ");
                format!("  {var}r{k} = change({args})\n")
            } else {
                let args = cursors(&|i| format!("c{i}")).join(", ");
                format!("  ({args}, {var}r{k}) = change({args})\n")
            }
        })
        .collect();
    let shown: Vec<String> = (cursors(&|i| format!("{{c{i}.at}} {{c{i}.line}}")).into_iter())
        .chain((0..c.calls).map(|k| format!("{{r{k}}}")))
        .collect();
    format!(
        "{CURSOR}\n{function}\n\nfn main() <console, alloc> {{\n{made}{calls}  print(\"{}\")\n}}\n",
        shown.join(" ")
    )
}

fn changing_places_with_mut_and_keeping_the_result_gives_what_taking_them_and_binding_what_comes_back_gives_and_the_same_code()
 {
    generate::check(12341, 6, change, |c| {
        let (mutable, taken) = (change_program(c, true), change_program(c, false));
        let mut cursors = c.starts.clone();
        let results: Vec<u8> = (0..c.calls).map(|_| run(c, &mut cursors)).collect();
        let shown: Vec<String> = (cursors.iter().map(|(a, l)| format!("{a} {l}")))
            .chain(results.iter().map(u8::to_string))
            .collect();
        let expected = format!("{}\n", shown.join(" "));
        assert_eq!(output_of(&mutable), expected, "for {mutable}");
        assert_eq!(output_of(&taken), expected, "for {taken}");
        for target in Target::ALL {
            let one = compile(mutable.as_bytes(), *target).unwrap_or_else(|e| panic!("{e:?}"));
            let other = compile(taken.as_bytes(), *target).unwrap_or_else(|e| panic!("{e:?}"));
            assert!(
                one == other,
                "the code for {target:?} differs between {mutable} and {taken}"
            );
        }
    });
}

/// A program that calls `next`, which changes a cursor and gives a `u8`, and
/// `step`, which changes it and gives nothing, on the line `line`.
fn standing_program(line: &str) -> String {
    format!(
        "{CURSOR}
fn next(c: mut Cursor) -> u8 {{
  was = c.at
  if c.at < 9 {{ c.at = c.at + 1 }}
  was
}}

fn step(c: mut Cursor) {{
  if c.at < 9 {{ c.at = c.at + 1 }}
}}

fn show(n: u8) {{
}}

fn main() <console, alloc> {{
  mut c = Cursor(at: 0, line: 0)
  mut d = Doc(cursor: Cursor(at: 5, line: 0), last: 0)
  {line}
  print(\"{{c.at}} {{d.cursor.at}} {{d.last}}\")
}}
"
    )
}

fn a_call_that_passes_mut_stands_on_a_line_of_its_own_or_as_the_whole_right_hand_side() {
    // Each line that is accepted, with what the program prints after it.
    for (line, printed) in [
        ("t = next(mut c)\n  show(t)", "1 5 0"),
        ("mut t = next(mut c)\n  t = next(mut c)\n  show(t)", "2 5 0"),
        ("d.last = next(mut c)", "1 5 0"),
        ("d.last = next(mut d.cursor)", "0 6 5"),
        ("t: u8 = next(mut d.cursor)\n  show(t)", "0 6 0"),
        ("_ = next(mut c)", "1 5 0"),
        ("step(mut c)", "1 5 0"),
    ] {
        let source = standing_program(line);
        assert_eq!(output_of(&source), format!("{printed}\n"), "for {source}");
    }
    // Each line that is refused, with what is written on it before the call
    // that is refused there.
    for (line, before) in [
        ("show(next(mut c))", "show("),
        ("n = 1 + next(mut c)\n  show(n)", "n = 1 + "),
        ("n = next(mut c) + 1\n  show(n)", "n = "),
        ("if next(mut c) == 1 { step(mut c) }", "if "),
        ("d = Doc(cursor: c, last: next(mut c))", "d = Doc(cursor: c, last: "),
        ("t = step(mut c)", "t = "),
        ("d.last = step(mut c)", "d.last = "),
    ] {
        let source = standing_program(line);
        let at = pos_after(&source[..source.find(line).expect("the line") + before.len()]);
        let errors = errors(&source);
        assert!(
            errors.iter().any(|e| e.pos == at),
            "nothing refused at {at:?} among {errors:?} for {source}"
        );
    }
}

fn a_result_of_a_call_that_passes_mut_that_nothing_uses_is_refused_as_any_such_value_is() {
    // For each type of result, the call that changes a cursor is refused as
    // the call that only reads it is.
    for (ty, value) in [("u8", "c.at"), ("str", "\"{c.at}\""), ("Cursor", "Cursor(at: c.at, line: 0)")] {
        let program = |call: &str| {
            format!(
                "{CURSOR}
fn changes(c: mut Cursor) -> {ty} <alloc> {{
  {value}
}}

fn reads(c: Cursor) -> {ty} <alloc> {{
  {value}
}}

fn main() <alloc> {{
  mut c = Cursor(at: 0, line: 0)
  {call}
  c.line = 1
}}
"
            )
        };
        let (changes, reads) = (program("changes(mut c)"), program("reads(c)"));
        let said = |e: &Error| (e.pos, e.message.clone(), e.fix.clone());
        let one: Vec<_> = errors(&changes).iter().map(said).collect();
        let other: Vec<_> = errors(&reads).iter().map(said).collect();
        assert!(!one.is_empty(), "nothing refused in {changes}");
        assert_eq!(one, other, "for {changes} and {reads}");
        let thrown = program("_ = changes(mut c)");
        assert!(errors(&thrown).is_empty(), "{:?} for {thrown}", errors(&thrown));
    }
}
