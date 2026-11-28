//! A trait's function, an implementation, a function type, and an operation
//! of an effect mark a parameter `mut` as a function does, as
//! `knowledge/language/mut-parameters.md` says. It means taking the value and
//! giving it back, before what it gives if it gives a result, so a program
//! that changes a value through one of them gives what the program written
//! with `take` gives, and is built to the same code.
//!
//! An implementation marks `mut` exactly the parameters its trait's function
//! marks, and a lambda exactly those the function type or the operation it is
//! passed as marks.

use smith::check;
use smith::diag::Error;
use smith::target::Target;
use smith::{compile, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::pos_after;

pub const ONCE: &[Test] = &[
    (
        "a_function_bound_by_a_trait_changes_a_value_through_it_as_the_issue_says",
        a_function_bound_by_a_trait_changes_a_value_through_it_as_the_issue_says,
    ),
    (
        "changing_a_value_through_a_trait_a_function_type_or_an_operation_gives_what_taking_it_gives_and_the_same_code",
        changing_a_value_through_a_trait_a_function_type_or_an_operation_gives_what_taking_it_gives_and_the_same_code,
    ),
    (
        "an_implementation_marks_mut_exactly_the_parameters_its_trait_marks",
        an_implementation_marks_mut_exactly_the_parameters_its_trait_marks,
    ),
    (
        "a_lambda_marks_mut_exactly_the_parameters_its_function_type_or_operation_marks",
        a_lambda_marks_mut_exactly_the_parameters_its_function_type_or_operation_marks,
    ),
    (
        "a_call_that_leaves_out_the_mut_or_the_take_its_parameter_has_is_refused_at_the_argument",
        a_call_that_leaves_out_the_mut_or_the_take_its_parameter_has_is_refused_at_the_argument,
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

fn a_function_bound_by_a_trait_changes_a_value_through_it_as_the_issue_says() {
    let source = "use std.console (console, print)
use std.bits (u8)

struct Count {
  n: u8
}

trait Step[T] {
  fn step(t: mut T)
}

impl Step[Count] {
  fn step(t: mut Count) {
    if t.n < 9 { t.n = t.n + 1 }
  }
}

fn twice[T: Step](t: mut T) {
  step(mut t)
  step(mut t)
}

fn main() <console, alloc> {
  mut c = Count(n: 0)
  twice(mut c)
  print(\"{c.n}\")
}
";
    assert_eq!(output_of(source), "2\n", "for {source}");
}

const CURSOR: &str = "use std.bits (u8)
use std.console (console, print)

struct Cursor {
  at: u8
  line: u8
}
";

/// A line of the body of what changes the cursor `x`.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// Adds a number below 50 to `at` when it is below 200.
    AddAt(u8),
    AddLine(u8),
    /// `line` becomes `at`.
    Copy,
    /// The cursor is replaced by a new one.
    Replace(u8, u8),
}

impl Step {
    fn text(self) -> String {
        match self {
            Step::AddAt(k) => format!("if x.at < 200 {{ x.at = x.at + {k} }}"),
            Step::AddLine(k) => format!("if x.line < 200 {{ x.line = x.line + {k} }}"),
            Step::Copy => "x.line = x.at".to_string(),
            Step::Replace(a, l) => format!("x = Cursor(at: {a}, line: {l})"),
        }
    }

    fn run(self, (at, line): (u8, u8)) -> (u8, u8) {
        match self {
            Step::AddAt(k) if at < 200 => (at + k, line),
            Step::AddLine(k) if line < 200 => (at, line + k),
            Step::AddAt(_) | Step::AddLine(_) => (at, line),
            Step::Copy => (at, at),
            Step::Replace(a, l) => (a, l),
        }
    }
}

/// What the parameter marked `mut` is a parameter of.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Of {
    /// A trait's function, written by an implementation.
    Trait,
    /// A function type, of which a lambda is passed.
    FunctionType,
    /// An operation of an effect, which a handler's lambda answers.
    Operation,
}

#[derive(Debug)]
struct Change {
    of: Of,
    lines: Vec<Step>,
    start: (u8, u8),
    calls: usize,
    /// Whether it gives `line` as a result besides the cursor.
    gives: bool,
}

fn change(g: &mut Gen) -> Change {
    let n = 1 + g.below(4);
    let lines = (0..n)
        .map(|_| match g.below(4) {
            0 => Step::AddAt(g.below(50) as u8),
            1 => Step::AddLine(g.below(50) as u8),
            2 => Step::Copy,
            _ => Step::Replace(g.below(10) as u8, g.below(10) as u8),
        })
        .collect();
    Change {
        of: *g.pick(&[Of::Trait, Of::FunctionType, Of::Operation]),
        lines,
        start: (g.below(10) as u8, g.below(10) as u8),
        calls: 1 + g.below(3),
        gives: g.one_in(2),
    }
}

/// The program that changes a cursor `calls` times through what `of` says,
/// with `mut` when `mutable` is true, and by taking it and binding what comes
/// back otherwise. It prints the cursor, and what the last call gives when
/// it gives a result.
fn change_program(c: &Change, mutable: bool) -> String {
    let lines: Vec<String> = c.lines.iter().map(|l| format!("  {}", l.text())).collect();
    let lines = lines.join("\n");
    // The signature of what changes the cursor, and its body.
    let (params, result, body) = match (mutable, c.gives) {
        (true, false) => ("x: mut Cursor", String::new(), lines.clone()),
        (true, true) => ("x: mut Cursor", " -> u8".to_string(), format!("{lines}\n  x.line")),
        (false, false) => (
            "given: take Cursor",
            " -> Cursor".to_string(),
            format!("  mut x = given\n{lines}\n  x"),
        ),
        (false, true) => (
            "given: take Cursor",
            " -> (Cursor, u8)".to_string(),
            format!("  mut x = given\n{lines}\n  (x, x.line)"),
        ),
    };
    // A lambda takes what its function type or its operation marks `take`
    // without writing it.
    let lambda_params = params.replace("take ", "");
    let lambda = format!("fn({lambda_params}){result} {{\n{body}\n}}");
    // The calls, each of which binds what it gives, and the last is shown.
    let calls: String = (0..c.calls)
        .map(|i| {
            let kept = format!("t{i}");
            match (mutable, c.gives) {
                (true, false) => "  change(mut c)\n".to_string(),
                (true, true) => format!("  {kept} = change(mut c)\n"),
                (false, false) => "  c = change(c)\n".to_string(),
                (false, true) => format!("  (c, {kept}) = change(c)\n"),
            }
        })
        .collect();
    let (at, line) = c.start;
    let shown = match c.gives {
        true => format!(" {{t{}}}", c.calls - 1),
        false => String::new(),
    };
    let run = format!(
        "  mut c = Cursor(at: {at}, line: {line})\n{calls}  print(\"{{c.at}} {{c.line}}{shown}\")\n"
    );
    let type_params = params
        .replace("x: mut Cursor", "mut Cursor")
        .replace("given: take Cursor", "take Cursor");
    match c.of {
        Of::Trait => {
            let trait_params = params.replace("Cursor", "T");
            let trait_result = result.replace("Cursor", "T");
            format!(
                "{CURSOR}
trait Change[T] {{
  fn change({trait_params}){trait_result}
}}

impl Change[Cursor] {{
  fn change({params}){result} {{
{body}
  }}
}}

fn main() <console, alloc> {{
{run}}}
"
            )
        }
        Of::FunctionType => format!(
            "{CURSOR}
fn changing(change: fn({type_params}){result}) <console, alloc> {{
{run}}}

fn main() <console, alloc> {{
  changing({lambda})
}}
"
        ),
        Of::Operation => format!(
            "{CURSOR}
effect changer {{
  fn change({params}){result}
}}

fn changed[T](body: fn() -> T <changer>) -> T {{
  changer(change: {lambda}, body: body)
}}

fn run() <console, alloc> {{
  with changed()
{run}}}

fn main() <console, alloc> {{
  run()
}}
"
        ),
    }
}

fn changing_a_value_through_a_trait_a_function_type_or_an_operation_gives_what_taking_it_gives_and_the_same_code()
 {
    generate::check(12391, 9, change, |c| {
        let (mutable, taken) = (change_program(c, true), change_program(c, false));
        let mut cursor = c.start;
        for _ in 0..c.calls {
            cursor = c.lines.iter().fold(cursor, |at, l| l.run(at));
        }
        let expected = match c.gives {
            true => format!("{} {} {}\n", cursor.0, cursor.1, cursor.1),
            false => format!("{} {}\n", cursor.0, cursor.1),
        };
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

/// Whether each parameter is marked `mut` in the trait's function, and in
/// the implementation's, or in the function type or operation and in the
/// lambda passed as it.
#[derive(Debug)]
struct Marks {
    declared: Vec<bool>,
    written: Vec<bool>,
}

fn marks(g: &mut Gen) -> Marks {
    let n = 1 + g.below(2);
    let declared: Vec<bool> = (0..n).map(|_| g.one_in(2)).collect();
    // Most mark what they should, so that the accepted ones are many.
    let written = if g.one_in(2) {
        declared.clone()
    } else {
        (0..n).map(|_| g.one_in(2)).collect()
    };
    Marks { declared, written }
}

/// The parameters `p0`, `p1`, ..., of the type `ty`, each marked `mut` where
/// `marked` says.
fn params(marked: &[bool], ty: &str) -> String {
    let params: Vec<String> = (marked.iter().enumerate())
        .map(|(i, &m)| format!("p{i}: {}{ty}", if m { "mut " } else { "" }))
        .collect();
    params.join(", ")
}

/// A body that changes each parameter `marked` says, and reads the others.
fn changes(marked: &[bool]) -> String {
    (marked.iter().enumerate())
        .map(|(i, &m)| match m {
            true => format!("    if p{i}.at < 9 {{ p{i}.at = p{i}.at + 1 }}\n"),
            false => format!("    n{i} = p{i}.at\n"),
        })
        .collect()
}

/// The arguments of a call that passes `c0`, `c1`, ..., each marked `mut`
/// where `marked` says.
fn args(marked: &[bool]) -> String {
    let args: Vec<String> = (marked.iter().enumerate())
        .map(|(i, &m)| format!("{}c{i}", if m { "mut " } else { "" }))
        .collect();
    args.join(", ")
}

/// The cursors a call is given, and the line that prints what the first
/// became.
fn cursors(n: usize) -> (String, &'static str) {
    let made: String = (0..n)
        .map(|i| format!("  mut c{i} = Cursor(at: {i}, line: 0)\n"))
        .collect();
    (made, "  print(\"{c0.at}\")\n")
}

fn an_implementation_marks_mut_exactly_the_parameters_its_trait_marks() {
    generate::check(12392, 12, marks, |m| {
        let (made, shown) = cursors(m.declared.len());
        let before_impl = format!(
            "{CURSOR}
trait Move[T] {{
  fn move({})
}}

impl Move[Cursor] {{
  fn move(",
            params(&m.declared, "T")
        );
        let source = format!(
            "{before_impl}{}) {{
{}  }}
}}

fn main() <console, alloc> {{
{made}  move({})
{shown}}}
",
            params(&m.written, "Cursor"),
            changes(&m.written),
            args(&m.declared)
        );
        let errors = errors(&source);
        if m.declared == m.written {
            let first = if m.declared[0] { "1" } else { "0" };
            assert_eq!(output_of(&source), format!("{first}\n"), "for {source}");
            return;
        }
        // The implementation is refused at the first parameter it marks
        // differently, naming the trait.
        let first = (m.declared.iter().zip(&m.written))
            .position(|(d, w)| d != w)
            .expect("one differs");
        let written = params(&m.written, "Cursor");
        let at = written.find(&format!("p{first}:")).expect("the parameter");
        let at = pos_after(&format!("{before_impl}{}", &written[..at]));
        assert!(
            (errors.iter()).any(|e| e.pos == at && e.message.contains("`Move`")),
            "not refused at {at:?}, naming `Move`, among {errors:?} for {source}"
        );
    });
}

fn a_lambda_marks_mut_exactly_the_parameters_its_function_type_or_operation_marks() {
    generate::check(
        12393,
        12,
        |g: &mut Gen| (marks(g), g.one_in(2)),
        |(m, operation)| {
            let (made, shown) = cursors(m.declared.len());
            let declared = m.declared.iter().map(|&d| if d { "mut Cursor" } else { "Cursor" });
            let declared: Vec<&str> = declared.collect();
            let lambda = format!("fn({}) {{\n{}  }}", params(&m.written, "Cursor"), changes(&m.written));
            let call = format!("  move({})\n", args(&m.declared));
            let (before_lambda, after_lambda) = if *operation {
                (
                    format!(
                        "{CURSOR}
effect mover {{
  fn move({})
}}

fn moved[T](body: fn() -> T <mover>) -> T {{
  mover(move: ",
                        params(&m.declared, "Cursor")
                    ),
                    format!(
                        ", body: body)
}}

fn main() <console, alloc> {{
  with moved()
{made}{call}{shown}}}
"
                    ),
                )
            } else {
                (
                    format!(
                        "{CURSOR}
fn moving(move: fn({})) <console, alloc> {{
{made}{call}{shown}}}

fn main() <console, alloc> {{
  moving(",
                        declared.join(", ")
                    ),
                    ")\n}\n".to_string(),
                )
            };
            let source = format!("{before_lambda}{lambda}{after_lambda}");
            let errors = errors(&source);
            if m.declared == m.written {
                // The lambda changes what it marks as a function does.
                let first = if m.declared[0] { "1" } else { "0" };
                assert_eq!(output_of(&source), format!("{first}\n"), "for {source}");
                return;
            }
            // The lambda is refused at the first parameter it marks
            // differently.
            let first = (m.declared.iter().zip(&m.written))
                .position(|(d, w)| d != w)
                .expect("one differs");
            let at = lambda.find(&format!("p{first}:")).expect("the parameter");
            let at = pos_after(&format!("{before_lambda}{}", &lambda[..at]));
            assert!(
                errors.iter().any(|e| e.pos == at),
                "not refused at {at:?} among {errors:?} for {source}"
            );
        },
    );
}

/// Where a call is of something whose parameter is marked `mut` or `take`.
#[derive(Clone, Copy, Debug)]
enum Callee {
    /// A trait's function, called at the type it is implemented for.
    Trait,
    /// A trait's function, called in a function the trait bounds.
    Bound,
    FunctionType,
    Operation,
}

/// A call that leaves out the mark of its parameter: on a line of its own,
/// or bound to a name, of a parameter marked `mut`, or else `take`.
#[derive(Debug)]
struct Unmarked {
    callee: Callee,
    bound: bool,
    take: bool,
}

/// The program with the call `line` of `g`, whose parameter is `marked` a
/// `Bag`, as `u.callee` says, and what is written before the call's
/// argument.
fn unmarked_program(u: &Unmarked, line: &str) -> (String, String) {
    let marked = if u.take { "take" } else { "mut" };
    let gives = if u.take { " -> u8" } else { "" };
    let body = if u.take { " 1 " } else { "" };
    // What the call is given, which has no `Copy`.
    let made = if u.take { "b" } else { "mut b" };
    let made = format!("  {made} = Bag(items: [1])\n");
    let types = "use std.bits (u8)\n\nstruct Bag {\n  items: List[u8]\n}\n";
    let (before, after) = match u.callee {
        Callee::Trait | Callee::Bound => {
            let declared = format!(
                "{types}\ntrait G[T] {{\n  fn g(t: {marked} T){gives}\n}}\n\nimpl G[Bag] {{\n  fn g(t: {marked} Bag){gives} {{{body}}}\n}}\n"
            );
            match u.callee {
                Callee::Trait => (format!("{declared}\nfn main() <alloc> {{\n{made}"), "}\n".to_string()),
                _ => (
                    format!("{declared}\nfn through[T: G](b: {marked} T) {{\n"),
                    format!("}}\n\nfn main() <alloc> {{\n{made}  through({marked} b)\n}}\n"),
                ),
            }
        }
        Callee::FunctionType => (
            format!("{types}\nfn calling(g: fn({marked} Bag){gives}) <alloc> {{\n{made}"),
            format!("}}\n\nfn main() <alloc> {{\n  calling(fn(t: {marked} Bag){gives} {{{body}}})\n}}\n"),
        ),
        Callee::Operation => (
            format!(
                "{types}\neffect gs {{\n  fn g(t: {marked} Bag){gives}\n}}\n\nfn handled[T](body: fn() -> T <gs>) -> T {{\n  gs(g: fn(t: {marked} Bag){gives} {{{body}}}, body: body)\n}}\n\nfn run() <alloc> {{\n  with handled()\n{made}"
            ),
            "}\n\nfn main() <alloc> {\n  run()\n}\n".to_string(),
        ),
    };
    let source = format!("{before}  {line}\n{after}");
    let at = format!("{before}  {}", &line[..line.find('(').expect("the call") + 1]);
    (source, at)
}

fn a_call_that_leaves_out_the_mut_or_the_take_its_parameter_has_is_refused_at_the_argument() {
    generate::check(
        12394,
        16,
        |g: &mut Gen| {
            let callee = *g.pick(&[Callee::Trait, Callee::Bound, Callee::FunctionType, Callee::Operation]);
            // `take` is checked here for a trait's function, which a call
            // finds by the types of its arguments.
            let take = matches!(callee, Callee::Trait | Callee::Bound) && g.one_in(3);
            Unmarked {
                callee,
                bound: take || g.one_in(2),
                take,
            }
        },
        |u| {
            let line = if u.bound { "x = g(b)" } else { "g(b)" };
            let (source, before) = unmarked_program(u, line);
            let at = pos_after(&before);
            let marked = if u.take { "take" } else { "mut" };
            let fix = format!("write `g({marked} b)`");
            let found = errors(&source);
            assert!(
                found.iter().any(|e| e.pos == at && e.fix.as_deref() == Some(fix.as_str())),
                "not refused at {at:?} with the fix {fix} among {found:?} for {source}"
            );
            // With the mark written, the call is accepted.
            let written = line.replace("(b)", &format!("({marked} b)"));
            let (fixed, _) = unmarked_program(u, &written);
            let fixed = match (u.take, u.bound) {
                // What a call that changes a value and gives nothing else
                // gives is not bound.
                (false, true) => unmarked_program(u, &format!("g({marked} b)")).0,
                _ => fixed,
            };
            let refused = errors(&fixed);
            assert!(refused.is_empty(), "{refused:?} for {fixed}");
        },
    );
}
