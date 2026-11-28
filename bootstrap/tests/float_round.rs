//! `floor`, `ceil`, `trunc`, and `round` of `std.float` round a float to a
//! whole number that is still a float, as `float.md` says: each gives what
//! Rust's `floor`, `ceil`, `trunc`, and `round_ties_even` give, bit for bit,
//! a float that is not a number being any such float, in the scalar form and
//! through the library, and every executable prints what the evaluator prints.
//!
//! The floats tried are those where rounding is hard: the halves and the
//! floats beside them, the zeros of both signs and the floats that round to
//! them, the subnormals, the greatest floats that are not whole and the least
//! that are, the greatest finite float, the infinities, and floats that are
//! not numbers, with floats of any bits and floats with a fraction drawn
//! besides.

use smith::lower;
use smith::scalar::{self, Builder, FloatBits, Program, Rounding, Type};
use smith::target::Target;

use crate::Test;
use crate::float_form as ff;
use crate::generate::{self, Gen};
use crate::numbers::with_header;
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "each_rounding_of_a_float_in_the_form_is_what_rust_gives_bit_for_bit",
        each_rounding_of_a_float_in_the_form_is_what_rust_gives_bit_for_bit,
    ),
    (
        "floor_ceil_trunc_and_round_of_an_f32_are_what_rust_gives",
        || roundings_are_what_rust_says(FloatBits::F32, 2),
    ),
    (
        "floor_ceil_trunc_and_round_of_an_f64_are_what_rust_gives",
        || roundings_are_what_rust_says(FloatBits::F64, 3),
    ),
    (
        "round_of_a_half_goes_to_the_even_whole_number_and_floor_goes_below",
        round_of_a_half_goes_to_the_even_whole_number_and_floor_goes_below,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "every_executable_prints_the_floor_ceil_trunc_and_round_of_an_f32_that_the_evaluator_prints",
        || roundings_on_every_executable(FloatBits::F32, 2),
    ),
    (
        "every_executable_prints_the_floor_ceil_trunc_and_round_of_an_f64_that_the_evaluator_prints",
        || roundings_on_every_executable(FloatBits::F64, 3),
    ),
];

/// The name of each rounding in `std.float`.
fn name(mode: Rounding) -> &'static str {
    match mode {
        Rounding::Floor => "floor",
        Rounding::Ceil => "ceil",
        Rounding::Trunc => "trunc",
        Rounding::Nearest => "round",
    }
}

/// What Rust gives for the rounding of the float of the size.
fn rust(mode: Rounding, bits: FloatBits, x: u64) -> u64 {
    match bits {
        FloatBits::F32 => {
            let a = f32::from_bits(x as u32);
            u64::from(
                match mode {
                    Rounding::Floor => a.floor(),
                    Rounding::Ceil => a.ceil(),
                    Rounding::Trunc => a.trunc(),
                    Rounding::Nearest => a.round_ties_even(),
                }
                .to_bits(),
            )
        }
        FloatBits::F64 => {
            let a = f64::from_bits(x);
            match mode {
                Rounding::Floor => a.floor(),
                Rounding::Ceil => a.ceil(),
                Rounding::Trunc => a.trunc(),
                Rounding::Nearest => a.round_ties_even(),
            }
            .to_bits()
        }
    }
}

/// The float of the size nearest the number, which the floats tried here hold
/// exactly.
fn of(bits: FloatBits, x: f64) -> u64 {
    match bits {
        FloatBits::F32 => u64::from((x as f32).to_bits()),
        FloatBits::F64 => x.to_bits(),
    }
}

/// The floats where rounding is hard, then floats of any bits and floats with
/// a fraction, drawn.
fn numbers(bits: FloatBits, g: &mut Gen) -> Vec<u64> {
    let sign = match bits {
        FloatBits::F32 => 1u64 << 31,
        FloatBits::F64 => 1u64 << 63,
    };
    // The bits of the fraction: from 2^this on, every float is whole.
    let fraction = match bits {
        FloatBits::F32 => 23,
        FloatBits::F64 => 52,
    };
    let mut all = ff::edges(bits);
    let mut hard = Vec::new();
    // The halves, which `round` takes to the even one of two, and the
    // quarters, of which the other sign is tried below.
    for k in 0..4 {
        hard.push(of(bits, f64::from(k) + 0.5));
    }
    hard.extend([of(bits, 0.25), of(bits, 0.75)]);
    let power = 2f64.powi(fraction);
    for x in [
        // The greatest float that is not whole, the greatest of a half, and the
        // least that is whole from where every float is.
        power - 0.5,
        power / 2.0 - 0.5,
        power / 2.0 + 0.5,
        power - 1.5,
        power,
        power + 1.0,
        power * 2.0,
        power * 2.0 + 2.0,
    ] {
        hard.push(of(bits, x));
    }
    // The least subnormal, the greatest, the least normal float, and the
    // greatest finite float.
    let top = match bits {
        FloatBits::F32 => u64::from(f32::MAX.to_bits()),
        FloatBits::F64 => f64::MAX.to_bits(),
    };
    hard.extend([1, (1 << fraction) - 1, 1 << fraction, top]);
    for x in hard {
        // The float, the floats on either side of it, and each of those of the
        // other sign.
        for y in [x, x + 1, x.wrapping_sub(1)] {
            all.extend([y, y ^ sign]);
        }
    }
    for _ in 0..20 {
        all.push(ff::float(g, bits));
        // A float of up to 2^20 with 10 bits of fraction, of either sign.
        let n = g.below(1 << 30) as f64 / 1024.0;
        all.push(of(bits, if g.below(2) == 0 { n } else { -n }));
    }
    all
}

fn each_rounding_of_a_float_in_the_form_is_what_rust_gives_bit_for_bit() {
    for (seed, bits) in [(0, FloatBits::F32), (1, FloatBits::F64)] {
        let ty = Type::Float(bits);
        let mut b = Builder::new();
        let v = b.params(&[ty]);
        let results = Rounding::ALL.map(|mode| b.float_round(mode, v[0])).to_vec();
        b.ret(results);
        let program = Program {
            data: Vec::new(),
            functions: vec![b.finish(&[ty; 4])],
            main: 0,
        };
        for x in generate::sample(seed, 0, |g| numbers(bits, g)) {
            let got = scalar::call(&program, 0, &[x]);
            for (mode, got) in Rounding::ALL.iter().zip(got) {
                let want = rust(*mode, bits, x);
                assert!(
                    ff::same(bits, got, want),
                    "{} of {x:#x} for {bits:?}: {got:#x} for {want:#x}",
                    name(*mode)
                );
            }
        }
    }
}

/// How a float is written in a program: as the shortest text that reads back
/// as it, or as the division that gives it.
fn written(bits: FloatBits, x: u64) -> String {
    let x = ff::value(bits, x);
    if x.is_nan() {
        "(0.0 / 0.0)".to_string()
    } else if x == f64::INFINITY {
        "(1.0 / 0.0)".to_string()
    } else if x == f64::NEG_INFINITY {
        "(-1.0 / 0.0)".to_string()
    } else {
        match bits {
            FloatBits::F32 => format!("{:e}", x as f32),
            FloatBits::F64 => format!("{x:e}"),
        }
    }
}

/// The program that prints, for each float, its four roundings, and then for
/// each whether it is the float Rust gives: the same number with the same
/// sign, or a float that is not a number where Rust gives one. What the program
/// must print on the second line of each float is `true` four times.
fn rounding_program(bits: FloatBits, g: &mut Gen) -> String {
    let t = match bits {
        FloatBits::F32 => "f32",
        FloatBits::F64 => "f64",
    };
    let values = numbers(bits, g);
    let mut source = format!(
        "use std.float (floor, ceil, trunc, round)\n\n\
         fn same(a: {t}, b: {t}) -> bool {{\n  \
           (a == b and 1.0 / a == 1.0 / b) or (a != a and b != b)\n\
         }}\n\n\
         fn main() <console, alloc> {{\n  xs: List[{t}] = [{}]\n",
        values.iter().map(|x| written(bits, *x)).collect::<Vec<_>>().join(", ")
    );
    for (i, x) in values.iter().enumerate() {
        let texts: Vec<String> = Rounding::ALL
            .iter()
            .map(|mode| format!("{{{}(xs[{i}])}}", name(*mode)))
            .collect();
        source += &format!("  print(\"{}\")\n", texts.join(" "));
        let checks: Vec<String> = Rounding::ALL
            .iter()
            .map(|mode| {
                let want = written(bits, rust(*mode, bits, *x));
                format!("{{same({}(xs[{i}]), {want})}}", name(*mode))
            })
            .collect();
        source += &format!("  print(\"{}\")\n", checks.join(" "));
    }
    source += "}\n";
    with_header(source)
}

fn roundings_are_what_rust_says(bits: FloatBits, seed: u64) {
    let source = generate::sample(seed, 0, |g| rounding_program(bits, g));
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    let printed = String::from_utf8_lossy(&outcome.stdout).to_string();
    let lines: Vec<&str> = printed.lines().collect();
    let count = generate::sample(seed, 0, |g| numbers(bits, g)).len();
    assert_eq!(lines.len(), 2 * count, "for {source}");
    for (k, pair) in lines.chunks(2).enumerate() {
        assert_eq!(pair[1], "true true true true", "{} is not what Rust gives at {k} for {source}", pair[0]);
    }
}

/// The examples of `float.md` and the issue, as the evaluator prints them.
fn round_of_a_half_goes_to_the_even_whole_number_and_floor_goes_below() {
    let source = with_header(
        "use std.float (floor, ceil, trunc, round)\n\n\
         fn main() <console, alloc> {\n  \
           x: f64 = -2.5\n  \
           print(\"{floor(x)} {ceil(x)} {trunc(x)} {round(x)}\")\n  \
           print(\"{round(2.5)} {round(3.5)} {floor(-0.0)} {ceil(-0.5)}\")\n  \
           y: f32 = -2.5\n  \
           print(\"{floor(y)} {ceil(y)} {trunc(y)} {round(y)}\")\n\
         }\n"
        .to_string(),
    );
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(
        String::from_utf8_lossy(&outcome.stdout),
        "-3.0 -2.0 -2.0 -2.0\n2.0 4.0 -0.0 -0.0\n-3.0 -2.0 -2.0 -2.0\n"
    );
}

/// Every executable prints what the evaluator prints, which the tests above
/// hold to what Rust gives.
fn roundings_on_every_executable(bits: FloatBits, seed: u64) {
    let source = generate::sample(seed, 0, |g| rounding_program(bits, g));
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    for target in runnable().into_iter().filter(|t: &Target| t.builds_floats()) {
        runs_as(source.as_bytes(), target, &outcome);
    }
}
