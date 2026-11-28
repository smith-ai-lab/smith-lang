//! Lowers `for i { e }` as it is built first, with no pass: one loop over the
//! index, from 0 to the size, that writes what the block gives on each round
//! to the element at the index, in bytes of the function's frame, which are
//! read as the array once the loop ends. No memory is asked for.

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
        let Ty::Array(element, size) = &ty else {
            unreachable!("checked")
        };
        // An array of no elements is made by no round.
        if *size == 0 {
            return (ty.clone(), Vec::new());
        }
        let kinds = self.scalars(&element[0]);
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
            size: *size,
        };
        self.round(out, &f.round, Some((&home, &element[0])), env);
        (ty.clone(), out.load_array(&home))
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
