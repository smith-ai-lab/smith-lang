//! A pattern in brackets that matches a `List`. Every property builds the
//! lists and the patterns itself, so it knows which arm each list must take
//! and what the arm gives, and asks the compiler for the same.
//!
//! A program holds a function that matches a list of one element type, which
//! is a whole number, a pair of them, an `Option` of one, or a list of them,
//! with arms written as patterns in brackets whose parts are names, literals,
//! cases, pairs, and patterns in brackets, and a last arm with a bare name.
//! An arm gives its place, and the sum of what its names hold, so the output
//! shows which arm took each list and what each name held.

use smith::diag::Pos;
use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "the_example_of_the_issue_reads_the_arguments",
        the_example_of_the_issue_reads_the_arguments,
    ),
    (
        "a_list_takes_the_first_arm_whose_pattern_matches_and_its_names_hold_the_elements",
        a_list_takes_the_first_arm_whose_pattern_matches_and_its_names_hold_the_elements,
    ),
    (
        "a_match_on_a_list_with_no_bare_name_at_the_end_is_one_error_at_the_match",
        a_match_on_a_list_with_no_bare_name_at_the_end_is_one_error_at_the_match,
    ),
    (
        "a_pattern_that_the_ones_before_it_cover_is_one_error_at_that_arm",
        a_pattern_that_the_ones_before_it_cover_is_one_error_at_that_arm,
    ),
    (
        "a_pattern_in_brackets_on_what_is_no_list_is_refused",
        a_pattern_in_brackets_on_what_is_no_list_is_refused,
    ),
    (
        "a_name_in_a_pattern_in_brackets_is_refused_for_an_element_that_cannot_be_copied",
        a_name_in_a_pattern_in_brackets_is_refused_for_an_element_that_cannot_be_copied,
    ),
];

fn run(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// The errors a program is refused with, or none.
fn refused(source: &str) -> Vec<smith::diag::Error> {
    match check(source.as_bytes()) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

fn the_example_of_the_issue_reads_the_arguments() {
    let source = "use std.console (console, print)\n\nfn show(args: List[str]) <console, alloc> {\n  match args {\n    [] { print(\"none\") }\n    [\"build\", path] { print(\"build {path}\") }\n    other { print(\"other\") }\n  }\n}\n\nfn main() <console, alloc> {\n  a: List[str] = [\"build\", \"a.smith\"]\n  show(a)\n  b: List[str] = []\n  show(b)\n  c: List[str] = [\"x\"]\n  show(c)\n  d: List[str] = [\"build\"]\n  show(d)\n}\n";
    assert_eq!(
        run(source),
        Outcome {
            stdout: b"build a.smith\nnone\nother\nother\n".to_vec(),
            status: 0
        }
    );
}

/// The type of what a pattern matches.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ty {
    Int,
    Pair,
    Opt,
    List,
}

const TYS: [Ty; 4] = [Ty::Int, Ty::Pair, Ty::Opt, Ty::List];

impl Ty {
    fn name(self) -> &'static str {
        match self {
            Ty::Int => "i32",
            Ty::Pair => "(i32, i32)",
            Ty::Opt => "Option[i32]",
            Ty::List => "List[i32]",
        }
    }

    /// The most elements a pattern in brackets has, and a list of the
    /// elements is as long as one more than that at most, which is longer
    /// than any pattern.
    fn most(self) -> usize {
        match self {
            Ty::List => 2,
            _ => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Int(i32),
    Pair(i32, i32),
    Opt(Option<i32>),
    List(Vec<i32>),
}

/// A pattern of a part: a name, a literal, a pair, a case, or a pattern in
/// brackets, whose own parts are whole numbers.
#[derive(Clone, Debug, PartialEq)]
enum Pat {
    Name,
    Lit(i32),
    Pair(Box<Pat>, Box<Pat>),
    Some(Box<Pat>),
    None,
    List(Vec<Pat>),
}

/// A whole number a literal may stand for: a list holds 0, 1, and 2, and a
/// literal is 0 or 1, so that some number matches no literal.
const NUMBERS: i32 = 3;

fn int_pattern(g: &mut Gen) -> Pat {
    if g.one_in(2) {
        Pat::Name
    } else {
        Pat::Lit(g.below(2) as i32)
    }
}

fn part_pattern(g: &mut Gen, ty: Ty) -> Pat {
    match ty {
        Ty::Int => int_pattern(g),
        Ty::Pair => Pat::Pair(Box::new(int_pattern(g)), Box::new(int_pattern(g))),
        Ty::Opt => {
            if g.one_in(3) {
                Pat::None
            } else {
                Pat::Some(Box::new(int_pattern(g)))
            }
        }
        // A list is not copied into a name, so a part that is a list is a
        // pattern in brackets.
        Ty::List => Pat::List((0..g.below(3)).map(|_| int_pattern(g)).collect()),
    }
}

fn list_pattern(g: &mut Gen, ty: Ty) -> Pat {
    Pat::List(
        (0..g.below(ty.most() + 1))
            .map(|_| part_pattern(g, ty))
            .collect(),
    )
}

fn part_value(g: &mut Gen, ty: Ty) -> Val {
    let int = |g: &mut Gen| g.below(NUMBERS as usize) as i32;
    match ty {
        Ty::Int => Val::Int(int(g)),
        Ty::Pair => Val::Pair(int(g), int(g)),
        Ty::Opt => Val::Opt(if g.one_in(3) { None } else { Some(int(g)) }),
        Ty::List => Val::List((0..g.below(4)).map(|_| int(g)).collect()),
    }
}

/// Every value of the type that a pattern can tell from another: the numbers
/// of `NUMBERS`, and lists as long as one more than the patterns are.
fn values(ty: Ty) -> Vec<Val> {
    let numbers = 0..NUMBERS;
    match ty {
        Ty::Int => numbers.map(Val::Int).collect(),
        Ty::Pair => (numbers.clone())
            .flat_map(|a| numbers.clone().map(move |b| Val::Pair(a, b)))
            .collect(),
        Ty::Opt => std::iter::once(Val::Opt(None))
            .chain(numbers.map(|n| Val::Opt(Some(n))))
            .collect(),
        Ty::List => {
            let mut all = vec![Vec::new()];
            let mut level = vec![Vec::new()];
            for _ in 0..3 {
                level = level
                    .iter()
                    .flat_map(|l: &Vec<i32>| {
                        numbers.clone().map(move |n| {
                            let mut l = l.clone();
                            l.push(n);
                            l
                        })
                    })
                    .collect();
                all.extend(level.clone());
            }
            all.into_iter().map(Val::List).collect()
        }
    }
}

/// Every list the arms can tell from another, of the element type.
fn lists(ty: Ty) -> Vec<Vec<Val>> {
    let parts = values(ty);
    let mut all: Vec<Vec<Val>> = vec![Vec::new()];
    let mut level: Vec<Vec<Val>> = vec![Vec::new()];
    for _ in 0..ty.most() + 1 {
        level = (level.iter())
            .flat_map(|l| {
                parts.iter().map(move |p| {
                    let mut l = l.clone();
                    l.push(p.clone());
                    l
                })
            })
            .collect();
        all.extend(level.clone());
    }
    all
}

/// Whether the pattern matches the value, and the sum of what its names
/// then hold, added to `sum`.
fn matches(p: &Pat, v: &Val, sum: &mut i32) -> bool {
    match (p, v) {
        (Pat::Name, Val::Int(n)) => {
            *sum += n;
            true
        }
        (Pat::Lit(a), Val::Int(b)) => a == b,
        (Pat::Pair(p, q), Val::Pair(a, b)) => {
            matches(p, &Val::Int(*a), sum) && matches(q, &Val::Int(*b), sum)
        }
        (Pat::Some(p), Val::Opt(Some(n))) => matches(p, &Val::Int(*n), sum),
        (Pat::None, Val::Opt(None)) => true,
        (Pat::List(ps), Val::List(ns)) => {
            ps.len() == ns.len()
                && ps
                    .iter()
                    .zip(ns)
                    .all(|(p, n)| matches(p, &Val::Int(*n), sum))
        }
        _ => false,
    }
}

/// Whether the pattern in brackets matches the list.
fn matches_list(p: &Pat, list: &[Val], sum: &mut i32) -> bool {
    let Pat::List(ps) = p else {
        unreachable!("a pattern in brackets")
    };
    ps.len() == list.len() && ps.iter().zip(list).all(|(p, v)| matches(p, v, sum))
}

/// The pattern as the program writes it, with the names it gives each part
/// after `name`.
fn written(p: &Pat, next: &mut usize, names: &mut Vec<String>) -> String {
    match p {
        Pat::Name => {
            let name = format!("n{next}");
            *next += 1;
            names.push(name.clone());
            name
        }
        Pat::Lit(n) => n.to_string(),
        Pat::Pair(a, b) => format!(
            "({}, {})",
            written(a, next, names),
            written(b, next, names)
        ),
        Pat::Some(p) => format!(".some({})", written(p, next, names)),
        Pat::None => ".none".to_string(),
        Pat::List(ps) => {
            let parts: Vec<String> = ps.iter().map(|p| written(p, next, names)).collect();
            format!("[{}]", parts.join(", "))
        }
    }
}

fn written_value(v: &Val) -> String {
    match v {
        Val::Int(n) => n.to_string(),
        Val::Pair(a, b) => format!("({a}, {b})"),
        Val::Opt(None) => ".none".to_string(),
        Val::Opt(Some(n)) => format!(".some({n})"),
        Val::List(ns) => {
            let parts: Vec<String> = ns.iter().map(i32::to_string).collect();
            format!("[{}]", parts.join(", "))
        }
    }
}

/// A function that matches a list, and a `main` that prints what it gives
/// for each list, with what the program must print.
#[derive(Debug)]
struct Case {
    ty: Ty,
    /// The arms before the last one, which has a bare name.
    arms: Vec<Pat>,
    given: Vec<Vec<Val>>,
}

/// The arm each list takes, as its place, and the sum of what its names
/// hold.
fn takes(arms: &[Pat], list: &[Val]) -> i32 {
    for (k, arm) in arms.iter().enumerate() {
        let mut sum = 0;
        if matches_list(arm, list, &mut sum) {
            return 1000 * (k as i32 + 1) + sum;
        }
    }
    1000 * (arms.len() as i32 + 1)
}

/// The arms of a program for lists of the type, each of which matches some
/// list that none before it does.
fn arms_of(g: &mut Gen, ty: Ty) -> Vec<Pat> {
    let all = lists(ty);
    let mut arms: Vec<Pat> = Vec::new();
    for _ in 0..1 + g.below(4) {
        let arm = list_pattern(g, ty);
        let new = all.iter().any(|l| {
            matches_list(&arm, l, &mut 0) && !arms.iter().any(|a| matches_list(a, l, &mut 0))
        });
        if new {
            arms.push(arm);
        }
    }
    arms
}

fn case(g: &mut Gen) -> Case {
    let ty = *g.pick(&TYS);
    let arms = arms_of(g, ty);
    let given = (0..4)
        .map(|_| (0..g.below(ty.most() + 2)).map(|_| part_value(g, ty)).collect())
        .collect();
    Case { ty, arms, given }
}

/// The lines of the arms, each written with its body, and the line of the
/// bare name.
fn arm_lines(c: &Case) -> Vec<String> {
    let mut lines: Vec<String> = c
        .arms
        .iter()
        .enumerate()
        .map(|(k, arm)| {
            let mut names = Vec::new();
            let pattern = written(arm, &mut 0, &mut names);
            let mut body = format!("{}", 1000 * (k + 1));
            for n in &names {
                body = format!("wrapping_add({body}, {n})");
            }
            format!("    {pattern} {{ {body} }}")
        })
        .collect();
    lines.push(format!("    rest {{ {} }}", 1000 * (c.arms.len() + 1)));
    lines
}

/// The program with the lines for the arms, and the line the `match` is on.
fn program(c: &Case, lines: &[String]) -> (String, usize) {
    let ty = c.ty.name();
    let head = format!(
        "use std.bits (i32)\nuse std.console (console, print)\nuse std.int (wrapping_add)\n\nfn pick(xs: List[{ty}]) -> i32 {{\n  match xs {{\n"
    );
    let at = head.matches('\n').count();
    let mut out = head;
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("  }\n}\n\nfn main() <console, alloc> {\n");
    for (k, list) in c.given.iter().enumerate() {
        let parts: Vec<String> = list.iter().map(written_value).collect();
        out.push_str(&format!(
            "  l{k}: List[{ty}] = [{}]\n  print(\"{{pick(l{k})}}\")\n",
            parts.join(", ")
        ));
    }
    out.push_str("}\n");
    (out, at)
}

fn a_list_takes_the_first_arm_whose_pattern_matches_and_its_names_hold_the_elements() {
    generate::check(
        0x11_57,
        30,
        case,
        |c| {
            let (source, _) = program(c, &arm_lines(c));
            let mut stdout = Vec::new();
            for list in &c.given {
                stdout.extend(format!("{}\n", takes(&c.arms, list)).bytes());
            }
            assert_eq!(run(&source), Outcome { stdout, status: 0 }, "{source}");
        },
    );
}

fn a_match_on_a_list_with_no_bare_name_at_the_end_is_one_error_at_the_match() {
    generate::check(0x11_58, 30, case, |c| {
        let mut lines = arm_lines(c);
        lines.pop();
        let (source, at) = program(c, &lines);
        let errors = refused(&source);
        assert_eq!(errors.len(), 1, "{errors:?} for\n{source}");
        assert_eq!(errors[0].pos.line, at, "{errors:?} for\n{source}");
    });
}

fn a_pattern_that_the_ones_before_it_cover_is_one_error_at_that_arm() {
    generate::check(0x11_59, 30, case, |c| {
        let lines = arm_lines(c);
        // The first arm again, after every arm.
        let again = lines[0].clone();
        let mut lines = lines;
        let last = lines.len() - 1;
        lines.insert(last, again);
        let (source, at) = program(c, &lines);
        let errors = refused(&source);
        assert_eq!(errors.len(), 1, "{errors:?} for\n{source}");
        assert_eq!(
            errors[0].pos.line,
            at + 1 + last,
            "{errors:?} for\n{source}"
        );
    });
}

fn a_pattern_in_brackets_on_what_is_no_list_is_refused() {
    let source = "use std.bits (i32)\nuse std.console (console, print)\n\nfn pick(n: i32) -> i32 {\n  match n {\n    [] { 0 }\n    _ { 1 }\n  }\n}\n\nfn main() <console, alloc> {\n  print(\"{pick(1)}\")\n}\n";
    let errors = refused(source);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].pos, Pos { line: 6, col: 5 });
}

fn a_name_in_a_pattern_in_brackets_is_refused_for_an_element_that_cannot_be_copied() {
    let source = "use std.bits (i32)\nuse std.console (console, print)\n\nfn pick(xs: List[List[i32]]) -> i32 {\n  match xs {\n    [a] { 1 }\n    _ { 0 }\n  }\n}\n\nfn main() <console, alloc> {\n  xs: List[List[i32]] = []\n  print(\"{pick(xs)}\")\n}\n";
    let errors = refused(source);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].pos.line, 6);
}
