//! A matrix, `[2, 3]f32`, is an array with a size for each axis, and
//! `for i, j { e }` is the matrix whose element at each pair of indices is
//! what its block gives there, on the evaluator and in every executable the
//! machine can run. Each index takes its size from its own axis of each array
//! read at it alone and of the type the `for` is written for, `m[i, j]` reads
//! and writes at indices `arith` proves below each size, and two matrix types
//! are the same exactly when every size and the element are.

use smith::{check, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "grid_gives_the_rows_the_page_says",
        grid_gives_the_rows_the_page_says,
    ),
    (
        "the_matrix_holds_at_each_pair_of_indices_what_the_block_gives",
        the_matrix_holds_at_each_pair_of_indices_what_the_block_gives,
    ),
    (
        "each_index_takes_its_size_from_its_own_axis_of_each_array_and_of_the_stated_type",
        each_index_takes_its_size_from_its_own_axis_of_each_array_and_of_the_stated_type,
    ),
    (
        "an_element_is_read_and_written_at_indices_proven_below_each_size_and_refused_otherwise",
        an_element_is_read_and_written_at_indices_proven_below_each_size_and_refused_otherwise,
    ),
    (
        "two_matrix_types_are_the_same_exactly_when_every_size_and_the_element_are",
        two_matrix_types_are_the_same_exactly_when_every_size_and_the_element_are,
    ),
];

pub const MACHINE: &[Test] = &[(
    "the_matrix_holds_at_each_pair_of_indices_what_the_block_gives_in_every_executable",
    the_matrix_holds_at_each_pair_of_indices_what_the_block_gives_in_every_executable,
)];

fn stdout(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    let out = scalar::run(&program);
    assert_eq!(out.status, 0, "for\n{source}");
    String::from_utf8(out.stdout).unwrap()
}

/// The example of the page, with each element printed by its indices.
fn grid_gives_the_rows_the_page_says() {
    let source = with_header(
        "fn grid() -> [2, 3]f32 { for i, j { float(i) * 3.0 + float(j) } }\n\n\
         fn main() <console, alloc> {\n  g = grid()\n  \
         print(\"{g[0, 0]} {g[0, 1]} {g[0, 2]}\")\n  \
         print(\"{g[1, 0]} {g[1, 1]} {g[1, 2]}\")\n  \
         print(\"{g[1, 2]}\")\n}\n"
            .to_string(),
    );
    assert_eq!(
        stdout(&source),
        "0.0 1.0 2.0\n3.0 4.0 5.0\n5.0\n",
        "for\n{source}"
    );
}

/// The names of the indices of a generated `for`, one for each axis.
const NAMES: &[&str] = &["i", "j", "l"];

/// What the block of a generated `for` gives at its indices: the matrix `a`
/// read at the indices in order, the matrix `t` read at them the other way
/// round, the array `v` read at the last alone, an index, a number, and `+`
/// and `*` of these.
#[derive(Clone, Debug)]
enum Term {
    Read,
    Reversed,
    Last,
    Index(usize),
    Number(u64),
    Add(Box<Term>, Box<Term>),
    Mul(Box<Term>, Box<Term>),
}

/// A term over `axes` indices, in which `+` or `*` of numbers alone is not
/// written: numbers worked out together take the range of what they meet,
/// which may not hold them.
fn term(g: &mut Gen, axes: usize, depth: usize) -> Term {
    let pair = |g: &mut Gen| {
        let x = term(g, axes, depth - 1);
        let y = match (&x, term(g, axes, depth - 1)) {
            (Term::Number(_), Term::Number(_)) => Term::Index(0),
            (_, y) => y,
        };
        (Box::new(x), Box::new(y))
    };
    match g.below(if depth == 0 { 5 } else { 7 }) {
        0 => Term::Read,
        1 => Term::Reversed,
        2 => Term::Last,
        3 => Term::Index(g.below(axes)),
        4 => Term::Number(g.below(10) as u64),
        5 => {
            let (x, y) = pair(g);
            Term::Add(x, y)
        }
        _ => {
            let (x, y) = pair(g);
            Term::Mul(x, y)
        }
    }
}

fn written(t: &Term, axes: usize) -> String {
    let names = &NAMES[..axes];
    let reversed: Vec<&str> = names.iter().rev().copied().collect();
    match t {
        Term::Read => format!("a[{}]", names.join(", ")),
        Term::Reversed => format!("t[{}]", reversed.join(", ")),
        Term::Last => format!("v[{}]", names[axes - 1]),
        Term::Index(k) => names[*k].to_string(),
        Term::Number(k) => k.to_string(),
        Term::Add(x, y) => format!("({} + {})", written(x, axes), written(y, axes)),
        Term::Mul(x, y) => format!("({} * {})", written(x, axes), written(y, axes)),
    }
}

/// The element of `a` at the indices, which `main` builds by a `for` as the
/// sum of each index times its weight, and one: the element of `t` at the
/// indices the other way round is that of `a` at them.
fn input(weights: &[u64], at: &[usize]) -> u64 {
    1 + weights.iter().zip(at).map(|(w, i)| w * *i as u64).sum::<u64>()
}

fn value(t: &Term, weights: &[u64], v: &[u64], at: &[usize]) -> u64 {
    match t {
        Term::Read | Term::Reversed => input(weights, at),
        Term::Last => v[at[at.len() - 1]],
        Term::Index(k) => at[*k] as u64,
        Term::Number(k) => *k,
        Term::Add(x, y) => value(x, weights, v, at) + value(y, weights, v, at),
        Term::Mul(x, y) => value(x, weights, v, at) * value(y, weights, v, at),
    }
}

/// How a generated `for` gets its sizes: from the result of a function whose
/// sizes are whole number parameters, from one whose sizes are numbers, or,
/// bound with no type in `main`, from the arrays its block reads.
#[derive(Clone, Copy, Debug)]
enum Stated {
    Names,
    Numbers,
    Reads,
}

/// A `for` over two or three indices, the program that prints each element
/// of the matrix it gives on a line of its own, row after row, and what it
/// prints.
#[derive(Debug)]
struct Built {
    source: String,
    printed: String,
}

fn built(g: &mut Gen) -> Built {
    let axes = 2 + g.below(2);
    let sizes: Vec<usize> = (0..axes).map(|_| 1 + g.below(3)).collect();
    let stated = *g.pick(&[Stated::Names, Stated::Numbers, Stated::Reads]);
    let mut block = term(g, axes, 2);
    if matches!(stated, Stated::Reads) {
        block = Term::Add(Box::new(Term::Read), Box::new(block));
    }
    let weights: Vec<u64> = (0..axes).map(|_| g.below(5) as u64).collect();
    let last = sizes[axes - 1];
    let v: Vec<u64> = (0..last).map(|_| g.below(256) as u64).collect();
    let names = &NAMES[..axes];
    let joined = |xs: &[String]| xs.join(", ");
    let numbers: Vec<String> = sizes.iter().map(usize::to_string).collect();
    let reversed_numbers: Vec<String> = numbers.iter().rev().cloned().collect();
    let params: Vec<String> = ["m", "k", "w"][..axes].iter().map(|s| s.to_string()).collect();
    let reversed_params: Vec<String> = params.iter().rev().cloned().collect();
    let weighted: Vec<String> = (names.iter().zip(&weights))
        .map(|(n, w)| format!("{n} * {w}"))
        .collect();
    let v_written: Vec<String> = v.iter().map(u64::to_string).collect();
    let mut main = format!(
        "fn main() <console, alloc> {{\n  a: [{s}]u8 = for {n} {{ {w} + 1 }}\n  t: [{r}]u8 = for {rn} {{ a[{n}] }}\n  v: [{last}]u8 = [{vs}]\n",
        s = joined(&numbers),
        n = names.join(", "),
        w = weighted.join(" + "),
        r = joined(&reversed_numbers),
        rn = names.iter().rev().copied().collect::<Vec<_>>().join(", "),
        vs = v_written.join(", "),
    );
    let e = written(&block, axes);
    let build = match stated {
        Stated::Reads => {
            main.push_str(&format!("  x = for {} {{ {e} }}\n", names.join(", ")));
            String::new()
        }
        Stated::Names | Stated::Numbers => {
            let (head, s, r, l) = match stated {
                Stated::Names => (
                    format!("[{}]", (params.iter().map(|p| format!("{p}: Nat")))
                        .collect::<Vec<_>>()
                        .join(", ")),
                    joined(&params),
                    joined(&reversed_params),
                    params[axes - 1].clone(),
                ),
                _ => (
                    String::new(),
                    joined(&numbers),
                    joined(&reversed_numbers),
                    last.to_string(),
                ),
            };
            main.push_str("  x = build(a, t, v)\n");
            format!(
                "fn build{head}(a: [{s}]u8, t: [{r}]u8, v: [{l}]u8) -> [{s}]u64 {{ for {} {{ {e} }} }}\n\n",
                names.join(", ")
            )
        }
    };
    // Each element, row after row, by loops over each axis.
    let counters: Vec<String> = (0..axes).map(|k| format!("c{k}")).collect();
    let mut print = format!("print(\"{{x[{}]}}\")", counters.join(", "));
    for (c, n) in counters.iter().zip(&sizes).rev() {
        print = format!("loop {c} in 0..{n} {{ {print} }}");
    }
    main.push_str(&format!("  {print}\n}}\n"));
    let mut printed = String::new();
    let total: usize = sizes.iter().product();
    for place in 0..total {
        let mut at = vec![0; axes];
        let mut rest = place;
        for k in (0..axes).rev() {
            at[k] = rest % sizes[k];
            rest /= sizes[k];
        }
        printed.push_str(&format!("{}\n", value(&block, &weights, &v, &at)));
    }
    Built {
        source: with_header(format!("use std.bits (u8, u64)\n\n{build}{main}")),
        printed,
    }
}

/// Whatever the block reads of matrices and arrays at the indices, in order,
/// the other way round, or the last alone, and works out from them, with
/// sizes written as numbers or as names, the element at each pair of indices
/// is what the block gives there.
fn the_matrix_holds_at_each_pair_of_indices_what_the_block_gives() {
    generate::check(12461, 12, built, |b| {
        assert_eq!(stdout(&b.source), b.printed, "for\n{}", b.source);
    });
}

/// The same, in every executable the machine can run.
fn the_matrix_holds_at_each_pair_of_indices_what_the_block_gives_in_every_executable() {
    generate::check(12462, 4, built, |b| {
        let outcome = scalar::Outcome {
            stdout: b.printed.clone().into_bytes(),
            status: 0,
        };
        for target in runnable() {
            runs_as(b.source.as_bytes(), target, &outcome);
        }
    });
}

/// The block reads `a: [p, q]` at `i, j` when it reads it, and `b: [r, s]`
/// at `j, i`, the other way round, when it reads it, and the result states
/// `[x, y]` when it states a type. Each index takes a size from its own axis
/// of each: `i` from `p`, `s` and `x`, and `j` from `q`, `r` and `y`. The
/// program compiles exactly when each index has one size, and is refused
/// naming the index, and two of its sizes when it has two.
fn each_index_takes_its_size_from_its_own_axis_of_each_array_and_of_the_stated_type() {
    generate::check(
        12463,
        16,
        |g: &mut Gen| {
            let mut size = || 1 + g.below(3);
            let shapes = [(size(), size()), (size(), size()), (size(), size())];
            (shapes, g.one_in(2), g.one_in(2), g.one_in(2))
        },
        |&([(p, q), (r, s), (x, y)], reads_a, reads_b, stated)| {
            let mut terms = Vec::new();
            if reads_a {
                terms.push("a[i, j]");
            }
            if reads_b {
                terms.push("b[j, i]");
            }
            terms.push("1");
            let block = terms.join(" + ");
            let (signature, body, call) = match stated {
                true => (
                    format!(" -> [{x}, {y}]u16"),
                    format!("for i, j {{ {block} }}"),
                    "z = f(a, b)",
                ),
                false => (
                    String::new(),
                    format!("\n  z = for i, j {{ {block} }}\n"),
                    "f(a, b)",
                ),
            };
            let source = with_header(format!(
                "use std.bits (u8, u16)\n\nfn f(a: [{p}, {q}]u8, b: [{r}, {s}]u8){signature} {{ {body}}}\n\n\
                 fn main() <console, alloc> {{\n  a: [{p}, {q}]u8 = for i, j {{ 1 }}\n  b: [{r}, {s}]u8 = for i, j {{ 1 }}\n  {call}\n  print(\"done\")\n}}\n"
            ));
            let given = |sizes: [(bool, usize); 3]| -> Vec<usize> {
                sizes.iter().filter(|(on, _)| *on).map(|(_, n)| *n).collect()
            };
            let of_i = given([(reads_a, p), (reads_b, s), (stated, x)]);
            let of_j = given([(reads_a, q), (reads_b, r), (stated, y)]);
            let one = |sizes: &[usize]| !sizes.is_empty() && sizes.iter().all(|n| *n == sizes[0]);
            match check(source.as_bytes()) {
                Ok(_) => assert!(one(&of_i) && one(&of_j), "for\n{source}"),
                Err(errors) => {
                    for (index, sizes) in [("`i`", &of_i), ("`j`", &of_j)] {
                        if one(sizes) {
                            continue;
                        }
                        let refused = (errors.iter())
                            .find(|e| e.message.contains(index))
                            .unwrap_or_else(|| panic!("{errors:?} for\n{source}"));
                        let mut distinct = sizes.clone();
                        distinct.sort_unstable();
                        distinct.dedup();
                        let named = (distinct.iter())
                            .filter(|n| refused.message.contains(&n.to_string()))
                            .count();
                        assert!(
                            distinct.len() < 2 || named >= 2,
                            "{} for\n{source}",
                            refused.message
                        );
                    }
                    assert!(!(one(&of_i) && one(&of_j)), "{errors:?} for\n{source}");
                }
            }
        },
    );
}

/// A matrix of `p` by `q` is read and written at `r, c`: written out as
/// numbers, or as parameters whose ranges are `0..rs` and `0..cs`, at or one
/// past each size. It compiles exactly when each index is below its size,
/// writes that element alone, and is refused naming what it cannot prove.
fn an_element_is_read_and_written_at_indices_proven_below_each_size_and_refused_otherwise() {
    generate::check(
        12464,
        16,
        |g: &mut Gen| {
            let (p, q) = (1 + g.below(3), 1 + g.below(3));
            let (r, c) = (g.below(p + 1), g.below(q + 1));
            let ranges = (p + g.below(2), q + g.below(2));
            (p, q, r, c, g.one_in(2).then_some(ranges))
        },
        |&(p, q, r, c, ranges)| {
            let (set, call, fits) = match ranges {
                None => (
                    format!(
                        "fn set(a: [{p}, {q}]u16) -> [{p}, {q}]u16 <console, alloc> {{\n  mut b = a\n  b[{r}, {c}] = 99\n  print(\"{{b[{r}, {c}]}}\")\n  b\n}}\n\n"
                    ),
                    "set(a)".to_string(),
                    (r < p, c < q),
                ),
                Some((rs, cs)) => (
                    format!(
                        "fn set(a: [{p}, {q}]u16, r: 0..{rs}, c: 0..{cs}) -> [{p}, {q}]u16 <console, alloc> {{\n  mut b = a\n  b[r, c] = 99\n  print(\"{{b[r, c]}}\")\n  b\n}}\n\n"
                    ),
                    format!("set(a, {}, {})", r.min(rs - 1), c.min(cs - 1)),
                    (rs <= p, cs <= q),
                ),
            };
            let source = with_header(format!(
                "use std.bits (u16)\n\n{set}fn main() <console, alloc> {{\n  a: [{p}, {q}]u16 = for i, j {{ i * 10 + j }}\n  b = {call}\n  loop i in 0..{p} {{ loop j in 0..{q} {{ print(\"{{b[i, j]}}\") }} }}\n}}\n"
            ));
            match check(source.as_bytes()) {
                Ok(_) => {
                    assert!(fits.0 && fits.1, "for\n{source}");
                    let (r, c) = match ranges {
                        None => (r, c),
                        Some((rs, cs)) => (r.min(rs - 1), c.min(cs - 1)),
                    };
                    let mut printed = "99\n".to_string();
                    for i in 0..p {
                        for j in 0..q {
                            let v = if (i, j) == (r, c) { 99 } else { i * 10 + j };
                            printed.push_str(&format!("{v}\n"));
                        }
                    }
                    assert_eq!(stdout(&source), printed, "for\n{source}");
                }
                Err(errors) => {
                    assert!(!(fits.0 && fits.1), "{errors:?} for\n{source}");
                    let unproven = |at: &str, size: usize| {
                        let goal = format!("`{at} < {size}`");
                        errors.iter().any(|e| e.message.contains(&goal))
                    };
                    let (at_r, at_c) = match ranges {
                        None => (r.to_string(), c.to_string()),
                        Some(_) => ("r".to_string(), "c".to_string()),
                    };
                    assert!(fits.0 || unproven(&at_r, p), "{errors:?} for\n{source}");
                    assert!(fits.1 || unproven(&at_c, q), "{errors:?} for\n{source}");
                }
            }
        },
    );
}

/// An array type of one to three sizes, each 1 to 3, and an element.
fn array_type(g: &mut Gen) -> (Vec<usize>, &'static str) {
    let sizes = (0..1 + g.below(3)).map(|_| 1 + g.below(3)).collect();
    (sizes, *g.pick(&["u8", "u16", "f32"]))
}

/// A value of one array type is given where another is needed. It compiles
/// exactly when the two have the same number of sizes, each size the same,
/// and the same element.
fn two_matrix_types_are_the_same_exactly_when_every_size_and_the_element_are() {
    generate::check(
        12465,
        24,
        |g: &mut Gen| {
            let first = array_type(g);
            // One time in three the second type is the first, which a pick of
            // each part alone would seldom give.
            let second = match g.one_in(3) {
                true => first.clone(),
                false => array_type(g),
            };
            (first, second)
        },
        |((sizes, element), (wanted, wanted_element))| {
            let written = |sizes: &[usize], element: &str| {
                let sizes: Vec<String> = sizes.iter().map(usize::to_string).collect();
                format!("[{}]{element}", sizes.join(", "))
            };
            let zero = if *element == "f32" { "0.0" } else { "0" };
            let names = NAMES[..sizes.len()].join(", ");
            let source = with_header(format!(
                "use std.bits (u8, u16)\n\nfn pass(x: {given}) -> {needed} {{ x }}\n\n\
                 fn main() <console, alloc> {{\n  x: {given} = for {names} {{ {zero} }}\n  y = pass(x)\n  print(\"done\")\n}}\n",
                given = written(sizes, element),
                needed = written(wanted, wanted_element),
            ));
            let same = sizes == wanted && element == wanted_element;
            assert_eq!(check(source.as_bytes()).is_ok(), same, "for\n{source}");
        },
    );
}
