//! The natural logarithm of a float, worked out exactly, written once in the
//! scalar form so that every target gives the same bits, and rounded once.
//!
//! This is the path that is right for every float, and it is not the fast one.
//! The numbers are the fixed point of [`super::exp`]: 256 bits with 240 after
//! the point. `x = m * 2^e` with `m` between 0.70 and 1.42, so that
//! `ln x = e * ln 2 + ln m`, and `ln m = 2 * (z + z^3 / 3 + ...)` for
//! `z = (m - 1) / (m + 1)`, whose magnitude is under 0.18. The quotient is the
//! product by a reciprocal that Newton's steps give from 1, with no division of
//! wide numbers, and the series is summed to 50 terms, which is past 240 bits
//! since `z^2` is under 2^-5. The error is under 2^-228, and the least that
//! `ln` gives of a float of 64 bits that is not 1 is above 2^-54, so the
//! error is under 2^-170 of the result, where the worst cases that are known
//! need under 2^-120.

use super::exp::{
    L, MASK, POINT, WORDS, Wide, add_wide, choose_wide, constant, constants, div_small, mask,
    mul_small, shift_down, sub_wide,
};
use super::ryu::{and, below, below_s, c, eq, looping, not, number, or, sub};
use super::{Binary, Bits, Builder, FloatBits, Function, Type, Value};

/// How many terms of the series are summed, and how many steps of Newton give
/// the reciprocal.
const TERMS: u64 = 50;
const STEPS: u64 = 7;

fn one_wide(f: &mut Builder) -> Wide {
    let mut words = [0u32; WORDS];
    words[POINT / 32] = 1 << (POINT % 32);
    constant(f, &words)
}

/// `v * 2^s` as the words of a number, for a `v` below 2^64 and a `s` that is
/// known.
fn place(f: &mut Builder, v: Value, s: usize) -> Wide {
    let zero = c(f, 0);
    (0..WORDS)
        .map(|i| {
            let low = 32 * i;
            let m = c(f, MASK);
            if s >= low + 32 {
                zero
            } else if s >= low {
                let by = c(f, (s - low) as u64);
                let moved = f.binary(Binary::ShiftLeft, v, by);
                and(f, moved, m)
            } else {
                let by = c(f, (low - s) as u64);
                let moved = f.binary(Binary::ShiftRightU, v, by);
                and(f, moved, m)
            }
        })
        .collect()
}

/// `a` moved up by `k` words.
fn words_up(f: &mut Builder, a: &Wide, k: usize) -> Wide {
    let zero = c(f, 0);
    (0..WORDS)
        .map(|i| if i >= k { a[i - k] } else { zero })
        .collect()
}

/// `a` moved up by `by` bits, which is below 32.
fn bits_up(f: &mut Builder, a: &Wide, by: u64) -> Wide {
    let m = mask(f);
    let zero = c(f, 0);
    (0..WORDS)
        .map(|i| {
            let by_c = c(f, by);
            let moved = f.binary(Binary::ShiftLeft, a[i], by_c);
            let moved = and(f, moved, m);
            let below_word = if i > 0 { a[i - 1] } else { zero };
            let back = c(f, 32 - by);
            let carried = f.binary(Binary::ShiftRightU, below_word, back);
            or(f, moved, carried)
        })
        .collect()
}

/// The words of `a` that are not 0, as a flag.
fn is_zero(f: &mut Builder, a: &Wide) -> Value {
    let mut acc = c(f, 0);
    for word in a {
        acc = or(f, acc, *word);
    }
    let zero = c(f, 0);
    eq(f, acc, zero)
}

/// The function that gives `1 / a` for a number `a` between 0.85 and 1.21.
pub(super) fn reciprocal(multiply: usize) -> Function {
    let mut f = Builder::new();
    let a = f.params(&[L; WORDS]);
    let one = one_wide(&mut f);
    let mut two = one.clone();
    two[POINT / 32] = c(&mut f, 2 << (POINT % 32));
    let mut start = vec![c(&mut f, 0)];
    start.extend(one.iter().copied());
    let done = looping(
        &mut f,
        start,
        |f, v| {
            let last = c(f, STEPS);
            below(f, v[0], last)
        },
        |f, v| {
            let r: Wide = v[1..].to_vec();
            let mut args = a.clone();
            args.extend(r.iter().copied());
            let product = f.call(multiply, args, &[L; WORDS]);
            let (factor, _) = sub_wide(f, &two, &product);
            let mut args = r;
            args.extend(factor);
            let next = f.call(multiply, args, &[L; WORDS]);
            let one = c(f, 1);
            let n = f.binary(Binary::Add, v[0], one);
            let mut out = vec![n];
            out.extend(next);
            out
        },
    );
    f.ret(done[1..].to_vec());
    f.finish(&[L; WORDS])
}

/// The function that gives `z + z^3 / 3 + z^5 / 5 + ...` of a `z` below 0.18.
pub(super) fn series(multiply: usize) -> Function {
    let mut f = Builder::new();
    let z = f.params(&[L; WORDS]);
    let mut args = z.clone();
    args.extend(z.iter().copied());
    let z2 = f.call(multiply, args, &[L; WORDS]);
    let mut start = vec![c(&mut f, 1)];
    start.extend(z.iter().copied());
    start.extend(z.iter().copied());
    let done = looping(
        &mut f,
        start,
        |f, v| {
            let last = c(f, TERMS + 1);
            below(f, v[0], last)
        },
        |f, v| {
            let n = v[0];
            let term: Wide = v[1..1 + WORDS].to_vec();
            let sum: Wide = v[1 + WORDS..].to_vec();
            let mut args = term;
            args.extend(z2.iter().copied());
            let term = f.call(multiply, args, &[L; WORDS]);
            let two = c(f, 2);
            let doubled = f.binary(Binary::Mul, n, two);
            let one = c(f, 1);
            let odd = f.binary(Binary::Add, doubled, one);
            let part = div_small(f, &term, odd);
            let (sum, _) = add_wide(f, &sum, &part);
            let next = f.binary(Binary::Add, n, one);
            let mut out = vec![next];
            out.extend(term);
            out.extend(sum);
            out
        },
    );
    f.ret(done[1 + WORDS..].to_vec());
    f.finish(&[L; WORDS])
}

/// The function that gives `ln(x)` of a float of the size, rounded once.
pub(super) fn ln(double: bool, multiply: usize, reciprocal: usize, series: usize) -> Function {
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
    let zero = c(&mut f, 0);
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
    let is_negative = eq(&mut f, negative, one);

    // A float that is not a number, an infinity, 0, or below 0.
    let (special, _) = f.block(&[]);
    let (not_special, _) = f.block(&[]);
    let is_special = eq(&mut f, field, field_mask);
    f.switch(is_special, vec![(not_special, vec![]), (special, vec![])]);
    f.at(special);
    {
        // `nan` and `inf` are themselves, and `-inf` is `nan`.
        let nothing = eq(&mut f, mantissa, zero);
        let bad = f.binary(Binary::And, nothing, is_negative);
        let nan = literal(&mut f, f64::NAN);
        let result = f.choose(bad, nan, x);
        f.ret(vec![result]);
    }
    f.at(not_special);
    let (zero_block, _) = f.block(&[]);
    let (not_zero, _) = f.block(&[]);
    let without_sign = c(&mut f, (1 << (mantissa_bits + exponent_bits)) - 1);
    let magnitude = and(&mut f, bits, without_sign);
    let is_zero_float = eq(&mut f, magnitude, zero);
    f.switch(
        is_zero_float,
        vec![(not_zero, vec![]), (zero_block, vec![])],
    );
    f.at(zero_block);
    {
        let minus_infinity = literal(&mut f, f64::NEG_INFINITY);
        f.ret(vec![minus_infinity]);
    }
    f.at(not_zero);
    let (below_block, _) = f.block(&[]);
    let (general, _) = f.block(&[]);
    f.switch(is_negative, vec![(general, vec![]), (below_block, vec![])]);
    f.at(below_block);
    {
        let nan = literal(&mut f, f64::NAN);
        f.ret(vec![nan]);
    }
    f.at(general);

    // `x = m * 2^exp2` for a whole `m` whose top bit is at `mantissa_bits`.
    let hidden = c(&mut f, 1 << mantissa_bits);
    let is_subnormal = eq(&mut f, field, zero);
    let with_hidden = or(&mut f, mantissa, hidden);
    let mut m = f.choose(is_subnormal, mantissa, with_hidden);
    let field_used = f.choose(is_subnormal, one, field);
    let offset = c(&mut f, bias + mantissa_bits);
    let mut exp2 = sub(&mut f, field_used, offset);
    for s in [32u64, 16, 8, 4, 2, 1] {
        if s > mantissa_bits {
            continue;
        }
        let limit = c(&mut f, 1 << (mantissa_bits + 1 - s));
        let need = below(&mut f, m, limit);
        let by = c(&mut f, s);
        let shifted = f.binary(Binary::ShiftLeft, m, by);
        m = f.choose(need, shifted, m);
        let lower = sub(&mut f, exp2, by);
        exp2 = f.choose(need, lower, exp2);
    }
    // `x = (m / 2^mantissa_bits) * 2^e`, and when that is not below `sqrt 2`
    // it is halved so that it is not above it.
    let bits_c = c(&mut f, mantissa_bits);
    let e = f.binary(Binary::Add, exp2, bits_c);
    let threshold = c(
        &mut f,
        (2f64.sqrt() * (1u64 << mantissa_bits) as f64) as u64,
    );
    let small = below(&mut f, m, threshold);
    let halve = not(&mut f, small);
    let halved = number(&mut f, halve);
    let e = f.binary(Binary::Add, e, halved);
    let down = place(&mut f, m, POINT - 1 - mantissa_bits as usize);
    let whole = place(&mut f, m, POINT - mantissa_bits as usize);
    let mm = choose_wide(&mut f, halve, &down, &whole);

    let one_w = one_wide(&mut f);
    let (above, borrow) = sub_wide(&mut f, &mm, &one_w);
    let (under, _) = sub_wide(&mut f, &one_w, &mm);
    let m_below = f.narrow(borrow, Bits::B8);
    let magnitude = choose_wide(&mut f, m_below, &under, &above);
    let (total, _) = add_wide(&mut f, &mm, &one_w);
    let half_total = shift_down(&mut f, &total, 1);
    let r = f.call(reciprocal, half_total, &[L; WORDS]);
    let mut args = magnitude;
    args.extend(r);
    let quotient = f.call(multiply, args, &[L; WORDS]);
    let z = shift_down(&mut f, &quotient, 1);
    let s = f.call(series, z, &[L; WORDS]);
    let (b, _) = add_wide(&mut f, &s, &s);

    // `e * ln 2`, with the sign of `e`.
    let (ln2, _) = constants();
    let ln2 = constant(&mut f, &ln2);
    let e_negative = below_s(&mut f, e, zero);
    let flipped = sub(&mut f, zero, e);
    let e_abs = f.choose(e_negative, flipped, e);
    let a = mul_small(&mut f, &ln2, e_abs);

    // `a` and `b` with their signs, `b` being negative when `m` is below 1.
    let same = eq(&mut f, e_negative, m_below);
    let (sum, _) = add_wide(&mut f, &a, &b);
    let (a_minus_b, a_short) = sub_wide(&mut f, &a, &b);
    let (b_minus_a, _) = sub_wide(&mut f, &b, &a);
    let a_less = f.narrow(a_short, Bits::B8);
    let difference = choose_wide(&mut f, a_less, &b_minus_a, &a_minus_b);
    let value = choose_wide(&mut f, same, &sum, &difference);
    let sign_if_different = f.choose(a_less, m_below, e_negative);
    let result_negative = f.choose(same, e_negative, sign_if_different);

    // `x` is 1 when nothing is left.
    let (one_block, _) = f.block(&[]);
    let (rest, _) = f.block(&[]);
    let nothing = is_zero(&mut f, &value);
    f.switch(nothing, vec![(rest, vec![]), (one_block, vec![])]);
    f.at(one_block);
    {
        let nothing = literal(&mut f, 0.0);
        f.ret(vec![nothing]);
    }
    f.at(rest);

    // The top bit moved to the top of the 256: by a whole number of words
    // while the top words are 0, then by bits.
    let mut value = value;
    let mut moved_by = c(&mut f, 0);
    for words in [4usize, 2, 1] {
        let top_zero = {
            let mut acc = c(&mut f, 0);
            for word in &value[WORDS - words..] {
                acc = or(&mut f, acc, *word);
            }
            eq(&mut f, acc, zero)
        };
        let shifted = words_up(&mut f, &value, words);
        value = choose_wide(&mut f, top_zero, &shifted, &value);
        let by = c(&mut f, 32 * words as u64);
        let more = f.binary(Binary::Add, moved_by, by);
        moved_by = f.choose(top_zero, more, moved_by);
    }
    for by in [16u64, 8, 4, 2, 1] {
        let limit = c(&mut f, 1 << (32 - by));
        let top_zero = below(&mut f, value[WORDS - 1], limit);
        let shifted = bits_up(&mut f, &value, by);
        value = choose_wide(&mut f, top_zero, &shifted, &value);
        let amount = c(&mut f, by);
        let more = f.binary(Binary::Add, moved_by, amount);
        moved_by = f.choose(top_zero, more, moved_by);
    }
    // The 64 top bits, of which the top `mantissa_bits + 1` are kept.
    let thirty_two = c(&mut f, 32);
    let high = f.binary(Binary::ShiftLeft, value[7], thirty_two);
    let top = or(&mut f, high, value[6]);
    let cut = 64 - (mantissa_bits + 1);
    let cut_c = c(&mut f, cut);
    let kept = f.binary(Binary::ShiftRightU, top, cut_c);
    let before = c(&mut f, cut - 1);
    let half = f.binary(Binary::ShiftRightU, top, before);
    let half = and(&mut f, half, one);
    let half_flag = {
        let z = eq(&mut f, half, zero);
        not(&mut f, z)
    };
    let low_mask = c(&mut f, (1 << (cut - 1)) - 1);
    let low = and(&mut f, top, low_mask);
    let mut rest_bits = low;
    for word in &value[..6] {
        rest_bits = or(&mut f, rest_bits, *word);
    }
    let sticky = {
        let z = eq(&mut f, rest_bits, zero);
        not(&mut f, z)
    };
    let odd = {
        let bit = and(&mut f, kept, one);
        let z = eq(&mut f, bit, zero);
        not(&mut f, z)
    };
    let more = f.binary(Binary::Or, sticky, odd);
    let up = f.binary(Binary::And, half_flag, more);
    let up = number(&mut f, up);
    let kept = f.binary(Binary::Add, kept, up);
    // The leading bit is at `15 - moved_by` of the power of 2, and `kept`
    // holds it, which adds the 1 that `biased - 1` leaves out.
    let top_exponent = c(&mut f, 14 + bias);
    let before_hidden = sub(&mut f, top_exponent, moved_by);
    let by = c(&mut f, mantissa_bits);
    let field_part = f.binary(Binary::ShiftLeft, before_hidden, by);
    let result = f.binary(Binary::Add, field_part, kept);
    let sign_number = number(&mut f, result_negative);
    let by = c(&mut f, mantissa_bits + exponent_bits);
    let sign_part = f.binary(Binary::ShiftLeft, sign_number, by);
    let result = or(&mut f, result, sign_part);
    let out = if double {
        result
    } else {
        f.narrow(result, Bits::B32)
    };
    let out = f.float_from_bits(out);
    f.ret(vec![out]);
    f.finish(&[float])
}
