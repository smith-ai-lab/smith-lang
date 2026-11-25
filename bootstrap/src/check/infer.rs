//! The range of a `var` with no stated type, which is the smallest that holds
//! every value the body gives it.
//!
//! The body of a function that has such a `var` is looked at again and again,
//! with the errors thrown away, until the range of each `var` is the one that
//! every value it is given needs. A `var` whose value is read where it is not
//! known, after an `if` or in a loop, is read as the range it has so far, so a
//! value that depends on the `var` itself through a branch or a loop makes it
//! grow at every look, and is refused when it has not stopped growing by the
//! last. Code with no branch is solved in order: each assignment gives the
//! `var` the range of its value, and the next line reads that range.
//!
//! The look that is kept gives each such `var` its range as if it were
//! written, so what `arith` proves, and what is built, is what stating that
//! range gives.

use std::collections::{HashMap, HashSet};

use super::{Body, Checker, Int, Numbers, Ty, Want, pattern_names, statements, written_expr};
use crate::diag::{Error, Pos};
use crate::parser::{Expr, Stmt};

/// What is learned of the `var`s while their ranges are solved, by the place
/// of the statement that defines each.
#[derive(Default)]
pub(super) struct Solve {
    /// The range each had at the end of the look before.
    pub(super) before: HashMap<Pos, Int>,
    /// The range each has from what it was given so far in this look.
    pub(super) totals: HashMap<Pos, Int>,
    /// The values each was given: the place of the statement, how it is
    /// written, and its range.
    pub(super) given: HashMap<Pos, Vec<(Pos, String, Int)>>,
    /// The `var`s that were given a value in a loop.
    pub(super) in_loop: HashSet<Pos>,
    /// How each is written where it is defined.
    pub(super) names: HashMap<Pos, String>,
    /// The `var`s whose value depends on the `var` itself where nothing says
    /// what it holds, so that nothing says what the value is.
    pub(super) stuck: HashSet<Pos>,
    /// The `var`s first given a number written out.
    pub(super) written: HashSet<Pos>,
    /// The first assignment of each `var` in `stuck` that reads it.
    pub(super) stuck_at: HashMap<Pos, Pos>,
    /// The element type each list or option that is written with nothing to
    /// say it had at the end of the look before, and what it has from what it
    /// is given and used for so far in this look.
    pub(super) before_elems: HashMap<Pos, Ty>,
    pub(super) elems: HashMap<Pos, Ty>,
    /// The names that were first given a list or an option written with
    /// nothing to say its element type.
    pub(super) empties: HashSet<Pos>,
}

/// What the last solved look left, which the look that is kept reads.
#[derive(Default)]
pub(super) struct Solved {
    /// The range of each `var`.
    pub(super) ranges: HashMap<Pos, Int>,
    /// The values each `var` was given.
    pub(super) given: HashMap<Pos, Vec<(Pos, String, Int)>>,
    /// The range of the first use that needs less than a `var` has.
    pub(super) needs: HashMap<Pos, Int>,
    /// The `var`s that are refused for being changed by themselves, which the
    /// look that is kept does not check against a range.
    pub(super) refused: HashSet<Pos>,
    /// The element type of each list or option written with nothing to say it.
    pub(super) elems: HashMap<Pos, Ty>,
}

/// Whether a block, or one inside it, defines a `var` with no type written.
pub(super) fn has_inferred(stmts: &[Stmt]) -> bool {
    statements(stmts).into_iter().any(|s| {
        matches!(
            s,
            Stmt::Define {
                var: true,
                ty: None,
                ..
            }
        ) || matches!(s, Stmt::Define { ty: None, value, .. } if is_empty(value))
    })
}

/// Whether a value is a list of nothing, or the case `.none`, which say
/// nothing of the type of their elements.
fn is_empty(value: &Expr) -> bool {
    match value {
        Expr::List { items, .. } => items.is_empty(),
        Expr::Case {
            name,
            payload: None,
            ..
        } => name == "none",
        _ => false,
    }
}

/// What two element types, one of a list or an option, make together: the
/// range that holds both ranges, or the type that both are, or none.
fn joined(a: &Ty, b: &Ty) -> Option<Ty> {
    match (a, b) {
        (Ty::Unknown, t) | (t, Ty::Unknown) => Some(t.clone()),
        (Ty::Int(x), Ty::Int(y)) => x.hull(*y).map(Ty::Int),
        (x, y) if x == y => Some(x.clone()),
        _ => None,
    }
}

/// The names that a pattern gives a value, which a `var` with no type written
/// is not solved for, as what a pattern gives is not read from its parts.
pub(super) fn patterned(stmts: &[Stmt]) -> HashSet<String> {
    statements(stmts)
        .into_iter()
        .filter_map(|s| match s {
            Stmt::Destructure { pattern, .. } => Some(pattern_names(pattern)),
            _ => None,
        })
        .flatten()
        .map(|(name, _)| name.to_string())
        .collect()
}

/// The range that holds a number written out, alone.
fn literal(value: &Expr) -> Option<Int> {
    let Expr::Number {
        magnitude,
        negative,
        ..
    } = value
    else {
        return None;
    };
    let n = i128::try_from(*magnitude).ok()?;
    let n = if *negative { -n } else { n };
    Int::between(n, n)
}

impl<'a> Checker<'a> {
    /// Whether a `var` with no type written takes its range from what it is
    /// given, in the body being checked.
    pub(super) fn infers(&self, body: &Body<'a>, name: &str) -> bool {
        self.infer_on && !matches!(body.numbers, Numbers::Off) && !self.patterned.contains(name)
    }

    /// Checks the value a `var` with no type written is first given, and
    /// gives the type it has: the range that holds every value it is given.
    /// The range of the value itself is the second part, for the places that
    /// need it.
    pub(super) fn infer_var(
        &mut self,
        body: &mut Body<'a>,
        pos: Pos,
        name: &str,
        value: &Expr,
    ) -> (Ty, Option<Int>) {
        let gives = match literal(value) {
            Some(r) => self.expr(body, value, &Want::Ty(Ty::Int(r))),
            None => self.expr(body, value, &Want::Any),
        };
        let Ty::Int(first) = gives else {
            return (gives, None);
        };
        if self.solve.is_none() && self.solved.refused.contains(&pos) {
            // The refusal is said, and what the `var` holds is not looked at again.
            return (Ty::Unknown, None);
        }
        let text = written_expr(value);
        let counted = !matches!(body.numbers, Numbers::Finding(_));
        let total = match self.solve.as_mut() {
            Some(solve) => {
                let before = solve.before.get(&pos).copied().unwrap_or(first);
                let total = before.hull(first).unwrap_or(first);
                if counted {
                    solve.totals.insert(pos, total);
                    solve.given.insert(pos, vec![(pos, text, first)]);
                    solve.names.insert(pos, name.to_string());
                    if literal(value).is_some() {
                        solve.written.insert(pos);
                    }
                }
                total
            }
            None => self.solved.ranges.get(&pos).copied().unwrap_or(first),
        };
        // The first value enters the range, which is wider where later values need it.
        self.expect(body, value, &gives, &Ty::Int(total));
        (Ty::Int(total), Some(first))
    }

    /// Whether a value says nothing of the type of its elements, in a body
    /// where such a type is solved from what the name is given and used for.
    pub(super) fn infers_empty(&self, body: &Body<'a>, value: &Expr) -> bool {
        self.infer_on
            && !matches!(body.numbers, Numbers::Off)
            && is_empty(value)
            && match value {
                Expr::Case { .. } => self.defs.case_of(self.defs.option, "none").is_some(),
                _ => true,
            }
    }

    /// Checks a list of nothing, or `.none`, given to a name: the type is the
    /// one the name is given and used for, and is none until one does.
    pub(super) fn infer_empty(&mut self, body: &mut Body<'a>, pos: Pos, value: &Expr) -> Ty {
        let known = match self.solve.as_ref() {
            Some(solve) => solve.before_elems.get(&pos),
            None => self.solved.elems.get(&pos),
        }
        .cloned();
        let element = known.clone().unwrap_or(Ty::Unknown);
        let ty = match value {
            Expr::List { .. } => Ty::List(vec![element]),
            _ => Ty::Enum(self.defs.option, vec![element]),
        };
        if self.solve.is_none() && known.is_none() {
            // Nothing settled it, which the check of the value says.
            return self.expr(body, value, &Want::Any);
        }
        self.expr(body, value, &Want::Ty(ty.clone()));
        if let Some(solve) = self.solve.as_mut() {
            solve.empties.insert(pos);
            if let Some(known) = known {
                solve.elems.insert(pos, known);
            }
        }
        ty
    }

    /// The element type of the list or option `def` was given or used for.
    fn settle_element(&mut self, def: Pos, element: &Ty) {
        if element.unknown() {
            return;
        }
        let Some(solve) = self.solve.as_mut() else {
            return;
        };
        let held = solve.elems.get(&def).cloned().unwrap_or(Ty::Unknown);
        if let Some(both) = joined(&held, element) {
            solve.elems.insert(def, both);
        }
    }

    /// Notes that a use needs `needed` of the list or option that `value`
    /// names, which says the type of its elements.
    pub(super) fn note_element(&mut self, body: &Body<'a>, value: &Expr, needed: &Ty) {
        let Expr::Name { name, .. } = value else {
            return;
        };
        let element = match needed {
            Ty::List(args) => args.first(),
            Ty::Enum(e, args) if *e == self.defs.option => args.first(),
            _ => None,
        };
        fn open(t: &Ty) -> bool {
            matches!(t, Ty::Param(_) | Ty::Flex(_)) || t.args().iter().any(open)
        }
        let element = element.filter(|e| !open(e));
        let (Some(element), Some(at)) = (element, body.names.iter().rposition(|(n, _)| n == name))
        else {
            return;
        };
        if let Some(def) = self.inferred.get(&at).copied()
            && self
                .solve
                .as_ref()
                .is_some_and(|s| s.empties.contains(&def))
        {
            self.settle_element(def, &element.clone());
        }
    }

    /// The type of a value that is put in a list: the range that holds a number
    /// written out alone, and otherwise the range its type has, cut to what is
    /// known of it where it is.
    fn given_range(&mut self, body: &mut Body<'a>, value: &Expr) -> Ty {
        let gives = match literal(value) {
            Some(r) => self.expr(body, value, &Want::Ty(Ty::Int(r))),
            None => self.expr(body, value, &Want::Any),
        };
        let counted = !matches!(body.numbers, Numbers::Finding(_));
        match gives {
            Ty::Int(g) if literal(value).is_none() && counted => {
                match self.known_range(body, value).and_then(|(lo, hi)| {
                    Int::between(lo.max(g.min()), hi.min(g.max())).filter(|_| lo <= hi)
                }) {
                    Some(tight) => Ty::Int(tight),
                    None => Ty::Int(g),
                }
            }
            gives => gives,
        }
    }

    /// Checks `name = value`, where `name` is a `var` whose range is solved:
    /// the value gives it its range so far and the next line reads it.
    pub(super) fn infer_assign(
        &mut self,
        body: &mut Body<'a>,
        at: usize,
        (name, pos): (&str, Pos),
        value: &Expr,
    ) {
        let def = self.inferred[&at];
        if self
            .solve
            .as_ref()
            .is_some_and(|s| s.empties.contains(&def))
        {
            // `xs = push(xs, e)` puts `e` in the list: its elements are those it has
            // and the range of `e`.
            if let Expr::Call(call) = value
                && call.callee == "push"
                && let [list, item] = call.args.as_slice()
                && matches!(&list.value, Expr::Name { name: n, .. } if n == name)
            {
                let held = self.given_range(body, &item.value);
                self.settle_element(def, &held);
                self.assign(body, at, value);
                return;
            }
            // A case with a number written out as what it holds is that number.
            let want = match value {
                Expr::Case {
                    payload: Some(inside),
                    ..
                } if let Some(r) = literal(inside) => {
                    Want::Ty(Ty::Enum(self.defs.option, vec![Ty::Int(r)]))
                }
                _ => Want::Ty(body.names[at].1.clone()),
            };
            let gives = self.expr(body, value, &want);
            if let Ty::List(args) | Ty::Enum(_, args) = &gives
                && let Some(element) = args.first()
            {
                self.settle_element(def, &element.clone());
            }
            self.assign(body, at, value);
            return;
        }
        self.assigning = Some((def, false));
        let gives = match literal(value) {
            Some(r) => self.expr(body, value, &Want::Ty(Ty::Int(r))),
            None => self.expr(body, value, &Want::Any),
        };
        let (_, read_itself) = self.assigning.take().expect("assigning");
        let looped = body.loops > 0 || body.outer_loops > 0;
        let counted = !matches!(body.numbers, Numbers::Finding(_));
        // What a value that reads the `var` gives is a bound only where the
        // `var` was first given a number written out, as there is nothing
        // else to say what it holds.
        let written = self
            .solve
            .as_ref()
            .is_some_and(|s| s.written.contains(&def));
        // What is known of the value where it is given, as of a loop's counter,
        // is the range it has.
        let gives = match gives {
            Ty::Int(g) if literal(value).is_none() && counted => {
                match self.known_range(body, value).and_then(|(lo, hi)| {
                    Int::between(lo.max(g.min()), hi.min(g.max())).filter(|_| lo <= hi)
                }) {
                    Some(tight) => Ty::Int(tight),
                    None => Ty::Int(g),
                }
            }
            gives => gives,
        };
        match gives {
            Ty::Int(g) if !read_itself || written => {
                let text = format!("{name} = {}", written_expr(value));
                if let Some(solve) = self.solve.as_mut().filter(|_| counted) {
                    let held = solve.totals.get(&def).copied().unwrap_or(g);
                    solve.totals.insert(def, held.hull(g).unwrap_or(held));
                    solve.given.entry(def).or_default().push((pos, text, g));
                    if looped {
                        solve.in_loop.insert(def);
                    }
                }
                body.facts.set_step(at, g);
            }
            gives => {
                if read_itself && counted {
                    // The value depends on the `var` itself only where it would be
                    // known if the `var` had the range it has so far. Where it is
                    // not, something else in it is already refused.
                    let before = self.errors.len();
                    let known = self.expr(body, value, &Want::Any);
                    self.errors.truncate(before);
                    if matches!(known, Ty::Int(_))
                        && let Some(solve) = self.solve.as_mut()
                    {
                        solve.stuck.insert(def);
                        solve.stuck_at.entry(def).or_insert(pos);
                        if looped {
                            solve.in_loop.insert(def);
                        }
                    }
                    body.facts.unset_step(at);
                } else if !matches!(gives, Ty::Flex(_)) {
                    // A number that its uses have not decided is decided by them,
                    // and not by the range this `var` has so far.
                    let needed = body.names[at].1.clone();
                    self.expect(body, value, &gives, &needed);
                }
            }
        }
        self.assign(body, at, value);
    }

    /// The range the `var` at `at` holds where it is read, while ranges are
    /// solved: what the last assignment gave it, where nothing lies between,
    /// and otherwise what it has been given so far.
    pub(super) fn read_inferred(&mut self, body: &Body<'a>, at: usize) -> Option<Ty> {
        self.solve.as_ref()?;
        let def = self.inferred.get(&at)?;
        // What was last given is known only where the facts are kept, which is
        // not while the numbers are being found.
        let kept = !matches!(body.numbers, Numbers::Finding(_));
        if kept && let Some(held) = body.facts.step(at) {
            return Some(Ty::Int(held));
        }
        // Where the `var` is read to give itself a value, and nothing says
        // what it holds, what it holds is not known: another value in the
        // same call, or in the operator, says the type, and none refuses it.
        if let Some((assigning, _)) = self.assigning.as_mut()
            && assigning == def
        {
            let def = *def;
            self.assigning = Some((def, true));
            return Some(Ty::Unknown);
        }
        let solve = self.solve.as_ref()?;
        let so_far = solve.totals.get(def)?;
        let before = solve.before.get(def).copied().unwrap_or(*so_far);
        Some(Ty::Int(so_far.hull(before).unwrap_or(*so_far)))
    }

    /// Notes that a use needs `needed` of the `var` named by `value`.
    pub(super) fn note_need(&mut self, body: &Body<'a>, value: &Expr, needed: Int) {
        if self.solve.is_none() {
            return;
        }
        let Expr::Name { name, .. } = value else {
            return;
        };
        let Some(at) = body.names.iter().rposition(|(n, _)| n == name) else {
            return;
        };
        if let Some(def) = self.inferred.get(&at) {
            self.solved.needs.entry(*def).or_insert(needed);
        }
    }

    /// Says, in the error at `from`, which range a `var` was inferred to
    /// have, from what, and what the use needs.
    pub(super) fn explain_inferred(
        &mut self,
        body: &Body<'a>,
        value: &Expr,
        needed: Int,
        from: usize,
    ) {
        if self.errors.len() == from {
            return;
        }
        let Expr::Name { name, .. } = value else {
            return;
        };
        let Some(at) = body.names.iter().rposition(|(n, _)| n == name) else {
            return;
        };
        let Some(def) = self.inferred.get(&at).copied() else {
            return;
        };
        let (Ty::Int(held), Some(given)) = (&body.names[at].1, self.solved.given.get(&def)) else {
            return;
        };
        let spell = |r: Int| self.defs.text(&Ty::Int(r), &super::Scope::default());
        let mut sources: Vec<String> = given
            .iter()
            .map(|(p, text, _)| format!("`{text}` at line {}", p.line))
            .collect();
        sources.dedup();
        let sources = match sources.len() {
            0 | 1 => sources.concat(),
            2 => sources.join(" and "),
            n => format!("{}, and {}", sources[..n - 1].join(", "), sources[n - 1]),
        };
        let top = given
            .iter()
            .max_by(|a, b| a.2.max().cmp(&b.2.max()).then(b.0.cmp(&a.0)));
        let top = top.map_or(String::new(), |(p, text, _)| {
            format!(", and its top is set by `{text}` at line {}", p.line)
        });
        let message = format!(
            "`{name}` is a `{}`, inferred from {sources}{top}; the use at line {} needs a `{}`",
            spell(*held),
            value.pos().line,
            spell(needed)
        );
        self.errors[from].message = message;
    }

    /// The error for a `var` that is changed by itself, so that its range has
    /// not stopped growing.
    pub(super) fn grows_by_itself(&self, def: Pos, solve: &Solve) -> Error {
        let name = solve.names.get(&def).map_or("it", |n| n.as_str());
        let place = if solve.in_loop.contains(&def) {
            "in this loop"
        } else {
            "through a branch"
        };
        let range = match self.solved.needs.get(&def) {
            Some(r) => self.defs.text(&Ty::Int(*r), &super::Scope::default()),
            None => "a..b".to_string(),
        };
        let first = solve
            .given
            .get(&def)
            .and_then(|g| g.first())
            .map_or(String::new(), |(_, text, _)| text.clone());
        Error::new(
            solve.stuck_at.get(&def).copied().unwrap_or(def),
            format!("`{name}` is changed by itself {place}, so its range is not inferred yet",),
        )
        .with_fix(format!(
            "state the range, as `var {name}: {range} = {first}`"
        ))
    }
}
