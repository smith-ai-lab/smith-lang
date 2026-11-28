//! A value at the top level of a file states its type, as every other
//! definition there does, and one that does not is refused at its name, with
//! the fix that writes the type its value has. One that states its type is
//! worked out when the program is built: a function that reads it gets what
//! the same expression written in its place gives, and a read asks the system
//! for no memory and calls nothing. Assigning to one, a second definition of
//! its name, one that needs an effect, and one that is defined by itself are
//! each refused at the name.

use smith::check;
use smith::diag::{Error, Pos};
use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::pos_after;
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "a_value_at_the_top_level_that_states_no_type_is_refused_at_its_name_with_the_fix_that_writes_its_type",
        a_value_at_the_top_level_that_states_no_type_is_refused_at_its_name_with_the_fix_that_writes_its_type,
    ),
    (
        "a_value_at_the_top_level_that_states_its_type_is_not_refused_for_stating_none",
        a_value_at_the_top_level_that_states_its_type_is_not_refused_for_stating_none,
    ),
    (
        "a_function_reads_a_value_at_the_top_level_that_another_value_and_a_function_make",
        a_function_reads_a_value_at_the_top_level_that_another_value_and_a_function_make,
    ),
    (
        "a_value_at_the_top_level_gives_what_its_expression_written_in_its_place_gives_and_no_call_reads_it",
        a_value_at_the_top_level_gives_what_its_expression_written_in_its_place_gives_and_no_call_reads_it,
    ),
    (
        "reading_a_list_at_the_top_level_in_every_round_of_a_loop_asks_for_no_memory",
        reading_a_list_at_the_top_level_in_every_round_of_a_loop_asks_for_no_memory,
    ),
    (
        "a_list_at_the_top_level_that_is_pushed_to_is_copied_and_stays_as_it_was",
        a_list_at_the_top_level_that_is_pushed_to_is_copied_and_stays_as_it_was,
    ),
    (
        "assigning_to_a_value_at_the_top_level_or_defining_its_name_again_is_one_error_at_the_name",
        assigning_to_a_value_at_the_top_level_or_defining_its_name_again_is_one_error_at_the_name,
    ),
    (
        "a_value_at_the_top_level_that_needs_an_effect_or_is_defined_by_itself_is_one_error_at_its_name",
        a_value_at_the_top_level_that_needs_an_effect_or_is_defined_by_itself_is_one_error_at_its_name,
    ),
];

/// The executables of the programs are checked against the evaluator.
pub const MACHINE: &[Test] = &[(
    "every_executable_reads_the_values_at_the_top_level_as_the_evaluator_does",
    every_executable_reads_the_values_at_the_top_level_as_the_evaluator_does,
)];

fn errors(source: &str) -> Vec<Error> {
    match check(source.as_bytes()) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

/// What the evaluator prints for the program.
fn printed(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    String::from_utf8(scalar::run(&program).stdout).unwrap()
}

/// What a top-level value may be given, the type it states, and the type the
/// fix names where it states none.
const VALUES: &[(&str, &str, &str)] = &[
    ("100", "u64", "100..101"),
    ("\"text\"", "str", "str"),
    ("Rect(w: 1, h: 1)", "Rect", "Rect"),
    ("[\"fn\", \"if\"]", "List[str]", "[2]str"),
    ("area(Rect(w: 1, h: 1))", "u64", "u64"),
];

/// A file with one top-level value, written with or without its type, and a
/// function that does not read it, and the place of its name.
fn file(name: &str, value: &str, ty: Option<&str>, above: usize) -> (String, Pos) {
    let mut out = String::from("use std.bits (u64)\n\nstruct Rect {\n  w: u64\n  h: u64\n}\n\n");
    out += "fn area(r: Rect) -> u64 { r.w }\n\n";
    for i in 0..above {
        out += &format!("fn above{i}() -> u64 {{ {i} }}\n\n");
    }
    let at = pos_after(&out);
    out += name;
    if let Some(ty) = ty {
        out += &format!(": {ty}");
    }
    out += &format!(" = {value}\n\nfn main() {{}}\n");
    (out, at)
}

/// A value at the top level that states no type is one error, at its name, which
/// says that a type is needed, names the value, and whose fix writes the type
/// its value has after the name, whatever the value is and wherever it is in
/// the file.
fn a_value_at_the_top_level_that_states_no_type_is_refused_at_its_name_with_the_fix_that_writes_its_type()
 {
    generate::check(
        7301,
        20,
        |g: &mut Gen| (g.below(VALUES.len()), g.below(3), g.below(2) == 0),
        |(value, above, long)| {
            let name = if *long { "unit_limit" } else { "limit" };
            let (source, at) = file(name, VALUES[*value].0, None, *above);
            let found = errors(&source);
            let refused: Vec<&Error> = found.iter().filter(|e| e.message.contains("states its type")).collect();
            assert_eq!(refused.len(), 1, "{found:?}\n{source}");
            let e = refused[0];
            assert_eq!(e.pos, at, "{e}\n{source}");
            assert!(!e.unbuilt, "{e}");
            assert!(e.message.contains(&format!("`{name}`")), "{e}");
            let fix = e.fix.as_deref().expect("a fix");
            assert!(fix.contains(&format!("{name}: {} = ", VALUES[*value].2)), "{fix}");
        },
    );
}

/// A value at the top level that states its type is not refused for stating none.
fn a_value_at_the_top_level_that_states_its_type_is_not_refused_for_stating_none() {
    generate::check(
        7302,
        20,
        |g: &mut Gen| (g.below(VALUES.len()), g.below(3)),
        |(value, above)| {
            let (source, _) = file("limit", VALUES[*value].0, Some(VALUES[*value].1), *above);
            let found = errors(&source);
            assert!(found.iter().all(|e| !e.message.contains("states its type")), "{found:?}\n{source}");
        },
    );
}

/// The program of the issue: a struct value, and a value a function makes of it.
fn a_function_reads_a_value_at_the_top_level_that_another_value_and_a_function_make() {
    let source = "use std.console (console, print)\n\nstruct Rect {\n  w: 0..100\n  h: 0..100\n}\n\nunit: Rect = Rect(w: 1, h: 1)\n\nfn area(r: Rect) -> Int { r.w * r.h }\n\nunit_area: Int = area(unit)\n\nfn main() <console, alloc> {\n  print(\"{unit_area} {unit.w}\")\n}\n";
    assert_eq!(printed(source), "1 1\n");
}

/// The kinds of value the generator makes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Number,
    Pair,
    Choice,
    Tuple,
    List,
}

const KINDS: &[Kind] = &[Kind::Number, Kind::Pair, Kind::Choice, Kind::Tuple, Kind::List];

/// Values at the top level, each of a kind, written as the file writes it and
/// as its expression is written in its place, with each value it reads
/// written out.
#[derive(Clone, Debug)]
struct Values {
    kinds: Vec<Kind>,
    written: Vec<String>,
    in_place: Vec<String>,
}

fn values(g: &mut Gen) -> Values {
    let mut v = Values {
        kinds: Vec::new(),
        written: Vec::new(),
        in_place: Vec::new(),
    };
    for _ in 0..1 + g.below(4) {
        let kind = *g.pick(KINDS);
        let (written, in_place) = expr(g, kind, 2, &v);
        v.kinds.push(kind);
        v.written.push(written);
        v.in_place.push(in_place);
    }
    v
}

/// An expression of the kind, as it is written with the values before it and
/// with them written out.
fn expr(g: &mut Gen, kind: Kind, depth: usize, v: &Values) -> (String, String) {
    let earlier: Vec<usize> = (0..v.kinds.len()).filter(|&k| v.kinds[k] == kind).collect();
    if !earlier.is_empty() && g.one_in(3) {
        let k = *g.pick(&earlier);
        return (format!("v{k}"), v.in_place[k].clone());
    }
    let part = |g: &mut Gen, kind: Kind| expr(g, kind, depth.saturating_sub(1), v);
    let call = |name: &str, parts: &[(String, String)]| {
        let (w, p): (Vec<&str>, Vec<&str>) = parts.iter().map(|(w, p)| (w.as_str(), p.as_str())).unzip();
        (format!("{name}({})", w.join(", ")), format!("{name}({})", p.join(", ")))
    };
    match kind {
        Kind::Number => match if depth == 0 { 0 } else { g.below(6) } {
            0 => {
                let n = g.below(1000).to_string();
                (n.clone(), n)
            }
            1 => {
                let (a, b) = (part(g, Kind::Number), part(g, Kind::Number));
                call("add", &[a, b])
            }
            2 => {
                let xs = part(g, Kind::List);
                call("sum", &[xs])
            }
            3 => {
                let p = part(g, Kind::Pair);
                call("first", &[p])
            }
            4 => {
                let c = part(g, Kind::Choice);
                call("pick", &[c])
            }
            _ => {
                let t = part(g, Kind::Tuple);
                call("left", &[t])
            }
        },
        Kind::Pair => {
            let (a, b) = (part(g, Kind::Number), part(g, Kind::Number));
            (format!("P(a: {}, b: {})", a.0, b.0), format!("P(a: {}, b: {})", a.1, b.1))
        }
        Kind::Choice => match g.below(2) {
            0 => (".none".to_string(), ".none".to_string()),
            _ => {
                let n = part(g, Kind::Number);
                (format!(".one({})", n.0), format!(".one({})", n.1))
            }
        },
        Kind::Tuple => {
            let (n, p) = (part(g, Kind::Number), part(g, Kind::Pair));
            (format!("({}, {})", n.0, p.0), format!("({}, {})", n.1, p.1))
        }
        Kind::List => {
            let items: Vec<(String, String)> = (0..g.below(4)).map(|_| part(g, Kind::Number)).collect();
            let (w, p): (Vec<String>, Vec<String>) = items.into_iter().unzip();
            (format!("[{}]", w.join(", ")), format!("[{}]", p.join(", ")))
        }
    }
}

const HEAD: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\n\nstruct P {\n  a: u64\n  b: u64\n}\n\nenum Choice {\n  none\n  one: u64\n}\n\nfn add(x: u64, y: u64) -> u64 { wrapping_add(x, y) }\n\nfn sum(xs: List[u64]) -> u64 {\n  mut total: u64 = 0\n  loop x in xs {\n    total = wrapping_add(total, x)\n  }\n  total\n}\n\nfn first(p: P) -> u64 { p.a }\n\nfn pick(c: Choice) -> u64 {\n  match c {\n    .none { 0 }\n    .one(n) { n }\n  }\n}\n\nfn left(t: (u64, P)) -> u64 {\n  (n, p) = t\n  wrapping_add(n, p.b)\n}\n\nfn show_pair(p: P) -> str <alloc> { \"{p.a}/{p.b}\" }\n\nfn show_choice(c: Choice) -> str <alloc> {\n  match c {\n    .none { \"none\" }\n    .one(n) { \"one {n}\" }\n  }\n}\n\nfn show_tuple(t: (u64, P)) -> str <alloc> {\n  (n, p) = t\n  \"{n} {show_pair(p)}\"\n}\n\nfn show_list(xs: List[u64]) -> str <alloc> { \"{xs.size} {sum(xs)}\" }\n\n";

/// The type a value of the kind states, and the function that shows one.
fn typed(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Number => ("u64", ""),
        Kind::Pair => ("P", "show_pair"),
        Kind::Choice => ("Choice", "show_choice"),
        Kind::Tuple => ("(u64, P)", "show_tuple"),
        Kind::List => ("List[u64]", "show_list"),
    }
}

/// The program that defines the values at the top level and prints each, and
/// the one that prints each expression written in its place.
fn programs(v: &Values) -> (String, String) {
    let mut top = String::from(HEAD);
    for (k, (kind, written)) in v.kinds.iter().zip(&v.written).enumerate() {
        top += &format!("v{k}: {} = {written}\n\n", typed(*kind).0);
    }
    let mut in_place = String::from(HEAD);
    let main = |values: &[String]| {
        let mut out = String::from("fn main() <console, alloc> {\n");
        for (kind, value) in v.kinds.iter().zip(values) {
            out += &format!("  print(\"{{{}({value})}}\")\n", typed(*kind).1);
        }
        out + "}\n"
    };
    let names: Vec<String> = (0..v.kinds.len()).map(|k| format!("v{k}")).collect();
    top += &main(&names);
    in_place += &main(&v.in_place);
    (top, in_place)
}

/// For every value the generator builds from literals, structs, enums, tuples,
/// lists, other values, and calls of functions that carry no effect, reading it
/// prints what its expression written in its place prints, and the program
/// holds no function for it, so no call reads it.
fn a_value_at_the_top_level_gives_what_its_expression_written_in_its_place_gives_and_no_call_reads_it()
 {
    generate::check(7303, 24, values, |v| {
        let (top, in_place) = programs(v);
        assert_eq!(printed(&top), printed(&in_place), "{top}");
        let program = lower(top.as_bytes()).unwrap();
        for k in 0..v.kinds.len() {
            let name = format!(".v{k}");
            assert!(
                program.functions.iter().all(|f| !f.name.ends_with(&name)),
                "a function gives `v{k}`\n{top}"
            );
        }
    });
}

fn every_executable_reads_the_values_at_the_top_level_as_the_evaluator_does() {
    generate::check(7304, 2, values, |v| {
        let (top, _) = programs(v);
        let program = lower(top.as_bytes()).unwrap();
        let ran = scalar::run(&program);
        for target in targets::runnable() {
            targets::runs_as(top.as_bytes(), target, &ran);
        }
    });
}

/// A program that reads a list of 1,000 numbers at the top level in each of
/// 1,000 rounds of a loop asks the system for no memory and the handler of
/// `alloc` for none.
fn reading_a_list_at_the_top_level_in_every_round_of_a_loop_asks_for_no_memory() {
    let numbers: Vec<String> = (0..1000).map(|n| n.to_string()).collect();
    let source = format!(
        "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\n\ntable: List[u64] = [{}]\n\nfn main() <console> {{\n  mut acc: u64 = 0\n  loop i in 0..1000 {{\n    if i < table.size {{\n      acc = wrapping_add(acc, table[i])\n    }}\n  }}\n  if acc == 499500 {{\n    print(\"read\")\n  }} else {{\n    print(\"wrong\")\n  }}\n}}\n",
        numbers.join(", ")
    );
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let (outcome, requests) = scalar::run_counting(&program, usize::MAX);
    let (_, taken) = scalar::run_taking(&program, usize::MAX);
    assert_eq!(String::from_utf8(outcome.stdout).unwrap(), "read\n");
    assert!(requests.is_empty(), "{requests:?}");
    assert!(taken.is_empty(), "{taken:?}");
}

/// A list read from the top level and pushed to is copied first, so the value
/// is what it was the next time it is read.
fn a_list_at_the_top_level_that_is_pushed_to_is_copied_and_stays_as_it_was() {
    let source = "use std.bits (u64)\nuse std.console (console, print)\nuse std.list (push)\n\ntable: List[u64] = [1, 2, 3]\n\nfn grown() -> List[u64] <alloc> {\n  mut xs = table\n  push(mut xs, 4)\n  xs\n}\n\nfn main() <console, alloc> {\n  xs = grown()\n  print(\"{xs.size} {table.size}\")\n}\n";
    assert_eq!(printed(source), "4 3\n");
}

/// The ways a file assigns to a value or defines its name again: the text
/// before the name, and the text after it, in the body of `main` or at the top
/// level, and whether it assigns.
const AGAIN: &[(&str, &str, bool, bool)] = &[
    ("  ", " = Rect(w: 2, h: 2)\n", true, true),
    ("  ", ".w = 2\n", true, true),
    ("  mut ", " = Rect(w: 2, h: 2)\n", true, false),
    ("  ", ": Rect = Rect(w: 2, h: 2)\n", true, false),
    ("  loop ", " in 0..3 {}\n", true, false),
    ("  match o {\n    .some(", ") {}\n    .none {}\n  }\n", true, false),
    ("fn ", "() -> u64 { 1 }\n\n", false, false),
    ("", ": u64 = 2\n\n", false, false),
];

/// Assigning to a value, and a second definition of its name in a function, as
/// a function, or as a value, is one error at the second name, which names it.
fn assigning_to_a_value_at_the_top_level_or_defining_its_name_again_is_one_error_at_the_name() {
    generate::check(
        7305,
        24,
        |g: &mut Gen| (g.below(AGAIN.len()), g.below(2) == 0),
        |(form, long)| {
            let name = if *long { "unit_limit" } else { "limit" };
            let (before, after, in_main, assigns) = AGAIN[*form];
            let mut source = format!(
                "use std.bits (u64)\nuse std.console (console, print)\n\nstruct Rect {{\n  w: 0..9\n  h: 0..9\n}}\n\n{name}: Rect = Rect(w: 1, h: 1)\n\n"
            );
            let mut at = Pos::START;
            if !in_main {
                source += before;
                at = pos_after(&source);
                source += name;
                source += after;
            }
            source += "fn main() <console, alloc> {\n  o: Option[Rect] = .none\n";
            if in_main {
                source += before;
                at = pos_after(&source);
                source += name;
                source += after;
            }
            source += &format!("  print(\"{{{name}.w}}\")\n}}\n");
            let found = errors(&source);
            let here: Vec<&Error> = found.iter().filter(|e| e.pos == at).collect();
            assert_eq!(here.len(), 1, "{found:?}\n{source}");
            assert!(here[0].message.contains(&format!("`{name}`")), "{}", here[0]);
            let said = if assigns { "assigned" } else { "already" };
            assert!(here[0].message.contains(said), "{}\n{source}", here[0]);
            assert!(found.iter().all(|e| !e.message.contains('@')), "{found:?}");
        },
    );
}

/// The ways a value needs an effect or is defined by itself: the items of the
/// file, written with `NAME` for the value's name, and the names of the values
/// it makes wrong.
const WRONG: &[(&str, &[&str])] = &[
    (
        "fn loud() -> u64 <console> {\n  print(\"x\")\n  1\n}\n\nNAME: u64 = loud()\n",
        &["NAME"],
    ),
    ("fn forever() -> u64 <diverge> { 1 }\n\nNAME: u64 = forever()\n", &["NAME"]),
    ("fn made() -> List[u64] <alloc> { [1] }\n\nNAME: u64 = made().size\n", &["NAME"]),
    ("NAME: u64 = add(NAME, 1)\n", &["NAME"]),
    ("NAME: u64 = other\n\nother: u64 = NAME\n", &["NAME", "other"]),
    ("NAME: u64 = twice()\n\nfn twice() -> u64 { add(NAME, NAME) }\n", &["NAME"]),
];

/// A value that prints, calls a function that carries `diverge` or another
/// effect, or is defined by itself, directly or through another value or a
/// function, is one error at its name, which says that a value at the top level
/// is worked out when the program is built.
fn a_value_at_the_top_level_that_needs_an_effect_or_is_defined_by_itself_is_one_error_at_its_name()
 {
    generate::check(
        7306,
        18,
        |g: &mut Gen| (g.below(WRONG.len()), g.below(2) == 0),
        |(form, long)| {
            let name = if *long { "unit_limit" } else { "limit" };
            let (items, wrong) = WRONG[*form];
            let head = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add)\n\nfn add(x: u64, y: u64) -> u64 { wrapping_add(x, y) }\n\n";
            let source = format!(
                "{head}{}\nfn main() <console, alloc> {{\n  print(\"{{{name}}}\")\n}}\n",
                items.replace("NAME", name)
            );
            let found = errors(&source);
            for value in wrong {
                let value = value.replace("NAME", name);
                let line = source
                    .lines()
                    .position(|l| l.starts_with(&format!("{value}: ")))
                    .expect("the value")
                    + 1;
                let at = Pos { line, col: 1 };
                let here: Vec<&Error> = found.iter().filter(|e| e.pos == at).collect();
                assert_eq!(here.len(), 1, "{found:?}\n{source}");
                assert!(
                    here[0].message.contains("worked out when the program is built"),
                    "{}\n{source}",
                    here[0]
                );
            }
            assert!(found.iter().all(|e| !e.message.contains('@')), "{found:?}");
        },
    );
}
