//! `x[i]` on a type of a program's own is `at` of the `Index` that the type
//! and the index choose, and compiles where its `valid` is proven. What each
//! index gives is checked against a model the test keeps beside the program,
//! in the evaluator and in every executable the machine can run.

use smith::diag::Pos;
use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::pos_after;
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "an_index_of_a_type_of_a_program_gives_what_its_at_gives",
        an_index_of_a_type_of_a_program_gives_what_its_at_gives,
    ),
    (
        "an_index_compiles_exactly_where_valid_is_proven_and_is_refused_at_it",
        an_index_compiles_exactly_where_valid_is_proven_and_is_refused_at_it,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_gives_what_at_gives_for_an_index_of_a_type_of_a_program",
    every_executable_gives_what_at_gives_for_an_index_of_a_type_of_a_program,
)];

/// The types the program indexes, and their implementations of `Index`: one
/// whose `valid` is `true` and whose `at` gives an `Option`, one whose `valid`
/// is a comparison of the index, and one for every key and value type.
const TYPES: &str = "use std.console (console, print)

struct Shelf {
  items: List[Byte]
}

impl Index[Shelf, Nat, Option[Byte]] {
  fn valid(c: Shelf, i: Nat) -> bool { true }
  fn at(c: Shelf, i: Nat) -> Option[Byte] {
    if i < c.items.size { .some(c.items[i]) } else { .none }
  }
}

struct Pair {
  a: Byte
  b: Byte
}

impl Index[Pair, Nat, Byte] {
  fn valid(c: Pair, i: Nat) -> bool { i < 2 }
  fn at(c: Pair, i: Nat) -> Byte {
    if i == 0 { c.a } else { c.b }
  }
}

struct Keyed[K, V] {
  keys: List[K]
  values: List[V]
}

impl[K: Eq, V: Copy] Index[Keyed[K, V], K, Option[V]] {
  fn valid(c: Keyed[K, V], i: K) -> bool { true }
  fn at(c: Keyed[K, V], i: K) -> Option[V] {
    loop j in 0..c.keys.size {
      if c.keys[j] == i and j < c.values.size { return .some(c.values[j]) }
    }
    .none
  }
}

struct Point {
  x: Byte
  y: Byte
}

fn shown(o: Option[Byte]) -> str <alloc> {
  match o {
    .some(v) { \"{v}\" }
    .none { \"-\" }
  }
}

fn y_of(o: Option[Point]) -> str <alloc> {
  match o {
    .some(p) { \"{p.y}\" }
    .none { \"-\" }
  }
}
";

const NAMES: &[&str] = &["ada", "alan", "grace", "edsger", "barbara"];

/// A program that indexes each type, and what it must print.
#[derive(Debug)]
struct Indexing {
    items: Vec<u8>,
    places: Vec<usize>,
    pair: (u8, u8),
    /// The keys, which may repeat, with a value each.
    entries: Vec<(usize, u8)>,
    asked: Vec<usize>,
    points: Vec<(u8, u8)>,
    point_asked: Vec<usize>,
}

fn indexing(g: &mut Gen) -> Indexing {
    let byte = |g: &mut Gen| g.below(256) as u8;
    let items: Vec<u8> = (0..g.below(5)).map(|_| byte(g)).collect();
    let places = (0..1 + g.below(4)).map(|_| g.below(items.len() + 2)).collect();
    let entries = (0..g.below(5)).map(|_| (g.below(NAMES.len()), byte(g))).collect();
    let asked = (0..1 + g.below(4)).map(|_| g.below(NAMES.len())).collect();
    let points: Vec<(u8, u8)> = (0..g.below(3)).map(|_| (byte(g), byte(g))).collect();
    let point_asked = (0..1 + g.below(3)).map(|_| g.below(4)).collect();
    Indexing {
        items,
        places,
        pair: (byte(g), byte(g)),
        entries,
        asked,
        points,
        point_asked,
    }
}

fn list<T>(items: &[T], each: impl Fn(&T) -> String) -> String {
    let items: Vec<String> = items.iter().map(each).collect();
    format!("[{}]", items.join(", "))
}

/// The program, and what the model says it prints.
fn program(t: &Indexing) -> (String, String) {
    let mut main = String::from("fn main() <console, alloc> {\n");
    let mut said = String::new();
    let shown = |v: Option<&u8>| v.map_or("-".to_string(), |v| v.to_string());
    main += &format!("  s = Shelf(items: {})\n", list(&t.items, |b| b.to_string()));
    for &p in &t.places {
        main += &format!("  print(shown(s[{p}]))\n");
        said += &format!("{}\n", shown(t.items.get(p)));
    }
    let (a, b) = t.pair;
    main += &format!("  p = Pair(a: {a}, b: {b})\n  print(\"{{p[0]}} {{p[1]}}\")\n");
    main += "  loop i in 0..2 {\n    print(\"{p[i]}\")\n  }\n";
    said += &format!("{a} {b}\n{a}\n{b}\n");
    let keys = list(&t.entries, |(k, _)| format!("\"{}\"", NAMES[*k]));
    let values = list(&t.entries, |(_, v)| v.to_string());
    main += &format!("  k: Keyed[str, Byte] = Keyed(keys: {keys}, values: {values})\n");
    for &a in &t.asked {
        main += &format!("  print(shown(k[\"{}\"]))\n", NAMES[a]);
        // The first entry of a key is the one `at` finds.
        let found = t.entries.iter().find(|(k, _)| *k == a).map(|(_, v)| v);
        said += &format!("{}\n", shown(found));
    }
    let keys: Vec<String> = (0..t.points.len()).map(|i| i.to_string()).collect();
    let points = list(&t.points, |(x, y)| format!("Point(x: {x}, y: {y})"));
    main += &format!(
        "  q: Keyed[Nat, Point] = Keyed(keys: [{}], values: {points})\n",
        keys.join(", ")
    );
    for &a in &t.point_asked {
        main += &format!("  print(y_of(q[{a}]))\n");
        said += &format!("{}\n", shown(t.points.get(a).map(|(_, y)| y)));
    }
    main += "}\n";
    (format!("{TYPES}\n{main}"), said)
}

/// Indexing a shelf, a pair, and a table of keys, each through its own
/// `impl` of `Index`, prints what the model of each says.
fn an_index_of_a_type_of_a_program_gives_what_its_at_gives() {
    generate::check(1, 12, indexing, |t| {
        let (source, said) = program(t);
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
        let outcome = scalar::run(&scalar::passes(&program));
        assert_eq!(String::from_utf8_lossy(&outcome.stdout), said, "for {source}");
        assert_eq!(outcome.status, 0);
    });
}

fn every_executable_gives_what_at_gives_for_an_index_of_a_type_of_a_program() {
    let (source, said) = program(&generate::sample(1, 0, indexing));
    let outcome = scalar::Outcome {
        stdout: said.into_bytes(),
        status: 0,
    };
    for target in runnable() {
        runs_as(source.as_bytes(), target, &outcome);
    }
}

/// An index of a pair, which a function is given, with an `if` of a bound
/// before it or none, and an index of a `Byte`, which implements no `Index`.
#[derive(Debug)]
struct Guarded {
    guard: Option<usize>,
    of_byte: bool,
}

/// The program, and the place and the name of the one error it must give,
/// or nothing when it compiles.
fn guarded(t: &Guarded) -> (String, Option<(Pos, &'static str)>) {
    let head = format!("{TYPES}\nfn read(p: Pair, n: Byte, i: Nat) -> Byte {{\n");
    let open = match t.guard {
        Some(g) => format!("  if i < {g} {{ "),
        None => "  ".to_string(),
    };
    let before = format!("{head}{open}");
    let name = if t.of_byte { "n" } else { "p" };
    let close = if t.guard.is_some() { " } else { 0 }" } else { "" };
    let main = "fn main() <console, alloc> {\n  print(\"{read(Pair(a: 1, b: 2), 3, 1)}\")\n}\n";
    let source = format!("{before}{name}[i]{close}\n}}\n\n{main}");
    let at = pos_after(&format!("{before}{name}"));
    let wanted = if t.of_byte {
        Some((at, "`Index`"))
    } else if t.guard.is_some_and(|g| g <= 2) {
        None
    } else {
        Some((at, "valid"))
    };
    (source, wanted)
}

/// `p[i]` of a pair compiles where an `if` proves `i` below 2, and is
/// otherwise one error at the index that names `valid`; an index of a `Byte`
/// is one error that names `Index`.
fn an_index_compiles_exactly_where_valid_is_proven_and_is_refused_at_it() {
    generate::check(
        2,
        16,
        |g| Guarded {
            guard: (!g.one_in(3)).then(|| 1 + g.below(3)),
            of_byte: g.one_in(5),
        },
        |t| {
            let (source, wanted) = guarded(t);
            let got = lower(source.as_bytes());
            match (got, wanted) {
                (Ok(_), None) => {}
                (Err(errors), Some((at, names))) => {
                    assert_eq!(errors.len(), 1, "{errors:?} for {source}");
                    assert_eq!(errors[0].pos, at, "{errors:?} for {source}");
                    assert!(errors[0].message.contains(names), "{} for {source}", errors[0]);
                }
                (Ok(_), Some(_)) => panic!("accepted:\n{source}"),
                (Err(errors), None) => panic!("{errors:?} for {source}"),
            }
        },
    );
}
