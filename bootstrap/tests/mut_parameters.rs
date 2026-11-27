//! A parameter marked `mut` is a value the function changes for its caller,
//! and a call that changes one marks it `mut`, as
//! `knowledge/language/mut-parameters.md` says. It means taking the value and giving it
//! back, so a program that changes a value with `mut` gives what the program
//! that takes it and assigns what comes back gives, and is built to the same
//! code.
//!
//! The generator builds the body of a function that changes a `Cursor`: it
//! adds to its fields, copies one to the other, replaces it, and leaves early
//! by `return`. It works out what the cursor is after each call, and writes the
//! function twice, with `mut` and with `take`, and the call twice, with
//! `mut` and as an assignment, of a `var`, a field of one, or an item of a
//! list.

use smith::check;
use smith::diag::Error;
use smith::target::Target;
use smith::{compile, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::pos_after;

pub const ONCE: &[Test] = &[
    (
        "a_function_changes_a_var_through_an_mut_parameter_as_the_page_says",
        a_function_changes_a_var_through_an_mut_parameter_as_the_page_says,
    ),
    (
        "changing_a_place_with_mut_gives_what_taking_it_and_assigning_what_comes_back_gives_and_the_same_code",
        changing_a_place_with_mut_gives_what_taking_it_and_assigning_what_comes_back_gives_and_the_same_code,
    ),
    (
        "a_mistake_in_a_function_that_changes_a_parameter_is_reported_as_in_the_function_that_takes_it_with_the_signature_as_written",
        a_mistake_in_a_function_that_changes_a_parameter_is_reported_as_in_the_function_that_takes_it_with_the_signature_as_written,
    ),
    (
        "a_call_marks_mut_exactly_the_arguments_whose_parameters_are_mut",
        a_call_marks_mut_exactly_the_arguments_whose_parameters_are_mut,
    ),
    (
        "what_is_passed_mut_is_a_var_or_a_field_or_an_item_of_one",
        what_is_passed_mut_is_a_var_or_a_field_or_an_item_of_one,
    ),
    (
        "one_place_is_passed_mut_once_and_places_that_do_not_overlap_are_passed_together",
        one_place_is_passed_mut_once_and_places_that_do_not_overlap_are_passed_together,
    ),
    (
        "a_function_does_not_store_return_or_give_a_lambda_a_parameter_marked_mut",
        a_function_does_not_store_return_or_give_a_lambda_a_parameter_marked_mut,
    ),
    (
        "what_was_known_of_a_value_before_a_call_changes_it_is_not_known_after",
        what_was_known_of_a_value_before_a_call_changes_it_is_not_known_after,
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

const CURSOR: &str = "use std.bits (u8)
use std.console (console, print)

struct Cursor {
  at: u8
  line: u8
}

struct Doc {
  cursor: Cursor
  other: Cursor
}
";

fn a_function_changes_a_var_through_an_mut_parameter_as_the_page_says() {
    let source = format!(
        "{CURSOR}
fn step(c: mut Cursor) {{
  if c.at < 9 {{ c.at = c.at + 1 }}
}}

fn main() <console, alloc> {{
  var c = Cursor(at: 0, line: 1)
  step(mut c)
  step(mut c)
  print(\"{{c.at}}\")
}}
"
    );
    assert_eq!(output_of(&source), "2\n", "for {source}");
    // `inout` is a name like any other, and not a keyword.
    let source = "fn main() {\n  inout = 1\n  n = inout + 1\n}\n";
    assert!(errors(source).is_empty(), "{:?} for {source}", errors(source));
}

/// A line of the body of a function that changes the cursor `x`.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// Adds a number below 50 to `at` when it is below 200.
    AddAt(u8),
    AddLine(u8),
    /// `line` becomes `at`.
    Copy,
    /// The cursor is replaced by a new one.
    Replace(u8, u8),
    /// The function returns when `at` is the number.
    ReturnAt(u8),
}

impl Step {
    fn text(self) -> String {
        match self {
            Step::AddAt(k) => format!("if x.at < 200 {{ x.at = x.at + {k} }}"),
            Step::AddLine(k) => format!("if x.line < 200 {{ x.line = x.line + {k} }}"),
            Step::Copy => "x.line = x.at".to_string(),
            Step::Replace(a, l) => format!("x = Cursor(at: {a}, line: {l})"),
            Step::ReturnAt(k) => format!("if x.at == {k} {{ return }}"),
        }
    }
}

/// A cursor after the lines run: `None` when it returns.
fn run(lines: &[Step], (mut at, mut line): (u8, u8)) -> (u8, u8) {
    for l in lines {
        match *l {
            Step::AddAt(k) if at < 200 => at += k,
            Step::AddLine(k) if line < 200 => line += k,
            Step::AddAt(_) | Step::AddLine(_) => {}
            Step::Copy => line = at,
            Step::Replace(a, l) => (at, line) = (a, l),
            Step::ReturnAt(k) if at == k => break,
            Step::ReturnAt(_) => {}
        }
    }
    (at, line)
}

#[derive(Debug)]
struct Change {
    lines: Vec<Step>,
    start: (u8, u8),
    calls: usize,
    /// 0 for a `var`, 1 for a field of one, 2 for an item of a list.
    place: usize,
    /// Whether the body ends with a bare `return`.
    returns: bool,
}

fn change(g: &mut Gen) -> Change {
    let n = 1 + g.below(5);
    let lines = (0..n)
        .map(|_| match g.below(5) {
            0 => Step::AddAt(g.below(50) as u8),
            1 => Step::AddLine(g.below(50) as u8),
            2 => Step::Copy,
            3 => Step::Replace(g.below(10) as u8, g.below(10) as u8),
            _ => Step::ReturnAt(g.below(10) as u8),
        })
        .collect();
    Change {
        lines,
        start: (g.below(10) as u8, g.below(10) as u8),
        calls: 1 + g.below(3),
        place: g.below(3),
        returns: g.one_in(3),
    }
}

/// The program that changes the place `calls` times, with `mut` when
/// `mut` is true, and by taking it and assigning what comes back otherwise.
fn change_program(c: &Change, mutable: bool) -> String {
    let body: Vec<String> = c.lines.iter().map(|l| format!("  {}", l.text())).collect();
    let function = if mutable {
        let end = if c.returns { "\n  return" } else { "" };
        format!("fn change(x: mut Cursor) {{\n{}{end}\n}}", body.join("\n"))
    } else {
        let body = body.join("\n").replace("{ return }", "{ return x }");
        let end = if c.returns { "return x" } else { "x" };
        format!("fn change(given: take Cursor) -> Cursor {{\n  var x = given\n{body}\n  {end}\n}}")
    };
    let (at, line) = c.start;
    let start = format!("Cursor(at: {at}, line: {line})");
    let (made, place) = match c.place {
        0 => (format!("var c = {start}"), "c"),
        1 => (
            format!("var d = Doc(cursor: {start}, other: Cursor(at: 0, line: 0))"),
            "d.cursor",
        ),
        _ => (format!("var cs = [{start}]"), "cs[0]"),
    };
    let call = if mutable {
        format!("  change(mut {place})\n")
    } else {
        format!("  {place} = change({place})\n")
    };
    format!(
        "{CURSOR}\n{function}\n\nfn main() <console, alloc> {{\n  {made}\n{}  print(\"{{{place}.at}} {{{place}.line}}\")\n}}\n",
        call.repeat(c.calls)
    )
}

fn changing_a_place_with_mut_gives_what_taking_it_and_assigning_what_comes_back_gives_and_the_same_code()
 {
    generate::check(12331, 6, change, |c| {
        let (mutable, taken) = (change_program(c, true), change_program(c, false));
        let mut cursor = c.start;
        for _ in 0..c.calls {
            cursor = run(&c.lines, cursor);
        }
        let expected = format!("{} {}\n", cursor.0, cursor.1);
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
    // A body that ends with a bare `return` gives the value back there, as a
    // function that does not change its parameter ends there.
    let ends = Change {
        lines: vec![Step::Copy],
        start: (1, 2),
        calls: 1,
        place: 0,
        returns: true,
    };
    let source = change_program(&ends, true);
    assert_eq!(output_of(&source), "1 1\n", "for {source}");
}

/// Lines with a mistake in them, in the body of a function that changes the
/// cursor `x`, each with whether it is a call that changes a value, whose one
/// error is at its argument.
const MISTAKES: &[(&str, bool)] = &[
    ("print(\"x\")", false),
    ("n = \"{x.at}\"", false),
    ("x.at = \"a\"", false),
    ("x.at = x.at + 1", false),
    ("n = nowhere", false),
    ("step(mut x.at)", true),
    ("n = look(x.at)", false),
];

/// What an error says, for comparing: where it is, and the place of its
/// column only on a line after the signature, whose spelling differs.
fn said(e: &Error, signature: usize) -> (usize, Option<usize>, String, Option<String>) {
    let written = |t: &str| {
        t.replace("fn change(given: take Cursor) -> Cursor", "fn change(x: mut Cursor)")
            .replace("fn change(given: Cursor) -> Cursor", "fn change(x: mut Cursor)")
    };
    let col = (e.pos.line != signature).then_some(e.pos.col);
    (e.pos.line, col, written(&e.message), e.fix.as_deref().map(written))
}

/// A function that changes a cursor, with `lines` for its body, written with
/// `mut` or with `take`, and called from `main`. The lines of the body are on
/// the same lines of the file in both.
fn mistake_program(lines: &[&str], mutable: bool) -> String {
    let body: String = lines.iter().map(|l| format!("  {l}\n")).collect();
    let (function, call) = if mutable {
        (format!("fn change(x: mut Cursor) {{\n{body}}}"), "change(mut c)")
    } else {
        (
            format!("fn change(given: take Cursor) -> Cursor {{ var x = given\n{body}  x\n}}"),
            "c = change(c)",
        )
    };
    format!(
        "{CURSOR}
fn step(c: mut Cursor) {{
  if c.at < 9 {{ c.at = c.at + 1 }}
}}

fn look(c: Cursor) -> u8 {{ c.at }}

{function}

fn main() <alloc> {{
  var c = Cursor(at: 0, line: 0)
  {call}
}}
"
    )
}

fn a_mistake_in_a_function_that_changes_a_parameter_is_reported_as_in_the_function_that_takes_it_with_the_signature_as_written()
 {
    generate::check(
        12334,
        30,
        |g: &mut Gen| {
            let fine = ["if x.at < 9 { x.at = x.at + 1 }", "x.line = x.at"];
            let before = (0..g.below(3)).map(|_| *g.pick(&fine)).collect::<Vec<_>>();
            (before, *g.pick(MISTAKES))
        },
        |(before, (mistake, changes))| {
            let mut lines = before.clone();
            lines.push(mistake);
            let (mutable, taken) = (mistake_program(&lines, true), mistake_program(&lines, false));
            let signature = 1 + (mutable.lines())
                .position(|l| l.starts_with("fn change"))
                .expect("the signature");
            let one: Vec<_> = errors(&mutable).iter().map(|e| said(e, signature)).collect();
            let other: Vec<_> = errors(&taken).iter().map(|e| said(e, signature)).collect();
            assert!(!one.is_empty(), "nothing refused in {mutable}");
            assert_eq!(one, other, "for {mutable} and {taken}");
            for (_, _, message, fix) in &one {
                let text = format!("{message} {}", fix.as_deref().unwrap_or(""));
                assert!(
                    !text.contains("-> Cursor") && !text.contains("given"),
                    "an error shows the lowered function: {text} for {mutable}"
                );
            }
            // A call that changes a value is refused once, at the argument
            // that does not fit.
            if *changes {
                let line = signature + lines.len();
                let col = 3 + mistake.find("x.at").expect("the argument");
                assert_eq!(one.len(), 1, "{one:?} for {mutable}");
                assert_eq!((one[0].0, one[0].1), (line, Some(col)), "for {mutable}");
            }
        },
    );
}

#[derive(Debug)]
struct Marks {
    /// Whether each parameter is marked `mut`.
    params: Vec<bool>,
    /// Whether each argument is marked `mut`.
    args: Vec<bool>,
}

fn marks(g: &mut Gen) -> Marks {
    let n = 1 + g.below(3);
    let params: Vec<bool> = (0..n).map(|_| g.one_in(2)).collect();
    // Most calls mark what they should, so that the accepted ones are many.
    let args = if g.one_in(2) {
        params.clone()
    } else {
        (0..n).map(|_| g.one_in(2)).collect()
    };
    Marks { params, args }
}

fn marks_program(m: &Marks) -> (String, String, String) {
    let params: Vec<String> = (m.params.iter().enumerate())
        .map(|(i, &mutable)| format!("p{i}: {}Cursor", if mutable { "mut " } else { "" }))
        .collect();
    let body: Vec<String> = (m.params.iter().enumerate())
        .map(|(i, &mutable)| {
            if mutable {
                format!("  if p{i}.at < 9 {{ p{i}.at = p{i}.at + 1 }}\n")
            } else {
                format!("  n{i} = p{i}.at\n")
            }
        })
        .collect();
    let written = |marked: &dyn Fn(usize) -> bool| {
        let args: Vec<String> = (0..m.params.len())
            .map(|i| format!("{}c{i}", if marked(i) { "mut " } else { "" }))
            .collect();
        format!("f({})", args.join(", "))
    };
    let call = written(&|i| m.args[i]);
    let fixed = written(&|i| m.args[i] || m.params[i]);
    let vars: String = (0..m.params.len())
        .map(|i| format!("  var c{i} = Cursor(at: {i}, line: 0)\n"))
        .collect();
    let before_call = format!(
        "{CURSOR}\nfn f({}) {{\n{}}}\n\nfn main() {{\n{vars}  ",
        params.join(", "),
        body.concat()
    );
    let source = format!("{before_call}{call}\n}}\n");
    (source, before_call, fixed)
}

fn a_call_marks_mut_exactly_the_arguments_whose_parameters_are_mut() {
    generate::check(12332, 30, marks, |m| {
        let (source, before_call, fixed) = marks_program(m);
        let errors = errors(&source);
        if m.params == m.args {
            assert!(errors.is_empty(), "{errors:?} for {source}");
            return;
        }
        // An argument that is not marked where its parameter is `mut` is
        // refused with the call written as it should be.
        if (m.params.iter().zip(&m.args)).any(|(p, a)| *p && !*a) {
            let fix = format!("write `{fixed}`");
            assert!(
                errors.iter().any(|e| e.fix.as_deref() == Some(fix.as_str())),
                "no error with the fix {fix} among {errors:?} for {source}"
            );
        }
        // Each `mut` before an argument whose parameter is not marked is
        // refused where it is written.
        let mut text = before_call.clone() + "f(";
        for (i, (p, a)) in m.params.iter().zip(&m.args).enumerate() {
            if *a && !*p {
                let at = pos_after(&text);
                assert!(
                    errors.iter().any(|e| e.pos == at),
                    "no error at {at:?} among {errors:?} for {source}"
                );
            }
            text += &format!("{}c{i}, ", if *a { "mut " } else { "" });
        }
    });
}

fn what_is_passed_mut_is_a_var_or_a_field_or_an_item_of_one() {
    let program = |made: &str, passed: &str| {
        format!(
            "{CURSOR}\nfn step(c: mut Cursor) {{\n  if c.at < 9 {{ c.at = c.at + 1 }}\n}}\n\nfn start() -> Cursor {{ Cursor(at: 0, line: 0) }}\n\nfn main() <alloc> {{\n  {made}\n  step(mut {passed})\n}}\n"
        )
    };
    let start = "Cursor(at: 0, line: 0)";
    let doc = format!("Doc(cursor: {start}, other: {start})");
    for (made, passed) in [
        (format!("var c = {start}"), "c".to_string()),
        (format!("var d = {doc}"), "d.cursor".to_string()),
        (format!("var cs = [{start}]"), "cs[0]".to_string()),
    ] {
        let source = program(&made, &passed);
        assert!(errors(&source).is_empty(), "{:?} for {source}", errors(&source));
    }
    for (made, passed) in [
        (format!("c = {start}"), "c".to_string()),
        (format!("d = {doc}"), "d.cursor".to_string()),
        ("n = 1".to_string(), start.to_string()),
        ("n = 1".to_string(), "start()".to_string()),
    ] {
        let source = program(&made, &passed);
        let at = pos_after(&source[..source.find(&format!("mut {passed}")).expect("call")]);
        let errors = errors(&source);
        assert!(
            errors.iter().any(|e| e.pos.line == at.line),
            "nothing refused on line {} among {errors:?} for {source}",
            at.line
        );
    }
    // A field of an item is a place that is not built, and is refused as one,
    // at the argument.
    let source = program(&format!("var ds = [{doc}]"), "ds[0].cursor");
    let at = pos_after(&source[..source.find("ds[0].cursor").expect("call")]);
    let errors = errors(&source);
    assert!(
        errors.iter().any(|e| e.pos == at && e.unbuilt),
        "not refused as not built at {at:?} among {errors:?} for {source}"
    );
}

/// The places two arguments may name.
const PLACES: &[&str] = &["a", "a.cursor", "a.other", "b", "b.cursor", "xs[0]", "xs[1]"];

/// Whether two places are one, or one holds the other.
fn overlaps(x: &str, y: &str) -> bool {
    let root = |p: &str| p.split(['.', '[']).next().expect("a root").to_string();
    x == y || (root(x) == root(y) && !x.contains('[') && (x.starts_with(y) || y.starts_with(x)))
}

fn one_place_is_passed_mut_once_and_places_that_do_not_overlap_are_passed_together() {
    generate::check(
        12333,
        30,
        |g: &mut Gen| (*g.pick(PLACES), *g.pick(PLACES)),
        |&(x, y)| {
            let start = "Cursor(at: 1, line: 2)";
            let source = format!(
                "{CURSOR}
fn both[T, U](p: mut T, q: mut U) {{
}}

fn main() <alloc> {{
  var a = Doc(cursor: {start}, other: Cursor(at: 3, line: 4))
  var b = Doc(cursor: {start}, other: {start})
  var xs = [{start}, Cursor(at: 5, line: 6)]
  both(mut {x}, mut {y})
}}
"
            );
            let errors = errors(&source);
            if overlaps(x, y) {
                let at = pos_after(&source[..source.find("both(mut").expect("call")]);
                assert!(
                    errors.iter().any(|e| e.pos.line == at.line),
                    "{x} and {y} are passed together in {source}"
                );
            } else {
                assert!(errors.is_empty(), "{errors:?} for {source}");
            }
        },
    );
    // Two places that do not overlap are changed as two.
    let source = format!(
        "{CURSOR}
fn swap(p: mut Cursor, q: mut Cursor) {{
  t = p.at
  p.at = q.at
  q.at = t
}}

fn main() <console, alloc> {{
  var d = Doc(cursor: Cursor(at: 1, line: 0), other: Cursor(at: 2, line: 0))
  swap(mut d.cursor, mut d.other)
  var xs = [Cursor(at: 3, line: 0), Cursor(at: 4, line: 0)]
  swap(mut xs[0], mut xs[1])
  print(\"{{d.cursor.at}} {{d.other.at}} {{xs[0].at}} {{xs[1].at}}\")
}}
"
    );
    assert_eq!(output_of(&source), "2 1 4 3\n", "for {source}");
}

fn a_function_does_not_store_return_or_give_a_lambda_a_parameter_marked_mut() {
    let types = "use std.bits (u8)

struct Log {
  items: List[u8]
  count: u8
}

struct Shelf {
  log: Log
}

struct Kept {
  items: List[u8]
}

fn burn(l: take Log) {
  shelf = Shelf(log: l)
}

fn run(f: fn() -> u8) -> u8 { f() }
";
    // Each line, and what is written on it before the use that is refused.
    for (line, before) in [
        ("shelf = Shelf(log: l)", "shelf = Shelf(log: "),
        ("kept = Kept(items: l.items)", "kept = Kept(items: "),
        ("var other = l", "var other = "),
        ("burn(l)", "burn("),
        ("return l", ""),
        ("n = run() { l.count }", "n = run() { "),
        ("n = run() {\n    l.count = 1\n    1\n  }", "n = run() {\n    "),
    ] {
        let source = format!(
            "{types}
fn keep(l: mut Log) {{
  {line}
}}

fn main() <alloc> {{
  var l = Log(items: [], count: 0)
  keep(mut l)
}}
"
        );
        let line_at = source.rfind(line).expect("the line");
        let at = pos_after(&source[..line_at + before.len()]);
        let errors = errors(&source);
        assert!(
            errors.iter().any(|e| e.pos == at),
            "nothing refused at {at:?} among {errors:?} for {source}"
        );
    }
    // A part that is copied is only read.
    let source = format!(
        "{types}
fn keep(l: mut Log) {{
  n = l.count
  if n < 9 {{ l.count = n + 1 }}
}}

fn main() <alloc> {{
  var l = Log(items: [], count: 0)
  keep(mut l)
}}
"
    );
    assert!(errors(&source).is_empty(), "{:?} for {source}", errors(&source));
}

fn what_was_known_of_a_value_before_a_call_changes_it_is_not_known_after() {
    let program = |changed: &str| {
        format!(
            "use std.bits (u8)

fn zero(n: u8) requires n == 0 {{
}}

fn same(n: mut u8) {{
  n = n
}}

fn main() {{
  var n: u8 = 0
  var m: u8 = 0
  zero(n)
  same(mut {changed})
  zero(n)
}}
"
        )
    };
    assert!(errors(&program("m")).is_empty(), "{:?}", errors(&program("m")));
    let source = program("n");
    let at = pos_after(&source[..source.rfind("zero(n)").expect("the second call")]);
    let errors = errors(&source);
    assert!(
        errors.iter().any(|e| e.pos == at),
        "nothing refused at {at:?} among {errors:?} for {source}"
    );
}
