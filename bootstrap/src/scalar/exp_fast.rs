//! The exponential of a float by the way that is fast, which says whether it
//! can tell what the nearest float is, and leaves the rest to [`super::exp`],
//! which is right for every float and is not fast.
//!
//! This is the strategy of Ziv: work the result out to a few bits more than a
//! float has, and if what is left over cannot change which float is nearest,
//! that float is the answer, and otherwise work it out again, exactly. The
//! answer is the one that [`super::exp`] gives whenever this one gives any,
//! because of the bound that follows.
//!
//! # The method
//!
//! A float `x` is cut as `x = N * ln2 / 64 + r` with `N` the whole number
//! nearest `x * 64 / ln2`, so that `|r| <= ln2 / 128 + 2^-44` (about 0.00543),
//! and `N = 64 m + j` with `j` from 0 to 63. Then
//! `exp(x) = 2^m * 2^(j / 64) * (1 + u)` with `u = exp(r) - 1`. The 64 numbers
//! `2^(j / 64)` are a table of pairs of floats, `th + tl`, the second the
//! rounding error of the first, so that `th + tl` is within 2^-107 of
//! `2^(j / 64)`. All of it is arithmetic on floats of 64 bits, rounded to the
//! nearest, so every target gives the same bits, and a float of 32 bits is
//! widened first, which is exact.
//!
//! # The bound
//!
//! Each step is given with the error it adds, as a part of the result, which is
//! about `th`, between 1 and 2. `u` is the unit roundoff, 2^-53.
//!
//! 1. `rh = x - N * L1` is exact. `L1` is `ln2 / 64` cut to 36 bits, so
//!    `N * L1` is exact for `|N| < 2^17` (the greatest here is about 2^16), the
//!    difference is a multiple of 2^-60 below 2^-7, which 53 bits hold, and the
//!    fused instruction rounds it once, which is not at all.
//! 2. `L2` is `ln2 / 64 - L1` as a float, and `p = N * L2` has its rounding error
//!    `pe` from one fused instruction, so `p + pe = N * L2` exactly. `s` and `e`
//!    are the sum `rh - p` and its error by TwoSum, exact. `rl = e - pe` is
//!    rounded, by at most 2^-113. `rhh, rll` are `s + rl` by FastTwoSum, exact
//!    to the same. What is left out is `N * L3` for `L3 = ln2 / 64 - L1 - L2`,
//!    which is under 2^-98, so under 2^-82 in all. `r = rhh + rll` is within
//!    2^-82 of the `r` that is meant, and `exp` changes an error of that size
//!    by a part of 0.6%.
//! 3. `u` is `r + r^2 / 2 + q` with `q = r^3 * P(r)` and `P` Horner's rule
//!    through `r^5 / 40320` of the series, so the series is cut after `r^8`, and
//!    what is left out is under `|r|^9 / 9! < 2^-86`. `r^2 = a + b` exactly, `a`
//!    the rounded square and `b` its error by one fused instruction, and the
//!    parts of `u` are `rhh`, `a / 2`, `b / 2 + rhh * rll`, `rll`, and `q`.
//! 4. `q` is at most `r^3 / 6 < 2^-25.1` and has the roundings of five products
//!    and sums, at most 5u of itself: under 2^-75.8. The four sums that make
//!    `lo` have at most `u` of a number under 2^-25.1 each: under 2^-76.1 in
//!    all. So `u = hi + lo` is within 2^-74.9 of the `u` of `r`.
//! 5. `p2 = th * hi` has its error by one fused instruction, `yh, e2` are
//!    `th + p2` and the error of it by TwoSum, exact, and `ylo` is the sum of
//!    `e2`, `p2e`, `th * lo`, `tl`, and `tl * hi`: four sums and a product of at
//!    most `u` of a number under 2^-24.1 each, under 2^-75.1. The 2^-107 of the
//!    table is below it.
//! 6. `rhi = yh + ylo` and `rlo` its error are exact.
//!
//! So `|rhi + rlo - exp(x) / 2^m| <= 2^-73.9 * exp(x) / 2^m`, and the bound that
//! is used, which is `E`, is 2^-72 for a float of 64 bits, and 2^-71 for one of
//! 32, where the difference from the nearest float of 32 is a sum with a
//! rounding of its own, under 2^-77.
//!
//! # Whether it is done
//!
//! The nearest float to the exact result is `rhi` when the exact result is
//! within half a unit in the last place of `rhi`, the half below being a half
//! less where `rhi` is a power of 2, and `rlo` is where `rhi + rlo` is, so it
//! is when `rlo + E * rhi` is below the half above and `rlo - E * rhi` is above
//! minus the half below, both strictly. When it is not, which is about 1 in
//! 300,000 floats, the exact function gives the answer. A result that is not a
//! float of the size's normal numbers, and a float that is not a number or is
//! out of range, are left to it as well.

use super::runtime::after;
use super::ryu::{and, c, eq};
use super::{
    Binary, Bits, Builder, FloatBinary, FloatBits, FloatCompare, Function, Fused, Type, Value,
};

const K: u64 = 0x40571547652b82fe; // 92.33248261689366
const L1: u64 = 0x3f862e42fefa0000; // 0.010830424696223417
const L2: u64 = 0x3d1cf79abc9e3b3a; // 2.572804622327669e-14
const INV6: u64 = 0x3fc5555555555555; // 0.16666666666666666
const INV24: u64 = 0x3fa5555555555555; // 0.041666666666666664
const INV120: u64 = 0x3f81111111111111; // 0.008333333333333333
const INV720: u64 = 0x3f56c16c16c16c17; // 0.001388888888888889
const INV5040: u64 = 0x3f2a01a01a01a01a; // 0.0001984126984126984
const INV40320: u64 = 0x3efa01a01a01a01a; // 2.48015873015873e-05
const TABLE: [(u64, u64); 64] = [
    (0x3ff0000000000000, 0x0000000000000000),
    (0x3ff02c9a3e778061, 0xbc719083535b085d),
    (0x3ff059b0d3158574, 0x3c8d73e2a475b465),
    (0x3ff0874518759bc8, 0x3c6186be4bb284ff),
    (0x3ff0b5586cf9890f, 0x3c98a62e4adc610b),
    (0x3ff0e3ec32d3d1a2, 0x3c403a1727c57b53),
    (0x3ff11301d0125b51, 0xbc96c51039449b3a),
    (0x3ff1429aaea92de0, 0xbc932fbf9af1369e),
    (0x3ff172b83c7d517b, 0xbc819041b9d78a76),
    (0x3ff1a35beb6fcb75, 0x3c8e5b4c7b4968e4),
    (0x3ff1d4873168b9aa, 0x3c9e016e00a2643c),
    (0x3ff2063b88628cd6, 0x3c8dc775814a8495),
    (0x3ff2387a6e756238, 0x3c99b07eb6c70573),
    (0x3ff26b4565e27cdd, 0x3c82bd339940e9d9),
    (0x3ff29e9df51fdee1, 0x3c8612e8afad1255),
    (0x3ff2d285a6e4030b, 0x3c90024754db41d5),
    (0x3ff306fe0a31b715, 0x3c86f46ad23182e4),
    (0x3ff33c08b26416ff, 0x3c932721843659a6),
    (0x3ff371a7373aa9cb, 0xbc963aeabf42eae2),
    (0x3ff3a7db34e59ff7, 0xbc75e436d661f5e3),
    (0x3ff3dea64c123422, 0x3c8ada0911f09ebc),
    (0x3ff4160a21f72e2a, 0xbc5ef3691c309278),
    (0x3ff44e086061892d, 0x3c489b7a04ef80d0),
    (0x3ff486a2b5c13cd0, 0x3c73c1a3b69062f0),
    (0x3ff4bfdad5362a27, 0x3c7d4397afec42e2),
    (0x3ff4f9b2769d2ca7, 0xbc94b309d25957e3),
    (0x3ff5342b569d4f82, 0xbc807abe1db13cad),
    (0x3ff56f4736b527da, 0x3c99bb2c011d93ad),
    (0x3ff5ab07dd485429, 0x3c96324c054647ad),
    (0x3ff5e76f15ad2148, 0x3c9ba6f93080e65e),
    (0x3ff6247eb03a5585, 0xbc9383c17e40b497),
    (0x3ff6623882552225, 0xbc9bb60987591c34),
    (0x3ff6a09e667f3bcd, 0xbc9bdd3413b26456),
    (0x3ff6dfb23c651a2f, 0xbc6bbe3a683c88ab),
    (0x3ff71f75e8ec5f74, 0xbc816e4786887a99),
    (0x3ff75feb564267c9, 0xbc90245957316dd3),
    (0x3ff7a11473eb0187, 0xbc841577ee04992f),
    (0x3ff7e2f336cf4e62, 0x3c705d02ba15797e),
    (0x3ff82589994cce13, 0xbc9d4c1dd41532d8),
    (0x3ff868d99b4492ed, 0xbc9fc6f89bd4f6ba),
    (0x3ff8ace5422aa0db, 0x3c96e9f156864b27),
    (0x3ff8f1ae99157736, 0x3c85cc13a2e3976c),
    (0x3ff93737b0cdc5e5, 0xbc675fc781b57ebc),
    (0x3ff97d829fde4e50, 0xbc9d185b7c1b85d1),
    (0x3ff9c49182a3f090, 0x3c7c7c46b071f2be),
    (0x3ffa0c667b5de565, 0xbc9359495d1cd533),
    (0x3ffa5503b23e255d, 0xbc9d2f6edb8d41e1),
    (0x3ffa9e6b5579fdbf, 0x3c90fac90ef7fd31),
    (0x3ffae89f995ad3ad, 0x3c97a1cd345dcc81),
    (0x3ffb33a2b84f15fb, 0xbc62805e3084d708),
    (0x3ffb7f76f2fb5e47, 0xbc75584f7e54ac3b),
    (0x3ffbcc1e904bc1d2, 0x3c823dd07a2d9e84),
    (0x3ffc199bdd85529c, 0x3c811065895048dd),
    (0x3ffc67f12e57d14b, 0x3c92884dff483cad),
    (0x3ffcb720dcef9069, 0x3c7503cbd1e949db),
    (0x3ffd072d4a07897c, 0xbc9cbc3743797a9c),
    (0x3ffd5818dcfba487, 0x3c82ed02d75b3707),
    (0x3ffda9e603db3285, 0x3c9c2300696db532),
    (0x3ffdfc97337b9b5f, 0xbc91a5cd4f184b5c),
    (0x3ffe502ee78b3ff6, 0x3c839e8980a9cc8f),
    (0x3ffea4afa2a490da, 0xbc9e9c23179c2893),
    (0x3ffefa1bee615a27, 0x3c9dc7f486a4b6b0),
    (0x3fff50765b6e4540, 0x3c99d3e12dd8a18b),
    (0x3fffa7c1819e90d8, 0x3c874853f3a5931e),
];

/// The table as bytes: for each `j`, the two floats `th` and `tl`, each as 8
/// bytes, the least first.
pub fn table() -> Vec<u8> {
    let mut out = Vec::with_capacity(64 * 16);
    for (high, low) in TABLE {
        out.extend_from_slice(&high.to_le_bytes());
        out.extend_from_slice(&low.to_le_bytes());
    }
    out
}

const F64: Type = Type::Float(FloatBits::F64);

fn lit(f: &mut Builder, bits: u64) -> Value {
    f.float(FloatBits::F64, bits)
}

fn number(f: &mut Builder, v: f64) -> Value {
    f.float(FloatBits::F64, v.to_bits())
}

fn add(f: &mut Builder, a: Value, b: Value) -> Value {
    f.float_binary(FloatBinary::Add, a, b)
}

fn sub(f: &mut Builder, a: Value, b: Value) -> Value {
    f.float_binary(FloatBinary::Sub, a, b)
}

fn mul(f: &mut Builder, a: Value, b: Value) -> Value {
    f.float_binary(FloatBinary::Mul, a, b)
}

/// `a + b` as the sum and its error, which together are `a + b` exactly.
fn two_sum(f: &mut Builder, a: Value, b: Value) -> (Value, Value) {
    let s = add(f, a, b);
    let bb = sub(f, s, a);
    let inner = sub(f, s, bb);
    let left = sub(f, a, inner);
    let right = sub(f, b, bb);
    (s, add(f, left, right))
}

/// What `exp(x)` is, as `(rhi + rlo) * 2^m`, for a float `x` of 64 bits, with
/// a relative error of at most 2^-73.9. `m` is a number of 64 bits that is
/// meant as signed.
fn core(f: &mut Builder, x: Value, table: Value) -> (Value, Value, Value) {
    let k = lit(f, K);
    let kf = mul(f, x, k);
    let zero = number(f, 0.0);
    let non_negative = f.float_compare(FloatCompare::GreaterEqual, kf, zero);
    let plus = number(f, 0.5);
    let minus = number(f, -0.5);
    let half = f.choose(non_negative, plus, minus);
    let shifted = add(f, kf, half);
    let n = f.int_from_float(true, shifted, Bits::B64);
    let six = c(f, 6);
    let m = f.binary(Binary::ShiftRightS, n, six);
    let sixty_three = c(f, 63);
    let j = and(f, n, sixty_three);
    let nf = f.float_from_int(true, n, FloatBits::F64);
    // `r = x - N * (L1 + L2)` as a pair of floats.
    let l1 = lit(f, L1);
    let l2 = lit(f, L2);
    let rh = f.float_fused(Fused::NegMulAdd, nf, l1, x);
    let p = mul(f, nf, l2);
    let pe = f.float_fused(Fused::MulSub, nf, l2, p);
    let minus_p = f.float_negate(p);
    let (s, e) = two_sum(f, rh, minus_p);
    let rl = sub(f, e, pe);
    let rhh = add(f, s, rl);
    let back = sub(f, rhh, s);
    let rll = sub(f, rl, back);
    // `r^2 = a + b`, and `q = r^3 * P(r)`.
    let a = mul(f, rhh, rhh);
    let b = f.float_fused(Fused::MulSub, rhh, rhh, a);
    let mut poly = lit(f, INV40320);
    for coefficient in [INV5040, INV720, INV120, INV24, INV6] {
        let next = lit(f, coefficient);
        poly = f.float_fused(Fused::MulAdd, poly, rhh, next);
    }
    let r3 = mul(f, a, rhh);
    let q = mul(f, r3, poly);
    // `u = r + r^2 / 2 + q` as `hi + lo`.
    let half_c = number(f, 0.5);
    let half_a = mul(f, a, half_c);
    let (hi, e1) = two_sum(f, rhh, half_a);
    let cross = mul(f, rhh, rll);
    let t1 = add(f, cross, q);
    let half_b = mul(f, b, half_c);
    let t2 = add(f, half_b, t1);
    let t3 = add(f, rll, t2);
    let lo = add(f, e1, t3);
    // The table: `th + tl` is `2^(j / 64)`.
    let four = c(f, 4);
    let offset = f.binary(Binary::ShiftLeft, j, four);
    let at = after(f, table, offset);
    let eight = c(f, 8);
    let at_low = after(f, at, eight);
    let th = f.load(F64, at);
    let tl = f.load(F64, at_low);
    // `2^(j / 64) * (1 + u)`.
    let p2 = mul(f, th, hi);
    let p2e = f.float_fused(Fused::MulSub, th, hi, p2);
    let (yh, e2) = two_sum(f, th, p2);
    let tl_hi = mul(f, tl, hi);
    let i1 = add(f, tl, tl_hi);
    let th_lo = mul(f, th, lo);
    let i2 = add(f, th_lo, i1);
    let i3 = add(f, p2e, i2);
    let ylo = add(f, e2, i3);
    let rhi = add(f, yh, ylo);
    let carried = sub(f, rhi, yh);
    let rlo = sub(f, ylo, carried);
    (rhi, rlo, m)
}

/// The function that gives `exp(x)` of a float of the size, and whether it is
/// the float nearest the exact one: 1, or 0 when the exact function must say.
pub(super) fn fast(double: bool) -> Function {
    let size = if double {
        FloatBits::F64
    } else {
        FloatBits::F32
    };
    let float = Type::Float(size);
    let mut f = Builder::new();
    let params = f.params(&[float, Type::Address]);
    let (x, table) = (params[0], params[1]);
    let xd = if double {
        x
    } else {
        f.float_resize(x, FloatBits::F64)
    };
    // The range in which the result is a normal number of the size.
    let (low, high) = if double {
        (-700.0, 700.0)
    } else {
        (-86.0, 88.0)
    };
    let low = number(&mut f, low);
    let high = number(&mut f, high);
    let above = f.float_compare(FloatCompare::GreaterEqual, xd, low);
    let below = f.float_compare(FloatCompare::LessEqual, xd, high);
    let in_range = f.binary(Binary::And, above, below);
    let (rhi, rlo, m) = core(&mut f, xd, table);
    let zero = c(&mut f, 0);
    // The value that is held to be the float, and the margins of the nearest.
    let (value_bits, done) = if double {
        let bits = f.float_bits(rhi);
        let mask = c(&mut f, (1 << 52) - 1);
        let mantissa = and(&mut f, bits, mask);
        let fifty_two = c(&mut f, 52);
        let moved = f.binary(Binary::ShiftRightU, bits, fifty_two);
        let eleven = c(&mut f, 0x7ff);
        let field = and(&mut f, moved, eleven);
        // Half the spacing of the floats at `rhi`: 2^(field - 1023 - 53), which
        // has the field `field - 53`.
        let fifty_three = c(&mut f, 53);
        let half_field = f.binary(Binary::Sub, field, fifty_three);
        let half_bits = f.binary(Binary::ShiftLeft, half_field, fifty_two);
        let half = f.float_from_bits(half_bits);
        let power = eq(&mut f, mantissa, zero);
        let quarter_c = number(&mut f, 0.5);
        let quarter = mul(&mut f, half, quarter_c);
        let below_half = f.choose(power, quarter, half);
        let error_scale = number(&mut f, 2f64.powi(-72));
        let error = mul(&mut f, rhi, error_scale);
        let high_edge = add(&mut f, rlo, error);
        let low_edge = sub(&mut f, rlo, error);
        let minus_below = f.float_negate(below_half);
        let up = f.float_compare(FloatCompare::Less, high_edge, half);
        let down = f.float_compare(FloatCompare::Greater, low_edge, minus_below);
        let certain = f.binary(Binary::And, up, down);
        let exponent = f.binary(Binary::ShiftLeft, m, fifty_two);
        let rounded = f.binary(Binary::Add, bits, exponent);
        (rounded, certain)
    } else {
        let narrow = f.float_resize(rhi, FloatBits::F32);
        let wide = f.float_resize(narrow, FloatBits::F64);
        let deviation_exact = sub(&mut f, rhi, wide);
        let deviation = add(&mut f, deviation_exact, rlo);
        let bits32 = f.float_bits(narrow);
        let bits = f.widen(false, bits32, Bits::B64);
        let mask = c(&mut f, (1 << 23) - 1);
        let mantissa = and(&mut f, bits, mask);
        let twenty_three = c(&mut f, 23);
        let moved = f.binary(Binary::ShiftRightU, bits, twenty_three);
        let ff = c(&mut f, 0xff);
        let field = and(&mut f, moved, ff);
        // Half the spacing of the floats of 32 at `narrow`, as a float of 64:
        // 2^(field - 127 - 24), which has the field `field - 151 + 1023`.
        let shift = c(&mut f, 1023 - 151);
        let half_field = f.binary(Binary::Add, field, shift);
        let fifty_two = c(&mut f, 52);
        let half_bits = f.binary(Binary::ShiftLeft, half_field, fifty_two);
        let half = f.float_from_bits(half_bits);
        let power = eq(&mut f, mantissa, zero);
        let quarter_c = number(&mut f, 0.5);
        let quarter = mul(&mut f, half, quarter_c);
        let below_half = f.choose(power, quarter, half);
        let error_scale = number(&mut f, 2f64.powi(-71));
        let error = mul(&mut f, rhi, error_scale);
        let high_edge = add(&mut f, deviation, error);
        let low_edge = sub(&mut f, deviation, error);
        let minus_below = f.float_negate(below_half);
        let up = f.float_compare(FloatCompare::Less, high_edge, half);
        let down = f.float_compare(FloatCompare::Greater, low_edge, minus_below);
        let certain = f.binary(Binary::And, up, down);
        let exponent = f.binary(Binary::ShiftLeft, m, twenty_three);
        let rounded = f.binary(Binary::Add, bits, exponent);
        (rounded, certain)
    };
    let out_bits = if double {
        value_bits
    } else {
        f.narrow(value_bits, Bits::B32)
    };
    let value = f.float_from_bits(out_bits);
    let flag = f.binary(Binary::And, in_range, done);
    f.ret(vec![value, flag]);
    f.finish(&[float, Type::Int(Bits::B8)])
}

/// The function that gives `exp(x)`: the float that `fast` gives when it is
/// sure of it, and otherwise the one `exact` gives.
pub(super) fn exp(double: bool, fast: usize, exact: usize) -> Function {
    let size = if double {
        FloatBits::F64
    } else {
        FloatBits::F32
    };
    let float = Type::Float(size);
    let mut f = Builder::new();
    let params = f.params(&[float, Type::Address]);
    let results = f.call(
        fast,
        vec![params[0], params[1]],
        &[float, Type::Int(Bits::B8)],
    );
    let (done, not_done) = (f.block(&[]).0, f.block(&[]).0);
    f.switch(results[1], vec![(not_done, vec![]), (done, vec![])]);
    f.at(done);
    f.ret(vec![results[0]]);
    f.at(not_done);
    let value = f.call(exact, vec![params[0]], &[float]);
    f.ret(value);
    f.finish(&[float])
}
