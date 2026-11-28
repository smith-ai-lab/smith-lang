//! Checks lists and text: `push`, a list written out, an index, and a string
//! that holds expressions.

use super::*;

/// Whether a call is of `push` or of `append`, the two functions that give a
/// list more elements, which the compiler builds, and no function of the
/// program takes the name of. The library's own `Text` calls `__append`.
pub(super) fn is_push(defs: &Defs, call: &Call) -> bool {
    let name = call.callee.as_str();
    call.qualifier.is_none()
        && match name {
            "push" | "append" => {
                defs.function(call.name_pos, name).is_none()
                    && defs.struct_at(call.name_pos, name).is_none()
            }
            _ => is_library_append(call),
        }
}

/// Whether a call is of `__append`, which the library calls where it adds
/// the elements of a list to another.
pub(super) fn is_library_append(call: &Call) -> bool {
    call.qualifier.is_none() && call.callee == "__append" && of_std(call.pos)
}

/// Whether a call is of `__finish`, which `Text` of the library calls where
/// the bytes it holds become a text, with nothing read again.
fn is_library_finish(call: &Call) -> bool {
    call.qualifier.is_none() && call.callee == "__finish" && of_std(call.pos)
}

/// Whether a call is of a function that only the library calls, and that a
/// function of this file builds, rather than an intrinsic of numbers.
pub(super) fn is_library_call(call: &Call) -> bool {
    is_library_append(call) || is_library_finish(call)
}

/// Whether a call is of `append`, as `push` is of `push`.
fn is_append(call: &Call) -> bool {
    call.callee == "append" || call.callee == "__append"
}

/// Whether a call lets go of the argument `arg`: both of `push` and the
/// list `append` adds to, which it takes, and not the one it adds from, which
/// it lends.
pub(super) fn gives_away(defs: &Defs, call: &Call, arg: &Arg) -> bool {
    if super::is_kernel(call) {
        return call.callee == "__consume"
            || (call.callee == "__sized" && std::ptr::eq(arg, &call.args[0]));
    }
    if super::frames::takes_frame(call) {
        return true;
    }
    match is_push(defs, call) {
        true => !is_append(call) || std::ptr::eq(arg, &call.args[0]),
        false => is_freeze(defs, call),
    }
}

/// Whether a call is of `utf8`, which the compiler builds, and no function
/// of the program takes the name of.
pub(super) fn is_utf8(defs: &Defs, call: &Call) -> bool {
    call.qualifier.is_none()
        && call.callee == "utf8"
        && !defs.function_names.contains_key("utf8")
        && defs.struct_named("utf8").is_none()
}

/// Whether a call is of `boundary`, the fact that a place falls between two
/// characters of a text, which the compiler builds, and no function of the
/// program takes the name of.
pub(super) fn is_boundary(defs: &Defs, call: &Call) -> bool {
    call.qualifier.is_none()
        && call.callee == "boundary"
        && !defs.function_names.contains_key("boundary")
        && defs.struct_named("boundary").is_none()
}

/// Whether a text, a slice, or an array is given where a list is needed, which
/// it is used as, with no call and no copy of its elements.
pub(super) fn as_list(given: &Ty, needed: &Ty) -> bool {
    match (given, needed) {
        (Ty::Text, _) => *needed == Ty::list(Ty::Int(Int::U8)),
        (Ty::Slice(of), Ty::List(wanted)) => of == wanted,
        // An array's elements are given to the list in the size they are wanted
        // in, so a range is taken where a wider one is, as it is anywhere.
        (Ty::Array(of, _), Ty::List(wanted)) => match (&of[0], &wanted[0]) {
            (Ty::Int(given), Ty::Int(needed)) => needed.holds(*given),
            (given, needed) => given == needed,
        },
        _ => false,
    }
}

/// The type a value of the type `given` has where a `pattern` is needed: a
/// text where a list is needed is the list of bytes it is used as, and a
/// slice is the list of its elements.
pub(super) fn as_read(given: &Ty, pattern: &Ty) -> Ty {
    match (given, pattern) {
        (Ty::Text, Ty::List(_)) => Ty::list(Ty::Int(Int::U8)),
        // An array of a range is the list of a wider one that the call needs.
        (Ty::Array(of, _), Ty::List(wanted)) => match (&of[0], &wanted[0]) {
            (Ty::Int(given), Ty::Int(needed)) if needed.holds(*given) => pattern.clone(),
            _ => Ty::list(of[0].clone()),
        },
        (Ty::Slice(of), Ty::List(_)) => Ty::list(of[0].clone()),
        _ => given.clone(),
    }
}

/// Whether a call is of `freeze`, which the compiler builds, and no function
/// of the program takes the name of.
pub(super) fn is_freeze(defs: &Defs, call: &Call) -> bool {
    call.qualifier.is_none()
        && call.callee == "freeze"
        && !defs.function_names.contains_key("freeze")
        && defs.struct_named("freeze").is_none()
        || is_library_finish(call)
}

impl<'a> Checker<'a> {
    /// Whether a function may need `arith` to know that a place falls between
    /// characters: it takes a piece, writes `boundary`, or calls a function
    /// whose contract says it. Only such a function has the bytes of a text
    /// it reads as variables, which the other proofs need not carry.
    pub(super) fn needs_boundaries(&self, f: &Function) -> bool {
        let defs = self.defs;
        let says = |e: &Expr| {
            super::exprs(std::iter::once(e))
                .iter()
                .any(|e| matches!(e, Expr::Call(c) if is_boundary(defs, c)))
        };
        let contract = |g: &Function| g.requires.iter().chain(&g.ensures).any(says);
        contract(f)
            || super::exprs(super::values(&f.body))
                .iter()
                .any(|e| match e {
                    Expr::Slice { .. } => true,
                    Expr::Call(c) if is_boundary(defs, c) => true,
                    Expr::Call(c) if c.qualifier.is_none() => defs
                        .function(c.name_pos, &c.callee)
                        .is_some_and(|k| contract(&defs.file.functions[k])),
                    _ => false,
                })
    }

    /// Checks `utf8(bytes)`, which gives `.ok` of the text that is the same
    /// bytes, or `.err` with the place of the first byte that is not valid.
    pub(super) fn utf8(&mut self, body: &mut Body<'a>, call: &Call) -> Ty {
        let defs = self.defs;
        let plain = call.args.iter().all(|a| a.name.is_none() && !a.after);
        if call.args.len() != 1 || !plain {
            self.errors.push(Error::new(
                call.pos,
                "`utf8` takes the one slice of bytes to read",
            ));
            return self.unknown_call(body, call);
        }
        let bytes = Ty::Slice(vec![Ty::Int(Int::U8)]);
        let value = &call.args[0].value;
        let given = self.expr(body, value, &Want::Ty(bytes.clone()));
        if given != Ty::Text {
            self.expect(body, value, &given, &bytes);
        }
        let result = defs
            .enum_named("Result")
            .expect("the prelude defines `Result`");
        let error = defs
            .struct_named("Utf8Error")
            .expect("the prelude defines `Utf8Error`");
        Ty::Enum(result, vec![Ty::Text, Ty::Struct(error, Vec::new())])
    }

    /// Checks `boundary(s, i)`, the fact that the place `i` falls between two
    /// characters of the text `s`, which is written only where nothing is
    /// computed: in a `requires`, an `ensures`, or an `invariant`.
    pub(super) fn boundary(&mut self, body: &mut Body<'a>, call: &Call) -> Ty {
        let plain = call.args.iter().all(|a| a.name.is_none() && !a.after);
        if call.args.len() != 2 || !plain {
            self.errors.push(Error::new(
                call.pos,
                "`boundary` takes a text and a place in it",
            ));
            return self.unknown_call(body, call);
        }
        if !body.contract && !self.invariant {
            self.errors.push(
                Error::new(
                    call.pos,
                    "`boundary` is a fact for a `requires`, an `ensures`, or an `invariant`, and is never computed",
                )
                .with_fix(
                    "state it in a `requires`, or show it where the program runs by checking that a byte beside the place is ASCII",
                ),
            );
        }
        let text = &call.args[0].value;
        let given = self.expr(body, text, &Want::Ty(Ty::Text));
        self.expect(body, text, &given, &Ty::Text);
        let place = &call.args[1].value;
        let nat = Ty::Int(Int::U64);
        let given = self.expr(body, place, &Want::Ty(nat.clone()));
        self.expect(body, place, &given, &nat);
        self.defs.bool_ty()
    }

    /// Checks `freeze(list)`, which gives the memory of the list as a slice,
    /// with nothing copied, and `__finish(list)`, which gives the bytes of a
    /// list of bytes as a text, as the library's `Text` knows them valid.
    pub(super) fn freeze(&mut self, body: &mut Body<'a>, call: &Call, want: &Want) -> Ty {
        let defs = self.defs;
        let plain = call.args.iter().all(|a| a.name.is_none() && !a.after);
        if call.args.len() != 1 || !plain {
            self.errors.push(Error::new(
                call.pos,
                "`freeze` takes the one list to freeze",
            ));
            return self.unknown_call(body, call);
        }
        let list_want = match want {
            Want::Ty(Ty::Slice(of)) => Want::Ty(Ty::List(of.clone())),
            _ => Want::Any,
        };
        let finishes = is_library_finish(call);
        match self.expr(body, &call.args[0].value, &list_want) {
            Ty::List(of) if finishes && of[0] == Ty::Int(Int::U8) => Ty::Text,
            Ty::List(of) if !finishes => Ty::Slice(of),
            Ty::Unknown => Ty::Unknown,
            other => {
                let given = defs.describe(&other, body.scope);
                self.errors.push(Error::new(
                    call.args[0].value.pos(),
                    format!(
                        "`{}` takes a list, and this is {given}",
                        call.callee.trim_start_matches("__")
                    ),
                ));
                Ty::Unknown
            }
        }
    }

    /// Checks `of[a..b]`, `of[..b]`, or `of[a..]`, the part of a slice or a
    /// text from `a` up to `b`, which shares its memory, and compiles only
    /// when `arith` proves that `a <= b` and `b <= of.size`, and for a text
    /// that both ends fall between characters.
    pub(super) fn piece(
        &mut self,
        body: &mut Body<'a>,
        of: &Expr,
        ends: (Option<&Expr>, Option<&Expr>),
        pos: Pos,
    ) -> Ty {
        let defs = self.defs;
        let slice = self.expr(body, of, &Want::Any);
        for end in [ends.0, ends.1].into_iter().flatten() {
            let wanted = match flexible(end) {
                true => Want::Ty(Ty::Int(Int::U64)),
                false => Want::Any,
            };
            match self.expr(body, end, &wanted) {
                Ty::Int(Int::U64) | Ty::Unknown => {}
                Ty::Int(t) => {
                    self.errors.push(
                        Error::new(
                            end.pos(),
                            format!(
                                "an end of a range is a `Nat`, and this is a `{}`",
                                self.defs.spelling(t)
                            ),
                        )
                        .with_fix("use `convert`"),
                    );
                }
                at @ Ty::Flex(_) => self.expect(body, end, &at, &Ty::Int(Int::U64)),
                other => {
                    let given = defs.describe(&other, body.scope);
                    self.errors.push(Error::new(
                        end.pos(),
                        format!("an end of a range is a whole number, and this is {given}"),
                    ));
                }
            }
        }
        match slice {
            Ty::Slice(_) => {
                self.in_piece(body, of, ends, pos, false);
                slice
            }
            Ty::Text => {
                self.in_piece(body, of, ends, pos, true);
                slice
            }
            Ty::List(_) => {
                self.errors
                    .push(Error::unsupported(pos, "a piece of a list"));
                Ty::Unknown
            }
            Ty::Unknown => Ty::Unknown,
            other => {
                let given = defs.describe(&other, body.scope);
                self.errors.push(Error::new(
                    pos,
                    format!("a piece is taken from a slice, and this is {given}"),
                ));
                Ty::Unknown
            }
        }
    }

    /// Checks `push(list, item)`, which gives the list with the item after
    /// its last element, and asks the system for memory.
    pub(super) fn push(&mut self, body: &mut Body<'a>, call: &Call, want: &Want) -> Ty {
        let defs = self.defs;
        let appends = is_append(call);
        let name = &call.callee;
        self.needs(body, "alloc", call.pos, &format!("`{name}`"));
        let plain = call.args.iter().all(|a| a.name.is_none() && !a.after);
        if call.args.len() != 2 || !plain {
            let second = match appends {
                true => "the list to add after it",
                false => "the item to add after it",
            };
            self.errors.push(Error::new(
                call.pos,
                format!("`{name}` takes a list and {second}"),
            ));
            return self.unknown_call(body, call);
        }
        let list_want = match want {
            Want::Ty(t @ Ty::List(_)) => Want::Ty(t.clone()),
            _ => Want::Any,
        };
        let list = self.expr(body, &call.args[0].value, &list_want);
        // A list written with nothing, that is pushed to, has the elements pushed.
        if let Ty::List(of) = &list
            && of[0].unknown()
            && !appends
            && let Some(settled) = self.push_to_empty(body, call)
        {
            return settled;
        }
        match list {
            Ty::Slice(_) => {
                self.expr(body, &call.args[1].value, &Want::Ty(Ty::Unknown));
                self.errors.push(
                    Error::new(
                        call.args[0].value.pos(),
                        format!(
                            "`{name}` adds to a list, and this is a slice, which cannot change"
                        ),
                    )
                    .with_fix("write `copy(s)` of the slice, which is a new list"),
                );
                Ty::Unknown
            }
            Ty::List(of) if appends => {
                let more = Ty::List(of.clone());
                let item = self.expr(body, &call.args[1].value, &Want::Ty(more.clone()));
                if !as_list(&item, &more) {
                    self.expect(body, &call.args[1].value, &item, &more);
                }
                if !ownership::copies(defs, body.scope, &of[0]) {
                    self.errors.push(
                        Error::new(
                            call.args[1].value.pos(),
                            "`append` copies the elements it adds, and this list holds elements that cannot be copied",
                        )
                        .with_fix("add the elements one at a time with `push`"),
                    );
                }
                more
            }
            Ty::List(of) => {
                let item = self.expr(body, &call.args[1].value, &Want::Ty(of[0].clone()));
                self.expect(body, &call.args[1].value, &item, &of[0]);
                Ty::List(of)
            }
            other => {
                self.expr(body, &call.args[1].value, &Want::Ty(Ty::Unknown));
                if other != Ty::Unknown {
                    let given = defs.describe(&other, body.scope);
                    self.errors.push(Error::new(
                        call.args[0].value.pos(),
                        format!("`{name}` adds to a list, and this is {given}"),
                    ));
                }
                Ty::Unknown
            }
        }
    }

    /// Checks a list written out, `[]` or `[a, b, c]`, where a list of
    /// known element type is wanted.
    pub(super) fn list_literal(
        &mut self,
        body: &mut Body<'a>,
        pos: Pos,
        items: &[Expr],
        want: &Want,
    ) -> Ty {
        let defs = self.defs;
        match want {
            Want::Ty(Ty::List(of)) => {
                if !items.is_empty() {
                    self.needs(body, "alloc", pos, "a list that holds elements");
                }
                for item in items {
                    let given = self.expr(body, item, &Want::Ty(of[0].clone()));
                    self.expect(body, item, &given, &of[0]);
                }
                Ty::List(of.clone())
            }
            Want::Ty(Ty::Array(of, n)) => {
                for item in items {
                    let given = self.expr(body, item, &Want::Ty(of[0].clone()));
                    self.expect(body, item, &given, &of[0]);
                }
                if items.len() as u64 != *n {
                    self.errors.push(Error::new(
                        pos,
                        format!(
                            "this array is written with {} elements, and its type has {n}",
                            items.len()
                        ),
                    ));
                }
                Ty::Array(of.clone(), *n)
            }
            Want::Ty(slice @ Ty::Slice(of)) => {
                if !matches!(of[0], Ty::Int(_)) {
                    for item in items {
                        self.expr(body, item, &Want::Ty(Ty::Unknown));
                    }
                    self.errors.push(Error::unsupported(
                        pos,
                        "a slice written out whose elements are not whole numbers",
                    ));
                    return Ty::Unknown;
                }
                for item in items {
                    let given = self.expr(body, item, &Want::Ty(of[0].clone()));
                    self.expect(body, item, &given, &of[0]);
                    if !matches!(item, Expr::Number { .. }) {
                        self.errors.push(Error::unsupported(
                            item.pos(),
                            "a slice written out with an element that is not a number written out",
                        ));
                    }
                }
                slice.clone()
            }
            Want::Ty(Ty::Unknown) => {
                for item in items {
                    self.expr(body, item, &Want::Ty(Ty::Unknown));
                }
                Ty::Unknown
            }
            Want::Ty(needed) => {
                for item in items {
                    self.expr(body, item, &Want::Ty(Ty::Unknown));
                }
                let needed = defs.describe(needed, body.scope);
                self.errors.push(Error::new(
                    pos,
                    format!("this is a list, where {needed} is needed"),
                ));
                Ty::Unknown
            }
            Want::Any | Want::Unused => {
                // Nothing says what it is, so it is as long as it is written, of
                // the smallest type that holds every element.
                let given: Vec<Ty> = items
                    .iter()
                    .map(|item| {
                        let given = self.expr(body, item, &Want::Any);
                        match item {
                            // A whole number is the range that holds it alone.
                            Expr::Number {
                                magnitude,
                                negative,
                                ..
                            } if matches!(given, Ty::Int(_) | Ty::Flex(_)) => {
                                let n = *magnitude as i128;
                                Int::between(
                                    if *negative { -n } else { n },
                                    if *negative { -n } else { n },
                                )
                                .map_or(given, Ty::Int)
                            }
                            _ => given,
                        }
                    })
                    .collect();
                let Some(first) = given.first() else {
                    self.errors.push(
                        Error::new(
                            pos,
                            "an array of nothing has no element to say its type, so it is written where a type is expected",
                        )
                        .with_fix("write its type, as in `xs: List[Nat] = []`"),
                    );
                    return Ty::Unknown;
                };
                if given.iter().any(Ty::unknown) {
                    return Ty::Unknown;
                }
                let mut element = first.clone();
                for (item, ty) in items.iter().zip(&given).skip(1) {
                    element = match (&element, ty) {
                        (Ty::Int(a), Ty::Int(b)) => match a.hull(*b) {
                            Some(hull) => Ty::Int(hull),
                            None => {
                                self.errors.push(Error::new(
                                    item.pos(),
                                    "no type of 64 bits holds every element of this array",
                                ));
                                return Ty::Unknown;
                            }
                        },
                        (a, b) if a == b => a.clone(),
                        (a, b) if a.differs_in_function(b) => {
                            self.errors.push(Error::new(item.pos(), WHICH_FUNCTION));
                            return Ty::Unknown;
                        }
                        (a, b) => {
                            let (a, b) = (defs.text(a, body.scope), defs.text(b, body.scope));
                            self.errors.push(
                                Error::new(
                                    item.pos(),
                                    format!(
                                        "an array holds one type, and this holds `{a}` and `{b}`"
                                    ),
                                )
                                .with_fix("write the type the array is to have where it is made"),
                            );
                            return Ty::Unknown;
                        }
                    };
                }
                Ty::Array(vec![element], items.len() as u64)
            }
        }
    }

    /// Checks `of[index]`, which reads an element of a list, and compiles
    /// only when `arith` proves that the index is below the list's size.
    pub(super) fn index(&mut self, body: &mut Body<'a>, of: &Expr, index: &Expr, pos: Pos) -> Ty {
        let defs = self.defs;
        let list = self.expr(body, of, &Want::Any);
        if let Ty::Matrix(element, sizes) = &list {
            return self.matrix_index(body, of, index, (&element[0], sizes), pos);
        }
        let at = self.index_type(body, index);
        match list {
            // An index that holds a mistake already reported is not proven.
            Ty::List(element) | Ty::Slice(element) | Ty::Array(element, _) if at.unknown() => {
                element[0].clone()
            }
            Ty::List(element) | Ty::Slice(element) => {
                self.in_range(body, of, None, index, pos);
                element[0].clone()
            }
            Ty::Array(element, n) => {
                self.in_range(body, of, Some(n), index, pos);
                element[0].clone()
            }
            Ty::Text => {
                self.in_range(body, of, None, index, pos);
                Ty::Int(Int::U8)
            }
            Ty::Unknown => Ty::Unknown,
            other => {
                let given = defs.describe(&other, body.scope);
                self.errors.push(Error::new(
                    pos,
                    format!(
                        "an index is read from a list, a slice, or a text, and this is {given}"
                    ),
                ));
                Ty::Unknown
            }
        }
    }

    /// Checks an index, which is a `Nat`, or a whole number of a range that
    /// is not below 0, and gives its type.
    pub(super) fn index_type(&mut self, body: &mut Body<'a>, index: &Expr) -> Ty {
        let defs = self.defs;
        let wanted = match flexible(index) {
            true => Want::Ty(Ty::Int(Int::U64)),
            false => Want::Any,
        };
        let at = self.expr(body, index, &wanted);
        match &at {
            Ty::Int(Int::U64) | Ty::Unknown => {}
            // A range of numbers that are not below 0 is a `Nat` where an index
            // is needed, with nothing written.
            Ty::Int(t) if Int::U64.holds(*t) => {
                self.widened.insert(addr(index), Int::U64);
            }
            Ty::Int(t) => {
                self.errors.push(
                    Error::new(
                        index.pos(),
                        format!(
                            "an index is a `Nat`, and this is a `{}`",
                            self.defs.spelling(*t)
                        ),
                    )
                    .with_fix("use `convert`"),
                );
            }
            Ty::Flex(_) => self.expect(body, index, &at, &Ty::Int(Int::U64)),
            other => {
                let given = defs.describe(other, body.scope);
                self.errors.push(Error::new(
                    index.pos(),
                    format!("an index is a whole number, and this is {given}"),
                ));
            }
        }
        at
    }

    /// Checks a string that holds expressions, which asks the system for
    /// memory, and checks each expression in it as the text it is written as.
    pub(super) fn interpolation(&mut self, body: &mut Body<'a>, pos: Pos, parts: &[Part]) -> Ty {
        let needs = "a string that holds an expression";
        if body.default.is_some() {
            self.needs(body, "alloc", pos, needs);
            for part in parts {
                if let Part::Expr(Expr::Call(call)) = part {
                    self.unknown_call(body, call);
                }
            }
            return Ty::Unknown;
        }
        self.needs(body, "alloc", pos, needs);
        for part in parts {
            if let Part::Expr(e) = part {
                self.expr(body, e, &Want::Ty(Ty::Text));
            }
        }
        Ty::Text
    }
}
