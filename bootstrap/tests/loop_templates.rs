//! A `mut` that starts as a number written out, and that a `loop` gives itself
//! plus a value or itself less a value, has the range that linear templates
//! over the count of passes bound: it is accepted where that range is finite
//! within 64 bits, and refused where it is not.
//!
//! Every generator knows the greatest and the least value its loop can give
//! the `mut`, from the count of its passes and what a pass adds, so what a
//! test expects shares nothing with the compiler.

use std::collections::BTreeSet;

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;

pub const ONCE: &[Test] = &[
    (
        "a_sum_over_an_array_of_the_program_prints_what_it_is_and_the_range_is_what_a_use_that_needs_less_says",
        a_sum_over_an_array_of_the_program_prints_what_it_is_and_the_range_is_what_a_use_that_needs_less_says,
    ),
    (
        "a_count_over_a_list_is_below_two_to_the_48_and_a_sum_of_bytes_is_below_two_to_the_56",
        a_count_over_a_list_is_below_two_to_the_48_and_a_sum_of_bytes_is_below_two_to_the_56,
    ),
    (
        "a_sum_of_nats_is_refused_with_the_range_why_and_the_choices",
        a_sum_of_nats_is_refused_with_the_range_why_and_the_choices,
    ),
    (
        "a_requires_that_bounds_the_size_of_a_list_makes_a_sum_of_nats_finite",
        a_requires_that_bounds_the_size_of_a_list_makes_a_sum_of_nats_finite,
    ),
    (
        "a_loop_that_counts_down_is_proven_where_it_cannot_pass_below_the_range",
        a_loop_that_counts_down_is_proven_where_it_cannot_pass_below_the_range,
    ),
    (
        "a_change_that_no_template_covers_is_refused_with_the_range_to_state",
        a_change_that_no_template_covers_is_refused_with_the_range_to_state,
    ),
    (
        "every_range_inferred_for_a_generated_loop_holds_for_every_input_and_one_that_does_not_fit_is_refused",
        every_range_inferred_for_a_generated_loop_holds_for_every_input_and_one_that_does_not_fit_is_refused,
    ),
    (
        "a_net_change_of_a_pass_that_adds_and_takes_in_nested_loops_is_the_range_the_directed_program_has",
        a_net_change_of_a_pass_that_adds_and_takes_in_nested_loops_is_the_range_the_directed_program_has,
    ),
    (
        "no_range_inferred_for_generated_nested_loops_that_add_and_take_leaves_out_a_value_the_var_has",
        no_range_inferred_for_generated_nested_loops_that_add_and_take_leaves_out_a_value_the_var_has,
    ),
    (
        "the_size_of_a_list_a_text_and_a_slice_is_below_two_to_the_48_wherever_it_is_read_from",
        the_size_of_a_list_a_text_and_a_slice_is_below_two_to_the_48_wherever_it_is_read_from,
    ),
    (
        "a_field_named_size_that_is_not_the_size_of_a_list_is_not_below_two_to_the_48",
        a_field_named_size_that_is_not_the_size_of_a_list_is_not_below_two_to_the_48,
    ),
    (
        "the_time_to_check_loops_grows_with_their_number_and_no_faster",
        the_time_to_check_loops_grows_with_their_number_and_no_faster,
    ),
];

fn run(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// The messages of the errors of a program that is refused.
fn refused(source: &str) -> Vec<String> {
    let errors = lower(source.as_bytes()).expect_err("a program that is refused");
    errors.into_iter().map(|e| e.message).collect()
}

/// The owner's program, which sums three numbers written out in a loop, prints
/// `6`, and `acc` is a `0..10`, which the error of a use that needs less says.
fn a_sum_over_an_array_of_the_program_prints_what_it_is_and_the_range_is_what_a_use_that_needs_less_says()
 {
    let body = |after: &str| {
        with_header(format!(
            "fn main() <console, alloc> {{\n  xs = [1, 2, 3]\n  mut acc = 0\n  loop x in xs {{\n    acc = acc + x\n  }}\n{after}}}\n"
        ))
    };
    assert_eq!(
        run(&body("  print(\"{acc}\")\n")),
        Outcome {
            stdout: b"6\n".to_vec(),
            status: 0
        }
    );
    let source = with_header(
        "fn takes(v: 0..9) -> 0..9 { v }\n\nfn main() <alloc> {\n  xs = [1, 2, 3]\n  mut acc = 0\n  loop x in xs {\n    acc = acc + x\n  }\n  y = takes(acc)\n}\n"
            .to_string(),
    );
    let said = refused(&source);
    assert!(
        said.iter().any(|m| m.contains("`acc` is a `0..10`")
            && m.contains("inferred")
            && m.contains("needs a `0..9`")),
        "{said:?}"
    );
}

/// A count `if p(x) { n = n + 1 }` over a list is a `0..2^48`, and a sum of a
/// `List[Byte]` is below 2^56, and both are proven.
fn a_count_over_a_list_is_below_two_to_the_48_and_a_sum_of_bytes_is_below_two_to_the_56() {
    let count = |holds: &str| {
        with_header(format!(
            "fn evens(xs: List[Nat]) -> {holds} {{\n  mut n = 0\n  loop x in xs {{\n    if x % 2 == 0 {{ n = n + 1 }}\n  }}\n  n\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{evens([1, 2, 4, 5])}}\")\n}}\n"
        ))
    };
    assert_eq!(
        run(&count("0..281474976710656")),
        Outcome {
            stdout: b"2\n".to_vec(),
            status: 0
        }
    );
    let said = refused(&count("0..281474976710655"));
    assert!(
        said.iter().any(|m| m.contains("`n` is a `0..281474976710656`")),
        "{said:?}"
    );
    let sum = |holds: &str| {
        with_header(format!(
            "fn total(xs: List[Byte]) -> {holds} {{\n  mut acc = 0\n  loop b in xs {{ acc = acc + b }}\n  acc\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{total([200, 100, 255])}}\")\n}}\n"
        ))
    };
    assert_eq!(
        run(&sum("0..72057594037927936")),
        Outcome {
            stdout: b"555\n".to_vec(),
            status: 0
        }
    );
    assert!(!refused(&sum("0..281474976710656")).is_empty());
}

/// A sum of a `List[Nat]` is refused: the error says the range and why, and
/// gives the choices, and the compiler adds no check.
fn a_sum_of_nats_is_refused_with_the_range_why_and_the_choices() {
    let source = with_header(
        "fn total(xs: List[Nat]) -> Nat {\n  mut acc = 0\n  loop x in xs { acc = acc + x }\n  acc\n}\n\nfn main() <alloc> {\n  y = total([1, 2])\n}\n"
            .to_string(),
    );
    let errors = lower(source.as_bytes()).expect_err("a sum that is not finite");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let said = &errors[0];
    assert!(
        said.message.contains("`acc`") && said.message.contains("64 bits"),
        "{said:?}"
    );
    let fix = said.fix.as_deref().expect("a fix");
    for choice in ["with an `if`", "wrapping_add", "requires"] {
        assert!(fix.contains(choice), "{fix}");
    }
}

/// A `requires` that bounds the size is what the passes are bounded by.
fn a_requires_that_bounds_the_size_of_a_list_makes_a_sum_of_nats_finite() {
    let source = with_header(
        "fn total(xs: List[0..1000]) -> 0..100001\n  requires xs.size <= 100\n{\n  mut acc = 0\n  loop x in xs { acc = acc + x }\n  acc\n}\n\n\
         fn main() <console, alloc> {\n  xs: List[0..1000] = [5, 6]\n  if xs.size <= 100 {\n    print(\"{total(xs)}\")\n  }\n}\n"
            .to_string(),
    );
    assert_eq!(
        run(&source),
        Outcome {
            stdout: b"11\n".to_vec(),
            status: 0
        }
    );
}

/// A loop that counts down, from a start, by a `1..4`, is proven where it
/// cannot pass below the range.
fn a_loop_that_counts_down_is_proven_where_it_cannot_pass_below_the_range() {
    let down = |start: u32, holds: &str| {
        with_header(format!(
            "fn down(d: 1..4) -> {holds} {{\n  mut n = {start}\n  loop i in 0..10 {{ n = n - d }}\n  n\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{down(3)}}\")\n}}\n"
        ))
    };
    assert_eq!(
        run(&down(100, "70..101")),
        Outcome {
            stdout: b"70\n".to_vec(),
            status: 0
        }
    );
    assert!(!refused(&down(100, "71..101")).is_empty());
    // From a start below what ten passes can take, the use of a `Nat` is refused.
    assert!(!refused(&down(20, "Nat")).is_empty());
}

/// A product, or any change the templates do not cover, is refused with the
/// range to state.
fn a_change_that_no_template_covers_is_refused_with_the_range_to_state() {
    for step in ["acc = acc * 2", "acc = acc + acc"] {
        let source = with_header(format!(
            "fn f(n: 0..11) -> 0..2049 {{\n  mut acc = 1\n  loop i in 0..n {{\n    {step}\n  }}\n  acc\n}}\n\nfn main() <alloc> {{\n  x = f(3)\n}}\n"
        ));
        let said = refused(&source);
        assert!(
            said.iter()
                .any(|m| m.contains("`acc`") && m.contains("changed by itself in this loop")),
            "{said:?} for\n{source}"
        );
    }
}

/// A loop of one of the shapes the templates cover, with the least and the
/// greatest the `mut` can be after it, worked out from its passes and what a
/// pass adds.
#[derive(Clone, Debug)]
struct Loop {
    /// How the loop is written, and the signature and the call that print the
    /// `mut` for the input that makes it greatest and for the one that makes
    /// it least.
    signature: String,
    body: String,
    call_most: String,
    call_least: String,
    least: i128,
    most: i128,
}

fn shapes(g: &mut Gen) -> Loop {
    let start = g.below(40) as i128;
    let takes = g.one_in(2);
    let sign = if takes { "-" } else { "+" };
    // A `mut` that takes away starts high enough that it stays above zero.
    let start = if takes { start + 8000 } else { start };
    match g.below(4) {
        0 => {
            // The elements of an array of `k` bytes, each added or taken.
            let k = 1 + g.below(6);
            let all = |n: u8| vec![n.to_string(); k].join(", ");
            let (lo, hi) = (0, 255 * k as i128);
            Loop {
                signature: format!("xs: [{k}]Byte"),
                body: format!("loop x in xs {{ acc = acc {sign} x }}"),
                call_most: format!("[{}]", all(if takes { 0 } else { 255 })),
                call_least: format!("[{}]", all(if takes { 255 } else { 0 })),
                least: if takes { start - hi } else { start + lo },
                most: if takes { start - lo } else { start + hi },
            }
        }
        1 => {
            // A range of at most `n - 1` passes, each adding or taking a `d`,
            // and of none where `n` is `0`.
            let n = 2 + g.below(9) as i128;
            let a = g.below(4) as i128;
            let b = a + 1 + g.below(5) as i128;
            let hi = (n - 1) * (b - 1);
            let (full, empty) = (format!("{}, {}", n - 1, b - 1), format!("0, {a}"));
            Loop {
                signature: format!("n: 0..{n}, d: {a}..{b}"),
                body: format!("loop i in 0..n {{ acc = acc {sign} d }}"),
                call_most: if takes { empty.clone() } else { full.clone() },
                call_least: if takes { full } else { empty },
                least: if takes { start - hi } else { start },
                most: if takes { start } else { start + hi },
            }
        }
        2 => {
            // A loop in a loop: the passes of both.
            let n = 2 + g.below(5) as i128;
            let k = 1 + g.below(4);
            let all = |v: u8| vec![v.to_string(); k].join(", ");
            let hi = (n - 1) * k as i128 * 255;
            let (full, empty) = (
                format!("{}, [{}]", n - 1, all(255)),
                format!("0, [{}]", all(0)),
            );
            Loop {
                signature: format!("n: 0..{n}, xs: [{k}]Byte"),
                body: format!("loop i in 0..n {{ loop x in xs {{ acc = acc {sign} x }} }}"),
                call_most: if takes { empty.clone() } else { full.clone() },
                call_least: if takes { full } else { empty },
                least: if takes { start - hi } else { start },
                most: if takes { start } else { start + hi },
            }
        }
        _ => {
            // A count of the bytes of an array above a number.
            let k = 1 + g.below(6);
            let above = g.below(200);
            let all = |n: usize| format!("[{}]", vec![n.to_string(); k].join(", "));
            // Every element is above the number, or none is.
            let (counted, none) = (all(255), all(0));
            Loop {
                signature: format!("xs: [{k}]Byte"),
                body: format!("loop x in xs {{ if x > {above} {{ acc = acc {sign} 1 }} }}"),
                call_most: if takes { none.clone() } else { counted.clone() },
                call_least: if takes { counted } else { none },
                least: if takes { start - k as i128 } else { start },
                most: if takes { start } else { start + k as i128 },
            }
        }
    }
    .with_start(start)
}

impl Loop {
    /// The loop with the start of its `mut` written before it.
    fn with_start(mut self, start: i128) -> Loop {
        self.body = format!("mut acc = {start}\n  {}", self.body);
        self
    }

    /// The program that passes the `mut` to a function that needs `lo..hi`.
    fn needing(&self, lo: i128, hi: i128) -> String {
        with_header(format!(
            "fn needs(v: {lo}..{hi}) -> {lo}..{hi} {{ v }}\n\n\
             fn f({}) -> Int {{\n  {}\n  needs(acc)\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{f({})}}\")\n  print(\"{{f({})}}\")\n}}\n",
            self.signature, self.body, self.call_most, self.call_least
        ))
    }
}

/// For each loop the generator builds, a use that needs exactly the range the
/// templates bound is accepted, and prints a value of it for the input that
/// makes it greatest and for the one that makes it least; a use that needs a
/// range that holds less, at either end, is refused, as a value is known that
/// it does not hold.
fn every_range_inferred_for_a_generated_loop_holds_for_every_input_and_one_that_does_not_fit_is_refused()
 {
    generate::check(9601, 24, shapes, |l| {
        let (lo, hi) = (l.least, l.most + 1);
        let source = l.needing(lo, hi);
        let out = run(&source);
        assert_eq!(
            out,
            Outcome {
                stdout: format!("{}\n{}\n", l.most, l.least).into_bytes(),
                status: 0
            },
            "{source}"
        );
        for (need_lo, need_hi) in [(lo, hi - 1), (lo + 1, hi)] {
            let source = l.needing(need_lo, need_hi);
            assert!(
                check(source.as_bytes()).is_err(),
                "a use that needs {need_lo}..{need_hi} was accepted, and the loop gives {lo}..={}\n{source}",
                hi - 1
            );
        }
    });
}

/// A pass of an outer loop that adds `30` in an inner loop and then may take
/// `1` is a change of `29..=30`, so the range is `0..=300`, and not the `-10`
/// that charging the `1` against the start would give.
fn a_net_change_of_a_pass_that_adds_and_takes_in_nested_loops_is_the_range_the_directed_program_has()
 {
    let program = |holds: &str| {
        with_header(format!(
            "fn f(c: bool) -> {holds} {{\n  mut s = 0\n  loop i in 0..10 {{\n    loop j in 0..10 {{ s = s + 3 }}\n    if c {{ s = s - 1 }}\n  }}\n  s\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{f(true)}} {{f(false)}}\")\n}}\n"
        ))
    };
    assert_eq!(
        run(&program("0..301")),
        Outcome {
            stdout: b"290 300\n".to_vec(),
            status: 0
        }
    );
    assert!(!refused(&program("0..300")).is_empty());
    assert!(!refused(&program("1..301")).is_empty());
}

/// A block of a generated program: it changes `s` by a number, or does so where
/// `c` holds, or runs a body again and again.
#[derive(Clone, Debug)]
enum Node {
    Change(i128),
    IfC(Vec<Node>),
    Rounds(usize, Vec<Node>),
}

fn nodes(g: &mut Gen, depth: usize) -> Vec<Node> {
    (0..1 + g.below(3))
        .map(|_| match g.below(if depth == 0 { 1 } else { 3 }) {
            0 => Node::Change(g.below(7) as i128 - 3),
            1 => Node::IfC(nodes(g, depth - 1)),
            _ => Node::Rounds(1 + g.below(4), nodes(g, depth - 1)),
        })
        .collect()
}

fn written(nodes: &[Node], indent: usize, counter: &mut usize, out: &mut String) {
    let pad = " ".repeat(indent);
    for node in nodes {
        match node {
            Node::Change(0) => out.push_str(&format!("{pad}s = s + 0\n")),
            Node::Change(d) if *d > 0 => out.push_str(&format!("{pad}s = s + {d}\n")),
            Node::Change(d) => out.push_str(&format!("{pad}s = s - {}\n", -d)),
            Node::IfC(body) => {
                out.push_str(&format!("{pad}if c {{\n"));
                written(body, indent + 2, counter, out);
                out.push_str(&format!("{pad}}}\n"));
            }
            Node::Rounds(k, body) => {
                *counter += 1;
                out.push_str(&format!("{pad}loop i{counter} in 0..{k} {{\n"));
                written(body, indent + 2, counter, out);
                out.push_str(&format!("{pad}}}\n"));
            }
        }
    }
}

/// Every value `s` can have after the nodes, from each of `from`, with each
/// `if` taken or not each time it is met, and in `all` every value it has at any
/// point on the way.
fn reachable(
    nodes: &[Node],
    from: BTreeSet<i128>,
    all: &mut BTreeSet<i128>,
) -> BTreeSet<i128> {
    let mut now = from;
    all.extend(now.iter().copied());
    for node in nodes {
        now = match node {
            Node::Change(d) => now.iter().map(|v| v + d).collect(),
            Node::IfC(body) => {
                let taken = reachable(body, now.clone(), all);
                now.union(&taken).copied().collect()
            }
            Node::Rounds(k, body) => {
                for _ in 0..*k {
                    now = reachable(body, now, all);
                }
                now
            }
        };
        all.extend(now.iter().copied());
    }
    now
}

/// For nested loops that add and take, with a branch in them, no use that
/// needs a range that leaves out a value the `mut` can end with is accepted:
/// the range inferred holds every value the `mut` has at any point.
fn no_range_inferred_for_generated_nested_loops_that_add_and_take_leaves_out_a_value_the_var_has()
 {
    // The whole of it is in a loop, so that what is known after it is the range
    // of the `mut` and not what its last assignment gave.
    let program_of = |g: &mut Gen| vec![Node::Rounds(1 + g.below(3), nodes(g, 2))];
    generate::check(9602, 30, program_of, |body| {
        let mut all = BTreeSet::new();
        let finals = reachable(body, [1000].into(), &mut all);
        let (least, most) = (*all.first().unwrap(), *all.last().unwrap());
        let mut text = String::new();
        written(body, 2, &mut 0, &mut text);
        let program = |lo: i128, hi: i128| {
            with_header(format!(
                "fn needs(v: {lo}..{hi}) -> {lo}..{hi} {{ v }}\n\n\
                 fn f(c: bool) -> Int {{\n  mut s = 1000\n{text}  needs(s)\n}}\n\n\
                 fn main() <console, alloc> {{\n  print(\"{{f(true)}} {{f(false)}}\")\n}}\n"
            ))
        };
        for (lo, hi) in [(least, most), (least + 1, most + 1)] {
            let source = program(lo, hi);
            assert!(
                check(source.as_bytes()).is_err(),
                "a use that needs {lo}..{hi} was accepted, and `s` can be {least} and {most}\n{source}"
            );
        }
        let source = program(least, most + 1);
        if let Ok(program) = lower(source.as_bytes()) {
            let out = scalar::run(&program);
            let said = String::from_utf8(out.stdout).expect("text");
            for value in said.split_whitespace() {
                let value: i128 = value.parse().expect("a number");
                assert!(finals.contains(&value), "{value} is not in {finals:?}\n{source}");
            }
        }
    });
}

/// A place that a list, a text, or a slice can be read from, with the program
/// that reads its size.
#[derive(Clone, Debug)]
struct Place {
    /// The declarations the expression needs, and the type of what it is read
    /// from.
    declarations: String,
    /// `f`'s parameter, if it takes one, and the lines before the expression.
    parameter: String,
    before: String,
    /// What is read, which has a `size`.
    expression: String,
    /// What `main` gives `f`, and the effects `f` has.
    argument: String,
    effects: &'static str,
    /// What `f` requires, a line, or none.
    requires: &'static str,
}

fn places(g: &mut Gen) -> Place {
    let ty = *g.pick(&["List[Nat]", "List[Byte]", "str", "List[str]", "Slice[Nat]"]);
    let make = |ty: &str| match ty {
        "str" => "\"abc\"".to_string(),
        "Slice[Nat]" => "freeze([1, 2])".to_string(),
        _ => "[]".to_string(),
    };
    let depth = 1 + g.below(2);
    match g.below(5) {
        // A field of a struct, and of a struct in a struct.
        0 => {
            let mut declarations = format!("struct S0 {{\n  items: {ty}\n}}\n");
            let (mut path, mut value) = ("b.items".to_string(), format!("S0(items: {})", make(ty)));
            for k in 1..depth {
                declarations.push_str(&format!("\nstruct S{k} {{\n  inner: S{}\n}}\n", k - 1));
                path = format!("b{}.items", ".inner".repeat(k));
                value = format!("S{k}(inner: {value})");
            }
            Place {
                declarations,
                parameter: format!("b: S{}", depth - 1),
                before: String::new(),
                expression: path,
                argument: value,
                effects: "<alloc>",
                requires: "",
            }
        }
        // An element of a list of them.
        1 => Place {
            declarations: String::new(),
            parameter: format!("xss: List[{ty}]"),
            before: "  if xss.size == 0 {\n    return 0\n  }\n".to_string(),
            expression: "xss[0]".to_string(),
            argument: format!("[{}]", make(ty)),
            effects: "<alloc>",
            requires: "",
        },
        // A name that a tuple was taken apart into.
        2 => Place {
            declarations: String::new(),
            parameter: format!("t: ({ty}, Nat)"),
            before: "  (xs, n) = t\n".to_string(),
            expression: "xs".to_string(),
            argument: format!("({}, 1)", make(ty)),
            effects: "<alloc>",
            requires: "",
        },
        // What a call gives.
        3 => Place {
            declarations: format!("fn make() -> {ty} <alloc> {{ {} }}\n", make(ty)),
            parameter: String::new(),
            before: String::new(),
            expression: "make()".to_string(),
            argument: String::new(),
            effects: "<alloc>",
            requires: "",
        },
        // A name.
        _ => Place {
            declarations: String::new(),
            parameter: format!("xs: {ty}"),
            before: String::new(),
            expression: "xs".to_string(),
            argument: make(ty),
            effects: "<alloc>",
            requires: "",
        },
    }
}

impl Place {
    fn program(&self, holds: &str) -> String {
        let imports = "use std.list (freeze)\n\n";
        with_header(format!(
            "{imports}{}\nfn f({}) -> {holds} {}\n{}{{\n{}  {}.size\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{f({})}}\")\n}}\n",
            self.declarations, self.parameter, self.effects, self.requires, self.before, self.expression, self.argument
        ))
    }
}

/// The size of a `List`, a `str`, or a `Slice` is below 2^48 wherever it is
/// read from: a name, a field of a struct, an element of a list, a name a
/// tuple was taken apart into, and what a call gives. A range that holds 2^48
/// less one is accepted, and the range of one less is refused.
fn the_size_of_a_list_a_text_and_a_slice_is_below_two_to_the_48_wherever_it_is_read_from() {
    generate::check(9603, 30, places, |place| {
        let source = place.program("0..281474976710656");
        assert!(check(source.as_bytes()).is_ok(), "{:?} for\n{source}", check(source.as_bytes()));
        let source = place.program("0..281474976710655");
        assert!(check(source.as_bytes()).is_err(), "accepted\n{source}");
    });
}

/// A field named `size` that holds a `Nat` is not the size of a list: nothing
/// shows that it is below 2^48.
fn a_field_named_size_that_is_not_the_size_of_a_list_is_not_below_two_to_the_48() {
    let source = with_header(
        "struct Sized {\n  size: Nat\n}\n\nfn f(s: Sized) -> 0..281474976710656 { s.size }\n\n\
         fn main() <console, alloc> {\n  print(\"{f(Sized(size: 3))}\")\n}\n"
            .to_string(),
    );
    assert!(check(source.as_bytes()).is_err(), "accepted\n{source}");
}

/// `loops` loops, each over an array and each adding to a `mut` of its own,
/// in one body.
fn many_loops(loops: usize) -> String {
    let mut out = String::from("fn f(xs: [4]Byte) -> Int {\n");
    for k in 0..loops {
        out.push_str(&format!("  mut a{k} = {k}\n  loop x in xs {{ a{k} = a{k} + x }}\n"));
    }
    out.push_str("  a0\n}\n\nfn main() <alloc> {\n  y = f([1, 2, 3, 4])\n}\n");
    with_header(out)
}

/// What checking `loops` loops costs, which the thread that checks them
/// computed for.
fn cost(loops: usize) -> f64 {
    let source = many_loops(loops);
    let start = crate::clock::thread_time();
    let result = check(source.as_bytes());
    let spent = crate::clock::thread_time() - start;
    assert!(result.is_ok(), "{result:?}");
    spent.as_secs_f64()
}

/// Five times the loops take about five times as long, far less than the
/// twenty-five times that looking at the body again for each loop would. The
/// bound leaves room for what a busy machine adds to a longer body.
fn the_time_to_check_loops_grows_with_their_number_and_no_faster() {
    let small = cost(60);
    let large = cost(300);
    assert!(
        large < small * 14.0,
        "60 loops took {small}s and 300 took {large}s, which is more than linear"
    );
}
