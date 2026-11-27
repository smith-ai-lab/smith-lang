//! Lowers lists and text: a string literal, a string that holds expressions,
//! a list written out, `push`, and reading an element of a list.

use super::*;

/// What the runtime says of bytes: whether they are valid UTF-8, and where the
/// first character that is not begins.
const VALID: [form::Type; 2] = [BYTE, form::Type::Int(form::Bits::B64)];

impl<'a> Lowering<'a> {
    pub(super) fn text(&mut self, out: &mut Out<'a>, text: &[u8]) -> Vec<form::Value> {
        let at = match self.data.iter().position(|t| t == text) {
            Some(at) => at,
            None => {
                self.data.push(text.to_vec());
                self.data.len() - 1
            }
        };
        let address = out.constant(form::Constant::Data(at), form::Type::Address);
        let length = out.constant(
            form::Constant::Int(text.len() as u64),
            form::Type::Int(form::Bits::B64),
        );
        vec![address, length]
    }

    /// Lowers an argument for a parameter of the type: a text or a slice is a
    /// list whose room is its size, so that it is passed as one, with no call.
    pub(super) fn passed(
        &mut self,
        out: &mut Out<'a>,
        value: &'a Expr,
        param: &Ty,
        env: &[Ty],
    ) -> Vec<form::Value> {
        let (ty, mut values) = self.expr(out, value, env);
        if let Ty::Array(of, n) = &ty
            && as_list(&ty, param)
        {
            // An array is a list of its size once its elements are in the frame.
            let Ty::List(wanted) = param else {
                unreachable!("checked")
            };
            // The elements are given in the size the list's are, a range that is
            // wider than the array's holding the same numbers.
            if let (Ty::Int(from), Ty::Int(to)) = (&of[0], &wanted[0])
                && from != to
            {
                let bits = bits_of(*to);
                values = values
                    .into_iter()
                    .map(|v| out.resize(from.signed(), v, bits))
                    .collect();
            }
            let (frame, ..) = self.written_to_frame(out, &wanted[0], *n, &values);
            let size = out.int(form::Bits::B64, i128::from(*n));
            return vec![frame, size, size];
        }
        if as_list(&ty, param) {
            values.push(values[1]);
        }
        values
    }

    /// Lowers `utf8(bytes)`: the text is the address and the size the bytes
    /// have, read once, and the error holds the place the runtime found.
    pub(super) fn utf8(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let ty = self.typed.type_of(expr, env);
        let Ty::Enum(e, args) = &ty else {
            unreachable!("checked")
        };
        let bytes = self.expr(out, &call.args[0].value, env).1;
        let read = (bytes[0], bytes[1]);
        let seen = self.call_runtime(out, Runtime::Valid, vec![read.0, read.1], VALID.to_vec());
        let layout = self.layout(*e, args);
        let valid = out.int(form::Bits::B8, 1);
        let ok = out.compare(form::Compare::Equal, seen[0], valid);
        let case = out.int(form::Bits::B8, 1);
        let case = out.binary(form::Binary::Sub, case, seen[0]);
        // A place that both cases use holds the value of the case that is held.
        let mut ok_values: Vec<Option<form::Value>> = vec![None; layout.places.len()];
        let mut err_values = ok_values.clone();
        for (value, &place) in [read.0, read.1].iter().zip(&layout.cases[0]) {
            ok_values[place] = Some(*value);
        }
        err_values[layout.cases[1][0]] = Some(seen[1]);
        let mut places = Vec::new();
        for (k, ty) in layout.places.iter().enumerate() {
            let zero = out.zero(*ty);
            let then = ok_values[k].unwrap_or(zero);
            let otherwise = err_values[k].unwrap_or(zero);
            places.push(out.choose(ok, then, otherwise));
        }
        let mut values = vec![case];
        values.extend(places);
        (ty, values)
    }

    /// The types of the scalars of each element of a list of the type.
    pub(super) fn element(&self, list: &Ty) -> Vec<form::Type> {
        match list {
            Ty::List(of) | Ty::Slice(of) => self.scalars(&of[0]),
            Ty::Text => self.scalars(&Ty::Int(Int::U8)),
            _ => unreachable!("checked"),
        }
    }

    /// Lowers `freeze`, which gives the address and the size of the list and
    /// not its room, so that no instruction is made for it.
    pub(super) fn freeze(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let values = self.expr(out, &call.args[0].value, env).1;
        (self.typed.type_of(expr, env), values[..2].to_vec())
    }

    /// Lowers `of[a..b]`, `of[..b]`, or `of[a..]`: the address of element `a`
    /// and `b - a` elements, with nothing copied.
    pub(super) fn piece(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        (of, from, to): (&'a Expr, Option<&'a Expr>, Option<&'a Expr>),
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let (ty, values) = self.expr(out, of, env);
        let (_, stride) = form::element(&self.element(&ty));
        let (address, size) = (values[0], values[1]);
        let to = match to {
            Some(to) => self.expr(out, to, env).1[0],
            None => size,
        };
        let (start, count) = match from {
            Some(from) => {
                let from = self.expr(out, from, env).1[0];
                (Some(from), out.binary(form::Binary::Sub, to, from))
            }
            None => (None, to),
        };
        let address = match start {
            Some(from) => {
                let width = out.int(form::Bits::B64, i128::from(stride));
                let offset = out.binary(form::Binary::Mul, from, width);
                out.binary(form::Binary::Add, address, offset)
            }
            None => address,
        };
        (self.typed.type_of(expr, env), vec![address, count])
    }

    /// Lowers `push`, which gives the list with the item after its last
    /// element, and `append`, which gives it with the elements of another
    /// list there.
    pub(super) fn push(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let (ty, mut args) = self.expr(out, &call.args[0].value, env);
        let more = self.expr(out, &call.args[1].value, env).1;
        let results = self.scalars(&ty);
        let kind = match call.callee.ends_with("append") {
            true => {
                args.extend(&more[..2]);
                Runtime::Append(self.element(&ty))
            }
            false => {
                args.extend(more);
                Runtime::Push(self.element(&ty))
            }
        };
        let values = self.call_runtime(out, kind, args, results);
        (ty, values)
    }

    /// Whether `e`, an index or a field of one, reads inside an array.
    pub(super) fn over_array(&self, e: &Expr, env: &[Ty]) -> bool {
        match e {
            Expr::Index { of, .. } => matches!(self.typed.type_of(of, env), Ty::Array(..)),
            Expr::Field { of, .. } => self.over_array(of, env),
            _ => false,
        }
    }

    /// Lowers `of[index]` of an array, whose elements are its scalars one
    /// after another, so that an index known when the program is built picks
    /// them out and nothing is asked of memory. Any other index reads the
    /// element from the bytes of the function's frame that the array is written
    /// to.
    pub(super) fn array_element(
        &mut self,
        out: &mut Out<'a>,
        _expr: &'a Expr,
        (of, index): (&'a Expr, &'a Expr),
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        // An array that is kept in the frame is read at the element alone.
        if let Expr::Name { name, .. } = of
            && let Some(at) = out.find(name)
            && let Some(home) = out.names()[at].home.clone()
        {
            let Ty::Array(element, _) = out.names()[at].ty.clone() else {
                unreachable!("checked")
            };
            let address = match self.constant_index(index, env) {
                Some(at) => out.moved(home.frame, at as u64 * home.stride),
                None => self.element_address(out, home.frame, home.stride, index, env),
            };
            return (element[0].clone(), self.load(out, address, &home.scalars));
        }
        let (array, values) = self.expr(out, of, env);
        let Ty::Array(element, n) = array else {
            unreachable!("checked")
        };
        let size = self.size(&element[0]);
        if let Some(at) = self.constant_index(index, env) {
            let from = at * size;
            return (element[0].clone(), values[from..from + size].to_vec());
        }
        let (frame, scalars, stride) = self.written_to_frame(out, &element[0], n, &values);
        let address = self.element_address(out, frame, stride, index, env);
        (element[0].clone(), self.load(out, address, &scalars))
    }

    /// The number an index of an array is, when it is known where the program
    /// is built.
    pub(super) fn constant_index(&self, index: &Expr, env: &[Ty]) -> Option<usize> {
        match (index, self.typed.type_of(index, env)) {
            (
                Expr::Number {
                    magnitude,
                    negative: false,
                    ..
                },
                _,
            ) => Some(*magnitude as usize),
            (_, Ty::Int(t)) if t.min() == t.max() => Some(t.min() as usize),
            _ => None,
        }
    }

    /// Writes the `n` elements of an array, given as their scalars, to bytes of
    /// the function's frame laid out as the elements of a list are, and gives
    /// where they are, the places of the scalars of an element, and how many
    /// bytes an element takes.
    pub(super) fn written_to_frame(
        &mut self,
        out: &mut Out<'a>,
        element: &Ty,
        n: u64,
        values: &[form::Value],
    ) -> (form::Value, Vec<(form::Type, u64)>, u64) {
        let scalars = self.scalars(element);
        let (places, stride) = form::element(&scalars);
        let frame = out.emit(
            form::Op::Frame { bytes: n * stride },
            vec![form::Type::Address],
        )[0];
        let at = |out: &mut Out<'a>, offset: u64| match offset {
            0 => frame,
            _ => {
                let by = out.int(form::Bits::B64, i128::from(offset));
                out.binary(form::Binary::Add, frame, by)
            }
        };
        for (k, element) in values.chunks(scalars.len().max(1)).enumerate() {
            for (value, place) in element.iter().zip(&places) {
                let address = at(out, k as u64 * stride + place);
                out.emit(
                    form::Op::Store {
                        ty: out.types[value.0],
                        address,
                        value: *value,
                    },
                    Vec::new(),
                );
            }
        }
        (frame, scalars.into_iter().zip(places).collect(), stride)
    }

    /// The address of the element at `index`, in bytes laid out as a list's.
    pub(super) fn element_address(
        &mut self,
        out: &mut Out<'a>,
        frame: form::Value,
        stride: u64,
        index: &'a Expr,
        env: &[Ty],
    ) -> form::Value {
        let (at, number) = self.expr(out, index, env);
        let place = match at {
            Ty::Int(t) if bits_of(t) == form::Bits::B64 => number[0],
            _ => out.widen(false, number[0]),
        };
        let width = out.int(form::Bits::B64, i128::from(stride));
        let offset = out.binary(form::Binary::Mul, place, width);
        out.binary(form::Binary::Add, frame, offset)
    }

    /// Works out where the element or the part of one that `expr` reads is,
    /// without reading it: its type, the address of its first byte, and each
    /// of its scalars with its place from the address. `expr` is an index,
    /// or a field, or the `size`, of a part of one.
    pub(super) fn element_at(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        env: &[Ty],
    ) -> (Ty, form::Value, Vec<(form::Type, u64)>) {
        let defs = self.typed.defs;
        match expr {
            Expr::Index { of, index, .. } => {
                let (list, values) = self.expr(out, of, env);
                let (at, number) = self.expr(out, index, env);
                let scalars = match list {
                    Ty::Text => self.scalars(&Ty::Int(Int::U8)),
                    _ => self.element(&list),
                };
                let (places, stride) = form::element(&scalars);
                let place = match at {
                    Ty::Int(t) if bits_of(t) == form::Bits::B64 => number[0],
                    _ => out.widen(false, number[0]),
                };
                let width = out.int(form::Bits::B64, i128::from(stride));
                let offset = out.binary(form::Binary::Mul, place, width);
                let address = out.binary(form::Binary::Add, values[0], offset);
                let element = match list {
                    Ty::List(of) | Ty::Slice(of) => of[0].clone(),
                    _ => Ty::Int(Int::U8),
                };
                (element, address, scalars.into_iter().zip(places).collect())
            }
            Expr::Field { of, name, .. } => {
                let (ty, address, scalars) = self.element_at(out, of, env);
                let (start, size, ty) = match ty {
                    Ty::List(_) | Ty::Slice(_) | Ty::Text => (1, 1, Ty::Int(Int::U64)),
                    Ty::Struct(s, args) => {
                        let mut start = 0;
                        let mut found = None;
                        for (field, t) in defs.file.structs[s].fields.iter().zip(&defs.fields[s]) {
                            let t = t.subst(&args);
                            if &field.name == name {
                                found = Some(t);
                                break;
                            }
                            start += self.size(&t);
                        }
                        let t = found.expect("checked");
                        (start, self.size(&t), t)
                    }
                    _ => unreachable!("checked"),
                };
                (ty, address, scalars[start..start + size].to_vec())
            }
            _ => unreachable!("checked"),
        }
    }

    /// Reads the scalars at their places from the address.
    pub(super) fn load(
        &mut self,
        out: &mut Out<'a>,
        address: form::Value,
        scalars: &[(form::Type, u64)],
    ) -> Vec<form::Value> {
        scalars
            .iter()
            .map(|&(ty, place)| {
                let at = match place {
                    0 => address,
                    _ => {
                        let by = out.int(form::Bits::B64, i128::from(place));
                        out.binary(form::Binary::Add, address, by)
                    }
                };
                out.emit(form::Op::Load { ty, address: at }, vec![ty])[0]
            })
            .collect()
    }

    /// Lowers a string literal that holds expressions: the parts are joined
    /// into one text.
    pub(super) fn interpolation(
        &mut self,
        out: &mut Out<'a>,
        parts: &'a [Part],
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let count = parts.len();
        let mut args = Vec::new();
        for part in parts {
            args.extend(match part {
                Part::Text(text) => self.text(out, text),
                Part::Expr(e) => self.expr(out, e, env).1,
            });
        }
        let values = self.call_runtime(out, Runtime::Join(count), args, TEXT.to_vec());
        (Ty::Text, values)
    }

    /// Lowers a list written out, `[]` or `[a, b, c]`.
    /// The scalars of a value of type `from`, as the scalars of the type `to`
    /// that it is held as, a whole number being the width of its range where
    /// it is made and the width of the range of what holds it where it is kept.
    pub(super) fn held_as(
        &mut self,
        out: &mut Out<'a>,
        from: &Ty,
        to: &Ty,
        values: Vec<form::Value>,
    ) -> Vec<form::Value> {
        let signed = matches!(from, Ty::Int(t) if t.min() < 0);
        let wanted = self.scalars(to);
        (values.into_iter().zip(wanted))
            .map(|(value, want)| match (out.types[value.0], want) {
                (form::Type::Int(have), form::Type::Int(bits)) if have != bits => {
                    out.resize(signed, value, bits)
                }
                _ => value,
            })
            .collect()
    }

    pub(super) fn list_literal(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        items: &'a [Expr],
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let ty = self.typed.type_of(expr, env);
        if let Ty::Array(element, _) = &ty {
            // An item is typed by what it is written as, and the array by the
            // range of all its items, so an item is held as an element is.
            let mut values = Vec::new();
            for item in items {
                let (item_ty, held) = self.expr(out, item, env);
                values.extend(self.held_as(out, &item_ty, &element[0], held));
            }
            return (ty, values);
        }
        let scalars = self.element(&ty);
        let (places, stride) = form::element(&scalars);
        if matches!(ty, Ty::Slice(_)) {
            return self.slice_literal(out, ty, items, &scalars);
        }
        let elements: Vec<Vec<form::Value>> = items
            .iter()
            .map(|item| self.expr(out, item, env).1)
            .collect();
        let count = out.int(form::Bits::B64, items.len() as i128);
        if items.is_empty() {
            let none = out.constant(form::Constant::Null, form::Type::Address);
            return (ty, vec![none, count, count]);
        }
        let bytes = out.int(form::Bits::B64, items.len() as i128 * stride as i128);
        let start = out.emit(
            form::Op::System {
                call: form::System::Allocate,
                args: vec![bytes],
            },
            vec![form::Type::Address],
        )[0];
        for (k, element) in elements.into_iter().enumerate() {
            for (value, place) in element.into_iter().zip(&places) {
                let offset = out.int(form::Bits::B64, (k as u64 * stride + place) as i128);
                let address = out.binary(form::Binary::Add, start, offset);
                out.emit(
                    form::Op::Store {
                        ty: out.types[value.0],
                        address,
                        value,
                    },
                    Vec::new(),
                );
            }
        }
        (ty, vec![start, count, count])
    }
}

impl<'a> Lowering<'a> {
    /// Lowers a slice written out: its elements, which the checker holds to
    /// be whole numbers written out, are bytes of the program's data, and no
    /// memory is asked for.
    fn slice_literal(
        &mut self,
        out: &mut Out<'a>,
        ty: Ty,
        items: &'a [Expr],
        scalars: &[form::Type],
    ) -> (Ty, Vec<form::Value>) {
        let width = scalars[0].bytes() as usize;
        let mut bytes = Vec::with_capacity(items.len() * width);
        for item in items {
            let Expr::Number {
                magnitude,
                negative,
                ..
            } = item
            else {
                unreachable!("checked")
            };
            let magnitude = *magnitude as i128;
            let number = if *negative { -magnitude } else { magnitude };
            bytes.extend_from_slice(&number.to_le_bytes()[..width]);
        }
        let count = out.int(form::Bits::B64, items.len() as i128);
        if items.is_empty() {
            let none = out.constant(form::Constant::Null, form::Type::Address);
            return (ty, vec![none, count]);
        }
        let address = self.text(out, &bytes)[0];
        (ty, vec![address, count])
    }
}

/// Whether the expression is an index, or a field, or the `size`, of a part of
/// one, so that only the scalars it reads need be read.
pub(super) fn indexed(e: &Expr) -> bool {
    match e {
        Expr::Index { .. } => true,
        Expr::Field { of, .. } => indexed(of),
        _ => false,
    }
}
