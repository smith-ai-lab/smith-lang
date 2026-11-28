//! A struct or an enum may hold itself with no `List` between, directly or
//! through other types: as a field, as what a case holds, in a tuple, or in an
//! `Option`, with `List`s mixed in. A tree of such a type is built, matched on,
//! and walked by a function that calls itself as the same tree is when a `List`
//! of one element stands in place of each direct hold, and `==`, `Fmt`, `Repr`,
//! and `Hash` are given to it. Its nodes go in the arena of what holds them.
//! A type that holds itself as a parameter of a function it holds is refused,
//! and so is one of which no value can be made.

use smith::scalar::{self, Outcome};
use smith::lower;

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "the_page_example_evaluates_negates_and_rewrites_a_tree",
        the_page_example_evaluates_negates_and_rewrites_a_tree,
    ),
    (
        "a_field_assigned_through_a_value_that_holds_itself_changes_no_copy_of_it",
        a_field_assigned_through_a_value_that_holds_itself_changes_no_copy_of_it,
    ),
    (
        "a_tree_that_holds_itself_is_walked_as_the_same_tree_built_with_lists_of_one",
        a_tree_that_holds_itself_is_walked_as_the_same_tree_built_with_lists_of_one,
    ),
    (
        "a_tree_that_holds_itself_is_compared_written_and_hashed_as_the_generator_built_it",
        a_tree_that_holds_itself_is_compared_written_and_hashed_as_the_generator_built_it,
    ),
    (
        "a_chain_ten_thousand_deep_is_walked_as_the_generator_built_it",
        a_chain_ten_thousand_deep_is_walked_as_the_generator_built_it,
    ),
    (
        "the_same_chain_built_with_lists_of_one_is_walked_the_same",
        the_same_chain_built_with_lists_of_one_is_walked_the_same,
    ),
    (
        "nodes_built_in_an_arena_are_given_back_with_it_and_ask_the_system_for_no_request_each",
        nodes_built_in_an_arena_are_given_back_with_it_and_ask_the_system_for_no_request_each,
    ),
    (
        "a_type_that_holds_itself_as_a_parameter_of_a_function_it_holds_is_refused",
        a_type_that_holds_itself_as_a_parameter_of_a_function_it_holds_is_refused,
    ),
    (
        "a_type_of_which_no_value_can_be_made_is_refused_at_its_name",
        a_type_of_which_no_value_can_be_made_is_refused_at_its_name,
    ),
];

pub const MACHINE: &[Test] = &[(
    "an_executable_walks_a_tree_that_holds_itself_as_the_evaluator_does",
    an_executable_walks_a_tree_that_holds_itself_as_the_evaluator_does,
)];

fn run(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

const PAGE: &str = "use std.alloc (arena)\nuse std.console (console, print)\n\n\
     enum Expr {\n  num: f64\n  neg: Expr\n  add: Pair\n}\n\n\
     struct Pair {\n  left: Expr\n  right: Expr\n}\n\n\
     fn eval(e: Expr) -> f64 <alloc> {\n  match e {\n    .num(v) { v }\n    .neg(x) { -eval(x) }\n    \
     .add(p) { eval(p.left) + eval(p.right) }\n  }\n}\n\n\
     fn twice(x: Expr) -> Expr { .add(Pair(left: x, right: x)) }\n\n\
     fn negate_all(e: Expr) -> Expr <alloc> {\n  match e {\n    .num(v) { .num(-v) }\n    .neg(x) { x }\n    \
     .add(p) { .add(Pair(left: negate_all(p.left), right: negate_all(p.right))) }\n  }\n}\n\n\
     fn rewritten(e: Expr) -> Expr <alloc> {\n  with arena()\n  negate_all(negate_all(e))\n}\n\n";

fn the_page_example_evaluates_negates_and_rewrites_a_tree() {
    let source = format!(
        "{PAGE}fn main() <console, alloc> {{\n  \
         e: Expr = .add(Pair(left: .num(1.0), right: .neg(.num(3.0))))\n  \
         print(\"{{eval(e)}}\")\n  print(\"{{negate_all(e)}}\")\n  print(\"{{repr(rewritten(e))}}\")\n  \
         print(\"{{eval(rewritten(e))}} {{eval(twice(e))}}\")\n}}\n"
    );
    let out = run(&source);
    assert_eq!(out.status, 0, "for\n{source}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "-2.0\n\
         .add(Pair(left: .num(-1.0), right: .num(3.0)))\n\
         .add(Pair(left: .num(1.0), right: .num(-3.0)))\n\
         -2.0 -4.0\n"
    );
}

fn a_field_assigned_through_a_value_that_holds_itself_changes_no_copy_of_it() {
    let source = format!(
        "{PAGE}struct Box {{\n  p: Pair\n  n: f64\n}}\n\n\
         fn main() <console, alloc> {{\n  \
         mut p = Pair(left: .num(1.0), right: .num(2.0))\n  q = p\n  p.left = .neg(.num(5.0))\n  \
         mut b = Box(p: q, n: 3.0)\n  b.p.right = .num(9.0)\n  b.n = 4.0\n  \
         print(\"{{p}}\")\n  print(\"{{q}}\")\n  print(\"{{b}}\")\n  \
         print(\"{{eval(.add(b.p))}} {{eval(twice(negate_all(.add(p))))}}\")\n}}\n"
    );
    let out = run(&source);
    assert_eq!(out.status, 0, "for\n{source}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Pair(left: .neg(.num(5.0)), right: .num(2.0))\n\
         Pair(left: .num(1.0), right: .num(2.0))\n\
         Box(p: Pair(left: .num(1.0), right: .num(9.0)), n: 4.0)\n\
         10.0 6.0\n"
    );
}

/// How a node of the generated type holds what is under it, in one field.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    /// `T` itself.
    Direct,
    /// `(u64, T)`.
    Tuple,
    /// `W`, a struct that holds a `T`.
    Via,
    /// `Option[T]`.
    Maybe,
    /// `List[T]`, which holds trees out of line in either form.
    Many,
}

const SLOTS: [Slot; 5] = [Slot::Direct, Slot::Tuple, Slot::Via, Slot::Maybe, Slot::Many];

/// A generated type, `enum T { leaf: u64  node: N }`, whose struct `N` has a
/// number and one field for each slot.
#[derive(Clone, Debug)]
struct Shape {
    slots: Vec<Slot>,
}

/// What a slot of a node holds.
#[derive(Clone, Debug, PartialEq)]
enum Held {
    One(Tree),
    Tuple(u64, Tree),
    Maybe(Option<Tree>),
    Many(Vec<Tree>),
}

#[derive(Clone, Debug, PartialEq)]
enum Tree {
    Leaf(u64),
    Node(u64, Vec<Held>),
}

fn shape(g: &mut Gen) -> Shape {
    let mut slots: Vec<Slot> = (0..1 + g.below(3)).map(|_| *g.pick(&SLOTS)).collect();
    // At least one slot holds the type with no `List` between.
    if slots.iter().all(|s| *s == Slot::Many) {
        slots[0] = *g.pick(&SLOTS[..4]);
    }
    Shape { slots }
}

fn tree(g: &mut Gen, s: &Shape, depth: usize) -> Tree {
    if depth == 0 || g.one_in(3) {
        return Tree::Leaf(g.below(5) as u64);
    }
    let held = (s.slots.iter())
        .map(|slot| match slot {
            Slot::Direct | Slot::Via => Held::One(tree(g, s, depth - 1)),
            Slot::Tuple => Held::Tuple(g.below(5) as u64, tree(g, s, depth - 1)),
            Slot::Maybe => Held::Maybe((!g.one_in(3)).then(|| tree(g, s, depth - 1))),
            Slot::Many => Held::Many((0..g.below(3)).map(|_| tree(g, s, depth - 1)).collect()),
        })
        .collect();
    Tree::Node(g.below(5) as u64, held)
}

/// What `walk` gives of a tree: its numbers, mixed in the order they are
/// written.
fn walked(t: &Tree) -> u64 {
    let mix = |h: u64, x: u64| h.wrapping_mul(31).wrapping_add(x);
    match t {
        Tree::Leaf(v) => *v,
        Tree::Node(v, held) => {
            let mut h = *v;
            for part in held {
                match part {
                    Held::One(t) => h = mix(h, walked(t)),
                    Held::Tuple(x, t) => h = mix(mix(h, *x), walked(t)),
                    Held::Maybe(Some(t)) => h = mix(h, walked(t)),
                    Held::Maybe(None) => h = mix(h, 7),
                    Held::Many(ts) => {
                        for t in ts {
                            h = mix(h, walked(t));
                        }
                    }
                }
            }
            h
        }
    }
}

impl Shape {
    /// The declarations, with each direct hold of `T` a `List[T]` when
    /// `lists`.
    fn declarations(&self, lists: bool) -> String {
        let held = if lists { "List[T]" } else { "T" };
        let mut fields = String::new();
        for (k, slot) in self.slots.iter().enumerate() {
            let ty = match slot {
                Slot::Direct => held.to_string(),
                Slot::Tuple => format!("(u64, {held})"),
                Slot::Via => "W".to_string(),
                Slot::Maybe => format!("Option[{held}]"),
                Slot::Many => "List[T]".to_string(),
            };
            fields += &format!("  s{k}: {ty}\n");
        }
        let via = match self.slots.contains(&Slot::Via) {
            true => format!("struct W {{\n  t: {held}\n}}\n\n"),
            false => String::new(),
        };
        format!(
            "enum T {{\n  leaf: u64\n  node: N\n}}\n\nstruct N {{\n  value: u64\n{fields}}}\n\n{via}"
        )
    }

    /// `walk`, which calls itself on each tree under a node, reading one that
    /// is held in a `List` of one by a `loop` over it.
    fn walk(&self, lists: bool) -> String {
        let each = |of: &str| match lists {
            true => format!("loop k in {of} {{ h = mix(h, walk(k)) }}"),
            false => format!("h = mix(h, walk({of}))"),
        };
        let mut body = String::new();
        for (k, slot) in self.slots.iter().enumerate() {
            body += "      ";
            body += &match slot {
                Slot::Direct => each(&format!("n.s{k}")),
                Slot::Via => each(&format!("n.s{k}.t")),
                Slot::Tuple => format!(
                    "match n.s{k} {{\n        (x, c) {{\n          h = mix(h, x)\n          {}\n        }}\n      }}",
                    each("c")
                ),
                Slot::Maybe => format!(
                    "match n.s{k} {{\n        .some(c) {{ {} }}\n        .none {{ h = mix(h, 7) }}\n      }}",
                    each("c")
                ),
                Slot::Many => format!("loop k in n.s{k} {{ h = mix(h, walk(k)) }}"),
            };
            body += "\n";
        }
        format!(
            "fn mix(h: u64, x: u64) -> u64 {{ wrapping_add(wrapping_mul(h, 31), x) }}\n\n\
             fn walk(t: T) -> u64 <alloc> {{\n  match t {{\n    .leaf(v) {{ v }}\n    .node(n) {{\n      \
             mut h: u64 = n.value\n{body}      h\n    }}\n  }}\n}}\n\n"
        )
    }

    /// The source of the tree, with each direct hold a `List` of one when
    /// `lists`, which is also how `Fmt` and `Repr` write it when not.
    fn write(&self, t: &Tree, lists: bool) -> String {
        let one = |t: &Tree| match lists {
            true => format!("[{}]", self.write(t, lists)),
            false => self.write(t, lists),
        };
        match t {
            Tree::Leaf(v) => format!(".leaf({v})"),
            Tree::Node(v, held) => {
                let mut fields = format!("value: {v}");
                for (k, (part, slot)) in held.iter().zip(&self.slots).enumerate() {
                    let written = match part {
                        Held::One(t) if *slot == Slot::Via => format!("W(t: {})", one(t)),
                        Held::One(t) => one(t),
                        Held::Tuple(x, t) => format!("({x}, {})", one(t)),
                        Held::Maybe(Some(t)) => format!(".some({})", one(t)),
                        Held::Maybe(None) => ".none".to_string(),
                        Held::Many(ts) => format!(
                            "[{}]",
                            ts.iter().map(|t| self.write(t, lists)).collect::<Vec<_>>().join(", ")
                        ),
                    };
                    fields += &format!(", s{k}: {written}");
                }
                format!(".node(N({fields}))")
            }
        }
    }

    /// A program that builds `a` and `b`, prints what `walk` gives of each and
    /// whether they are equal, and, when `traits`, prints `a` by `Fmt` and by
    /// `repr` and whether the hashes of `a` and of a second `a` are equal.
    fn program(&self, a: &Tree, b: &Tree, lists: bool, traits: bool) -> String {
        let mut main = format!(
            "  a: T = {}\n  b: T = {}\n  print(\"{{walk(a)}} {{walk(b)}}\")\n  print(\"{{a == b}}\")\n",
            self.write(a, lists),
            self.write(b, lists)
        );
        if traits {
            main += &format!(
                "  c: T = {}\n  print(\"{{a}}\")\n  print(\"{{repr(a)}}\")\n  print(\"{{hash(a) == hash(c)}} {{a == c}}\")\n",
                self.write(a, lists)
            );
        }
        format!(
            "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\n\n{}{}\
             fn main() <console, alloc> {{\n{main}}}\n",
            self.declarations(lists),
            self.walk(lists)
        )
    }
}

/// A tree that differs from `t` in one number, or not at all.
fn changed(g: &mut Gen, t: &Tree) -> Tree {
    let mut t = t.clone();
    if g.one_in(2) {
        return t;
    }
    let mut at = &mut t;
    loop {
        match at {
            Tree::Leaf(v) => {
                *v += 1;
                break;
            }
            Tree::Node(v, held) => {
                let under: Vec<usize> = (0..held.len())
                    .filter(|&k| match &held[k] {
                        Held::One(_) | Held::Tuple(..) | Held::Maybe(Some(_)) => true,
                        Held::Many(ts) => !ts.is_empty(),
                        Held::Maybe(None) => false,
                    })
                    .collect();
                if under.is_empty() || g.one_in(3) {
                    *v += 1;
                    break;
                }
                at = match &mut held[under[g.below(under.len())]] {
                    Held::One(t) | Held::Tuple(_, t) | Held::Maybe(Some(t)) => t,
                    Held::Many(ts) => &mut ts[0],
                    Held::Maybe(None) => unreachable!("not chosen"),
                };
            }
        }
    }
    t
}

#[derive(Debug)]
struct Case {
    shape: Shape,
    a: Tree,
    b: Tree,
}

fn case(g: &mut Gen) -> Case {
    let shape = shape(g);
    let depth = 1 + g.below(4);
    let a = tree(g, &shape, depth);
    let b = changed(g, &a);
    Case { shape, a, b }
}

fn walks_as_with_lists(c: &Case) -> String {
    let said = format!("{} {}\n{}\n", walked(&c.a), walked(&c.b), c.a == c.b);
    for lists in [false, true] {
        let source = c.shape.program(&c.a, &c.b, lists, false);
        let out = run(&source);
        assert_eq!(out.status, 0, "for\n{source}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), said, "for\n{source}");
    }
    said
}

fn a_tree_that_holds_itself_is_walked_as_the_same_tree_built_with_lists_of_one() {
    generate::check(1258, 10, case, |c| {
        walks_as_with_lists(c);
    });
}

fn a_tree_that_holds_itself_is_compared_written_and_hashed_as_the_generator_built_it() {
    generate::check(1259, 10, case, |c| {
        let source = c.shape.program(&c.a, &c.b, false, true);
        let written = c.shape.write(&c.a, false);
        let said = format!(
            "{} {}\n{}\n{written}\n{written}\ntrue true\n",
            walked(&c.a),
            walked(&c.b),
            c.a == c.b
        );
        let out = run(&source);
        assert_eq!(out.status, 0, "for\n{source}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), said, "for\n{source}");
    });
}

/// A chain of `depth` nodes that a loop builds, each holding the chain under
/// it in its first slot that holds `T` in place, and leaves in the rest.
#[derive(Debug)]
struct Chain {
    shape: Shape,
    depth: usize,
}

fn chain(g: &mut Gen) -> Chain {
    // A node of a long chain has at most two fields, to keep the walk short.
    let mut shape = shape(g);
    shape.slots.truncate(2);
    if shape.slots.iter().all(|s| *s == Slot::Many) {
        shape.slots[0] = Slot::Direct;
    }
    Chain {
        shape,
        depth: g.below(10_001),
    }
}

impl Chain {
    /// The slot the chain goes on in.
    fn on(&self) -> usize {
        (self.shape.slots.iter())
            .position(|s| *s != Slot::Many)
            .expect("a slot that holds in place")
    }

    fn tree(&self) -> Tree {
        let mut t = Tree::Leaf(1);
        for level in 0..self.depth {
            let held = (self.shape.slots.iter())
                .enumerate()
                .map(|(k, slot)| {
                    let under = if k == self.on() {
                        std::mem::replace(&mut t, Tree::Leaf(0))
                    } else {
                        Tree::Leaf(2)
                    };
                    match slot {
                        Slot::Direct | Slot::Via => Held::One(under),
                        Slot::Tuple => Held::Tuple(3, under),
                        Slot::Maybe => Held::Maybe(Some(under)),
                        Slot::Many => Held::Many(vec![under]),
                    }
                })
                .collect();
            t = Tree::Node((level % 5) as u64, held);
        }
        t
    }

    fn source(&self, lists: bool) -> String {
        let one = |e: &str| match lists {
            true => format!("[{e}]"),
            false => e.to_string(),
        };
        let fields: Vec<String> = (self.shape.slots.iter())
            .enumerate()
            .map(|(k, slot)| {
                let under = if k == self.on() { "t" } else { ".leaf(2)" };
                let held = match slot {
                    Slot::Direct => one(under),
                    Slot::Via => format!("W(t: {})", one(under)),
                    Slot::Tuple => format!("(3, {})", one(under)),
                    Slot::Maybe => format!(".some({})", one(under)),
                    Slot::Many => format!("[{under}]"),
                };
                format!("s{k}: {held}")
            })
            .collect();
        format!(
            "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\n\n{}{}\
             fn chain(depth: Nat) -> T <alloc> {{\n  mut t = T.leaf(1)\n  \
             loop level in 0..depth {{\n    t = .node(N(value: level % 5, {}))\n  }}\n  t\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{walk(chain({}))}}\")\n}}\n",
            self.shape.declarations(lists),
            self.shape.walk(lists),
            fields.join(", "),
            self.depth,
        )
    }

    fn stdout(&self) -> String {
        format!("{}\n", walked(&self.tree()))
    }
}

/// Checks that the chain, built with each direct hold a `List` of one when
/// `lists`, is walked to what the generator says.
fn walks_chain(seed: u64, lists: bool) {
    generate::check(seed, 1, chain, |c| {
        let source = c.source(lists);
        let out = run(&source);
        assert_eq!(out.status, 0, "for\n{source}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), c.stdout(), "for\n{source}");
    });
}

fn a_chain_ten_thousand_deep_is_walked_as_the_generator_built_it() {
    walks_chain(1260, false);
}

fn the_same_chain_built_with_lists_of_one_is_walked_the_same() {
    walks_chain(1260, true);
}

fn an_executable_walks_a_tree_that_holds_itself_as_the_evaluator_does() {
    for target in targets::runnable() {
        generate::check(1261, 2, case, |c| {
            let said = format!("{} {}\n{}\n", walked(&c.a), walked(&c.b), c.a == c.b);
            for lists in [false, true] {
                let said = Outcome {
                    stdout: said.clone().into_bytes(),
                    status: 0,
                };
                let source = c.shape.program(&c.a, &c.b, lists, false);
                targets::runs_as(source.as_bytes(), target, &said);
            }
        });
        generate::check(1262, 1, chain, |c| {
            let said = Outcome {
                stdout: c.stdout().into_bytes(),
                status: 0,
            };
            targets::runs_as(c.source(false).as_bytes(), target, &said);
        });
    }
}

/// A program that builds a chain of `n` nodes in an arena, `calls` times,
/// and prints what the walks of them add up to.
fn in_arena(n: usize, calls: usize) -> String {
    let shape = Shape {
        slots: vec![Slot::Direct, Slot::Tuple],
    };
    format!(
        "use std.alloc (arena)\nuse std.bits (u64)\nuse std.console (console, print)\n\
         use std.int (wrapping_add, wrapping_mul)\n\n{}{}\
         fn chain(depth: Nat) -> T <alloc> {{\n  mut t = T.leaf(1)\n  \
         loop level in 0..depth {{\n    t = .node(N(value: level % 5, s0: t, s1: (3, .leaf(2))))\n  }}\n  t\n}}\n\n\
         fn once(n: Nat) -> u64 {{\n  with arena()\n  walk(chain(n))\n}}\n\n\
         fn main() <console, alloc> {{\n  mut s: u64 = 0\n  \
         loop i in 0..{calls} {{ s = wrapping_add(s, once({n})) }}\n  print(\"{{s}}\")\n}}\n",
        shape.declarations(false),
        shape.walk(false),
    )
}

fn nodes_built_in_an_arena_are_given_back_with_it_and_ask_the_system_for_no_request_each() {
    let asks = |n: usize, calls: usize| {
        let source = in_arena(n, calls);
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
        let (out, requests) = scalar::run_counting(&program, usize::MAX);
        assert_eq!(out.status, 0, "for\n{source}");
        requests
    };
    let (small, large) = (asks(100, 1), asks(4000, 1));
    let bytes = |r: &[usize]| r.iter().sum::<usize>();
    // The memory asked for grows with the nodes, and no node is a request.
    assert!(bytes(&large) > bytes(&small), "{small:?} then {large:?}");
    assert!(large.len() * 100 < 4000, "{} requests for 4000 nodes", large.len());
    // What one arena took is given back when it ends, for the next to take.
    assert_eq!(asks(4000, 4), large);
}

fn a_type_that_holds_itself_as_a_parameter_of_a_function_it_holds_is_refused() {
    for (declarations, name) in [
        ("enum T {\n  wrap: fn(T) -> T\n  stop: u8\n}\n", "T"),
        ("enum T {\n  wrap: fn(List[T]) -> u8\n  stop: u8\n}\n", "T"),
        ("enum T {\n  wrap: fn(U) -> u8\n  stop: u8\n}\n\nstruct U {\n  t: List[T]\n}\n", "U"),
    ] {
        let source = format!(
            "use std.bits (u8)\n\n{declarations}\nfn main() {{\n  x: T = T.stop(1)\n}}\n"
        );
        let errors = lower(source.as_bytes()).expect_err(&source);
        let refused: Vec<_> = (errors.iter())
            .filter(|e| e.message.contains("as a parameter of a function it holds"))
            .collect();
        assert!(
            refused.len() == 1 && refused[0].message.contains(&format!("`{name}`")),
            "{errors:?} for\n{source}"
        );
    }
}

/// The line and column, from 1, of where `name` is first declared in `source`.
fn declared_at(source: &str, name: &str) -> (usize, usize) {
    for (k, line) in source.lines().enumerate() {
        for kind in ["struct ", "enum "] {
            let start = format!("{kind}{name} ");
            if line.starts_with(&start) {
                return (k + 1, kind.len() + 1);
            }
        }
    }
    unreachable!("declared")
}

fn a_type_of_which_no_value_can_be_made_is_refused_at_its_name() {
    // Each declares the types it names, of which no value can be made, the
    // first declared first.
    for (declarations, names) in [
        ("struct Loop {\n  next: Loop\n}\n", vec!["Loop"]),
        ("enum Loop {\n  a: Loop\n  b: (u8, Loop)\n}\n", vec!["Loop"]),
        ("struct Loop {\n  w: W\n}\n\nstruct W {\n  l: Loop\n}\n", vec!["Loop", "W"]),
        ("struct Loop {\n  o: Option[Loop]\n  x: Loop\n}\n", vec!["Loop"]),
    ] {
        let source = format!(
            "use std.bits (u8)\n\n{declarations}\nfn main() {{\n  x: Option[Loop] = .none\n}}\n"
        );
        let errors = lower(source.as_bytes()).expect_err(&source);
        // A cycle of such types is refused once, at the first declared.
        let name = names[0];
        let (line, col) = declared_at(&source, name);
        let refused: Vec<_> = (errors.iter())
            .filter(|e| e.message.contains("can be made"))
            .collect();
        assert!(
            refused.len() == 1
                && (refused[0].pos.line, refused[0].pos.col) == (line, col)
                && refused[0].message.contains(&format!("no value of `{name}` can be made")),
            "{errors:?} for\n{source}"
        );
        // Given a way out, the same types are accepted.
        let out = declarations.replacen("{\n", "{\n  none: u8\n", 1);
        let out = match out.starts_with("struct") {
            true => out.replacen("next: Loop", "next: Option[Loop]", 1)
                .replacen("w: W", "w: Option[W]", 1)
                .replacen("x: Loop", "x: Option[Loop]", 1)
                .replacen("  none: u8\n", "", 1),
            false => out,
        };
        let accepted = format!(
            "use std.bits (u8)\n\n{out}\nfn main() {{\n  x: Option[Loop] = .none\n}}\n"
        );
        lower(accepted.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{accepted}"));
    }
}
