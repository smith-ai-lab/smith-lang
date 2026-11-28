//! A range has the trait implemented for a range that holds it: for
//! generated implementations of a trait for ranges that do not overlap, and a
//! value of a generated range, a use gets the one implementation whose range
//! holds the value's, whether it is a call, a bound of a type parameter, or
//! the range inside an `Option`, a struct, a tuple, or a `List`, and is
//! refused naming the trait when none does. A second implementation for a
//! range that overlaps one already written is refused, naming the first. And
//! `Wrapping` is had only by a range that fills a size.

use smith::{check, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "the_page_example_gives_a_range_the_trait_of_a_byte",
        the_page_example_gives_a_range_the_trait_of_a_byte,
    ),
    (
        "a_range_uses_the_implementation_whose_range_holds_it_in_every_place_or_is_refused_naming_the_trait",
        a_range_uses_the_implementation_whose_range_holds_it_in_every_place_or_is_refused_naming_the_trait,
    ),
    (
        "an_implementation_for_a_range_that_overlaps_one_written_before_is_refused_naming_it",
        an_implementation_for_a_range_that_overlaps_one_written_before_is_refused_naming_it,
    ),
    (
        "a_range_that_fills_a_size_has_wrapping_and_one_a_value_narrower_does_not",
        a_range_that_fills_a_size_has_wrapping_and_one_a_value_narrower_does_not,
    ),
];

fn prints(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    String::from_utf8(scalar::run(&program).stdout).unwrap()
}

fn refusals(source: &str) -> Vec<String> {
    match check(source.as_bytes()) {
        Ok(_) => panic!("accepted: {source}"),
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
    }
}

const DOUBLE: &str = "trait Double[T] {\n  fn double(x: T) -> Nat\n}\n\n\
     impl Double[Byte] {\n  fn double(x: Byte) -> Nat { x * 2 }\n}\n\n\
     fn twice(x: 0..10) -> Nat { double(x) }\n\n";

fn the_page_example_gives_a_range_the_trait_of_a_byte() {
    let source = format!(
        "use std.console (console, print)\n\n{DOUBLE}\
         fn main() <console, alloc> {{\n  print(\"{{twice(3)}}\")\n}}\n"
    );
    assert_eq!(prints(&source), "6\n", "{source}");
}

/// The places a value of the range is used in, as what each needs written
/// beside the implementations and the body of `f`, which takes `x`.
const PLACES: [(&str, &str); 6] = [
    ("", "  pick(x)"),
    (
        "fn via[T: Pick](v: T) -> Nat { pick(v) }\n\n",
        "  via(x)",
    ),
    (
        "impl[T: Pick] Pick[Option[T]] {\n  fn pick(x: Option[T]) -> Nat {\n    match x {\n      .some(v) { pick(v) }\n      .none { 0 }\n    }\n  }\n}\n\n",
        "  o: Option[R] = .some(x)\n  pick(o)",
    ),
    (
        "struct Box[T] {\n  item: T\n}\n\nimpl[T: Pick] Pick[Box[T]] {\n  fn pick(x: Box[T]) -> Nat { pick(x.item) }\n}\n\n",
        "  pick(Box(item: x))",
    ),
    (
        "impl[T: Pick] Pick[(T, bool)] {\n  fn pick(x: (T, bool)) -> Nat {\n    (v, _) = x\n    pick(v)\n  }\n}\n\n",
        "  p: (R, bool) = (x, true)\n  pick(p)",
    ),
    (
        "impl[T: Pick] Pick[List[T]] {\n  fn pick(x: List[T]) -> Nat {\n    mut n: Nat = 0\n    loop v in x { n = pick(v) }\n    n\n  }\n}\n\n",
        "  l: List[R] = [x]\n  pick(l)",
    ),
];

#[derive(Debug)]
struct Lookup {
    /// The ranges implemented, `a..b` each, in order and apart.
    ranges: Vec<(i64, i64)>,
    /// The range of the value.
    value: (i64, i64),
    place: usize,
}

fn lookup(g: &mut Gen) -> Lookup {
    let mut ranges = Vec::new();
    let mut at = -300 + g.below(40) as i64;
    for _ in 0..1 + g.below(3) {
        let start = at + g.below(30) as i64;
        let end = start + 1 + g.below(120) as i64;
        ranges.push((start, end));
        at = end;
    }
    // A value inside one of them, or reaching past the ends of one.
    let (a, b) = *g.pick(&ranges);
    let low = a - 5 + g.below(12) as i64;
    let high = (low + 1).max(b - 6 + g.below(12) as i64);
    Lookup {
        ranges,
        value: (low, high),
        place: g.below(PLACES.len()),
    }
}

fn lookup_program(c: &Lookup) -> String {
    let (low, high) = c.value;
    let range = format!("{low}..{high}");
    let impls: String = (c.ranges.iter().enumerate())
        .map(|(i, (a, b))| {
            format!("impl Pick[{a}..{b}] {{\n  fn pick(x: {a}..{b}) -> Nat {{ {} }}\n}}\n\n", i + 1)
        })
        .collect();
    let (beside, body) = PLACES[c.place];
    format!(
        "use std.console (console, print)\n\n\
         trait Pick[T] {{\n  fn pick(x: T) -> Nat\n}}\n\n{impls}{beside}\
         fn f(x: {range}) -> Nat <alloc> {{\n{}\n}}\n\n\
         fn main() <console, alloc> {{\n  print(\"{{f({low})}}\")\n}}\n",
        body.replace('R', &range)
    )
}

fn a_range_uses_the_implementation_whose_range_holds_it_in_every_place_or_is_refused_naming_the_trait()
 {
    generate::check(717, 36, lookup, |c| {
        let source = lookup_program(c);
        let (low, high) = c.value;
        let holder = (c.ranges.iter()).position(|&(a, b)| a <= low && high <= b);
        match holder {
            Some(i) => assert_eq!(prints(&source), format!("{}\n", i + 1), "{source}"),
            None => {
                let errors = refusals(&source);
                assert!(
                    errors.iter().any(|e| e.contains("`Pick`")),
                    "{errors:?} for {source}"
                );
            }
        }
    });
}

#[derive(Debug)]
struct Overlap {
    first: (i64, i64),
    second: (i64, i64),
}

/// Whether `a..b` is one of the sizes, which may overlap one another.
fn is_size((a, b): (i64, i64)) -> bool {
    [(0, 256), (-128, 128), (0, 65536), (-32768, 32768)].contains(&(a, b))
}

fn overlap(g: &mut Gen) -> Overlap {
    loop {
        let a = -200 + g.below(300) as i64;
        let first = (a, a + 1 + g.below(150) as i64);
        // A second range that shares a number with the first.
        let shared = first.0 + g.below((first.1 - first.0) as usize) as i64;
        let low = shared - g.below(60) as i64;
        let second = (low, shared + 1 + g.below(60) as i64);
        if !is_size(first) && !is_size(second) {
            return Overlap { first, second };
        }
    }
}

fn an_implementation_for_a_range_that_overlaps_one_written_before_is_refused_naming_it() {
    generate::check(7170, 16, overlap, |c| {
        let ((a, b), (x, y)) = (c.first, c.second);
        let source = format!(
            "use std.console (console, print)\n\n\
             trait Pick[T] {{\n  fn pick(x: T) -> Nat\n}}\n\n\
             impl Pick[{a}..{b}] {{\n  fn pick(x: {a}..{b}) -> Nat {{ 1 }}\n}}\n\n\
             impl Pick[{x}..{y}] {{\n  fn pick(x: {x}..{y}) -> Nat {{ 2 }}\n}}\n\n\
             fn main() <console, alloc> {{\n  v: {a}..{b} = {a}\n  print(\"{{pick(v)}}\")\n}}\n"
        );
        let errors = refusals(&source);
        let inside = a <= x && y <= b && (a, b) != (x, y);
        let stands = if inside { "is inside" } else { "overlaps" };
        let named = format!("`{a}..{b}`, which `{x}..{y}` {stands}");
        let same = format!("is already implemented for `{a}..{b}`");
        assert!(
            errors.iter().any(|e| e.contains(&named) || (a, b) == (x, y) && e.contains(&same)),
            "{errors:?} for {source}"
        );
    });
}

fn a_range_that_fills_a_size_has_wrapping_and_one_a_value_narrower_does_not() {
    for bits in [8u32, 16, 32, 64] {
        for signed in [false, true] {
            let (low, high): (i128, i128) = if signed {
                (-(1 << (bits - 1)), 1 << (bits - 1))
            } else {
                (0, 1 << bits)
            };
            let program = |high: i128| {
                format!(
                    "use std.console (console, print)\nuse std.int (wrapping_add)\n\n\
                     fn next(i: {low}..{high}) -> {low}..{high} {{ wrapping_add(i, 1) }}\n\n\
                     fn main() <console, alloc> {{\n  print(\"{{next({low})}}\")\n}}\n"
                )
            };
            let filled = program(high);
            if let Err(errors) = check(filled.as_bytes()) {
                panic!("{errors:?} for {filled}");
            }
            let narrower = program(high - 1);
            let errors = refusals(&narrower);
            assert!(
                errors.iter().any(|e| e.contains("`Wrapping`")),
                "{errors:?} for {narrower}"
            );
        }
    }
}
