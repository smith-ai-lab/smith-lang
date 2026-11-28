//! `for i { e }` with one index is the array whose element at each `i` is what
//! its block gives for that `i`, on the evaluator and in every executable the
//! machine can run, held in the frame so that no memory is asked for. The
//! index takes its size from each array read at it alone and from the type the
//! `for` is written for, an index used in arithmetic takes none, and a block
//! may need no effect but `alloc`.

use smith::scalar::{self, Op, System};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "ramp_and_doubled_give_the_arrays_the_page_says",
        ramp_and_doubled_give_the_arrays_the_page_says,
    ),
    (
        "the_array_holds_at_each_index_what_the_block_gives_and_asks_for_no_memory",
        the_array_holds_at_each_index_what_the_block_gives_and_asks_for_no_memory,
    ),
    (
        "an_index_takes_its_size_from_each_array_read_at_it_alone_and_from_the_stated_type",
        an_index_takes_its_size_from_each_array_read_at_it_alone_and_from_the_stated_type,
    ),
    (
        "an_index_in_arithmetic_takes_no_size_and_its_read_is_proven_in_range_or_refused",
        an_index_in_arithmetic_takes_no_size_and_its_read_is_proven_in_range_or_refused,
    ),
    (
        "a_block_that_needs_an_effect_other_than_alloc_is_refused",
        a_block_that_needs_an_effect_other_than_alloc_is_refused,
    ),
];

pub const MACHINE: &[Test] = &[(
    "the_array_holds_at_each_index_what_the_block_gives_in_every_executable",
    the_array_holds_at_each_index_what_the_block_gives_in_every_executable,
)];

fn lowered(source: &str) -> scalar::Program {
    lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"))
}

fn stdout(source: &str) -> String {
    let out = scalar::run(&lowered(source));
    assert_eq!(out.status, 0, "for\n{source}");
    String::from_utf8(out.stdout).unwrap()
}

/// The two examples of the issue, `ramp` and `doubled`.
fn ramp_and_doubled_give_the_arrays_the_page_says() {
    let source = with_header(
        "fn ramp() -> [4]f32 { for i { float(i) } }\n\n\
         fn doubled(a: [3]f32) -> [3]f32 { for i { a[i] * 2.0 } }\n\n\
         fn main() <console, alloc> {\n  print(\"{ramp()}\")\n  print(\"{doubled([1.0, 2.0, 3.0])}\")\n}\n"
            .to_string(),
    );
    assert_eq!(
        stdout(&source),
        "[0.0, 1.0, 2.0, 3.0]\n[2.0, 4.0, 6.0]\n",
        "for\n{source}"
    );
}

/// What the block of a generated `for` gives at an index: the arrays `a0` and
/// `a1` read at it alone, the array `s` read one place after it, the index,
/// a number, and `+` and `*` of these.
#[derive(Clone, Debug)]
enum Term {
    Read(usize),
    Shifted,
    Index,
    Number(u64),
    Add(Box<Term>, Box<Term>),
    Mul(Box<Term>, Box<Term>),
}

/// A term, in which `+` or `*` of numbers alone is not written: numbers worked
/// out together take the range of what they meet, which may not hold them.
fn term(g: &mut Gen, depth: usize) -> Term {
    let pair = |g: &mut Gen| {
        let x = term(g, depth - 1);
        let y = match (&x, term(g, depth - 1)) {
            (Term::Number(_), Term::Number(_)) => Term::Index,
            (_, y) => y,
        };
        (Box::new(x), Box::new(y))
    };
    match g.below(if depth == 0 { 4 } else { 6 }) {
        0 => Term::Read(g.below(2)),
        1 => Term::Shifted,
        2 => Term::Index,
        3 => Term::Number(g.below(10) as u64),
        4 => {
            let (x, y) = pair(g);
            Term::Add(x, y)
        }
        _ => {
            let (x, y) = pair(g);
            Term::Mul(x, y)
        }
    }
}

fn written(t: &Term) -> String {
    match t {
        Term::Read(j) => format!("a{j}[i]"),
        Term::Shifted => "s[i + 1]".to_string(),
        Term::Index => "i".to_string(),
        Term::Number(k) => k.to_string(),
        Term::Add(x, y) => format!("({} + {})", written(x), written(y)),
        Term::Mul(x, y) => format!("({} * {})", written(x), written(y)),
    }
}

/// The arrays the block reads, `a0` and `a1` of `n` bytes and `s` of `n + 1`.
struct Arrays {
    a: [Vec<u64>; 2],
    s: Vec<u64>,
}

fn value(t: &Term, arrays: &Arrays, i: usize) -> u64 {
    match t {
        Term::Read(j) => arrays.a[*j][i],
        Term::Shifted => arrays.s[i + 1],
        Term::Index => i as u64,
        Term::Number(k) => *k,
        Term::Add(x, y) => value(x, arrays, i) + value(y, arrays, i),
        Term::Mul(x, y) => value(x, arrays, i) * value(y, arrays, i),
    }
}

/// A `for` over one index, the program that prints the array it gives, and
/// what it prints. When `stated` the `for` is the result of `build`, of a type
/// that gives the index its size; otherwise it is bound with no type in
/// `main`, and the block reads `a0` at the index alone, which gives it.
#[derive(Debug)]
struct Built {
    source: String,
    printed: String,
    stated: bool,
}

fn built(g: &mut Gen) -> Built {
    let n = 1 + g.below(5);
    let stated = g.one_in(2);
    let mut block = term(g, 2);
    if !stated {
        block = Term::Add(Box::new(Term::Read(0)), Box::new(block));
    }
    let mut byte = || g.below(256) as u64;
    let arrays = Arrays {
        a: [(0..n).map(|_| byte()).collect(), (0..n).map(|_| byte()).collect()],
        s: (0..=n).map(|_| byte()).collect(),
    };
    let list = |xs: &[u64]| {
        let items: Vec<String> = xs.iter().map(u64::to_string).collect();
        format!("[{}]", items.join(", "))
    };
    let (a0, a1, s) = (list(&arrays.a[0]), list(&arrays.a[1]), list(&arrays.s));
    let source = if stated {
        format!(
            "use std.bits (u8, u64)\n\nfn build(a0: [{n}]u8, a1: [{n}]u8, s: [{m}]u8) -> [{n}]u64 {{ for i {{ {e} }} }}\n\nfn main() <console, alloc> {{\n  print(\"{{build({a0}, {a1}, {s})}}\")\n}}\n",
            m = n + 1,
            e = written(&block)
        )
    } else {
        format!(
            "use std.bits (u8)\n\nfn main() <console, alloc> {{\n  a0: [{n}]u8 = {a0}\n  a1: [{n}]u8 = {a1}\n  s: [{m}]u8 = {s}\n  r = for i {{ {e} }}\n  print(\"{{r}}\")\n}}\n",
            m = n + 1,
            e = written(&block)
        )
    };
    let values: Vec<u64> = (0..n).map(|i| value(&block, &arrays, i)).collect();
    Built {
        source: with_header(source),
        printed: format!("{}\n", list(&values)),
        stated,
    }
}

/// Whatever the block reads of arrays at the index and works out from it, the
/// element at each index is what the block gives there, and the function that
/// builds the array never calls the allocator: the array is in its frame.
fn the_array_holds_at_each_index_what_the_block_gives_and_asks_for_no_memory() {
    generate::check(12441, 12, built, |b| {
        assert_eq!(stdout(&b.source), b.printed, "for\n{}", b.source);
        if b.stated {
            let program = lowered(&b.source);
            let build = (program.functions.iter())
                .find(|f| f.name.ends_with("build"))
                .expect("build is kept");
            let allocates = (build.blocks.iter().flat_map(|b| &b.instructions))
                .any(|i| matches!(i.op, Op::System { call: System::Allocate, .. }));
            assert!(!allocates, "for\n{}", b.source);
        }
    });
}

/// The same, in every executable the machine can run.
fn the_array_holds_at_each_index_what_the_block_gives_in_every_executable() {
    generate::check(12442, 4, built, |b| {
        let outcome = scalar::Outcome {
            stdout: b.printed.clone().into_bytes(),
            status: 0,
        };
        for target in runnable() {
            runs_as(b.source.as_bytes(), target, &outcome);
        }
    });
}

const INDICES: &[&str] = &["i", "k", "row"];

/// The sizes of `a` and `b`, which the block reads at the index alone when it
/// reads them, and the size the result of the function states, if it states
/// one. The program compiles exactly when they give the index one size, and
/// is refused naming the index, and both sizes when there are two.
fn an_index_takes_its_size_from_each_array_read_at_it_alone_and_from_the_stated_type() {
    generate::check(
        12443,
        16,
        |g: &mut Gen| {
            let size = |g: &mut Gen| 1 + g.below(3);
            let read = |g: &mut Gen, size: usize| g.one_in(2).then_some(size);
            let (p, q) = (size(g), size(g));
            let (a, b) = (read(g, p), read(g, q));
            let stated = g.one_in(2).then(|| size(g));
            (*g.pick(INDICES), p, q, a.is_some(), b.is_some(), stated)
        },
        |&(index, p, q, reads_a, reads_b, stated)| {
            let mut terms = Vec::new();
            if reads_a {
                terms.push(format!("a[{index}]"));
            }
            if reads_b {
                terms.push(format!("b[{index}]"));
            }
            terms.push("1".to_string());
            let block = terms.join(" + ");
            let ones = |n: usize| format!("[{}]", vec!["1"; n].join(", "));
            let (signature, end) = match stated {
                Some(r) => (format!(" -> [{r}]u16"), String::new()),
                None => (" <console, alloc>".to_string(), String::new()),
            };
            let body = match stated {
                Some(_) => format!("for {index} {{ {block} }}"),
                None => format!("\n  x = for {index} {{ {block} }}\n  print(\"{{x}}\")\n"),
            };
            let source = with_header(format!(
                "use std.bits (u8, u16)\n\nfn f(a: [{p}]u8, b: [{q}]u8){signature} {{ {body}{end}}}\n\nfn main() <console, alloc> {{\n  y = f({}, {})\n  print(\"done\")\n}}\n",
                ones(p),
                ones(q)
            ));
            let sizes: Vec<usize> = (stated.into_iter())
                .chain(reads_a.then_some(p))
                .chain(reads_b.then_some(q))
                .collect();
            let named = format!("`{index}`");
            match check(source.as_bytes()) {
                Ok(_) => assert!(
                    !sizes.is_empty() && sizes.iter().all(|s| *s == sizes[0]),
                    "for\n{source}"
                ),
                Err(errors) => {
                    let refused = errors.iter().find(|e| e.message.contains(&named));
                    let refused = refused.unwrap_or_else(|| panic!("{errors:?} for\n{source}"));
                    let mut distinct = sizes.clone();
                    distinct.sort_unstable();
                    distinct.dedup();
                    // Two of the sizes that differ are named.
                    let named = (distinct.iter())
                        .filter(|s| refused.message.contains(&s.to_string()))
                        .count();
                    match distinct.len() {
                        0 => {}
                        1 => panic!("{errors:?} for\n{source}"),
                        _ => assert!(named >= 2, "{} for\n{source}", refused.message),
                    }
                }
            }
        },
    );
}

/// `x[i + d]` gives `i` no size. With a stated size `r`, it compiles exactly
/// when `arith` proves `i + d` below the size `m` of `x` for every `i` below
/// `r`, and is refused as a read out of range is otherwise. With no type
/// stated, the index has no size, and the refusal names it.
fn an_index_in_arithmetic_takes_no_size_and_its_read_is_proven_in_range_or_refused() {
    generate::check(
        12444,
        12,
        |g: &mut Gen| (1 + g.below(4), 1 + g.below(4), 1 + g.below(2), g.one_in(3)),
        |&(m, r, d, unstated)| {
            let xs = format!("[{}]", vec!["1"; m].join(", "));
            if unstated {
                let source = with_header(format!(
                    "use std.bits (u8)\n\nfn main() <console, alloc> {{\n  x: [{m}]u8 = {xs}\n  y = for j {{ x[j + {d}] }}\n  print(\"{{y}}\")\n}}\n"
                ));
                let errors = check(source.as_bytes()).expect_err(&source);
                assert!(
                    errors.iter().any(|e| e.message.contains("`j`")),
                    "{errors:?} for\n{source}"
                );
                return;
            }
            let source = with_header(format!(
                "use std.bits (u8)\n\nfn shifted(x: [{m}]u8) -> [{r}]u8 {{ for j {{ x[j + {d}] }} }}\n\nfn main() <console, alloc> {{\n  print(\"{{shifted({xs})}}\")\n}}\n"
            ));
            match check(source.as_bytes()) {
                Ok(_) => assert!(r + d <= m, "for\n{source}"),
                Err(errors) => {
                    assert!(r + d > m, "{errors:?} for\n{source}");
                    assert!(
                        errors.iter().any(|e| e.message.contains("cannot prove that")),
                        "{errors:?} for\n{source}"
                    );
                }
            }
        },
    );
}

/// A block that calls a function needing an effect other than `alloc` is
/// refused, naming the function and the effect, with the fix of a `loop`,
/// even where the function lists the effect, and one that only allocates is
/// not.
fn a_block_that_needs_an_effect_other_than_alloc_is_refused() {
    let calls: &[(&str, &str, &str, &str)] = &[
        ("std.time (now, time)", "time", "now", "{ t = now()\n 1 }"),
        ("std.console (console, print)", "console", "print", "{ print(\"a\")\n 1 }"),
        ("std.console (console)", "alloc", "", "{ t = \"{i}\"\n t.size }"),
    ];
    for &(module, effect, callee, block) in calls {
        let source = with_header(format!(
            "use {module}\nuse std.bits (u8)\n\nfn f() -> [3]Nat <{effect}, alloc> {{ for i {block} }}\n\nfn main() <{effect}, console, alloc> {{\n  x = f()\n  print(\"done\")\n}}\n"
        ).replace("<alloc, alloc>", "<alloc>").replace("<alloc, console, alloc>", "<console, alloc>"));
        match check(source.as_bytes()) {
            Ok(_) => assert!(callee.is_empty(), "for\n{source}"),
            Err(errors) => {
                assert!(!callee.is_empty(), "{errors:?} for\n{source}");
                let refused = (errors.iter())
                    .find(|e| e.message.contains("a `for` may only allocate"))
                    .unwrap_or_else(|| panic!("{errors:?} for\n{source}"));
                assert!(
                    refused.message.contains(&format!("`{callee}`"))
                        && refused.message.contains(&format!("`{effect}`"))
                        && refused.fix.as_deref().is_some_and(|f| f.contains("`loop`")),
                    "{refused:?} for\n{source}"
                );
            }
        }
    }
}
