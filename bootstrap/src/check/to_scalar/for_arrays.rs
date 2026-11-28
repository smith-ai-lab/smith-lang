//! Lowers `for i { e }` and `for i, j { e }` as they are built first, with no
//! pass: one loop over the place of each element, from 0 to the number of
//! elements, that writes what the block gives on each round to the element
//! at the place, in bytes of the function's frame, which are read as the array
//! once the loop ends. The elements of a matrix are row after row, so each
//! index is worked out from the place, the last the one that changes first.
//! No memory is asked for.

use super::*;
use crate::parser::For;

impl<'a> Lowering<'a> {
    pub(super) fn for_expr(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        f: &'a For,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let ty = self.typed.type_of(expr, env);
        let (element, size) = ty.elements().expect("checked");
        // An array of no elements is made by no round.
        if size == 0 {
            return (ty.clone(), Vec::new());
        }
        let sizes = match &ty {
            Ty::Matrix(_, sizes) => sizes.clone(),
            _ => vec![size],
        };
        let indices: Vec<(&str, u64)> = (f.indices.iter())
            .zip(sizes)
            .map(|((name, _), n)| (name.as_str(), n))
            .collect();
        let kinds = self.scalars(element);
        let (places, stride) = form::element(&kinds);
        let frame = out.emit(
            form::Op::Frame {
                bytes: size * stride,
            },
            vec![form::Type::Address],
        )[0];
        let home = Home {
            frame,
            scalars: kinds.into_iter().zip(places).collect(),
            stride,
            size,
        };
        self.round(out, &f.round, Some((&home, element, &indices)), env);
        (ty.clone(), out.load_array(&home))
    }

    /// Defines each index of a `for` on the round whose counter is the place
    /// `counter`, of the type `t`: the index of the last axis is what is left
    /// of the place over its size, and each one before it is what is left
    /// over its own size of the place divided by the sizes after it.
    pub(super) fn define_indices(
        &mut self,
        out: &mut Out<'a>,
        indices: &[(&str, u64)],
        (counter, t): (form::Value, Int),
    ) {
        let bits = bits_of(t);
        let mut after = 1;
        let mut defined = Vec::new();
        for (k, &(name, size)) in indices.iter().enumerate().rev() {
            let mut at = counter;
            if after > 1 {
                let by = out.int(bits, i128::from(after));
                at = out.binary(form::Binary::DivU, at, by);
            }
            if k > 0 {
                let by = out.int(bits, i128::from(size));
                at = out.binary(form::Binary::RemU, at, by);
            }
            after *= size;
            let index = Ty::Int(Int::between(0, i128::from(size) - 1).expect("a size"));
            let at = self.held_as(out, &Ty::Int(t), &index, vec![at]);
            defined.push((name, index, at));
        }
        for (name, index, at) in defined.into_iter().rev() {
            out.define(name, index, at);
        }
    }

    /// Writes what the block of the `loop` of a `for` gave on a round, of the
    /// type its last expression has, to the element of the array at the
    /// counter, held as an element is.
    pub(super) fn keep_round(
        &mut self,
        out: &mut Out<'a>,
        l: &'a Loop,
        (home, element): (&Home, &Ty),
        counter: form::Value,
        gives: Vec<form::Value>,
        env: &[Ty],
    ) {
        let Some(Stmt::Expr(last)) = l.body.last() else {
            return;
        };
        let given = self.typed.type_of(last, env);
        let held = self.held_as(out, &given, element, gives);
        let place = out.resize(false, counter, form::Bits::B64);
        let width = out.int(form::Bits::B64, i128::from(home.stride));
        let offset = out.binary(form::Binary::Mul, place, width);
        let address = out.binary(form::Binary::Add, home.frame, offset);
        for (value, (ty, at)) in held.into_iter().zip(&home.scalars) {
            let address = out.moved(address, *at);
            out.emit(
                form::Op::Store {
                    ty: *ty,
                    address,
                    value,
                },
                Vec::new(),
            );
        }
    }
}
