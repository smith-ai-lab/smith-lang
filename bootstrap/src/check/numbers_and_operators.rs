//! Checks numbers and operators: the whole number types, a number written out,
//! `+`, `-`, and `*`, the `wrapping_`, `saturating_`, `checked_`, and `narrow`
//! functions, the functions of `Bits`, and `Convert` between numbers.

use super::*;

/// Whether a range of more than 64 bits is a type, which it is when the lowering
/// keeps it in two words.
const WIDE_BUILT: bool = true;

/// A whole number type: the range from `min` to `max`, both held. A range
/// of more than 127 bits has a greatest number above what an `i128` holds, and
/// then `max` is `i128::MAX` and `above` is how much greater the number is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Int {
    min: i128,
    max: i128,
    above: u128,
}

/// A bound of a range, which is a sign and a size: the least of a range is not
/// below -2 to the 127, and the greatest is below 2 to the 128.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Edge {
    negative: bool,
    size: u128,
}

impl Edge {
    fn of(n: i128) -> Edge {
        Edge {
            negative: n < 0,
            size: n.unsigned_abs(),
        }
    }

    fn add(self, other: Edge) -> Option<Edge> {
        if self.negative == other.negative {
            return Some(Edge {
                negative: self.negative,
                size: self.size.checked_add(other.size)?,
            });
        }
        let (more, less) = if self.size >= other.size {
            (self, other)
        } else {
            (other, self)
        };
        let size = more.size - less.size;
        Some(Edge {
            negative: more.negative && size != 0,
            size,
        })
    }

    fn negated(self) -> Edge {
        Edge {
            negative: !self.negative && self.size != 0,
            size: self.size,
        }
    }

    fn sub(self, other: Edge) -> Option<Edge> {
        self.add(other.negated())
    }

    fn mul(self, other: Edge) -> Option<Edge> {
        let size = self.size.checked_mul(other.size)?;
        Some(Edge {
            negative: size != 0 && self.negative != other.negative,
            size,
        })
    }

    fn less(self, other: Edge) -> bool {
        match (self.negative, other.negative) {
            (true, false) => true,
            (false, true) => false,
            (false, false) => self.size < other.size,
            (true, true) => self.size > other.size,
        }
    }
}

impl Int {
    pub(super) const I8: Int = Int::sized(true, 8);
    pub(super) const I16: Int = Int::sized(true, 16);
    pub(super) const I32: Int = Int::sized(true, 32);
    pub(super) const I64: Int = Int::sized(true, 64);
    pub(super) const U8: Int = Int::sized(false, 8);
    pub(super) const U16: Int = Int::sized(false, 16);
    pub(super) const U32: Int = Int::sized(false, 32);
    pub(super) const U64: Int = Int::sized(false, 64);

    /// The sizes the compiler keeps a range at, which each has a name.
    pub(super) const ALL: [Int; 8] = [
        Int::I8,
        Int::I16,
        Int::I32,
        Int::I64,
        Int::U8,
        Int::U16,
        Int::U32,
        Int::U64,
    ];

    /// The range that fills `bits` bits, signed or not.
    pub(super) const fn sized(signed: bool, bits: u32) -> Int {
        if signed && bits == 128 {
            Int {
                min: i128::MIN,
                max: i128::MAX,
                above: 0,
            }
        } else if signed {
            Int {
                min: -(1i128 << (bits - 1)),
                max: (1i128 << (bits - 1)) - 1,
                above: 0,
            }
        } else if bits == 128 {
            Int {
                min: 0,
                max: i128::MAX,
                above: 1 << 127,
            }
        } else {
            Int {
                min: 0,
                max: (1i128 << bits) - 1,
                above: 0,
            }
        }
    }

    /// The range from `min` up to and including `max`, or none if a
    /// number of it needs more than 128 bits.
    pub(super) fn between(min: i128, max: i128) -> Option<Int> {
        let int = Int { min, max, above: 0 };
        let holder = int.holder();
        (WIDE_BUILT || !int.wide())
            .then_some(())
            .filter(|()| int.min >= holder.min && max <= holder.max && max >= min - 1)
            .map(|()| int)
    }

    /// The range from the bound `low` up to and including the bound `high`,
    /// or none if a number of it needs more than 128 bits.
    fn between_bounds(low: Edge, high: Edge) -> Option<Int> {
        let min = if low.negative {
            -i128::try_from(low.size.checked_sub(1)?).ok()? - 1
        } else {
            i128::try_from(low.size).ok()?
        };
        let (max, above) = if high.negative {
            (-i128::try_from(high.size.checked_sub(1)?).ok()? - 1, 0)
        } else if let Ok(max) = i128::try_from(high.size) {
            (max, 0)
        } else {
            (i128::MAX, high.size - i128::MAX as u128)
        };
        let int = Int { min, max, above };
        // A range may be empty, which has its greatest number just below its
        // least.
        let ordered = !int.top().less(int.bottom().sub(Edge::of(1))?);
        (ordered && (WIDE_BUILT || !int.wide()) && int.holds_in(int.holder())).then_some(int)
    }

    /// The greatest number of the range as a bound.
    fn top(self) -> Edge {
        if self.above == 0 {
            Edge::of(self.max)
        } else {
            Edge {
                negative: false,
                size: self.max as u128 + self.above,
            }
        }
    }

    /// The least number of the range as a bound.
    fn bottom(self) -> Edge {
        Edge::of(self.min)
    }

    /// Whether every number of the range is a number of `holder`.
    fn holds_in(self, holder: Int) -> bool {
        holder.min <= self.min && !holder.top().less(self.top())
    }

    /// Whether a number of the range needs more than 64 bits.
    pub(super) fn wide(self) -> bool {
        self.bits() > 64
    }

    /// The name a size is written with, which is `u8` for `0..256`.
    fn size_name(self) -> Option<&'static str> {
        const NAMES: [&str; 8] = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"];
        Int::ALL.iter().position(|t| *t == self).map(|i| NAMES[i])
    }

    /// How the type is written when no name of it is: `a..b`.
    pub(super) fn name(self) -> String {
        match self.size_name() {
            Some(name) => name.to_string(),
            None if self.above == 0 => format!("{}..{}", self.min, self.max + 1),
            None => match self.top().size.checked_add(1) {
                Some(end) => format!("{}..{end}", self.min),
                None => format!("{}..340282366920938463463374607431768211456", self.min),
            },
        }
    }

    /// The type a name written in a file stands for: `Int` is `i64`, `Nat`
    /// is `u64`, and `Byte` is `u8`.
    pub(super) fn named(name: &str) -> Option<Int> {
        let name = match name {
            "Int" => "i64",
            "Byte" => "u8",
            "Nat" => "u64",
            _ => name,
        };
        Int::ALL.into_iter().find(|t| t.size_name() == Some(name))
    }

    /// Whether the type has a negative value, so that it is kept signed.
    pub(super) fn signed(self) -> bool {
        self.min < 0
    }

    /// How many bits the smallest size that holds the type as itself has.
    pub(super) fn bits(self) -> u32 {
        [8, 16, 32, 64, 128]
            .into_iter()
            .find(|&bits| self.holds_in(Int::sized(self.signed(), bits)))
            .unwrap_or(128)
    }

    /// Whether every value of `other` is a value of the type.
    pub(super) fn holds(self, other: Int) -> bool {
        other.holds_in(self)
    }

    /// The smallest range that holds every number of both, if a type holds it.
    pub(super) fn hull(self, other: Int) -> Option<Int> {
        let (low, high) = (
            if self.bottom().less(other.bottom()) {
                self.bottom()
            } else {
                other.bottom()
            },
            if self.top().less(other.top()) {
                other.top()
            } else {
                self.top()
            },
        );
        Int::between_bounds(low, high)
    }

    /// The size the type is kept at: the smallest one that holds it as itself.
    pub(super) fn holder(self) -> Int {
        Int::sized(self.signed(), self.bits())
    }

    /// The least value of the type.
    pub(super) fn min(self) -> i128 {
        self.min
    }

    /// The least and the greatest value, when a number of each is an `i128`: a
    /// range whose greatest is above `i128::MAX` has no such pair, and `arith`
    /// gives its values no fact.
    pub(super) fn ends(self) -> Option<(i128, i128)> {
        (self.above == 0).then_some((self.min, self.max))
    }

    /// The greatest value of the type, which a range of more than 127 bits has
    /// no `i128` for: where `wide` says it is not held, this is `i128::MAX`.
    pub(super) fn max(self) -> i128 {
        self.max
    }
}

/// Whether `expr`, a number with `magnitude`, is a character outside ASCII.
pub(super) fn expr_is_wide_character(expr: &Expr, magnitude: u128) -> bool {
    matches!(expr, Expr::Number { text, .. } if text.starts_with('\'')) && magnitude > 127
}

/// The exact range of `left op right`, when a type holds it: from the least
/// to the greatest value the operation gives over the numbers of both.
fn exact(op: ArithOp, left: Int, right: Int) -> Option<Int> {
    // A range of more than 127 bits has no `i128` for its greatest number, and
    // a quotient of one is worked out where it is built.
    let (low, high) = match op {
        ArithOp::Add => (
            left.bottom().add(right.bottom())?,
            left.top().add(right.top())?,
        ),
        ArithOp::Sub => (
            left.bottom().sub(right.top())?,
            left.top().sub(right.bottom())?,
        ),
        ArithOp::Mul => {
            let products = [
                left.bottom().mul(right.bottom())?,
                left.bottom().mul(right.top())?,
                left.top().mul(right.bottom())?,
                left.top().mul(right.top())?,
            ];
            let mut low = products[0];
            let mut high = products[0];
            for p in products {
                if p.less(low) {
                    low = p;
                }
                if high.less(p) {
                    high = p;
                }
            }
            (low, high)
        }
        ArithOp::Div => quotients(left, right)?,
        ArithOp::Rem => {
            // What is left of the least number of the largest size, taken by
            // `-1`, is asked of a processor that cannot give it: a smaller
            // size is worked at a larger one.
            let holder = left.hull(right)?.holder();
            if left.min() < 0
                && right.min() < 0
                && left.min() == holder.min()
                && holder.bits() == 64
            {
                return None;
            }
            remainders(left, right)
        }
    };
    let result = Int::between_bounds(low, high)?;
    // A quotient and a remainder of numbers of 64 bits or fewer are of the type
    // of the numbers, where `arith` shows the least number is not divided by
    // `-1`, as they always were.
    if matches!(op, ArithOp::Div | ArithOp::Rem) && result.wide() && !left.wide() && !right.wide() {
        return None;
    }
    Some(result)
}

/// The numbers a divisor of the range `of` can be, which is its negative
/// part and its positive part: a divisor is never zero.
fn divisors(of: Int) -> impl Iterator<Item = (Edge, Edge)> {
    let negative = (of.min() < 0).then(|| (of.bottom(), Edge::of(of.max().min(-1))));
    let positive =
        (of.top().size > 0 && !of.top().negative).then(|| (Edge::of(of.min().max(1)), of.top()));
    negative.into_iter().chain(positive)
}

/// `a / b` rounded toward zero, of bounds, where `b` is not zero.
fn quotient(a: Edge, b: Edge) -> Edge {
    let size = a.size / b.size;
    Edge {
        negative: size != 0 && a.negative != b.negative,
        size,
    }
}

/// The least and the greatest quotient, rounded toward zero, of a number of
/// the range `left` by a number of the range `right` that is not zero. A
/// quotient is monotone in each side where the divisor keeps its sign, so
/// they are at the ends of the ranges.
fn quotients(left: Int, right: Int) -> Option<(Edge, Edge)> {
    let mut found: Option<(Edge, Edge)> = None;
    for (low, high) in divisors(right) {
        for a in [left.bottom(), left.top()] {
            for b in [low, high] {
                let q = quotient(a, b);
                found = Some(found.map_or((q, q), |(l, h)| {
                    (if q.less(l) { q } else { l }, if h.less(q) { q } else { h })
                }));
            }
        }
    }
    // A divisor that can only be zero is refused where it is checked.
    Some(found.unwrap_or((Edge::of(0), Edge::of(0))))
}

/// The range that holds what is left over, which has the sign of the number
/// divided and is below the divisor and below that number, where the divisor
/// is not zero.
fn remainders(left: Int, right: Int) -> (Edge, Edge) {
    let sizes = |(low, high): (Edge, Edge)| [low.size, high.size];
    let most = divisors(right).flat_map(sizes).max().unwrap_or(1);
    let least = divisors(right)
        .map(|d| sizes(d).into_iter().min().unwrap_or(1))
        .min()
        .unwrap_or(1);
    let (bottom, top) = (left.bottom(), left.top());
    if !bottom.negative && least > top.size {
        // Every divisor is above every number divided, which is left whole.
        return (bottom, top);
    }
    if (top.size == 0 || top.negative) && bottom.size < least {
        return (bottom, top);
    }
    let below = most.saturating_sub(1);
    let up = if !top.negative && top.size > 0 {
        Edge {
            negative: false,
            size: top.size.min(below),
        }
    } else {
        Edge::of(0)
    };
    let down = if bottom.negative {
        Edge {
            negative: bottom.size.min(below) > 0,
            size: bottom.size.min(below),
        }
    } else {
        Edge::of(0)
    };
    (down, up)
}

/// A function of `Bits` whose result has the exact range of what it makes
/// of numbers that are not negative.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BitFn {
    And,
    Or,
    Xor,
    Left,
    Right,
}

impl BitFn {
    pub(super) fn named(name: &str) -> Option<BitFn> {
        Some(match name {
            "bit_and" => BitFn::And,
            "bit_or" => BitFn::Or,
            "bit_xor" => BitFn::Xor,
            "shift_left" => BitFn::Left,
            "shift_right" => BitFn::Right,
            _ => return None,
        })
    }
}

/// Which function of `Bits` a call of two numbers is, where it is written by
/// the name the trait gives it and no function of the program takes that name.
pub(super) fn bits_function(defs: &Defs, call: &Call) -> Option<BitFn> {
    let op = BitFn::named(&call.callee)?;
    let plain = call.args.len() == 2 && call.args.iter().all(|a| a.name.is_none() && !a.after);
    let of_bits = match &call.qualifier {
        None => {
            defs.function(call.name_pos, &call.callee).is_none()
                && defs
                    .trait_fns_at(call.name_pos, &call.callee)
                    .is_some_and(|of| matches!(of[..], [(k, _)] if k == defs.bits))
        }
        Some((q, at)) => defs.trait_at(*at, q) == Some(defs.bits),
    };
    (plain && of_bits).then_some(op)
}

/// The least and the greatest of `x op y` over every `x` from `a.0` to `a.1`
/// and every `y` from `b.0` to `b.1`, for numbers that are not negative: found
/// bit by bit from the top, as Warren's "Propagating bounds through logical
/// operations" does.
fn bit_bounds(op: BitFn, (a, b): ((u64, u64), (u64, u64))) -> (u64, u64) {
    let ((a0, a1), (c0, c1)) = (a, b);
    match op {
        BitFn::And => (min_and(a0, a1, c0, c1), max_and(a0, a1, c0, c1)),
        BitFn::Or => (min_or(a0, a1, c0, c1), max_or(a0, a1, c0, c1)),
        BitFn::Xor => (min_xor(a0, a1, c0, c1), max_xor(a0, a1, c0, c1)),
        BitFn::Left | BitFn::Right => unreachable!("a shift is not a bit by bit function"),
    }
}

fn min_or(mut a: u64, b: u64, mut c: u64, d: u64) -> u64 {
    let mut m = 1u64 << 63;
    while m != 0 {
        if !a & c & m != 0 {
            let t = (a | m) & m.wrapping_neg();
            if t <= b {
                a = t;
                break;
            }
        } else if a & !c & m != 0 {
            let t = (c | m) & m.wrapping_neg();
            if t <= d {
                c = t;
                break;
            }
        }
        m >>= 1;
    }
    a | c
}

fn max_or(a: u64, mut b: u64, c: u64, mut d: u64) -> u64 {
    let mut m = 1u64 << 63;
    while m != 0 {
        if b & d & m != 0 {
            let t = (b - m) | (m - 1);
            if t >= a {
                b = t;
                break;
            }
            let t = (d - m) | (m - 1);
            if t >= c {
                d = t;
                break;
            }
        }
        m >>= 1;
    }
    b | d
}

fn min_and(mut a: u64, b: u64, mut c: u64, d: u64) -> u64 {
    let mut m = 1u64 << 63;
    while m != 0 {
        if !a & !c & m != 0 {
            let t = (a | m) & m.wrapping_neg();
            if t <= b {
                a = t;
                break;
            }
            let t = (c | m) & m.wrapping_neg();
            if t <= d {
                c = t;
                break;
            }
        }
        m >>= 1;
    }
    a & c
}

fn max_and(a: u64, mut b: u64, c: u64, mut d: u64) -> u64 {
    let mut m = 1u64 << 63;
    while m != 0 {
        if b & !d & m != 0 {
            let t = (b & !m) | (m - 1);
            if t >= a {
                b = t;
                break;
            }
        } else if !b & d & m != 0 {
            let t = (d & !m) | (m - 1);
            if t >= c {
                d = t;
                break;
            }
        }
        m >>= 1;
    }
    b & d
}

fn min_xor(mut a: u64, b: u64, mut c: u64, d: u64) -> u64 {
    let mut m = 1u64 << 63;
    while m != 0 {
        if !a & c & m != 0 {
            let t = (a | m) & m.wrapping_neg();
            if t <= b {
                a = t;
            }
        } else if a & !c & m != 0 {
            let t = (c | m) & m.wrapping_neg();
            if t <= d {
                c = t;
            }
        }
        m >>= 1;
    }
    a ^ c
}

fn max_xor(a: u64, mut b: u64, c: u64, mut d: u64) -> u64 {
    let mut m = 1u64 << 63;
    while m != 0 {
        if b & d & m != 0 {
            let t = (b - m) | (m - 1);
            if t >= a {
                b = t;
            } else {
                let t = (d - m) | (m - 1);
                if t >= c {
                    d = t;
                }
            }
        }
        m >>= 1;
    }
    b ^ d
}

/// Whether an expression is a literal, or an operation on literals only,
/// which takes the type of what it is used with.
/// Whether `-x` is what the operation is: the parser writes it as `0 - x`
/// with the `0` at the place of the `-`.
pub(crate) fn is_negation(op: ArithOp, left: &Expr, op_pos: Pos) -> bool {
    matches!(
        (op, left),
        (ArithOp::Sub, Expr::Number { magnitude: 0, pos, .. }) if *pos == op_pos
    )
}

/// Whether the operand is a float written out or a call of `float`, which take
/// the float type their use decides.
pub(super) fn float_deferred(defs: &Defs, e: &Expr) -> bool {
    match e {
        Expr::Float { .. } => true,
        Expr::Call(call) => is_float_call(defs, call),
        _ => false,
    }
}

/// Whether the call is of `float`, which makes a float of a whole number and
/// which no function of the program takes the name of.
pub(crate) fn is_float_call(defs: &Defs, call: &Call) -> bool {
    let (at, name) = (call.name_pos, call.callee.as_str());
    name == "float"
        && call.qualifier.is_none()
        && call.args.len() == 1
        && defs.function(at, name).is_none()
        && defs.struct_at(at, name).is_none()
        && defs.trait_fns_at(at, name).is_none()
}

pub(super) fn flexible(e: &Expr) -> bool {
    match e {
        Expr::Number { .. } => true,
        Expr::Arith { left, right, .. } => flexible(left) && flexible(right),
        Expr::Match(m) => literal_blocks(m),
        _ => false,
    }
}

/// Whether an `if` with `else`, or a `match`, gives a value that every block
/// that does not leave ends in a literal, and at least one does not leave.
/// Its type is that of what it is used with, as a literal's is.
pub(super) fn literal_blocks(m: &Match) -> bool {
    if !matches!(m.form, Form::Match | Form::If { otherwise: true }) {
        return false;
    }
    let mut blocks = m
        .arms
        .iter()
        .filter(|a| !proof::ends_in_exit(&a.body))
        .peekable();
    blocks.peek().is_some()
        && blocks.all(|a| matches!(a.body.last(), Some(Stmt::Expr(e)) if flexible(e)))
}

/// The whole numbers a body binds to names with no type written.
pub(super) enum Numbers {
    /// None are looked for, as in a default.
    Off,
    /// The first look, at how each is used, which gives each an undecided
    /// type. The places of the literals, in order, are noted.
    Finding(Vec<Pos>),
    /// The second look, with each type decided, by the place of its
    /// literal, with the place of the use that decided it, if one did.
    Decided(HashMap<Pos, (Int, Option<Pos>)>),
}

/// What a narrowing does with a value that does not fit.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Narrowing {
    /// `wrapping_narrow`, which keeps the low bits.
    Wrapping,
    /// `clamp`, which gives the nearest value the type holds.
    Saturating,
    /// `narrow`, which gives `.none` where the value does not fit.
    Checked,
}

impl Narrowing {
    pub(super) fn name(self) -> &'static str {
        match self {
            Narrowing::Wrapping => "wrapping_narrow",
            Narrowing::Saturating => "clamp",
            Narrowing::Checked => "narrow",
        }
    }
}

/// Which narrowing a call is of, if it is of one the compiler builds, which
/// no function of the program takes the name of.
pub(super) fn narrowing(defs: &Defs, call: &Call) -> Option<Narrowing> {
    let named = [
        Narrowing::Wrapping,
        Narrowing::Saturating,
        Narrowing::Checked,
    ]
    .into_iter()
    .find(|n| n.name() == call.callee)?;
    let (at, name) = (call.name_pos, call.callee.as_str());
    let taken = defs.function(at, name).is_some()
        || defs.struct_at(at, name).is_some()
        || defs.trait_fns_at(at, name).is_some();
    (call.qualifier.is_none() && !taken).then_some(named)
}

impl<'a> Checker<'a> {
    /// Checks a whole number literal bound to `name` with no type written.
    /// While the uses are looked at, it is an undecided number. Once they
    /// have decided its type, it is that type, and the literal must fit.
    pub(super) fn literal_binding(&mut self, body: &mut Body<'a>, name: &str, value: &Expr) -> Ty {
        let pos = value.pos();
        match &mut body.numbers {
            Numbers::Off => unreachable!("numbers are looked for"),
            Numbers::Finding(places) => {
                places.push(pos);
                self.expr(body, value, &Want::Ty(Ty::Unknown));
                let Numbers::Finding(places) = &body.numbers else {
                    unreachable!()
                };
                Ty::Flex(places.len() - 1)
            }
            Numbers::Decided(decided) => {
                let (int, by) = decided[&pos];
                self.deciding = by;
                let ty = self.expr(body, value, &Want::Ty(Ty::Int(int)));
                self.deciding = None;
                body.decided.insert(name.to_string(), (int, by));
                ty
            }
        }
    }

    /// Checks `left + right`, `left - right`, `left * right`, `left / right`,
    /// or `left % right`, whose sides
    /// are whole numbers of one type, which is the type it gives. It
    /// compiles only when `arith` proves that the result fits that type.
    pub(super) fn arith(
        &mut self,
        body: &mut Body<'a>,
        expr: &Expr,
        (op, left, right, op_pos): (ArithOp, &Expr, &Expr, Pos),
        want: &Want,
    ) -> Ty {
        let wanted = match want {
            Want::Ty(t @ (Ty::Int(_) | Ty::Flex(_) | Ty::Float(_))) => Want::Ty(t.clone()),
            _ => Want::Any,
        };
        let defs = self.defs;
        let unary = is_negation(op, left, op_pos);
        // A literal beside a number of a range is the range that holds it
        // alone, where the exact range of the result fits a type, and takes
        // the type of the number where it does not.
        let (l, r) = match (flexible(left), flexible(right)) {
            // `-x`, which is a float with its sign flipped where `x` is one.
            (true, false) if unary => {
                let by = if matches!(want, Want::Ty(Ty::Float(_))) {
                    want
                } else {
                    &Want::Any
                };
                let r = self.expr(body, right, by);
                if matches!(r, Ty::Float(_)) {
                    return r;
                }
                let side = self.literal_want(op, (left, &r), false);
                (self.expr(body, left, &side), r)
            }
            // A float written out, and `float(n)`, take the type of the other
            // side, or of the place, and are an `f64` when nothing decides.
            (false, false) if float_deferred(defs, left) && float_deferred(defs, right) => (
                self.expr(body, left, &wanted),
                self.expr(body, right, &wanted),
            ),
            (false, false) if float_deferred(defs, left) => {
                let r = self.expr(body, right, &Want::Any);
                let side = Self::side_want(&r);
                (self.expr(body, left, &side), r)
            }
            (false, false) if float_deferred(defs, right) => {
                let l = self.expr(body, left, &Want::Any);
                let side = Self::side_want(&l);
                let r = self.expr(body, right, &side);
                (l, r)
            }
            (true, false) => {
                let r = self.expr(body, right, &Want::Any);
                let side = self.literal_want(op, (left, &r), false);
                (self.expr(body, left, &side), r)
            }
            (false, true) => {
                let l = self.expr(body, left, &Want::Any);
                let side = self.literal_want(op, (right, &l), true);
                let r = self.expr(body, right, &side);
                (l, r)
            }
            (true, true) => (
                self.expr(body, left, &wanted),
                self.expr(body, right, &wanted),
            ),
            // A call whose result a literal may decide takes the type of the
            // other side, as a literal does.
            (false, false) if matches!(right, Expr::Call(_)) && !matches!(left, Expr::Call(_)) => {
                let l = self.expr(body, left, &Want::Any);
                let side = Self::side_want(&l);
                (l, self.expr(body, right, &side))
            }
            (false, false) if matches!(left, Expr::Call(_)) && !matches!(right, Expr::Call(_)) => {
                let r = self.expr(body, right, &Want::Any);
                let side = Self::side_want(&r);
                (self.expr(body, left, &side), r)
            }
            (false, false) => (
                self.expr(body, left, &Want::Any),
                self.expr(body, right, &Want::Any),
            ),
        };
        let symbol = op.symbol();
        if matches!(op, ArithOp::Div | ArithOp::Rem)
            && let Ty::Int(b) = &r
            && b.min() <= 0
            && 0 <= b.max()
        {
            self.divisor(body, expr, op_pos);
        }
        let gives = match (&l, &r) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => return Ty::Unknown,
            // An operation of one type whose exact range is more than 64 bits is
            // of that type where `arith` shows it fits, as it always was, and
            // is the exact range, in two words, where it does not.
            (Ty::Int(a), Ty::Int(b))
                if a == b
                    && exact(op, *a, *b).is_some_and(|e| e.wide() && !a.wide())
                    && self.would_fit(body, expr, *a) =>
            {
                return l.clone();
            }
            (Ty::Int(a), Ty::Int(b)) if let Some(exact) = exact(op, *a, *b) => {
                // The result is the exact range, which no proof is asked of,
                // and each side is the number at the size that range is kept
                // at. A quotient is worked out at the size that holds both
                // sides and the result, since it drops no bit of either.
                let at = match op {
                    ArithOp::Div | ArithOp::Rem => a.hull(*b).and_then(|h| h.hull(exact)),
                    _ => Some(exact),
                }
                .unwrap_or(exact);
                // The least number of a size taken by `-1` is what a processor
                // cannot do, so a remainder of one is worked at a larger size.
                let at = match op {
                    ArithOp::Rem
                        if at.signed()
                            && a.min() == at.holder().min()
                            && b.min() < 0
                            && at.holder().bits() < 64 =>
                    {
                        Int::sized(true, 2 * at.holder().bits())
                    }
                    _ => at,
                };
                for (side, own) in [(left, *a), (right, *b)] {
                    if own != at {
                        self.widened.insert(addr(side), at);
                    }
                }
                return Ty::Int(exact);
            }
            (Ty::Float(a), Ty::Float(b)) if a == b && op != ArithOp::Rem => return l.clone(),
            (Ty::Float(_), Ty::Float(_)) if op == ArithOp::Rem => {
                self.errors
                    .push(Error::unsupported(op_pos, "`%` of floats"));
                return Ty::Unknown;
            }
            (Ty::Float(_), Ty::Float(_)) => {
                self.errors.push(Error::unsupported(
                    op_pos,
                    "an `f32` and an `f64` in one operator",
                ));
                return Ty::Unknown;
            }
            (Ty::Float(_), Ty::Int(_) | Ty::Flex(_)) | (Ty::Int(_) | Ty::Flex(_), Ty::Float(_)) => {
                let (a, b) = (defs.describe(&l, body.scope), defs.describe(&r, body.scope));
                self.errors.push(
                    Error::new(
                        op_pos,
                        format!(
                            "`{symbol}` takes two whole numbers or two floats, and the sides \
                             are {a} and {b}"
                        ),
                    )
                    .with_fix("`float(n)` makes a float of a whole number"),
                );
                return Ty::Unknown;
            }
            (Ty::Int(a), Ty::Int(b)) if a == b => l.clone(),
            (Ty::Int(a), Ty::Int(b)) => {
                self.errors.push(Error::new(
                    op_pos,
                    format!(
                        "`{symbol}` needs both sides to be of one type, and the left is `{}` \
                         where the right is `{}`",
                        self.defs.spelling(*a),
                        self.defs.spelling(*b)
                    ),
                ));
                return Ty::Unknown;
            }
            (Ty::Flex(_) | Ty::Int(_), Ty::Flex(_) | Ty::Int(_)) => {
                use_at(op_pos);
                if !flex_pair(&l, &r) {
                    unreachable!("a number and a number")
                }
                if matches!(l, Ty::Flex(_)) && matches!(r, Ty::Flex(_)) {
                    l.clone()
                } else if matches!(l, Ty::Flex(_)) {
                    r.clone()
                } else {
                    l.clone()
                }
            }
            _ if !unary
                && matches!(op, ArithOp::Add | ArithOp::Sub | ArithOp::Mul)
                && !(matches!(l, Ty::Int(_) | Ty::Flex(_))
                    && matches!(r, Ty::Int(_) | Ty::Flex(_))) =>
            {
                return self.operator(body, (op, left, right, op_pos), (&l, &r));
            }
            (other, _) | (_, other) if !matches!(other, Ty::Int(_) | Ty::Flex(_)) => {
                let given = defs.describe(other, body.scope);
                self.errors.push(
                    Error::new(
                        op_pos,
                        format!("`{symbol}` is for whole numbers, and a side of it is {given}"),
                    )
                    .with_fix("only whole numbers have arithmetic so far"),
                );
                return Ty::Unknown;
            }
            _ => return Ty::Unknown,
        };
        if let Ty::Int(t) = gives {
            self.fits(body, expr, t);
        }
        gives
    }

    /// Checks `left + right`, `left - right`, or `left * right` of two types
    /// that are not both whole numbers, which is a call of the function of the
    /// trait of the operator that the two types choose. It compiles where the
    /// `requires` of that function is proven.
    fn operator(
        &mut self,
        body: &mut Body<'a>,
        (op, left, right, op_pos): (ArithOp, &Expr, &Expr, Pos),
        (l, r): (&Ty, &Ty),
    ) -> Ty {
        let defs = self.defs;
        let (k, name) = match op {
            ArithOp::Add => (defs.operators[0], "add"),
            ArithOp::Sub => (defs.operators[1], "sub"),
            _ => (defs.operators[2], "mul"),
        };
        let fi = traits::trait_function(&defs.file.traits[k], name).expect("a function");
        let types = vec![l.clone(), r.clone(), Ty::Unknown];
        match defs.find(k, &types, body.scope, &mut Vec::new()) {
            Found::Yes(all, _) => {
                let sig = &defs.trait_sigs[k][fi];
                body.facts.sibling = Some(k);
                self.contract_at(
                    body,
                    (op_pos, addr(left), None),
                    vec![(left, 0), (right, 1)],
                    &defs.file.traits[k].functions[fi],
                    sig,
                );
                body.facts.sibling = None;
                all[2].clone()
            }
            Found::No => {
                self.errors.push(Error::new(
                    op_pos,
                    format!(
                        "there is no implementation of `{}` for {}",
                        defs.file.traits[k].name,
                        defs.types_of(k, &types, body.scope)
                    ),
                ));
                Ty::Unknown
            }
            Found::Two(a, b) => {
                self.errors.push(defs.ambiguous(op_pos, a, b));
                Ty::Unknown
            }
            Found::Unknown => Ty::Unknown,
        }
    }

    /// Checks the measure `e` of a `decreases`, which is an expression of whole
    /// numbers of any types, worked out as an exact number that is never
    /// computed: so no `+`, `-`, or `*` in it is asked to fit a type. The
    /// type given is that of a whole number, where it is one.
    pub(super) fn measure_type(&mut self, body: &mut Body<'a>, e: &Expr) -> Ty {
        let Expr::Arith { left, right, .. } = e else {
            return self.expr(body, e, &Want::Any);
        };
        let (l, r) = (
            self.measure_type(body, left),
            self.measure_type(body, right),
        );
        let ty = match (l, r) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => Ty::Unknown,
            (Ty::Int(a), Ty::Int(b)) => Ty::Int(a.hull(b).unwrap_or(Int::U64)),
            (Ty::Flex(k), _) | (_, Ty::Flex(k)) => Ty::Flex(k),
            (other, _) => other,
        };
        self.types.insert(addr(e), ty.clone());
        ty
    }

    /// The type of a binding written `a..b` where a bound is a name, which is
    /// the smallest range that holds every number the range can have, and
    /// each name with whether it is the end. None, with the mistake reported,
    /// where a name is not one that cannot change.
    pub(super) fn named_range(
        &mut self,
        body: &Body<'a>,
        t: &Type,
    ) -> Option<(Ty, Vec<(String, bool)>)> {
        let Some((first, end)) = &t.range else {
            return None;
        };
        let mut ends = [0i128; 2];
        let mut named = Vec::new();
        for (at, (bound, is_end)) in [(first, false), (end, true)].into_iter().enumerate() {
            ends[at] = match bound {
                Bound::Number(n) => *n,
                Bound::Size(name) => {
                    let found = body
                        .names
                        .iter()
                        .rfind(|(n, _)| n == name)
                        .map(|(_, ty)| ty);
                    match found {
                        None => {
                            self.errors.push(
                                Error::new(
                                    t.pos,
                                    format!(
                                        "there is no name `{name}` defined before this, and a bound is one"
                                    ),
                                )
                                .with_fix("write the bound as a number, or bind the name first"),
                            );
                            return None;
                        }
                        Some(_) if body.vars.contains(name) => {
                            self.errors.push(
                                Error::new(
                                    t.pos,
                                    format!("`{name}` is a `var`, which can change, so its size is not a bound"),
                                )
                                .with_fix("bind its value to a name with `=`, and write the name"),
                            );
                            return None;
                        }
                        Some(Ty::Text) => {
                            self.errors.push(Error::unsupported(
                                t.pos,
                                "the `size` of a `str` as a bound",
                            ));
                            return None;
                        }
                        Some(Ty::List(_) | Ty::Slice(_)) => {
                            named.push((format!("{name}.size"), is_end));
                            if is_end { i128::from(u64::MAX) } else { 0 }
                        }
                        Some(Ty::Unknown) => return None,
                        Some(_) => {
                            self.errors.push(Error::new(
                                t.pos,
                                format!(
                                    "`{name}` is not a list, a slice, or a str, and the `size` of one is a bound"
                                ),
                            ));
                            return None;
                        }
                    }
                }
                Bound::Name(name) => {
                    let found = body
                        .names
                        .iter()
                        .rfind(|(n, _)| n == name)
                        .map(|(_, ty)| ty);
                    match found {
                        None => {
                            self.errors.push(
                                Error::new(
                                    t.pos,
                                    format!(
                                        "there is no name `{name}` defined before this, and a bound is one"
                                    ),
                                )
                                .with_fix("write the bound as a number, or bind the name first"),
                            );
                            return None;
                        }
                        Some(_) if body.vars.contains(name) => {
                            self.errors.push(
                                Error::new(
                                    t.pos,
                                    format!("`{name}` is a `var`, which can change, so it is not a bound"),
                                )
                                .with_fix("bind its value to a name with `=`, and write the name"),
                            );
                            return None;
                        }
                        Some(Ty::Int(int)) => {
                            named.push((name.clone(), is_end));
                            if is_end { int.max() } else { int.min() }
                        }
                        Some(Ty::Unknown) => return None,
                        Some(_) => {
                            self.errors.push(Error::new(
                                t.pos,
                                format!("`{name}` is not a whole number, and a bound is one"),
                            ));
                            return None;
                        }
                    }
                }
            };
        }
        let [min, end] = ends;
        match Int::between(min, (end - 1).max(min - 1)) {
            Some(int) => Some((Ty::Int(int), named)),
            None => {
                self.errors.push(Error::new(
                    t.pos,
                    format!(
                        "`{}` needs more than 64 bits, which is not built yet",
                        t.written()
                    ),
                ));
                None
            }
        }
    }

    /// What is wanted of the operand `side`, which is flexible, beside `other`,
    /// the type of the other operand, which is on the left when `other_left`.
    fn literal_want(&self, op: ArithOp, side: (&Expr, &Ty), other_left: bool) -> Want {
        let (literal, other) = side;
        if let (
            Expr::Number {
                magnitude,
                negative,
                ..
            },
            Ty::Int(own),
        ) = (literal, other)
            && (*magnitude <= i64::MAX as u128 || own.wide() && *magnitude <= i128::MAX as u128)
        {
            let value = if *negative {
                -(*magnitude as i128)
            } else {
                *magnitude as i128
            };
            if let Some(alone) = Int::between(value, value) {
                let fits = if other_left {
                    exact(op, *own, alone)
                } else {
                    exact(op, alone, *own)
                };
                if fits.is_some_and(|e| {
                    !e.wide() || own.wide() && matches!(op, ArithOp::Div | ArithOp::Rem)
                }) {
                    return Want::Ty(Ty::Int(alone));
                }
            }
        }
        Self::side_want(other)
    }

    /// What is wanted of a literal that is an operand, when the other
    /// operand has type `other`.
    pub(super) fn side_want(other: &Ty) -> Want {
        match other {
            Ty::Int(_) | Ty::Flex(_) | Ty::Float(_) => Want::Ty(other.clone()),
            _ => Want::Any,
        }
    }

    /// Checks `float(n)`, which is the float of the value of the whole number
    /// `n`: the float type that is wanted, and an `f64` where nothing decides.
    pub(super) fn float_of(&mut self, body: &mut Body<'a>, call: &Call, want: &Want) -> Ty {
        let defs = self.defs;
        let plain = call.args.iter().all(|a| a.name.is_none() && !a.after);
        if !plain {
            self.errors.push(Error::new(
                call.pos,
                "`float` takes the one whole number to make a float of",
            ));
            return self.unknown_call(body, call);
        }
        let value = &call.args[0].value;
        match self.expr(body, value, &Want::Any) {
            Ty::Int(_) => {}
            Ty::Unknown => return Ty::Unknown,
            other => {
                let given = defs.describe(&other, body.scope);
                self.errors.push(Error::new(
                    value.pos(),
                    format!("`float` takes a whole number, and this is {given}"),
                ));
                return Ty::Unknown;
            }
        }
        match want {
            Want::Ty(Ty::Float(float)) => Ty::Float(*float),
            _ => Ty::Float(Float::F64),
        }
    }

    /// Checks a float literal, which becomes the float type that is wanted
    /// of it, as `FromFloat` says, and an `f64` where nothing decides.
    pub(super) fn float_literal(
        &mut self,
        body: &Body,
        expr: &Expr,
        (text, negative): (&str, bool),
        pos: Pos,
        want: &Want,
    ) -> Ty {
        let float = match want {
            Want::Ty(Ty::Float(float)) => *float,
            Want::Any | Want::Unused | Want::Ty(Ty::Unknown | Ty::Flex(_)) => Float::F64,
            Want::Ty(needed) => {
                let needed = self.defs.describe(needed, body.scope);
                self.errors.push(Error::new(
                    pos,
                    format!("this is a float, where {needed} is needed"),
                ));
                return Ty::Unknown;
            }
        };
        if float.bits_of(text, negative).is_none() {
            self.errors.push(Error::new(
                pos,
                format!(
                    "`{}` is too large for `{}`, which has no number that large",
                    written_expr(expr),
                    float.written()
                ),
            ));
        }
        Ty::Float(float)
    }

    /// Checks a whole number literal, which becomes the type that is wanted
    /// of it, as `FromInt` says, when it fits.
    pub(super) fn number(
        &mut self,
        body: &Body,
        expr: &Expr,
        magnitude: u128,
        negative: bool,
        pos: Pos,
        want: &Want,
    ) -> Ty {
        let defs = self.defs;
        let int = match want {
            Want::Ty(Ty::Int(int)) => *int,
            Want::Any | Want::Unused => Int::I64,
            Want::Ty(t @ (Ty::Unknown | Ty::Flex(_))) => return t.clone(),
            Want::Ty(needed) => {
                let needed = defs.describe(needed, body.scope);
                self.errors.push(Error::new(
                    pos,
                    format!("this is a number, where {needed} is needed"),
                ));
                return Ty::Unknown;
            }
        };
        let value = if negative {
            -(magnitude as i128)
        } else {
            magnitude as i128
        };
        let fits = int.min() <= value && value <= int.max();
        // A character outside ASCII takes more than one byte in a text.
        let wide = expr_is_wide_character(expr, magnitude);
        if fits && wide && int.bits() == 8 {
            self.errors.push(Error::new(
                pos,
                format!(
                    "`{}` is not ASCII, so it takes more than one byte and is not a `{}`",
                    written_expr(expr),
                    self.defs.spelling(int)
                ),
            ));
        }
        if !fits {
            let because = match self.deciding {
                Some(at) => format!(", as the use at {}:{} needs", at.line, at.col),
                None => String::new(),
            };
            self.errors.push(Error::new(
                pos,
                format!(
                    "`{}` does not fit `{}`{because}, which holds {} to {}",
                    written_expr(expr),
                    self.defs.spelling(int),
                    int.min(),
                    int.max()
                ),
            ));
        }
        Ty::Int(int)
    }

    /// Checks a call of a function the compiler builds for the prelude: it
    /// takes whole numbers of one type, and gives the type, a `bool`, an
    /// `Order`, an `Option` of the type, or text.
    pub(super) fn intrinsic(&mut self, body: &mut Body<'a>, call: &Call) -> Ty {
        let defs = self.defs;
        let tys: Vec<Ty> = (call.args.iter())
            .map(|a| self.expr(body, &a.value, &Want::Any))
            .collect();
        let ty = tys[0].clone();
        match call.callee.as_str() {
            "__equal" | "__fits_add" | "__fits_sub" | "__fits_mul" => defs.bool_ty(),
            "__compare" => Ty::Enum(defs.order, Vec::new()),
            "__decimal" | "__float_text" => {
                self.needs(body, "alloc", call.pos, &format!("`{}`", call.callee));
                Ty::Text
            }
            name if name.starts_with("__checked_") => Ty::Enum(defs.option, vec![ty]),
            _ => ty,
        }
    }

    /// Checks a call of `__system`, which gives the 64-bit number the kernel
    /// gives, of `__reserve`, which gives a list of bytes with room and no
    /// elements, and of `__sized`, which gives the list it is given with the
    /// size it says.
    pub(super) fn kernel(&mut self, body: &mut Body<'a>, call: &Call) -> Ty {
        let tys: Vec<Ty> = (call.args.iter())
            .map(|a| self.expr(body, &a.value, &Want::Any))
            .collect();
        let bytes = Ty::List(vec![Ty::Int(Int::U8)]);
        match call.callee.as_str() {
            "__system" => Ty::Int(Int::I64),
            "__consume" => Ty::Nothing,
            "__reserve" => {
                self.needs(body, "alloc", call.pos, "`__reserve`");
                bytes
            }
            _ => tys.first().cloned().unwrap_or(bytes),
        }
    }

    /// Checks a call of `wrapping_div` or `wrapping_rem`, which compiles only
    /// when `arith` proves that the divisor is not zero.
    pub(super) fn wrapping_division(&mut self, body: &mut Body<'a>, call: &Call, sig: &Signature) {
        let by = (params_of(&sig.params, &call.args).into_iter().flatten())
            .position(|p| p == 1)
            .map(|i| &call.args[i].value);
        let op = if call.callee == "wrapping_div" {
            ArithOp::Div
        } else {
            ArithOp::Rem
        };
        let given: Vec<String> = call.args.iter().map(|a| written_expr(&a.value)).collect();
        if let Some(by) = by {
            let whole = format!("{}({})", call.callee, given.join(", "));
            self.not_zero(body, by, (whole, op), call.pos);
        }
    }

    /// Checks the amount of a call of `shift_left` or `shift_right`, which
    /// compiles only when `arith` proves that it is below the number of bits
    /// of the type `ty` that is shifted.
    pub(super) fn shift(&mut self, body: &mut Body<'a>, call: &Call, sig: &Signature, ty: &Ty) {
        let by = (params_of(&sig.params, &call.args).into_iter().flatten())
            .position(|p| p == 1)
            .map(|i| &call.args[i].value);
        match (ty, by) {
            (Ty::Int(t), Some(by)) => self.shifts(body, by, *t, call.pos),
            (Ty::Unknown, _) | (_, None) => {}
            _ => self.errors.push(Error::unsupported(
                call.pos,
                "a shift of a number whose type is a type parameter",
            )),
        }
    }

    /// Checks a call of `bit_and`, `bit_or`, `bit_xor`, `shift_left`, or
    /// `shift_right` of `Bits`, which gives the exact range of what it makes
    /// of numbers that are not negative, and takes the range that fills a size
    /// of numbers that may be negative. None, before anything is checked,
    /// where the arguments are not two, written by position.
    pub(super) fn bits_exact(
        &mut self,
        body: &mut Body<'a>,
        call: &Call,
        (op, k, fi): (BitFn, usize, usize),
    ) -> Option<Ty> {
        let defs = self.defs;
        if call.args.len() != 2 || call.args.iter().any(|a| a.name.is_some() || a.after) {
            return None;
        }
        let name = call.callee.as_str();
        let shifts = matches!(op, BitFn::Left | BitFn::Right);
        let (first, second) = (&call.args[0].value, &call.args[1].value);
        // A literal beside a number that is not negative is the range that
        // holds it alone, and beside any other number the type of it.
        let alone = |e: &Expr| match e {
            Expr::Number {
                magnitude,
                negative: false,
                ..
            } if *magnitude <= i64::MAX as u128 => {
                let n = *magnitude as i128;
                Int::between(n, n).map_or(Want::Any, |t| Want::Ty(Ty::Int(t)))
            }
            _ => Want::Any,
        };
        let beside = |lit: &Expr, other: &Ty| match other {
            Ty::Int(o) if o.min() >= 0 => match alone(lit) {
                Want::Any => Self::side_want(other),
                w => w,
            },
            _ => Self::side_want(other),
        };
        let (ta, tb) = if shifts {
            let ta = self.expr(body, first, &alone(first));
            let by = if flexible(second) {
                match alone(second) {
                    Want::Any => Want::Ty(Ty::Int(Int::U8)),
                    w => w,
                }
            } else {
                Want::Any
            };
            (ta, self.expr(body, second, &by))
        } else {
            match (flexible(first), flexible(second)) {
                (true, false) => {
                    let b = self.expr(body, second, &Want::Any);
                    let want = beside(first, &b);
                    (self.expr(body, first, &want), b)
                }
                (false, true) => {
                    let a = self.expr(body, first, &Want::Any);
                    let want = beside(second, &a);
                    let b = self.expr(body, second, &want);
                    (a, b)
                }
                (true, true) => (
                    self.expr(body, first, &alone(first)),
                    self.expr(body, second, &alone(second)),
                ),
                (false, false) => (
                    self.expr(body, first, &Want::Any),
                    self.expr(body, second, &Want::Any),
                ),
            }
        };
        let (a, b) = match (&ta, &tb) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => return Some(Ty::Unknown),
            (Ty::Int(a), Ty::Int(b)) => (*a, *b),
            (Ty::Flex(_) | Ty::Int(_), Ty::Flex(_) | Ty::Int(_)) if !shifts => {
                use_at(call.pos);
                if !flex_pair(&ta, &tb) {
                    unreachable!("a number and a number")
                }
                return Some(if matches!(ta, Ty::Flex(_)) { tb } else { ta });
            }
            (Ty::Flex(_), _) => {
                use_at(call.pos);
                return Some(ta);
            }
            _ => {
                // Not whole numbers: what the trait asks of the type.
                let types = [ta.clone()];
                if ta != tb {
                    let (x, y) = (
                        defs.describe(&ta, body.scope),
                        defs.describe(&tb, body.scope),
                    );
                    self.errors.push(Error::new(
                        call.pos,
                        format!("`{name}` takes two values of one type, and these are {x} and {y}"),
                    ));
                    return Some(Ty::Unknown);
                }
                return match defs.find(k, &types, body.scope, &mut Vec::new()) {
                    Found::Yes(..) => Some(ta),
                    Found::No => {
                        self.errors
                            .push(self.no_implementation(body, call, k, &types));
                        Some(Ty::Unknown)
                    }
                    Found::Two(x, y) => {
                        self.errors.push(defs.ambiguous(call.pos, x, y));
                        Some(Ty::Unknown)
                    }
                    Found::Unknown => Some(Ty::Unknown),
                };
            }
        };
        let sig = &defs.trait_sigs[k][fi];
        let written = format!("{name}({}, {})", written_expr(first), written_expr(second));
        if shifts && !Int::U8.holds(b) {
            // The amount is a `Byte`, which it enters where `arith` shows it fits.
            self.enters(body, second, (b, Int::U8));
        }
        let negative = a.min() < 0 || (!shifts && b.min() < 0);
        if negative {
            if (!shifts && a != b) || !Int::ALL.contains(&a) {
                let message = if !shifts && a != b {
                    format!(
                        "`{name}` of a number that may be negative takes two numbers of one \
                         range, and these are `{}` and `{}`",
                        defs.spelling(a),
                        defs.spelling(b)
                    )
                } else {
                    format!(
                        "`{name}` of a number that may be negative takes a range that fills \
                         a size, and `{}` does not",
                        defs.spelling(a)
                    )
                };
                self.errors.push(
                    Error::new(call.pos, message)
                        .with_fix("bind the number to a type that fills a size, as `i32` does"),
                );
                return Some(Ty::Unknown);
            }
            let mistakes = self.errors.len();
            if shifts {
                self.shifts(body, second, a, call.pos);
            }
            if shifts && op == BitFn::Left && self.errors.len() == mistakes {
                let most = self.amount_at_most(body, second, b);
                if !(self.proves_product_fits(body, first, 1i128 << most, a)) {
                    self.errors.push(
                        Error::new(
                            call.pos,
                            format!(
                                "`{written}` may drop a bit, as `shift_left` drops none, and it \
                                 does not fit a `{}`",
                                defs.spelling(a)
                            ),
                        )
                        .with_fix("use `wrapping_shift_left` where the bits that leave the top are meant to go"),
                    );
                }
            }
            body.facts.note_bit_call(call, sig, name, a);
            return Some(Ty::Int(a));
        }
        let result = match op {
            BitFn::Right => {
                self.amount_below(body, second, 64, "a shift", call.pos);
                let (lo, hi) = (b.min().clamp(0, 63), b.max().clamp(0, 63));
                Int::between(a.min() >> hi, a.max() >> lo)
            }
            BitFn::Left => {
                let mistakes = self.errors.len();
                self.amount_below(body, second, 64, "a shift", call.pos);
                let most = if self.errors.len() == mistakes {
                    self.amount_at_most(body, second, b)
                } else {
                    63
                };
                let (lo, hi) = (
                    a.min().checked_mul(1i128 << b.min().clamp(0, 63)),
                    a.max().checked_mul(1i128 << most),
                );
                // A shift that no type of 64 bits holds is of the type of the number,
                // where it is shown to fit.
                match lo
                    .zip(hi)
                    .and_then(|(lo, hi)| Int::between(lo, hi))
                    .filter(|r| !r.wide())
                {
                    Some(r) => Some(r),
                    // No type holds the exact range, so the result is the type
                    // of the number, as a product's is, where `arith` proves
                    // that it fits.
                    None => {
                        if self.errors.len() == mistakes
                            && !self.proves_product_fits(body, first, 1i128 << most, a)
                        {
                            self.errors.push(
                                Error::new(
                                    call.pos,
                                    format!(
                                        "cannot prove that `{written}` fits `{}`, as the number \
                                         times two to the amount may be more",
                                        defs.spelling(a)
                                    ),
                                )
                                .with_fix(
                                    "check the number and the amount first, or use `wrapping_shift_left`",
                                ),
                            );
                        }
                        body.facts.note_bit_call(call, sig, name, a);
                        return Some(Ty::Int(a));
                    }
                }
            }
            _ => {
                let ends = |t: Int| (t.min() as u64, t.max() as u64);
                let (lo, hi) = bit_bounds(op, (ends(a), ends(b)));
                Int::between(i128::from(lo), i128::from(hi))
            }
        };
        let result = result.expect("a range of numbers that are not negative");
        // Each side is the number at the size the work is done at.
        let sides: [(&Expr, Int); 2] = [(first, a), (second, b)];
        for (side, own) in sides {
            let at = match op {
                BitFn::Left if std::ptr::eq(side, first) => result,
                BitFn::Left | BitFn::Right if std::ptr::eq(side, second) => continue,
                BitFn::Right => own,
                _ => a.hull(b).expect("a range"),
            };
            if own != at {
                self.widened.insert(addr(side), at);
            }
        }
        body.facts.note_bit_call(call, sig, name, result);
        Some(Ty::Int(result))
    }

    /// The greatest amount that `by` is proven to be, of at most 63, which is
    /// the greatest of its type where nothing proves less.
    fn amount_at_most(&mut self, body: &mut Body<'a>, by: &Expr, ty: Int) -> u32 {
        if let Expr::Number { magnitude, .. } = by {
            return (*magnitude).min(63) as u32;
        }
        let (mut low, mut high) = (ty.min().clamp(0, 63), ty.max().clamp(0, 63));
        while low < high {
            let mid = low + (high - low) / 2;
            if self.proves_at_most(body, by, mid) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        high as u32
    }

    /// Checks `narrow(x)` or one of its relatives, which take the whole
    /// number `x` to the whole number type that is expected, one that does
    /// not hold all the values of the type of `x`. `narrow` is expected to
    /// give an `Option` of it.
    pub(super) fn narrow(
        &mut self,
        body: &mut Body<'a>,
        call: &Call,
        narrowing: Narrowing,
        want: &Want,
    ) -> Ty {
        let defs = self.defs;
        let name = call.callee.as_str();
        let plain = call.args.iter().all(|a| a.name.is_none() && !a.after);
        if call.args.len() != 1 || !plain {
            self.errors.push(Error::new(
                call.pos,
                format!("`{name}` takes the one whole number to narrow"),
            ));
            return self.unknown_call(body, call);
        }
        let value = &call.args[0].value;
        let given = self.expr(body, value, &Want::Any);
        let from = match given {
            Ty::Int(from) => from,
            Ty::Unknown | Ty::Flex(_) => return Ty::Unknown,
            other => {
                let given = defs.describe(&other, body.scope);
                self.errors.push(Error::new(
                    value.pos(),
                    format!("`{name}` takes a whole number, and this is {given}"),
                ));
                return Ty::Unknown;
            }
        };
        let (wanted, to) = match want {
            Want::Ty(Ty::Unknown | Ty::Flex(_)) => return Ty::Unknown,
            Want::Ty(Ty::Int(to)) if narrowing != Narrowing::Checked => (Ty::Int(*to), *to),
            Want::Ty(Ty::Enum(e, of)) if *e == defs.option && narrowing == Narrowing::Checked => {
                match of.as_slice() {
                    [Ty::Int(to)] => (Ty::Enum(*e, of.clone()), *to),
                    _ => return self.narrow_unexpected(body, call, name, want),
                }
            }
            _ => return self.narrow_unexpected(body, call, name, want),
        };
        if to.min() <= from.min() && from.max() <= to.max() {
            let error = if to == from {
                Error::new(
                    call.pos,
                    format!(
                        "this is already a `{}`, so there is nothing to narrow",
                        self.defs.spelling(from)
                    ),
                )
            } else {
                Error::new(
                    call.pos,
                    format!(
                        "`{}` holds every value of `{}`, so there is nothing to narrow",
                        self.defs.spelling(to),
                        self.defs.spelling(from)
                    ),
                )
                .with_fix("use `convert`")
            };
            self.errors.push(error);
            return Ty::Unknown;
        }
        wanted
    }

    /// The error for a narrowing that is not given the type it gives.
    pub(super) fn narrow_unexpected(
        &mut self,
        body: &Body,
        call: &Call,
        name: &str,
        want: &Want,
    ) -> Ty {
        let message = match want {
            Want::Ty(needed) => {
                let needed = self.defs.describe(needed, body.scope);
                format!(
                    "`{name}` gives {}, and {needed} is needed",
                    self.narrowed(name)
                )
            }
            _ => format!("`{name}` gives the type that is expected here, and none is"),
        };
        let fix = match name {
            "narrow" => "write the type, as in `y: Option[u8] = narrow(x)`",
            _ => "write the type, as in `y: u8 = clamp(x)`",
        };
        self.errors
            .push(Error::new(call.pos, message).with_fix(fix));
        Ty::Unknown
    }

    /// What a narrowing gives, in words.
    pub(super) fn narrowed(&self, name: &str) -> &'static str {
        match name {
            "narrow" => "an `Option` of a whole number",
            _ => "a whole number",
        }
    }

    /// Checks `__convert(a)`, which the prelude's conversions between whole
    /// numbers call, and which gives the type that is expected.
    pub(super) fn convert_intrinsic(
        &mut self,
        body: &mut Body<'a>,
        call: &Call,
        want: &Want,
    ) -> Ty {
        self.expr(body, &call.args[0].value, &Want::Any);
        match want {
            Want::Ty(t) => t.clone(),
            _ => Ty::Unknown,
        }
    }
}
