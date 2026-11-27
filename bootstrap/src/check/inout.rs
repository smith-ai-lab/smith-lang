//! What `inout` refuses, as `knowledge/language/inout.md` says: a call marks
//! each argument whose parameter is `inout`, and only those; what it passes
//! `inout` is a place that can be assigned, on a line of its own; one place is
//! passed once. The parser refuses a lambda that holds a parameter marked
//! `inout`, and has lowered `inout` to taking the value and giving it
//! back, and the rest of what `inout` means is what that means.

use super::*;
use crate::parser::{Param, changed_place};

/// A place passed `inout`: the name, the fields after it, and the item of the
/// array or list they name, if it is one.
type Place<'e> = (&'e str, Vec<&'e str>, Option<&'e Expr>);

/// Whether two places passed `inout` are one, or one holds the other. Two
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

impl<'a> Checker<'a> {
    /// Checks what a call passes `inout`, and that it passes `inout` what its
    /// function changes. Gives whether it found a mistake, which it reports.
    pub(super) fn passed_inout(&mut self, body: &Body<'a>, call: &Call) -> bool {
        let before = self.errors.len();
        self.inout_mistakes(body, call);
        self.errors.len() > before
    }

    fn inout_mistakes(&mut self, body: &Body<'a>, call: &Call) {
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
            let changed = param.is_some_and(|p| p.inout);
            match arg.inout {
                None if changed => {
                    unmarked = unmarked.or(Some((arg, param.expect("changed"))));
                }
                None => {}
                Some(at) if !changed => self.errors.push(
                    Error::new(
                        at,
                        format!(
                            "`inout` is written before an argument that `{callee}` does not change"
                        ),
                    )
                    .with_fix("take away `inout`"),
                ),
                Some(_) => {
                    if changed_place(&arg.value).is_none() {
                        self.errors.push(Error::new(
                            arg.value.pos(),
                            "what is passed `inout` is a `var`, or a field or an item of one, as nothing would see the change to anything else",
                        ));
                        continue;
                    }
                    if !arg.given_back {
                        self.errors.push(Error::new(
                            call.pos,
                            format!(
                                "a call that passes `inout` is a line of its own, as `{callee}` gives nothing"
                            ),
                        ));
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
                                "`{one}` is passed `inout` twice, and one place is changed by one argument"
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
                if p.is_some_and(|p| p.inout) && a.inout.is_none() {
                    a.inout = Some(a.value.pos());
                }
            }
            let names = |_: usize, a: &Arg| a.name.as_ref().map(|(n, _)| n.clone());
            let fix = format!("write `{}`", self.call_written(&fixed, &names));
            self.errors.push(
                Error::new(
                    arg.value.pos(),
                    format!(
                        "`{callee}` changes `{}` for its caller, so the call marks it `inout`",
                        param.name
                    ),
                )
                .with_fix(fix),
            );
        }
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
