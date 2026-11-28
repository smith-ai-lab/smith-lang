//! `expect` of `std.test`, the check of a test: a check that holds gives
//! `.ok(())`, and one that fails gives a `Failed` that names the file, the
//! line and the column of the `expect`, and the check as written, with both
//! sides of a comparison as `repr` writes them.
//!
//! The generator writes each check from values it chose, so it knows whether
//! the check holds, where it is, and what each side is. The programs run in
//! the evaluator, so nothing here depends on the machine.

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_comparison_that_fails_shows_both_sides_as_repr_writes_them",
        a_comparison_that_fails_shows_both_sides_as_repr_writes_them,
    ),
    (
        "a_check_given_as_a_value_and_not_written_in_braces_is_refused_at_the_call",
        a_check_given_as_a_value_and_not_written_in_braces_is_refused_at_the_call,
    ),
];

/// A value one side of a comparison holds, as the program writes it and as
/// `repr` writes it.
#[derive(Clone, Debug)]
enum Value {
    Whole(i64),
    Text(String),
    Truth(bool),
}

impl Value {
    fn repr(&self) -> String {
        match self {
            Value::Whole(n) => n.to_string(),
            Value::Text(t) => format!("\"{t}\""),
            Value::Truth(b) => b.to_string(),
        }
    }

    /// The function that gives the value back, by which a side is worked
    /// out by a call.
    fn same(&self) -> &'static str {
        match self {
            Value::Whole(_) => "same_whole",
            Value::Text(_) => "same_text",
            Value::Truth(_) => "same_truth",
        }
    }
}

/// One check: the lines before the `expect`, the check as written, and what
/// a failure says of it: `None` when it holds, and otherwise the sides, which
/// a check that is no comparison has none of.
#[derive(Debug)]
struct Check {
    lines: Vec<String>,
    written: String,
    fails: Option<Option<(String, String)>>,
}

const OPS: [&str; 6] = ["==", "!=", "<", "<=", ">", ">="];

fn holds(op: &str, a: &Value, b: &Value) -> bool {
    let order = match (a, b) {
        (Value::Whole(x), Value::Whole(y)) => x.cmp(y),
        (Value::Text(x), Value::Text(y)) => x.cmp(y),
        (Value::Truth(x), Value::Truth(y)) => x.cmp(y),
        _ => unreachable!("the sides of a comparison are of one type"),
    };
    match op {
        "==" => order.is_eq(),
        "!=" => order.is_ne(),
        "<" => order.is_lt(),
        "<=" => order.is_le(),
        ">" => order.is_gt(),
        _ => order.is_ge(),
    }
}

/// A value of the kind, drawn from few, so that two are often equal.
fn value(g: &mut Gen, kind: usize) -> Value {
    match kind {
        0 => Value::Whole(g.below(7) as i64 - 3),
        1 => Value::Text(g.pick(&["", "a", "ab", "b"]).to_string()),
        _ => Value::Truth(g.one_in(2)),
    }
}

/// How a side is written: as a literal, unless `literal` is false, as a
/// name bound before the check, or as a call that gives it.
fn side(g: &mut Gen, v: &Value, name: &str, literal_ok: bool, lines: &mut Vec<String>) -> String {
    let literal = match v {
        Value::Whole(n) => n.to_string(),
        Value::Text(t) => format!("\"{t}\""),
        Value::Truth(b) => b.to_string(),
    };
    match g.below(3) {
        0 if literal_ok && !matches!(v, Value::Truth(_)) => literal,
        1 => {
            lines.push(format!("{name} = {}({literal})", v.same()));
            name.to_string()
        }
        _ => format!("{}({literal})", v.same()),
    }
}

fn a_check(g: &mut Gen, k: usize) -> Check {
    let mut lines = Vec::new();
    if g.one_in(3) {
        lines.push("try expect { true }".to_string());
    }
    if g.one_in(5) {
        // A check that is no comparison is shown as written, `not` included.
        let a = Value::Whole(g.below(3) as i64);
        let b = Value::Whole(g.below(3) as i64);
        let (written, value) = if g.one_in(2) {
            let truth = g.one_in(2);
            (format!("same_truth({truth})"), truth)
        } else {
            let (x, y) = (a.repr(), b.repr());
            (format!("not (same_whole({x}) == {y})"), !holds("==", &a, &b))
        };
        let fails = (!value).then_some(None);
        return Check {
            lines,
            written,
            fails,
        };
    }
    let kind = g.below(3);
    let (a, b) = (value(g, kind), value(g, kind));
    // Only whole numbers are ordered here.
    let op = if kind == 0 {
        *g.pick(&OPS)
    } else {
        *g.pick(&OPS[..2])
    };
    // Two literals are refused as a comparison whose answer is known.
    let left = side(g, &a, &format!("left{k}"), true, &mut lines);
    let literal = !left.ends_with(')') && !left.starts_with("left");
    let right = side(g, &b, &format!("right{k}"), !literal, &mut lines);
    let fails = (!holds(op, &a, &b)).then(|| Some((a.repr(), b.repr())));
    Check {
        lines,
        written: format!("{left} {op} {right}"),
        fails,
    }
}

/// A program of a few checks, each in a function of its own that lists no
/// effect, and what it prints: for each, `held`, or what its failure says.
#[derive(Debug)]
struct Program {
    source: String,
    printed: String,
}

fn a_program(g: &mut Gen) -> Program {
    let checks: Vec<Check> = (0..1 + g.below(4)).map(|k| a_check(g, k)).collect();
    let mut source = String::from(
        "use std.bits (i64)\nuse std.console (console, print)\nuse std.test (Failed, expect)\n\n\
         fn same_whole(x: i64) -> i64 { x }\n\
         fn same_text(x: str) -> str { x }\n\
         fn same_truth(x: bool) -> bool { x }\n",
    );
    let mut printed = String::new();
    for (k, check) in checks.iter().enumerate() {
        source.push_str(&format!("\nfn check{k}() -> Result[(), Failed] {{\n"));
        for line in &check.lines {
            source.push_str(&format!("  {line}\n"));
        }
        let line = source.lines().count() + 1;
        source.push_str(&format!("  expect {{ {} }}\n}}\n", check.written));
        match &check.fails {
            None => printed.push_str("held\n"),
            Some(sides) => {
                printed.push_str(&format!("main.smith:{line}:3: expected {}\n", check.written));
                if let Some((left, right)) = sides {
                    printed.push_str(&format!("left: {left}\nright: {right}\n"));
                }
            }
        }
    }
    source.push_str(
        "\nfn show(r: Result[(), Failed]) <console, alloc> {\n  match r {\n    .ok(done) { print(\"held\") }\n    .err(f) {\n      print(\"{f.path}:{f.line}:{f.column}: expected {f.check}\")\n      match f.sides {\n        .some(s) {\n          print(\"left: {s.left}\")\n          print(\"right: {s.right}\")\n        }\n        .none { }\n      }\n    }\n  }\n}\n\nfn main() <console, alloc> {\n",
    );
    for k in 0..checks.len() {
        source.push_str(&format!("  show(check{k}())\n"));
    }
    source.push_str("}\n");
    Program { source, printed }
}

/// For every check of every type and comparison, a check that holds gives
/// `.ok(())`; a comparison that fails names the place of the `expect` and the
/// check as written, and shows each side as `repr` writes it, whether it is
/// a literal, a name, or a call that is worked out; and any other check that
/// is false is shown as written with no sides. No check function lists
/// `alloc`, so a check fails with nothing listed.
fn a_comparison_that_fails_shows_both_sides_as_repr_writes_them() {
    generate::check(1247, 40, a_program, |p| {
        let program = smith::lower(p.source.as_bytes())
            .unwrap_or_else(|e| panic!("{e:?}\n{}", p.source));
        let program = smith::scalar::passes(&program);
        let ran = smith::scalar::run(&program);
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout),
            p.printed,
            "{}",
            p.source
        );
    });
}

/// `expect` takes its check written in braces after it, as the code it
/// writes the failure from, so a check passed as a value is refused, at the
/// call, saying `expect`.
fn a_check_given_as_a_value_and_not_written_in_braces_is_refused_at_the_call() {
    let source = "use std.test (Failed, expect)\n\nfn check(holds: fn() -> bool) -> Result[(), Failed] {\n  expect(holds)\n}\n\nfn main() {\n}\n";
    let errors = smith::check(source.as_bytes()).expect_err(source);
    let line = source.lines().position(|l| l.contains("expect(")).unwrap() + 1;
    let col = source.lines().nth(line - 1).unwrap().find("expect").unwrap() + 1;
    assert!(
        errors.iter().any(|e| {
            e.pos.line == line && e.pos.col == col && e.message.contains("`expect`")
        }),
        "{errors:?}"
    );
}
