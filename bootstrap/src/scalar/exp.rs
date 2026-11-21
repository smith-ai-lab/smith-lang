//! The exponential of a float, worked out exactly, written once in the scalar
//! form so that every target gives the same bits, and rounded once.
//!
//! This is the path that is right for every float, and it is not the fast one.
//! A number is held as 256 bits of fixed point with 240 of them after the
//! point, in eight words of 32 bits that each sit in a number of 64. `x` is
//! cut as `x = q * ln 2 + r` with `r` between 0 and `ln 2`, the series of
//! `exp(r / 64)` is summed to 24 terms, which is past the 240 bits, and the
//! result is squared six times. The error is under 2^-228, and a float whose
//! `exp` is that near half way between two floats does not exist, as the
//! worst cases that are known need under 2^-120. The sum and the products are
//! of whole numbers, so every target gives the same bits.

use super::ryu::{and, below, below_s, c, eq, looping, mul, not, number, or, sub};
use super::{Binary, Bits, Builder, FloatBits, FloatCompare, Function, Type, Value};

pub(super) const L: Type = Type::Int(Bits::B64);
pub(super) const WORDS: usize = 8;
pub(super) const MASK: u64 = 0xffff_ffff;
/// The bits after the point.
pub(super) const POINT: usize = 240;
/// How many terms of the series are summed, and how many times the sum is
/// squared after `r` is divided by 2^SQUARES.
const TERMS: u64 = 24;
const SQUARES: usize = 6;
/// The multiple of `ln 2` that makes a number that is not below the least
/// float of any size one that is above 0.
const SHIFT: u64 = 1100;

pub(super) type Wide = Vec<Value>;

/// `ln 2` in fixed point with 240 bits after the point, and `SHIFT` times it,
/// as eight words of 32 bits each, the least first.
pub(super) fn constants() -> ([u32; WORDS], [u32; WORDS]) {
    // The sum of `1 / (k * 2^k)`, with 320 bits after the point, in 11 words.
    const LONG: usize = 11;
    let mut sum = [0u64; LONG];
    for k in 1..=330u64 {
        let mut term = [0u32; LONG];
        let bit = 320usize.checked_sub(k as usize);
        let Some(bit) = bit else { break };
        term[bit / 32] = 1 << (bit % 32);
        // The term divided by `k`.
        let mut rest = 0u64;
        for i in (0..LONG).rev() {
            let t = (rest << 32) | u64::from(term[i]);
            term[i] = (t / k) as u32;
            rest = t % k;
        }
        let mut carry = 0u64;
        for i in 0..LONG {
            let t = sum[i] + u64::from(term[i]) + carry;
            sum[i] = t & MASK;
            carry = t >> 32;
        }
    }
    let to_words = |long: [u64; LONG]| -> [u32; WORDS] {
        // 80 bits are dropped: two words and 16 bits.
        let mut out = [0u32; WORDS];
        for (i, word) in out.iter_mut().enumerate() {
            let low = long[i + 2] >> 16;
            let high = (long.get(i + 3).copied().unwrap_or(0) << 16) & MASK;
            *word = (low | high) as u32;
        }
        out
    };
    let ln2 = to_words(sum);
    let mut many = [0u64; LONG];
    let mut carry = 0u64;
    for i in 0..LONG {
        let t = sum[i] * SHIFT + carry;
        many[i] = t & MASK;
        carry = t >> 32;
    }
    (ln2, to_words(many))
}

pub(super) fn constant(f: &mut Builder, words: &[u32; WORDS]) -> Wide {
    words.iter().map(|w| c(f, u64::from(*w))).collect()
}

pub(super) fn mask(f: &mut Builder) -> Value {
    c(f, MASK)
}

/// `a + b`, and the carry out.
pub(super) fn add_wide(f: &mut Builder, a: &Wide, b: &Wide) -> (Wide, Value) {
    let m = mask(f);
    let mut carry = c(f, 0);
    let mut out = Vec::new();
    for i in 0..WORDS {
        let t = f.binary(Binary::Add, a[i], b[i]);
        let t = f.binary(Binary::Add, t, carry);
        out.push(and(f, t, m));
        let thirty_two = c(f, 32);
        carry = f.binary(Binary::ShiftRightU, t, thirty_two);
    }
    (out, carry)
}

/// `a - b`, and 1 if it was below 0.
pub(super) fn sub_wide(f: &mut Builder, a: &Wide, b: &Wide) -> (Wide, Value) {
    let m = mask(f);
    let two_32 = c(f, 1 << 32);
    let one = c(f, 1);
    let mut borrow = c(f, 0);
    let mut out = Vec::new();
    for i in 0..WORDS {
        let t = f.binary(Binary::Add, a[i], two_32);
        let t = sub(f, t, b[i]);
        let t = sub(f, t, borrow);
        out.push(and(f, t, m));
        let thirty_two = c(f, 32);
        let kept = f.binary(Binary::ShiftRightU, t, thirty_two);
        borrow = sub(f, one, kept);
    }
    (out, borrow)
}

pub(super) fn choose_wide(f: &mut Builder, on: Value, then: &Wide, otherwise: &Wide) -> Wide {
    (0..WORDS)
        .map(|i| f.choose(on, then[i], otherwise[i]))
        .collect()
}

/// `a * m` for a number `m` below 2^32.
pub(super) fn mul_small(f: &mut Builder, a: &Wide, m: Value) -> Wide {
    let mk = mask(f);
    let mut carry = c(f, 0);
    let mut out = Vec::new();
    for word in a {
        let t = mul(f, *word, m);
        let t = f.binary(Binary::Add, t, carry);
        out.push(and(f, t, mk));
        let thirty_two = c(f, 32);
        carry = f.binary(Binary::ShiftRightU, t, thirty_two);
    }
    out
}

/// `a / n` for a number `n` that is not 0 and below 2^32, rounded down.
pub(super) fn div_small(f: &mut Builder, a: &Wide, n: Value) -> Wide {
    let mut rest = c(f, 0);
    let mut out = vec![rest; WORDS];
    for i in (0..WORDS).rev() {
        let thirty_two = c(f, 32);
        let high = f.binary(Binary::ShiftLeft, rest, thirty_two);
        let t = or(f, high, a[i]);
        out[i] = f.binary(Binary::DivU, t, n);
        rest = f.binary(Binary::RemU, t, n);
    }
    out
}

/// The product of two numbers, which is `a * b / 2^240` of their words, rounded
/// down.
fn product(f: &mut Builder, a: &Wide, b: &Wide) -> Wide {
    let m = mask(f);
    let zero = c(f, 0);
    let mut p = [zero; 2 * WORDS];
    for i in 0..WORDS {
        let mut carry = zero;
        for j in 0..WORDS {
            let t = mul(f, a[i], b[j]);
            let t = f.binary(Binary::Add, t, p[i + j]);
            let t = f.binary(Binary::Add, t, carry);
            p[i + j] = and(f, t, m);
            let thirty_two = c(f, 32);
            carry = f.binary(Binary::ShiftRightU, t, thirty_two);
        }
        p[i + WORDS] = carry;
    }
    // Words 7 and after, moved down 16 bits.
    (0..WORDS)
        .map(|j| {
            let sixteen = c(f, 16);
            let low = f.binary(Binary::ShiftRightU, p[7 + j], sixteen);
            let above = if 8 + j < 2 * WORDS { p[8 + j] } else { zero };
            let high = f.binary(Binary::ShiftLeft, above, sixteen);
            let high = and(f, high, m);
            or(f, low, high)
        })
        .collect()
}

/// `a` moved down by `by` bits, which is below 32.
pub(super) fn shift_down(f: &mut Builder, a: &Wide, by: u64) -> Wide {
    let m = mask(f);
    let zero = c(f, 0);
    (0..WORDS)
        .map(|i| {
            let by_c = c(f, by);
            let low = f.binary(Binary::ShiftRightU, a[i], by_c);
            let above = if i + 1 < WORDS { a[i + 1] } else { zero };
            let back = c(f, 32 - by);
            let high = f.binary(Binary::ShiftLeft, above, back);
            let high = and(f, high, m);
            or(f, low, high)
        })
        .collect()
}

/// The low 64 bits of `a >> t`, for any `t`.
fn low_after_shift(f: &mut Builder, a: &Wide, t: Value) -> Value {
    let mut out = c(f, 0);
    let zero = c(f, 0);
    for (i, word) in a.iter().enumerate() {
        let at = c(f, 32 * i as u64);
        // Where the word lies once moved: `at - t` bits up, or down by the
        // negative.
        let up = below(f, t, at);
        let up_by = sub(f, at, t);
        let down_by = sub(f, t, at);
        let sixty_four = c(f, 64);
        let thirty_two = c(f, 32);
        let in_up = below(f, up_by, sixty_four);
        let moved_up = f.binary(Binary::ShiftLeft, *word, up_by);
        let moved_up = f.choose(in_up, moved_up, zero);
        let in_down = below(f, down_by, thirty_two);
        let moved_down = f.binary(Binary::ShiftRightU, *word, down_by);
        let moved_down = f.choose(in_down, moved_down, zero);
        let piece = f.choose(up, moved_up, moved_down);
        out = or(f, out, piece);
    }
    out
}

/// Whether any bit of `a` below the place `t` is 1.
fn any_below(f: &mut Builder, a: &Wide, t: Value) -> Value {
    let mut any = f.int(Bits::B8, 0);
    let zero = c(f, 0);
    for (i, word) in a.iter().enumerate() {
        let at = c(f, 32 * i as u64);
        let end = c(f, 32 * (i as u64 + 1));
        // The bits of the word that are below `t`.
        let whole = {
            let past = below(f, end, t);
            let same = eq(f, end, t);
            or(f, past, same)
        };
        let some = below(f, at, t);
        let part = sub(f, t, at);
        let one = c(f, 1);
        let bits = f.binary(Binary::ShiftLeft, one, part);
        let bits = sub(f, bits, one);
        let all = c(f, MASK);
        let partial = f.choose(some, bits, zero);
        let low = f.choose(whole, all, partial);
        let kept = and(f, *word, low);
        let nonzero = {
            let is_zero = eq(f, kept, zero);
            not(f, is_zero)
        };
        any = f.binary(Binary::Or, any, nonzero);
    }
    any
}

/// The function `a * b / 2^240`, of eight words and eight words.
pub(super) fn multiply() -> Function {
    let mut f = Builder::new();
    let params = f.params(&[L; 2 * WORDS]);
    let (a, b) = (params[..WORDS].to_vec(), params[WORDS..].to_vec());
    let out = product(&mut f, &a, &b);
    f.ret(out);
    f.finish(&[L; WORDS])
}

/// The function that gives `exp(r)` for the `r` given as `r / 64`, in the
/// eight words of fixed point: the sum of the series, squared six times.
pub(super) fn series(multiply: usize) -> Function {
    let mut f = Builder::new();
    let r = f.params(&[L; WORDS]);
    let zero = c(&mut f, 0);
    let one_word = c(&mut f, 1 << (POINT - 7 * 32));
    // The first term is 1.
    let mut start = vec![c(&mut f, 1)];
    let mut one = [zero; WORDS];
    one[7] = one_word;
    start.extend(one.iter().copied());
    start.extend(one.iter().copied());
    let done = looping(
        &mut f,
        start,
        |f, v| {
            let last = c(f, TERMS + 1);
            below(f, v[0], last)
        },
        |f, v| {
            let n = v[0];
            let term: Vec<Value> = v[1..1 + WORDS].to_vec();
            let sum: Vec<Value> = v[1 + WORDS..].to_vec();
            let mut args = term;
            args.extend(r.iter().copied());
            let times = f.call(multiply, args, &[L; WORDS]);
            let term = div_small(f, &times, n);
            let (sum, _) = add_wide(f, &sum, &term);
            let one = c(f, 1);
            let next = f.binary(Binary::Add, n, one);
            let mut out = vec![next];
            out.extend(term);
            out.extend(sum);
            out
        },
    );
    let mut sum: Vec<Value> = done[1 + WORDS..].to_vec();
    for _ in 0..SQUARES {
        let mut args = sum.clone();
        args.extend(sum.iter().copied());
        sum = f.call(multiply, args, &[L; WORDS]);
    }
    f.ret(sum);
    f.finish(&[L; WORDS])
}

/// The function that gives `exp(x)` of a float of the size, rounded once.
pub(super) fn exp(double: bool, series: usize) -> Function {
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
    // A float that is not a number, an infinity, one that is too great or too
    // small to have an exponential that is not infinity or 0, and one so near 0
    // that the exponential is 1.
    let (special, _) = f.block(&[]);
    let (not_special, _) = f.block(&[]);
    let is_special = eq(&mut f, field, field_mask);
    f.switch(is_special, vec![(not_special, vec![]), (special, vec![])]);
    f.at(special);
    {
        // `nan` and `inf` are themselves, and `-inf` is 0.
        let nothing = eq(&mut f, mantissa, zero);
        let is_negative = eq(&mut f, negative, one);
        let to_zero = f.binary(Binary::And, nothing, is_negative);
        let zero_float = literal(&mut f, 0.0);
        let result = f.choose(to_zero, zero_float, x);
        f.ret(vec![result]);
    }
    f.at(not_special);
    let (huge_block, _) = f.block(&[]);
    let (not_huge, _) = f.block(&[]);
    let high = literal(&mut f, if double { 710.0 } else { 89.0 });
    let is_huge = f.float_compare(FloatCompare::Greater, x, high);
    f.switch(is_huge, vec![(not_huge, vec![]), (huge_block, vec![])]);
    f.at(huge_block);
    {
        let infinity = literal(&mut f, f64::INFINITY);
        f.ret(vec![infinity]);
    }
    f.at(not_huge);
    let (small_block, _) = f.block(&[]);
    let (not_small, _) = f.block(&[]);
    let low = literal(&mut f, if double { -746.0 } else { -104.0 });
    let is_small = f.float_compare(FloatCompare::Less, x, low);
    f.switch(is_small, vec![(not_small, vec![]), (small_block, vec![])]);
    f.at(small_block);
    {
        let nothing = literal(&mut f, 0.0);
        f.ret(vec![nothing]);
    }
    f.at(not_small);
    let (near_block, _) = f.block(&[]);
    let (general, _) = f.block(&[]);
    // Below 2^-54 of a float of 64 bits, 2^-25 of one of 32, the exponential
    // is rounded to 1.
    let least_field = c(&mut f, bias - if double { 54 } else { 25 });
    let near = below(&mut f, field, least_field);
    f.switch(near, vec![(general, vec![]), (near_block, vec![])]);
    f.at(near_block);
    {
        let one_float = literal(&mut f, 1.0);
        f.ret(vec![one_float]);
    }
    f.at(general);
    let (ln2, many) = constants();
    let ln2 = constant(&mut f, &ln2);
    let many = constant(&mut f, &many);
    // `x` as fixed point: the mantissa with its 1 moved by `240 + e` bits, where
    // `x = m * 2^e`.
    let hidden = c(&mut f, 1 << mantissa_bits);
    let m = or(&mut f, mantissa, hidden);
    let offset = c(&mut f, bias + mantissa_bits);
    let e = sub(&mut f, field, offset);
    let point = c(&mut f, POINT as u64);
    let shift = f.binary(Binary::Add, e, point);
    let mk = mask(&mut f);
    let sixty_four = c(&mut f, 64);
    let thirty_two = c(&mut f, 32);
    let mut fixed = Vec::new();
    for i in 0..WORDS {
        let at = c(&mut f, 32 * i as u64);
        // Where the word lies relative to the mantissa moved by `shift`.
        let below_shift = below(&mut f, at, shift);
        let up_by = sub(&mut f, shift, at);
        let down_by = sub(&mut f, at, shift);
        let up_ok = below(&mut f, up_by, thirty_two);
        let moved_up = f.binary(Binary::ShiftLeft, m, up_by);
        let moved_up = and(&mut f, moved_up, mk);
        let moved_up = f.choose(up_ok, moved_up, zero);
        let down_ok = below(&mut f, down_by, sixty_four);
        let moved_down = f.binary(Binary::ShiftRightU, m, down_by);
        let moved_down = and(&mut f, moved_down, mk);
        let moved_down = f.choose(down_ok, moved_down, zero);
        fixed.push(f.choose(below_shift, moved_up, moved_down));
    }
    // `a = x + SHIFT * ln 2`, which is above 0.
    let (sum, _) = add_wide(&mut f, &many, &fixed);
    let (difference, _) = sub_wide(&mut f, &many, &fixed);
    let is_negative = eq(&mut f, negative, one);
    let a = choose_wide(&mut f, is_negative, &difference, &sum);
    // The quotient of `a` by `ln 2`, near.
    let shifted = literal(&mut f, SHIFT as f64 * std::f64::consts::LN_2);
    let a_float = f.float_binary(super::FloatBinary::Add, x, shifted);
    let inverse = literal(&mut f, std::f64::consts::LOG2_E);
    let scaled = f.float_binary(super::FloatBinary::Mul, a_float, inverse);
    let q0 = f.int_from_float(false, scaled, Bits::B64);
    let times = mul_small(&mut f, &ln2, q0);
    let (rest, below_zero) = sub_wide(&mut f, &a, &times);
    let (back, _) = add_wide(&mut f, &rest, &ln2);
    let flag = f.narrow(below_zero, Bits::B8);
    let q_down = sub(&mut f, q0, one);
    let u = choose_wide(&mut f, flag, &back, &rest);
    let q1 = f.choose(flag, q_down, q0);
    let (after, short) = sub_wide(&mut f, &u, &ln2);
    let not_short = {
        let s = f.narrow(short, Bits::B8);
        not(&mut f, s)
    };
    let r = choose_wide(&mut f, not_short, &after, &u);
    let q_up = f.binary(Binary::Add, q1, one);
    let q = f.choose(not_short, q_up, q1);
    // `exp(r)`, from the series of `r / 64`.
    let small = shift_down(&mut f, &r, 6);
    let e = f.call(series, small, &[L; WORDS]);
    // The result is `e * 2^(q - SHIFT)`, and `e` is between 1 and 2.
    let shifts = c(&mut f, SHIFT);
    let exponent = sub(&mut f, q, shifts);
    let bias_c = c(&mut f, bias);
    let biased = f.binary(Binary::Add, exponent, bias_c);
    // Subnormal: the bits that are kept are fewer by `1 - biased`.
    let not_normal = below_s(&mut f, biased, one);
    let fewer = sub(&mut f, one, biased);
    let fewer = f.choose(not_normal, fewer, zero);
    let base = c(&mut f, POINT as u64 - mantissa_bits);
    let cut = f.binary(Binary::Add, base, fewer);
    let cut_less = sub(&mut f, cut, one);
    let with_half = low_after_shift(&mut f, &e, cut_less);
    let kept = f.binary(Binary::ShiftRightU, with_half, one);
    let half = and(&mut f, with_half, one);
    let sticky = any_below(&mut f, &e, cut_less);
    let odd = and(&mut f, kept, one);
    let odd = {
        let z = eq(&mut f, odd, zero);
        not(&mut f, z)
    };
    let half_flag = {
        let z = eq(&mut f, half, zero);
        not(&mut f, z)
    };
    let more = f.binary(Binary::Or, sticky, odd);
    let up = f.binary(Binary::And, half_flag, more);
    let up = number(&mut f, up);
    let kept = f.binary(Binary::Add, kept, up);
    let field_part = {
        let before = sub(&mut f, biased, one);
        let by = c(&mut f, mantissa_bits);
        let moved = f.binary(Binary::ShiftLeft, before, by);
        f.choose(not_normal, zero, moved)
    };
    let result = f.binary(Binary::Add, field_part, kept);
    let infinity = {
        let by = c(&mut f, mantissa_bits);
        f.binary(Binary::ShiftLeft, field_mask, by)
    };
    let over = {
        let ok = below(&mut f, result, infinity);
        not(&mut f, ok)
    };
    let result = f.choose(over, infinity, result);
    let out = if double {
        result
    } else {
        f.narrow(result, Bits::B32)
    };
    let out = f.float_from_bits(out);
    f.ret(vec![out]);
    f.finish(&[float])
}
