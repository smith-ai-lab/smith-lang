//! A reference for `exp`, `ln`, `sin`, and `cos` of floats that no host library
//! gives: each is worked out in numbers of 448 bits, which is 395 bits past the
//! 53 of an `f64`, and so far past the 2^-120 that the hardest numbers to round
//! need, and then rounded once, to the nearest, ties to even, to the size of
//! the float. It is slow and it is plain: series of whole numbers, with nothing
//! that the code under test shares.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The bits a number keeps.
const PREC: usize = 448;

/// A number: `mant * 2^exp`, with the sign, and a mantissa of 32-bit words that
/// has no zero word above its first. 0 has no word.
#[derive(Clone, Debug)]
pub struct Big {
    neg: bool,
    mant: Vec<u32>,
    exp: i64,
}

fn bit_length(m: &[u32]) -> usize {
    match m.last() {
        Some(top) => (m.len() - 1) * 32 + (32 - top.leading_zeros() as usize),
        None => 0,
    }
}

fn trim(m: &mut Vec<u32>) {
    while m.last() == Some(&0) {
        m.pop();
    }
}

fn shift_left(m: &[u32], by: usize) -> Vec<u32> {
    if m.is_empty() {
        return Vec::new();
    }
    let (words, bits) = (by / 32, by % 32);
    let mut out = vec![0u32; words];
    let mut carry = 0u32;
    for &w in m {
        out.push(if bits == 0 { w } else { (w << bits) | carry });
        carry = if bits == 0 { 0 } else { w >> (32 - bits) };
    }
    out.push(carry);
    trim(&mut out);
    out
}

fn shift_right(m: &[u32], by: usize) -> Vec<u32> {
    let (words, bits) = (by / 32, by % 32);
    if words >= m.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in words..m.len() {
        let low = m[i] >> bits;
        let high = if bits == 0 {
            0
        } else {
            m.get(i + 1).copied().unwrap_or(0) << (32 - bits)
        };
        out.push(low | high);
    }
    trim(&mut out);
    out
}

fn compare_mant(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| {
        for i in (0..a.len()).rev() {
            if a[i] != b[i] {
                return a[i].cmp(&b[i]);
            }
        }
        std::cmp::Ordering::Equal
    })
}

fn add_mant(a: &[u32], b: &[u32]) -> Vec<u32> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n + 1);
    let mut carry = 0u64;
    for i in 0..n {
        let x = u64::from(a.get(i).copied().unwrap_or(0))
            + u64::from(b.get(i).copied().unwrap_or(0))
            + carry;
        out.push(x as u32);
        carry = x >> 32;
    }
    out.push(carry as u32);
    trim(&mut out);
    out
}

/// `a - b`, where `a` is not below `b`.
fn sub_mant(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i64;
    for (i, &word) in a.iter().enumerate() {
        let mut x = i64::from(word) - i64::from(b.get(i).copied().unwrap_or(0)) - borrow;
        borrow = 0;
        if x < 0 {
            x += 1 << 32;
            borrow = 1;
        }
        out.push(x as u32);
    }
    trim(&mut out);
    out
}

fn mul_mant(a: &[u32], b: &[u32]) -> Vec<u32> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let t = u64::from(out[i + j]) + u64::from(x) * u64::from(y) + carry;
            out[i + j] = t as u32;
            carry = t >> 32;
        }
        let mut k = i + b.len();
        while carry != 0 {
            let t = u64::from(out[k]) + carry;
            out[k] = t as u32;
            carry = t >> 32;
            k += 1;
        }
    }
    trim(&mut out);
    out
}

/// The quotient of `a` by `b`, which is not 0, a bit at a time.
fn divide_mant(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut quotient = vec![0u32; a.len()];
    let mut rest: Vec<u32> = Vec::new();
    for i in (0..bit_length(a)).rev() {
        rest = shift_left(&rest, 1);
        if (a[i / 32] >> (i % 32)) & 1 == 1 {
            rest = add_mant(&rest, &[1]);
        }
        if compare_mant(&rest, b) != std::cmp::Ordering::Less {
            rest = sub_mant(&rest, b);
            quotient[i / 32] |= 1 << (i % 32);
        }
    }
    trim(&mut quotient);
    quotient
}

impl Big {
    pub fn zero() -> Big {
        Big {
            neg: false,
            mant: Vec::new(),
            exp: 0,
        }
    }

    pub fn from_u64(n: u64) -> Big {
        let mut mant = vec![n as u32, (n >> 32) as u32];
        trim(&mut mant);
        Big {
            neg: false,
            mant,
            exp: 0,
        }
    }

    pub fn from_i64(n: i64) -> Big {
        let mut b = Big::from_u64(n.unsigned_abs());
        b.neg = n < 0 && !b.mant.is_empty();
        b
    }

    /// The number a finite float of 64 bits is.
    pub fn from_f64(x: f64) -> Big {
        assert!(x.is_finite());
        let bits = x.to_bits();
        let fraction = bits & ((1 << 52) - 1);
        let field = (bits >> 52) & 0x7ff;
        let (m, e) = if field == 0 {
            (fraction, -1074)
        } else {
            (fraction | (1 << 52), field as i64 - 1075)
        };
        let mut b = Big::from_u64(m);
        b.exp = e;
        b.neg = (bits >> 63) == 1 && !b.mant.is_empty();
        b
    }

    pub fn is_zero(&self) -> bool {
        self.mant.is_empty()
    }

    /// The least `k` with `|self| < 2^k`, or 0 for 0.
    pub fn magnitude_bits(&self) -> i64 {
        if self.is_zero() {
            0
        } else {
            self.exp + bit_length(&self.mant) as i64
        }
    }

    fn kept(mut self, prec: usize) -> Big {
        trim(&mut self.mant);
        let n = bit_length(&self.mant);
        if n > prec {
            let cut = n - prec;
            self.mant = shift_right(&self.mant, cut);
            self.exp += cut as i64;
        }
        if self.mant.is_empty() {
            self.neg = false;
            self.exp = 0;
        }
        self
    }

    pub fn negated(mut self) -> Big {
        if !self.is_zero() {
            self.neg = !self.neg;
        }
        self
    }

    pub fn ldexp(mut self, k: i64) -> Big {
        if !self.is_zero() {
            self.exp += k;
        }
        self
    }

    pub fn add(&self, other: &Big) -> Big {
        self.add_at(other, PREC)
    }

    pub fn add_at(&self, other: &Big, prec: usize) -> Big {
        if self.is_zero() {
            return other.clone().kept(prec);
        }
        if other.is_zero() {
            return self.clone().kept(prec);
        }
        // A number far below the other is left out, as it cannot change what
        // is kept.
        let (hi, lo) = if self.magnitude_bits() >= other.magnitude_bits() {
            (self, other)
        } else {
            (other, self)
        };
        if hi.magnitude_bits() - lo.magnitude_bits() > prec as i64 + 64 {
            return hi.clone().kept(prec);
        }
        let e = self.exp.min(other.exp);
        let a = shift_left(&self.mant, (self.exp - e) as usize);
        let b = shift_left(&other.mant, (other.exp - e) as usize);
        let (neg, mant) = if self.neg == other.neg {
            (self.neg, add_mant(&a, &b))
        } else {
            match compare_mant(&a, &b) {
                std::cmp::Ordering::Less => (other.neg, sub_mant(&b, &a)),
                _ => (self.neg, sub_mant(&a, &b)),
            }
        };
        Big { neg, mant, exp: e }.kept(prec)
    }

    pub fn sub(&self, other: &Big) -> Big {
        self.add(&other.clone().negated())
    }

    pub fn sub_at(&self, other: &Big, prec: usize) -> Big {
        self.add_at(&other.clone().negated(), prec)
    }

    pub fn mul(&self, other: &Big) -> Big {
        self.mul_at(other, PREC)
    }

    pub fn mul_at(&self, other: &Big, prec: usize) -> Big {
        let mant = mul_mant(&self.mant, &other.mant);
        Big {
            neg: self.neg != other.neg,
            mant,
            exp: self.exp + other.exp,
        }
        .kept(prec)
    }

    pub fn div(&self, other: &Big) -> Big {
        self.div_at(other, PREC)
    }

    pub fn div_at(&self, other: &Big, prec: usize) -> Big {
        assert!(!other.is_zero());
        if self.is_zero() {
            return Big::zero();
        }
        let need = prec + 2 + bit_length(&other.mant);
        let shift = need.saturating_sub(bit_length(&self.mant));
        let numerator = shift_left(&self.mant, shift);
        Big {
            neg: self.neg != other.neg,
            mant: divide_mant(&numerator, &other.mant),
            exp: self.exp - shift as i64 - other.exp,
        }
        .kept(prec)
    }

    pub fn div_small(&self, n: u64) -> Big {
        self.div(&Big::from_u64(n))
    }

    /// Whether `self` is below `other`.
    pub fn below(&self, other: &Big) -> bool {
        self.sub(other).neg
    }

    /// The integer nearest, as a number that holds one.
    pub fn nearest_integer(&self) -> i64 {
        let half = Big::from_u64(1).ldexp(-1);
        let shifted = if self.neg {
            self.sub(&half)
        } else {
            self.add(&half)
        };
        // Truncated towards 0.
        if shifted.exp >= 0 {
            let m = shift_left(&shifted.mant, shifted.exp as usize);
            let n = m.first().copied().unwrap_or(0) as i64 | (m.get(1).copied().unwrap_or(0) as i64) << 32;
            return if shifted.neg { -n } else { n };
        }
        let m = shift_right(&shifted.mant, (-shifted.exp) as usize);
        let n = m.first().copied().unwrap_or(0) as i64 | (m.get(1).copied().unwrap_or(0) as i64) << 32;
        if shifted.neg { -n } else { n }
    }

    /// The bits of the float of 64 bits nearest this number, and the even one
    /// of two that are as near.
    pub fn to_f64_bits(&self) -> u64 {
        self.round_to(53, -1074, 1023)
    }

    /// The bits of the float of 32 bits nearest this number.
    pub fn to_f32_bits(&self) -> u32 {
        self.round_to(24, -149, 127) as u32
    }

    /// The bits of the float with `p` bits of mantissa, whose least number is
    /// `2^least` and whose greatest exponent is `top`.
    fn round_to(&self, p: usize, least: i64, top: i64) -> u64 {
        let sign = u64::from(self.neg);
        let field_bits = if p == 53 { 11 } else { 8 };
        let width_sign = sign << (p + field_bits - 1);
        if self.is_zero() {
            return width_sign;
        }
        let n = bit_length(&self.mant) as i64;
        let leading = self.exp + n - 1;
        let lsb = (leading - (p as i64 - 1)).max(least);
        let cut = lsb - self.exp;
        let (q, half, rest) = if cut > 0 {
            let cut = cut as usize;
            let q = shift_right(&self.mant, cut);
            let half = {
                let m = shift_right(&self.mant, cut - 1);
                m.first().copied().unwrap_or(0) & 1 == 1
            };
            let kept_low = shift_left(&shift_right(&self.mant, cut - 1), cut - 1);
            let rest = compare_mant(&kept_low, &self.mant) != std::cmp::Ordering::Equal;
            (q, half, rest)
        } else {
            (shift_left(&self.mant, (-cut) as usize), false, false)
        };
        let mut value = q.first().copied().unwrap_or(0) as u64 | (q.get(1).copied().unwrap_or(0) as u64) << 32;
        let odd = value & 1 == 1;
        // The mantissa that is half way, and rounded to the even one.
        let sticky = rest;
        if half && (sticky || odd) {
            value += 1;
        }
        let mut lsb = lsb;
        if value == 1 << p {
            value >>= 1;
            lsb += 1;
        }
        if value == 0 {
            return width_sign;
        }
        let top_exponent = lsb + (64 - value.leading_zeros() as i64) - 1;
        if top_exponent > top {
            let infinity = ((1u64 << field_bits) - 1) << (p - 1);
            return width_sign | infinity;
        }
        let bias = top;
        if value < 1 << (p - 1) {
            // Not normal: the mantissa is the number.
            return width_sign | value;
        }
        let field = (top_exponent + bias) as u64;
        width_sign | (field << (p - 1)) | (value & ((1 << (p - 1)) - 1))
    }
}

/// `n!`-free series helpers use these two numbers, cached to a precision that
/// holds what is asked for, one of a few, as making a number takes time that
/// grows faster than its size. What the functions of a float of small size ask
/// for is made to 768 bits, and what the functions of a float of any size ask
/// for to a precision that holds every size of float.
fn cached(name: &'static str, prec: usize, make: fn(usize) -> Big) -> Big {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, (usize, Big)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    // The lock is held while the number is made, so that tests that run at the
    // same time and ask for it wait for one of them to make it, and do not each
    // make it.
    let mut held_by = cache.lock().unwrap();
    if let Some((held, found)) = held_by.get(name)
        && *held >= prec
    {
        return found.clone().kept(prec);
    }
    // A few more bits are made than are held, for the error of the series.
    let (held, made) = match prec {
        0..=704 => (768, make(768 + 64)),
        705..=832 => (896, make(896 + 64)),
        833..=1024 => (1024, make(1024 + 64)),
        _ => (prec.max(1900), make(prec.max(1900))),
    };
    held_by.insert(name, (held, made.clone()));
    made.kept(prec)
}

/// `atan(1 / n)`, by its series.
fn arctangent_of_inverse(n: u64, prec: usize) -> Big {
    let inverse = Big::from_u64(1).div_at(&Big::from_u64(n), prec + 16);
    let square = Big::from_u64(n * n);
    let mut term = inverse.clone();
    let mut sum = inverse;
    let mut k = 1u64;
    loop {
        term = term.div_at(&square, prec + 16);
        k += 2;
        let part = term.div_at(&Big::from_u64(k), prec + 16);
        if part.is_zero() || part.magnitude_bits() < sum.magnitude_bits() - prec as i64 - 20 {
            break;
        }
        sum = if (k / 2) % 2 == 1 {
            sum.sub_at(&part, prec + 16)
        } else {
            sum.add_at(&part, prec + 16)
        };
    }
    sum
}

/// `pi`, to `prec` bits, by Machin's formula.
fn pi_to(prec: usize) -> Big {
    let a = arctangent_of_inverse(5, prec + 8);
    let b = arctangent_of_inverse(239, prec + 8);
    let four = Big::from_u64(16).mul_at(&a, prec + 8);
    let minus = Big::from_u64(4).mul_at(&b, prec + 8);
    four.sub_at(&minus, prec + 8)
}

/// `ln 2`, to `prec` bits: twice the inverse hyperbolic tangent of a third.
fn ln2_to(prec: usize) -> Big {
    atanh_series(&Big::from_u64(1).div_at(&Big::from_u64(3), prec + 16), prec + 16)
        .mul_at(&Big::from_u64(2), prec + 16)
}

/// `atanh(z)` for a `z` of small magnitude.
fn atanh_series(z: &Big, prec: usize) -> Big {
    let square = z.mul_at(z, prec);
    let mut power = z.clone();
    let mut sum = z.clone();
    let mut k = 1u64;
    loop {
        power = power.mul_at(&square, prec);
        k += 2;
        let term = power.div_at(&Big::from_u64(k), prec);
        if term.is_zero() || term.magnitude_bits() < sum.magnitude_bits() - prec as i64 - 8 {
            break;
        }
        sum = sum.add_at(&term, prec);
    }
    sum
}

fn pi(prec: usize) -> Big {
    cached("pi", prec, pi_to)
}

fn ln2(prec: usize) -> Big {
    cached("ln2", prec, ln2_to)
}

/// `e^x`.
pub fn exp(x: &Big) -> Big {
    let p = PREC + 32;
    if x.is_zero() {
        return Big::from_u64(1);
    }
    let l = ln2(800);
    let k = x.div_at(&l, 64).nearest_integer();
    let r = x.sub_at(&Big::from_i64(k).mul_at(&l, 800), p).ldexp(-8);
    let mut term = Big::from_u64(1);
    let mut sum = Big::from_u64(1);
    let mut n = 1u64;
    loop {
        term = term.mul_at(&r, p).div_at(&Big::from_u64(n), p);
        if term.is_zero() || term.magnitude_bits() < -(p as i64) - 16 {
            break;
        }
        sum = sum.add_at(&term, p);
        n += 1;
    }
    for _ in 0..8 {
        sum = sum.mul_at(&sum, p);
    }
    sum.ldexp(k)
}

/// The natural logarithm of a number above 0.
pub fn ln(x: &Big) -> Big {
    assert!(!x.is_zero() && !x.neg);
    let p = PREC + 32;
    // `x = m * 2^e` with `m` between 0.7 and 1.5.
    let mut e = x.magnitude_bits();
    let mut m = x.clone().ldexp(-e);
    if m.below(&Big::from_u64(7).div_small(10)) {
        m = m.ldexp(1);
        e -= 1;
    }
    let one = Big::from_u64(1);
    let z = m.sub_at(&one, p).div_at(&m.add_at(&one, p), p);
    let series = if z.is_zero() {
        Big::zero()
    } else {
        atanh_series(&z, p).ldexp(1)
    };
    Big::from_i64(e)
        .mul_at(&ln2(800), p)
        .add_at(&series, p)
}

/// `x = k * pi / 2 + r` for the whole `k` that makes `r` least, which is
/// between -pi/4 and pi/4: the remainder of `k` by 4, and `r`, with the
/// precision that the size of `x` needs.
pub fn reduce(x: &Big) -> (u32, Big) {
    let p = PREC + 32;
    let size = x.magnitude_bits().max(0) as usize;
    let wide = size + p + 64;
    let half_pi = pi(wide).ldexp(-1);
    let k = x.div_at(&half_pi, size + 80).nearest_integer_wide();
    let r = x.sub_at(&k.0.mul_at(&half_pi, wide), p);
    (k.1, r)
}

/// The sine and cosine of `x`, a finite number of any size.
pub fn sin_cos(x: &Big) -> (Big, Big) {
    let p = PREC + 32;
    if x.is_zero() {
        return (Big::zero(), Big::from_u64(1));
    }
    let (quadrant, r) = reduce(x);
    let r2 = r.mul_at(&r, p);
    let (mut s, mut c) = (r.clone(), Big::from_u64(1));
    let (mut ts, mut tc) = (r.clone(), Big::from_u64(1));
    let mut n = 1u64;
    loop {
        tc = tc.mul_at(&r2, p).div_at(&Big::from_u64((2 * n - 1) * (2 * n)), p).negated();
        ts = ts.mul_at(&r2, p).div_at(&Big::from_u64((2 * n) * (2 * n + 1)), p).negated();
        if tc.is_zero() || (tc.magnitude_bits() < -(p as i64) - 16 && ts.magnitude_bits() < -(p as i64) - 16) {
            break;
        }
        c = c.add_at(&tc, p);
        s = s.add_at(&ts, p);
        n += 1;
    }
    match quadrant {
        0 => (s, c),
        1 => (c, s.negated()),
        2 => (s.negated(), c.negated()),
        _ => (c.negated(), s),
    }
}

impl Big {
    /// The integer nearest, as a number, and its remainder by 4 as 0 to 3, for
    /// a number of any size.
    fn nearest_integer_wide(&self) -> (Big, u32) {
        let half = Big::from_u64(1).ldexp(-1);
        let shifted = if self.neg {
            self.sub_at(&half, self.mant.len() * 32 + 64)
        } else {
            self.add_at(&half, self.mant.len() * 32 + 64)
        };
        // Truncated towards 0.
        let mant = if shifted.exp >= 0 {
            shift_left(&shifted.mant, shifted.exp as usize)
        } else {
            shift_right(&shifted.mant, (-shifted.exp) as usize)
        };
        let low = mant.first().copied().unwrap_or(0) & 3;
        let remainder = if shifted.neg { (4 - low) & 3 } else { low };
        let whole = Big {
            neg: shifted.neg && !mant.is_empty(),
            mant,
            exp: 0,
        };
        (whole, remainder)
    }
}

/// The correctly rounded `exp` of a finite float of 64 bits, as the bits of a
/// float: `inf` past the greatest, and 0 or a subnormal below.
pub fn exp_f64(x: f64) -> u64 {
    exp(&Big::from_f64(x)).to_f64_bits()
}

pub fn ln_f64(x: f64) -> u64 {
    ln(&Big::from_f64(x)).to_f64_bits()
}

pub fn sin_f64(x: f64) -> u64 {
    sin_cos(&Big::from_f64(x)).0.to_f64_bits()
}

pub fn cos_f64(x: f64) -> u64 {
    sin_cos(&Big::from_f64(x)).1.to_f64_bits()
}

pub fn exp_f32(x: f32) -> u32 {
    exp(&Big::from_f64(f64::from(x))).to_f32_bits()
}

pub fn ln_f32(x: f32) -> u32 {
    ln(&Big::from_f64(f64::from(x))).to_f32_bits()
}

pub fn sin_f32(x: f32) -> u32 {
    sin_cos(&Big::from_f64(f64::from(x))).0.to_f32_bits()
}

pub fn cos_f32(x: f32) -> u32 {
    sin_cos(&Big::from_f64(f64::from(x))).1.to_f32_bits()
}

/// The number of words of 32 bits, the least first, each in a number of 64,
/// that has `point` bits after the point.
pub fn from_words(words: &[u64], point: i64) -> Big {
    let mut sum = Big::zero();
    for (i, word) in words.iter().enumerate() {
        let part = Big::from_u64(*word).ldexp(32 * i as i64 - point);
        sum = sum.add_at(&part, 600);
    }
    sum
}

/// The least `k` with `|a - b| < 2^k`.
pub fn distance_bits(a: &Big, b: &Big) -> i64 {
    let difference = a.sub_at(b, 600);
    if difference.is_zero() {
        i64::MIN
    } else {
        difference.magnitude_bits()
    }
}
