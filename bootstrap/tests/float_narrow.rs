//! `clamp(x)` and `narrow(x)` of a float to a range of whole numbers, as
//! `arithmetic.md` says from "`clamp(x)` of a float" on: `clamp` gives the
//! whole number nearest the float, the even one at a half, the least of the
//! range below it and the greatest above it, as for an infinity, and the
//! greatest for a float that is not a number; `narrow` gives `.some` exactly
//! when the float is a whole number of the range. What each must give comes
//! from Rust, and the evaluator, each executable for arm64 on the machine
//! `aarch64`, and each executable this machine runs print it.
//!
//! The ranges tried are those of each size, signed and not, a few small ones,
//! and some drawn; the floats are, for each end of the range, the end, the
//! whole numbers beside it, the halves on either side, and the floats next to
//! it, and then the halves near zero, both zeros, both infinities, a float that
//! is not a number, and floats drawn.

use smith::lower;
use smith::scalar::{self, FloatBits};
use smith::target::Target;

use crate::Test;
use crate::aarch64;
use crate::float_form as ff;
use crate::generate::{self, Gen};
use crate::numbers::with_header;
use crate::targets::{runnable, scalar_runs_as};

pub const ONCE: &[Test] = &[
    (
        "clamp_and_narrow_of_an_f32_give_what_rust_says_in_the_evaluator_and_on_arm64",
        || clamp_and_narrow_in_the_evaluator_and_on_arm64(FloatBits::F32, 1301),
    ),
    (
        "clamp_and_narrow_of_an_f64_give_what_rust_says_in_the_evaluator_and_on_arm64",
        || clamp_and_narrow_in_the_evaluator_and_on_arm64(FloatBits::F64, 1302),
    ),
    (
        "clamp_of_a_float_rounds_a_half_to_the_even_one_and_narrow_takes_only_a_whole_number",
        clamp_of_a_float_rounds_a_half_to_the_even_one_and_narrow_takes_only_a_whole_number,
    ),
    (
        "clamp_of_a_float_to_a_range_of_more_than_64_bits_is_refused",
        clamp_of_a_float_to_a_range_of_more_than_64_bits_is_refused,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "every_executable_prints_the_clamp_and_narrow_of_an_f32_that_the_evaluator_prints",
        || clamp_and_narrow_on_every_executable(FloatBits::F32, 1303),
    ),
    (
        "every_executable_prints_the_clamp_and_narrow_of_an_f64_that_the_evaluator_prints",
        || clamp_and_narrow_on_every_executable(FloatBits::F64, 1304),
    ),
];

/// The float of the size nearest `x`, as an `f64`, which holds it exactly.
fn sized(bits: FloatBits, x: f64) -> f64 {
    match bits {
        FloatBits::F32 => f64::from(x as f32),
        FloatBits::F64 => x,
    }
}

/// The floats of the size on either side of the one nearest `x`.
fn beside(bits: FloatBits, x: f64) -> [f64; 2] {
    match bits {
        FloatBits::F32 => {
            let f = x as f32;
            [f64::from(f.next_down()), f64::from(f.next_up())]
        }
        FloatBits::F64 => [x.next_down(), x.next_up()],
    }
}

/// What `clamp` of the float to `least..=greatest` gives, as Rust says: a
/// cast of a whole float to `i128` is exact for every float of the ranges
/// tried, and holds an infinity to the ends of `i128`, past every range.
fn clamped(x: f64, (least, greatest): (i128, i128)) -> i128 {
    if x.is_nan() {
        greatest
    } else {
        (x.round_ties_even() as i128).clamp(least, greatest)
    }
}

/// What `narrow` of the float to `least..=greatest` gives, as Rust says.
fn narrowed(x: f64, (least, greatest): (i128, i128)) -> Option<i128> {
    let whole = x.is_finite() && x.trunc() == x;
    let n = x as i128;
    (whole && (least..=greatest).contains(&n)).then_some(n)
}

/// The ranges tried: of each size, signed and not, three small ones, and
/// three drawn, one of them of numbers from 2^63 up.
fn ranges(g: &mut Gen) -> Vec<(i128, i128)> {
    let mut all = vec![(0, 499), (-3, 3), (5, 5)];
    for size in [8, 16, 32, 64] {
        all.push((0, (1i128 << size) - 1));
        all.push((-(1i128 << (size - 1)), (1i128 << (size - 1)) - 1));
    }
    let signed = |g: &mut Gen| {
        let n = g.below(1 << 31) as i128 * g.below(1 << 31) as i128 * 2;
        if g.one_in(2) { n } else { -n }
    };
    let (a, b) = (signed(g), signed(g));
    all.push((a.min(b), a.max(b)));
    let (a, b) = (g.below(1 << 20) as i128, g.below(1 << 20) as i128);
    all.push((a.min(b) - 1000, a.max(b)));
    let top = |g: &mut Gen| (1i128 << 63) + g.below(1 << 31) as i128 * g.below(1 << 31) as i128 * 2;
    let (a, b) = (top(g), top(g));
    all.push((a.min(b), a.max(b)));
    all
}

/// The floats of the size tried for the range.
fn floats(bits: FloatBits, (least, greatest): (i128, i128), g: &mut Gen) -> Vec<f64> {
    let mut all = Vec::new();
    for end in [least, greatest] {
        let e = end as f64;
        for d in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            all.push(sized(bits, e + d));
        }
        all.extend(beside(bits, e));
    }
    all.extend([0.5, 1.5, 2.5, -0.5, -1.5, -2.5, 0.0, -0.0]);
    all.extend([f64::INFINITY, f64::NEG_INFINITY, f64::NAN]);
    for _ in 0..2 {
        all.push(ff::value(bits, ff::float(g, bits)));
        let span = (greatest - least) as f64;
        let x = least as f64 + span * (g.below(1 << 20) as f64 / f64::from(1 << 20));
        all.push(sized(bits, x));
    }
    all
}

/// How a float is written in a program: as the shortest text that reads back
/// as it, or as the division that gives it.
fn written(bits: FloatBits, x: f64) -> String {
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

/// A program that prints, for each range and each float tried for it,
/// `clamp(x)`, `clamp(round(x))`, and `narrow(x)`, with what it must print.
#[derive(Debug)]
struct Case {
    source: String,
    said: String,
}

fn case(bits: FloatBits, g: &mut Gen) -> Case {
    let t = match bits {
        FloatBits::F32 => "f32",
        FloatBits::F64 => "f64",
    };
    let mut functions = String::new();
    let mut body = String::new();
    let mut said = String::new();
    for (k, range) in ranges(g).into_iter().enumerate() {
        let (least, greatest) = range;
        let ty = format!("{least}..{}", greatest + 1);
        functions += &format!(
            "fn clamp_{k}(x: {t}) -> {ty} {{ clamp(x) }}\n\n\
             fn narrow_{k}(x: {t}) -> Option[{ty}] {{ narrow(x) }}\n\n"
        );
        let xs = floats(bits, range, g);
        let list: Vec<String> = xs.iter().map(|x| written(bits, *x)).collect();
        body += &format!(
            "  xs_{k}: List[{t}] = [{}]\n  \
             loop i in 0..xs_{k}.size {{\n    \
               x = xs_{k}[i]\n    \
               match narrow_{k}(x) {{\n      \
                 .some(n) {{ print(\"{{clamp_{k}(x)}} {{clamp_{k}(round(x))}} some {{n}}\") }}\n      \
                 .none {{ print(\"{{clamp_{k}(x)}} {{clamp_{k}(round(x))}} none\") }}\n    \
               }}\n  \
             }}\n",
            list.join(", ")
        );
        for x in xs {
            let c = clamped(x, range);
            let n = match narrowed(x, range) {
                Some(n) => format!("some {n}"),
                None => "none".to_string(),
            };
            said += &format!("{c} {c} {n}\n");
        }
    }
    let source = format!(
        "use std.float (round)\nuse std.int (clamp, narrow)\n\n{functions}\
         fn main() <console, alloc, diverge> {{\n{body}}}\n"
    );
    Case {
        source: with_header(source),
        said,
    }
}

/// The evaluator, and each executable for arm64 run on the machine `aarch64`,
/// print what Rust says.
fn clamp_and_narrow_in_the_evaluator_and_on_arm64(bits: FloatBits, seed: u64) {
    generate::check(seed, 1, |g| case(bits, g), |case| {
        let source = &case.source;
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
        let out = scalar::run(&program);
        let printed = String::from_utf8(out.stdout).expect("text");
        assert_eq!(printed, case.said, "the evaluator, for\n{source}");
        let built = scalar::passes(&program);
        let mut ran_arm64 = false;
        for target in Target::ALL.iter().filter(|t| t.builds_floats()) {
            let bytes = target.emit(&built);
            if aarch64::is_executable(&bytes) {
                ran_arm64 = true;
                let printed = String::from_utf8(aarch64::run(&bytes).stdout).expect("text");
                assert_eq!(printed, case.said, "arm64 {target:?}, for\n{source}");
            }
        }
        assert!(ran_arm64, "an arm64 executable");
    });
}

/// Every executable this machine runs prints what the evaluator prints.
fn clamp_and_narrow_on_every_executable(bits: FloatBits, seed: u64) {
    let targets: Vec<Target> = runnable().into_iter().filter(|t| t.builds_floats()).collect();
    generate::check(seed, 1, |g| case(bits, g), |case| {
        let source = &case.source;
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
        let out = scalar::run(&program);
        assert_eq!(String::from_utf8(out.stdout.clone()).expect("text"), case.said);
        for &target in &targets {
            scalar_runs_as(&scalar::passes(&program), target, &out);
        }
    });
}

/// The examples of `arithmetic.md`, as the evaluator prints them.
fn clamp_of_a_float_rounds_a_half_to_the_even_one_and_narrow_takes_only_a_whole_number() {
    let source = with_header(
        "use std.float (floor)\nuse std.int (clamp, narrow)\n\n\
         fn level(x: f64) -> Byte { clamp(x) }\n\n\
         fn seats(x: f64) -> Option[0..500] { narrow(x) }\n\n\
         fn shown(x: f64) -> 0..501 {\n  match seats(x) {\n    .some(n) { n }\n    .none { 500 }\n  }\n}\n\n\
         fn main() <console, alloc> {\n  \
           print(\"{level(2.4)} {level(2.5)} {level(3.5)} {level(-7.0)} {level(1e30)}\")\n  \
           print(\"{clamp_byte(floor(2.9))}\")\n  \
           print(\"{shown(12.0)} {shown(12.5)} {shown(500.0)} {shown(-1.0)}\")\n\
         }\n\n\
         fn clamp_byte(x: f64) -> Byte { clamp(x) }\n"
            .to_string(),
    );
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(
        String::from_utf8_lossy(&outcome.stdout),
        "2 2 4 0 255\n2\n12 500 500 500\n"
    );
}

/// A float is taken only to a range of 64 bits or fewer.
fn clamp_of_a_float_to_a_range_of_more_than_64_bits_is_refused() {
    for name in ["clamp", "narrow"] {
        let to = if name == "clamp" {
            "0..36893488147419103232".to_string()
        } else {
            "Option[0..36893488147419103232]".to_string()
        };
        let source = format!(
            "use std.int ({name})\n\nfn wide(x: f64) -> {to} {{ {name}(x) }}\n\n\
             fn main() {{\n  y = wide(1.0)\n}}\n"
        );
        let errors = lower(source.as_bytes()).expect_err("refused");
        assert!(
            errors.iter().any(|e| e.message.contains(&format!("`{name}`"))),
            "{errors:?} for {source}"
        );
    }
}
