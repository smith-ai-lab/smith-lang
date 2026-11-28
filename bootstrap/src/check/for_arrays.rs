//! Checks `for i { e }`, the array whose element at each index is what the
//! block gives: the size of the index, from each array that is a name read
//! at the index alone and from the type the `for` is written for, which must
//! agree; the index, which is the range below the size, so that `arith`
//! proves a read of an array from its type; and the block, which may need no
//! effect but `alloc`.

use super::*;
use crate::parser::For;

impl<'a> Checker<'a> {
    pub(super) fn for_expr(&mut self, body: &mut Body<'a>, f: &For, want: &Want) -> Ty {
        let (name, at) = f.index();
        let (element, stated) = match want {
            Want::Ty(Ty::Array(of, n)) => (Some(of[0].clone()), Some(*n)),
            Want::Ty(Ty::Unknown) => (Some(Ty::Unknown), None),
            _ => (None, None),
        };
        let mut sizes = self.read_sizes(body, f);
        if let Some(n) = stated {
            sizes.push((n, "the type the `for` is written for".to_string()));
        }
        let size = match &sizes[..] {
            [] => {
                if element != Some(Ty::Unknown) {
                    self.errors.push(
                        Error::new(
                            at,
                            format!(
                                "the index `{name}` has no size, as no array is read at it \
                                 alone and no type is stated for the `for`"
                            ),
                        )
                        .with_fix(format!(
                            "read an array at `{name}` alone, as `a[{name}]`, or state the type \
                             of the `for`, as `x: [4]f32 = for {name} {{ ... }}`"
                        )),
                    );
                }
                None
            }
            [(n, _), rest @ ..] => {
                if let Some((m, other)) = rest.iter().find(|(m, _)| m != n) {
                    let (_, first) = &sizes[0];
                    self.errors.push(Error::new(
                        at,
                        format!(
                            "the index `{name}` has two sizes: {n} from {first}, and {m} from \
                             {other}"
                        ),
                    ));
                    None
                } else {
                    Some(*n)
                }
            }
        };
        if !self.assigned_vars(body, &[&f.round.body]).is_empty() {
            self.errors.push(Error::unsupported(
                f.pos,
                "a `for` that assigns a `mut` outside it",
            ));
        }
        let outer = body.names.len();
        if body.names.iter().any(|(n, _)| n == name) {
            self.errors.push(self.defined_twice(body, name, at));
        }
        let index = match size.and_then(|n| Int::between(0, n as i128 - 1)) {
            Some(range) => Ty::Int(range),
            None => Ty::Unknown,
        };
        body.names.push((name.to_string(), index));
        self.fors += 1;
        let wanted = match &element {
            Some(t) => Want::Ty(t.clone()),
            None => Want::Any,
        };
        let given = match self.block(body, &f.round.body, &wanted) {
            End::Value(given, value) => {
                if let Some(needed) = &element {
                    self.expect(body, value, &given, needed);
                }
                given
            }
            End::Empty | End::Binding(_) => Ty::Nothing,
        };
        self.fors -= 1;
        body.truncate_names(outer);
        let element = match element {
            Some(Ty::Unknown) | None => given,
            Some(needed) => needed,
        };
        if !element.unknown() && !ownership::copies(self.defs, body.scope, &element) {
            self.errors.push(Error::unsupported(
                f.pos,
                "a `for` whose elements have no `Copy`",
            ));
        }
        match size {
            Some(n) if !element.unknown() => Ty::Array(vec![element], n),
            _ => Ty::Unknown,
        }
    }

    /// The size each array that is a name, and is read at the index of the
    /// `for` alone, gives the index, with the name that gives it.
    fn read_sizes(&self, body: &Body<'a>, f: &For) -> Vec<(u64, String)> {
        let (name, _) = f.index();
        let mut found = Vec::new();
        for e in exprs(values(&f.round.body)) {
            let Expr::Index { of, index, .. } = e else {
                continue;
            };
            let (Expr::Name { name: array, .. }, Expr::Name { name: at, .. }) = (&**of, &**index)
            else {
                continue;
            };
            if at != name {
                continue;
            }
            if let Some((_, Ty::Array(_, n))) = body.names.iter().rev().find(|(n, _)| n == array) {
                found.push((*n, format!("`{array}`")));
            }
        }
        found
    }
}
