//! A struct whose field has a statement as its type holds a proof of it. Making
//! the struct owes the proof, every holder of the struct has the fact, and the
//! field is in no executable.
//!
//! What a construction owes is what a call owes, so the program that makes the
//! struct is judged against the program that states the statement in a
//! `requires`, and the executable against the program with the proof field
//! taken out by hand.

use smith::{check, compile, lower, scalar};

use crate::Test;
use crate::arith::condition;
use crate::generate::{self, Gen};
use crate::numbers::{INTS, Int, with_header};
use crate::proof_parameters::{Owing, owing};
use crate::targets::any;

pub const ONCE: &[Test] = &[
    (
        "a_construction_compiles_exactly_when_arith_proves_the_statement_of_the_values_it_gives_part_1_of_4",
        || a_construction_compiles_exactly_when_arith_proves_the_statement_of_the_values_it_gives(0),
    ),
    (
        "a_construction_compiles_exactly_when_arith_proves_the_statement_of_the_values_it_gives_part_2_of_4",
        || a_construction_compiles_exactly_when_arith_proves_the_statement_of_the_values_it_gives(1),
    ),
    (
        "a_construction_compiles_exactly_when_arith_proves_the_statement_of_the_values_it_gives_part_3_of_4",
        || a_construction_compiles_exactly_when_arith_proves_the_statement_of_the_values_it_gives(2),
    ),
    (
        "a_construction_compiles_exactly_when_arith_proves_the_statement_of_the_values_it_gives_part_4_of_4",
        || a_construction_compiles_exactly_when_arith_proves_the_statement_of_the_values_it_gives(3),
    ),
    (
        "a_construction_that_gives_a_proof_compiles_exactly_when_it_is_of_the_statement_for_those_values_part_1_of_3",
        || a_construction_that_gives_a_proof_compiles_exactly_when_it_is_of_the_statement_for_those_values(0),
    ),
    (
        "a_construction_that_gives_a_proof_compiles_exactly_when_it_is_of_the_statement_for_those_values_part_2_of_3",
        || a_construction_that_gives_a_proof_compiles_exactly_when_it_is_of_the_statement_for_those_values(1),
    ),
    (
        "a_construction_that_gives_a_proof_compiles_exactly_when_it_is_of_the_statement_for_those_values_part_3_of_3",
        || a_construction_that_gives_a_proof_compiles_exactly_when_it_is_of_the_statement_for_those_values(2),
    ),
    (
        "a_value_of_the_struct_has_the_statement_of_its_fields_wherever_it_is_held",
        a_value_of_the_struct_has_the_statement_of_its_fields_wherever_it_is_held,
    ),
    (
        "the_executable_of_a_struct_with_proof_fields_is_that_of_the_struct_without_them_part_1_of_4",
        || the_executable_of_a_struct_with_proof_fields_is_that_of_the_struct_without_them(0),
    ),
    (
        "the_executable_of_a_struct_with_proof_fields_is_that_of_the_struct_without_them_part_2_of_4",
        || the_executable_of_a_struct_with_proof_fields_is_that_of_the_struct_without_them(1),
    ),
    (
        "the_executable_of_a_struct_with_proof_fields_is_that_of_the_struct_without_them_part_3_of_4",
        || the_executable_of_a_struct_with_proof_fields_is_that_of_the_struct_without_them(2),
    ),
    (
        "the_executable_of_a_struct_with_proof_fields_is_that_of_the_struct_without_them_part_4_of_4",
        || the_executable_of_a_struct_with_proof_fields_is_that_of_the_struct_without_them(3),
    ),
    (
        "a_field_that_a_proof_field_names_is_refused_an_assignment_and_another_is_not",
        a_field_that_a_proof_field_names_is_refused_an_assignment_and_another_is_not,
    ),
    (
        "a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read_part_1_of_6",
        || a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read(0),
    ),
    (
        "a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read_part_2_of_6",
        || a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read(1),
    ),
    (
        "a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read_part_3_of_6",
        || a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read(2),
    ),
    (
        "a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read_part_4_of_6",
        || a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read(3),
    ),
    (
        "a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read_part_5_of_6",
        || a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read(4),
    ),
    (
        "a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read_part_6_of_6",
        || a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read(5),
    ),
    (
        "a_value_that_was_read_has_the_fact_and_a_value_that_breaks_it_is_refused_with_its_path",
        a_value_that_was_read_has_the_fact_and_a_value_that_breaks_it_is_refused_with_its_path,
    ),
    (
        "a_struct_with_a_proof_field_is_written_as_the_data_it_holds",
        a_struct_with_a_proof_field_is_written_as_the_data_it_holds,
    ),
    (
        "eq_ord_fmt_and_repr_leave_a_proof_field_out",
        eq_ord_fmt_and_repr_leave_a_proof_field_out,
    ),
    (
        "a_range_written_as_a_value_is_made_exactly_where_its_ends_are_proven_in_order",
        a_range_written_as_a_value_is_made_exactly_where_its_ends_are_proven_in_order,
    ),
    (
        "the_cursor_and_step_of_the_page_compile_and_a_step_outside_the_if_is_refused_with_its_goal",
        the_cursor_and_step_of_the_page_compile_and_a_step_outside_the_if_is_refused_with_its_goal,
    ),
];

fn accepted(source: &str) -> bool {
    check(source.as_bytes()).is_ok()
}

fn lowered(source: &str) -> scalar::Program {
    lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"))
}

fn output(source: &str) -> String {
    let program = lowered(source);
    String::from_utf8(scalar::run(&program).stdout).expect("text")
}

/// What a refused program must name.
fn refused(source: &str, names: &str) {
    let errors = check(source.as_bytes()).expect_err(source);
    assert!(
        errors.iter().any(|e| e.message.contains(names)),
        "{errors:?} does not name `{names}` for\n{source}"
    );
}

const MAIN: &str = "fn main() <console, alloc> {\n  print(\"{f(1, 2)}\")\n}\n";

/// A struct that holds `x`, `y`, and the proof of the statement over them,
/// and the one with the proof taken out by hand.
fn pair(t: &str, owed: &str) -> (String, String) {
    (
        format!("struct P {{\n  x: {t}\n  y: {t}\n  ok: {owed}\n}}\n\n"),
        format!("struct P {{\n  x: {t}\n  y: {t}\n}}\n\n"),
    )
}

/// For every statement and every test a caller makes, a construction that
/// leaves the proof field out compiles exactly when the function that states
/// the statement in a `requires` is called by the same test, and the program is
/// the one with the proof field taken out by hand.
fn a_construction_compiles_exactly_when_arith_proves_the_statement_of_the_values_it_gives(part: u64) {
    generate::check_part(5601, 80, part, 4, owing, |o: &Owing| {
        let (t, owed, tested) = (o.ty, &o.owed, &o.tested);
        let (held, bare) = pair(t, owed);
        let body = |make: &str| {
            format!("fn f(x: {t}, y: {t}) -> {t} {{\n  if {tested} {{ {make} }} else {{ 0 }}\n}}\n\n{MAIN}")
        };
        let with = with_header(format!("{held}{}", body("P(x: y, y: x).x")));
        let by_hand = with_header(format!("{bare}{}", body("P(x: y, y: x).x")));
        let requires = with_header(format!(
            "fn need(x: {t}, y: {t}) -> {t}\n  requires {owed}\n{{ x }}\n\n{}",
            body("need(y, x)")
        ));
        let ok = accepted(&requires);
        // A program is accepted exactly when it is lowered, so it is checked once.
        let made = lower(with.as_bytes());
        assert_eq!(made.is_ok(), ok, "for\n{with}\nand\n{requires}");
        if o.proves {
            assert!(ok, "for\n{requires}");
        }
        if ok {
            let made = made.unwrap_or_else(|e| panic!("{e:?} for\n{with}"));
            assert_eq!(made, lowered(&by_hand), "for\n{with}\nand\n{by_hand}");
        }
    });
}

/// A proof written for the field is accepted exactly when it is of the
/// statement for the values given, and not for what else is known.
fn a_construction_that_gives_a_proof_compiles_exactly_when_it_is_of_the_statement_for_those_values(part: u64) {
    generate::check_part(5602, 80, part, 3,owing, |o: &Owing| {
        let (t, owed, tested) = (o.ty, &o.owed, &o.tested);
        let (held, _) = pair(t, owed);
        let given = with_header(format!(
            "{held}fn f(x: {t}, y: {t}) -> {t} {{\n  match check({tested}) {{\n    .some(q) {{ P(x: y, y: x, ok: q).x }}\n    .none {{ 0 }}\n  }}\n}}\n\n{MAIN}"
        ));
        let requires = with_header(format!(
            "fn need(x: {t}, y: {t}) -> {t}\n  requires {owed}\n{{ x }}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  if {tested} {{ need(y, x) }} else {{ 0 }}\n}}\n\n{MAIN}"
        ));
        assert_eq!(accepted(&given), accepted(&requires), "for\n{given}\nand\n{requires}");
    });
}

/// Where a value of the struct is held: the function that reads `c` out of it
/// and makes the value again from the swapped fields, and the call of that
/// function in `main` once `c` is a value of the struct.
fn holders(t: &str) -> Vec<(&'static str, String, &'static str, &'static str)> {
    vec![
        (
            "a parameter",
            format!("fn f(c: P) -> {t} {{ P(x: c.y, y: c.x).x }}"),
            "",
            "f(c)",
        ),
        (
            "a field",
            format!(
                "struct W {{\n  c: P\n  n: {t}\n}}\n\nfn f(w: W) -> {t} {{ P(x: w.c.y, y: w.c.x).x }}"
            ),
            "",
            "f(W(c: c, n: 0))",
        ),
        (
            "a list",
            format!(
                "fn f(xs: List[P]) -> {t}\n  requires xs.size > 0\n{{ P(x: xs[0].y, y: xs[0].x).x }}"
            ),
            "      xs: List[P] = [c]\n",
            "f(xs)",
        ),
        (
            "a mut",
            format!("fn f(c: P) -> {t} {{\n  mut d = c\n  P(x: d.y, y: d.x).x\n}}"),
            "",
            "f(c)",
        ),
    ]
}

/// For every statement and every place a value of the struct is held, the
/// value is known to satisfy the statement with no line written: building the
/// value again from its own fields is accepted, and building it with the
/// fields swapped is accepted exactly when the statement implies the swapped
/// one, as it is for a function that states the statement in a `requires`.
fn a_value_of_the_struct_has_the_statement_of_its_fields_wherever_it_is_held() {
    generate::check(5603, 12, owing, |o: &Owing| {
        let (t, owed) = (o.ty, &o.owed);
        let (held, _) = pair(t, owed);
        let oracle = with_header(format!(
            "fn need(x: {t}, y: {t}) -> {t}\n  requires {owed}\n{{ x }}\n\nfn f(x: {t}, y: {t}) -> {t}\n  requires {owed}\n{{ need(y, x) }}\n\nfn g(x: {t}, y: {t}) -> {t} {{\n  if {owed} {{ f(x, y) }} else {{ 0 }}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{g(1, 2)}}\")\n}}\n"
        ));
        let implied = accepted(&oracle);
        for (place, function, setup, call) in holders(t) {
            // A value of the struct is only made where its proof is built.
            let main = format!(
                "fn main() <console, alloc> {{\n  match check({owed_at}) {{\n    .some(q) {{\n      c = P(x: 1, y: 2, ok: q)\n{setup}      print(\"{{{call}}}\")\n    }}\n    .none {{ print(\"none\") }}\n  }}\n}}\n",
                owed_at = at(owed, "1", "2"),
            );
            let program = |function: &str| with_header(format!("{held}{function}\n\n{main}"));
            let again = program(&function.replace("(x: c.y, y: c.x)", "(x: c.x, y: c.y)")
                .replace("(x: w.c.y, y: w.c.x)", "(x: w.c.x, y: w.c.y)")
                .replace("(x: xs[0].y, y: xs[0].x)", "(x: xs[0].x, y: xs[0].y)")
                .replace("(x: d.y, y: d.x)", "(x: d.x, y: d.y)"));
            assert!(accepted(&again), "{place}: {:?} for\n{again}", check(again.as_bytes()));
            let swapped = program(&function);
            assert_eq!(accepted(&swapped), implied, "{place}: for\n{swapped}\nand\n{oracle}");
        }
    });
}

/// The statement with `x` and `y` taken as the numbers `a` and `b`.
fn at(statement: &str, a: &str, b: &str) -> String {
    statement
        .chars()
        .map(|c| match c {
            'x' => format!("({a})"),
            'y' => format!("({b})"),
            c => c.to_string(),
        })
        .collect()
}

/// The types a data field may have, which a struct of them is laid out from.
fn field_type(g: &mut Gen) -> Int {
    *g.pick(INTS)
}

/// For every struct with proof fields among its fields, its size, its layout,
/// and every executable are those of the struct with the proof fields taken
/// out by hand, with the traits it is given and with a list, a function, and a
/// struct that holds it.
fn the_executable_of_a_struct_with_proof_fields_is_that_of_the_struct_without_them(part: u64) {
    generate::check_part(
        5604,
        8,
        part,
        4,
        |g: &mut Gen| (field_type(g), field_type(g), g.below(100) as u32, g.below(100) as u32),
        |&(a, b, lo, hi)| {
            let (lo, hi) = (lo.min(hi), lo.max(hi));
            let owed = ["lo <= hi", "lo <= hi and hi >= lo", "not (hi < lo)"];
            for statement in owed {
                let program = |proofs: bool| {
                    let fields = if proofs {
                        format!("  lo: {}\n  hi: {}\n  ok: {statement}\n  more: {}\n  fine: hi >= lo\n", a.name, a.name, b.name)
                    } else {
                        format!("  lo: {}\n  hi: {}\n  more: {}\n", a.name, a.name, b.name)
                    };
                    with_header(format!(
                        "struct Span {{\n{fields}}}\n\nstruct Holder {{\n  span: Span\n  tag: {}\n}}\n\nfn widen(s: Span) -> Span {{\n  Span(lo: s.lo, hi: s.hi, more: s.more)\n}}\n\nfn main() <console, alloc> {{\n  s = Span(lo: {lo}, hi: {hi}, more: 3)\n  mut xs: List[Span] = []\n  push(mut xs, s)\n  push(mut xs, widen(s))\n  h = Holder(span: s, tag: 1)\n  print(\"{{xs.size}} {{h.span.hi}} {{s == widen(s)}} {{s < widen(s)}}\")\n  print(\"{{s}}\")\n  print(\"{{repr(h)}}\")\n}}\n",
                        b.name
                    ))
                };
                let (with, without) = (program(true), program(false));
                let a = compile(with.as_bytes(), any()).unwrap_or_else(|e| panic!("{e:?}\n{with}"));
                let b = compile(without.as_bytes(), any()).unwrap_or_else(|e| panic!("{e:?}\n{without}"));
                assert!(a == b, "the executables differ for\n{with}\nand\n{without}");
            }
        },
    );
}

/// For every field that a proof field's statement names, assigning it is
/// refused, saying the proof; one that nothing names is assigned as before.
fn a_field_that_a_proof_field_names_is_refused_an_assignment_and_another_is_not() {
    generate::check(5605, 20, |g: &mut Gen| (*g.pick(INTS), g.below(2)), |&(t, which)| {
        let named = ["x", "y"][which];
        let source = |field: &str| {
            with_header(format!(
                "struct P {{\n  x: {t}\n  y: {t}\n  ok: x <= y\n  z: {t}\n}}\n\nfn f(c: P) -> P {{\n  mut d = c\n  d.{field} = 1\n  d\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{f(P(x: 0, y: 0, z: 0)).z}}\")\n}}\n",
                t = t.name
            ))
        };
        refused(&source(named), &format!("`{named}` cannot be assigned, as `ok` is a proof about it"));
        let errors = check(source(named).as_bytes()).expect_err("an assignment of a named field");
        let whole = format!("build the value again, as `P({named}: e, {other}: d.{other}, z: d.z)`", other = ["x", "y"][1 - which]);
        let whole = if which == 0 { whole } else { "build the value again, as `P(x: d.x, y: e, z: d.z)`".to_string() };
        assert!(errors.iter().any(|e| e.fix.as_deref() == Some(whole.as_str())), "{errors:?}");
        // The type of another field that names a field is as good as a statement.
        let sized = with_header(format!(
            "struct Row {{\n  n: {t}\n  cells: [n]f32\n}}\n\nfn main() {{\n  mut r = Row(n: 3, cells: [1.0, 2.0, 3.0])\n  r.n = 9\n}}\n",
            t = t.name
        ));
        refused(&sized, "`n` cannot be assigned, as the type of `cells` names it");
        let free = source("z");
        assert!(accepted(&free), "{:?} for\n{free}", check(free.as_bytes()));
    });
}

const READ_HEAD: &str = "use std.console (console, print)\nuse std.deserialize (ReadError)\nuse std.json (FromJson, from_json, describe, to_json)\n\n";

/// For every statement over two numbers and every two numbers, the struct that
/// holds the statement is read from JSON exactly when the statement holds of
/// them, and what is read is the data; where it does not hold, the error names
/// the struct, the proof field, and the statement.
fn a_struct_with_a_proof_field_is_read_exactly_when_the_statement_holds_of_what_is_read(part: u64) {
    generate::check_part(
        5606, 6, part, 6,
        |g: &mut Gen| {
            let (text, holds) = condition(g, 1, false);
            let (x, y) = (g.below(21) as i128 - 10, g.below(21) as i128 - 10);
            let says = holds(x, y);
            (text, x, y, says)
        },
        |(text, x, y, says)| {
            let source = format!(
                "{READ_HEAD}struct P {{\n  x: i32\n  y: i32\n  ok: {text}\n}}\n\nfn parse(text: str) -> Result[P, ReadError] <alloc> {{\n  from_json(text)\n}}\n\nfn main() <console, alloc> {{\n  match parse(\"\\{{\\\"x\\\": {x}, \\\"y\\\": {y}\\}}\") {{\n    .ok(p) {{ print(\"read {{p.x}} {{p.y}}\") }}\n    .err(e) {{ print(describe(e)) }}\n  }}\n}}\n"
            );
            let source = with_header(source);
            let said = output(&source);
            if *says {
                assert_eq!(said, format!("read {x} {y}\n"), "for\n{source}");
            } else {
                assert!(
                    said.contains("`ok` of `P` does not hold: `") && !said.starts_with("read"),
                    "{said} for\n{source}"
                );
            }
        },
    );
}

/// What is read has the statement as a fact, so the struct is built again from
/// it with nothing to prove, and a value that breaks the statement is refused
/// with the path of the value and the statement as it is written.
fn a_value_that_was_read_has_the_fact_and_a_value_that_breaks_it_is_refused_with_its_path() {
    let source = with_header(format!(
        "{READ_HEAD}struct Cursor {{\n  at: Nat\n  inside: at <= 10\n}}\n\nstruct Doc {{\n  cursors: List[Cursor]\n}}\n\nfn parse(text: str) -> Result[Doc, ReadError] <alloc> {{\n  from_json(text)\n}}\n\nfn again(c: Cursor) -> Nat {{ Cursor(at: c.at).at }}\n\nfn main() <console, alloc> {{\n  match parse(\"\\{{\\\"cursors\\\": [\\{{\\\"at\\\": 3\\}}, \\{{\\\"at\\\": 11\\}}]\\}}\") {{\n    .ok(d) {{ print(\"{{d.cursors.size}}\") }}\n    .err(e) {{ print(describe(e)) }}\n  }}\n  match parse(\"\\{{\\\"cursors\\\": [\\{{\\\"at\\\": 3\\}}]\\}}\") {{\n    .ok(d) {{ print(\"{{d.cursors.size}}\") }}\n    .err(e) {{ print(describe(e)) }}\n  }}\n}}\n"
    ));
    let said = output(&source);
    let lines: Vec<&str> = said.lines().collect();
    assert!(
        lines[0].contains("cursors[1]") && lines[0].contains("`inside` of `Cursor` does not hold: `at <= 10`"),
        "{said} for\n{source}"
    );
    assert_eq!(lines[1], "1", "{said} for\n{source}");
    let usable = with_header(format!(
        "{READ_HEAD}struct Cursor {{\n  at: Nat\n  inside: at <= 10\n}}\n\nfn parse(text: str) -> Result[Cursor, ReadError] <alloc> {{\n  from_json(text)\n}}\n\nfn main() <console, alloc> {{\n  match parse(\"\\{{\\\"at\\\": 4\\}}\") {{\n    .ok(c) {{ print(\"{{Cursor(at: c.at).at}}\") }}\n    .err(e) {{ print(describe(e)) }}\n  }}\n}}\n"
    ));
    assert_eq!(output(&usable), "4\n", "for\n{usable}");
}

/// A struct with a proof field is written as the data it holds.
fn a_struct_with_a_proof_field_is_written_as_the_data_it_holds() {
    let written = with_header(format!(
        "{READ_HEAD}struct Span {{\n  lo: Nat\n  hi: Nat\n  ok: lo <= hi\n}}\n\nfn make(lo: Nat, hi: Nat) -> Option[Span] {{\n  match check(lo <= hi) {{\n    .some(q) {{ .some(Span(lo: lo, hi: hi, ok: q)) }}\n    .none {{ .none }}\n  }}\n}}\n\nfn main() <console, alloc> {{\n  match make(1, 2) {{\n    .some(s) {{ print(to_json(s)) }}\n    .none {{ print(\"none\") }}\n  }}\n}}\n"
    ));
    assert_eq!(output(&written), "{\"lo\":1,\"hi\":2}\n", "for\n{written}");
}

/// `Cursor(at: 4) == Cursor(at: 4)`, the order of two, how one is shown, and
/// how one is written leave the proof field out, for every value.
fn eq_ord_fmt_and_repr_leave_a_proof_field_out() {
    generate::check(5607, 30, |g: &mut Gen| (g.below(50), g.below(50)), |&(a, b)| {
        let source = with_header(format!(
            "struct Cursor[size: Nat] {{\n  at: Nat\n  inside: at <= size\n}}\n\nfn main() <console, alloc> {{\n  a = Cursor[100](at: {a})\n  b = Cursor[100](at: {b})\n  print(\"{{a == b}} {{a < b}} {{a > b}}\")\n  print(\"{{a}}\")\n  print(\"{{repr(a)}}\")\n}}\n"
        ));
        let expected = format!(
            "{} {} {}\nCursor(at: {a})\nCursor(at: {a})\n",
            a == b,
            a < b,
            a > b
        );
        assert_eq!(output(&source), expected, "for\n{source}");
    });
}

/// `a..b` written as a value is made exactly where `arith` proves `a <= b`,
/// whether the ends are numbers or names that a `requires` or an `if` orders.
fn a_range_written_as_a_value_is_made_exactly_where_its_ends_are_proven_in_order() {
    generate::check(5608, 30, |g: &mut Gen| (g.below(30), g.below(30)), |&(a, b)| {
        let numbers = with_header(format!(
            "fn main() <console, alloc> {{\n  r = {a}..{b}\n  print(\"{{r.start}}\")\n}}\n"
        ));
        assert_eq!(accepted(&numbers), a <= b, "for\n{numbers}");
        let names = |guard: &str, close: &str| {
            with_header(format!(
                "fn size(from: Nat, to: Nat) -> Nat {guard}{{\n  r = from..to\n  r.end - r.start\n}}{close}\n\nfn main() <console, alloc> {{\n  print(\"{{size({a}, {b})}}\")\n}}\n"
            ))
        };
        let open = names("", "");
        assert!(!accepted(&open), "for\n{open}");
        let stated = names("\n  requires from <= to\n", "");
        assert_eq!(accepted(&stated), a <= b, "for\n{stated}");
    });
}

const CURSOR: &str = "struct Cursor[size: Nat] {
  at: Nat
  inside: at <= size
}

fn first() -> Cursor[10] { Cursor(at: 4) }

fn step[size: Nat](c: Cursor[size]) -> Cursor[size] {
  if c.at < size { Cursor(at: c.at + 1) } else { c }
}

";

/// `Cursor` and `step` of the page compile and print what they should, and
/// `Cursor(at: c.at + 1)` outside the `if` is refused with the goal for the
/// values given.
fn the_cursor_and_step_of_the_page_compile_and_a_step_outside_the_if_is_refused_with_its_goal() {
    let program = with_header(format!(
        "{CURSOR}fn main() <console, alloc> {{\n  c = step(first())\n  print(\"{{c.at}}\")\n}}\n"
    ));
    assert_eq!(output(&program), "5\n");
    let outside = with_header(format!(
        "{CURSOR}fn past[size: Nat](c: Cursor[size]) -> Cursor[size] {{\n  Cursor(at: c.at + 1)\n}}\n\nfn main() <console, alloc> {{\n  c = past(first())\n  print(\"{{c.at}}\")\n}}\n"
    ));
    refused(&outside, "cannot prove `inside` of `Cursor`: `(c.at + 1) <= 10`");
    let beyond = with_header(format!(
        "{CURSOR}fn main() <console, alloc> {{\n  c: Cursor[10] = Cursor(at: 11)\n  print(\"{{c.at}}\")\n}}\n"
    ));
    refused(&beyond, "cannot prove `inside` of `Cursor`: `11 <= 10`");
}
