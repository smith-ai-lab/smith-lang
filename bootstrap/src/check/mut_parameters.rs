//! What `mut` refuses, as `knowledge/language/mut-parameters.md` says: a call marks
//! each argument whose parameter is `mut`, and only those; what it passes
//! `mut` is a place that can be assigned, on a line of its own; one place is
//! passed once. The parser refuses a lambda that holds a parameter marked
//! `mut`, and has lowered `mut` to taking the value and giving it
//! back, and the rest of what `mut` means is what that means.

use super::*;
use crate::parser::{Function, Param, changed_place, own_result};

/// A place passed `mut`: the name, the fields after it, and the item of the
/// array or list they name, if it is one.
type Place<'e> = (&'e str, Vec<&'e str>, Option<&'e Expr>);

/// Whether two places passed `mut` are one, or one holds the other. Two
/// items of one list are one unless each is a different number.
fn overlap(a: &Place, b: &Place) -> bool {
    if a.0 != b.0 || !a.1.iter().zip(&b.1).all(|(x, y)| x == y) {
        return false;
    }
    match (a.2, b.2) {
        (Some(i), Some(j)) if a.1 == b.1 => !different_numbers(i, j),
        _ => true,
    }
}

fn different_numbers(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Number { text: x, .. }, Expr::Number { text: y, .. }) => {
            x.replace('_', "") != y.replace('_', "")
        }
        _ => false,
    }
}

fn written_place(p: &Place) -> String {
    let mut out = std::iter::once(p.0)
        .chain(p.1.iter().copied())
        .collect::<Vec<_>>()
        .join(".");
    if let Some(i) = p.2 {
        out = format!("{out}[{}]", written_expr(i));
    }
    out
}

/// Whether a call is of `old`, which names what a parameter marked `mut` was
/// given, and no function of the program takes the name of.
pub(super) fn is_old(defs: &Defs, call: &Call) -> bool {
    call.qualifier.is_none()
        && call.callee == "old"
        && !defs.function_names.contains_key("old")
        && defs.struct_named("old").is_none()
}

/// The name `old` is written of, when `call` is `old` of a name.
pub(super) fn old_name<'e>(defs: &Defs, call: &'e Call) -> Option<&'e str> {
    match call.args.as_slice() {
        [arg] if is_old(defs, call) => match &arg.value {
            Expr::Name { name, .. } => Some(name),
            _ => None,
        },
        _ => None,
    }
}

impl<'a> Checker<'a> {
    /// Checks `old(c)`, what the parameter `c` marked `mut` was given when
    /// the call began, which is written only in an `ensures`.
    pub(super) fn old(&mut self, body: &mut Body<'a>, call: &Call) -> Ty {
        if !self.ensuring {
            self.errors.push(
                Error::new(call.pos, "`old` is written only in an `ensures`")
                    .with_fix("name the value before it changes with a binding"),
            );
            return self.unknown_call(body, call);
        }
        let plain = call.args.iter().all(|a| a.name.is_none() && !a.after);
        let named = old_name(self.defs, call).filter(|_| plain);
        let param = named.and_then(|name| {
            let f = body.f?;
            let at = f.params.iter().position(|p| p.name == name)?;
            f.params[at].mutable.then(|| body.names[at].1.clone())
        });
        match (named, param) {
            (_, Some(ty)) => ty,
            (Some(name), None) => {
                self.errors.push(
                    Error::new(
                        call.pos,
                        format!(
                            "`{name}` is not a parameter marked `mut`, so it is the same before the call and after, and `old` is not written of it"
                        ),
                    )
                    .with_fix(format!("write `{name}`")),
                );
                self.unknown_call(body, call)
            }
            (None, _) => {
                self.errors.push(Error::new(
                    call.pos,
                    "`old` is written of the name of a parameter marked `mut`",
                ));
                self.unknown_call(body, call)
            }
        }
    }

    /// Checks that a call that passes `mut` keeps what its function gives
    /// exactly when the function gives a result, when `gives` is what the
    /// lowered call gives. Gives whether it found a mistake, which it reports.
    pub(super) fn kept_mistake(
        &mut self,
        body: &Body<'a>,
        call: &Call,
        f: &Function,
        gives: &Ty,
    ) -> bool {
        let keeps = call.args.iter().any(|a| a.keeps);
        let callee = written_name(&call.callee);
        match (own_result(f), keeps) {
            (None, true) => {
                self.errors.push(
                    Error::new(
                        call.pos,
                        format!("`{callee}` gives nothing, so a call that passes `mut` to it is a line of its own"),
                    )
                    .with_fix(format!("take away what `{callee}` is bound or assigned to")),
                );
                true
            }
            (Some(_), false) => {
                // What the function gives is the last item of what the lowered
                // call gives, and nothing uses it.
                let Ty::Tuple(items) = gives else {
                    return false;
                };
                let result = items.last().cloned().unwrap_or(Ty::Unknown);
                if result == Ty::Nothing {
                    return false;
                }
                let before = self.errors.len();
                self.dropped(body, &Expr::Call(call.clone()), &result);
                // A value that is not dropped is still not used.
                if self.errors.len() == before && !result.unknown() {
                    let described = self.defs.text(&result, body.scope);
                    self.errors.push(
                        Error::new(
                            call.pos,
                            format!("this gives a `{described}`, which nothing uses"),
                        )
                        .with_fix("bind it to a name"),
                    );
                }
                true
            }
            _ => false,
        }
    }

    /// Checks what a call passes `mut`, and that it passes `mut` what its
    /// function changes. Gives whether it found a mistake, which it reports.
    pub(super) fn passed_mut(&mut self, body: &Body<'a>, call: &Call) -> bool {
        let before = self.errors.len();
        self.mut_mistakes(body, call);
        self.errors.len() > before
    }

    fn mut_mistakes(&mut self, body: &Body<'a>, call: &Call) {
        let defs = self.defs;
        let local = body.names.iter().any(|(n, _)| *n == call.callee);
        let f = (call.through.is_none() && call.qualifier.is_none() && !local)
            .then(|| defs.function(call.name_pos, &call.callee))
            .flatten();
        let params: Vec<Option<&Param>> = match f {
            Some(f) => {
                let sig = &defs.signatures[f];
                let written = &defs.file.functions[f].params;
                // Arguments that fit no parameter are reported where they are
                // matched to them.
                let Some(at) = params_of(&sig.params, sig.by_position(), &call.args) else {
                    return;
                };
                (at.iter())
                    .map(|&i| {
                        let (name, _) = sig.params.get(i)?;
                        written.iter().find(|p| p.name == *name)
                    })
                    .collect()
            }
            None => vec![None; call.args.len()],
        };
        let callee = written_name(&call.callee);
        let mut unmarked = None;
        let mut places: Vec<Place> = Vec::new();
        for (arg, param) in call.args.iter().zip(&params) {
            let changed = param.is_some_and(|p| p.mutable);
            match arg.mutable {
                None if changed => {
                    unmarked = unmarked.or(Some((arg, param.expect("changed"))));
                }
                None => {}
                Some(at) if !changed => self.errors.push(
                    Error::new(
                        at,
                        format!(
                            "`mut` is written before an argument that `{callee}` does not change"
                        ),
                    )
                    .with_fix("take away `mut`"),
                ),
                Some(_) => {
                    if changed_place(&arg.value).is_none() && field_of_item(&arg.value) {
                        self.errors.push(Error::unsupported(
                            arg.value.pos(),
                            "passing a field of an item `mut`",
                        ));
                        continue;
                    }
                    if changed_place(&arg.value).is_none() {
                        self.errors.push(Error::new(
                            arg.value.pos(),
                            "what is passed `mut` is a `mut`, or a field or an item of one, as nothing would see the change to anything else",
                        ));
                        continue;
                    }
                    if !arg.given_back {
                        let mut error = Error::new(
                            call.pos,
                            "a call that passes `mut` is a line of its own, or the whole right-hand side of a binding or an assignment",
                        );
                        if f.is_some_and(|f| own_result(&defs.file.functions[f]).is_some()) {
                            error = error.with_fix(format!(
                                "bind what `{callee}` gives to a name on a line before"
                            ));
                        }
                        self.errors.push(error);
                        continue;
                    }
                    let place: Place = (
                        name_in(&arg.value),
                        fields_in(&arg.value),
                        item_in(&arg.value),
                    );
                    if let Some(other) = places.iter().find(|p| overlap(p, &place)) {
                        let (one, two) = (written_place(other), written_place(&place));
                        let message = if one == two {
                            format!(
                                "`{one}` is passed `mut` twice, and one place is changed by one argument"
                            )
                        } else {
                            format!(
                                "`{one}` and `{two}` are one place, or one holds the other, and it is changed by one argument"
                            )
                        };
                        self.errors.push(Error::new(arg.value.pos(), message));
                    }
                    places.push(place);
                }
            }
        }
        if let Some((arg, param)) = unmarked {
            let mut fixed = call.clone();
            for (a, p) in fixed.args.iter_mut().zip(&params) {
                if p.is_some_and(|p| p.mutable) && a.mutable.is_none() {
                    a.mutable = Some(a.value.pos());
                }
            }
            let names = |_: usize, a: &Arg| a.name.as_ref().map(|(n, _)| n.clone());
            let fix = format!("write `{}`", self.call_written(&fixed, &names));
            self.errors.push(
                Error::new(
                    arg.value.pos(),
                    format!(
                        "`{callee}` changes `{}` for its caller, so the call marks it `mut`",
                        param.name
                    ),
                )
                .with_fix(fix),
            );
        }
    }
}

/// Whether an expression is a field of an item of a place, as `cs[0].at` is,
/// which is not built as a place a call changes.
fn field_of_item(e: &Expr) -> bool {
    match e {
        Expr::Field { of, .. } => match &**of {
            Expr::Index { of, .. } => changed_place(of).is_some() || field_of_item(of),
            of => field_of_item(of),
        },
        _ => false,
    }
}

/// The name at the root of a place.
fn name_in(e: &Expr) -> &str {
    match e {
        Expr::Name { name, .. } => name,
        Expr::Field { of, .. } | Expr::Index { of, .. } => name_in(of),
        _ => "",
    }
}

/// The fields of a place, from its root.
fn fields_in(e: &Expr) -> Vec<&str> {
    fn all<'e>(e: &'e Expr, out: &mut Vec<&'e str>) {
        match e {
            Expr::Field { of, name, .. } => {
                all(of, out);
                out.push(name);
            }
            Expr::Index { of, .. } => all(of, out),
            _ => {}
        }
    }
    let mut out = Vec::new();
    all(e, &mut out);
    out
}

/// The index of a place that is an item.
fn item_in(e: &Expr) -> Option<&Expr> {
    match e {
        Expr::Index { index, .. } => Some(index),
        _ => None,
    }
}

impl Checker<'_> {
    /// Reports a call that gives away the arguments at `at` without `take`
    /// written before them, at the first, with the call written with `take`
    /// before each argument it gives away.
    pub(super) fn unmarked_take(&mut self, call: &Call, at: &[Pos]) {
        let mut fixed = call.clone();
        for a in &mut fixed.args {
            if at.contains(&a.value.pos()) {
                a.take = Some(a.value.pos());
            }
        }
        let Some(arg) = call.args.iter().find(|a| at.contains(&a.value.pos())) else {
            return;
        };
        let names = |_: usize, a: &Arg| a.name.as_ref().map(|(n, _)| n.clone());
        let fix = format!("write `{}`", self.call_written(&fixed, &names));
        self.errors.push(
            Error::new(
                arg.value.pos(),
                format!(
                    "`{}` takes `{}`, so the call marks it `take`",
                    written_name(&call.callee),
                    written_expr(&arg.value)
                ),
            )
            .with_fix(fix),
        );
    }
}
