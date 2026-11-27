//! The struct of the arguments of a function is given what a struct is given:
//! it is read from JSON, written to JSON, compared, and shown, with the fields
//! of the parameters in their order and a default where one was written. A
//! function with a `take` parameter has one, one with a pattern for a parameter
//! has none and is refused where `Function` is asked of it, and `call` holds no
//! copy of the function it calls.

use smith::diag::Error;
use smith::scalar::{self, Constant, Op, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "the_arguments_of_a_function_are_read_written_compared_and_shown_as_a_struct_is",
        the_arguments_of_a_function_are_read_written_compared_and_shown_as_a_struct_is,
    ),
    (
        "a_function_with_a_pattern_for_a_parameter_has_no_function_and_one_with_a_take_parameter_has_part_1_of_3",
        || a_function_with_a_pattern_for_a_parameter_has_no_function_and_one_with_a_take_parameter_has(0),
    ),
    (
        "a_function_with_a_pattern_for_a_parameter_has_no_function_and_one_with_a_take_parameter_has_part_2_of_3",
        || a_function_with_a_pattern_for_a_parameter_has_no_function_and_one_with_a_take_parameter_has(1),
    ),
    (
        "a_function_with_a_pattern_for_a_parameter_has_no_function_and_one_with_a_take_parameter_has_part_3_of_3",
        || a_function_with_a_pattern_for_a_parameter_has_no_function_and_one_with_a_take_parameter_has(2),
    ),
    (
        "call_holds_the_function_once_and_no_copy_of_it",
        call_holds_the_function_once_and_no_copy_of_it,
    ),
];

fn errors(source: &str) -> Vec<Error> {
    check(source.as_bytes()).err().unwrap_or_default()
}

fn outcome(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// The generic function that reads the arguments of a function from JSON and
/// says them in each form a struct has, and calls the function with them.
const RUN: &str = "fn run[F, A, R](f: F, text: str) -> str <alloc>\n  \
    requires Function[F, A, R]\n  requires FromJson[A]\n  requires Serialize[A]\n  \
    requires Eq[A]\n  requires Repr[A]\n  requires Discard[A]\n{\n  \
    r: Result[A, ReadError] = from_json(text)\n  match r {\n    \
    .ok(a) { \"{Repr.repr(a)} {to_json(a)} {Eq.eq(a, a)}\" }\n    .err(e) { \"none\" }\n  }\n}\n\n";

const USES: &str = "use std.console (console, print)\nuse std.deserialize (ReadError)\n\
    use std.json (FromJson, from_json, to_json)\nuse std.serialize (Serialize)\n\n";

/// The parameters of a function, each with its default if it has one and the
/// value the JSON gives it if it does.
#[derive(Clone, Debug)]
struct Params {
    defaults: Vec<Option<&'static str>>,
    said: Vec<Option<&'static str>>,
}

fn params(g: &mut Gen) -> Params {
    let words = ["a", "bb", "ccc", "d d"];
    let count = g.below(3) + 1;
    let with = g.below(count + 1);
    let defaults: Vec<Option<&'static str>> = (0..count)
        .map(|i| (i >= count - with).then(|| *g.pick(&words)))
        .collect();
    let said = (0..count)
        .map(|i| (defaults[i].is_none() || g.below(2) == 0).then(|| *g.pick(&words)))
        .collect();
    Params { defaults, said }
}

/// The arguments of a function of parameters `a0`, `a1`, ... are a struct of
/// those fields, in that order, read with the defaults of the function, written
/// as JSON, equal to themselves, and shown with the name of the function.
fn the_arguments_of_a_function_are_read_written_compared_and_shown_as_a_struct_is() {
    // Each program holds eight functions, one case each, and prints the line of
    // each, so that a program is checked twice and not sixteen times.
    generate::check(7931, 2, |g: &mut Gen| (0..8).map(|_| params(g)).collect::<Vec<_>>(), |batch| {
        let mut functions = String::new();
        let mut calls = Vec::new();
        let mut said = String::new();
        for (n, p) in batch.iter().enumerate() {
            let list: Vec<String> = (p.defaults.iter().enumerate())
                .map(|(i, d)| match d {
                    Some(d) => format!("a{i}: str = \"{d}\""),
                    None => format!("a{i}: str"),
                })
                .collect();
            let fields: Vec<String> = (p.said.iter().enumerate())
                .filter_map(|(i, v)| v.map(|v| format!("\\\"a{i}\\\": \\\"{v}\\\"")))
                .collect();
            functions += &format!("fn pick{n}({}) -> str {{ a0 }}\n\n", list.join(", "));
            calls.push(format!("  print(run(pick{n}, \"\\{{{}\\}}\"))\n", fields.join(", ")));
            let value = |i: usize| p.said[i].or(p.defaults[i]).expect("a value");
            let shown: Vec<String> = (0..p.said.len()).map(|i| format!("a{i}: \"{}\"", value(i))).collect();
            let written: Vec<String> = (0..p.said.len()).map(|i| format!("\"a{i}\":\"{}\"", value(i))).collect();
            said += &format!("pick{n}.Args({}) {{{}}} true\n", shown.join(", "), written.join(","));
        }
        let source = format!(
            "{USES}{functions}{RUN}fn main() <console, alloc> {{\n{}}}\n",
            calls.concat()
        );
        assert_eq!(outcome(&source).stdout, said.into_bytes(), "for\n{source}");
    });
}

/// A pattern for a parameter has no name for a field, so the function has no
/// `Function` and is one error where its name is given for one. A `take`
/// parameter is a field, and `call` spends it.
fn a_function_with_a_pattern_for_a_parameter_has_no_function_and_one_with_a_take_parameter_has(part: u64) {
    let program = |name: &str| {
        format!(
            "{USES}fn pat((a, b): (str, str)) -> str <alloc> {{ \"{{a}}{{b}}\" }}\n\n\
             fn spend(x: take str) -> str {{ x }}\n\n{RUN}\
             fn main() <console, alloc> {{\n  print(run({name}, \"\\{{\\\"x\\\": \\\"v\\\"\\}}\"))\n}}\n"
        )
    };
    // Each of the three is a part, so that no part compiles the same text twice.
    match part {
        0 => assert!(errors(&program("spend")).is_empty()),
        1 => assert_eq!(outcome(&program("spend")).stdout, b"spend.Args(x: \"v\") {\"x\":\"v\"} true\n"),
        _ => {
            let found = errors(&program("pat"));
            assert_eq!(found.len(), 1, "{found:?}");
            assert!(found[0].message.contains("`pat`") && found[0].message.contains("`Function`"));
        }
    }
}

/// The number the function multiplies by, to find it again.
const MARK: u64 = 7_654_321;

/// `call` is the call of the function: the function is in the built program
/// once, and what it does is not written again where `call` is.
fn call_holds_the_function_once_and_no_copy_of_it() {
    // One level of `call` and three.
    for through in &[1usize, 3] {
        let mut source = format!(
            "use std.console (console, print)\nuse std.deserialize (ReadError)\nuse std.int (wrapping_mul)\n\
             use std.json (FromJson, from_json)\n\nfn grow(a: Int) -> Int {{ wrapping_mul(a, {MARK}) }}\n\n\
             fn call0[F, A, R](f: F, a: take A) -> R\n  requires Function[F, A, R]\n{{\n  Function.call(f, a)\n}}\n\n"
        );
        for level in 1..*through {
            source += &format!(
                "fn call{level}[F, A, R](f: F, a: take A) -> R\n  requires Function[F, A, R]\n{{\n  call{}(f, a)\n}}\n\n",
                level - 1
            );
        }
        source += &format!(
            "fn run[F, A, R](f: F, text: str) -> Option[R] <alloc>\n  requires Function[F, A, R]\n  \
             requires FromJson[A]\n  requires Discard[A]\n{{\n  r: Result[A, ReadError] = from_json(text)\n  \
             match r {{\n    .ok(a) {{ Option.some(call{}(f, a)) }}\n    .err(e) {{ Option.none }}\n  }}\n}}\n\n\
             fn show(o: Option[Int]) -> str <alloc> {{\n  match o {{\n    .some(v) {{ \"{{v}}\" }}\n    .none {{ \"none\" }}\n  }}\n}}\n\n\
             fn main() <console, alloc> {{\n  print(show(run(grow, \"\\{{\\\"a\\\": 3\\}}\")))\n}}\n",
            through - 1
        );
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
        let holding = (program.functions.iter())
            .filter(|f| {
                (f.blocks.iter().flat_map(|b| &b.instructions))
                    .any(|i| i.op == Op::Constant(Constant::Int(MARK)))
            })
            .count();
        assert_eq!(holding, 1, "for\n{source}");
        assert_eq!(scalar::run(&program).stdout, format!("{}\n", 3 * MARK).into_bytes());
    }
}
