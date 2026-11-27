//! The reference that the tests of `exp`, `ln`, `sin`, and `cos` hold the
//! functions to is right: it gives the correctly rounded value that is known
//! for a few numbers, and what the host's functions give, within a unit in the
//! last place, for the rest, and it gives the same float at two precisions.

use crate::Test;
use crate::exact::{self, Big};
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "the_reference_gives_the_correctly_rounded_value_of_numbers_whose_value_is_known",
        the_reference_gives_the_correctly_rounded_value_of_numbers_whose_value_is_known,
    ),
    (
        "the_reference_is_within_a_unit_in_the_last_place_of_the_host_for_generated_numbers_part_1_of_4",
        || the_reference_is_within_a_unit_in_the_last_place_of_the_host_for_generated_numbers(0),
    ),
    (
        "the_reference_is_within_a_unit_in_the_last_place_of_the_host_for_generated_numbers_part_2_of_4",
        || the_reference_is_within_a_unit_in_the_last_place_of_the_host_for_generated_numbers(1),
    ),
    (
        "the_reference_is_within_a_unit_in_the_last_place_of_the_host_for_generated_numbers_part_3_of_4",
        || the_reference_is_within_a_unit_in_the_last_place_of_the_host_for_generated_numbers(2),
    ),
    (
        "the_reference_is_within_a_unit_in_the_last_place_of_the_host_for_generated_numbers_part_4_of_4",
        || the_reference_is_within_a_unit_in_the_last_place_of_the_host_for_generated_numbers(3),
    ),
    (
        "the_reference_rounds_to_the_nearest_and_the_even_one_of_two_as_near",
        the_reference_rounds_to_the_nearest_and_the_even_one_of_two_as_near,
    ),
];

fn the_reference_gives_the_correctly_rounded_value_of_numbers_whose_value_is_known() {
    // `e`, `ln 2`, and the sine and cosine of 1, as `f64` constants of the
    // standard library give them, which are the correctly rounded ones.
    assert_eq!(exact::exp_f64(1.0), std::f64::consts::E.to_bits());
    assert_eq!(exact::ln_f64(2.0), std::f64::consts::LN_2.to_bits());
    assert_eq!(exact::sin_f64(1.0), 0x3feaed548f090cee);
    assert_eq!(exact::cos_f64(1.0), 0x3fe14a280fb5068c);
    // `pi` is the one the standard library gives, and the sine of the nearest
    // float to it is what it is: about 1.2246e-16.
    assert_eq!(exact::cos_f64(std::f64::consts::PI), (-1.0f64).to_bits());
    assert_eq!(exact::sin_f64(std::f64::consts::PI), 0x3ca1a62633145c07);
    // A few numbers whose value is exact.
    assert_eq!(exact::exp_f64(0.0), 1.0f64.to_bits());
    assert_eq!(exact::ln_f64(1.0), 0);
    assert_eq!(exact::sin_f64(0.0), 0);
    assert_eq!(exact::cos_f64(0.0), 1.0f64.to_bits());
    // The edges of the range: past the greatest float is infinity, and the
    // least subnormal is where a half of it is rounded.
    assert_eq!(exact::exp_f64(710.0), f64::INFINITY.to_bits());
    assert_eq!(exact::exp_f64(-746.0), 0);
    assert_eq!(exact::exp_f64(-745.0), 1);
    assert_eq!(exact::exp_f32(89.0), f32::INFINITY.to_bits());
    assert_eq!(exact::exp_f32(1.0), std::f32::consts::E.to_bits());
    assert_eq!(exact::ln_f32(2.0), std::f32::consts::LN_2.to_bits());
    assert_eq!(exact::sin_f32(1.0), 0x3f576aa4);
    assert_eq!(exact::cos_f32(1.0), 0x3f0a5140);
}

/// The distance in floats between two numbers of the same sign.
fn apart(a: u64, b: u64) -> u64 {
    a.max(b) - a.min(b)
}

fn numbers(g: &mut Gen, range: f64) -> f64 {
    let unit = g.below(1 << 30) as f64 / f64::from(1u32 << 30);
    (unit * 2.0 - 1.0) * range
}

fn the_reference_is_within_a_unit_in_the_last_place_of_the_host_for_generated_numbers(part: usize) {
    let xs: Vec<f64> = generate::sample(70, 0, |g| {
        (0..12)
            .map(|k| numbers(g, [1.0, 10.0, 700.0, 100.0][k % 4]))
            .collect()
    });
    for x in xs.into_iter().skip(part * 3).take(3) {
        let (e, host) = (exact::exp_f64(x), x.exp().to_bits());
        assert!(apart(e, host) <= 1, "exp({x}): {e:#x} for {host:#x}");
        let (s, host) = (exact::sin_f64(x), x.sin());
        let host = host.to_bits();
        assert!(s >> 63 == host >> 63 && apart(s & !(1 << 63), host & !(1 << 63)) <= 1, "sin({x}): {s:#x} for {host:#x}");
        let (c, host) = (exact::cos_f64(x), x.cos());
        let host = host.to_bits();
        assert!(c >> 63 == host >> 63 && apart(c & !(1 << 63), host & !(1 << 63)) <= 1, "cos({x}): {c:#x} for {host:#x}");
        let y = x.abs() + 0.001;
        let (l, host) = (exact::ln_f64(y), y.ln());
        let host = host.to_bits();
        assert!(l >> 63 == host >> 63 && apart(l & !(1 << 63), host & !(1 << 63)) <= 1, "ln({y}): {l:#x} for {host:#x}");
    }
}

fn the_reference_rounds_to_the_nearest_and_the_even_one_of_two_as_near() {
    // A number half way between two floats is rounded to the even one: one
    // and a half units above 1.0 is 1 + 2^-52 * 1.5, between 1 + 2^-52 and
    // 1 + 2^-51, and the even one is the second.
    let one = Big::from_u64(1);
    let unit = Big::from_u64(1).ldexp(-52);
    let up = one.add(&unit.mul(&Big::from_u64(3)).ldexp(-1));
    assert_eq!(up.to_f64_bits(), (1.0f64 + 2.0 * f64::EPSILON).to_bits());
    let down = one.add(&unit.ldexp(-1));
    assert_eq!(down.to_f64_bits(), 1.0f64.to_bits());
    // A number above the half is rounded up, and one below it down.
    let above = down.add(&Big::from_u64(1).ldexp(-100));
    assert_eq!(above.to_f64_bits(), (1.0f64 + f64::EPSILON).to_bits());
    // A subnormal and the greatest number.
    assert_eq!(Big::from_u64(1).ldexp(-1074).to_f64_bits(), 1);
    assert_eq!(Big::from_u64(1).ldexp(-1075).to_f64_bits(), 0);
    assert_eq!(Big::from_u64(3).ldexp(-1076).to_f64_bits(), 1);
    assert_eq!(Big::from_u64(1).ldexp(1024).to_f64_bits(), f64::INFINITY.to_bits());
    assert_eq!(Big::from_u64(1).ldexp(-149).to_f32_bits(), 1);
    assert_eq!(Big::from_i64(-3).to_f32_bits(), (-3.0f32).to_bits());
}
