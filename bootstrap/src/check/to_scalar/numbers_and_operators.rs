//! Lowers numbers and operators: a number written out, `+`, `-`, and `*`, the
//! functions the compiler builds for comparing, for `wrapping_`, `saturating_`,
//! and `checked_` arithmetic, for `Bits`, and for `narrow` and its relatives, and
//! `Convert` between numbers.

use super::float_bits;
use super::*;
use crate::check::numbers_and_operators::{BitFn, bits_function, is_negation};

/// How many bits a number of the type has, in the scalar form.
pub(super) fn bits_of(t: Int) -> form::Bits {
    match t.bits() {
        8 => form::Bits::B8,
        16 => form::Bits::B16,
        32 => form::Bits::B32,
        _ => form::Bits::B64,
    }
}

/// The value of a whole number literal.
pub(super) fn signed(magnitude: u128, negative: bool) -> i128 {
    let magnitude = magnitude as i128;
    if negative { -magnitude } else { magnitude }
}

impl<'a> Lowering<'a> {
    /// Lowers a whole number literal.
    pub(super) fn number(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        (magnitude, negative): (u128, bool),
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let typed = self.typed;
        let ty = typed.type_of(expr, env);
        let Ty::Int(t) = ty else {
            unreachable!("checked")
        };
        let magnitude = magnitude as i128;
        let value = if negative { -magnitude } else { magnitude };
        if t.wide() {
            return (
                ty,
                vec![
                    out.int(form::Bits::B64, value),
                    out.int(form::Bits::B64, value >> 64),
                ],
            );
        }
        (ty, vec![out.int(bits_of(t), value)])
    }

    /// Lowers `left + right`, `left - right`, `left * right`, `left / right`,
    /// or `left % right`.
    pub(super) fn operation(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        (op, left, right, op_pos): (ArithOp, &'a Expr, &'a Expr, Pos),
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let typed = self.typed;
        let ty = typed.type_of(expr, env);
        if let Ty::Float(_) = ty {
            return self.float_operation(out, (op, left, right, op_pos), ty, env);
        }
        if let Some((instance, gives)) = typed.operator(expr, op, (left, right), env) {
            return self.operator_call(out, instance, gives, (left, right), env);
        }
        let (taken, a) = self.expr(out, left, env);
        let b = self.expr(out, right, env).1;
        // The numbers are two words when the result is, and when a quotient or
        // a remainder is taken of numbers that are, whose result is smaller.
        if matches!(&ty, Ty::Int(t) if t.wide()) || matches!(&taken, Ty::Int(t) if t.wide()) {
            let not_negative = matches!(
                typed.type_of(left, env),
                Ty::Int(t) if t.min() >= 0
            );
            // Two numbers of 64 bits or fewer that are both signed or both not
            // have a product whose halves are the two results of one
            // instruction, and no product of the words above is needed.
            let halves = match (typed.type_of(left, env), typed.type_of(right, env)) {
                (Ty::Int(l), Ty::Int(r)) if !l.wide() && !r.wide() && l.signed() == r.signed() => {
                    Some(if l.signed() {
                        form::Binary::MulHighS
                    } else {
                        form::Binary::MulHighU
                    })
                }
                _ => None,
            };
            return self.wide_operation(
                out,
                (op, right, not_negative, halves),
                (ty.clone(), taken, a, b),
            );
        }
        let b = b[0];
        let word = match op {
            crate::parser::ArithOp::Add => form::Binary::Add,
            crate::parser::ArithOp::Sub => form::Binary::Sub,
            crate::parser::ArithOp::Mul => form::Binary::Mul,
            crate::parser::ArithOp::Div | crate::parser::ArithOp::Rem => {
                return self.division(out, (op, right), (ty, taken, a[0], b));
            }
        };
        (ty, vec![out.binary(word, a[0], b)])
    }

    /// Lowers an operator that is a call of the function `instance` of the
    /// trait of the operator, on the two sides.
    fn operator_call(
        &mut self,
        out: &mut Out<'a>,
        instance: Instance,
        gives: Ty,
        (left, right): (&'a Expr, &'a Expr),
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let defs = self.typed.defs;
        let params = defs.params(instance.template);
        let prepared = self.copy_for(out, &instance);
        let mut args = Vec::new();
        for (side, (_, param)) in [left, right].into_iter().zip(&params) {
            let param = param.subst(&instance.args);
            args.extend(self.passed(out, side, &param, env));
        }
        let results = self.finish_call(out, prepared, args, &gives);
        (gives, results)
    }

    /// Lowers `left + right`, `left - right`, `left * right`, or
    /// `left / right` of two floats, which is one instruction that rounds
    /// once, and `-right`, which the parser writes as `0 - right`.
    fn float_operation(
        &mut self,
        out: &mut Out<'a>,
        (op, left, right, op_pos): (ArithOp, &'a Expr, &'a Expr, Pos),
        ty: Ty,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        if is_negation(op, left, op_pos) {
            let value = self.expr(out, right, env).1[0];
            return (ty, vec![out.float_negate(value)]);
        }
        let a = self.expr(out, left, env).1[0];
        let b = self.expr(out, right, env).1[0];
        let word = match op {
            ArithOp::Add => form::FloatBinary::Add,
            ArithOp::Sub => form::FloatBinary::Sub,
            ArithOp::Mul => form::FloatBinary::Mul,
            ArithOp::Div => form::FloatBinary::Div,
            ArithOp::Rem => unreachable!("refused"),
        };
        (ty, vec![out.float_binary(word, a, b)])
    }

    /// Lowers a quotient or what is left over, from the two sides at the size
    /// that holds both and the result, which the checker made them. The
    /// divisor is not zero, and the least number is not divided by `-1`, as
    /// the size holds the result too. An unsigned number divided by a power of
    /// two is shifted, and what is left of it is the bits below the power.
    fn division(
        &mut self,
        out: &mut Out<'a>,
        (op, right): (ArithOp, &'a Expr),
        (ty, taken, a, b): (Ty, Ty, form::Value, form::Value),
    ) -> (Ty, Vec<form::Value>) {
        let (Ty::Int(result), Ty::Int(at)) = (&ty, &taken) else {
            unreachable!("checked")
        };
        let bits = bits_of(*at);
        let power = match right {
            Expr::Number {
                magnitude,
                negative: false,
                ..
            } if !at.signed() && magnitude.is_power_of_two() => Some(*magnitude),
            _ => None,
        };
        let value = match (op, power) {
            (ArithOp::Div, Some(power)) => {
                let by = out.int(bits, i128::from(power.trailing_zeros()));
                out.binary(form::Binary::ShiftRightU, a, by)
            }
            (_, Some(power)) => {
                let mask = out.int(bits, power as i128 - 1);
                out.binary(form::Binary::And, a, mask)
            }
            (ArithOp::Div, None) if at.signed() => out.binary(form::Binary::DivS, a, b),
            (ArithOp::Div, None) => out.binary(form::Binary::DivU, a, b),
            (_, None) if at.signed() => out.binary(form::Binary::RemS, a, b),
            (_, None) => out.binary(form::Binary::RemU, a, b),
        };
        // The result is kept at the size of its own range.
        let value = out.resize(result.signed(), value, bits_of(*result));
        (ty, vec![value])
    }

    /// Whether the call is `Eq::eq` or `Ord::compare` of two values that are
    /// whole numbers or `bool`, whose `Eq` and `Ord` no program writes, and
    /// which are therefore compared where they are.
    pub(super) fn compares_scalars(&self, call: &Call, name: &str, env: &[Ty]) -> bool {
        let typed = self.typed;
        matches!(&call.qualifier, Some((q, _)) if q == if name == "eq" { "Eq" } else { "Ord" })
            && call.callee == name
            && call.args.len() == 2
            && match typed.type_of(&call.args[0].value, env) {
                Ty::Int(_) | Ty::Float(_) => true,
                Ty::Enum(e, _) => e == typed.defs.boolean,
                _ => false,
            }
    }

    /// Lowers `left == right` of whole numbers or `bool`s: one comparison.
    pub(super) fn equal_scalars(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let (ta, a) = self.expr(out, &call.args[0].value, env);
        let (tb, b) = self.expr(out, &call.args[1].value, env);
        if let (Ty::Float(_), Ty::Float(_)) = (&ta, &tb) {
            return (self.typed.defs.bool_ty(), vec![out.float_equal(a[0], b[0])]);
        }
        if let Some(mixed) = Mixed::of(&ta, &tb) {
            return (
                self.typed.defs.bool_ty(),
                vec![mixed.stand(out, a[0], b[0]).0],
            );
        }
        if matches!(&ta, Ty::Int(t) if t.wide()) {
            return (
                self.typed.defs.bool_ty(),
                vec![self.wide_equal(out, &a, &b)],
            );
        }
        (
            self.typed.defs.bool_ty(),
            vec![out.compare(form::Compare::Equal, a[0], b[0])],
        )
    }

    /// Lowers a `match` that the parser wrote for `<`, `<=`, `>`, `>=`, or
    /// `!=` of whole numbers or `bool`s: one comparison, and for `<=`, `>=`,
    /// and `!=` the answer of the opposite comparison with its bit flipped,
    /// which `fold` leaves out when a block branches on it. Gives `None` for any
    /// other `match`.
    pub(super) fn comparison(
        &mut self,
        out: &mut Out<'a>,
        m: &'a Match,
        env: &[Ty],
    ) -> Option<Vec<form::Value>> {
        let Expr::Call(call) = &m.of else {
            return None;
        };
        if m.form == Form::Not {
            if !self.compares_scalars(call, "eq", env) {
                return None;
            }
            let same = self.equal_scalars(out, call, env).1[0];
            let one = out.int(form::Bits::B8, 1);
            return Some(vec![out.binary(form::Binary::Xor, same, one)]);
        }
        if m.form != Form::Match || !self.compares_scalars(call, "compare", env) {
            return None;
        }
        // What each of `less`, `equal`, and `greater` answers.
        let mut answers = [None; 3];
        for arm in &m.arms {
            let Pattern::Case {
                name, inner: None, ..
            } = &arm.pattern
            else {
                return None;
            };
            let at = ["less", "equal", "greater"]
                .iter()
                .position(|c| c == name)?;
            let [Stmt::Expr(Expr::Name { name, .. })] = &arm.body[..] else {
                return None;
            };
            answers[at] = Some(match name.as_str() {
                "true" => true,
                "false" => false,
                _ => return None,
            });
        }
        // Both sides are the same range, the first as it is given here, or
        // two that no type of 64 bits holds.
        let (ty, a_all) = self.expr(out, &call.args[0].value, env);
        let a = a_all[0];
        let (other, b_all) = self.expr(out, &call.args[1].value, env);
        let b = b_all[0];
        if let (Ty::Float(_), Ty::Float(_)) = (&ty, &other) {
            return self.float_comparison(out, answers, (a, b));
        }
        if let Some(mixed) = Mixed::of(&ty, &other) {
            return self.mixed_comparison(out, mixed, answers, (a, b));
        }
        if let Ty::Int(t) = &ty
            && t.wide()
        {
            // `a <= b` is not `b < a`, and `a >= b` is not `a < b`.
            let (x, y, flip) = match answers {
                [Some(true), Some(false), Some(false)] => (&a_all, &b_all, false),
                [Some(false), Some(false), Some(true)] => (&b_all, &a_all, false),
                [Some(true), Some(true), Some(false)] => (&b_all, &a_all, true),
                [Some(false), Some(true), Some(true)] => (&a_all, &b_all, true),
                _ => return None,
            };
            // Each side is as signed as the range it was written with: the range
            // both are taken at may hold a number above what a signed word of
            // 128 bits does and a negative one.
            let signed =
                |side: &Expr| matches!(self.typed.type_of(side, env), Ty::Int(t) if t.signed());
            let (first, second) = (signed(&call.args[0].value), signed(&call.args[1].value));
            let sides = if std::ptr::eq(x, &a_all) {
                (first, second)
            } else {
                (second, first)
            };
            let below = self.wide_less(out, sides, x, y);
            if !flip {
                return Some(vec![below]);
            }
            let one = out.int(form::Bits::B8, 1);
            return Some(vec![out.binary(form::Binary::Xor, below, one)]);
        }
        let less = match ty {
            Ty::Int(t) if t.signed() => form::Compare::LessS,
            _ => form::Compare::LessU,
        };
        // `a <= b` is not `b < a`, and `a >= b` is not `a < b`.
        let (op, x, y, flip) = match answers {
            [Some(true), Some(false), Some(false)] => (less, a, b, false),
            [Some(false), Some(false), Some(true)] => (less, b, a, false),
            [Some(true), Some(true), Some(false)] => (less, b, a, true),
            [Some(false), Some(true), Some(true)] => (less, a, b, true),
            _ => return None,
        };
        let below = out.compare(op, x, y);
        if !flip {
            return Some(vec![below]);
        }
        let one = out.int(form::Bits::B8, 1);
        Some(vec![out.binary(form::Binary::Xor, below, one)])
    }

    /// What the comparison of two floats, as the answers of `less`, `equal`,
    /// and `greater` say, gives, by how floats are ordered: `-0.0` is below
    /// `0.0`, and a float that is not a number is above all others.
    fn float_comparison(
        &mut self,
        out: &mut Out<'a>,
        answers: [Option<bool>; 3],
        (a, b): (form::Value, form::Value),
    ) -> Option<Vec<form::Value>> {
        let (x, y, flip) = match answers {
            [Some(true), Some(false), Some(false)] => (a, b, false),
            [Some(false), Some(false), Some(true)] => (b, a, false),
            [Some(true), Some(true), Some(false)] => (b, a, true),
            [Some(false), Some(true), Some(true)] => (a, b, true),
            _ => return None,
        };
        let below = out.float_less(x, y);
        if !flip {
            return Some(vec![below]);
        }
        let one = out.int(form::Bits::B8, 1);
        Some(vec![out.binary(form::Binary::Xor, below, one)])
    }

    /// What the comparison of two ranges that no type of 64 bits holds, as the
    /// answers of `less`, `equal`, and `greater` say, gives: the numbers of the
    /// one range that has negative ones are below those of the other when it
    /// has a negative number, and the two are otherwise compared as numbers
    /// that are not negative.
    fn mixed_comparison(
        &mut self,
        out: &mut Out<'a>,
        mixed: Mixed,
        answers: [Option<bool>; 3],
        (a, b): (form::Value, form::Value),
    ) -> Option<Vec<form::Value>> {
        let (equal, below) = mixed.stand(out, a, b);
        // The answer for each of `less`, `equal`, and `greater`.
        let said = |out: &mut Out<'a>, answer: Option<bool>| {
            out.int(form::Bits::B8, i128::from(answer == Some(true)))
        };
        let (l, e, g) = (
            said(out, answers[0]),
            said(out, answers[1]),
            said(out, answers[2]),
        );
        let inner = out.choose(equal, e, g);
        Some(vec![out.choose(below, l, inner)])
    }

    /// Whether the call is `Ord::compare` of two ranges that no type of 64 bits
    /// holds, which is a call of its own and not a `match` on `<` or `>=`.
    pub(super) fn compares_mixed(&self, call: &Call, env: &[Ty]) -> bool {
        matches!(&call.qualifier, Some((q, _)) if q == "Ord")
            && call.callee == "compare"
            && call.args.len() == 2
            && Mixed::of(
                &self.typed.type_of(&call.args[0].value, env),
                &self.typed.type_of(&call.args[1].value, env),
            )
            .is_some()
    }

    /// Whether the call is `Ord::compare` of two numbers of which a range of more
    /// than 64 bits holds both, which the checker made both two words.
    pub(super) fn compares_wide(&self, call: &Call, env: &[Ty]) -> bool {
        matches!(&call.qualifier, Some((q, _)) if q == "Ord")
            && call.callee == "compare"
            && call.args.len() == 2
            && matches!(
                (
                    self.typed.type_of(&call.args[0].value, env),
                    self.typed.type_of(&call.args[1].value, env),
                ),
                (Ty::Int(a), Ty::Int(b)) if a.wide() || b.wide() || a.hull(b).is_some_and(|h| h.wide())
            )
    }

    /// Whether the call is `Fmt::fmt` of a number of a range of more than 64 bits.
    pub(super) fn formats_wide(&self, call: &Call, env: &[Ty]) -> bool {
        matches!(&call.qualifier, Some((q, _)) if q == "Fmt")
            && call.callee == "fmt"
            && call.args.len() == 1
            && matches!(
                self.typed.type_of(&call.args[0].value, env),
                Ty::Int(t) if t.wide()
            )
    }

    /// Lowers `Fmt::fmt` of a number that is two words: its digits in decimal.
    pub(super) fn format_wide(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let (ty, values) = self.expr(out, &call.args[0].value, env);
        let Ty::Int(t) = ty else {
            unreachable!("checked")
        };
        let signed = out.int(form::Bits::B8, i128::from(t.signed()));
        let text = self.call_runtime(
            out,
            Runtime::DecimalWide,
            vec![values[0], values[1], signed],
            TEXT.to_vec(),
        );
        (Ty::Text, text)
    }

    /// Lowers `Ord::compare` of numbers that are two words, as the `Order` of
    /// the numbers.
    pub(super) fn compare_wide(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let defs = self.typed.defs;
        let (ta, a) = self.expr(out, &call.args[0].value, env);
        let (tb, b) = self.expr(out, &call.args[1].value, env);
        let (Ty::Int(x), Ty::Int(y)) = (ta, tb) else {
            unreachable!("checked")
        };
        let (a, b) = (self.wide_from(out, x, &a), self.wide_from(out, y, &b));
        let equal = self.wide_equal(out, &a, &b);
        // Each side is as signed as the range it was written with, not as the
        // range both are taken at.
        let signed =
            |side: &Expr| matches!(self.typed.type_of(side, env), Ty::Int(t) if t.signed());
        let sides = (signed(&call.args[0].value), signed(&call.args[1].value));
        let below = self.wide_less(out, sides, &a, &b);
        let case = |out: &mut Out<'a>, name: &str| {
            let k = defs.case_of(defs.order, name).expect("a case");
            out.int(form::Bits::B8, k as i128)
        };
        let (less, same, greater) = (case(out, "less"), case(out, "equal"), case(out, "greater"));
        let inner = out.choose(equal, same, greater);
        (
            Ty::Enum(defs.order, Vec::new()),
            vec![out.choose(below, less, inner)],
        )
    }

    /// Lowers `Ord::compare` of two ranges that no type of 64 bits holds, as the
    /// `Order` of the numbers.
    pub(super) fn compare_mixed(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let defs = self.typed.defs;
        let (ta, a) = self.expr(out, &call.args[0].value, env);
        let (tb, b) = self.expr(out, &call.args[1].value, env);
        let mixed = Mixed::of(&ta, &tb).expect("checked");
        let (equal, below) = mixed.stand(out, a[0], b[0]);
        let case = |out: &mut Out<'a>, name: &str| {
            let k = defs.case_of(defs.order, name).expect("a case");
            out.int(form::Bits::B8, k as i128)
        };
        let (less, same, greater) = (case(out, "less"), case(out, "equal"), case(out, "greater"));
        let inner = out.choose(equal, same, greater);
        (
            Ty::Enum(defs.order, Vec::new()),
            vec![out.choose(below, less, inner)],
        )
    }

    /// Lowers `float(n)`: the float nearest the value of `n`, rounded once,
    /// to the nearest, ties to even.
    pub(super) fn float_call(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let (given, values) = self.expr(out, &call.args[0].value, env);
        let (Ty::Int(from), ty @ Ty::Float(float)) = (given, self.typed.type_of(expr, env)) else {
            unreachable!("checked")
        };
        let value = out.float_from_int(from.signed(), values[0], float_bits(float));
        (ty, vec![value])
    }

    /// Lowers a call of `narrow` or one of its relatives.
    pub(super) fn narrow_call(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        narrowing: Narrowing,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let typed = self.typed;
        let (given, values) = self.expr(out, &call.args[0].value, env);
        let (Ty::Int(from), ty) = (given, typed.type_of(expr, env)) else {
            unreachable!("checked")
        };
        let to = match &ty {
            Ty::Int(to) => *to,
            Ty::Enum(_, of) => match of.as_slice() {
                [Ty::Int(to)] => *to,
                _ => unreachable!("checked"),
            },
            _ => unreachable!("checked"),
        };
        let values = self.narrowing(out, narrowing, (from, to), &values);
        (ty, values)
    }

    /// Lowers `__system`, which asks the kernel for the call its first
    /// argument numbers, in the order of `Raw`, with the words after it: a
    /// whole number is widened to 64 bits, and a list, a slice, or a text is
    /// its address. It gives what the kernel gives. Lowers `__reserve`, which
    /// asks for as many bytes as it is given and gives a list of them with
    /// none as its elements, and `__sized`, which gives the list it is given
    /// with as many elements as it is told.
    pub(super) fn kernel_call(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let ty = self.typed.type_of(expr, env);
        let mut words = Vec::new();
        for arg in &call.args {
            let (ty, values) = self.expr(out, &arg.value, env);
            words.push(match ty {
                Ty::Int(i) => out.resize(i.signed(), values[0], form::Bits::B64),
                _ => values[0],
            });
        }
        match call.callee.as_str() {
            "__consume" => (ty, Vec::new()),
            "__reserve" => {
                let address = out.emit(
                    form::Op::System {
                        call: form::System::Allocate,
                        args: vec![words[0]],
                    },
                    vec![form::Type::Address],
                )[0];
                let none = out.int(form::Bits::B64, 0);
                (ty, vec![address, none, words[0]])
            }
            "__sized" => {
                let (_, list) = self.expr(out, &call.args[0].value, env);
                (ty, vec![list[0], words[1], list[2]])
            }
            _ => {
                let Expr::Number { magnitude, .. } = &call.args[0].value else {
                    unreachable!("checked")
                };
                use form::Raw::*;
                let order = [
                    Socket,
                    Bind,
                    Listen,
                    Accept,
                    Connect,
                    GetSockName,
                    SetSockOpt,
                    SendTo,
                    RecvFrom,
                    Close,
                    ClockGetTime,
                    NanoSleep,
                    Kind,
                    EpollCreate1,
                    EpollCtl,
                    EpollWait,
                ];
                let raw = order[*magnitude as usize];
                let call = form::System::Raw(raw);
                let args = words[1..].to_vec();
                let given = out.emit(
                    form::Op::System { call, args },
                    vec![form::Type::Int(form::Bits::B64)],
                );
                (ty, given)
            }
        }
    }

    /// Lowers a call of a function the compiler builds for the prelude's
    /// operations on numbers.
    pub(super) fn intrinsic_call(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let typed = self.typed;
        let mut args = Vec::new();
        let mut int = None;
        let mut float = false;
        for arg in &call.args {
            let (ty, values) = self.expr(out, &arg.value, env);
            float |= matches!(ty, Ty::Float(_));
            if let (Ty::Int(t), None) = (ty, int) {
                int = Some(t);
            }
            args.push(values[0]);
        }
        if float && call.callee == "__exp" {
            let ty = typed.type_of(expr, env);
            let Ty::Float(size) = ty else {
                unreachable!("checked")
            };
            let double = float_bits(size) == form::FloatBits::F64;
            let at = match self.exp_table {
                Some(at) => at,
                None => {
                    self.data.push(form::exp_table());
                    let at = self.data.len() - 1;
                    self.exp_table = Some(at);
                    at
                }
            };
            let table = out.constant(form::Constant::Data(at), form::Type::Address);
            let values = self.call_runtime(
                out,
                Runtime::Exp { double },
                vec![args[0], table],
                vec![form::Type::Float(float_bits(size))],
            );
            return (ty, values);
        }
        if float && call.callee == "__ln" {
            let ty = typed.type_of(expr, env);
            let Ty::Float(size) = ty else {
                unreachable!("checked")
            };
            let double = float_bits(size) == form::FloatBits::F64;
            let values = self.call_runtime(
                out,
                Runtime::Ln { double },
                vec![args[0]],
                vec![form::Type::Float(float_bits(size))],
            );
            return (ty, values);
        }
        if float && (call.callee == "__sin" || call.callee == "__cos") {
            let ty = typed.type_of(expr, env);
            let Ty::Float(size) = ty else {
                unreachable!("checked")
            };
            let double = float_bits(size) == form::FloatBits::F64;
            let cosine = call.callee == "__cos";
            let values = self.call_runtime(
                out,
                Runtime::Trig { double, cosine },
                vec![args[0]],
                vec![form::Type::Float(float_bits(size))],
            );
            return (ty, values);
        }
        if float && call.callee == "__sqrt" {
            return (typed.type_of(expr, env), vec![out.float_sqrt(args[0])]);
        }
        if float && call.callee == "__float_text" {
            return (typed.type_of(expr, env), self.float_text(out, args[0]));
        }
        if float {
            let values = self.float_intrinsic(out, &call.callee, &args);
            return (typed.type_of(expr, env), values);
        }
        let int = int.expect("checked");
        let values = self.intrinsic(out, &call.callee, int, &args);
        (typed.type_of(expr, env), values)
    }

    /// Lowers the text of a float, the address and the length of the text.
    fn float_text(&mut self, out: &mut Out<'a>, value: form::Value) -> Vec<form::Value> {
        let form::Type::Float(size) = out.types[value.0] else {
            unreachable!("a float")
        };
        let double = size == form::FloatBits::F64;
        let (inverses, powers) = match self.float_tables {
            Some(at) => at,
            None => {
                let (inverses, powers) = form::ryu_tables();
                self.data.push(inverses);
                self.data.push(powers);
                let at = (self.data.len() - 2, self.data.len() - 1);
                self.float_tables = Some(at);
                at
            }
        };
        // The bits of the float are what it is as an integer of its size.
        let eight = out.int(form::Bits::B64, 8);
        let scratch = out.emit(
            form::Op::System {
                call: form::System::Allocate,
                args: vec![eight],
            },
            vec![form::Type::Address],
        )[0];
        out.emit(
            form::Op::Store {
                ty: form::Type::Float(size),
                address: scratch,
                value,
            },
            Vec::new(),
        );
        let bits = if double {
            out.emit(
                form::Op::Load {
                    ty: form::Type::Int(form::Bits::B64),
                    address: scratch,
                },
                vec![form::Type::Int(form::Bits::B64)],
            )[0]
        } else {
            let narrow = out.emit(
                form::Op::Load {
                    ty: form::Type::Int(form::Bits::B32),
                    address: scratch,
                },
                vec![form::Type::Int(form::Bits::B32)],
            )[0];
            out.widen(false, narrow)
        };
        let inverses = out.constant(form::Constant::Data(inverses), form::Type::Address);
        let powers = out.constant(form::Constant::Data(powers), form::Type::Address);
        self.call_runtime(
            out,
            Runtime::FloatText { double },
            vec![bits, inverses, powers],
            vec![form::Type::Address, form::Type::Int(form::Bits::B64)],
        )
    }

    /// What the function the compiler builds for `==` or `compare` of floats
    /// gives for the two floats `args`.
    fn float_intrinsic(
        &mut self,
        out: &mut Out<'a>,
        name: &str,
        args: &[form::Value],
    ) -> Vec<form::Value> {
        let defs = self.typed.defs;
        let (a, b) = (args[0], args[1]);
        match name {
            "__equal" => vec![out.float_equal(a, b)],
            "__compare" => {
                let case = |out: &mut Out<'a>, name: &str| {
                    let k = defs.case_of(defs.order, name).expect("a case");
                    out.int(form::Bits::B8, k as i128)
                };
                let below = out.float_less(a, b);
                let same = out.float_equal(a, b);
                let (l, e, g) = (case(out, "less"), case(out, "equal"), case(out, "greater"));
                let after = out.choose(same, e, g);
                vec![out.choose(below, l, after)]
            }
            _ => unreachable!("checked"),
        }
    }

    /// Which function of `Bits` with two numbers `call` is a call of.
    pub(super) fn bits_function(&self, call: &Call) -> Option<BitFn> {
        bits_function(self.typed.defs, call)
    }

    /// Lowers a call of `bit_and`, `bit_or`, `bit_xor`, `shift_left`, or
    /// `shift_right`: the numbers are at the size the work is done at, which
    /// the checker made them, and the result is taken to the size its range is
    /// kept at.
    pub(super) fn bits_call(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        op: BitFn,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        use form::Binary::*;
        let typed = self.typed;
        let (Ty::Int(a), a_values) = self.expr(out, &call.args[0].value, env) else {
            unreachable!("a whole number")
        };
        let (Ty::Int(b), b_values) = self.expr(out, &call.args[1].value, env) else {
            unreachable!("a whole number")
        };
        let ty = typed.type_of(expr, env);
        let Ty::Int(r) = ty else {
            unreachable!("a whole number")
        };
        let (x, y) = (a_values[0], b_values[0]);
        let done = match op {
            BitFn::And => out.binary(And, x, y),
            BitFn::Or => out.binary(Or, x, y),
            BitFn::Xor => out.binary(Xor, x, y),
            BitFn::Left | BitFn::Right => {
                let how = match (op, a.signed()) {
                    (BitFn::Left, _) => ShiftLeft,
                    (_, true) => ShiftRightS,
                    (_, false) => ShiftRightU,
                };
                // The amount is a `Byte`, whatever size its range is kept at.
                let by = if bits_of(b) == form::Bits::B8 {
                    y
                } else {
                    out.resize(b.signed(), y, form::Bits::B8)
                };
                out.binary(how, x, by)
            }
        };
        let value = if bits_of(a) == bits_of(r) {
            done
        } else {
            out.resize(a.signed(), done, bits_of(r))
        };
        (ty, vec![value])
    }

    /// Lowers a call of a function that only calls the function the compiler
    /// builds for `kind` and `op` on its parameters, in order, at the call: a
    /// `wrapping_`, `saturating_`, or `checked_` `add`, `sub`, or `mul`. No
    /// call, frame, or return is left, whatever the function's size.
    /// `places` says which parameter each argument is for.
    pub(super) fn number_call(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        (kind, op): (&str, &str),
        places: &[usize],
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let mut args = [None; 2];
        let mut int = None;
        for (arg, at) in call.args.iter().zip(places) {
            let (ty, values) = self.expr(out, &arg.value, env);
            if let (Ty::Int(t), None) = (ty, int) {
                int = Some(t);
            }
            args[*at] = Some(values[0]);
        }
        let (a, b) = (args[0].expect("checked"), args[1].expect("checked"));
        let values = self.arithmetic(out, kind, op, int.expect("checked"), a, b);
        (self.typed.type_of(expr, env), values)
    }

    /// What the function the compiler builds for an operation on numbers of
    /// type `t` gives for the values `args`: whether they are equal, how
    /// they are ordered, or the result of `wrapping_`, `saturating_`, or
    /// `checked_` `add`, `sub`, or `mul`.
    pub(super) fn intrinsic(
        &mut self,
        out: &mut Out<'a>,
        name: &str,
        t: Int,
        args: &[form::Value],
    ) -> Vec<form::Value> {
        let defs = self.typed.defs;
        let (a, b) = (args[0], args.get(1).copied());
        let case = |out: &mut Out<'a>, e: usize, name: &str| {
            let k = defs.case_of(e, name).expect("a case");
            out.constant(
                form::Constant::Int(k as u64),
                form::Type::Int(form::Bits::B8),
            )
        };
        match name {
            "__equal" => vec![out.compare(form::Compare::Equal, a, b.expect("two"))],
            "__compare" => {
                let b = b.expect("two");
                let less = if t.signed() {
                    form::Compare::LessS
                } else {
                    form::Compare::LessU
                };
                let below = out.compare(less, a, b);
                let same = out.compare(form::Compare::Equal, a, b);
                let (l, e, g) = (
                    case(out, defs.order, "less"),
                    case(out, defs.order, "equal"),
                    case(out, defs.order, "greater"),
                );
                let after = out.choose(same, e, g);
                vec![out.choose(below, l, after)]
            }
            "__bit_and" => vec![out.binary(form::Binary::And, a, b.expect("two"))],
            "__bit_or" => vec![out.binary(form::Binary::Or, a, b.expect("two"))],
            "__bit_xor" => vec![out.binary(form::Binary::Xor, a, b.expect("two"))],
            "__bit_not" => {
                let all = out.int(bits_of(t), -1);
                vec![out.binary(form::Binary::Xor, a, all)]
            }
            "__shift_left" | "__shift_right" => {
                let by = b.expect("two");
                let op = match (name, t.signed()) {
                    ("__shift_left", _) => form::Binary::ShiftLeft,
                    (_, true) => form::Binary::ShiftRightS,
                    (_, false) => form::Binary::ShiftRightU,
                };
                vec![out.binary(op, a, by)]
            }
            // Whether the answer fits, which the checked operation says.
            "__fits_add" | "__fits_sub" | "__fits_mul" => {
                let op = &name["__fits_".len()..];
                let answer = self.arithmetic(out, "checked", op, t, a, b.expect("two"));
                let some = defs.case_of(defs.option, "some").expect("a case");
                let some = out.constant(
                    form::Constant::Int(some as u64),
                    form::Type::Int(form::Bits::B8),
                );
                vec![out.compare(form::Compare::Equal, answer[0], some)]
            }
            "__decimal" => {
                let kind = Runtime::Decimal {
                    bits: bits_of(t),
                    signed: t.signed(),
                };
                self.call_runtime(out, kind, vec![a], TEXT.to_vec())
            }
            _ => {
                let (kind, op) = name[2..].split_once('_').expect("a named operation");
                self.arithmetic(out, kind, op, t, a, b.expect("two"))
            }
        }
    }

    /// The values of `a` and `b`, numbers of type `t`, put through an
    /// operation: `add`, `sub`, or `mul`. `wrapping` gives the result with
    /// the bits that do not fit left off, `saturating` gives the nearest
    /// value that fits, and `checked` gives an `Option` that holds the
    /// result if it fits.
    ///
    /// The operation says whether its answer fitted, except for an addition or
    /// a subtraction of an unsigned number of fewer than 64 bits, which a
    /// comparison says.
    pub(super) fn arithmetic(
        &mut self,
        out: &mut Out<'a>,
        kind: &str,
        op: &str,
        t: Int,
        a: form::Value,
        b: form::Value,
    ) -> Vec<form::Value> {
        use form::Binary::*;
        use form::Compare::*;
        let bits = bits_of(t);
        let word = match op {
            "add" => Add,
            "sub" => Sub,
            "shift_left" => ShiftLeft,
            _ => Mul,
        };
        if kind == "wrapping" && matches!(op, "div" | "rem") {
            // The divisor is not zero, which the call proved. The least number
            // divided by -1 is the one quotient that does not fit, and what a
            // processor cannot do: it is divided by 1 instead, and the choice
            // gives the number negated, which is the least number again, or,
            // for what is left over, 0.
            let (div, rem) = if t.signed() {
                (DivS, RemS)
            } else {
                (DivU, RemU)
            };
            let how = if op == "div" { div } else { rem };
            if !t.signed() {
                return vec![out.binary(how, a, b)];
            }
            let minus = out.int(bits, -1);
            let one = out.int(bits, 1);
            let is_minus = out.compare(Equal, b, minus);
            let by = out.choose(is_minus, one, b);
            let done = out.binary(how, a, by);
            if op == "rem" {
                return vec![done];
            }
            let zero = out.int(bits, 0);
            let negated = out.binary(Sub, zero, a);
            return vec![out.choose(is_minus, negated, done)];
        }
        if kind == "wrapping" {
            return vec![out.binary(word, a, b)];
        }
        if !t.signed() && op != "mul" && t.bits() < 64 {
            // The sum wraps and is below an operand exactly when it did not
            // fit, and a difference does not fit exactly when the operand
            // taken is the greater: one operation, one comparison, and one
            // choice, which is shorter than widening the numbers to see
            // whether they fit. The second is the operand that is a number
            // written out more often, which a compare holds in itself.
            if kind == "saturating" && op == "sub" {
                // The greater of the two, less the second: no number below
                // 0 is made, so no comparison of the difference is needed.
                let below = out.compare(LessU, a, b);
                let greater = out.choose(below, b, a);
                return vec![out.binary(Sub, greater, b)];
            }
            let end = out.int(bits, if op == "add" { t.max() } else { 0 });
            let r = out.binary(word, a, b);
            let wrong = if op == "add" {
                out.compare(LessU, r, b)
            } else {
                out.compare(LessU, a, b)
            };
            if kind == "saturating" {
                return vec![out.choose(wrong, end, r)];
            }
            return self.optional(out, bits, wrong, r);
        }
        // The operation says whether its answer fitted, and a target makes
        // that the flag of the operation. The end that a saturating function
        // stops at is worked out first, from the operands, so that nothing
        // that changes the flags comes between the operation and the choice.
        let end = (kind == "saturating").then(|| {
            if !t.signed() {
                return out.int(bits, if op == "sub" { 0 } else { t.max() });
            }
            // The end is the greatest value when the answer went above it,
            // and the least when it went below, which the signs say: an
            // addition goes below when the second is negative, a subtraction
            // when it is not, and a product when the signs differ.
            let sign = if op == "mul" {
                out.binary(Xor, a, b)
            } else {
                b
            };
            let by = out.int(bits, i128::from(t.bits() - 1));
            let all_or_none = out.binary(ShiftRightS, sign, by);
            let limit = out.int(bits, if op == "sub" { t.min() } else { t.max() });
            out.binary(Xor, all_or_none, limit)
        });
        let (r, overflowed) = out.overflows(word, t.signed(), a, b);
        match end {
            Some(end) => vec![out.choose(overflowed, end, r)],
            None => self.optional(out, bits, overflowed, r),
        }
    }

    /// The `Option` that holds `r`, a number of `bits`, unless `wrong`, a
    /// `bool`, says it did not fit: its tag, then its number.
    fn optional(
        &mut self,
        out: &mut Out<'a>,
        bits: form::Bits,
        wrong: form::Value,
        r: form::Value,
    ) -> Vec<form::Value> {
        let defs = self.typed.defs;
        let some = defs.case_of(defs.option, "some").expect("a case");
        let none = defs.case_of(defs.option, "none").expect("a case");
        let zero = out.int(bits, 0);
        // `wrong` is 1 or 0, which is the case when `.none` is 1 and `.some` is 0.
        let tag = if (none, some) == (1, 0) {
            wrong
        } else {
            let some = out.int(form::Bits::B8, some as i128);
            let none = out.int(form::Bits::B8, none as i128);
            out.choose(wrong, none, some)
        };
        vec![tag, out.choose(wrong, zero, r)]
    }

    /// The value `a`, a whole number of type `from`, taken to type `to`,
    /// which does not hold all of its values: as `wrapping_narrow`
    /// does, which keeps the bits, as `clamp` does, which gives the
    /// nearest value that fits, and as `narrow` does, which gives an
    /// `Option` that holds the value if it fits. A narrowing to a smaller type
    /// is one instruction, and to a type of the same size none.
    pub(super) fn narrowing(
        &mut self,
        out: &mut Out<'a>,
        narrowing: Narrowing,
        (from, to): (Int, Int),
        values: &[form::Value],
    ) -> Vec<form::Value> {
        use form::Compare::*;
        let defs = self.typed.defs;
        if from.wide() {
            return self.narrowing_wide(out, narrowing, (from, to), values);
        }
        let a = values[0];
        let (wide, narrow) = (bits_of(from), bits_of(to));
        let r = out.resize(from.signed(), a, narrow);
        if narrowing == Narrowing::Wrapping {
            return vec![r];
        }
        let less = if from.signed() { LessS } else { LessU };
        let no = out.int(form::Bits::B8, 0);
        let below = if from.min() < to.min() {
            let least = out.int(wide, to.min());
            out.compare(less, a, least)
        } else {
            no
        };
        let above = if from.max() > to.max() {
            let greatest = out.int(wide, to.max());
            out.compare(less, greatest, a)
        } else {
            no
        };
        if narrowing == Narrowing::Saturating {
            let (least, greatest) = (out.int(narrow, to.min()), out.int(narrow, to.max()));
            let low = out.choose(below, least, r);
            return vec![out.choose(above, greatest, low)];
        }
        let one = out.int(form::Bits::B8, 1);
        let wrong = out.choose(below, one, above);
        let some = defs.case_of(defs.option, "some").expect("a case");
        let none = defs.case_of(defs.option, "none").expect("a case");
        let some = out.int(form::Bits::B8, some as i128);
        let none = out.int(form::Bits::B8, none as i128);
        let zero = out.int(narrow, 0);
        vec![out.choose(wrong, none, some), out.choose(wrong, zero, r)]
    }

    /// `narrowing` of a number that is two words, to a range of 64 bits or
    /// fewer, or to one that is two words too: the number is compared with the
    /// least and the greatest of the range as two words, and the low word is
    /// the number where it fits a range that is one word.
    fn narrowing_wide(
        &mut self,
        out: &mut Out<'a>,
        narrowing: Narrowing,
        (from, to): (Int, Int),
        a: &[form::Value],
    ) -> Vec<form::Value> {
        let defs = self.typed.defs;
        let words = |out: &mut Out<'a>, n: i128| {
            vec![
                out.int(form::Bits::B64, n),
                out.int(form::Bits::B64, n >> 64),
            ]
        };
        let keep = |out: &mut Out<'a>, words: &[form::Value]| {
            if to.wide() {
                words.to_vec()
            } else {
                vec![out.resize(from.signed(), words[0], bits_of(to))]
            }
        };
        let r = keep(out, a);
        if narrowing == Narrowing::Wrapping {
            return r;
        }
        let no = out.int(form::Bits::B8, 0);
        let below = if from.min() < to.min() {
            let least = words(out, to.min());
            self.wide_less(out, (from.signed(), to.min() < 0), a, &least)
        } else {
            no
        };
        let above = if from.max() > to.max() {
            let greatest = words(out, to.max());
            self.wide_less(out, (to.max() < 0, from.signed()), &greatest, a)
        } else {
            no
        };
        if narrowing == Narrowing::Saturating {
            let (least, greatest) = (words(out, to.min()), words(out, to.max()));
            let (least, greatest) = (keep(out, &least), keep(out, &greatest));
            return r
                .iter()
                .enumerate()
                .map(|(i, word)| {
                    let low = out.choose(below, least[i], *word);
                    out.choose(above, greatest[i], low)
                })
                .collect();
        }
        let one = out.int(form::Bits::B8, 1);
        let wrong = out.choose(below, one, above);
        let some = defs.case_of(defs.option, "some").expect("a case");
        let none = defs.case_of(defs.option, "none").expect("a case");
        let some = out.int(form::Bits::B8, some as i128);
        let none = out.int(form::Bits::B8, none as i128);
        let mut values = vec![out.choose(wrong, none, some)];
        for word in r {
            let zero = out.constant(form::Constant::Int(0), out.types[word.0]);
            values.push(out.choose(wrong, zero, word));
        }
        values
    }
}

/// For a function whose whole body, in the prelude or the library, is a call of the function the compiler builds for `wrapping_`,
/// `saturating_`, or `checked_` `add`, `sub`, or `mul`, which kind it is and
/// which operation.
pub(super) fn number_function(f: &Function) -> Option<(&'static str, &'static str)> {
    let (kind, op) = f.name.split_once('_')?;
    let kind = ["wrapping", "saturating", "checked"]
        .into_iter()
        .find(|k| *k == kind)?;
    let op = ["add", "sub", "mul", "shift_left", "div", "rem"]
        .into_iter()
        .find(|o| {
            *o == op && (!matches!(*o, "shift_left" | "div" | "rem") || kind == "wrapping")
        })?;
    (f.body.len() == 1 && f.params.len() == 2 && (of_prelude(f.pos) || of_std(f.pos)))
        .then_some((kind, op))
}

/// Two ranges that no type of 64 bits holds: one has negative numbers and is
/// kept as an `i64`, and the other has numbers from 2^63 up and is kept as a
/// `u64`.
#[derive(Clone, Copy)]
pub(super) struct Mixed {
    /// Whether the first of the two is the one with negative numbers.
    first_signed: bool,
}

impl Mixed {
    /// Whether the two types are such ranges.
    pub(super) fn of(first: &Ty, second: &Ty) -> Option<Mixed> {
        let (Ty::Int(a), Ty::Int(b)) = (first, second) else {
            return None;
        };
        if a == b || a.hull(*b).is_some() {
            return None;
        }
        Some(Mixed {
            first_signed: a.signed(),
        })
    }

    /// Whether the numbers are equal, and whether the first is below the
    /// second, as bytes that are 0 or 1.
    fn stand<'a>(
        self,
        out: &mut Out<'a>,
        a: form::Value,
        b: form::Value,
    ) -> (form::Value, form::Value) {
        let (signed, unsigned) = if self.first_signed { (a, b) } else { (b, a) };
        let signed = out.resize(true, signed, form::Bits::B64);
        let unsigned = out.resize(false, unsigned, form::Bits::B64);
        let zero = out.int(form::Bits::B64, 0);
        let one = out.int(form::Bits::B8, 1);
        let negative = out.compare(form::Compare::LessS, signed, zero);
        let not_negative = out.binary(form::Binary::Xor, negative, one);
        let same_bits = out.compare(form::Compare::Equal, signed, unsigned);
        let equal = out.binary(form::Binary::And, not_negative, same_bits);
        // The signed number is below the unsigned one when it is negative, and
        // otherwise as the bits say; the unsigned one is below it when the
        // signed one is not negative and the bits say so.
        if self.first_signed {
            let below_bits = out.compare(form::Compare::LessU, signed, unsigned);
            (equal, out.binary(form::Binary::Or, negative, below_bits))
        } else {
            let below_bits = out.compare(form::Compare::LessU, unsigned, signed);
            (
                equal,
                out.binary(form::Binary::And, not_negative, below_bits),
            )
        }
    }
}

impl<'a> Lowering<'a> {
    /// The two words of a number of the range `from`, which is of 64 bits or
    /// fewer or is already two words, as a number of a range of more than 64
    /// bits: the low word is the number at 64 bits, and the high word is its
    /// sign, which is all ones for a negative number and none otherwise.
    pub(super) fn wide_from(
        &mut self,
        out: &mut Out<'a>,
        from: Int,
        values: &[form::Value],
    ) -> Vec<form::Value> {
        if from.wide() {
            return values.to_vec();
        }
        let low = out.resize(from.signed(), values[0], form::Bits::B64);
        let high = if from.signed() {
            let by = out.int(form::Bits::B64, 63);
            out.binary(form::Binary::ShiftRightS, low, by)
        } else {
            out.int(form::Bits::B64, 0)
        };
        vec![low, high]
    }

    /// Lowers `left + right`, `left - right`, or `left * right` of numbers that
    /// are two words each, which the checker made the sides of the result's size.
    fn wide_operation(
        &mut self,
        out: &mut Out<'a>,
        (op, right, not_negative, halves): (ArithOp, &'a Expr, bool, Option<form::Binary>),
        (ty, taken, a, b): (Ty, Ty, Vec<form::Value>, Vec<form::Value>),
    ) -> (Ty, Vec<form::Value>) {
        let values = match op {
            ArithOp::Add => {
                let low = out.binary(form::Binary::Add, a[0], b[0]);
                // The sum is below an operand exactly when it carried.
                let carried = out.compare(form::Compare::LessU, low, a[0]);
                let carry = out.resize(false, carried, form::Bits::B64);
                let sum = out.binary(form::Binary::Add, a[1], b[1]);
                vec![low, out.binary(form::Binary::Add, sum, carry)]
            }
            ArithOp::Sub => {
                let low = out.binary(form::Binary::Sub, a[0], b[0]);
                // The low word of the first is below that of the second exactly
                // when the difference borrowed.
                let borrowed = out.compare(form::Compare::LessU, a[0], b[0]);
                let borrow = out.resize(false, borrowed, form::Bits::B64);
                let difference = out.binary(form::Binary::Sub, a[1], b[1]);
                vec![low, out.binary(form::Binary::Sub, difference, borrow)]
            }
            ArithOp::Mul if halves.is_some() => {
                let low = out.binary(form::Binary::Mul, a[0], b[0]);
                vec![low, out.binary(halves.expect("some"), a[0], b[0])]
            }
            ArithOp::Mul => {
                // The low words give both halves of their product, and each
                // high word times the other low word is above them.
                let low = out.binary(form::Binary::Mul, a[0], b[0]);
                let carried = out.binary(form::Binary::MulHighU, a[0], b[0]);
                let first = out.binary(form::Binary::Mul, a[0], b[1]);
                let second = out.binary(form::Binary::Mul, a[1], b[0]);
                let sum = out.binary(form::Binary::Add, carried, first);
                vec![low, out.binary(form::Binary::Add, sum, second)]
            }
            ArithOp::Div | ArithOp::Rem => {
                let Ty::Int(at) = &taken else {
                    unreachable!("checked")
                };
                let power = match right {
                    Expr::Number {
                        magnitude,
                        negative: false,
                        ..
                    } if magnitude.is_power_of_two() && *magnitude > 1 => Some(*magnitude),
                    _ => None,
                };
                let both = match power {
                    Some(power) if not_negative || !at.signed() => {
                        self.wide_by_power(out, op, power.trailing_zeros(), (a[0], a[1]))
                    }
                    _ => self.wide_division(out, (op, at.signed()), (a[0], a[1]), (b[0], b[1])),
                };
                // The result is kept at the size of its own range, which holds
                // it: the low word is the number where it is smaller.
                match &ty {
                    Ty::Int(result) if !result.wide() => {
                        vec![out.resize(result.signed(), both[0], bits_of(*result))]
                    }
                    _ => both,
                }
            }
        };
        (ty, values)
    }

    /// The two words of `a / 2^k` or `a % 2^k` of a number that is not negative,
    /// for `k` from 1 to 127: the halves of the number moved or kept, and no
    /// instruction more than that.
    fn wide_by_power(
        &mut self,
        out: &mut Out<'a>,
        op: ArithOp,
        k: u32,
        (low, high): (form::Value, form::Value),
    ) -> Vec<form::Value> {
        let word = |out: &mut Out<'a>, n: u128| out.int(form::Bits::B64, n as i128);
        let zero = word(out, 0);
        match (op, k) {
            (ArithOp::Div, 64) => vec![high, zero],
            (ArithOp::Div, k) if k > 64 => {
                let by = word(out, u128::from(k - 64));
                vec![out.binary(form::Binary::ShiftRightU, high, by), zero]
            }
            (ArithOp::Div, k) => {
                let by = word(out, u128::from(k));
                let back = word(out, u128::from(64 - k));
                let down = out.binary(form::Binary::ShiftRightU, low, by);
                let across = out.binary(form::Binary::ShiftLeft, high, back);
                vec![
                    out.binary(form::Binary::Or, down, across),
                    out.binary(form::Binary::ShiftRightU, high, by),
                ]
            }
            (_, 64) => vec![low, zero],
            (_, k) if k > 64 => {
                let mask = word(out, (1u128 << (k - 64)) - 1);
                vec![low, out.binary(form::Binary::And, high, mask)]
            }
            (_, k) => {
                let mask = word(out, (1u128 << k) - 1);
                vec![out.binary(form::Binary::And, low, mask), zero]
            }
        }
    }

    /// The two words of `a` taken from 0, the number negated.
    fn wide_negated(
        &mut self,
        out: &mut Out<'a>,
        (low, high): (form::Value, form::Value),
    ) -> (form::Value, form::Value) {
        let zero = out.int(form::Bits::B64, 0);
        let off = out.binary(form::Binary::Sub, zero, low);
        let borrowed = out.compare(form::Compare::LessU, zero, low);
        let borrow = out.resize(false, borrowed, form::Bits::B64);
        let off_high = out.binary(form::Binary::Sub, zero, high);
        (off, out.binary(form::Binary::Sub, off_high, borrow))
    }

    /// The two words of `a` negated where `on`, a byte, is not 0.
    fn wide_negated_if(
        &mut self,
        out: &mut Out<'a>,
        on: form::Value,
        a: (form::Value, form::Value),
    ) -> (form::Value, form::Value) {
        let (low, high) = self.wide_negated(out, a);
        (out.choose(on, low, a.0), out.choose(on, high, a.1))
    }

    /// The two words of `a / b` or `a % b` of numbers that are two words each and
    /// whose divisor is not 0, by the function of the runtime that divides
    /// numbers that are not negative: a signed quotient is that of the sizes,
    /// negative when the signs differ, and what is left over has the sign of the
    /// number divided.
    fn wide_division(
        &mut self,
        out: &mut Out<'a>,
        (op, signed): (ArithOp, bool),
        a: (form::Value, form::Value),
        b: (form::Value, form::Value),
    ) -> Vec<form::Value> {
        let (a_size, b_size, a_negative, b_negative) = if signed {
            let zero = out.int(form::Bits::B64, 0);
            let a_negative = out.compare(form::Compare::LessS, a.1, zero);
            let b_negative = out.compare(form::Compare::LessS, b.1, zero);
            (
                self.wide_negated_if(out, a_negative, a),
                self.wide_negated_if(out, b_negative, b),
                a_negative,
                b_negative,
            )
        } else {
            let no = out.int(form::Bits::B8, 0);
            (a, b, no, no)
        };
        let words = self.call_runtime(
            out,
            Runtime::DivideWide,
            vec![a_size.0, a_size.1, b_size.0, b_size.1],
            vec![form::Type::Int(form::Bits::B64); 4],
        );
        let (value, negative) = match op {
            ArithOp::Div => {
                let differ = out.binary(form::Binary::Xor, a_negative, b_negative);
                ((words[0], words[1]), differ)
            }
            _ => ((words[2], words[3]), a_negative),
        };
        let (low, high) = if signed {
            self.wide_negated_if(out, negative, value)
        } else {
            value
        };
        vec![low, high]
    }

    /// Whether `a` is below `b`, both two words, where each is a signed number
    /// when its flag says so and is not negative otherwise. A negative number
    /// is below one that is not, and two that are both negative or both not are
    /// ordered as the words are, since a negative number has its top bit set.
    pub(super) fn wide_less(
        &mut self,
        out: &mut Out<'a>,
        (a_signed, b_signed): (bool, bool),
        a: &[form::Value],
        b: &[form::Value],
    ) -> form::Value {
        let zero = out.int(form::Bits::B64, 0);
        let negative = |out: &mut Out<'a>, signed: bool, v: &[form::Value]| {
            if signed {
                out.compare(form::Compare::LessS, v[1], zero)
            } else {
                out.int(form::Bits::B8, 0)
            }
        };
        let (a_negative, b_negative) = (negative(out, a_signed, a), negative(out, b_signed, b));
        let differ = out.binary(form::Binary::Xor, a_negative, b_negative);
        let high_below = out.compare(form::Compare::LessU, a[1], b[1]);
        let high_same = out.compare(form::Compare::Equal, a[1], b[1]);
        let low_below = out.compare(form::Compare::LessU, a[0], b[0]);
        let low_decides = out.binary(form::Binary::And, high_same, low_below);
        let ordered = out.binary(form::Binary::Or, high_below, low_decides);
        out.choose(differ, a_negative, ordered)
    }

    /// Whether `a` and `b`, both two words, are the same number.
    pub(super) fn wide_equal(
        &mut self,
        out: &mut Out<'a>,
        a: &[form::Value],
        b: &[form::Value],
    ) -> form::Value {
        let low = out.compare(form::Compare::Equal, a[0], b[0]);
        let high = out.compare(form::Compare::Equal, a[1], b[1]);
        out.binary(form::Binary::And, low, high)
    }
}
