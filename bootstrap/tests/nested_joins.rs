//! What `arith` knows after the join of an `if` nested in `if`s that bind
//! names: a fact is known exactly when it holds on every path into the join,
//! whatever names the paths bound, and a name bound to a value is known as
//! the value written in its place.
//!
//! The generator works out what the program does with the arithmetic of Rust
//! on 128 bits, for every value of the parameter up to past the greatest
//! number it wrote, so it shares nothing with the compiler.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[(
    "a_fact_is_known_after_a_join_exactly_when_it_holds_on_every_path_whatever_names_the_paths_bound",
    a_fact_is_known_after_a_join_exactly_when_it_holds_on_every_path_whatever_names_the_paths_bound,
)];

/// The values of `c` the model is run for. `f` requires that `c` is at most
/// 1000, so no sum is near the greatest `Nat`, and no number the generator
/// writes is above 255, so every `c` above 255 takes the paths 256 takes.
const CS: std::ops::RangeInclusive<i128> = 0..=256;

/// The values of `c` the program is run for.
const RUN: [i128; 4] = [0, 4, 100, 256];

/// The numbers a condition compares with. None is 0, as `x < 0` of a `Nat`
/// is refused as always false.
const LIMITS: [i128; 5] = [3, 10, 100, 250, 255];

/// The numbers added to a value.
const ADDS: [i128; 5] = [0, 1, 5, 100, 200];

/// What a value is made from.
#[derive(Clone, Copy, Debug)]
enum Base {
    /// The parameter `c`, a `Nat`.
    C,
    /// The name `w{n}`, bound to a value.
    Bound(usize),
    /// The `mut` `m{n}`, a `Nat`.
    Mut(usize),
}

/// `base + add`.
#[derive(Clone, Copy, Debug)]
struct Value(Base, i128);

#[derive(Clone, Debug)]
enum Stmt {
    /// `w{n} = value`, whose value has no `mut` in it.
    Bind(usize, Value),
    /// `mut m{n}: Nat = value`.
    Var(usize, Value),
    /// `m{n} = value`.
    Set(usize, Value),
    /// `r = value`, where `r` is a `Byte`, so the value must be proven to fit.
    SetR(Value),
    /// `if value < limit` when the flag is true, else `if value > limit`, with
    /// its block and the block of its `else`, if it has one.
    If(Value, bool, i128, Vec<Stmt>, Option<Vec<Stmt>>),
}

#[derive(Debug)]
struct Program {
    body: Vec<Stmt>,
    /// The `k` of `x: Byte = r + k` at the end, which is printed.
    k: i128,
}

/// What is in scope while a block is generated.
#[derive(Clone, Default)]
struct Scope {
    bound: Vec<usize>,
    vars: Vec<usize>,
    /// What the condition of the innermost `if` around is made from, in its
    /// first block, which a value is often made from too, so that the
    /// condition is what proves that the value fits.
    guard: Option<Base>,
}

struct Names {
    next: usize,
}

/// A value, which has no `mut` in it when `pure`. It is made from a bound
/// name more often than from anything else, as a fact about a bound name is
/// what a join may lose.
fn value(g: &mut Gen, scope: &Scope, pure: bool) -> Value {
    let base = match (g.below(5), scope.guard) {
        (0, Some(Base::Mut(_))) if pure => Base::C,
        (0, Some(guard)) => guard,
        (1 | 2, _) if !scope.bound.is_empty() => Base::Bound(*g.pick(&scope.bound)),
        (3, _) if !pure && !scope.vars.is_empty() => Base::Mut(*g.pick(&scope.vars)),
        _ => Base::C,
    };
    Value(base, *g.pick(&ADDS))
}

fn block(g: &mut Gen, scope: &mut Scope, names: &mut Names, depth: usize) -> Vec<Stmt> {
    let count = 1 + g.below(3);
    let mut out = Vec::new();
    for n in 0..count {
        let kinds = if depth < 4 { 6 } else { 4 };
        // A block inside an `if` often binds a name first, as a fact about
        // such a name is what the join after it may lose.
        let first = n == 0 && depth > 0 && !g.one_in(3);
        let s = match if first { 0 } else { g.below(kinds) } {
            0 => {
                let v = value(g, scope, true);
                names.next += 1;
                scope.bound.push(names.next);
                Stmt::Bind(names.next, v)
            }
            1 => {
                let v = value(g, scope, false);
                names.next += 1;
                scope.vars.push(names.next);
                Stmt::Var(names.next, v)
            }
            2 if !scope.vars.is_empty() => {
                let at = *g.pick(&scope.vars);
                Stmt::Set(at, value(g, scope, false))
            }
            2 | 3 => Stmt::SetR(value(g, scope, false)),
            _ => {
                let of = value(g, scope, false);
                let less = !g.one_in(4);
                let limit = *g.pick(&LIMITS);
                let mut inner = scope.clone();
                inner.guard = Some(of.0);
                let then = block(g, &mut inner, names, depth + 1);
                let mut inner = scope.clone();
                inner.guard = None;
                let otherwise = g
                    .one_in(2)
                    .then(|| block(g, &mut inner, names, depth + 1));
                Stmt::If(of, less, limit, then, otherwise)
            }
        };
        out.push(s);
    }
    out
}

fn program(g: &mut Gen) -> Program {
    let mut names = Names { next: 0 };
    let body = block(g, &mut Scope::default(), &mut names, 0);
    Program {
        body,
        k: *g.pick(&[0, 5, 100, 155]),
    }
}

/// The text of the program, with each name bound to a value, or with each
/// such name's value written in its place when `inline`.
fn source(p: &Program, inline: bool) -> String {
    fn bound_of(body: &[Stmt], n: usize) -> Option<Value> {
        body.iter().find_map(|s| match s {
            Stmt::Bind(m, v) if *m == n => Some(*v),
            Stmt::If(_, _, _, a, b) => {
                bound_of(a, n).or_else(|| b.as_deref().and_then(|b| bound_of(b, n)))
            }
            _ => None,
        })
    }
    fn text(p: &Program, v: Value, inline: bool) -> String {
        let base = match v.0 {
            Base::C => "c".to_string(),
            Base::Bound(n) if inline => {
                let of = bound_of(&p.body, n).expect("a bound name is bound");
                format!("({})", text(p, of, inline))
            }
            Base::Bound(n) => format!("w{n}"),
            Base::Mut(n) => format!("m{n}"),
        };
        if v.1 == 0 {
            base
        } else {
            format!("{base} + {}", v.1)
        }
    }
    fn write(p: &Program, body: &[Stmt], inline: bool, pad: &str, out: &mut String) {
        for s in body {
            match s {
                Stmt::Bind(_, _) if inline => {}
                Stmt::Bind(n, v) => out.push_str(&format!("{pad}w{n} = {}\n", text(p, *v, inline))),
                Stmt::Var(n, v) => {
                    out.push_str(&format!("{pad}mut m{n}: Nat = {}\n", text(p, *v, inline)));
                }
                Stmt::Set(n, v) => out.push_str(&format!("{pad}m{n} = {}\n", text(p, *v, inline))),
                Stmt::SetR(v) => out.push_str(&format!("{pad}r = {}\n", text(p, *v, inline))),
                Stmt::If(of, less, limit, then, otherwise) => {
                    let op = if *less { "<" } else { ">" };
                    out.push_str(&format!("{pad}if {} {op} {limit} {{\n", text(p, *of, inline)));
                    let inner = format!("{pad}  ");
                    write(p, then, inline, &inner, out);
                    if let Some(otherwise) = otherwise {
                        out.push_str(&format!("{pad}}} else {{\n"));
                        write(p, otherwise, inline, &inner, out);
                    }
                    out.push_str(&format!("{pad}}}\n"));
                }
            }
        }
    }
    let mut out = String::from(
        "use std.console (console, print)\n\nfn f(c: Nat) <console, alloc>\n  requires c <= 1000\n{\n  mut r: Byte = 0\n",
    );
    write(p, &p.body, inline, "  ", &mut out);
    out.push_str(&format!("  x: Byte = r + {}\n  print(\"{{x}}\")\n}}\n\n", p.k));
    out.push_str("fn main() <console, alloc> {\n");
    for c in RUN {
        out.push_str(&format!("  f({c})\n"));
    }
    out.push_str("}\n");
    out
}

/// What `r + k` is at the end for `c`, or none when a value given to `r`, or
/// `r + k`, does not fit a `Byte`.
fn model(p: &Program, c: i128) -> Option<i128> {
    #[derive(Default)]
    struct Env {
        bound: Vec<(usize, i128)>,
        vars: Vec<(usize, i128)>,
        r: i128,
    }
    fn get(env: &Env, c: i128, v: Value) -> i128 {
        let base = match v.0 {
            Base::C => c,
            Base::Bound(n) => env.bound.iter().rev().find(|(m, _)| *m == n).expect("bound").1,
            Base::Mut(n) => env.vars.iter().rev().find(|(m, _)| *m == n).expect("a mut").1,
        };
        base + v.1
    }
    fn run(body: &[Stmt], env: &mut Env, c: i128) -> Option<()> {
        for s in body {
            match s {
                Stmt::Bind(n, v) => {
                    let x = get(env, c, *v);
                    env.bound.push((*n, x));
                }
                Stmt::Var(n, v) => {
                    let x = get(env, c, *v);
                    env.vars.push((*n, x));
                }
                Stmt::Set(n, v) => {
                    let x = get(env, c, *v);
                    env.vars.iter_mut().rev().find(|(m, _)| m == n).expect("a mut").1 = x;
                }
                Stmt::SetR(v) => {
                    let x = get(env, c, *v);
                    env.r = (x <= 255).then_some(x)?;
                }
                Stmt::If(of, less, limit, then, otherwise) => {
                    let x = get(env, c, *of);
                    let taken = if *less { x < *limit } else { x > *limit };
                    let branch = if taken { Some(then) } else { otherwise.as_ref() };
                    if let Some(branch) = branch {
                        let (b, v) = (env.bound.len(), env.vars.len());
                        run(branch, env, c)?;
                        env.bound.truncate(b);
                        env.vars.truncate(v);
                    }
                }
            }
        }
        Some(())
    }
    let mut env = Env::default();
    run(&p.body, &mut env, c)?;
    let end = env.r + p.k;
    (end <= 255).then_some(end)
}

/// For every program whose blocks nest `if`s to depth 4, bind names and
/// `mut`s inside them, and give `r` and the `mut`s values in them, the program
/// is accepted exactly when no value given to `r`, nor `r + k`, is above
/// 255 for any `c`, so that what is known after each join is what holds on
/// every path into it; it is accepted exactly when the same program with each
/// bound name's value written in its place is; and it prints what the model
/// says.
fn a_fact_is_known_after_a_join_exactly_when_it_holds_on_every_path_whatever_names_the_paths_bound() {
    generate::check(1259, 150, program, |p| {
        let text = source(p, false);
        let inlined = source(p, true);
        let fits = CS.into_iter().all(|c| model(p, c).is_some());
        let accepted = check(text.as_bytes()).is_ok();
        assert_eq!(accepted, fits, "fits for every c is {fits}\n{text}");
        assert_eq!(
            check(inlined.as_bytes()).is_ok(),
            accepted,
            "with the bindings:\n{text}\nwith their values in their place:\n{inlined}"
        );
        if accepted {
            let program = lower(text.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {text}"));
            let expected: String = RUN
                .iter()
                .map(|&c| format!("{}\n", model(p, c).expect("it fits")))
                .collect();
            assert_eq!(
                scalar::run(&program),
                Outcome {
                    stdout: expected.into_bytes(),
                    status: 0
                },
                "{text}"
            );
        }
    });
}
