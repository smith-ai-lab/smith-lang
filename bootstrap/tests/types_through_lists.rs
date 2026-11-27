//! A struct or an enum that holds itself only through a `List`, directly or
//! through other types, is given `Eq`, `Fmt`, `Repr`, and `Hash` as any other
//! is, and a tree of it that the generator builds to any depth is compared,
//! written, and hashed as the same shape of types that hold nothing of
//! themselves is: by the evaluator, and by every executable the machine can
//! run.

use smith::scalar::Outcome;
use smith::{lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "a_type_that_holds_itself_through_a_list_is_compared_written_and_hashed_as_the_generator_built_it",
        a_type_that_holds_itself_through_a_list_is_compared_written_and_hashed_as_the_generator_built_it,
    ),
    (
        "a_tree_holding_itself_through_a_list_is_hashed_as_the_same_shape_of_types_that_do_not_hold_themselves",
        a_tree_holding_itself_through_a_list_is_hashed_as_the_same_shape_of_types_that_do_not_hold_themselves,
    ),
    (
        "a_tree_a_hundred_levels_deep_is_compared_written_and_hashed",
        a_tree_a_hundred_levels_deep_is_compared_written_and_hashed,
    ),
];

pub const MACHINE: &[Test] = &[(
    "an_executable_compares_writes_and_hashes_a_type_that_holds_itself_through_a_list_as_the_generator_built_it",
    an_executable_compares_writes_and_hashes_a_type_that_holds_itself_through_a_list_as_the_generator_built_it,
)];

/// A tree: a number and the trees under it.
#[derive(Clone, Debug, PartialEq)]
struct Tree {
    value: u8,
    kids: Vec<Tree>,
}

/// How a tree is declared and written: by one struct that holds a `List` of
/// itself, by a struct that holds a struct that holds the `List`, or by an enum
/// whose case holds a struct that holds the `List`.
#[derive(Clone, Copy, Debug)]
enum Shape {
    Direct,
    Through,
    Cases,
}

const SHAPES: [Shape; 3] = [Shape::Direct, Shape::Through, Shape::Cases];

impl Shape {
    fn declarations(self) -> &'static str {
        match self {
            Shape::Direct => "struct Node {\n  value: u8\n  kids: List[Node]\n}\n",
            Shape::Through => {
                "struct Node {\n  value: u8\n  held: Holder\n}\n\n\
                 struct Holder {\n  kids: List[Node]\n}\n"
            }
            Shape::Cases => {
                "enum Node {\n  leaf: u8\n  branch: Forest\n}\n\n\
                 struct Forest {\n  trees: List[Node]\n  name: str\n}\n"
            }
        }
    }

    /// The source that builds the tree, and the text `repr` gives for it.
    fn write(self, t: &Tree) -> (String, String) {
        let kids: Vec<(String, String)> = t.kids.iter().map(|k| self.write(k)).collect();
        let built = kids.iter().map(|k| k.0.as_str()).collect::<Vec<_>>().join(", ");
        let shown = kids.iter().map(|k| k.1.as_str()).collect::<Vec<_>>().join(", ");
        let v = t.value;
        match self {
            Shape::Direct => (
                format!("Node(value: {v}, kids: [{built}])"),
                format!("Node(value: {v}, kids: [{shown}])"),
            ),
            Shape::Through => (
                format!("Node(value: {v}, held: Holder(kids: [{built}]))"),
                format!("Node(value: {v}, held: Holder(kids: [{shown}]))"),
            ),
            Shape::Cases if t.kids.is_empty() => (format!(".leaf({v})"), format!(".leaf({v})")),
            Shape::Cases => (
                format!(".branch(Forest(trees: [{built}], name: \"n{v}\"))"),
                format!(".branch(Forest(trees: [{shown}], name: \"n{v}\"))"),
            ),
        }
    }
}

/// A tree of at most `depth` levels under it, whose nodes have at most
/// three kids.
fn tree(g: &mut Gen, depth: usize) -> Tree {
    let count = if depth == 0 { 0 } else { g.below(4) };
    Tree {
        value: g.below(4) as u8,
        kids: (0..count).map(|_| tree(g, depth - 1)).collect(),
    }
}

/// A tree that differs from `t` in one number or in a kid left out, or not at
/// all.
fn changed(g: &mut Gen, t: &Tree) -> Tree {
    let mut t = t.clone();
    match g.below(3) {
        0 => {}
        1 => {
            let mut at = &mut t;
            while !at.kids.is_empty() && g.below(3) != 0 {
                let k = g.below(at.kids.len());
                at = &mut at.kids[k];
            }
            at.value = at.value.wrapping_add(1 + g.below(3) as u8) % 4;
        }
        _ => {
            let mut at = &mut t;
            while !at.kids.is_empty() && g.below(3) != 0 {
                let k = g.below(at.kids.len());
                at = &mut at.kids[k];
            }
            at.kids.pop();
        }
    }
    t
}

/// A program that builds two trees and prints whether they are equal, the
/// first through `Fmt` and through `repr`, and whether their hashes are equal
/// when they are, and what it prints.
struct Case {
    source: String,
    stdout: String,
}

impl std::fmt::Debug for Case {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}

fn case(g: &mut Gen) -> Case {
    let shape = *g.pick(&SHAPES);
    let depth = 1 + g.below(4);
    let a = tree(g, depth);
    let b = changed(g, &a);
    // A tree that is `Cases` and has no kids is a leaf, whatever its kids were
    // meant to be, so `branch` with none is never written.
    let (built_a, shown_a) = shape.write(&a);
    let (built_b, _) = shape.write(&b);
    let same = a == b;
    let mut source = format!(
        "use std.bits (u8)\nuse std.console (console, print)\n\n{}\n\
         fn main() <console, alloc> {{\n  \
         a: Node = {built_a}\n  b: Node = {built_b}\n  \
         print(\"{{a == b}}\")\n  print(\"{{a}}\")\n  print(\"{{repr(a)}}\")\n",
        shape.declarations(),
    );
    let mut stdout = format!("{same}\n{shown_a}\n{shown_a}\n");
    if same {
        source += "  print(\"{hash(a) == hash(b)}\")\n";
        stdout += "true\n";
    }
    source += "}\n";
    Case { source, stdout }
}

fn run(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

fn a_type_that_holds_itself_through_a_list_is_compared_written_and_hashed_as_the_generator_built_it()
{
    generate::check(258, 12, case, |c| {
        let out = run(&c.source);
        assert_eq!(String::from_utf8_lossy(&out.stdout), c.stdout);
        assert_eq!(out.status, 0);
    });
}

fn an_executable_compares_writes_and_hashes_a_type_that_holds_itself_through_a_list_as_the_generator_built_it()
 {
    for target in targets::runnable() {
        generate::check(259, 3, case, |c| {
            let said = Outcome {
                stdout: c.stdout.clone().into_bytes(),
                status: 0,
            };
            targets::runs_as(c.source.as_bytes(), target, &said);
        });
        let c = deep(1000);
        let said = Outcome {
            stdout: c.stdout.into_bytes(),
            status: 0,
        };
        targets::runs_as(c.source.as_bytes(), target, &said);
    }
}

/// The program that prints the hash of a tree of at most three levels, of
/// types that hold themselves through a `List`, and of one type for each level
/// that holds nothing of itself, as the same shape.
fn hash_programs(t: &Tree) -> (String, String) {
    fn depth(t: &Tree) -> usize {
        t.kids.iter().map(|k| 1 + depth(k)).max().unwrap_or(0)
    }
    fn build(t: &Tree, level: Option<usize>) -> String {
        let kids: Vec<String> = t.kids.iter().map(|k| build(k, level.map(|l| l + 1))).collect();
        let name = level.map_or("Node".to_string(), |l| format!("Level{l}"));
        format!("{name}(value: {}, kids: [{}])", t.value, kids.join(", "))
    }
    let levels = depth(t);
    let header = "use std.bits (u8)\nuse std.console (console, print)\n\n";
    let main = |a: String| {
        format!("fn main() <console, alloc> {{\n  a = {a}\n  print(\"{{hash(a)}}\")\n}}\n")
    };
    let recursive = format!(
        "{header}struct Node {{\n  value: u8\n  kids: List[Node]\n}}\n\n{}",
        main(build(t, None))
    );
    let mut written_out = header.to_string();
    for l in 0..=levels {
        let below = format!("Level{}", l + 1);
        written_out += &format!("struct Level{l} {{\n  value: u8\n  kids: List[{below}]\n}}\n\n");
    }
    written_out += &format!("struct Level{} {{\n  value: u8\n}}\n\n", levels + 1);
    (recursive, written_out + &main(build(t, Some(0))))
}

fn a_tree_holding_itself_through_a_list_is_hashed_as_the_same_shape_of_types_that_do_not_hold_themselves()
 {
    generate::check(260, 8, |g| tree(g, 3), |t| {
        let (recursive, written_out) = hash_programs(t);
        let (x, y) = (run(&recursive), run(&written_out));
        assert_eq!(x.status, 0, "for\n{recursive}");
        assert_eq!(y.status, 0, "for\n{written_out}");
        assert_eq!(x, y, "for\n{recursive}\nand\n{written_out}");
    });
}

/// A program that builds two chains of `depth` levels in a loop and one of a
/// level fewer, and prints whether they are equal, whether their hashes are,
/// and how long the text `repr` gives for the first is.
fn deep(depth: usize) -> Case {
    let source = format!(
        "use std.bits (u8)\nuse std.console (console, print)\n\n{}\n\
         fn chain(depth: Nat) -> Node <alloc> {{\n  \
         var t = Node(value: 1, kids: [])\n  \
         loop i in 0..depth {{\n    t = Node(value: 2, kids: [t])\n  }}\n  t\n}}\n\n\
         fn main() <console, alloc> {{\n  \
         a = chain({depth})\n  b = chain({depth})\n  c = chain({})\n  \
         print(\"{{a == b}} {{a == c}}\")\n  \
         print(\"{{hash(a) == hash(b)}} {{hash(a) == hash(c)}}\")\n  \
         print(\"{{repr(a).size == {}}}\")\n}}\n",
        Shape::Direct.declarations(),
        depth - 1,
        // `Node(value: 2, kids: [` and `])` around the level under it.
        "Node(value: 1, kids: [])".len() * (depth + 1),
    );
    Case {
        source,
        stdout: "true false\ntrue false\ntrue\n".to_string(),
    }
}

fn a_tree_a_hundred_levels_deep_is_compared_written_and_hashed() {
    let c = deep(100);
    let out = run(&c.source);
    assert_eq!(out.status, 0, "{}", String::from_utf8_lossy(&out.stdout));
    assert_eq!(String::from_utf8_lossy(&out.stdout), c.stdout);
}
