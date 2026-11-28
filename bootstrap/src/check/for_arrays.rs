//! Checks `for i { e }`, the array whose element at each index is what the
//! block gives, and `for i, j { e }`, the matrix whose element at each pair
//! of indices is: the size of each index, from each array that is a name read
//! at the index alone along an axis and from the type the `for` is written
//! for, which must agree; each index, which is the range below its size, so
//! that `arith` proves a read of an array from its type; and the block, which
//! may need no effect but `alloc`. It checks a read of a matrix, `m[i, j]`,
//! too, with one index for each axis.

use super::*;
use crate::parser::For;

impl<'a> Checker<'a> {
    pub(super) fn for_expr(&mut self, body: &mut Body<'a>, f: &For, want: &Want) -> Ty {
        let indices = &f.indices;
        let (element, stated) = match want {
            Want::Ty(Ty::Array(of, n)) if indices.len() == 1 => (Some(of[0].clone()), vec![*n]),
            Want::Ty(Ty::Matrix(of, sizes)) if sizes.len() == indices.len() => {
                (Some(of[0].clone()), sizes.clone())
            }
            Want::Ty(Ty::Unknown) => (Some(Ty::Unknown), Vec::new()),
            // A type with another number of sizes than the `for` has indices
            // says no size of any.
            Want::Ty(wanted @ (Ty::Array(of, _) | Ty::Matrix(of, _))) => {
                let wanted = self.defs.text(wanted, body.scope);
                let gives = match indices.len() {
                    1 => "an array of one size".to_string(),
                    n => format!("a matrix of {n} sizes"),
                };
                self.errors.push(Error::new(
                    f.pos,
                    format!(
                        "a `for` over {} gives {gives}, and a `{wanted}` is needed",
                        match indices.len() {
                            1 => "one index".to_string(),
                            n => format!("{n} indices"),
                        }
                    ),
                ));
                (Some(of[0].clone()), Vec::new())
            }
            _ => (None, Vec::new()),
        };
        let unknown = element.is_some() && stated.is_empty();
        let mut sizes = Vec::new();
        for (axis, (name, at)) in indices.iter().enumerate() {
            let mut found = self.read_sizes(body, f, name);
            if let Some(n) = stated.get(axis) {
                found.push((*n, "the type the `for` is written for".to_string()));
            }
            sizes.push(self.index_size(indices, (name, *at), &found, unknown));
        }
        if !self.assigned_vars(body, &[&f.round.body]).is_empty() {
            self.errors.push(Error::unsupported(
                f.pos,
                "a `for` that assigns a `mut` outside it",
            ));
        }
        let outer = body.names.len();
        for (k, ((name, at), size)) in indices.iter().zip(&sizes).enumerate() {
            let twice = indices[..k].iter().any(|(n, _)| n == name);
            if twice || body.names[..outer].iter().any(|(n, _)| n == name) {
                self.errors.push(self.defined_twice(body, name, *at));
            }
            let index = match size.and_then(|n| Int::between(0, n as i128 - 1)) {
                Some(range) => Ty::Int(range),
                None => Ty::Unknown,
            };
            body.names.push((name.to_string(), index));
        }
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
        match sizes.iter().copied().collect::<Option<Vec<u64>>>() {
            _ if element.unknown() => Ty::Unknown,
            Some(sizes) if sizes.len() == 1 => Ty::Array(vec![element], sizes[0]),
            Some(sizes) => Ty::Matrix(vec![element], sizes),
            None => Ty::Unknown,
        }
    }

    /// The size of the index `name` from the sizes found for it, with what
    /// gives each, which must agree. An index with none is refused, unless a
    /// mistake already reported is why.
    fn index_size(
        &mut self,
        indices: &[(String, Pos)],
        (name, at): (&str, Pos),
        found: &[(u64, String)],
        unknown: bool,
    ) -> Option<u64> {
        match found {
            [] => {
                if !unknown {
                    let fours = vec!["4"; indices.len()].join(", ");
                    let names: Vec<&str> = indices.iter().map(|(n, _)| n.as_str()).collect();
                    let names = names.join(", ");
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
                             of the `for`, as `x: [{fours}]f32 = for {names} {{ ... }}`"
                        )),
                    );
                }
                None
            }
            [(n, first), rest @ ..] => {
                if let Some((m, other)) = rest.iter().find(|(m, _)| m != n) {
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
        }
    }

    /// The size each array that is a name, and is read at the index `name`
    /// alone along one of its axes, gives the index, with the name that gives
    /// it.
    fn read_sizes(&self, body: &Body<'a>, f: &For, name: &str) -> Vec<(u64, String)> {
        let mut found = Vec::new();
        for e in exprs(values(&f.round.body)) {
            let Expr::Index { of, index, .. } = e else {
                continue;
            };
            let Expr::Name { name: array, .. } = &**of else {
                continue;
            };
            let Some((_, ty)) = body.names.iter().rev().find(|(n, _)| n == array) else {
                continue;
            };
            let at: Vec<&Expr> = match &**index {
                Expr::Tuple { items, .. } => items.iter().collect(),
                one => vec![one],
            };
            let sizes = match ty {
                Ty::Array(_, n) => vec![*n],
                Ty::Matrix(_, sizes) => sizes.clone(),
                _ => continue,
            };
            if sizes.len() != at.len() {
                continue;
            }
            for (n, e) in sizes.iter().zip(at) {
                if matches!(e, Expr::Name { name: i, .. } if i == name) {
                    found.push((*n, format!("`{array}`")));
                }
            }
        }
        found
    }

    /// Checks `of[i, j]`, a read of an element of a matrix, with one index
    /// for each axis, each of which `arith` proves below the size of its axis.
    pub(super) fn matrix_index(
        &mut self,
        body: &mut Body<'a>,
        of: &Expr,
        index: &Expr,
        (element, sizes): (&Ty, &[u64]),
        pos: Pos,
    ) -> Ty {
        let at: Vec<&Expr> = match index {
            Expr::Tuple { items, .. } => items.iter().collect(),
            one => vec![one],
        };
        if at.len() != sizes.len() {
            let (written, axes, given) = (written_expr(of), sizes.len(), at.len());
            self.errors.push(Error::new(
                pos,
                format!(
                    "`{written}` is a matrix of {axes} sizes, so it is read with {axes} indices, \
                     and this has {given}"
                ),
            ));
            return Ty::Unknown;
        }
        for (e, n) in at.into_iter().zip(sizes) {
            if !self.index_type(body, e).unknown() {
                self.in_range(body, of, Some(*n), e, pos);
            }
        }
        element.clone()
    }
}
