//! `f32` and `f64` as the language says: a float written out, `+`, `-`, `*`,
//! and `/`, a `-` before a float, the comparisons, `float(n)`, and floats in a
//! struct, a list, an argument, and a result. Every answer is what Rust gives
//! for the same operation on the same bits, and every executable of a chip
//! that builds floats gives what the evaluator gives.
//!
//! The values are drawn at the edges of each format and at random, and they
//! come out of a list, so that no target can know them. A float that is not
//! a number or is infinite is written as the `0.0 / 0.0` or `1.0 / 0.0` that
//! gives it.

use smith::scalar;
use smith::target::Target;
use smith::{check, lower};

use crate::Test;
use crate::exact::{self, Big};
use crate::generate::{self, Gen};
use crate::numbers::with_header;
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "the_evaluator_gives_what_rust_gives_for_every_operator_of_f32",
        || answers_are_what_rust_says::<f32>(0),
    ),
    (
        "the_evaluator_gives_what_rust_gives_for_every_operator_of_f64",
        || answers_are_what_rust_says::<f64>(1),
    ),
    (
        "the_text_of_an_f32_is_its_shortest_digits_laid_out_as_the_page_says",
        || texts_are_what_rust_says::<f32>(2),
    ),
    (
        "the_text_of_an_f64_is_its_shortest_digits_laid_out_as_the_page_says",
        || texts_are_what_rust_says::<f64>(3),
    ),
    (
        "the_square_root_of_an_f32_is_the_float_nearest_the_exact_root",
        || roots_are_what_rust_says::<f32>(4),
    ),
    (
        "the_square_root_of_an_f64_is_the_float_nearest_the_exact_root",
        || roots_are_what_rust_says::<f64>(5),
    ),
    (
        "the_logarithm_of_an_f32_is_the_float_nearest_the_exact_one_part_1_of_3",
        || logarithms_are_what_the_reference_says::<f32>(31),
    ),
    (
        "the_logarithm_of_an_f32_is_the_float_nearest_the_exact_one_part_2_of_3",
        || logarithms_are_what_the_reference_says::<f32>(32),
    ),
    (
        "the_logarithm_of_an_f32_is_the_float_nearest_the_exact_one_part_3_of_3",
        || logarithms_are_what_the_reference_says::<f32>(33),
    ),
    (
        "the_logarithm_of_an_f64_is_the_float_nearest_the_exact_one_part_1_of_3",
        || logarithms_are_what_the_reference_says::<f64>(41),
    ),
    (
        "the_logarithm_of_an_f64_is_the_float_nearest_the_exact_one_part_2_of_3",
        || logarithms_are_what_the_reference_says::<f64>(42),
    ),
    (
        "the_logarithm_of_an_f64_is_the_float_nearest_the_exact_one_part_3_of_3",
        || logarithms_are_what_the_reference_says::<f64>(43),
    ),
    (
        "the_sine_and_the_cosine_of_an_f32_are_the_floats_nearest_the_exact_ones_part_1_of_3",
        || trigs_are_what_the_reference_says::<f32>(51),
    ),
    (
        "the_sine_and_the_cosine_of_an_f32_are_the_floats_nearest_the_exact_ones_part_2_of_3",
        || trigs_are_what_the_reference_says::<f32>(52),
    ),
    (
        "the_sine_and_the_cosine_of_an_f32_are_the_floats_nearest_the_exact_ones_part_3_of_3",
        || trigs_are_what_the_reference_says::<f32>(53),
    ),
    (
        "the_sine_and_the_cosine_of_an_f64_are_the_floats_nearest_the_exact_ones_part_1_of_3",
        || trigs_are_what_the_reference_says::<f64>(61),
    ),
    (
        "the_sine_and_the_cosine_of_an_f64_are_the_floats_nearest_the_exact_ones_part_2_of_3",
        || trigs_are_what_the_reference_says::<f64>(62),
    ),
    (
        "the_sine_and_the_cosine_of_an_f64_are_the_floats_nearest_the_exact_ones_part_3_of_3",
        || trigs_are_what_the_reference_says::<f64>(63),
    ),
    (
        "the_reduction_and_the_series_of_trig_are_within_2_to_the_minus_228_of_the_reference",
        the_reduction_and_the_series_of_trig_are_within_2_to_the_minus_228_of_the_reference,
    ),
    (
        "the_reciprocal_and_the_series_of_ln_are_within_2_to_the_minus_228_of_the_reference",
        the_reciprocal_and_the_series_of_ln_are_within_2_to_the_minus_228_of_the_reference,
    ),
    (
        "the_series_of_exp_is_within_2_to_the_minus_228_of_the_reference",
        the_series_of_exp_is_within_2_to_the_minus_228_of_the_reference,
    ),
    (
        "the_exponential_of_an_f32_is_the_float_nearest_the_exact_one_part_1_of_3",
        || exponentials_are_what_the_reference_says::<f32>(11),
    ),
    (
        "the_exponential_of_an_f32_is_the_float_nearest_the_exact_one_part_2_of_3",
        || exponentials_are_what_the_reference_says::<f32>(12),
    ),
    (
        "the_exponential_of_an_f32_is_the_float_nearest_the_exact_one_part_3_of_3",
        || exponentials_are_what_the_reference_says::<f32>(13),
    ),
    (
        "the_exponential_of_an_f64_is_the_float_nearest_the_exact_one_part_1_of_3",
        || exponentials_are_what_the_reference_says::<f64>(21),
    ),
    (
        "the_exponential_of_an_f64_is_the_float_nearest_the_exact_one_part_2_of_3",
        || exponentials_are_what_the_reference_says::<f64>(22),
    ),
    (
        "the_exponential_of_an_f64_is_the_float_nearest_the_exact_one_part_3_of_3",
        || exponentials_are_what_the_reference_says::<f64>(23),
    ),
    (
        "a_float_written_only_inside_a_string_is_still_a_float_of_the_file",
        a_float_written_only_inside_a_string_is_still_a_float_of_the_file,
    ),
    (
        "a_tie_between_two_shortest_texts_goes_to_the_even_digit",
        a_tie_between_two_shortest_texts_goes_to_the_even_digit,
    ),
    (
        "a_whole_number_and_a_float_in_one_operator_are_refused",
        a_whole_number_and_a_float_in_one_operator_are_refused,
    ),
    (
        "a_float_too_large_for_its_type_is_refused",
        a_float_too_large_for_its_type_is_refused,
    ),
    (
        "a_float_literal_takes_the_type_its_use_decides_and_is_an_f64_when_nothing_does",
        a_float_literal_takes_the_type_its_use_decides_and_is_an_f64_when_nothing_does,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "every_executable_gives_what_rust_gives_for_every_operator_of_f32",
        || answers_on_every_executable::<f32>(0),
    ),
    (
        "every_executable_gives_what_rust_gives_for_every_operator_of_f64",
        || answers_on_every_executable::<f64>(1),
    ),
    (
        "every_executable_prints_2_5_for_half_of_5",
        every_executable_prints_2_5_for_half_of_5,
    ),
    (
        "every_executable_gives_the_square_root_of_an_f32_that_the_evaluator_gives",
        || roots_on_every_executable::<f32>(4),
    ),
    (
        "every_executable_gives_the_square_root_of_an_f64_that_the_evaluator_gives",
        || roots_on_every_executable::<f64>(5),
    ),
    (
        "every_executable_gives_the_sine_and_the_cosine_of_an_f32_that_the_evaluator_gives_part_1_of_3",
        || trigs_on_every_executable::<f32>(51),
    ),
    (
        "every_executable_gives_the_sine_and_the_cosine_of_an_f32_that_the_evaluator_gives_part_2_of_3",
        || trigs_on_every_executable::<f32>(52),
    ),
    (
        "every_executable_gives_the_sine_and_the_cosine_of_an_f32_that_the_evaluator_gives_part_3_of_3",
        || trigs_on_every_executable::<f32>(53),
    ),
    (
        "every_executable_gives_the_sine_and_the_cosine_of_an_f64_that_the_evaluator_gives_part_1_of_3",
        || trigs_on_every_executable::<f64>(61),
    ),
    (
        "every_executable_gives_the_sine_and_the_cosine_of_an_f64_that_the_evaluator_gives_part_2_of_3",
        || trigs_on_every_executable::<f64>(62),
    ),
    (
        "every_executable_gives_the_sine_and_the_cosine_of_an_f64_that_the_evaluator_gives_part_3_of_3",
        || trigs_on_every_executable::<f64>(63),
    ),
    (
        "every_executable_gives_the_logarithm_of_an_f32_that_the_evaluator_gives_part_1_of_3",
        || logarithms_on_every_executable::<f32>(31),
    ),
    (
        "every_executable_gives_the_logarithm_of_an_f32_that_the_evaluator_gives_part_2_of_3",
        || logarithms_on_every_executable::<f32>(32),
    ),
    (
        "every_executable_gives_the_logarithm_of_an_f32_that_the_evaluator_gives_part_3_of_3",
        || logarithms_on_every_executable::<f32>(33),
    ),
    (
        "every_executable_gives_the_logarithm_of_an_f64_that_the_evaluator_gives_part_1_of_3",
        || logarithms_on_every_executable::<f64>(41),
    ),
    (
        "every_executable_gives_the_logarithm_of_an_f64_that_the_evaluator_gives_part_2_of_3",
        || logarithms_on_every_executable::<f64>(42),
    ),
    (
        "every_executable_gives_the_logarithm_of_an_f64_that_the_evaluator_gives_part_3_of_3",
        || logarithms_on_every_executable::<f64>(43),
    ),
    (
        "every_executable_gives_the_exponential_of_an_f32_that_the_evaluator_gives_part_1_of_3",
        || exponentials_on_every_executable::<f32>(11),
    ),
    (
        "every_executable_gives_the_exponential_of_an_f32_that_the_evaluator_gives_part_2_of_3",
        || exponentials_on_every_executable::<f32>(12),
    ),
    (
        "every_executable_gives_the_exponential_of_an_f32_that_the_evaluator_gives_part_3_of_3",
        || exponentials_on_every_executable::<f32>(13),
    ),
    (
        "every_executable_gives_the_exponential_of_an_f64_that_the_evaluator_gives_part_1_of_3",
        || exponentials_on_every_executable::<f64>(21),
    ),
    (
        "every_executable_gives_the_exponential_of_an_f64_that_the_evaluator_gives_part_2_of_3",
        || exponentials_on_every_executable::<f64>(22),
    ),
    (
        "every_executable_gives_the_exponential_of_an_f64_that_the_evaluator_gives_part_3_of_3",
        || exponentials_on_every_executable::<f64>(23),
    ),
    (
        "every_executable_writes_the_shortest_text_of_an_f32",
        || texts_on_every_executable::<f32>(2),
    ),
    (
        "every_executable_writes_the_shortest_text_of_an_f64",
        || texts_on_every_executable::<f64>(3),
    ),
];

/// What a float type must give for the program to be written and its answers
/// worked out: Rust's own operations on it.
trait Float: Copy + PartialOrd + 'static {
    const NAME: &'static str;
    /// The bit that is the sign.
    const SIGN: u64;
    /// The values at the edges of the format.
    fn edges() -> Vec<Self>;
    /// A float of the bits, which are those of the type's width.
    fn from_bits(bits: u64) -> Self;
    fn is_nan(self) -> bool;
    fn is_infinite(self) -> bool;
    fn negative(self) -> bool;
    fn text(self) -> String;
    fn add(self, other: Self) -> Self;
    fn sub(self, other: Self) -> Self;
    fn mul(self, other: Self) -> Self;
    fn div(self, other: Self) -> Self;
    fn of_int(n: i64) -> Self;
    fn bits(self) -> u64;
    /// The float a text reads as, if it is a number this type holds.
    fn parsed(text: &str) -> Option<Self>;
    /// The bits, with every float that is not a number one value.
    fn key(self) -> u64;
}

macro_rules! float {
    ($t:ident, $bits:ident, $name:literal, $edges:expr) => {
        impl Float for $t {
            const NAME: &'static str = $name;
            const SIGN: u64 = 1 << ($bits::BITS - 1);
            fn parsed(text: &str) -> Option<$t> {
                text.parse::<$t>().ok().filter(|x| x.is_finite())
            }
            fn bits(self) -> u64 {
                u64::from(self.to_bits())
            }
            fn edges() -> Vec<$t> {
                $edges
            }
            fn from_bits(bits: u64) -> $t {
                $t::from_bits(bits as $bits)
            }
            fn is_nan(self) -> bool {
                $t::is_nan(self)
            }
            fn is_infinite(self) -> bool {
                $t::is_infinite(self)
            }
            fn negative(self) -> bool {
                self.is_sign_negative()
            }
            fn text(self) -> String {
                format!("{self:e}")
            }
            fn add(self, other: $t) -> $t {
                self + other
            }
            fn sub(self, other: $t) -> $t {
                self - other
            }
            fn mul(self, other: $t) -> $t {
                self * other
            }
            fn div(self, other: $t) -> $t {
                self / other
            }
            fn of_int(n: i64) -> $t {
                n as $t
            }
            fn key(self) -> u64 {
                if $t::is_nan(self) {
                    u64::MAX
                } else {
                    u64::from(self.to_bits())
                }
            }
        }
    };
}

float!(
    f32,
    u32,
    "f32",
    vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        0.1,
        3.0,
        f32::MAX,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        16_777_216.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ]
);
float!(
    f64,
    u64,
    "f64",
    vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        0.1,
        3.0,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        9_007_199_254_740_992.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ]
);

/// How a value is written in a program: as the shortest text that reads back
/// as exactly it, or as the expression that gives it.
fn written<T: Float>(x: T) -> String {
    if x.is_nan() {
        "(0.0 / 0.0)".to_string()
    } else if x.is_infinite() && x.negative() {
        "(-1.0 / 0.0)".to_string()
    } else if x.is_infinite() {
        "(1.0 / 0.0)".to_string()
    } else {
        x.text()
    }
}

/// Whether `a` and `b` are the same value, a float that is not a number being
/// the same as itself and `-0.0` another value than `0.0`.
fn same<T: Float>(a: T, b: T) -> bool {
    a.key() == b.key()
}

/// Whether `a` is below `b`, with `-0.0` below `0.0` and a float that is not a
/// number above every other.
fn below<T: Float>(a: T, b: T) -> bool {
    match (a.is_nan(), b.is_nan()) {
        (true, _) => false,
        (false, true) => true,
        _ if a < b => true,
        _ => a == b && a.negative() && !b.negative(),
    }
}

/// The values the program works on: the edges and some that are drawn.
fn values<T: Float>(g: &mut Gen) -> Vec<T> {
    let edges = T::edges();
    // The three that are not numbers or are no finite number, and the two
    // zeros, which every operator must tell apart, are always there.
    let mut values: Vec<T> = edges[11..].to_vec();
    values.extend(&edges[..2]);
    for _ in 0..2 {
        values.push(*g.pick(&edges[2..11]));
    }
    for _ in 0..2 {
        let bits = (g.below(1 << 30) as u64) << 34 | (g.below(1 << 30) as u64) << 4 | g.below(16) as u64;
        let x = T::from_bits(bits);
        // A float of the bits, which is a number and not an infinity.
        if !x.is_nan() && !x.is_infinite() {
            values.push(x);
        }
    }
    values
}

/// The whole numbers `float(n)` is tried on.
fn ints(g: &mut Gen) -> Vec<i64> {
    let mut ns = vec![0, 1, -1, 16_777_217, i64::MAX, i64::MIN, 9_007_199_254_740_993];
    ns.push(g.below(1 << 30) as i64 * 977 - (1 << 39));
    ns
}

/// The source of a program, and what it must print.
fn program<T: Float>(g: &mut Gen) -> (String, String) {
    let t = T::NAME;
    let xs = values::<T>(g);
    let ns = ints(g);
    let mut source = format!(
        "struct Pair {{\n  a: {t}\n  b: {t}\n}}\n\n\
         fn add(p: Pair) -> {t} {{ p.a + p.b }}\n\n\
         fn flip(x: {t}) -> {t} {{ -x }}\n\n\
         fn line(a: {t}, b: {t}, s: {t}, d: {t}, p: {t}, q: {t}) ! console, alloc {{\n  \
         print(\"{{a + b == s}} {{a - b == d}} {{a * b == p}} {{a / b == q}} {{a < b}} {{a <= b}} {{a > b}} {{a >= b}} {{a == b}} {{a != b}}\")\n}}\n\n\
         fn whole(n: Int, a: {t}) ! console, alloc {{\n  print(\"{{float(n) == a}}\")\n}}\n\n\
         fn main() ! console, alloc {{\n  xs: List[{t}] = [{}]\n  ns: List[Int] = [{}]\n",
        xs.iter().map(|x| written(*x)).collect::<Vec<_>>().join(", "),
        ns.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
    );
    let mut said = String::new();
    for (i, &a) in xs.iter().enumerate() {
        for (j, &b) in xs.iter().enumerate() {
            let results = [a.add(b), a.sub(b), a.mul(b), a.div(b)];
            let args: Vec<String> = results.iter().map(|r| written(*r)).collect();
            source += &format!("  line(xs[{i}], xs[{j}], {})\n", args.join(", "));
            let held = [
                true,
                true,
                true,
                true,
                below(a, b),
                !below(b, a),
                below(b, a),
                !below(a, b),
                same(a, b),
                !same(a, b),
            ];
            let words: Vec<String> = held.iter().map(|h| h.to_string()).collect();
            said += &format!("{}\n", words.join(" "));
        }
    }
    for (i, &n) in ns.iter().enumerate() {
        source += &format!("  whole(ns[{i}], {})\n", written(T::of_int(n)));
        said += "true\n";
    }
    // A float in a struct, an argument, and a result, and the sign of one flipped.
    let (a, b) = (xs[0], xs[1]);
    source += &format!(
        "  p = Pair(a: xs[0], b: xs[1])\n  print(\"{{add(p) == {}}} {{flip(xs[0]) == {}}}\")\n",
        written(a.add(b)),
        written(neg(a)),
    );
    said += "true true\n";
    source += "}\n";
    (with_header(source), said)
}

/// `-x`: the float with its sign bit flipped.
fn neg<T: Float>(x: T) -> T {
    T::from_bits(x.bits() ^ T::SIGN)
}

fn answers_are_what_rust_says<T: Float>(seed: u64) {
    let (source, said) = generate::sample(seed, 0, |g| program::<T>(g));
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(String::from_utf8_lossy(&outcome.stdout), said, "for {source}");
    assert_eq!(outcome.status, 0);
}

fn answers_on_every_executable<T: Float>(seed: u64) {
    let (source, said) = generate::sample(seed, 0, |g| program::<T>(g));
    let outcome = scalar::Outcome {
        stdout: said.into_bytes(),
        status: 0,
    };
    for target in runnable()
        .into_iter()
        .filter(|t: &Target| t.builds_floats())
    {
        runs_as(source.as_bytes(), target, &outcome);
    }
}

/// A program of `items` whose `main` prints the `bool` that `shown` says, so
/// that nothing in it is dead code.
fn reaching(items: &str, shown: &str) -> String {
    with_header(format!(
        "{items}\nfn main() ! console, alloc {{\n  print(\"{{{shown}}}\")\n}}\n"
    ))
}

fn a_whole_number_and_a_float_in_one_operator_are_refused() {
    for operator in ["+", "-", "*", "/"] {
        let source = reaching(
            &format!("fn f(n: Int, x: f64) -> f64 {{ n {operator} x }}"),
            "f(1, 2.0) == 3.0",
        );
        let errors = check(source.as_bytes()).expect_err("a whole number and a float");
        assert!(
            errors.iter().any(|e| e.message.contains("float")),
            "{operator}: {errors:?}"
        );
    }
    for operator in ["<", "<=", ">", ">=", "==", "!="] {
        let source = reaching(
            &format!("fn f(n: Int, x: f64) -> bool {{ n {operator} x }}"),
            "f(1, 2.0)",
        );
        let errors = check(source.as_bytes()).expect_err("a whole number and a float");
        assert!(
            errors.iter().any(|e| e.message.contains("f64")),
            "{operator}: {errors:?}"
        );
    }
    let source = reaching("fn half(n: Int) -> f64 { n / 2.0 }", "half(5) == 2.5");
    assert!(check(source.as_bytes()).is_err(), "{source}");
    let source = reaching("fn half(n: Int) -> f64 { float(n) / 2.0 }", "half(5) == 2.5");
    assert!(check(source.as_bytes()).is_ok(), "{source}");
}

fn a_float_too_large_for_its_type_is_refused() {
    let source = reaching("fn big() -> f32 { 1e39 }", "big() == 1.0");
    assert!(check(source.as_bytes()).is_err(), "{source}");
    let source = reaching("fn big() -> f64 { 1e39 }", "big() == 1.0");
    assert!(check(source.as_bytes()).is_ok(), "{source}");
    let source = reaching("fn big() -> f64 { 1e999 }", "big() == 1.0");
    assert!(check(source.as_bytes()).is_err(), "{source}");
}

fn a_float_literal_takes_the_type_its_use_decides_and_is_an_f64_when_nothing_does() {
    // `0.1` is the `f32` nearest a tenth where an `f32` is wanted, and the
    // `f64` nearest it where nothing decides, so the two are not equal as
    // floats once the first is widened: each is compared as its own type.
    let source = with_header(
        "fn tenth() -> f32 { 0.1 }\n\nfn main() ! console, alloc {\n  \
         x = 0.1\n  y: f32 = 0.1\n  print(\"{x == 0.1} {y == tenth()}\")\n}\n"
            .to_string(),
    );
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(String::from_utf8_lossy(&outcome.stdout), "true true\n");
}

/// The text of a float that the page says: `nan`, `inf`, `-inf`, `0.0` and
/// `-0.0`, and for any other the shortest digits, which Rust's `{:e}` gives,
/// with a point and no exponent from `10^-5` to `10^16`, and otherwise as
/// `d.ddde-x`, with no point for one digit.
fn spelled<T: Float>(x: T) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    let sign = if x.negative() { "-" } else { "" };
    if x.is_infinite() {
        return format!("{sign}inf");
    }
    let shown = x.text();
    let shown = shown.trim_start_matches('-');
    if shown == "0e0" {
        return format!("{sign}0.0");
    }
    let (mantissa, exponent) = shown.split_once('e').expect("an exponent");
    let exponent: i32 = exponent.parse().expect("a number");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let n = digits.len() as i32;
    let body = if (-5..=16).contains(&exponent) {
        if exponent >= 0 {
            let int = exponent + 1;
            if n <= int {
                format!("{digits}{}.0", "0".repeat((int - n) as usize))
            } else {
                format!("{}.{}", &digits[..int as usize], &digits[int as usize..])
            }
        } else {
            format!("0.{}{digits}", "0".repeat((-exponent - 1) as usize))
        }
    } else if n == 1 {
        format!("{digits}e{exponent}")
    } else {
        format!("{}.{}e{exponent}", &digits[..1], &digits[1..])
    };
    format!("{sign}{body}")
}

/// The floats whose text is tried: the edges of the format, powers of 2 and of
/// 10, a number of each length, and some drawn at random.
fn texts<T: Float>(g: &mut Gen) -> Vec<T> {
    let mut values: Vec<T> = T::edges();
    for k in 0..4 {
        values.push(T::from_bits(1u64 << (T::SIGN.trailing_zeros() - 1 - k)));
    }
    for text in [
        "1e0", "1e1", "1e5", "1e15", "1e16", "1e17", "1e21", "1e22", "1e23", "1e-4", "1e-5",
        "1e-6", "1.5e-7", "5e-324", "2.2250738585072014e-308", "123456789e0", "0.3", "1e10",
        "9.999999e15", "4.35e0", "8.41e21",
    ] {
        // Only the numbers the type can hold.
        if let Some(x) = T::parsed(text) {
            values.push(x);
            values.push(neg(x));
        }
    }
    // The powers of 2, and the numbers just above and below them, where the
    // interval of the numbers that round to a float is not symmetric.
    let (mantissa, top) = if T::NAME == "f32" { (23, 254) } else { (52, 2046) };
    for e in (1..top).step_by(if T::NAME == "f32" { 7 } else { 29 }) {
        let power = (e as u64) << mantissa;
        values.push(T::from_bits(power));
        values.push(T::from_bits(power + 1));
        values.push(T::from_bits(power - 1));
    }
    for _ in 0..40 {
        let bits = (g.below(1 << 30) as u64) << 34 | (g.below(1 << 30) as u64) << 4 | g.below(16) as u64;
        let x = T::from_bits(bits ^ ((g.below(1 << 20) as u64) << 20));
        if !x.is_nan() && !x.is_infinite() {
            values.push(x);
        }
    }
    values
}

/// The program that prints each float on a line, and what it must print.
fn text_program<T: Float>(g: &mut Gen) -> (String, String) {
    let values = texts::<T>(g);
    let t = T::NAME;
    let mut source = format!(
        "fn main() ! console, alloc {{\n  xs: List[{t}] = [{}]\n",
        values.iter().map(|x| written(*x)).collect::<Vec<_>>().join(", ")
    );
    let mut said = String::new();
    for (i, x) in values.iter().enumerate() {
        source += &format!("  print(\"{{xs[{i}]}}\")\n");
        said += &format!("{}\n", spelled(*x));
        // The text of a number is a float that is written out, which reads back
        // as the float.
        if !x.is_nan() && !x.is_infinite() {
            source += &format!("  print(\"{{xs[{i}] == {}}}\")\n", spelled(*x));
            said += "true\n";
        }
    }
    source += "}\n";
    (with_header(source), said)
}

/// Whether the text is what Rust writes, or the other of two shortest texts
/// that are equally near the float, which Rust rounds up where Ryu goes to
/// the even digit: the same number of digits, each a number that reads back as
/// the float, and the last digit apart by 1.
fn same_text<T: Float>(ours: &str, theirs: &str) -> bool {
    if ours == theirs {
        return true;
    }
    let digits = |t: &str| -> Vec<char> {
        let mantissa = t.split('e').next().unwrap_or(t);
        mantissa.chars().filter(|c| c.is_ascii_digit()).collect()
    };
    let (a, b) = (digits(ours), digits(theirs));
    let same_shape = ours.len() == theirs.len()
        && ours.chars().zip(theirs.chars()).filter(|(x, y)| x != y).count() == 1;
    let (Some(x), Some(y)) = (T::parsed(ours), T::parsed(theirs)) else {
        return false;
    };
    // The two read back as one float, so they are two shortest texts of it.
    same_shape && a.len() == b.len() && x.bits() == y.bits()
}

fn texts_are_what_rust_says<T: Float>(seed: u64) {
    let (source, said) = generate::sample(seed, 0, |g| text_program::<T>(g));
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    let got = String::from_utf8_lossy(&outcome.stdout).to_string();
    let (got, want): (Vec<&str>, Vec<&str>) = (got.lines().collect(), said.lines().collect());
    assert_eq!(got.len(), want.len(), "for {source}");
    for (ours, theirs) in got.iter().zip(&want) {
        assert!(same_text::<T>(ours, theirs), "{ours} is not {theirs} for {source}");
    }
}

/// Every executable writes what the evaluator writes, which the other test
/// holds to what Rust writes.
fn texts_on_every_executable<T: Float>(seed: u64) {
    let (source, _) = generate::sample(seed, 0, |g| text_program::<T>(g));
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    for target in runnable().into_iter().filter(|t: &Target| t.builds_floats()) {
        runs_as(source.as_bytes(), target, &outcome);
    }
}

/// `2097152.25` is as near `2097152.2` as `2097152.3`, and both read back as
/// it, so the digit that is even is the one written.
fn a_tie_between_two_shortest_texts_goes_to_the_even_digit() {
    let source = with_header(
        "fn main() ! console, alloc {\n  x: f32 = 2097152.25\n  y: f32 = 2097152.75\n  print(\"{x} {y}\")\n}\n"
            .to_string(),
    );
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(String::from_utf8_lossy(&outcome.stdout), "2097152.2 2097152.8\n");
}

/// The program of `number-types.md`: `half(5)` is `2.5`, on every target that
/// builds floats.
fn every_executable_prints_2_5_for_half_of_5() {
    let source = with_header(
        "fn half(n: Int) -> f64 { float(n) / 2.0 }\n\nfn main() ! console, alloc {\n  print(\"{half(5)}\")\n}\n"
            .to_string(),
    );
    let outcome = scalar::Outcome {
        stdout: b"2.5\n".to_vec(),
        status: 0,
    };
    for target in runnable().into_iter().filter(|t: &Target| t.builds_floats()) {
        runs_as(source.as_bytes(), target, &outcome);
    }
}

/// The sources of the programs of floats that the other tests of this file
/// run: every operator and comparison, and the text, of each size, so that a
/// target that is run by a host is held to the evaluator on them.
pub fn sources() -> Vec<String> {
    (0..SOURCES).map(source).collect()
}

/// How many programs there are.
pub const SOURCES: usize = 12;

/// The program of a place in the list, made alone so that a test can run a
/// few of them.
pub fn source(at: usize) -> String {
    match at {
        0 => generate::sample(0, 0, program::<f32>).0,
        1 => generate::sample(1, 0, program::<f64>).0,
        2 => generate::sample(2, 0, text_program::<f32>).0,
        3 => generate::sample(3, 0, text_program::<f64>).0,
        4 => generate::sample(4, 0, root_program::<f32>).0,
        5 => generate::sample(5, 0, root_program::<f64>).0,
        6 => generate::sample(11, 0, exponential_program::<f32>).0,
        7 => generate::sample(21, 0, exponential_program::<f64>).0,
        8 => generate::sample(31, 0, logarithm_program::<f32>).0,
        9 => generate::sample(41, 0, logarithm_program::<f64>).0,
        10 => generate::sample(51, 0, trig_program::<f32>).0,
        _ => generate::sample(61, 0, trig_program::<f64>).0,
    }
}

/// Rust's square root of a float, which is the one that is rounded once.
fn root<T: Float>(x: T) -> T {
    T::from_bits(if T::NAME == "f32" {
        u64::from(f32::from_bits(x.bits() as u32).sqrt().to_bits())
    } else {
        f64::from_bits(x.bits()).sqrt().to_bits()
    })
}

/// The program that compares the square root of each float with the one that
/// Rust gives, as a float written out, and what it must print.
fn root_program<T: Float>(g: &mut Gen) -> (String, String) {
    let mut values = texts::<T>(g);
    // The perfect squares and the numbers beside them, where a root is exact or
    // is half way between two floats.
    for k in [1.0f64, 2.0, 3.0, 4.0, 9.0, 25.0, 1e6, 4503599627370496.0] {
        if let Some(x) = T::parsed(&format!("{:e}", k * k)) {
            values.extend([x, T::from_bits(x.bits() + 1), T::from_bits(x.bits() - 1)]);
        }
    }
    let t = T::NAME;
    let mut source = format!(
        "use std.float (sqrt)\n\nfn main() ! console, alloc {{\n  xs: List[{t}] = [{}]\n",
        values.iter().map(|x| written(*x)).collect::<Vec<_>>().join(", ")
    );
    let mut said = String::new();
    for (i, x) in values.iter().enumerate() {
        source += &format!("  print(\"{{sqrt(xs[{i}]) == {}}}\")\n", written(root(*x)));
        said += "true\n";
        let _ = x;
    }
    source += "}\n";
    (with_header(source), said)
}

fn roots_are_what_rust_says<T: Float>(seed: u64) {
    let (source, said) = generate::sample(seed, 0, root_program::<T>);
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(String::from_utf8_lossy(&outcome.stdout), said, "for {source}");
}

fn roots_on_every_executable<T: Float>(seed: u64) {
    let (source, _) = generate::sample(seed, 0, root_program::<T>);
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    for target in runnable().into_iter().filter(|t: &Target| t.builds_floats()) {
        runs_as(source.as_bytes(), target, &outcome);
    }
}

/// A float that is written only inside a string, and no other, is a float of
/// the file, so what floats are compared by is read for it.
fn a_float_written_only_inside_a_string_is_still_a_float_of_the_file() {
    let source = with_header(
        "fn main() ! console, alloc {\n  print(\"{1.5 == 1.5} {2.0 < 3.0} {1.5}\")\n}\n".to_string(),
    );
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(String::from_utf8_lossy(&outcome.stdout), "true true 1.5\n");
}

/// The exponential of a finite float that the reference gives, as a float.
fn exact_exp<T: Float>(x: T) -> T {
    // Past a thousand the result is infinity or 0 in either type, and the
    // reference would reduce a number of 1000 bits.
    let magnitude = if T::NAME == "f32" { f64::from(f32::from_bits(x.bits() as u32)) } else { f64::from_bits(x.bits()) };
    if magnitude.abs() > 1000.0 {
        let infinity = if T::NAME == "f32" { 0x7f80_0000 } else { 0x7ff0_0000_0000_0000 };
        let bits = if magnitude > 0.0 { infinity } else { 0 };
        return T::from_bits(bits);
    }
    if T::NAME == "f32" {
        T::from_bits(u64::from(exact::exp_f32(f32::from_bits(x.bits() as u32))))
    } else {
        T::from_bits(exact::exp_f64(f64::from_bits(x.bits())))
    }
}

/// The numbers the exponential is tried at: the edges, numbers about 2^-54,
/// where the result is 1 and where it is not, numbers of every size up to
/// where the result is infinity or 0, and the numbers at those places.
fn exponent_inputs<T: Float>(g: &mut Gen) -> Vec<T> {
    let (high, low, tiny) = if T::NAME == "f32" {
        (88.8f64, -103.97f64, -25)
    } else {
        (709.79f64, -745.14f64, -54)
    };
    let mut values: Vec<T> = Vec::new();
    let parse = |v: f64| T::parsed(&format!("{v:e}"));
    // The edges, but for the ones that are not numbers or are infinite, which
    // `exp` is given as they are.
    for x in T::edges() {
        if !x.is_nan() && !x.is_infinite() {
            values.push(x);
        }
    }
    for v in [
        high,
        high + 0.01,
        low,
        low - 0.01,
        1.0,
        -1.0,
        0.5,
        2f64.powi(tiny),
        -(2f64.powi(tiny)),
        1.5 * 2f64.powi(tiny),
        -1.5 * 2f64.powi(tiny),
        2f64.powi(tiny - 1),
        std::f64::consts::LN_2,
        -std::f64::consts::LN_2,
        10.0,
        -10.0,
    ] {
        if let Some(x) = parse(v) {
            values.push(x);
        }
    }
    // Numbers whose exponential is as near a tie between two floats as a
    // number can be built to be, and which no random number is. `1 + x` is
    // exactly half way between two floats when `x` is an odd multiple of half
    // the spacing after 1, and `exp x` is above it by about `x^2 / 2`, which is
    // 2^-107 of the result for `f64` at the first of them and grows as the
    // square of the multiple. Below 0 the same holds from `1 - x` at the odd
    // multiples of a quarter of the spacing. The float before each is as near
    // the tie from the other side. A series cut short, or a fixed point of too
    // few bits, rounds one of these wrongly.
    let places = if T::NAME == "f32" { 23 } else { 52 };
    for j in [0u32, 1, 2, 3, 7, 100, 1000, 1 << 10, 1 << 16] {
        let odd = f64::from(2 * j + 1);
        for v in [
            odd * 2f64.powi(-(places + 1)),
            -odd * 2f64.powi(-(places + 2)),
        ] {
            if let Some(x) = parse(v) {
                values.push(x);
                values.push(T::from_bits(x.bits() - 1));
            }
        }
    }
    for k in 0..18 {
        let unit = g.below(1 << 30) as f64 / f64::from(1u32 << 30);
        let v = match k % 6 {
            0 => (unit * 2.0 - 1.0) * 2.0,
            1 => (unit * 2.0 - 1.0) * 30.0,
            2 => low + (high - low) * unit,
            3 => unit * high,
            4 => unit * low,
            _ => (unit * 2.0 - 1.0) * 2f64.powi(tiny + 3),
        };
        if let Some(x) = parse(v) {
            values.push(x);
        }
    }
    values
}

/// The program that compares the exponential of each float with the one the
/// reference gives, as a float written out, and what it prints.
fn exponential_program<T: Float>(g: &mut Gen) -> (String, String) {
    let values = exponent_inputs::<T>(g);
    let t = T::NAME;
    let mut source = format!(
        "use std.float (exp)\n\nfn main() ! console, alloc {{\n  xs: List[{t}] = [{}]\n",
        values.iter().map(|x| written(*x)).collect::<Vec<_>>().join(", ")
    );
    let mut said = String::new();
    for (i, x) in values.iter().enumerate() {
        source += &format!("  print(\"{{exp(xs[{i}]) == {}}}\")\n", written(exact_exp(*x)));
        said += "true\n";
    }
    source += "}\n";
    (with_header(source), said)
}

fn exponentials_are_what_the_reference_says<T: Float>(seed: u64) {
    let (source, said) = generate::sample(seed, 0, exponential_program::<T>);
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(String::from_utf8_lossy(&outcome.stdout), said, "for {source}");
}

fn exponentials_on_every_executable<T: Float>(seed: u64) {
    let (source, _) = generate::sample(seed, 0, exponential_program::<T>);
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    for target in runnable().into_iter().filter(|t: &Target| t.builds_floats()) {
        runs_as(source.as_bytes(), target, &outcome);
    }
}

/// The logarithm of a float above 0 that the reference gives, as a float.
fn exact_ln<T: Float>(x: T) -> T {
    if T::NAME == "f32" {
        T::from_bits(u64::from(exact::ln_f32(f32::from_bits(x.bits() as u32))))
    } else {
        T::from_bits(exact::ln_f64(f64::from_bits(x.bits())))
    }
}

/// The numbers the logarithm is tried at: those beside 1, where the result is
/// least, those beside the powers of 2 and of the square root of 2, where the
/// reduction changes, the least and the greatest, and floats of any size.
fn logarithm_inputs<T: Float>(g: &mut Gen) -> Vec<T> {
    let (mantissa, bits) = if T::NAME == "f32" { (23, 32) } else { (52, 64) };
    let parse = |v: f64| T::parsed(&format!("{v:e}"));
    let unit = 2f64.powi(-mantissa);
    let mut values: Vec<T> = Vec::new();
    for v in [
        1.0,
        1.0 + unit,
        1.0 - unit / 2.0,
        1.0 + 2.0 * unit,
        1.0 - unit,
        2.0,
        0.5,
        10.0,
        0.1,
        std::f64::consts::E,
        std::f64::consts::SQRT_2,
        std::f64::consts::SQRT_2 + 1e-6,
        std::f64::consts::SQRT_2 - 1e-6,
        std::f64::consts::FRAC_1_SQRT_2,
        0.75,
        1.5,
        1e30,
        1e-30,
        2f64.powi(100),
        2f64.powi(-100),
    ] {
        if let Some(x) = parse(v) {
            values.push(x);
        }
    }
    // The least normal number, the least subnormal one, and the greatest.
    for x in T::edges() {
        if !x.is_nan() && !x.is_infinite() && !x.negative() {
            values.push(x);
        }
    }
    for k in 0..24 {
        let word = (g.below(1 << 30) as u64) << 34
            | (g.below(1 << 30) as u64) << 4
            | g.below(16) as u64;
        let mut pattern = word & ((1u64 << (bits - 1)) - 1);
        // Some of them near 1, and the others with any exponent.
        if k % 3 == 0 {
            let near = g.below(4096) as i64 - 2048;
            let one = if bits == 32 { 0x3f80_0000i64 } else { 0x3ff0_0000_0000_0000i64 };
            pattern = (one + near) as u64;
        }
        let x = T::from_bits(pattern);
        if !x.is_nan() && !x.is_infinite() {
            values.push(x);
        }
    }
    values.retain(|x| !x.negative() && x.key() != T::from_bits(0).key());
    values
}

/// The program that compares the logarithm of each float with the one the
/// reference gives, as a float written out, and what it prints. It also tries
/// 0 and a number below 0.
fn logarithm_program<T: Float>(g: &mut Gen) -> (String, String) {
    let values = logarithm_inputs::<T>(g);
    let t = T::NAME;
    let mut source = format!(
        "use std.float (ln)\n\nfn main() ! console, alloc {{\n  xs: List[{t}] = [{}]\n  zero: {t} = 0.0\n  below: {t} = -1.0\n",
        values.iter().map(|x| written(*x)).collect::<Vec<_>>().join(", ")
    );
    let mut said = String::new();
    for (i, x) in values.iter().enumerate() {
        source += &format!("  print(\"{{ln(xs[{i}]) == {}}}\")\n", written(exact_ln(*x)));
        said += "true\n";
    }
    // `-inf` is below 0 and is its own double, and `nan` is after `inf`.
    source += "  print(\"{ln(zero) < 0.0 and ln(zero) + ln(zero) == ln(zero)}\")\n";
    source += "  print(\"{ln(below) > 1.0 / zero}\")\n";
    said += "true\ntrue\n";
    source += "}\n";
    (with_header(source), said)
}

fn logarithms_are_what_the_reference_says<T: Float>(seed: u64) {
    let (source, said) = generate::sample(seed, 0, logarithm_program::<T>);
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(String::from_utf8_lossy(&outcome.stdout), said, "for {source}");
}

fn logarithms_on_every_executable<T: Float>(seed: u64) {
    let (source, _) = generate::sample(seed, 0, logarithm_program::<T>);
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    for target in runnable().into_iter().filter(|t: &Target| t.builds_floats()) {
        runs_as(source.as_bytes(), target, &outcome);
    }
}

/// The function of the runtime that sums the series of `exp`, which takes the
/// eight words of `r / 64` as fixed point with 240 bits after the point and
/// gives `exp(r)` in the same form, is as near the reference as its comment
/// says, 2^-228, so that no float whose exponential is as near a tie as the
/// worst cases that are known (about 2^-120) is rounded wrongly. The floats
/// cannot show this: a series cut to 12 terms is wrong by about 2^-101, and
/// there is no list of floats within 2^-100 of a tie to try it on.
fn the_series_of_exp_is_within_2_to_the_minus_228_of_the_reference() {
    let source = "use std.console (console, print)\nuse std.float (exp)\n\nfn main() ! console, alloc {\n  print(\"{exp(1.0)}\")\n}\n";
    let program = lower(source.as_bytes()).expect("builds");
    // The function of eight words that gives eight words.
    let series = program
        .functions
        .iter()
        .position(|f| f.blocks[0].params.len() == 8 && f.results.len() == 8)
        .expect("the series is in the program");
    let inputs: Vec<[u64; 8]> = generate::sample(99, 0, |g| {
        (0..40)
            .map(|k| {
                let mut words = [0u64; 8];
                for w in words.iter_mut().take(7) {
                    *w = g.below(1 << 32) as u64;
                }
                // `r` is below `ln 2`, so `r / 64` is below 0.0109: 700 of the
                // 65536 that the top word counts.
                words[7] = g.below(700) as u64;
                match k {
                    0 => [0; 8],
                    1 => [0xffff_ffff, 0xffff_ffff, 0xffff_ffff, 0xffff_ffff, 0xffff_ffff, 0xffff_ffff, 0xffff_ffff, 699],
                    _ => words,
                }
            })
            .collect()
    });
    let mut worst = i64::MIN;
    for words in inputs {
        let got = scalar::call(&program, series, &words);
        let r = exact::from_words(&words, 240).ldexp(6);
        let want = exact::exp(&r);
        let distance = exact::distance_bits(&exact::from_words(&got, 240), &want);
        worst = worst.max(distance);
        assert!(distance < -228, "r / 64 = {words:x?} is wrong by about 2^{distance}");
    }
    eprintln!("the series is wrong by under 2^{worst}");
}

/// The two functions of the runtime that `ln` is made of, which take eight
/// words of fixed point with 240 bits after the point: the reciprocal of a
/// number between 0.85 and 1.21, and `z + z^3 / 3 + ...` of a `z` below 0.18,
/// are as near the reference as the comment of `ln` says, 2^-228, so that no
/// float is rounded wrongly for want of bits. The floats cannot show this: a
/// series cut to 8 terms is wrong by about 2^-45 and shows, but one of 40 is
/// wrong by about 2^-200 and shows on no float that is known.
fn the_reciprocal_and_the_series_of_ln_are_within_2_to_the_minus_228_of_the_reference() {
    let source = "use std.console (console, print)\nuse std.float (ln)\n\nfn main() ! console, alloc {\n  print(\"{ln(2.0)}\")\n}\n";
    let program = lower(source.as_bytes()).expect("builds");
    // The functions of eight words that give eight words, in the order the
    // runtime places them: the reciprocal, then the series.
    let found: Vec<usize> = program
        .functions
        .iter()
        .enumerate()
        .filter(|(_, f)| f.blocks[0].params.len() == 8 && f.results.len() == 8)
        .map(|(i, _)| i)
        .collect();
    let [reciprocal, series] = found[..] else {
        panic!("the reciprocal and the series are in the program, found {found:?}")
    };
    let words_of = |value: f64| -> [u64; 8] {
        // A number below 4 with 240 bits after the point, from a float.
        let scaled = value * 2f64.powi(60);
        let mut words = [0u64; 8];
        let whole = scaled as u64;
        // `whole * 2^180` as words of 32 bits.
        for (k, part) in [whole & 0xffff_ffff, whole >> 32].into_iter().enumerate() {
            let at = 180 + 32 * k;
            for bit in 0..32 {
                if part >> bit & 1 == 1 {
                    let place = at + bit;
                    words[place / 32] |= 1 << (place % 32);
                }
            }
        }
        words
    };
    let inputs: Vec<[u64; 8]> = generate::sample(98, 0, |g| {
        (0..30)
            .map(|k| {
                let mut words = [0u64; 8];
                for w in words.iter_mut().take(7) {
                    *w = g.below(1 << 32) as u64;
                }
                // Between 0.85 and 1.21, from the top word: 2^16 is 1.0.
                words[7] = 55706 + g.below(23593) as u64;
                match k {
                    0 => words_of(1.0),
                    1 => words_of(0.85),
                    2 => words_of(1.21),
                    _ => words,
                }
            })
            .collect()
    });
    let mut worst = i64::MIN;
    for words in inputs {
        let a = exact::from_words(&words, 240);
        let got = scalar::call(&program, reciprocal, &words);
        let want = Big::from_u64(1).div_at(&a, 600);
        let distance = exact::distance_bits(&exact::from_words(&got, 240), &want);
        worst = worst.max(distance);
        assert!(distance < -228, "1 / {words:x?} is wrong by about 2^{distance}");
    }
    let inputs: Vec<[u64; 8]> = generate::sample(97, 0, |g| {
        (0..30)
            .map(|k| {
                let mut words = [0u64; 8];
                for w in words.iter_mut().take(7) {
                    *w = g.below(1 << 32) as u64;
                }
                // Below 0.18, which is 11796 of the 65536 of the top word.
                words[7] = g.below(11796) as u64;
                match k {
                    0 => [0; 8],
                    1 => words_of(0.17),
                    // The greatest, where the series is slowest.
                    2..=7 => {
                        words[7] = 11790 + g.below(6) as u64;
                        words
                    }
                    _ => words,
                }
            })
            .collect()
    });
    for words in inputs {
        let z = exact::from_words(&words, 240);
        let got = scalar::call(&program, series, &words);
        // `z + z^3 / 3 + ...` is half of `ln((1 + z) / (1 - z))`.
        let one = Big::from_u64(1);
        let ratio = one.add_at(&z, 600).div_at(&one.sub_at(&z, 600), 600);
        let want = if z.is_zero() {
            Big::zero()
        } else {
            exact::ln(&ratio).ldexp(-1)
        };
        let distance = exact::distance_bits(&exact::from_words(&got, 240), &want);
        worst = worst.max(distance);
        assert!(distance < -228, "the series of {words:x?} is wrong by about 2^{distance}");
    }
    eprintln!("the reciprocal and the series are wrong by under 2^{worst}");
}

/// The sine and the cosine of a float that the reference gives, as floats.
fn exact_trig<T: Float>(x: T) -> (T, T) {
    if T::NAME == "f32" {
        let x = f32::from_bits(x.bits() as u32);
        (
            T::from_bits(u64::from(exact::sin_f32(x))),
            T::from_bits(u64::from(exact::cos_f32(x))),
        )
    } else {
        let x = f64::from_bits(x.bits());
        (
            T::from_bits(exact::sin_f64(x)),
            T::from_bits(exact::cos_f64(x)),
        )
    }
}

/// The numbers the sine and the cosine are tried at: the edges, the floats
/// nearest the multiples of a half of pi, where the result is least and the
/// reduction is hardest, powers of 2 from the least that is not given as it is
/// to the greatest, and floats of any size, of either sign.
fn trig_inputs<T: Float>(g: &mut Gen) -> Vec<T> {
    let bits = if T::NAME == "f32" { 32 } else { 64 };
    let parse = |v: f64| T::parsed(&format!("{v:e}"));
    let mut values: Vec<T> = Vec::new();
    for x in T::edges().into_iter().take(6) {
        if !x.is_nan() && !x.is_infinite() && x.key() != T::from_bits(0).key() && x.key() != T::from_bits(1 << (bits - 1)).key() {
            values.push(x);
        }
    }
    let powers: &[i32] = if T::NAME == "f32" {
        &[-13, 0, 100]
    } else {
        &[-26, 0, 1023]
    };
    for k in powers {
        let v = 2f64.powi(*k);
        for sign in [1.0, -1.0] {
            if let Some(x) = parse(sign * v) {
                values.push(x);
            }
        }
    }
    for k in [1u64, 3, 355, 12_345_678] {
        let v = k as f64 * std::f64::consts::FRAC_PI_2;
        if let Some(x) = parse(v) {
            values.push(x);
        }
    }
    for v in [1e22, 0.1, 6.0] {
        if let Some(x) = parse(v) {
            values.push(x);
        }
    }
    for _ in 0..4 {
        let word = (g.below(1 << 30) as u64) << 34 | (g.below(1 << 30) as u64) << 4 | g.below(16) as u64;
        let x = T::from_bits(if bits == 64 { word } else { word & ((1u64 << bits) - 1) });
        if !x.is_nan() && !x.is_infinite() && x.key() != T::from_bits(0).key() && x.key() != T::from_bits(1 << (bits - 1)).key() {
            values.push(x);
        }
    }
    values
}

/// The program that compares the sine and the cosine of each float with the
/// ones the reference gives, as floats written out, and what it prints. It
/// also tries 0, and an infinity.
fn trig_program<T: Float>(g: &mut Gen) -> (String, String) {
    let values = trig_inputs::<T>(g);
    let t = T::NAME;
    let mut source = format!(
        "use std.float (sin, cos)\n\nfn main() ! console, alloc {{\n  xs: List[{t}] = [{}]\n  zero: {t} = 0.0\n  infinity: {t} = 1.0 / zero\n",
        values.iter().map(|x| written(*x)).collect::<Vec<_>>().join(", ")
    );
    let mut said = String::new();
    for (i, x) in values.iter().enumerate() {
        let (sine, cosine) = exact_trig(*x);
        source += &format!("  print(\"{{sin(xs[{i}]) == {}}}\")\n", written(sine));
        source += &format!("  print(\"{{cos(xs[{i}]) == {}}}\")\n", written(cosine));
        said += "true\ntrue\n";
    }
    // `sin 0` is 0 and `cos 0` is 1, and an infinity has no sine and no
    // cosine, which are after every float.
    source += "  print(\"{sin(zero) == zero and cos(zero) == 1.0}\")\n";
    source += "  print(\"{sin(infinity) > infinity and cos(infinity) > infinity}\")\n";
    said += "true\ntrue\n";
    source += "}\n";
    (with_header(source), said)
}

fn trigs_are_what_the_reference_says<T: Float>(seed: u64) {
    let (source, said) = generate::sample(seed, 0, trig_program::<T>);
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    assert_eq!(String::from_utf8_lossy(&outcome.stdout), said, "for {source}");
}

fn trigs_on_every_executable<T: Float>(seed: u64) {
    let (source, _) = generate::sample(seed, 0, trig_program::<T>);
    let lowered = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    let outcome = scalar::run(&scalar::passes(&lowered));
    for target in runnable().into_iter().filter(|t: &Target| t.builds_floats()) {
        runs_as(source.as_bytes(), target, &outcome);
    }
}

/// The two functions of the runtime that `sin` and `cos` are made of, which
/// are the reduction of a float by `pi / 2` with as many bits of it as the
/// float's exponent needs, and the series of `sin r` and `cos r` of the eight
/// words of fixed point `r` that it gives, are as near the reference as the
/// comment of `sin` says, 2^-228. A float that is near a multiple of `pi / 2`
/// shows the first only when the bits that are taken are too few, and none of
/// them is within 2^-100 of one, so the floats cannot show this.
fn the_reduction_and_the_series_of_trig_are_within_2_to_the_minus_228_of_the_reference() {
    let source = "use std.console (console, print)\nuse std.float (sin)\n\nfn main() ! console, alloc {\n  print(\"{sin(1.0)}\")\n}\n";
    let program = lower(source.as_bytes()).expect("builds");
    let reduce = program
        .functions
        .iter()
        .position(|f| f.blocks[0].params.len() == 2 && f.results.len() == 10)
        .expect("the reduction is in the program");
    let series = program
        .functions
        .iter()
        .position(|f| f.blocks[0].params.len() == 8 && f.results.len() == 16)
        .expect("the series is in the program");
    let cases: Vec<(u64, i64)> = generate::sample(96, 0, |g| {
        let mut cases = Vec::new();
        let exponents = [-79i64, -60, -27, -10, 0, 1, 5, 40, 100, 500, 900, 971];
        for (k, e) in exponents.iter().enumerate() {
            for j in 0..3 {
                let m = (1u64 << 52) | (g.below(1 << 30) as u64) << 22 | g.below(1 << 22) as u64;
                let e = if k == 0 && j == 0 { 971 } else { *e };
                cases.push((m, e));
            }
        }
        // The greatest and the least mantissas, and a float at 2^k.
        cases.push(((1 << 53) - 1, 971));
        cases.push((1 << 52, 971));
        cases.push(((1 << 53) - 1, -79));
        cases
    });
    let mut worst = i64::MIN;
    for (m, e) in cases {
        let x = Big::from_u64(m).ldexp(e);
        let (quarter, r) = exact::reduce(&x);
        let got = scalar::call(&program, reduce, &[m, e as u64]);
        assert_eq!(got[0], u64::from(quarter), "the quarter of {m} * 2^{e}");
        let magnitude = exact::from_words(&got[2..], 240);
        let signed = if got[1] == 1 { magnitude.negated() } else { magnitude };
        let distance = exact::distance_bits(&signed, &r);
        worst = worst.max(distance);
        assert!(distance < -228, "{m} * 2^{e} is reduced wrongly by about 2^{distance}");
    }
    let inputs: Vec<[u64; 8]> = generate::sample(95, 0, |g| {
        (0..30)
            .map(|k| {
                let mut words = [0u64; 8];
                for w in words.iter_mut().take(7) {
                    *w = g.below(1 << 32) as u64;
                }
                // Below `pi / 4`, which is 51472 of the 65536 of the top word.
                words[7] = g.below(51472) as u64;
                match k {
                    0 => [0; 8],
                    // The greatest, where the series is slowest.
                    1..=6 => {
                        words[7] = 51460 + g.below(10) as u64;
                        words
                    }
                    _ => words,
                }
            })
            .collect()
    });
    for words in inputs {
        let r = exact::from_words(&words, 240);
        let got = scalar::call(&program, series, &words);
        let (sine, cosine) = exact::sin_cos(&r);
        for (part, want) in [(&got[..8], sine), (&got[8..], cosine)] {
            let distance = exact::distance_bits(&exact::from_words(part, 240), &want);
            worst = worst.max(distance);
            assert!(distance < -228, "the series of {words:x?} is wrong by about 2^{distance}");
        }
    }
    eprintln!("the reduction and the series are wrong by under 2^{worst}");
}
