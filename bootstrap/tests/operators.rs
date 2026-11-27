//! Generators and properties of `+`, `-`, and `*` as calls of `Add.add`,
//! `Sub.sub`, and `Mul.mul`: a type of a program's own has an operator by an
//! `impl`, a generic function states what it needs of its operands, and a
//! whole number's operator in a generic function is the one instruction it
//! is anywhere.
//!
//! Every generator works out the answer with the arithmetic of Rust on 128
//! bits, so it shares nothing with the compiler.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::contracts::near;
use crate::generate::{self, Gen};
use crate::numbers::{INTS, with_header};

pub const ONCE: &[Test] = &[
    (
        "a_generic_function_over_an_operator_compiles_exactly_when_the_answer_fits_the_type_part_1_of_3",
        || a_generic_function_over_an_operator_compiles_exactly_when_the_answer_fits_the_type(0),
    ),
    (
        "a_generic_function_over_an_operator_compiles_exactly_when_the_answer_fits_the_type_part_2_of_3",
        || a_generic_function_over_an_operator_compiles_exactly_when_the_answer_fits_the_type(1),
    ),
    (
        "a_generic_function_over_an_operator_compiles_exactly_when_the_answer_fits_the_type_part_3_of_3",
        || a_generic_function_over_an_operator_compiles_exactly_when_the_answer_fits_the_type(2),
    ),
    (
        "an_operator_on_structs_gives_what_its_function_gives_by_its_name_part_1_of_2",
        || an_operator_on_structs_gives_what_its_function_gives_by_its_name(0),
    ),
    (
        "an_operator_on_structs_gives_what_its_function_gives_by_its_name_part_2_of_2",
        || an_operator_on_structs_gives_what_its_function_gives_by_its_name(1),
    ),
    (
        "an_operator_between_types_with_no_impl_is_one_error_that_names_the_trait_and_both_types",
        an_operator_between_types_with_no_impl_is_one_error_that_names_the_trait_and_both_types,
    ),
];

/// The three operators: the trait, the function, the sign, and what it does.
const OPERATORS: [(&str, &str, &str, &str); 3] = [
    ("Add", "add", "+", "wrapping_add"),
    ("Sub", "sub", "-", "wrapping_sub"),
    ("Mul", "mul", "*", "wrapping_mul"),
];

fn apply(op: usize, a: i128, b: i128) -> i128 {
    match op {
        0 => a + b,
        1 => a - b,
        _ => a.saturating_mul(b),
    }
}

#[derive(Debug)]
struct Generic {
    source: String,
    /// The same program with the operation written where it is used, as the
    /// function of the operator that wraps, which the proof makes it.
    direct: String,
    /// The answer, and whether it fits the type.
    answer: i128,
    fits: bool,
}

/// A generic function that states `requires` of its operator and uses it, and
/// a `main` that calls it with two whole numbers of one type.
fn generic(g: &mut Gen) -> Generic {
    let ty = *g.pick(INTS);
    let op = g.below(3);
    let (a, b) = (near(g, ty), near(g, ty));
    let answer = apply(op, a, b);
    let (tr, _, sign, wrapping) = OPERATORS[op];
    let t = ty.name;
    Generic {
        direct: with_header(format!(
            "fn main() <console, alloc> {{\n  x: {t} = {a}\n  y: {t} = {b}\n  print(\"{{{wrapping}(x, y)}}\")\n}}\n"
        )),
        source: with_header(format!(
            "fn apply[T: {tr}](x: T, y: T) -> T\n  requires {tr}.valid(x, y)\n{{\n  x {sign} y\n}}\n\nfn main() <console, alloc> {{\n  x: {t} = {a}\n  y: {t} = {b}\n  print(\"{{apply(x, y)}}\")\n}}\n"
        )),
        answer,
        fits: ty.min() <= answer && answer <= ty.max(),
    }
}

/// For every operator and every whole number type, a generic function that
/// asks of its caller that the answer fits is accepted exactly where it does,
/// and prints the answer, which the operator in its body works out in one
/// instruction.
fn a_generic_function_over_an_operator_compiles_exactly_when_the_answer_fits_the_type(part: u64) {
    // A program is accepted exactly when it is lowered, so it is checked once.
    generate::check_part(220, 150, part, 3, generic, |c| match lower(c.source.as_bytes()) {
        Ok(program) => {
            assert!(c.fits, "accepted, though the answer does not fit");
            assert_eq!(
                scalar::run(&program),
                Outcome {
                    stdout: format!("{}\n", c.answer).into_bytes(),
                    status: 0
                }
            );
            // The operator is the one instruction, and no call is left.
            let direct = lower(c.direct.as_bytes()).expect("lowers");
            assert_eq!(
                size(&scalar::inline(program)),
                size(&scalar::inline(direct))
            );
        }
        Err(errors) => {
            assert!(!c.fits, "refused, though the answer fits: {errors:?}");
            assert!(
                errors.iter().any(|e| e.message.contains("cannot prove")),
                "{errors:?} does not say what could not be proven"
            );
        }
    });
}

/// How many functions and instructions a program holds.
fn size(program: &scalar::Program) -> (usize, usize) {
    let instructions = (program.functions.iter())
        .flat_map(|f| &f.blocks)
        .map(|b| b.instructions.len())
        .sum();
    (program.functions.len(), instructions)
}

#[derive(Debug)]
struct Structs {
    source: String,
    /// What `main` prints twice, once with the operator and once with the
    /// function called by its name.
    prints: String,
}

/// Three structs of `n` whole numbers, an `impl` of one operator for the first
/// two with the third as the result, which works each field out with the
/// function of the operator that wraps, and a `main` that prints each field of
/// the answer with the operator and with the function.
fn structs(g: &mut Gen) -> Structs {
    let op = g.below(3);
    let (tr, function, sign, wrapping) = OPERATORS[op];
    let n = 1 + g.below(3);
    let same = g.one_in(2);
    let (second, result) = if same { ("A", "A") } else { ("B", "C") };
    let mut source = String::from("use std.bits (i64)\nuse std.int (wrapping_add, wrapping_sub, wrapping_mul)\n\n");
    let names = if same { vec!["A"] } else { vec!["A", "B", "C"] };
    for name in &names {
        source.push_str(&format!("struct {name} {{\n"));
        for i in 0..n {
            source.push_str(&format!("  f{i}: i64\n"));
        }
        source.push_str("}\n\n");
    }
    let fields: Vec<String> = (0..n)
        .map(|i| format!("f{i}: {wrapping}(a.f{i}, b.f{i})"))
        .collect();
    let impl_for = if same {
        format!("{tr}[A]")
    } else {
        format!("{tr}[A, B, C]")
    };
    source.push_str(&format!(
        "impl {impl_for} {{\n  fn valid(a: A, b: {second}) -> bool {{ true }}\n  fn {function}(a: A, b: {second}) -> {result} {{ {result}({}) }}\n}}\n\n",
        fields.join(", ")
    ));
    let (x, y): (Vec<i128>, Vec<i128>) = (0..n)
        .map(|_| (i128::from(g.below(1000) as i64 - 500), i128::from(g.below(1000) as i64 - 500)))
        .unzip();
    let build = |name: &str, values: &[i128]| {
        let parts: Vec<String> = values
            .iter()
            .enumerate()
            .map(|(i, v)| format!("f{i}: {v}"))
            .collect();
        format!("{name}({})", parts.join(", "))
    };
    source.push_str("fn main() <console, alloc> {\n");
    source.push_str(&format!("  x = {}\n  y = {}\n", build("A", &x), build(second, &y)));
    source.push_str(&format!("  by_operator = x {sign} y\n  by_name = {tr}.{function}(x, y)\n"));
    let mut prints = String::new();
    for i in 0..n {
        source.push_str(&format!("  print(\"{{by_operator.f{i}}}\")\n"));
    }
    for i in 0..n {
        source.push_str(&format!("  print(\"{{by_name.f{i}}}\")\n"));
    }
    source.push_str("}\n");
    for _ in 0..2 {
        for i in 0..n {
            prints.push_str(&format!("{}\n", apply(op, x[i], y[i])));
        }
    }
    Structs {
        source: with_header(source),
        prints,
    }
}

/// For every struct a generator builds with an `impl` of `Add`, `Sub`, or
/// `Mul`, of one type or of two types with a result of a third, the operator
/// gives what the function gives when it is called by its name.
fn an_operator_on_structs_gives_what_its_function_gives_by_its_name(part: u64) {
    generate::check_part(221, 60, part, 2, structs, |c| {
        let program = lower(c.source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            scalar::run(&program),
            Outcome {
                stdout: c.prints.clone().into_bytes(),
                status: 0
            }
        );
    });
}

/// Two structs, with no `impl` of the operator, and an expression that uses it.
fn no_impl(g: &mut Gen) -> (String, usize, [&'static str; 2]) {
    let op = g.below(3);
    let names = [["Cat", "Dog"], ["Left", "Right"], ["Rock", "Tree"]];
    let names = names[g.below(names.len())];
    let sign = OPERATORS[op].2;
    let source = format!(
        "struct {a} {{\n  x: str\n}}\n\nstruct {b} {{\n  x: str\n}}\n\nfn main() {{\n  p = {a}(x: \"a\")\n  q = {b}(x: \"b\")\n  r = p {sign} q\n}}\n",
        a = names[0],
        b = names[1]
    );
    (source, op, names)
}

/// For every operator between two types that no `impl` relates, the error is
/// one, and it names the trait and both types.
fn an_operator_between_types_with_no_impl_is_one_error_that_names_the_trait_and_both_types() {
    generate::check(222, 40, no_impl, |(source, op, names)| {
        let errors = check(source.as_bytes()).expect_err("refused");
        assert_eq!(errors.len(), 1, "{errors:?}");
        let message = &errors[0].message;
        for needed in [OPERATORS[*op].0, names[0], names[1]] {
            assert!(message.contains(needed), "{message} does not name {needed}");
        }
    });
}
