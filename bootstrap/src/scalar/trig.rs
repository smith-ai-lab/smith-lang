//! The sine and the cosine of a float, worked out exactly, written once in the
//! scalar form so that every target gives the same bits, and rounded once.
//!
//! The numbers are the fixed point of [`super::exp`]: 256 bits with 240 after
//! the point. `x = m * 2^e` is multiplied by `2 / pi` with as many bits of it
//! as `e` asks for, which is the reduction of Payne and Hanek: only the bits of
//! `2 / pi` that can change `x * 2 / pi` modulo 4 and down to 2^-265 are
//! taken, a window of 11 words that the exponent picks from a table of 1312
//! bits, so a float as great as `2^1023` is reduced exactly. What is left is
//! the quarter of the circle, `N`, and the fraction `f` between -0.5 and 0.5,
//! and `r = f * pi / 2` is at most `pi / 4`. The series of `sin r` and
//! `cos r` are summed to 30 terms, which is past 240 bits since `r^2` is under
//! 2^-0.7, and the quarter picks which of them, and with which sign, is the
//! result, which is rounded once. The error is under 2^-228, the least that
//! `sin` or `cos` gives of a float that is not near 0 is above 2^-62, so it is
//! under 2^-166 of the result, where the worst cases that are known need under
//! 2^-120.

use super::exp::{
    L, MASK, POINT, WORDS, Wide, add_wide, choose_wide, constant, div_small, sub_wide,
};
use super::ln::round_fixed;
use super::ryu::{and, below, c, eq, looping, not, number, or, sub};
use super::{Binary, Bits, Builder, FloatBits, Function, Type, Value};

/// How many terms of each series are summed.
const TERMS: u64 = 30;
/// The words of `2 / pi` that the window holds, and how many words of it the
/// table holds past the three of 0 that stand for the bits before the first.
const WINDOW: usize = 11;
const PAD: usize = 3;
const TABLE: usize = 41;

// What the constants are made of: numbers of 32-bit limbs, the least first.

const LIMBS: usize = 48;

fn small_div(v: &mut [u32], d: u32) {
    let mut rest = 0u64;
    for limb in v.iter_mut().rev() {
        let t = (rest << 32) | u64::from(*limb);
        *limb = (t / u64::from(d)) as u32;
        rest = t % u64::from(d);
    }
}

fn small_mul(v: &mut [u32], m: u32) {
    let mut carry = 0u64;
    for limb in v.iter_mut() {
        let t = u64::from(*limb) * u64::from(m) + carry;
        *limb = t as u32;
        carry = t >> 32;
    }
}

fn big_add(a: &mut [u32], b: &[u32]) {
    let mut carry = 0u64;
    for (x, y) in a.iter_mut().zip(b) {
        let t = u64::from(*x) + u64::from(*y) + carry;
        *x = t as u32;
        carry = t >> 32;
    }
}

fn big_sub(a: &mut [u32], b: &[u32]) {
    let mut borrow = 0i64;
    for (x, y) in a.iter_mut().zip(b) {
        let t = i64::from(*x) - i64::from(*y) - borrow;
        *x = t as u32;
        borrow = i64::from(t < 0);
    }
}

fn at_least(a: &[u32], b: &[u32]) -> bool {
    for (x, y) in a.iter().zip(b).rev() {
        if x != y {
            return x > y;
        }
    }
    true
}

fn is_nothing(v: &[u32]) -> bool {
    v.iter().all(|x| *x == 0)
}

/// `atan(1 / n)` with `32 * LIMBS` bits after the point, in `LIMBS + 1` limbs.
fn atan_inverse(n: u32) -> Vec<u32> {
    let mut term = vec![0u32; LIMBS + 1];
    term[LIMBS] = 1;
    small_div(&mut term, n);
    let mut sum = vec![0u32; LIMBS + 1];
    let mut k = 0u32;
    while !is_nothing(&term) {
        let mut part = term.clone();
        small_div(&mut part, 2 * k + 1);
        if k.is_multiple_of(2) {
            big_add(&mut sum, &part);
        } else {
            big_sub(&mut sum, &part);
        }
        small_div(&mut term, n * n);
        k += 1;
    }
    sum
}

/// `pi` with `32 * LIMBS` bits after the point.
fn pi_limbs() -> Vec<u32> {
    let mut a = atan_inverse(5);
    let mut b = atan_inverse(239);
    small_mul(&mut a, 16);
    small_mul(&mut b, 4);
    big_sub(&mut a, &b);
    a
}

/// `pi / 2` in fixed point with 240 bits after the point, as eight words.
pub(super) fn half_pi() -> [u32; WORDS] {
    let pi = pi_limbs();
    // The bits of `pi` below the place `2^-241`, dropped, and one more.
    let drop = 32 * LIMBS - POINT;
    let mut out = [0u32; WORDS];
    for (i, word) in out.iter_mut().enumerate() {
        let mut value = 0u64;
        for b in 0..32 {
            // Bit `32 * i + b` of `pi / 2 * 2^240` is bit `32 * i + b + 1 + drop`
            // of `pi * 2^(32 * LIMBS)`.
            let bit = 32 * i + b + 1 + drop;
            if bit < 32 * (LIMBS + 1) && pi[bit / 32] >> (bit % 32) & 1 == 1 {
                value |= 1 << b;
            }
        }
        *word = value as u32;
    }
    out
}

/// The words of `2 / pi` after the point, from the first: the table of the
/// bits that the reduction takes a window of, `TABLE` words long.
fn two_over_pi() -> Vec<u32> {
    let pi = pi_limbs();
    // `2 * 2^(64 * LIMBS) / (pi * 2^(32 * LIMBS))`, by long division.
    let total = 64 * LIMBS + 1;
    let mut quotient = [0u32; LIMBS + 1];
    let mut rest = vec![0u32; LIMBS + 2];
    let mut divisor = pi.clone();
    divisor.resize(LIMBS + 2, 0);
    for bit in (0..=total).rev() {
        // `rest = rest * 2 + the bit of the numerator`, which is 1 only at
        // the top: the numerator is `2^total`.
        let mut carry = u32::from(bit == total);
        for limb in rest.iter_mut() {
            let top = *limb >> 31;
            *limb = (*limb << 1) | carry;
            carry = top;
        }
        if at_least(&rest, &divisor) {
            big_sub(&mut rest, &divisor);
            if bit / 32 < quotient.len() {
                quotient[bit / 32] |= 1 << (bit % 32);
            }
        }
    }
    // The quotient is `2 / pi * 2^(32 * LIMBS)`: word `k` after the point is
    // limb `LIMBS - 1 - k`.
    (0..TABLE).map(|k| quotient[LIMBS - 1 - k]).collect()
}

fn one_wide(f: &mut Builder) -> Wide {
    let mut words = [0u32; WORDS];
    words[POINT / 32] = 1 << (POINT % 32);
    constant(f, &words)
}

/// The function that gives `sin r` and then `cos r`, as sixteen words, for the
/// eight words of an `r` between 0 and 0.8.
pub(super) fn series(multiply: usize) -> Function {
    let mut f = Builder::new();
    let r = f.params(&[L; WORDS]);
    let mut args = r.clone();
    args.extend(r.iter().copied());
    let r2 = f.call(multiply, args, &[L; WORDS]);
    let zero = c(&mut f, 0);
    let one = one_wide(&mut f);
    let mut results = Vec::new();
    // `sin r = r - r^3 / 3! + ...` and `cos r = 1 - r^2 / 2! + ...`: the terms
    // are `t_k = t_(k-1) * r^2 / ((2k)(2k+1))` and `u_k = u_(k-1) * r^2 /
    // ((2k-1)(2k))`, added at the even `k` and taken away at the odd.
    for sine in [true, false] {
        let first: Wide = if sine { r.clone() } else { one.clone() };
        let mut start = vec![c(&mut f, 0)];
        start.extend(first.iter().copied());
        start.extend(std::iter::repeat_n(zero, 2 * WORDS));
        let done = looping(
            &mut f,
            start,
            |f, v| {
                let last = c(f, TERMS);
                below(f, v[0], last)
            },
            |f, v| {
                let k = v[0];
                let term: Wide = v[1..1 + WORDS].to_vec();
                let plus: Wide = v[1 + WORDS..1 + 2 * WORDS].to_vec();
                let minus: Wide = v[1 + 2 * WORDS..].to_vec();
                let one_c = c(f, 1);
                let odd = and(f, k, one_c);
                let is_odd = eq(f, odd, one_c);
                let (added, _) = add_wide(f, &plus, &term);
                let (taken, _) = add_wide(f, &minus, &term);
                let plus = choose_wide(f, is_odd, &plus, &added);
                let minus = choose_wide(f, is_odd, &taken, &minus);
                // The next term.
                let next_k = f.binary(Binary::Add, k, one_c);
                let two = c(f, 2);
                let double = f.binary(Binary::Mul, next_k, two);
                let divisor = if sine {
                    let above = f.binary(Binary::Add, double, one_c);
                    f.binary(Binary::Mul, double, above)
                } else {
                    let below_ = sub(f, double, one_c);
                    f.binary(Binary::Mul, below_, double)
                };
                let mut args = term;
                args.extend(r2.iter().copied());
                let times = f.call(multiply, args, &[L; WORDS]);
                let term = div_small(f, &times, divisor);
                let mut out = vec![next_k];
                out.extend(term);
                out.extend(plus);
                out.extend(minus);
                out
            },
        );
        let plus: Wide = done[1 + WORDS..1 + 2 * WORDS].to_vec();
        let minus: Wide = done[1 + 2 * WORDS..].to_vec();
        let (value, _) = sub_wide(&mut f, &plus, &minus);
        results.extend(value);
    }
    f.ret(results);
    f.finish(&[L; 2 * WORDS])
}

/// The word `index` of a table, for an `index` that is a value: the one of the
/// words that it is.
fn pick(f: &mut Builder, table: &[u32], index: Value, from: usize) -> Value {
    let last = table.len() - 1;
    let mut result = c(f, u64::from(table[last.min(from + 33)]));
    for v in (0..34).rev() {
        let word = c(f, u64::from(table.get(from + v).copied().unwrap_or(0)));
        let at = c(f, v as u64);
        let is = eq(f, index, at);
        result = f.choose(is, word, result);
    }
    result
}

/// The function that reduces `m * 2^e` for a whole `m` of 53 bits (24 for a
/// float of 32), and an `e` that is not below -78: it gives the quarter of the
/// circle that `x` is in, 0 to 3, whether `r` is below 0, which is 1 or 0, and
/// the eight words of `|r|`, which is at most `pi / 4`, where `x` is `N`
/// quarters and `r` more.
pub(super) fn reduce(multiply: usize) -> Function {
    let mut f = Builder::new();
    let params = f.params(&[L; 2]);
    let (m, e) = (params[0], params[1]);
    let zero = c(&mut f, 0);
    let one = c(&mut f, 1);
    // `e - 2 + 96`, which is not below 0 for the floats that get here: the
    // word of the table is `u >> 5` and the bits `m` is moved up by `u & 31`.
    let ninety_four = c(&mut f, 94);
    let u = f.binary(Binary::Add, e, ninety_four);
    let five = c(&mut f, 5);
    let word = f.binary(Binary::ShiftRightU, u, five);
    let thirty_one = c(&mut f, 31);
    let d = and(&mut f, u, thirty_one);
    let mut table = vec![0u32; PAD];
    table.extend(two_over_pi());
    let window: Vec<Value> = (0..WINDOW).map(|j| pick(&mut f, &table, word, j)).collect();
    // `m * 2^d` as three limbs.
    let moved_up = f.binary(Binary::ShiftLeft, m, d);
    let mk = c(&mut f, MASK);
    let a0 = and(&mut f, moved_up, mk);
    let thirty_two = c(&mut f, 32);
    let a1 = f.binary(Binary::ShiftRightU, moved_up, thirty_two);
    // The bits of `m * 2^d` above the 64: `m >> (64 - d)`, in two moves so
    // that `d` of 0 is not a move of 64.
    let sixty_three = c(&mut f, 63);
    let back = sub(&mut f, sixty_three, d);
    let half_m = f.binary(Binary::ShiftRightU, m, one);
    let a2 = f.binary(Binary::ShiftRightU, half_m, back);
    let limbs = [a0, a1, a2];
    // The product with the window, whose last word is its least.
    let mut p = [zero; WINDOW + 3];
    for (i, a) in limbs.iter().enumerate() {
        let mut carry = zero;
        for j in 0..WINDOW {
            let w = window[WINDOW - 1 - j];
            let t = f.binary(Binary::Mul, *a, w);
            let t = f.binary(Binary::Add, t, p[i + j]);
            let t = f.binary(Binary::Add, t, carry);
            p[i + j] = and(&mut f, t, mk);
            carry = f.binary(Binary::ShiftRightU, t, thirty_two);
        }
        p[i + WINDOW] = carry;
    }
    // `x * 2 / pi` is the product with 350 bits after the point: the quarter
    // is the two bits above, and the fraction the 240 bits that follow.
    let thirty = c(&mut f, 30);
    let quarter = f.binary(Binary::ShiftRightU, p[10], thirty);
    let fourteen = c(&mut f, 14);
    let eighteen = c(&mut f, 18);
    let mut fraction: Wide = (0..WORDS)
        .map(|i| {
            let low = f.binary(Binary::ShiftRightU, p[3 + i], fourteen);
            let high = f.binary(Binary::ShiftLeft, p[4 + i], eighteen);
            let high = and(&mut f, high, mk);
            or(&mut f, low, high)
        })
        .collect();
    let sixteen_bits = c(&mut f, 0xffff);
    fraction[7] = and(&mut f, fraction[7], sixteen_bits);
    // At or past a half, the next quarter and the fraction below it.
    let fifteen = c(&mut f, 15);
    let high_bit = f.binary(Binary::ShiftRightU, fraction[7], fifteen);
    let past_half = eq(&mut f, high_bit, one);
    let one_w = one_wide(&mut f);
    let (complement, _) = sub_wide(&mut f, &one_w, &fraction);
    let magnitude = choose_wide(&mut f, past_half, &complement, &fraction);
    let next = f.binary(Binary::Add, quarter, one);
    let quarter = f.choose(past_half, next, quarter);
    let three = c(&mut f, 3);
    let quarter = and(&mut f, quarter, three);
    // `r = |f| * pi / 2`.
    let half_pi = constant(&mut f, &half_pi());
    let mut args = magnitude;
    args.extend(half_pi);
    let r = f.call(multiply, args, &[L; WORDS]);
    let negative = number(&mut f, past_half);
    let mut out = vec![quarter, negative];
    out.extend(r);
    f.ret(out);
    f.finish(&[L; 2 + WORDS])
}

/// The function that gives `sin(x)`, or `cos(x)`, of a float of the size,
/// rounded once.
pub(super) fn trig(double: bool, cosine: bool, reduce: usize, series: usize) -> Function {
    let (mantissa_bits, exponent_bits, bias, size) = if double {
        (52u64, 11u64, 1023u64, FloatBits::F64)
    } else {
        (23, 8, 127, FloatBits::F32)
    };
    let float = Type::Float(size);
    let mut f = Builder::new();
    let x = f.params(&[float])[0];
    let literal = |f: &mut Builder, v: f64| -> Value {
        match size {
            FloatBits::F32 => f.float(size, u64::from((v as f32).to_bits())),
            FloatBits::F64 => f.float(size, v.to_bits()),
        }
    };
    let raw = f.float_bits(x);
    let bits = if double {
        raw
    } else {
        f.widen(false, raw, Bits::B64)
    };
    let one = c(&mut f, 1);
    let mantissa_mask = c(&mut f, (1 << mantissa_bits) - 1);
    let mantissa = and(&mut f, bits, mantissa_mask);
    let by = c(&mut f, mantissa_bits);
    let moved = f.binary(Binary::ShiftRightU, bits, by);
    let field_mask = c(&mut f, (1 << exponent_bits) - 1);
    let field = and(&mut f, moved, field_mask);
    let by = c(&mut f, mantissa_bits + exponent_bits);
    let negative = f.binary(Binary::ShiftRightU, bits, by);
    let negative = and(&mut f, negative, one);
    let x_negative = eq(&mut f, negative, one);

    // A float that is not a number, or an infinity, has none.
    let (special, _) = f.block(&[]);
    let (not_special, _) = f.block(&[]);
    let is_special = eq(&mut f, field, field_mask);
    f.switch(is_special, vec![(not_special, vec![]), (special, vec![])]);
    f.at(special);
    {
        let nan = literal(&mut f, f64::NAN);
        f.ret(vec![nan]);
    }
    f.at(not_special);
    // A float so near 0 that the sine is itself, or the cosine is 1.
    let (near, _) = f.block(&[]);
    let (general, _) = f.block(&[]);
    let cut = if cosine {
        if double { 27 } else { 13 }
    } else if double {
        26
    } else {
        12
    };
    let least_field = c(&mut f, bias - cut);
    let is_near = below(&mut f, field, least_field);
    f.switch(is_near, vec![(general, vec![]), (near, vec![])]);
    f.at(near);
    {
        let result = if cosine { literal(&mut f, 1.0) } else { x };
        f.ret(vec![result]);
    }
    f.at(general);

    // `|x| = m * 2^e`, and the window of `2 / pi` that `e` asks for.
    let hidden = c(&mut f, 1 << mantissa_bits);
    let m = or(&mut f, mantissa, hidden);
    let offset = c(&mut f, bias + mantissa_bits);
    let e = sub(&mut f, field, offset);
    let reduced = f.call(reduce, vec![m, e], &[L; 2 + WORDS]);
    let quarter = reduced[0];
    let past_half = eq(&mut f, reduced[1], one);
    let r: Wide = reduced[2..].to_vec();
    let both = f.call(series, r, &[L; 2 * WORDS]);
    let sine_r: Wide = both[..WORDS].to_vec();
    let cosine_r: Wide = both[WORDS..].to_vec();

    // The quarter of the circle says which of them, and the sign.
    let q_is = |f: &mut Builder, n: u64| {
        let k = c(f, n);
        eq(f, quarter, k)
    };
    let (q0, q1, q2) = (q_is(&mut f, 0), q_is(&mut f, 1), q_is(&mut f, 2));
    // `r` is below 0 when the fraction was past a half.
    let r_negative = past_half;
    let r_positive = not(&mut f, r_negative);
    let always = f.int(Bits::B8, 1);
    let never = f.int(Bits::B8, 0);
    let (first, second, third, fourth) = if cosine {
        (
            (&cosine_r, never),
            (&sine_r, r_positive),
            (&cosine_r, always),
            (&sine_r, r_negative),
        )
    } else {
        (
            (&sine_r, r_negative),
            (&cosine_r, never),
            (&sine_r, r_positive),
            (&cosine_r, always),
        )
    };
    let pick_of = |f: &mut Builder, on: Value, a: (&Wide, Value), b: (&Wide, Value)| {
        (choose_wide(f, on, a.0, b.0), f.choose(on, a.1, b.1))
    };
    let (w23, s23) = pick_of(&mut f, q2, third, fourth);
    let (w123, s123) = pick_of(&mut f, q1, second, (&w23, s23));
    let (value, result_negative) = pick_of(&mut f, q0, first, (&w123, s123));
    // The sine of a number below 0 is the negative of the sine.
    let result_negative = if cosine {
        result_negative
    } else {
        f.binary(Binary::Xor, result_negative, x_negative)
    };
    let out = round_fixed(&mut f, value, result_negative, double);
    f.ret(vec![out]);
    f.finish(&[float])
}
