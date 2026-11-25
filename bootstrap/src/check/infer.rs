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
//! A `var` that starts as a number written out, and that is given itself plus
//! a value or itself less a value, is solved by linear templates over the count
//! of times each such assignment runs, which is the product of the rounds of
//! the loops around it, as `templates` says. Its range is the range of what it
//! starts as and is otherwise given, widened by what those runs add or take
//! away, and is refused where that is not within 64 bits.
//!
//! The look that is kept gives each such `var` its range as if it were
//! written, so what `arith` proves, and what is built, is what stating that
//! range gives.

use std::collections::{HashMap, HashSet};

use super::proof::MAX_SIZE;
use super::{
    Body, Checker, Int, Numbers, Ty, Want, exprs, pattern_names, statements, written_expr,
};
use crate::diag::{Error, Pos};
use crate::parser::{ArithOp, Call, Expr, Over, Range, Stmt};

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
    /// The range of the first value of each `var`.
    pub(super) first: HashMap<Pos, Int>,
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
    /// The element type a use of each needs, which is the type of its elements
    /// where nothing is put in it.
    pub(super) wanted: HashMap<Pos, Ty>,
    /// What each was given: the place, how it is written, and the element.
    pub(super) put: HashMap<Pos, Vec<(Pos, String)>>,
    /// How many rounds each `loop` has, by its place.
    pub(super) passes: HashMap<Pos, Passes>,
    /// How many `loop`s were around each `var` where it is defined.
    pub(super) depth: HashMap<Pos, usize>,
    /// What each `var` that starts as a number written out is given by
    /// adding to itself or taking from itself, which a template solves.
    pub(super) adds: HashMap<Pos, Vec<Add>>,
    /// The `var`s whose adds do not make a range of 64 bits or less, with
    /// the one that does not.
    pub(super) unbounded: HashMap<Pos, Unbounded>,
}

/// How many rounds a `loop` has: at least, and at most where that is known
/// when the program is built.
pub(super) type Passes = (u128, Option<u128>);

/// One assignment of a `var` that adds a value to itself, or takes it away.
#[derive(Clone, Debug)]
pub(super) struct Add {
    pub(super) pos: Pos,
    pub(super) text: String,
    /// The range of what one pass adds.
    pub(super) low: i128,
    pub(super) high: i128,
    /// How many times at most it runs while the `var` lives, if that is known.
    pub(super) times: Option<u128>,
    /// Whether it takes away, and does not add.
    pub(super) takes: bool,
}

/// What stops the range of a `var` that adds to itself from being one.
#[derive(Clone, Debug)]
pub(super) struct Unbounded {
    pub(super) add: Add,
}

impl Solve {
    /// The element type of each list or option written with nothing: what is
    /// put in it, and where nothing is, what a use of it needs.
    pub(super) fn merged_elems(&self) -> HashMap<Pos, Ty> {
        self.empties
            .iter()
            .filter_map(|def| {
                // What a use needs is the type of the elements where it holds
                // everything that is put in, and otherwise what is put in is,
                // and the use says it needs more.
                let held = match (self.elems.get(def), self.wanted.get(def)) {
                    (Some(put), Some(need)) if joined(need, put).as_ref() == Some(need) => need,
                    (Some(put), _) => put,
                    (None, Some(need)) => need,
                    (None, None) => return None,
                };
                Some((*def, held.clone()))
            })
            .collect()
    }
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
    /// The `var`s whose range a template gave, whose assignments that add to
    /// themselves the look that is kept does not check against it, as the
    /// count of passes is what shows that they fit.
    pub(super) templated: HashSet<Pos>,
    /// The element type of each list or option written with nothing to say it.
    pub(super) elems: HashMap<Pos, Ty>,
    /// What each list or option written with nothing was given.
    pub(super) put: HashMap<Pos, Vec<(Pos, String)>>,
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

/// Whether a type is, or holds, a type that is not decided yet.
fn open(t: &Ty) -> bool {
    matches!(t, Ty::Param(_) | Ty::Flex(_)) || t.args().iter().any(open)
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
        let depth = self.rounds.len();
        let total = match self.solve.as_mut() {
            Some(solve) => {
                let before = solve.before.get(&pos).copied().unwrap_or(first);
                let total = before.hull(first).unwrap_or(first);
                if counted {
                    solve.depth.insert(pos, depth);
                    solve.totals.insert(pos, total);
                    solve.given.insert(pos, vec![(pos, text, first)]);
                    solve.names.insert(pos, name.to_string());
                    solve.first.insert(pos, first);
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
        }
        ty
    }

    /// The element type of the list or option `def` was given or used for.
    fn settle_element(&mut self, def: Pos, element: &Ty) {
        if element.unknown() || open(element) {
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

    /// What is wrong with a list written with nothing, whose elements are what
    /// was put in it, where the use needs other: what it was inferred from and
    /// where.
    pub(super) fn element_inferred(
        &self,
        body: &Body<'a>,
        name: &str,
        given: &str,
        needed: &str,
        at: Pos,
    ) -> Option<String> {
        let index = body.names.iter().rposition(|(n, _)| n == name)?;
        let def = self.inferred.get(&index)?;
        let put = self.solved.put.get(def).filter(|p| !p.is_empty())?;
        let mut from: Vec<String> = put
            .iter()
            .map(|(p, text)| format!("`{text}` at line {}", p.line))
            .collect();
        from.dedup();
        let from = match from.len() {
            1 => from.concat(),
            2 => from.join(" and "),
            n => format!("{}, and {}", from[..n - 1].join(", "), from[n - 1]),
        };
        Some(format!(
            "`{name}` is {given}, inferred from {from}; the use at line {} needs {needed}",
            at.line
        ))
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
            // A use needs the type of the elements, and says nothing of what is put in.
            if let Some(solve) = self.solve.as_mut() {
                solve.wanted.entry(def).or_insert_with(|| element.clone());
            }
        }
    }

    /// Checks `push(xs, e)` where `xs` is a list written with nothing and its
    /// elements are not known yet: they are the range of `e`, and the list is
    /// that of them.
    pub(super) fn push_to_empty(&mut self, body: &mut Body<'a>, call: &Call) -> Option<Ty> {
        let [list, item] = call.args.as_slice() else {
            return None;
        };
        let Expr::Name { name, .. } = &list.value else {
            return None;
        };
        let at = body.names.iter().rposition(|(n, _)| n == name.as_str())?;
        let def = self.inferred.get(&at).copied()?;
        if !self
            .solve
            .as_ref()
            .is_some_and(|s| s.empties.contains(&def))
            || !matches!(&body.names[at].1, Ty::List(of) if of[0].unknown())
        {
            return None;
        }
        let held = self.given_range(body, &item.value);
        let counted = !matches!(body.numbers, Numbers::Finding(_));
        if counted
            && !held.unknown()
            && !open(&held)
            && let Some(solve) = self.solve.as_mut()
        {
            solve
                .put
                .entry(def)
                .or_default()
                .push((call.pos, written_expr(&Expr::Call(call.clone()))));
        }
        self.settle_element(def, &held);
        (!held.unknown() && !open(&held)).then(|| Ty::List(vec![held]))
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
        if self.self_add(body, at, (name, pos), value) {
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
        if let Some(calls) = self.lambda_from.get(&at)
            && let Ty::Int(held) = &body.names[at].1
        {
            let spell = |r: Int| match r.ends() {
                Some((low, high)) => format!("{low}..{}", high + 1),
                None => r.name(),
            };
            let mut sources: Vec<String> = calls
                .iter()
                .map(|(p, text)| format!("the call `{text}` at line {}", p.line))
                .collect();
            sources.dedup();
            let sources = match sources.len() {
                0 | 1 => sources.concat(),
                2 => sources.join(" and "),
                n => format!("{}, and {}", sources[..n - 1].join(", "), sources[n - 1]),
            };
            let line = value.pos().line;
            let uses = match &self.using {
                Some(callee) => format!("`{callee}` at line {line}"),
                None => format!("the use at line {line}"),
            };
            self.errors[from].message = format!(
                "`{name}` is a `{}`, inferred from {sources}, and {uses} needs a `{}`",
                spell(*held),
                spell(needed)
            );
            self.errors[from].fix = Some(format!(
                "pass a value that fits, or write the type of `{name}`"
            ));
            return;
        }
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

    /// Whether `value` is `name` with something added or taken away, which a
    /// template solves.
    pub(super) fn adds_to(&self, name: &str, value: &Expr) -> bool {
        let Expr::Arith {
            op, left, right, ..
        } = value
        else {
            return false;
        };
        let is_name = |e: &Expr| matches!(e, Expr::Name { name: n, .. } if n == name);
        let reads = |e: &Expr| {
            exprs([e])
                .into_iter()
                .any(|e| matches!(e, Expr::Name { name: n, .. } if n == name))
        };
        match op {
            ArithOp::Add => (is_name(left) && !reads(right)) || (is_name(right) && !reads(left)),
            ArithOp::Sub => is_name(left) && !reads(right),
            _ => false,
        }
    }

    /// How many rounds the `loop` over the range `r` has: at most the
    /// greatest the end can be less the least the start can be, and at least
    /// the least the end can be less the greatest the start can be.
    pub(super) fn range_rounds(&mut self, body: &mut Body<'a>, r: &Range) -> Passes {
        let (Some((from_lo, from_hi)), Some((to_lo, to_hi))) = (
            self.known_range(body, &r.from),
            self.known_range(body, &r.to),
        ) else {
            return (0, None);
        };
        let count = |n: Option<i128>| u128::try_from(n.unwrap_or(0)).unwrap_or(0);
        (
            count(to_lo.checked_sub(from_hi)),
            to_hi.checked_sub(from_lo).map(|n| count(Some(n))),
        )
    }

    /// How many rounds the `loop` over `o`, of the type `given`, has: the size
    /// of an array, and for a `List`, a `str`, or a `Slice`, the bounds on its
    /// size, which is below 2 to the 48.
    pub(super) fn over_rounds(&mut self, body: &mut Body<'a>, o: &Over, given: &Ty) -> Passes {
        match given {
            Ty::Array(_, n) => (u128::from(*n), Some(u128::from(*n))),
            Ty::List(_) | Ty::Slice(_) | Ty::Text => {
                let (least, most) = self.known_size(body, &o.value).unwrap_or((0, MAX_SIZE));
                (
                    u128::try_from(least).unwrap_or(0),
                    Some(u128::try_from(most.min(MAX_SIZE)).unwrap_or(0)),
                )
            }
            _ => (0, None),
        }
    }

    /// Checks `name = name + e`, `name = e + name`, and `name = name - e`,
    /// where `name` is a `var` that starts as a number written out, and notes
    /// what one pass adds, which `templates` turns into a range. Where `e`
    /// reads `name`, or its range is not known, this is not such a value.
    fn self_add(
        &mut self,
        body: &mut Body<'a>,
        at: usize,
        (name, pos): (&str, Pos),
        value: &Expr,
    ) -> bool {
        let def = self.inferred[&at];
        let written = self
            .solve
            .as_ref()
            .is_some_and(|s| s.written.contains(&def));
        let Expr::Arith {
            op, left, right, ..
        } = value
        else {
            return false;
        };
        let is_name = |e: &Expr| matches!(e, Expr::Name { name: n, .. } if n == name);
        let (delta, negated) = match op {
            ArithOp::Add if is_name(left) => (&**right, false),
            ArithOp::Add if is_name(right) => (&**left, false),
            ArithOp::Sub if is_name(left) => (&**right, true),
            _ => return false,
        };
        let reads = exprs([delta])
            .into_iter()
            .any(|e| matches!(e, Expr::Name { name: n, .. } if n == name));
        if !written || reads || body.lambdas > 0 || body.outer_loops > 0 {
            return false;
        }
        let gives = match literal(delta) {
            Some(r) => self.expr(body, delta, &Want::Ty(Ty::Int(r))),
            None => self.expr(body, delta, &Want::Any),
        };
        let Ty::Int(g) = gives else {
            return false;
        };
        let counted = !matches!(body.numbers, Numbers::Finding(_));
        let (mut low, mut high) = (g.min(), g.max());
        if literal(delta).is_none()
            && counted
            && let Some((lo, hi)) = self.known_range(body, delta)
            && lo <= hi
        {
            (low, high) = (low.max(lo), high.min(hi));
        }
        if g.ends().is_none() {
            return false;
        }
        let (low, high) = if negated { (-high, -low) } else { (low, high) };
        let from = self
            .solve
            .as_ref()
            .and_then(|s| s.depth.get(&def))
            .copied()
            .unwrap_or(0);
        let times = self.rounds[from.min(self.rounds.len())..]
            .iter()
            .try_fold(1u128, |n, r| n.checked_mul((*r)?));
        if counted && let Some(solve) = self.solve.as_mut() {
            let looped = body.loops > 0;
            solve.adds.entry(def).or_default().push(Add {
                pos,
                text: format!("{name} = {}", written_expr(value)),
                low,
                high,
                times,
                takes: negated,
            });
            if looped {
                solve.in_loop.insert(def);
            }
        }
        body.facts.unset_step(at);
        true
    }

    /// The range of each `var` that adds to itself: the range that holds what
    /// it starts as and what it is given otherwise, and then, for each add,
    /// at most as many times as it runs, the least and the greatest of what
    /// they come to. This is the linear template `start + passes * low <= x`
    /// and `x <= start + passes * high`, which holds before the first pass and
    /// after each pass, as each pass adds between `low` and `high`.
    pub(super) fn templates(&self, solve: &mut Solve, body: &[Stmt]) {
        let defs: Vec<Pos> = solve.adds.keys().copied().collect();
        for def in defs {
            let adds = solve.adds[&def].clone();
            // A `var` that is given nothing but what it starts as, and adds to
            // itself, is solved from the net change of each pass of each loop
            // around the adds, in the order the body is read. Otherwise each
            // add counts for itself, as often as it runs.
            let only_adds = solve.given.get(&def).is_some_and(|g| g.len() == 1);
            let net = only_adds
                .then(|| {
                    let name = solve.names.get(&def)?;
                    let rest = after_define(body, def)?;
                    let walk = Walk {
                        name,
                        adds: &adds,
                        passes: &solve.passes,
                    };
                    walk.block(rest).flatten()
                })
                .flatten();
            let Some(base) = solve.given.get(&def).and_then(|g| {
                g.iter()
                    .map(|(_, _, r)| *r)
                    .reduce(|a, b| a.hull(b).unwrap_or(a))
            }) else {
                continue;
            };
            let Some((mut low, mut high)) = base.ends() else {
                continue;
            };
            let mut failed = None;
            if let Some(effect) = net {
                // The least and the greatest value any prefix of the changes gives.
                match (low.checked_add(effect.least), high.checked_add(effect.most)) {
                    (Some(l), Some(h)) => (low, high) = (l, h),
                    _ => failed = Some(adds[0].clone()),
                }
            }
            for add in adds.iter().filter(|_| net.is_none()) {
                let reach = |step: i128| -> Option<i128> {
                    let n = i128::try_from(add.times.or((step == 0).then_some(0))?).ok()?;
                    step.checked_mul(n)
                };
                let (down, up) = (reach(add.low), reach(add.high));
                match (down, up) {
                    (Some(down), Some(up)) => {
                        match (low.checked_add(down.min(0)), high.checked_add(up.max(0))) {
                            (Some(l), Some(h)) => (low, high) = (l, h),
                            _ => failed = failed.or(Some(add.clone())),
                        }
                    }
                    _ => failed = failed.or(Some(add.clone())),
                }
            }
            let range = Int::between(low, high).filter(|r| !r.wide());
            match (failed, range) {
                (None, Some(range)) => {
                    solve.totals.insert(def, range);
                    for add in &adds {
                        solve.given.entry(def).or_default().push((
                            add.pos,
                            add.text.clone(),
                            range,
                        ));
                    }
                }
                (failed, _) => {
                    let add = failed.unwrap_or_else(|| adds[0].clone());
                    solve.unbounded.insert(def, Unbounded { add });
                    solve.stuck.insert(def);
                    solve.stuck_at.entry(def).or_insert(adds[0].pos);
                }
            }
        }
    }

    /// The error for a `var` that adds to itself in a loop, where what it
    /// comes to is not a range of 64 bits or less.
    pub(super) fn not_finite(&self, def: Pos, solve: &Solve) -> Error {
        let name = solve.names.get(&def).map_or("it", |n| n.as_str());
        let Some(Unbounded { add }) = solve.unbounded.get(&def) else {
            return self.grows_by_itself(def, solve);
        };
        let step = self.defs.text(
            &Ty::Int(Int::between(add.low, add.high).unwrap_or(Int::I64)),
            &super::Scope::default(),
        );
        let passes = match add.times {
            Some(n) => format!("at most {n} times"),
            None => "a number of times that is not known when the program is built".to_string(),
        };
        let place = if solve.in_loop.contains(&def) {
            "in this loop"
        } else {
            "here"
        };
        Error::new(
            add.pos,
            format!(
                "`{name}` is changed by itself {place}, and its range is not finite within 64 bits: \
                 `{}` adds a `{step}`, {passes}",
                add.text
            ),
        )
        .with_fix(format!(
            "write `checked_{op}`, which gives an `Option` and costs one comparison, where it is \
             seen; or `wrapping_{op}`; or a `requires` that bounds the size",
            op = if add.takes { "sub" } else { "add" }
        ))
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
            format!("`{name}` is changed by itself {place}, so its range is not inferred",),
        )
        .with_fix(format!(
            "state the range, as `var {name}: {range} = {first}`"
        ))
    }
}

/// What a block does to a `var` that it adds to and takes from: the least and
/// the greatest change it can end with, and the least and the greatest that
/// any part of it, from its start, has made at any point.
#[derive(Clone, Copy, Debug)]
struct Effect {
    low: i128,
    high: i128,
    /// At most `0`: the empty part is one.
    least: i128,
    /// At least `0`.
    most: i128,
    /// Whether a `break` ends it, which a `loop` around it takes.
    breaks: bool,
}

impl Effect {
    const NONE: Effect = Effect {
        low: 0,
        high: 0,
        least: 0,
        most: 0,
        breaks: false,
    };

    /// One assignment that changes the `var` by `low..=high`, which may run
    /// or not.
    fn add(low: i128, high: i128) -> Effect {
        Effect {
            low,
            high,
            least: low.min(0),
            most: high.max(0),
            breaks: false,
        }
    }

    /// `self` and then `next`.
    fn then(self, next: Effect) -> Option<Effect> {
        Some(Effect {
            low: self.low.checked_add(next.low)?,
            high: self.high.checked_add(next.high)?,
            least: self.least.min(self.low.checked_add(next.least)?),
            most: self.most.max(self.high.checked_add(next.most)?),
            breaks: self.breaks || next.breaks,
        })
    }

    /// `self` or `other`, whichever runs.
    fn or(self, other: Effect) -> Effect {
        Effect {
            low: self.low.min(other.low),
            high: self.high.max(other.high),
            least: self.least.min(other.least),
            most: self.most.max(other.most),
            breaks: self.breaks || other.breaks,
        }
    }

    /// `self` run once in each round of a loop that has `passes`.
    fn repeated(self, (fewest, most): Passes) -> Option<Effect> {
        // A `break` ends a round where a part of it was done, and ends the loop.
        let (each, fewest) = if self.breaks {
            let widened = Effect {
                low: self.low.min(self.least),
                high: self.high.max(self.most),
                ..self
            };
            (widened, 0)
        } else {
            (self, fewest)
        };
        let times = |n: Option<u128>, step: i128| -> Option<i128> {
            match n {
                Some(n) => step.checked_mul(i128::try_from(n).ok()?),
                None if step == 0 => Some(0),
                None => None,
            }
        };
        let fewest = Some(fewest);
        // The rounds that run before the last one, which may be cut short.
        let before = most.map(|n| n.saturating_sub(1));
        Some(Effect {
            low: times(if each.low >= 0 { fewest } else { most }, each.low)?,
            high: times(if each.high >= 0 { most } else { fewest }, each.high)?,
            least: each.least.checked_add(times(before, each.low.min(0))?)?,
            most: each.most.checked_add(times(before, each.high.max(0))?)?,
            breaks: false,
        })
    }
}

/// Reads what a body does to the `var` `name`, from the statements that come
/// after the one that defines it.
struct Walk<'s> {
    name: &'s str,
    adds: &'s [Add],
    passes: &'s HashMap<Pos, Passes>,
}

impl Walk<'_> {
    /// What the statements do, one after another, or none where a number grew
    /// too large or the `var` is given a value other than itself changed.
    fn block(&self, stmts: &[Stmt]) -> Option<Option<Effect>> {
        let mut all = Effect::NONE;
        for stmt in stmts {
            let next = match stmt {
                Stmt::Define {
                    name, pos, value, ..
                } if name == self.name => {
                    let add = self.adds.iter().find(|a| a.pos == *pos)?;
                    self.expr(value)?.then(Effect::add(add.low, add.high))?
                }
                Stmt::Define { value, .. }
                | Stmt::Assign { value, .. }
                | Stmt::Destructure { value, .. }
                | Stmt::Expr(value) => self.expr(value)?,
            };
            all = all.then(next)?;
        }
        Some(Some(all))
    }

    fn expr(&self, e: &Expr) -> Option<Effect> {
        match e {
            Expr::Match(m) => {
                let mut arms = m.arms.iter().map(|a| self.block(&a.body).flatten());
                let mut any = arms.next().map_or(Some(Effect::NONE), |a| a)?;
                for arm in arms {
                    any = any.or(arm?);
                }
                self.expr(&m.of)?.then(any)
            }
            Expr::Loop(l) => {
                let body = self.block(&l.body).flatten()?;
                let passes = self.passes.get(&l.pos).copied().unwrap_or((0, None));
                body.repeated(passes)
            }
            Expr::Break { .. } => Some(Effect {
                breaks: true,
                ..Effect::NONE
            }),
            Expr::Call(call) => call
                .args
                .iter()
                .try_fold(Effect::NONE, |all, a| all.then(self.expr(&a.value)?)),
            Expr::Arith { left, right, .. } => self.expr(left)?.then(self.expr(right)?),
            Expr::Field { of, .. } => self.expr(of),
            Expr::Case {
                payload: Some(p), ..
            } => self.expr(p),
            Expr::Tuple { items, .. } | Expr::List { items, .. } => items
                .iter()
                .try_fold(Effect::NONE, |all, i| all.then(self.expr(i)?)),
            Expr::Index { of, index, .. } => self.expr(of)?.then(self.expr(index)?),
            Expr::Return { value: Some(v), .. } => self.expr(v),
            _ => Some(Effect::NONE),
        }
    }
}

/// The statements that follow the one at `def`, which defines a `var`, in the
/// block that holds it.
fn after_define(body: &[Stmt], def: Pos) -> Option<&[Stmt]> {
    for (i, stmt) in body.iter().enumerate() {
        if matches!(stmt, Stmt::Define { pos, .. } if *pos == def) {
            return Some(&body[i + 1..]);
        }
        let (Stmt::Define { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::Destructure { value, .. }
        | Stmt::Expr(value)) = stmt;
        let mut inside = Vec::new();
        super::blocks(value, &mut inside);
        if let Some(found) = inside.into_iter().find_map(|b| after_define(b, def)) {
            return Some(found);
        }
    }
    None
}
