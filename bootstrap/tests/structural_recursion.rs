//! A function that calls itself only on a part of its argument needs no
//! `decreases`: a name a `match` on the parameter binds from what a case
//! holds, a field of it, an element of a list of it that a `loop` names, and
//! a piece that `arith` proves shorter. Functions in a circle are structural
//! when every call passes such a part at one place they all share. Anything
//! else in that place is refused as a missing `decreases` is.

use smith::diag::Error;
use smith::scalar::Outcome;
use smith::{lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "a_walk_into_a_slice_and_into_a_tree_needs_no_decreases",
        a_walk_into_a_slice_and_into_a_tree_needs_no_decreases,
    ),
    (
        "a_call_of_itself_is_accepted_exactly_when_it_passes_a_part_of_the_parameter_in_its_place",
        a_call_of_itself_is_accepted_exactly_when_it_passes_a_part_of_the_parameter_in_its_place,
    ),
    (
        "a_piece_is_a_part_exactly_when_arith_proves_it_shorter",
        a_piece_is_a_part_exactly_when_arith_proves_it_shorter,
    ),
    (
        "functions_in_a_circle_are_accepted_exactly_when_their_parts_share_a_place",
        functions_in_a_circle_are_accepted_exactly_when_their_parts_share_a_place,
    ),
    (
        "a_structural_walk_of_a_deep_tree_gives_what_a_walk_with_fuel_gives",
        a_structural_walk_of_a_deep_tree_gives_what_a_walk_with_fuel_gives,
    ),
    (
        "a_circle_walking_a_deep_tree_gives_what_a_walk_with_fuel_gives",
        a_circle_walking_a_deep_tree_gives_what_a_walk_with_fuel_gives,
    ),
    (
        "a_structural_walk_of_a_long_slice_gives_what_a_walk_with_fuel_gives",
        a_structural_walk_of_a_long_slice_gives_what_a_walk_with_fuel_gives,
    ),
    (
        "a_structural_call_that_is_not_the_last_thing_needs_alloc",
        a_structural_call_that_is_not_the_last_thing_needs_alloc,
    ),
];

pub const MACHINE: &[Test] = &[(
    "an_executable_walks_a_deep_tree_structurally_as_with_fuel",
    an_executable_walks_a_deep_tree_structurally_as_with_fuel,
)];

fn run(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

const HEADER: &str = "use std.console (console, print)\nuse std.bits (u64)\nuse std.int (wrapping_add)\n\n";

/// A tree: a leaf holds a number, and a node holds a number and its forest.
const TREE: &str = "enum Tree {\n  leaf: u64\n  node: Node\n}\n\n\
     struct Node {\n  value: u64\n  forest: Forest\n}\n\n\
     struct Forest {\n  trees: List[Tree]\n}\n\n";

fn a_walk_into_a_slice_and_into_a_tree_needs_no_decreases() {
    let source = format!(
        "{HEADER}struct Node {{\n  value: u64\n  kids: List[Node]\n}}\n\n\
         fn total(xs: Slice[u64]) -> u64 <alloc> {{\n  \
         if xs.size == 0 {{ 0 }} else {{ wrapping_add(xs[0], total(xs[1..])) }}\n}}\n\n\
         fn count(n: Node) -> u64 <alloc> {{\n  \
         mut c: u64 = 1\n  loop k in n.kids {{ c = wrapping_add(c, count(k)) }}\n  c\n}}\n\n\
         fn main() <console, alloc> {{\n  \
         print(\"{{total([4, 9, 6])}}\")\n  \
         print(\"{{count(Node(value: 0, kids: [Node(value: 1, kids: []), Node(value: 2, kids: [])]))}}\")\n}}\n"
    );
    let out = run(&source);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "19\n3\n");
    assert_eq!(out.status, 0);
}

/// What a call passes in one place.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Given {
    /// The part the walk reached, which is a part of the parameter at this
    /// place when the walk was of it, and of the other one when it was not.
    Part,
    /// The parameter at this place itself.
    Whole,
    /// A tree built in the function.
    Built,
}

const GIVEN: [Given; 3] = [Given::Part, Given::Whole, Given::Built];

/// The steps from a parameter of type `Tree` down to a part of the same type:
/// a `match` that takes the node apart, the field `forest` of the node and
/// `trees` of the forest, and a `loop` over those trees, once for each level.
/// The code at the bottom is what `inner` gives of the name it reached. What
/// a level gives is bound to a name before it is added, as a `match` written
/// as an argument in an assignment in a `loop` is not checked today.
fn walk(root: &str, levels: usize, at: usize, inner: &dyn Fn(&str) -> String) -> String {
    if levels == 0 {
        return inner(root);
    }
    let (n, s, k) = (format!("n{at}"), format!("s{at}"), format!("k{at}"));
    let below = walk(&k, levels - 1, at + 1, inner);
    format!(
        "match {root} {{\n.leaf(v{at}) {{ v{at} }}\n.node({n}) {{\nmut {s} = {n}.value\n\
         loop {k} in {n}.forest.trees {{\nx{at} = {below}\n{s} = wrapping_add({s}, x{at})\n}}\n{s}\n}}\n}}"
    )
}

/// A function `f(a: Tree, b: Tree)` that walks one of its parameters for a
/// number of levels and calls itself with what each place is given, and
/// whether the call is refused, which it is when the place of the parameter
/// walked is not given what the walk reached.
#[derive(Debug)]
struct SelfCall {
    source: String,
    refused: bool,
}

fn self_call(g: &mut Gen) -> SelfCall {
    let levels = 1 + g.below(3);
    let walked = g.below(2);
    let givens = [*g.pick(&GIVEN), *g.pick(&GIVEN)];
    let names = ["a", "b"];
    let call = |part: &str| {
        let args: Vec<String> = (0..2)
            .map(|place| match givens[place] {
                Given::Part => part.to_string(),
                Given::Whole => names[place].to_string(),
                Given::Built => ".node(Node(value: 1, forest: Forest(trees: [])))".to_string(),
            })
            .collect();
        format!("f({}, {})", args[0], args[1])
    };
    let body = walk(names[walked], levels, 0, &call);
    let source = format!(
        "{HEADER}{TREE}fn f(a: Tree, b: Tree) -> u64 <alloc> {{\n{body}\n}}\n\n\
         fn main() <console, alloc> {{\n  t = Tree.leaf(1)\n  print(\"{{f(t, t)}}\")\n}}\n"
    );
    SelfCall {
        source,
        refused: givens[walked] != Given::Part,
    }
}

/// Whether the errors are each the one a missing `decreases` gives, naming
/// the function.
fn only_missing_decreases(errors: &[Error], named: &str) -> bool {
    !errors.is_empty()
        && errors.iter().all(|e| {
            e.message.contains("needs a `decreases`") && e.message.contains(&format!("`{named}`"))
        })
}

fn a_call_of_itself_is_accepted_exactly_when_it_passes_a_part_of_the_parameter_in_its_place() {
    generate::check(1264, 24, self_call, |c| match lower(c.source.as_bytes()) {
        Ok(_) => assert!(!c.refused, "accepted\n{}", c.source),
        Err(errors) => {
            assert!(c.refused, "{errors:?} for\n{}", c.source);
            assert!(only_missing_decreases(&errors, "f"), "{errors:?} for\n{}", c.source);
        }
    });
}

/// A function over a slice that calls itself on a piece of it, and what it
/// prints when the piece is shorter.
fn piece(g: &mut Gen) -> (String, Option<&'static str>) {
    // Each piece, and what `total([4, 9, 6])` prints when it is shorter.
    const PIECES: [(&str, Option<&str>); 6] = [
        ("xs[1..]", Some("19")),
        ("xs[..xs.size - 1]", Some("12")),
        ("xs[1..xs.size]", Some("19")),
        ("xs[0..]", None),
        ("xs[..xs.size]", None),
        ("xs", None),
    ];
    let (piece, prints) = *g.pick(&PIECES);
    let source = format!(
        "{HEADER}fn total(xs: Slice[u64]) -> u64 <alloc> {{\n  \
         if xs.size == 0 {{ 0 }} else {{ wrapping_add(xs[0], total({piece})) }}\n}}\n\n\
         fn main() <console, alloc> {{\n  print(\"{{total([4, 9, 6])}}\")\n}}\n"
    );
    (source, prints)
}

fn a_piece_is_a_part_exactly_when_arith_proves_it_shorter() {
    generate::check(1265, 12, piece, |(source, prints)| {
        match lower(source.as_bytes()) {
            Ok(program) => {
                let prints = prints.unwrap_or_else(|| panic!("accepted\n{source}"));
                let out = scalar::run(&program);
                let said = String::from_utf8_lossy(&out.stdout);
                assert_eq!(said, format!("{prints}\n"), "for\n{source}");
            }
            Err(errors) => {
                assert!(prints.is_none(), "{errors:?} for\n{source}");
                assert!(only_missing_decreases(&errors, "total"), "{errors:?} for\n{source}");
            }
        }
    });
}

/// Two functions, `f(a: Tree, b: Tree)` and `g(a: Node, b: Node)`, that call
/// each other: `f` takes a node out of the tree at one place and passes it on
/// in that place, and `g` passes on each tree of the node at one place in that
/// place, each with the other place given a value it builds. They share a
/// place exactly when both are the same.
fn circle(g: &mut Gen) -> (String, bool) {
    let (pf, pg) = (g.below(2), g.below(2));
    let names = ["a", "b"];
    let call = |of: &str, place: usize, part: &str, built: &str| {
        let mut args = [built, built];
        args[place] = part;
        format!("{of}({}, {})", args[0], args[1])
    };
    let into_g = call("g", pf, "n", "Node(value: 1, forest: Forest(trees: []))");
    let into_f = call("f", pg, "k", "Tree.leaf(1)");
    let source = format!(
        "{HEADER}{TREE}fn f(a: Tree, b: Tree) -> u64 <alloc> {{\n  \
         match {walked_f} {{\n    .leaf(v) {{ v }}\n    .node(n) {{ {into_g} }}\n  }}\n}}\n\n\
         fn g(a: Node, b: Node) -> u64 <alloc> {{\n  mut s = {walked_g}.value\n  \
         loop k in {walked_g}.forest.trees {{ s = wrapping_add(s, {into_f}) }}\n  s\n}}\n\n\
         fn main() <console, alloc> {{\n  t = Tree.leaf(1)\n  print(\"{{f(t, t)}}\")\n}}\n",
        walked_f = names[pf],
        walked_g = names[pg],
    );
    (source, pf == pg)
}

fn functions_in_a_circle_are_accepted_exactly_when_their_parts_share_a_place() {
    generate::check(1266, 12, circle, |(source, shared)| {
        match lower(source.as_bytes()) {
            Ok(_) => assert!(*shared, "accepted\n{source}"),
            Err(errors) => {
                assert!(!*shared, "{errors:?} for\n{source}");
                assert!(
                    errors.len() == 1
                        && errors[0].message.contains("call each other")
                        && errors[0].message.contains("needs a `decreases`"),
                    "{errors:?} for\n{source}"
                );
            }
        }
    });
}

/// How a deep value is walked: into a tree by a `match` and a `loop`, into it
/// by two functions that call each other, or into a slice by a piece.
#[derive(Clone, Copy, Debug)]
enum Walk {
    Tree,
    Circle,
    Slice,
}

/// A tree of `depth` levels that a loop builds: each level is a node of the
/// number `value * level % 7` whose trees are the level under it, and a leaf
/// of the level when the level is a multiple of `every`. Or the slice of the
/// numbers of the levels, for `Walk::Slice`.
#[derive(Debug)]
struct Deep {
    walk: Walk,
    depth: usize,
    every: usize,
    value: usize,
}

fn deep(walk: Walk) -> impl Fn(&mut Gen) -> Deep {
    move |g| Deep {
        walk,
        depth: 2 + g.below(9_999),
        every: 2 + g.below(5),
        value: 1 + g.below(5),
    }
}

/// A walk of the tree with a `decreases` over a fuel parameter, which the
/// tree and the circle are held to.
const SUM_FUEL: &str = "fn sum_fuel(t: Tree, fuel: u64) -> u64 <alloc>\n  decreases fuel\n{\n  \
     if fuel == 0 {\n    0\n  } else {\n    match t {\n      .leaf(v) { v }\n      .node(n) {\n        \
     mut s = n.value\n        loop k in n.forest.trees {\n          \
     s = wrapping_add(s, sum_fuel(k, fuel - 1))\n        }\n        s\n      }\n    }\n  }\n}\n\n";

impl Deep {
    /// What the walk gives.
    fn sum(&self) -> u64 {
        let mut sum = match self.walk {
            Walk::Slice => 0,
            Walk::Tree | Walk::Circle => 1,
        };
        for level in 0..self.depth {
            sum += (self.value * level % 7) as u64;
            if level % self.every == 0 && !matches!(self.walk, Walk::Slice) {
                sum += level as u64;
            }
        }
        sum
    }

    /// A program that builds the value and prints what the walk gives of it,
    /// and what the same walk with fuel gives.
    fn source(&self) -> String {
        let Deep {
            depth,
            every,
            value,
            ..
        } = self;
        let fuel = depth + 1;
        let (functions, build, print) = match self.walk {
            Walk::Tree => (
                format!(
                    "fn sum(t: Tree) -> u64 <alloc> {{\n  match t {{\n    .leaf(v) {{ v }}\n    .node(n) {{\n      \
                     mut s = n.value\n      loop k in n.forest.trees {{ s = wrapping_add(s, sum(k)) }}\n      s\n    }}\n  }}\n}}\n\n{SUM_FUEL}"
                ),
                true,
                format!("{{sum(t)}} {{sum_fuel(t, {fuel})}}"),
            ),
            Walk::Circle => (
                format!(
                    "fn odd(t: Tree) -> u64 <alloc> {{\n  match t {{\n    .leaf(v) {{ v }}\n    .node(n) {{ even(n) }}\n  }}\n}}\n\n\
                     fn even(n: Node) -> u64 <alloc> {{\n  mut s = n.value\n  \
                     loop k in n.forest.trees {{ s = wrapping_add(s, odd(k)) }}\n  s\n}}\n\n{SUM_FUEL}"
                ),
                true,
                format!("{{odd(t)}} {{sum_fuel(t, {fuel})}}"),
            ),
            Walk::Slice => (
                "fn total(xs: Slice[u64]) -> u64 <alloc> {\n  \
                 if xs.size == 0 { 0 } else { wrapping_add(xs[0], total(xs[1..])) }\n}\n\n\
                 fn total_fuel(xs: Slice[u64], fuel: u64) -> u64 <alloc>\n  decreases fuel\n{\n  \
                 if xs.size == 0 or fuel == 0 { 0 } else { wrapping_add(xs[0], total_fuel(xs[1..], fuel - 1)) }\n}\n\n"
                    .to_string(),
                false,
                format!("{{total(s)}} {{total_fuel(s, {fuel})}}"),
            ),
        };
        let (types, made, kept) = match build {
            true => (
                TREE,
                "mut t = Tree.leaf(1)",
                format!(
                    "if level % {every} == 0 {{\n      \
                     t = .node(Node(value: v, forest: Forest(trees: [t, .leaf(level)])))\n    \
                     }} else {{\n      t = .node(Node(value: v, forest: Forest(trees: [t])))\n    }}"
                ),
            ),
            false => ("", "mut xs: List[u64] = []", "push(mut xs, v)".to_string()),
        };
        let frozen = if build { "" } else { "s = freeze(take xs)\n  " };
        format!(
            "{HEADER}use std.list (freeze, push)\n\n{types}{functions}\
             fn main() <console, alloc> {{\n  {made}\n  \
             loop level in 0..{depth} {{\n    v = level * {value} % 7\n    {kept}\n  }}\n  \
             {frozen}print(\"{print}\")\n}}\n"
        )
    }

    fn stdout(&self) -> String {
        let sum = self.sum();
        format!("{sum} {sum}\n")
    }
}

/// Checks that the evaluator gives what the walk with fuel gives, for one
/// value of the walk.
fn walks_as_with_fuel(seed: u64, walk: Walk) {
    generate::check(seed, 1, deep(walk), |d| {
        let source = d.source();
        let out = run(&source);
        assert_eq!(out.status, 0, "for\n{source}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), d.stdout(), "for\n{source}");
    });
}

fn a_structural_walk_of_a_deep_tree_gives_what_a_walk_with_fuel_gives() {
    walks_as_with_fuel(1267, Walk::Tree);
}

fn a_circle_walking_a_deep_tree_gives_what_a_walk_with_fuel_gives() {
    walks_as_with_fuel(1268, Walk::Circle);
}

fn a_structural_walk_of_a_long_slice_gives_what_a_walk_with_fuel_gives() {
    walks_as_with_fuel(1269, Walk::Slice);
}

fn an_executable_walks_a_deep_tree_structurally_as_with_fuel() {
    for target in targets::runnable() {
        for (seed, walk) in [(1270, Walk::Tree), (1271, Walk::Circle), (1272, Walk::Slice)] {
            generate::check(seed, 1, deep(walk), |d| {
                let said = Outcome {
                    stdout: d.stdout().into_bytes(),
                    status: 0,
                };
                targets::runs_as(d.source().as_bytes(), target, &said);
            });
        }
    }
}

fn a_structural_call_that_is_not_the_last_thing_needs_alloc() {
    for effects in ["", " <alloc>"] {
        let source = format!(
            "{HEADER}fn total(xs: Slice[u64]) -> u64{effects} {{\n  \
             if xs.size == 0 {{ 0 }} else {{ wrapping_add(xs[0], total(xs[1..])) }}\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{total([4, 9, 6])}}\")\n}}\n"
        );
        let found = lower(source.as_bytes());
        match effects {
            "" => {
                let errors = found.expect_err(&source);
                assert!(
                    errors.len() == 1 && errors[0].message.contains("`alloc`"),
                    "{errors:?} for\n{source}"
                );
            }
            _ => assert!(found.is_ok(), "{found:?} for\n{source}"),
        }
    }
}
