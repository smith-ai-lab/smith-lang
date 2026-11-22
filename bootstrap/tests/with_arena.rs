//! `with arena()` gives the rest of a block an arena of its own, which is
//! freed whole when the block ends: a function that uses it does not carry
//! `alloc`, a program that calls it again and again does not grow, and what
//! leaves the block is still there after it.

use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::with_imports;
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_request_handled_in_an_arena_needs_no_alloc_and_the_program_does_not_grow_with_the_requests",
        a_request_handled_in_an_arena_needs_no_alloc_and_the_program_does_not_grow_with_the_requests,
    ),
    (
        "a_program_with_arenas_prints_what_it_prints_with_the_lines_removed_when_every_value_is_read_after_the_block",
        a_program_with_arenas_prints_what_it_prints_with_the_lines_removed_when_every_value_is_read_after_the_block,
    ),
    (
        "a_function_that_allocates_only_in_an_arena_needs_no_alloc_and_removing_the_line_is_refused_naming_alloc",
        a_function_that_allocates_only_in_an_arena_needs_no_alloc_and_removing_the_line_is_refused_naming_alloc,
    ),
    (
        "entering_and_leaving_an_arena_costs_the_same_whatever_was_allocated_in_it",
        entering_and_leaving_an_arena_costs_the_same_whatever_was_allocated_in_it,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_prints_what_the_evaluator_says_when_arenas_hold_what_a_program_builds",
    every_executable_prints_what_the_evaluator_says_when_arenas_hold_what_a_program_builds,
)];

/// `squares` and `sum_of_squares` of `arenas.md`.
const SQUARES: &str = "fn squares(n: u64) -> List[u64] ! alloc {\n  var acc: List[u64] = []\n  loop i in 0..n { acc = push(acc, wrapping_mul(i, i)) }\n  acc\n}\n\nfn sum_of_squares(n: u64) -> u64 ! alloc {\n  xs = squares(n)\n  var total = 0\n  loop i in 0..xs.size { total = wrapping_add(total, xs[i]) }\n  total\n}\n\n";

/// A server that answers `calls` requests with `handle`, as `arenas.md` writes
/// it, and prints what the answers add up to.
fn serving(calls: u64, n: u64) -> String {
    with_imports(&format!(
        "{SQUARES}struct Request {{\n  n: u64\n}}\n\nstruct Response {{\n  total: u64\n  label: str\n}}\n\nfn respond(req: Request, total: u64) -> Response ! alloc {{\n  Response(total: total, label: \"n{{req.n}}\")\n}}\n\nfn handle(req: Request) -> Response {{\n  with arena()\n  respond(req, sum_of_squares(req.n))\n}}\n\nfn main() ! console, alloc {{\n  var sum = 0\n  loop i in 0..{calls} {{\n    r = handle(Request(n: {n}))\n    sum = wrapping_add(sum, r.total)\n  }}\n  print(\"{{sum}}\")\n}}\n"
    ))
}

fn asks(source: &str) -> (usize, Vec<u8>) {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let (outcome, requests) = scalar::run_counting(&program, usize::MAX);
    assert_eq!(outcome.status, 0, "for {source}");
    (requests.len(), outcome.stdout)
}

/// `handle` carries no `alloc`, though what it calls does, and the system is
/// asked for the same memory however many requests it answers.
fn a_request_handled_in_an_arena_needs_no_alloc_and_the_program_does_not_grow_with_the_requests() {
    for n in [1, 7, 40] {
        let one: u64 = (0..n).map(|i| i * i).sum();
        let mut asked = Vec::new();
        for calls in [3, 30, 90] {
            let (requests, stdout) = asks(&serving(calls, n));
            assert_eq!(stdout, format!("{}\n", calls * one).into_bytes());
            asked.push(requests);
        }
        assert_eq!(asked[0], asked[1], "for {n} elements: {asked:?}");
        assert_eq!(asked[1], asked[2], "for {n} elements: {asked:?}");
    }
}

/// Where a function puts its arena.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Placed {
    /// Nowhere: the function carries `alloc`.
    No,
    /// On its first line, so the function carries no `alloc`.
    Top,
    /// On the first line of one branch of an `if`, so the function carries
    /// `alloc` for the other.
    Branch,
}

/// What a level of the generated program does.
#[derive(Clone, Debug)]
struct Level {
    placed: Placed,
    /// The size of the list it builds and does not return.
    temp: u64,
    /// Whether the result holds the elements of that list, and of a second
    /// call of the level below.
    add_temp: bool,
    add_other: bool,
    /// Whether it ends with a `return`, which leaves the block of its arena.
    returns: bool,
}

/// A program, once with the arenas and once with their lines removed and
/// `alloc` added to each signature that then needs it, and what it prints.
#[derive(Debug)]
struct Case {
    with: String,
    without: String,
    stdout: Vec<u8>,
}

const ITEMS: &str = "use std.list (copy)\n\nfn total(xs: List[u64]) -> u64 {\n  var s = 0\n  loop i in 0..xs.size { s = wrapping_add(s, xs[i]) }\n  s\n}\n\nfn build(n: u64, seed: u64) -> List[u64] ! alloc {\n  var acc: List[u64] = []\n  loop i in 0..n { acc = push(acc, wrapping_add(wrapping_mul(i, seed), 1)) }\n  acc\n}\n\n";

fn built(n: u64, seed: u64) -> Vec<u64> {
    (0..n)
        .map(|i| i.wrapping_mul(seed).wrapping_add(1))
        .collect()
}

/// What the level `k` returns for `a`.
fn returns(levels: &[Level], base: u64, k: usize, a: u64) -> Vec<u64> {
    if k == 0 {
        return built(base, a);
    }
    let l = &levels[k - 1];
    let mut xs = returns(levels, base, k - 1, a);
    if l.add_temp {
        xs.extend(built(l.temp, a));
    }
    if l.add_other {
        xs.extend(returns(levels, base, k - 1, a.wrapping_add(1)));
    }
    xs
}

/// The statements of level `k` below its arena.
fn body(k: usize, l: &Level, indent: &str) -> String {
    let mut s = format!(
        "{indent}r = f{}(a)\n{indent}t = build({}, a)\n{indent}var acc: List[u64] = copy(r)\n",
        k - 1,
        l.temp
    );
    if l.add_temp {
        s += &format!("{indent}loop i in 0..t.size {{ acc = push(acc, t[i]) }}\n");
    }
    if l.add_other {
        s += &format!(
            "{indent}more = f{}(wrapping_add(a, 1))\n{indent}loop i in 0..more.size {{ acc = push(acc, more[i]) }}\n",
            k - 1
        );
    }
    let keyword = if l.returns { "return " } else { "" };
    s + &format!("{indent}{keyword}acc\n")
}

/// Which levels carry `alloc` when the arenas are written: a level does when
/// it is not wholly inside one. The level `0` builds a list and carries it.
fn carries(l: &Level) -> bool {
    l.placed != Placed::Top
}

fn function(k: usize, l: &Level, arenas: bool) -> String {
    let alloc = if !arenas || carries(l) { " ! alloc" } else { "" };
    let head = format!("fn f{k}(a: u64) -> List[u64]{alloc} {{\n");
    let inside = match (arenas, l.placed) {
        (true, Placed::Top) => format!("  with arena()\n{}", body(k, l, "  ")),
        (true, Placed::Branch) => format!(
            "  if wrapping_add(a, 0) < 4 {{\n    with arena()\n{}  }} else {{\n{}  }}\n",
            body(k, l, "    "),
            body(k, l, "    ")
        ),
        _ => body(k, l, "  "),
    };
    format!("{head}{inside}}}\n\n")
}

fn program(g: &mut Gen) -> Case {
    let depth = 1 + g.below(4);
    let base = g.below(5) as u64;
    let levels: Vec<Level> = (0..depth)
        .map(|_| Level {
            placed: *g.pick(&[Placed::No, Placed::Top, Placed::Top, Placed::Branch]),
            temp: g.below(5) as u64,
            add_temp: g.one_in(2),
            add_other: g.one_in(3),
            returns: g.one_in(3),
        })
        .collect();
    let calls: Vec<u64> = (0..1 + g.below(3)).map(|_| g.below(9) as u64).collect();
    let kept: Vec<u64> = (0..1 + g.below(3)).map(|_| g.below(7) as u64).collect();
    let make = |arenas: bool| {
        let mut source = ITEMS.to_string();
        source += &format!(
            "fn f0(a: u64) -> List[u64] ! alloc {{\n  build({base}, a)\n}}\n\n"
        );
        for (k, l) in levels.iter().enumerate() {
            source += &function(k + 1, l, arenas);
        }
        source += "fn main() ! console, alloc {\n";
        for (j, seed) in calls.iter().enumerate() {
            source += &format!("  r{j} = f{depth}({seed})\n");
        }
        // A list made in a block and kept in a `var` outside it.
        source += "  var kept: List[u64] = []\n";
        if arenas {
            source += &format!(
                "  loop j in 0..{} {{\n    with arena()\n    t = build(4, j)\n    kept = copy(t)\n  }}\n",
                kept.len()
            );
        } else {
            source += &format!(
                "  loop j in 0..{} {{\n    t = build(4, j)\n    kept = copy(t)\n  }}\n",
                kept.len()
            );
        }
        // Memory that an arena gave up is built over, so that a value read
        // from it is not what it was.
        source += "  pad = build(300, 7)\n";
        for j in 0..calls.len() {
            source += &format!("  print(\"{{total(r{j})}} {{r{j}.size}}\")\n");
        }
        source += "  print(\"{total(kept)} {kept.size} {total(pad)}\")\n}\n";
        with_imports(&source)
    };
    let mut stdout = Vec::new();
    for seed in &calls {
        let xs = returns(&levels, base, depth, *seed);
        let sum = xs.iter().fold(0u64, |s, x| s.wrapping_add(*x));
        stdout.extend(format!("{sum} {}\n", xs.len()).bytes());
    }
    let last = built(4, kept.len() as u64 - 1);
    let sum = last.iter().fold(0u64, |s, x| s.wrapping_add(*x));
    let pad = built(300, 7).iter().fold(0u64, |s, x| s.wrapping_add(*x));
    stdout.extend(format!("{sum} {} {pad}\n", last.len()).bytes());
    Case {
        with: make(true),
        without: make(false),
        stdout,
    }
}

fn a_program_with_arenas_prints_what_it_prints_with_the_lines_removed_when_every_value_is_read_after_the_block()
 {
    generate::check(313, 40, program, |case| {
        let with = lower(case.with.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let without = lower(case.without.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(scalar::check(&with), Ok(()));
        assert_eq!(scalar::run(&without).stdout, case.stdout);
        assert_eq!(scalar::run(&with).stdout, case.stdout);
        assert_eq!(scalar::run(&scalar::passes(&with)).stdout, case.stdout);
    });
}

fn every_executable_prints_what_the_evaluator_says_when_arenas_hold_what_a_program_builds() {
    let targets = runnable();
    generate::check(314, 8, program, |case| {
        let program = lower(case.with.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let said = scalar::run(&scalar::passes(&program));
        assert_eq!(said.stdout, case.stdout);
        for &target in &targets {
            runs_as(case.with.as_bytes(), target, &said);
        }
    });
}

/// A function that allocates only in its arena lists no effect, and nor does
/// its caller. Without the line, each is refused for the effect it lacks.
fn a_function_that_allocates_only_in_an_arena_needs_no_alloc_and_removing_the_line_is_refused_naming_alloc()
 {
    generate::check(315, 20, |g| (1 + g.below(30), g.below(3)), |&(n, depth)| {
        let mut source = format!(
            "{SQUARES}fn f0() -> u64 {{\n  with arena()\n  sum_of_squares({n})\n}}\n\n"
        );
        for k in 1..=depth {
            source += &format!("fn f{k}() -> u64 {{ f{}() }}\n\n", k - 1);
        }
        source += &format!("fn main() ! console, alloc {{\n  print(\"{{f{depth}()}}\")\n}}\n");
        let program = lower(with_imports(&source).as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let expected: u64 = (0..n as u64).map(|i| i * i).sum();
        assert_eq!(
            scalar::run(&program).stdout,
            format!("{expected}\n").into_bytes()
        );
        let removed = source.replacen("  with arena()\n", "", 1);
        let Err(errors) = lower(with_imports(&removed).as_bytes()) else {
            panic!("a function with no `alloc` that needs it was built")
        };
        assert!(
            format!("{errors:?}").contains("`alloc`"),
            "{errors:?} does not name `alloc`"
        );
    });
}

/// The instructions that run for a program with the arena, less those for the
/// same program without it, are the same however much is built in the block.
fn entering_and_leaving_an_arena_costs_the_same_whatever_was_allocated_in_it() {
    let ran = |n: u64, arena: bool| {
        let (line, alloc) = if arena {
            ("  with arena()\n", "")
        } else {
            ("", " ! alloc")
        };
        let source = with_imports(&format!(
            "{SQUARES}fn run(n: u64) -> u64{alloc} {{\n{line}  sum_of_squares(n)\n}}\n\nfn main() ! console, alloc {{\n  print(\"{{run({n})}}\")\n}}\n"
        ));
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let (outcome, count) = scalar::run_measured(&program);
        assert_eq!(outcome.status, 0);
        count
    };
    let extra: Vec<i64> = [0, 1, 10, 100, 1000]
        .iter()
        .map(|n| ran(*n, true) as i64 - ran(*n, false) as i64)
        .collect();
    assert!(extra.iter().all(|e| *e == extra[0]), "{extra:?}");
}
