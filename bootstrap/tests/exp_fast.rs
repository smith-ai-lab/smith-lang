//! The fast way to the exponential of a float, which says whether it can tell
//! the float nearest the exact one, gives that float whenever it says so: a
//! program made of the two functions of the runtime, which works out the fast
//! one of each of a great many floats and, where it says it is sure, the exact
//! one, and counts the floats where the two differ. The count is none.

use std::cell::RefCell;

use smith::scalar::runtime::{self, Runtime};
use smith::scalar::{
    self, Binary, Bits, Builder, Compare, Constant, Function, FloatBinary, FloatBits, Op, Program,
    Type, Value,
};
use smith::target::Target;

use crate::Test;
use crate::targets::{fingerprinting, record, run, runnable, scalar_runs_as};

pub const ONCE: &[Test] = &[
    (
        "the_fast_exponential_of_an_f32_is_the_exact_one_wherever_it_is_sure_on_the_evaluator",
        || the_fast_exponential_is_the_exact_one_wherever_it_is_sure_on_the_evaluator(false),
    ),
    (
        "the_fast_exponential_of_an_f64_is_the_exact_one_wherever_it_is_sure_on_the_evaluator",
        || the_fast_exponential_is_the_exact_one_wherever_it_is_sure_on_the_evaluator(true),
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "the_fast_exponential_of_an_f32_is_the_exact_one_wherever_it_is_sure_in_every_executable",
        || the_fast_exponential_is_the_exact_one_wherever_it_is_sure_in_every_executable(false),
    ),
    (
        "the_fast_exponential_of_an_f64_is_the_exact_one_wherever_it_is_sure_in_every_executable",
        || the_fast_exponential_is_the_exact_one_wherever_it_is_sure_in_every_executable(true),
    ),
];

const LONG: Type = Type::Int(Bits::B64);
const BYTE: Type = Type::Int(Bits::B8);

/// The place of a function of the runtime among those the program holds,
/// which are made as they are asked for.
fn place(kind: Runtime, held: &RefCell<Vec<(Runtime, Option<Function>)>>) -> usize {
    if let Some(at) = held.borrow().iter().position(|(k, _)| *k == kind) {
        return at;
    }
    let at = held.borrow().len();
    held.borrow_mut().push((kind.clone(), None));
    let function = runtime::build(kind, &mut |k| place(k, held));
    held.borrow_mut()[at].1 = Some(function);
    at
}

fn long(b: &mut Builder, n: u64) -> Value {
    b.int(Bits::B64, n)
}

fn double(b: &mut Builder, v: f64) -> Value {
    b.float(FloatBits::F64, v.to_bits())
}

/// A program that works out `count` floats, each drawn from a generator of the
/// bits and kept in the range where the result is a normal number: of every
/// size of exponent, most of them below 2^-5 or near a multiple of `ln 2 / 64`,
/// where the reduction is least and where it is most careful. For each, it works
/// out the fast function, and where that says it is sure, the exact function
/// too. It writes three numbers of 8 bytes: how many floats, how many the fast
/// function was sure of, and how many of those it gave another float than the
/// exact function.
fn program(is_double: bool, count: u64, seed: u64) -> Program {
    let held: RefCell<Vec<(Runtime, Option<Function>)>> = RefCell::new(Vec::new());
    let fast = place(Runtime::ExpFast { double: is_double }, &held);
    let exact = place(Runtime::ExpExact { double: is_double }, &held);
    let size = if is_double { FloatBits::F64 } else { FloatBits::F32 };
    let float = Type::Float(size);
    let limit = if is_double { 700.0 } else { 86.0 };

    let mut b = Builder::new();
    let (head, p) = b.block(&[LONG, LONG, LONG, LONG]);
    let (body, _) = b.block(&[]);
    let (finish, fin) = b.block(&[LONG, LONG]);
    let zero = long(&mut b, 0);
    let start = long(&mut b, seed | 1);
    b.jump(head, vec![zero, start, zero, zero]);
    b.at(head);
    let (i, s, sure, wrong) = (p[0], p[1], p[2], p[3]);
    let total = long(&mut b, count);
    let more = b.compare(Compare::LessU, i, total);
    b.switch(more, vec![(finish, vec![sure, wrong]), (body, vec![])]);
    b.at(body);
    // xorshift64*.
    let twelve = long(&mut b, 12);
    let t = b.binary(Binary::ShiftRightU, s, twelve);
    let s1 = b.binary(Binary::Xor, s, t);
    let twenty_five = long(&mut b, 25);
    let t = b.binary(Binary::ShiftLeft, s1, twenty_five);
    let s2 = b.binary(Binary::Xor, s1, t);
    let twenty_seven = long(&mut b, 27);
    let t = b.binary(Binary::ShiftRightU, s2, twenty_seven);
    let s3 = b.binary(Binary::Xor, s2, t);
    let multiplier = long(&mut b, 0x2545_f491_4f6c_dd1d);
    let r = b.binary(Binary::Mul, s3, multiplier);
    // A float in [-1, 1) from the top 53 bits.
    let eleven = long(&mut b, 11);
    let top = b.binary(Binary::ShiftRightU, r, eleven);
    let whole = b.float_from_int(false, top, FloatBits::F64);
    let unit = double(&mut b, 2f64.powi(-53));
    let u = b.float_binary(FloatBinary::Mul, whole, unit);
    let two = double(&mut b, 2.0);
    let doubled = b.float_binary(FloatBinary::Mul, u, two);
    let one = double(&mut b, 1.0);
    let v = b.float_binary(FloatBinary::Sub, doubled, one);
    // The size of the numbers: all of the range, or a few.
    let three = long(&mut b, 3);
    let mode = b.binary(Binary::And, s3, three);
    let mut scale = double(&mut b, 40.0);
    for (number, size) in [(0u64, limit), (1, 8.0), (2, 0.02)] {
        let k = long(&mut b, number);
        let is = b.compare(Compare::Equal, mode, k);
        let value = double(&mut b, size);
        scale = b.choose(is, value, scale);
    }
    let wide = b.float_binary(FloatBinary::Mul, v, scale);
    // On half of them, the float is put beside a multiple of `ln 2 / 64`.
    let step = double(&mut b, std::f64::consts::LN_2 / 64.0);
    let over = b.float_binary(FloatBinary::Div, wide, step);
    let whole_steps = b.int_from_float(true, over, Bits::B64);
    let as_float = b.float_from_int(true, whole_steps, FloatBits::F64);
    let on_step = b.float_binary(FloatBinary::Mul, as_float, step);
    let small = double(&mut b, 1e-9);
    let spread = b.float_binary(FloatBinary::Mul, u, small);
    let beside = b.float_binary(FloatBinary::Add, on_step, spread);
    let four = long(&mut b, 4);
    let bit = b.binary(Binary::And, s3, four);
    let flag = b.compare(Compare::Equal, bit, four);
    let chosen = b.choose(flag, beside, wide);
    let x = if is_double {
        chosen
    } else {
        b.float_resize(chosen, FloatBits::F32)
    };
    let table = b.emit(Op::Constant(Constant::Data(0)), &[Type::Address])[0];
    let result = b.call(fast, vec![x, table], &[float, BYTE]);
    let (value, certain) = (result[0], result[1]);
    let one_long = long(&mut b, 1);
    let next = b.binary(Binary::Add, i, one_long);
    let (check, _) = b.block(&[]);
    let (join, q) = b.block(&[LONG, LONG]);
    b.switch(certain, vec![(join, vec![sure, wrong]), (check, vec![])]);
    b.at(check);
    let right = b.call(exact, vec![x], &[float]);
    let ours = b.float_bits(value);
    let theirs = b.float_bits(right[0]);
    let same = b.compare(Compare::Equal, ours, theirs);
    let sure_one = b.binary(Binary::Add, sure, one_long);
    let wrong_one = b.binary(Binary::Add, wrong, one_long);
    let wrong_next = b.choose(same, wrong, wrong_one);
    b.jump(join, vec![sure_one, wrong_next]);
    b.at(join);
    b.jump(head, vec![next, s3, q[0], q[1]]);
    b.at(finish);
    let size_of = long(&mut b, 24);
    let out = b.emit(
        Op::System {
            call: scalar::System::Allocate,
            args: vec![size_of],
        },
        &[Type::Address],
    )[0];
    let eight = long(&mut b, 8);
    let second = b.binary(Binary::Add, out, eight);
    let sixteen = long(&mut b, 16);
    let third = b.binary(Binary::Add, out, sixteen);
    b.store(out, total);
    b.store(second, fin[0]);
    b.store(third, fin[1]);
    let write = scalar::System::Write(scalar::Stream::Stdout);
    b.emit(
        Op::System {
            call: write,
            args: vec![out, size_of],
        },
        &[],
    );
    b.ret(Vec::new());
    let main = b.finish(&[]);
    let mut functions: Vec<Function> = held
        .into_inner()
        .into_iter()
        .map(|(_, f)| f.expect("built"))
        .collect();
    functions.push(main);
    let main = functions.len() - 1;
    Program {
        data: vec![smith::scalar::exp_table()],
        functions,
        main,
    }
}

/// How many floats, how many the fast function was sure of, and how many of
/// those it got wrong, as the program writes them.
fn counts(bytes: &[u8]) -> (u64, u64, u64) {
    let word = |k: usize| u64::from_le_bytes(bytes[8 * k..8 * k + 8].try_into().unwrap());
    assert_eq!(bytes.len(), 24, "{bytes:?}");
    (word(0), word(1), word(2))
}

/// The count that a run makes: 20 for the evaluator, which works out the exact
/// function slowly, and what `SMITH_EXP_FAST_INPUTS` says, or 200000, for an
/// executable.
fn how_many() -> u64 {
    std::env::var("SMITH_EXP_FAST_INPUTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200_000)
}

fn the_fast_exponential_is_the_exact_one_wherever_it_is_sure_on_the_evaluator(is_double: bool) {
    let program = program(is_double, 40, 12345);
    assert_eq!(scalar::check(&program), Ok(()));
    let said = scalar::run(&program);
    assert_eq!(said.status, 0);
    let (total, sure, wrong) = counts(&said.stdout);
    assert_eq!(total, 40);
    assert!(sure >= 38, "the fast function was sure of {sure} of 40");
    assert_eq!(wrong, 0, "the fast function gave another float than the exact one");
}

/// Every executable that this machine runs gives the counts the evaluator
/// gives for the same floats; and, on the machine itself, the fast function is
/// held to the exact one on as many floats as `SMITH_EXP_FAST_INPUTS` says,
/// 200000 when it says nothing, which it is sure of nearly all, and gives
/// none of another float.
fn the_fast_exponential_is_the_exact_one_wherever_it_is_sure_in_every_executable(
    is_double: bool,
) {
    let small = program(is_double, 40, 12345);
    let said = scalar::run(&small);
    for target in runnable() {
        scalar_runs_as(&small, target, &said);
    }
    let count = how_many();
    let seed = std::env::var("SMITH_EXP_FAST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(987_654_321);
    let program = program(is_double, count, seed);
    let Some(target) = Target::host().filter(|t| t.builds_floats()) else {
        return;
    };
    let bytes = target.emit(&program);
    record(target, &[&bytes]);
    if fingerprinting() {
        return;
    }
    let ran = run(&bytes);
    assert_eq!(ran.status, 0);
    let (total, sure, wrong) = counts(&ran.stdout);
    assert_eq!(total, count);
    assert_eq!(wrong, 0, "of {total} floats, {wrong} were wrong in the {sure} it was sure of");
    // The fast function is sure of nearly all: about 1 in 300000 it is not.
    assert!(sure * 1000 >= total * 999, "it was sure of {sure} of {total}");
    if count > 200_000 {
        eprintln!("{total} floats, {sure} sure, {wrong} wrong");
    }
}
