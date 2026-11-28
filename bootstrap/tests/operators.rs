//! Generators and properties of `+`, `-`, and `*` as calls of `Add.add`,
//! `Sub.sub`, and `Mul.mul`: a type of a program's own has an operator by an
//! `impl`, a generic function states what it needs of its operands, and a
//! whole number's operator in a generic function is the one instruction it
//! is anywhere. `/` and `%` are calls of `Div.div` and `Rem.rem` of whole
//! numbers, in a generic function and by name.
//!
//! Every generator works out the answer with the arithmetic of Rust on 128
//! bits, so it shares nothing with the compiler.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::contracts::near;
use crate::generate::{self, Gen};
use crate::numbers::{INTS, Int, with_header};

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
    (
        "div_and_rem_called_by_name_divide_an_int_and_a_byte",
        div_and_rem_called_by_name_divide_an_int_and_a_byte,
    ),
    (
        "a_quotient_or_what_is_left_over_through_its_trait_is_accepted_exactly_where_valid_holds_part_1_of_3",
        || a_quotient_or_what_is_left_over_through_its_trait_is_accepted_exactly_where_valid_holds(0),
    ),
    (
        "a_quotient_or_what_is_left_over_through_its_trait_is_accepted_exactly_where_valid_holds_part_2_of_3",
        || a_quotient_or_what_is_left_over_through_its_trait_is_accepted_exactly_where_valid_holds(1),
    ),
    (
        "a_quotient_or_what_is_left_over_through_its_trait_is_accepted_exactly_where_valid_holds_part_3_of_3",
        || a_quotient_or_what_is_left_over_through_its_trait_is_accepted_exactly_where_valid_holds(2),
    ),
    (
        "the_least_number_by_minus_one_a_divisor_of_zero_and_a_plain_pair_divide_exactly_where_valid_holds",
        the_least_number_by_minus_one_a_divisor_of_zero_and_a_plain_pair_divide_exactly_where_valid_holds,
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

/// `Div.div(7, 2)` is 3 and `Rem.rem(7, 2)` is 1, of an `Int` and of a `u8`.
fn div_and_rem_called_by_name_divide_an_int_and_a_byte() {
    let source = with_header(
        "fn main() <console, alloc> {\n  print(\"{Div.div(7, 2)} {Rem.rem(7, 2)}\")\n  a: Byte = 7\n  b: Byte = 2\n  print(\"{Div.div(a, b)} {Rem.rem(a, b)}\")\n}\n"
            .to_string(),
    );
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        scalar::run(&program),
        Outcome {
            stdout: b"3 1\n3 1\n".to_vec(),
            status: 0
        }
    );
}

#[derive(Debug)]
struct Divided {
    source: String,
    /// The same program with `+` in the place of the operator, and numbers
    /// whose sum fits.
    added: String,
    /// The instructions the operator takes beyond the one `+` takes.
    beyond: usize,
    /// The answer, where `valid` holds.
    answer: Option<i128>,
    /// A program that prints `valid` of the two numbers, run and not proven.
    asked: String,
}

/// A function that divides, or takes what is left over, of two numbers of a
/// type parameter bound by `Div` or `Rem`, with `requires` of its `valid`, and
/// a `main` that calls it, and the trait's function by name, with two whole
/// numbers of one type. The divisor is often 0, and often -1 with the least
/// number of its type as the dividend.
fn divided(g: &mut Gen) -> Divided {
    let ty = *g.pick(INTS);
    let quotient = g.one_in(2);
    let (a, b) = match g.below(4) {
        0 => (near(g, ty), 0),
        1 => (ty.min(), (-1).max(ty.min())),
        _ => (near(g, ty), near(g, ty)),
    };
    division(ty, quotient, (a, b))
}

/// The function over `/`, when `quotient`, or `%` of type `ty` and its call
/// with `a` and `b`, as `divided` says.
fn division(ty: Int, quotient: bool, (a, b): (i128, i128)) -> Divided {
    // The divisor is not zero, and the least number is not divided by -1
    // where the quotient is asked for or worked out at 64 bits.
    let least_by_minus_one = ty.signed && a == ty.min() && b == -1;
    let valid = b != 0 && !(least_by_minus_one && (quotient || ty.bits == 64));
    let answer = valid.then(|| if quotient { a / b } else { a % b });
    let (tr, function, sign) = if quotient {
        ("Div", "div", "/")
    } else {
        ("Rem", "rem", "%")
    };
    let t = ty.name;
    // `apply` is of the trait `of` and its operator, and `main` calls the
    // function of the operator by name.
    let program = |of: &str, sign: &str, (a, b): (i128, i128)| {
        with_header(format!(
            "fn apply[T: {of}](x: T, y: T) -> T\n  requires {of}.valid(x, y)\n{{\n  x {sign} y\n}}\n\nfn main() <console, alloc> {{\n  x: {t} = {a}\n  y: {t} = {b}\n  print(\"{{apply(x, y)}}\")\n  print(\"{{{tr}.{function}(x, y)}}\")\n}}\n"
        ))
    };
    Divided {
        source: program(tr, sign, (a, b)),
        added: program("Add", "+", (1, 1)),
        // A signed `%` below 64 bits is taken to twice the size and back.
        beyond: if !quotient && ty.signed && ty.bits < 64 { 3 } else { 0 },
        answer,
        asked: with_header(format!(
            "fn main() <console, alloc> {{\n  x: {t} = {a}\n  y: {t} = {b}\n  print(\"{{{tr}.valid(x, y)}}\")\n}}\n"
        )),
    }
}

/// For every whole number type and every pair the generator draws, a generic
/// function over `/` or `%` that asks `valid` of its caller, and the trait's
/// function called by name, are accepted exactly where `valid` holds, and
/// give what `a / b` and `a % b` give. The operator in the copy for the type
/// is one instruction, as `+` is, and no call, and a signed `%` of fewer than
/// 64 bits is taken to twice the size first, as `%` of whole numbers is.
fn a_quotient_or_what_is_left_over_through_its_trait_is_accepted_exactly_where_valid_holds(
    part: u64,
) {
    generate::check_part(223, 240, part, 3, divided, divides_exactly_where_valid_holds);
}

/// For every whole number type and each of `/` and `%`, the property of
/// `a_quotient_or_what_is_left_over_through_its_trait_is_accepted_exactly_where_valid_holds`
/// holds of the least number by -1, of a divisor of 0, and of `7` by `2`,
/// which a draw may miss: the least number by -1 is the one pair where `valid`
/// of a quotient and of a remainder differ, and differ by size.
fn the_least_number_by_minus_one_a_divisor_of_zero_and_a_plain_pair_divide_exactly_where_valid_holds()
{
    for ty in INTS {
        for quotient in [true, false] {
            for pair in [(ty.min(), (-1).max(ty.min())), (7, 0), (7, 2)] {
                divides_exactly_where_valid_holds(&division(*ty, quotient, pair));
            }
        }
    }
}

/// Whether `c` is accepted exactly where `valid` holds, prints the answer
/// twice, and is one instruction beyond its sum as `beyond` says, and whether
/// `valid`, run, gives what it is proven to be.
fn divides_exactly_where_valid_holds(c: &Divided) {
    let asked = lower(c.asked.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        scalar::run(&asked),
        Outcome {
            stdout: format!("{}\n", c.answer.is_some()).into_bytes(),
            status: 0
        }
    );
    match lower(c.source.as_bytes()) {
        Ok(program) => {
            let answer = c.answer.expect("accepted, though `valid` does not hold");
            assert_eq!(
                scalar::run(&program),
                Outcome {
                    stdout: format!("{answer}\n{answer}\n").into_bytes(),
                    status: 0
                }
            );
            let added = lower(c.added.as_bytes()).expect("lowers");
            let (functions, instructions) = size(&scalar::inline(added));
            assert_eq!(
                size(&scalar::inline(program)),
                (functions, instructions + c.beyond)
            );
        }
        Err(errors) => {
            assert!(c.answer.is_none(), "refused, though `valid` holds: {errors:?}");
            assert!(
                errors.iter().any(|e| e.message.contains("cannot prove")),
                "{errors:?} does not say what could not be proven"
            );
        }
    }
}
