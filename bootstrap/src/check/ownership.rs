//! Ownership: which types have `Copy` and `Discard`, and the check that a
//! value without `Copy` is used at most once, that a parameter marked `take`
//! owns its argument, and that a lent value is never kept.
//!
//! The check reads the body of a function after the rest of the checker is
//! done with it, in the order the body runs, and carries the places that
//! have been given away. It reads the type of every expression from what the
//! checker recorded, so it holds no types of its own.
//!
//! Nothing is known of a type parameter but what its bounds say, so it has
//! `Copy` and `Discard` only where a bound asks. A function has `Copy` for
//! now.

use super::*;
use crate::parser::{Param, is_operation};

/// Stands for `Copy` in `Defs::withouts`, where each trait is named by its
/// place among the file's traits and `Copy`, which the prelude does not
/// define, has none.
pub(super) const COPY: usize = usize::MAX;
/// Stands for `Discard`, in the same way.
pub(super) const DISCARD: usize = usize::MAX - 1;

/// Whether a type has the trait `which`, `COPY` or `DISCARD`: when no
/// `without` takes it from the type, each of its parts has it, and each type
/// parameter in it has it by a bound of `scope`. When it has not, `Err`
/// gives the first type parameter that lacks it, if that is why.
fn has(
    (defs, scope): (&Defs, &Scope),
    which: usize,
    ty: &Ty,
    open: &mut Vec<Ty>,
) -> Result<(), Option<usize>> {
    let parts: Vec<Ty> = match ty {
        Ty::Param(i) => {
            let flags = if which == COPY {
                &scope.copy
            } else {
                &scope.discard
            };
            return if flags.get(*i).copied().unwrap_or(true) {
                Ok(())
            } else {
                Err(Some(*i))
            };
        }
        Ty::Struct(s, _) if defs.withouts.contains(&(which, Def::Struct(*s))) => return Err(None),
        Ty::Enum(e, _) if defs.withouts.contains(&(which, Def::Enum(*e))) => return Err(None),
        Ty::Struct(s, args) => defs.fields[*s].iter().map(|t| t.subst(args)).collect(),
        Ty::Enum(e, args) => (defs.cases[*e].iter().flatten())
            .map(|t| t.subst(args))
            .collect(),
        // A list is never copied unasked, whatever its elements are.
        Ty::List(_) if which == COPY => return Err(None),
        Ty::Tuple(items) | Ty::List(items) => items.clone(),
        _ => return Ok(()),
    };
    // A type that holds itself, which is reported elsewhere, has what its
    // other parts have.
    if open.contains(ty) {
        return Ok(());
    }
    open.push(ty.clone());
    let mut all = Ok(());
    for t in &parts {
        match has((defs, scope), which, t, open) {
            Err(Some(i)) => {
                all = Err(Some(i));
                break;
            }
            Err(None) => all = Err(None),
            Ok(()) => {}
        }
    }
    open.pop();
    all
}

/// Whether a value of the type may be duplicated.
pub(super) fn copies(defs: &Defs, scope: &Scope, ty: &Ty) -> bool {
    has((defs, scope), COPY, ty, &mut Vec::new()).is_ok()
}

/// The type parameter, if there is one, that a type lacks `which` because of.
fn lacking(defs: &Defs, scope: &Scope, which: usize, ty: &Ty) -> Option<usize> {
    has((defs, scope), which, ty, &mut Vec::new())
        .err()
        .flatten()
}

/// Whether a value of the type may be dropped.
pub(crate) fn discards(defs: &Defs, scope: &Scope, ty: &Ty) -> bool {
    undiscardable(defs, scope, ty).is_none()
}

/// Why a type has no `Discard`: `None` when it has it, and otherwise the type
/// parameter that lacks it, if that is why.
fn undiscardable(defs: &Defs, scope: &Scope, ty: &Ty) -> Option<Option<usize>> {
    has((defs, scope), DISCARD, ty, &mut Vec::new()).err()
}

/// What follows `what is dropped, as nothing takes it`: which type cannot be
/// dropped, and the fix.
fn why_dropped(defs: &Defs, scope: &Scope, ty: &Ty, cause: Option<usize>) -> (String, String) {
    match cause {
        Some(p) => {
            let param = &scope.names[p];
            (
                format!(", and `{param}` has no `Discard`"),
                format!("write `{param}: Discard`"),
            )
        }
        None => (
            format!(
                ", and a `{}` cannot be dropped, as it has no `Discard`",
                defs.text(ty, scope)
            ),
            "give it to a function that takes it, or return it".to_string(),
        ),
    }
}

/// A name, and the fields after it that are read from it.
type Path = (String, Vec<String>);

/// A value the function owns and has not taken on some path, whose type has
/// no `Discard`, because of a type parameter or a `without`.
#[derive(Clone, Debug)]
struct Held {
    name: String,
    /// The fields after the name that the value is a part by: none for the
    /// whole of what the name holds, and the parts left when others were taken.
    fields: Vec<String>,
    pos: Pos,
    ty: Ty,
}

/// What is known at one point of the body: which places have been given
/// away, and where, and which values may not have been used yet. A path in
/// `gone` is a place whose value is gone, whole or in part.
#[derive(Clone, Debug, Default)]
struct Moved {
    gone: Vec<(Path, Pos)>,
    held: Vec<Held>,
    /// The names, on this path, that own a value that may hold a child task
    /// and have not handed it on.
    own: Vec<Own>,
}

/// A name that owns a value that may hold a child task, and where it was
/// bound, and how deep in the blocks it is.
#[derive(Clone, Debug)]
struct Own {
    name: String,
    pos: Pos,
    depth: usize,
    /// The fields of what it owns that were handed on, each as the names of
    /// the fields from the name, which it does not drop.
    handed: Vec<Vec<String>>,
}

impl PartialEq for Own {
    fn eq(&self, other: &Own) -> bool {
        (&self.name, self.pos, self.depth) == (&other.name, other.pos, other.depth)
    }
}

/// A value that is dropped: the part of a name that is `part` fields from it,
/// or the whole of it, but for the fields of that part in `except`, which were
/// handed on.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Dropped {
    pub(super) name: String,
    pub(super) part: Vec<String>,
    pub(super) except: Vec<Vec<String>>,
}

impl Dropped {
    fn whole(o: &Own) -> Dropped {
        Dropped {
            name: o.name.clone(),
            part: Vec::new(),
            except: o.handed.clone(),
        }
    }
}

/// What a name owns where paths meet that another path handed on or kept, and
/// this one did not, as what it must drop where it ends.
fn extras(moved: &Moved, kept: &[Own]) -> Vec<Dropped> {
    let mut all = Vec::new();
    for o in &moved.own {
        match kept.iter().find(|k| *k == o) {
            None => all.push(Dropped::whole(o)),
            Some(k) => {
                for part in &k.handed {
                    if o.handed.contains(part) {
                        continue;
                    }
                    let inside = o
                        .handed
                        .iter()
                        .filter(|h| h.starts_with(part))
                        .map(|h| h[part.len()..].to_vec())
                        .collect();
                    all.push(Dropped {
                        name: o.name.clone(),
                        part: part.clone(),
                        except: inside,
                    });
                }
            }
        }
    }
    all
}

/// A place in the function where names are dropped, by the address of the
/// statement, arm, loop, or expression it is written at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Site {
    /// Once the last statement of a block has run: the names that die with it.
    After(usize),
    /// Once the value of an assignment is made, before the name takes it: the
    /// value the name held.
    Replace(usize),
    /// Where a `break` or a `return` leaves, once its value is made.
    Leave(usize),
    /// At the end of an arm of a `match`.
    Arm(usize),
    /// Where a loop over a range ends because the range does.
    Done(usize),
    /// After a call, where the task may be asked to end and the function leaves
    /// at once: what the function owns and has not handed on.
    Cancel(usize),
    /// At the start of the work of a child, by the address of its lambda, where
    /// the task is asked to end before it ran: what the lambda was given and
    /// would have given away.
    Start(usize),
}

/// Where the values that may hold a child task are dropped, which the
/// lowering reads, as it knows which of them do.
#[derive(Clone, Debug, Default)]
pub(super) struct Drops {
    /// What is dropped at a site.
    pub(super) at: HashMap<Site, Vec<Dropped>>,
    /// The expressions whose value is made and then dropped, by address.
    pub(super) temps: HashSet<usize>,
    /// The loops over a value that is made where the loop is written, by
    /// address, which holds it in a name of its own while it runs.
    pub(super) held: HashSet<usize>,
}

/// The name a loop over a value that is not a name keeps it in.
pub(super) fn held_name(l: &Loop) -> String {
    format!("<over {}>", addr(l))
}

/// Whether a pattern binds a name.
fn binds_name(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Bind { .. } => true,
        Pattern::Case { inner, .. } => inner.as_deref().is_some_and(binds_name),
        Pattern::Tuple { items, .. } | Pattern::List { items, .. } => items.iter().any(binds_name),
        Pattern::Number { .. } | Pattern::Text { .. } => false,
    }
}

/// The names that own on every one of the paths.
fn intersection<'m>(states: impl Iterator<Item = &'m Moved>) -> Vec<Own> {
    let mut all: Option<Vec<Own>> = None;
    for moved in states {
        all = Some(match all {
            None => moved.own.clone(),
            Some(mut kept) => {
                kept.retain(|o| moved.own.contains(o));
                for o in &mut kept {
                    if let Some(other) = moved.own.iter().find(|m| *m == &*o) {
                        for part in &other.handed {
                            if !o.handed.contains(part) {
                                o.handed.push(part.clone());
                            }
                        }
                    }
                }
                kept
            }
        });
    }
    all.unwrap_or_default()
}

/// Whether a value of the type may hold a child task, which a type parameter
/// may be given.
fn may_hold(defs: &Defs, ty: &Ty, seen: &mut Vec<Ty>) -> bool {
    let parts: Vec<Ty> = match ty {
        Ty::Param(_) => return true,
        Ty::Struct(s, _) if Some(*s) == defs.task => return true,
        Ty::Struct(s, _) if defs.handles.iter().any(|(h, _)| h == s) => return true,
        Ty::Struct(s, _) if Some(*s) == defs.file_handle => return true,
        Ty::Struct(s, args) => defs.fields[*s].iter().map(|t| t.subst(args)).collect(),
        Ty::Enum(e, args) => (defs.cases[*e].iter().flatten())
            .map(|t| t.subst(args))
            .collect(),
        Ty::Tuple(items) | Ty::List(items) | Ty::Array(items, _) => items.clone(),
        _ => return false,
    };
    if seen.contains(ty) {
        return false;
    }
    seen.push(ty.clone());
    let any = parts.iter().any(|t| may_hold(defs, t, seen));
    seen.pop();
    any
}

/// The effects whose handles a value of the type holds: `net` for a connection,
/// a listener, or a socket, and `fs` for a `File`.
fn handle_effects(defs: &Defs, ty: &Ty) -> Vec<&'static str> {
    fn walk(defs: &Defs, ty: &Ty, seen: &mut Vec<Ty>, found: &mut Vec<&'static str>) {
        let parts: Vec<Ty> = match ty {
            Ty::Struct(s, _) if defs.handles.iter().any(|(h, _)| h == s) => {
                if !found.contains(&"net") {
                    found.push("net");
                }
                return;
            }
            Ty::Struct(s, _) if Some(*s) == defs.file_handle => {
                if !found.contains(&"fs") {
                    found.push("fs");
                }
                return;
            }
            Ty::Struct(s, args) => defs.fields[*s].iter().map(|t| t.subst(args)).collect(),
            Ty::Enum(e, args) => (defs.cases[*e].iter().flatten())
                .map(|t| t.subst(args))
                .collect(),
            Ty::Tuple(items) | Ty::List(items) | Ty::Array(items, _) => items.clone(),
            _ => return,
        };
        if seen.contains(ty) {
            return;
        }
        seen.push(ty.clone());
        parts.iter().for_each(|t| walk(defs, t, seen, found));
        seen.pop();
    }
    let mut found = Vec::new();
    walk(defs, ty, &mut Vec::new(), &mut found);
    found
}

/// Whether a value of the type holds a handle of `net`, which a type parameter
/// does not say.
fn holds_handle(defs: &Defs, ty: &Ty, seen: &mut Vec<Ty>) -> bool {
    let parts: Vec<Ty> = match ty {
        Ty::Struct(s, _) if defs.handles.iter().any(|(h, _)| h == s) => return true,
        Ty::Struct(s, _) if Some(*s) == defs.file_handle => return true,
        Ty::Struct(s, args) => defs.fields[*s].iter().map(|t| t.subst(args)).collect(),
        Ty::Enum(e, args) => (defs.cases[*e].iter().flatten())
            .map(|t| t.subst(args))
            .collect(),
        Ty::Tuple(items) | Ty::List(items) | Ty::Array(items, _) => items.clone(),
        _ => return false,
    };
    if seen.contains(ty) {
        return false;
    }
    seen.push(ty.clone());
    let any = parts.iter().any(|t| holds_handle(defs, t, seen));
    seen.pop();
    any
}

/// What a lambda that is being read defines or takes as a parameter, and the
/// names it gives away that it did not.
#[derive(Default)]
struct Frame {
    /// What was held where the lambda began, which its `return` does not drop.
    outer: Vec<(String, Pos)>,
    own: HashSet<String>,
    gives: Vec<(String, Pos, Option<Ty>)>,
    /// The names that are gone where each `return` of the lambda leaves it.
    leaves: Vec<Vec<String>>,
    /// How deep in the blocks the lambda's own names begin.
    depth: usize,
}

/// Whether the parameter that receives a lambda owns it.
#[derive(Clone, Copy, PartialEq)]
enum Kept {
    /// It is marked `take`, so the function is called once.
    Taken,
    /// It lends the function, so the function may be called any number of
    /// times.
    Lent,
    /// It is the body of a handler, or the work of a child, which runs exactly
    /// once, so what it gives away is given and not dropped.
    Called,
}

/// What lends a value, if it is lent: the function, or the lambda, whose
/// parameter it is.
type Lent = Option<String>;

/// The states of the paths that reach one point, or `None` where no path
/// does, as after a `break`.
type State = Option<Moved>;

/// Where a value goes when it is given away.
#[derive(Clone, Copy)]
enum Flow<'e> {
    /// It is only looked at.
    Lend,
    /// It is given a name, which owns it unless it was lent.
    Bind,
    /// It becomes part of a bigger value, which is stored.
    Stored,
    /// It is the result of the function or the lambda.
    Returned,
    /// It is the argument of a parameter marked `take` of the function.
    Taken(&'e str),
}

/// The most rounds a loop's body is read, which is far more than it needs.
const ROUNDS: usize = 1000;

/// Whether two states know the same: the same places gone, and the same
/// values held.
fn same(a: &State, b: &State) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let held = |x: &Moved, y: &Moved| {
                x.held.iter().all(|h| {
                    (y.held.iter())
                        .any(|g| g.name == h.name && g.pos == h.pos && g.fields == h.fields)
                })
            };
            let gone = |x: &Moved, y: &Moved| {
                x.gone
                    .iter()
                    .all(|(p, _)| y.gone.iter().any(|(q, _)| p == q))
            };
            held(a, b)
                && held(b, a)
                && gone(a, b)
                && gone(b, a)
                && a.own == b.own
                && (a.own.iter().zip(&b.own)).all(|(x, y)| x.handed == y.handed)
        }
        _ => false,
    }
}

/// The state where two paths meet: a place is gone if it is gone on either.
fn join(a: State, b: State) -> State {
    match (a, b) {
        (None, other) | (other, None) => other,
        (Some(mut a), Some(b)) => {
            a.own.retain(|o| b.own.contains(o));
            for o in &mut a.own {
                if let Some(other) = b.own.iter().find(|m| *m == &*o) {
                    for part in &other.handed {
                        if !o.handed.contains(part) {
                            o.handed.push(part.clone());
                        }
                    }
                }
            }
            for (path, at) in b.gone {
                if !a.gone.iter().any(|(p, _)| *p == path) {
                    a.gone.push((path, at));
                }
            }
            for h in b.held {
                if !a
                    .held
                    .iter()
                    .any(|g| g.name == h.name && g.pos == h.pos && g.fields == h.fields)
                {
                    a.held.push(h);
                }
            }
            Some(a)
        }
    }
}

struct Walk<'a> {
    defs: &'a Defs<'a>,
    types: &'a HashMap<usize, Ty>,
    f: &'a Function,
    scope: &'a Scope,
    errors: &'a mut Vec<Error>,
    /// The places already reported, so that the second look at a loop's body
    /// does not report a mistake twice.
    reported: HashSet<Pos>,
    /// The names that are lent, each with what lends it, for as long as they
    /// mean what they did when they were bound.
    lent: HashMap<String, String>,
    /// The states at each `break` of the loops being read, innermost last,
    /// each with the address of the `break`, and how deep the loop's block is.
    breaks: Vec<(usize, Vec<(Moved, usize)>)>,
    /// The lambdas being read, innermost last.
    inside: Vec<Frame>,
    /// The names that were used after they were given away.
    reused: HashSet<String>,
    /// For a name bound to a lambda that reads names of its function, the names
    /// it holds, which it lends for as long as it can be called.
    holding: HashMap<String, Vec<String>>,
    /// The lambda whose call reads the names it holds.
    via: Option<String>,
    /// Where the values that may hold a child are dropped.
    drops: &'a mut Drops,
    /// How deep in the blocks, the arms, and the lambdas the walk is.
    depth: usize,
    /// Where each name that owned a value that may hold a child was bound,
    /// for the names a `mut` is assigned to again.
    bound: HashMap<String, (Pos, usize)>,
    /// The names bound to a value that holds a handle of `net`, with the type as
    /// it is written, which the exit of a task that is asked to end closes.
    handles: HashMap<String, (String, Vec<&'static str>)>,
    /// The name a loop's element stands for, which what is read of the element
    /// is read of.
    alias: HashMap<String, String>,
    /// Whether the expression read next is a part of a value that is made.
    projecting: bool,
    /// The names that can be used where the walk is, with how deep each is
    /// bound, so that a name that is bound again, as a `mut` is, keeps the
    /// place it was bound at.
    visible: Vec<(String, usize)>,
    /// The names that were given on through the element of a loop over them.
    consumed: HashSet<String>,
    /// The name the loop read next is given, where it begins, to the loop.
    consume: Option<String>,
    /// The parameters marked `mut`, which the function gives back and does
    /// not give away.
    changed: HashSet<String>,
    /// The places of the expressions that give back what a parameter marked
    /// `mut` holds: what the function gives, and what a call that changes a
    /// value is passed.
    given_back: HashSet<usize>,
}

/// The places of the expressions of `f` that give back a value a parameter
/// marked `mut` holds: the end of the body and each `return` of a function
/// that has one, and each argument passed `mut`.
fn given_back(f: &Function) -> HashSet<usize> {
    let mut all = Vec::new();
    for value in values(&f.body) {
        walk(value, &mut all);
    }
    let mut given = Vec::new();
    let changes = f.params.iter().any(|p| p.mutable);
    if changes && let Some(Stmt::Expr(last)) = f.body.last() {
        given.push(last);
    }
    for e in &all {
        match e {
            Expr::Return { value: Some(v), .. } if changes => given.push(v),
            Expr::Call(call) => {
                given.extend(
                    (call.args.iter())
                        .filter(|a| a.given_back)
                        .map(|a| &a.value),
                );
            }
            _ => {}
        }
    }
    let mut out = HashSet::new();
    for e in given {
        out.insert(addr(e));
        if let Expr::Tuple { items, .. } = e {
            out.extend(items.iter().map(addr));
        }
    }
    out
}

/// Checks the body of `f`. Mistakes go in `errors`.
pub(super) fn check(
    defs: &Defs,
    types: &HashMap<usize, Ty>,
    (f, scope, params): (&Function, &Scope, &[(String, Ty)]),
    errors: &mut Vec<Error>,
    drops: &mut Drops,
) {
    let mut walk = Walk {
        defs,
        types,
        f,
        scope,
        errors,
        reported: HashSet::new(),
        lent: (f.params.iter().filter(|p| !p.take))
            .map(|p| (p.name.clone(), format!("`{}`", f.name)))
            .collect(),
        breaks: Vec::new(),
        inside: Vec::new(),
        reused: HashSet::new(),
        holding: HashMap::new(),
        via: None,
        drops,
        depth: 0,
        bound: HashMap::new(),
        handles: HashMap::new(),
        alias: HashMap::new(),
        projecting: false,
        visible: Vec::new(),
        consumed: HashSet::new(),
        consume: None,
        changed: (f.params.iter().filter(|p| p.mutable))
            .map(|p| p.name.clone())
            .collect(),
        given_back: given_back(f),
    };
    let flow = if f.result.is_some() {
        Flow::Returned
    } else {
        Flow::Lend
    };
    let mut st = Some(Moved::default());
    for p in f.params.iter().filter(|p| p.take) {
        if let Some((_, ty)) = params.iter().find(|(n, _)| *n == p.name) {
            walk.own(&mut st, &p.name, p.pos, ty);
        }
    }
    walk.block(&mut st, &f.body, flow);
    // What the function owns that it did not hand on ends with it.
    let dying = walk.dying(&mut st, 0);
    if let Some(last) = f.body.last() {
        walk.record(Site::After(addr(last)), dying);
    }
    walk.dropped(&st);
}

fn written((name, fields): &Path) -> String {
    std::iter::once(name.as_str())
        .chain(fields.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(".")
}

/// Whether one path is the other or a part of it.
fn overlap(a: &Path, b: &Path) -> bool {
    a.0 == b.0 && a.1.iter().zip(&b.1).all(|(x, y)| x == y)
}

impl<'a> Walk<'a> {
    /// The name and fields an expression reads, if it reads a place. A name
    /// that is no value, as the enum in `Pet.none` is not, is no place.
    fn place(&self, e: &Expr) -> Option<Path> {
        match e {
            Expr::Name { name, .. } => self
                .types
                .contains_key(&addr(e))
                .then(|| (name.clone(), Vec::new())),
            Expr::Field { of, name, .. } => {
                let (root, mut fields) = self.place(of)?;
                fields.push(name.clone());
                Some((root, fields))
            }
            _ => None,
        }
    }

    /// The parameter of the function that a path is, if it is of a function
    /// type.
    fn fn_param(&self, path: &Path) -> Option<&'a Param> {
        let f: &'a Function = self.f;
        let own = path.1.is_empty() && self.inside.iter().all(|fr| !fr.own.contains(&path.0));
        (own.then(|| f.params.iter().find(|p| p.name == path.0 && p.ty.function))).flatten()
    }

    fn copies(&self, e: &Expr) -> bool {
        (self.types.get(&addr(e))).is_none_or(|ty| copies(self.defs, self.scope, ty))
    }

    /// Whether a value of the type may hold a child task.
    fn holds(&self, ty: &Ty) -> bool {
        may_hold(self.defs, ty, &mut Vec::new())
    }

    /// Notes that the names own values that may hold a child task, which they
    /// drop where they end if they have not handed them on.
    fn record(&mut self, site: Site, items: Vec<Dropped>) {
        if items.is_empty() {
            return;
        }
        let list = self.drops.at.entry(site).or_default();
        for item in items {
            if !list.contains(&item) {
                list.push(item);
            }
        }
    }

    /// Leaves the block, the arm, or the lambda the walk is in: the names bound
    /// in it can be used no more.
    fn leave(&mut self) {
        self.depth -= 1;
        let depth = self.depth;
        self.visible.retain(|(_, d)| *d <= depth);
    }

    /// Takes the names that own a value and are at least `depth` deep out of
    /// the state, as they end.
    fn dying(&mut self, st: &mut State, depth: usize) -> Vec<Dropped> {
        let mut dropped = Vec::new();
        if let Some(moved) = st {
            moved.own.retain(|o| {
                let dies = o.depth >= depth;
                if dies {
                    dropped.push(Dropped::whole(o));
                }
                !dies
            });
        }
        dropped
    }

    /// Notes that a name was bound to a value that may hold a child task, and
    /// owns it unless it is lent.
    fn track(&mut self, st: &mut State, name: &str, pos: Pos, ty: &Ty) {
        if self.lent.contains_key(name) || !self.holds(ty) {
            return;
        }
        if holds_handle(self.defs, ty, &mut Vec::new()) {
            self.handles.insert(
                name.to_string(),
                (
                    self.defs.text(ty, self.scope),
                    handle_effects(self.defs, ty),
                ),
            );
        } else {
            self.handles.remove(name);
        }
        // A name that is bound again keeps the place it was first bound at.
        let outer = (self.visible.iter())
            .find(|(n, depth)| n == name && *depth < self.depth)
            .and_then(|_| self.bound.get(name).copied());
        let (pos, depth) = outer.unwrap_or((pos, self.depth));
        self.bound.insert(name.to_string(), (pos, depth));
        if let Some(moved) = st {
            moved.own.retain(|o| o.name != name);
            moved.own.push(Own {
                name: name.to_string(),
                pos,
                depth,
                handed: Vec::new(),
            });
        }
    }

    /// Notes that a value was bound that must be taken, if its type has no
    /// `Discard`.
    fn own(&mut self, st: &mut State, name: &str, pos: Pos, ty: &Ty) {
        self.track(st, name, pos, ty);
        if self.lent.contains_key(name) || undiscardable(self.defs, self.scope, ty).is_none() {
            return;
        }
        if let Some(moved) = st {
            let held = Held {
                name: name.to_string(),
                fields: Vec::new(),
                pos,
                ty: ty.clone(),
            };
            moved.held.retain(|h| h.name != name);
            moved.held.push(held);
        }
    }

    /// Reports each value that may be dropped where `st` is, as the function
    /// ends there, or as the lambda that returns does, which drops only what
    /// it holds itself.
    fn dropped(&mut self, st: &State) {
        let Some(moved) = st else {
            return;
        };
        let outer = self.inside.last().map(|f| f.outer.clone());
        for h in &moved.held {
            if outer
                .as_ref()
                .is_some_and(|o| o.iter().any(|(n, p)| *n == h.name && *p == h.pos))
            {
                continue;
            }
            let Some(cause) = undiscardable(self.defs, self.scope, &h.ty) else {
                continue;
            };
            let (why, fix) = why_dropped(self.defs, self.scope, &h.ty, cause);
            let place = written(&(h.name.clone(), h.fields.clone()));
            // `_ =` is no name, so it says what it was written to do.
            let message = if h.name == "_" && h.fields.is_empty() {
                let ty = self.defs.text(&h.ty, self.scope);
                format!("`_ =` throws this `{ty}` away{why}")
            } else {
                format!("`{place}` is dropped here, as nothing takes it{why}")
            };
            if let Some(error) = self.error(h.pos, message) {
                error.fix = Some(fix);
            }
        }
    }

    /// Reports a value that nothing uses, as the result of a call that is
    /// a statement of its own is, when its type has no `Discard`.
    fn temporary(&mut self, e: &Expr) {
        let made = matches!(
            e,
            Expr::Call(_) | Expr::Tuple { .. } | Expr::List { .. } | Expr::Case { .. }
        );
        if !made {
            return;
        }
        let Some(ty) = self.types.get(&addr(e)) else {
            return;
        };
        let Some(cause) = undiscardable(self.defs, self.scope, ty) else {
            return;
        };
        let (why, fix) = why_dropped(self.defs, self.scope, ty, cause);
        let message = format!("this value is dropped, as nothing takes it{why}");
        if let Some(error) = self.error(e.pos(), message) {
            error.fix = Some(fix);
        }
    }

    /// Whether a value that is read by fields is made where it is read, as
    /// the result of a call, and so is bound to no name.
    fn made_root(e: &Expr) -> bool {
        match e {
            Expr::Field { of, .. } => Self::made_root(of),
            e => matches!(
                e,
                Expr::Call(_) | Expr::Tuple { .. } | Expr::List { .. } | Expr::Case { .. }
            ),
        }
    }

    /// Reports each other field of the value `of`, which is not bound to a name,
    /// that has no `Discard`, as the field `name` is taken and what is left of
    /// the value is dropped.
    fn left_of_field(&mut self, of: &Expr, name: &str) {
        let Some(Ty::Struct(s, args)) = self.types.get(&addr(of)).cloned() else {
            return;
        };
        let fields = &self.defs.file.structs[s].fields;
        let taken = fields
            .iter()
            .zip(&self.defs.fields[s])
            .find(|(f, _)| f.name == name)
            .map(|(_, t)| t.subst(&args));
        // A field that is duplicated is read and not taken, so the whole of
        // the value is dropped, as one that only a lend reads is.
        if taken.is_none_or(|t| copies(self.defs, self.scope, &t)) {
            let whole = Ty::Struct(s, args);
            if let Some(cause) = undiscardable(self.defs, self.scope, &whole) {
                let (why, fix) = why_dropped(self.defs, self.scope, &whole, cause);
                let message = format!("this value is dropped, as nothing takes it{why}");
                if let Some(error) = self.error(of.pos(), message) {
                    error.fix = Some(fix);
                }
            }
            return;
        }
        for (f, t) in fields.iter().zip(&self.defs.fields[s]) {
            if f.name == name {
                continue;
            }
            let t = t.subst(&args);
            let Some(cause) = undiscardable(self.defs, self.scope, &t) else {
                continue;
            };
            let (why, fix) = why_dropped(self.defs, self.scope, &t, cause);
            let message = format!(
                "this value is dropped, as nothing takes its field `{}`{why}",
                f.name
            );
            if let Some(error) = self.error(of.pos(), message) {
                error.fix = Some(fix);
            }
        }
    }

    /// Notes each name a pattern binds to a part of a value of type `ty`.
    fn own_pattern(&mut self, st: &mut State, pattern: &Pattern, ty: &Ty) {
        match (pattern, ty) {
            (Pattern::Bind { name, pos, .. }, ty) => self.own(st, name, *pos, ty),
            (
                Pattern::Case {
                    name,
                    inner: Some(inner),
                    ..
                },
                Ty::Enum(e, args),
            ) => {
                if let Some(c) = self.defs.case_of(*e, name)
                    && let Some(held) = self.defs.cases[*e][c].as_ref()
                {
                    self.own_pattern(st, inner, &held.subst(args));
                }
            }
            (Pattern::Tuple { items, .. }, Ty::Tuple(tys)) => {
                for (p, t) in items.iter().zip(tys) {
                    self.own_pattern(st, p, t);
                }
            }
            (Pattern::List { items, .. }, Ty::List(of)) => {
                for p in items {
                    self.own_pattern(st, p, &of[0]);
                }
            }
            _ => {}
        }
    }

    /// What is left of `h` to take when its part at `fields` is taken: each
    /// other field, along the way to it, whose type has no `Discard`.
    fn leftover(&self, h: &Held, fields: &[String]) -> Vec<Held> {
        let mut left = Vec::new();
        let mut at = h.fields.clone();
        let mut ty = h.ty.clone();
        for field in &fields[h.fields.len()..] {
            let Ty::Struct(s, args) = &ty else {
                break;
            };
            let names = &self.defs.file.structs[*s].fields;
            let mut inner = None;
            for (f, t) in names.iter().zip(&self.defs.fields[*s]) {
                let t = t.subst(args);
                if f.name == *field {
                    inner = Some(t);
                } else if undiscardable(self.defs, self.scope, &t).is_some() {
                    let mut of = at.clone();
                    of.push(f.name.clone());
                    left.push(Held {
                        name: h.name.clone(),
                        fields: of,
                        pos: h.pos,
                        ty: t,
                    });
                }
            }
            let Some(inner) = inner else {
                break;
            };
            at.push(field.clone());
            ty = inner;
        }
        left
    }

    fn error(&mut self, pos: Pos, message: String) -> Option<&mut Error> {
        if !self.reported.insert(pos) {
            return None;
        }

        self.errors.push(Error::new(pos, message));
        self.errors.last_mut()
    }

    /// Reads a place. Whether the value was lent is what it gives.
    fn read(&mut self, st: &mut State, e: &Expr, path: Path, flow: Flow) -> Lent {
        let Some(moved) = st else {
            return None;
        };
        let lent = self.lent.get(&path.0).cloned();
        // A value that is read to be given on is the one it is given to that
        // drops it, so a name that holds a child hands it on.
        // A field of a child is read to act on the child, as `join` does with
        // its number, so that reads it too.
        let of_child = matches!(e, Expr::Field { of, .. }
            if matches!(self.types.get(&addr(&**of)), Some(Ty::Struct(s, _)) if Some(*s) == self.defs.task));
        if !matches!(flow, Flow::Lend) || of_child {
            let aliased = self.alias.contains_key(&path.0);
            let root = self.alias.get(&path.0).unwrap_or(&path.0).clone();
            if aliased {
                self.consumed.insert(root.clone());
            }
            if path.1.is_empty() || of_child || aliased {
                moved.own.retain(|o| o.name != root && o.name != path.0);
            } else if let Some(o) = moved.own.iter_mut().find(|o| o.name == root) {
                // A field that is given on is not dropped with the rest.
                let covered = o.handed.iter().any(|h| path.1.starts_with(h));
                if !covered {
                    o.handed.retain(|h| !h.starts_with(&path.1));
                    o.handed.push(path.1.clone());
                }
            }
        }
        // A value is taken where a value without `Copy` would be given away:
        // looking at it, lending it, and reading a part of it take nothing.
        // A part that has no `Discard` is taken, and each other part of what
        // holds it that has none is left to be taken.
        let part = (!path.1.is_empty() && !matches!(flow, Flow::Lend))
            .then(|| self.types.get(&addr(e)))
            .flatten()
            .is_some_and(|ty| undiscardable(self.defs, self.scope, ty).is_some());
        if !matches!(flow, Flow::Lend) && path.1.is_empty() {
            moved.held.retain(|h| h.name != path.0);
        }
        if part && !moved.gone.iter().any(|(p, _)| overlap(p, &path)) {
            let mut parts = Vec::new();
            moved.held.retain(|h| {
                let covers = h.name == path.0 && path.1.starts_with(&h.fields);
                if covers {
                    parts.push(h.clone());
                }
                !covers
            });
            for h in parts {
                moved.held.extend(self.leftover(&h, &path.1));
            }
        }
        if let Some((_, at)) = moved.gone.iter().find(|(p, _)| overlap(p, &path)) {
            let holder = (self.via.as_ref())
                .map(|f| format!(", and `{f}` holds it"))
                .unwrap_or_default();
            let message = format!(
                "`{}` is used here, after it was given away at {}:{}{holder}",
                written(&path),
                at.line,
                at.col
            );
            let ty = self.types.get(&addr(e));
            let blame = ty.and_then(|ty| lacking(self.defs, self.scope, COPY, ty));
            let duplicable = match ty {
                Some(Ty::List(of)) => copies(self.defs, self.scope, &of[0]),
                _ => false,
            };
            let fix = match blame {
                Some(p) => Some(format!("write `{}: Copy`", self.scope.names[p])),
                None if duplicable => Some(format!(
                    "write `copy({})` where two lists are meant",
                    written(&path)
                )),
                None => None,
            };
            self.reused.insert(path.0.clone());
            if let Some(error) = self.error(e.pos(), message) {
                error.fix = fix;
            }
            return lent;
        }
        // A parameter marked `mut` is given back when the function returns,
        // so it is not given away, nor is a part of it that has no `Copy`.
        if self.changed.contains(&path.0)
            && !matches!(flow, Flow::Lend)
            && !self.given_back.contains(&addr(e))
            && !self.copies(e)
        {
            let what = match flow {
                Flow::Bind => "bound to another name".to_string(),
                Flow::Returned => "returned".to_string(),
                Flow::Stored => "stored".to_string(),
                Flow::Taken(callee) => format!("passed to the `take` parameter of `{callee}`"),
                Flow::Lend => unreachable!("checked"),
            };
            let whole = if path.1.is_empty() {
                format!("`{}` is marked `mut`", path.0)
            } else {
                format!(
                    "`{}` is a part of `{}`, which is marked `mut`",
                    written(&path),
                    path.0
                )
            };
            let f = written_name(&self.f.name);
            self.error(
                e.pos(),
                format!("{whole}, so `{f}` gives it back when it returns, and it cannot be {what}"),
            );
            return lent;
        }
        // A function that is called once has no `Copy`. A lent function has,
        // but it is not given to a parameter that takes its function.
        let function = self.fn_param(&path);
        let once = function.is_some_and(|p| p.take);
        let lent_function = function.is_some_and(|p| !p.take) && matches!(flow, Flow::Taken(_));
        if once && matches!(flow, Flow::Lend) {
            let message = format!(
                "`{}` is given to a parameter that may call it more than once, and it can be called once",
                path.0
            );
            if let Some(moved) = st {
                moved.held.retain(|h| h.name != path.0);
            }
            if let Some(error) = self.error(e.pos(), message) {
                error.fix =
                    Some("write `take` before the function type of that parameter".to_string());
            }
            return lent;
        }
        if !once && !lent_function && self.copies(e) {
            return lent;
        }
        // A lambda gives away what it did not define or take, and the
        // lambda is spent by its call, which the parameter it is passed to
        // must say.
        if lent.is_none()
            && !matches!(flow, Flow::Lend)
            && let Some(frame) = self.inside.last_mut()
            && !frame.own.contains(&path.0)
        {
            frame
                .gives
                .push((path.0.clone(), e.pos(), self.types.get(&addr(e)).cloned()));
        }
        match flow {
            Flow::Lend => {}
            Flow::Bind if lent.is_some() => {}
            Flow::Bind => moved.gone.push((path, e.pos())),
            _ if lent.is_some() => {
                let by = lent.clone().unwrap_or_default();
                let what = match flow {
                    Flow::Returned => "returned".to_string(),
                    Flow::Stored => "stored".to_string(),
                    Flow::Taken(callee) => {
                        format!("passed to the `take` parameter of `{callee}`")
                    }
                    Flow::Lend | Flow::Bind => unreachable!("handled above"),
                };
                let whole = if path.1.is_empty() {
                    format!("`{}` is lent to {by}", path.0)
                } else {
                    format!(
                        "`{}` is a part of `{}`, which is lent to {by}",
                        written(&path),
                        path.0
                    )
                };
                let fix = if by.starts_with("this lambda") {
                    format!(
                        "write `take` before the type of `{}` in the function type",
                        path.0
                    )
                } else {
                    format!("write `take` before the type of `{}`", path.0)
                };
                if let Some(error) = self.error(
                    e.pos(),
                    format!("{whole}, and a lent value cannot be {what}"),
                ) && by != "this `loop`"
                {
                    error.fix = Some(fix);
                }
            }
            _ => moved.gone.push((path, e.pos())),
        }
        lent
    }

    /// Reads an expression, whose value goes where `flow` says. It gives
    /// whether the value is lent, which a binding keeps.
    fn eval(&mut self, st: &mut State, e: &Expr, flow: Flow) -> Lent {
        if st.is_none() {
            return None;
        }
        if let Some(path) = self.place(e) {
            // A lambda that is bound and stored or returned by its name gives away
            // the values it holds, as one that is stored or returned where it is
            // written does.
            if matches!(e, Expr::Name { .. })
                && !matches!(flow, Flow::Lend | Flow::Bind)
                && let Some(held) = self.holding.get(&path.0).cloned()
            {
                for name in held {
                    let all: Vec<&Expr> = {
                        let mut all = Vec::new();
                        for value in values(&self.f.body) {
                            super::walk(value, &mut all);
                        }
                        all
                    };
                    let read = all
                        .into_iter()
                        .find(|e| matches!(e, Expr::Name { name: n, .. } if *n == name));
                    if let Some(read) = read {
                        self.read(st, read, (name, Vec::new()), flow);
                    }
                }
            }
            return self.read(st, e, path, flow);
        }
        let projected = std::mem::take(&mut self.projecting);
        // A value that is made and only looked at is dropped.
        if matches!(flow, Flow::Lend) {
            self.temporary(e);
            let made = matches!(
                e,
                Expr::Call(_) | Expr::Tuple { .. } | Expr::List { .. } | Expr::Case { .. }
            );
            if made && !projected && self.types.get(&addr(e)).is_some_and(|ty| self.holds(ty)) {
                self.drops.temps.insert(addr(e));
            }
        }
        match e {
            Expr::Text { .. } | Expr::Number { .. } | Expr::Float { .. } | Expr::Unbuilt { .. } => {
                None
            }
            Expr::Interp { parts, .. } => {
                for part in parts {
                    if let Part::Expr(value) = part {
                        self.eval(st, value, Flow::Lend);
                    }
                }
                None
            }
            Expr::Name { .. } => None,
            Expr::Field { of, name, .. } => {
                let made = Self::made_root(of);
                self.projecting = true;
                self.eval(
                    st,
                    of,
                    if matches!(**of, Expr::Field { .. }) {
                        Flow::Lend
                    } else {
                        Flow::Stored
                    },
                );
                self.projecting = false;
                if made {
                    self.left_of_field(of, name);
                }
                None
            }
            Expr::Call(call) => {
                // A call of a lambda that holds names reads them, so they are
                // not given away while it can still be called.
                if call.qualifier.is_none()
                    && let Some(held) = self.holding.get(&call.callee).cloned()
                {
                    self.via = Some(call.callee.clone());
                    for name in held {
                        let read = Expr::Name {
                            name: name.clone(),
                            pos: call.name_pos,
                        };
                        self.read(st, &read, (name, Vec::new()), Flow::Lend);
                    }
                    self.via = None;
                }
                self.call(st, call);
                None
            }
            Expr::Case { payload, .. } => {
                if let Some(payload) = payload {
                    self.eval(st, payload, Flow::Stored);
                }
                None
            }
            Expr::Match(m) => self.matching(st, m, flow),
            Expr::Arith { left, right, .. } => {
                self.eval(st, left, Flow::Lend);
                self.eval(st, right, Flow::Lend);
                None
            }
            Expr::Tuple { items, .. } | Expr::List { items, .. } => {
                for item in items {
                    self.eval(st, item, Flow::Stored);
                }
                None
            }
            Expr::Index { of, index, .. } => {
                self.projecting = true;
                self.eval(st, of, Flow::Lend);
                self.projecting = false;
                self.eval(st, index, Flow::Lend);
                None
            }
            Expr::Slice { of, from, to, .. } => {
                self.projecting = true;
                self.eval(st, of, Flow::Lend);
                self.projecting = false;
                for end in [from, to].into_iter().flatten() {
                    self.eval(st, end, Flow::Lend);
                }
                None
            }
            Expr::Lambda(l) => {
                self.lambda(st, l, &[], (Kept::Lent, false));
                // A lambda that is stored or returned takes the values of what it
                // reads, as a value that is stored or returned does, so a value
                // without `Copy` is given away and one that is lent is refused.
                if let Some(s) = self.defs.env_of(l)
                    && !matches!(flow, Flow::Bind)
                {
                    let mut all = Vec::new();
                    for value in values(&l.body) {
                        super::walk(value, &mut all);
                    }
                    for field in &self.defs.file.structs[s].fields {
                        let read = all
                            .iter()
                            .find(|e| matches!(e, Expr::Name { name, .. } if *name == field.name));
                        if let Some(read) = read {
                            self.read(st, read, (field.name.clone(), Vec::new()), flow);
                        }
                    }
                }
                None
            }
            Expr::Loop(l) => {
                self.looping(st, l);
                None
            }
            Expr::Break { .. } => {
                // What the loop's block owns ends where it is left.
                let inside = self.breaks.last().map(|(depth, _)| *depth);
                if let Some(depth) = inside {
                    let dying = self.dying(st, depth);
                    self.record(Site::Leave(addr(e)), dying);
                }
                if let (Some(moved), Some((_, exits))) = (st.take(), self.breaks.last_mut()) {
                    exits.push((moved, addr(e)));
                }
                None
            }
            // A `return` ends the path where it is, as the function or the
            // lambda gives its value to its caller and no line after it runs.
            // The value goes where the result of the function or the lambda
            // goes, so a lent value is not returned and a value without
            // `Copy` is given away, as the last expression gives it away.
            Expr::Return { value, .. } => {
                if let Some(value) = value {
                    self.eval(st, value, Flow::Returned);
                }
                // What the function or the lambda owns ends where it returns.
                let base = self.inside.last().map_or(0, |f| f.depth);
                let dying = self.dying(st, base);
                self.record(Site::Leave(addr(e)), dying);
                self.dropped(st);
                if let (Some(moved), Some(frame)) = (st.as_ref(), self.inside.last_mut()) {
                    frame
                        .leaves
                        .push(moved.gone.iter().map(|(p, _)| p.0.clone()).collect());
                }
                *st = None;
                None
            }
        }
    }

    /// The statements of a block, whose last gives the value that goes where
    /// `flow` says. It gives whether that value is lent.
    fn block(&mut self, st: &mut State, stmts: &[Stmt], flow: Flow) -> Lent {
        self.depth += 1;
        let lent = self.statements(st, stmts, flow);
        // What the block owns ends with it.
        let dying = self.dying(st, self.depth);
        if let Some(last) = stmts.last() {
            self.record(Site::After(addr(last)), dying);
        }
        self.leave();
        lent
    }

    fn statements(&mut self, st: &mut State, stmts: &[Stmt], flow: Flow) -> Lent {
        let mut lent = None;
        for (i, stmt) in stmts.iter().enumerate() {
            let last = i + 1 == stmts.len();
            match stmt {
                Stmt::Define {
                    name, value, pos, ..
                } => {
                    let l = self.eval(st, value, Flow::Bind);
                    if let Expr::Lambda(lambda) = value
                        && let Some(s) = self.defs.env_of(lambda)
                    {
                        let held = (self.defs.file.structs[s].fields.iter())
                            .map(|f| f.name.clone())
                            .collect();
                        self.holding.insert(name.clone(), held);
                    }
                    // A name bound to such a lambda holds what it holds.
                    if let Expr::Name { name: from, .. } = value
                        && let Some(held) = self.holding.get(from).cloned()
                    {
                        self.holding.insert(name.clone(), held);
                    }
                    // A `mut` that is bound again drops the value it held, if
                    // it did not hand it on.
                    let old = (st.as_ref())
                        .and_then(|m| m.own.iter().find(|o| o.name == *name))
                        .map(Dropped::whole);
                    if let Some(old) = old {
                        self.record(Site::Replace(addr(stmt)), vec![old]);
                    }
                    self.bind(st, name, l);
                    if let Some(ty) = self.types.get(&addr(value)) {
                        self.own(st, name, *pos, &ty.clone());
                    }
                }
                Stmt::Assign {
                    name,
                    fields,
                    value,
                    ..
                } => {
                    self.eval(st, value, Flow::Stored);
                    // The value a name held is dropped where another takes its
                    // place, unless it was handed on, and the name owns the new.
                    if let (
                        Stmt::Assign {
                            fields, at: None, ..
                        },
                        Some(ty),
                    ) = (stmt, self.types.get(&addr(value)).cloned())
                        && fields.is_empty()
                        && self.holds(&ty)
                        && !self.lent.contains_key(name)
                        && let Some(moved) = st
                    {
                        if let Some(o) = moved.own.iter_mut().find(|o| o.name == *name) {
                            let old = Dropped::whole(o);
                            o.handed.clear();
                            self.record(Site::Replace(addr(stmt)), vec![old]);
                        } else if let Some(&(pos, depth)) = self.bound.get(name) {
                            moved.own.push(Own {
                                name: name.clone(),
                                pos,
                                depth,
                                handed: Vec::new(),
                            });
                        }
                    }
                    // The value a field held is dropped where another takes its
                    // place, unless it was handed on, and the field owns the new.
                    if let (
                        Stmt::Assign {
                            fields, at: None, ..
                        },
                        Some(ty),
                    ) = (stmt, self.types.get(&addr(value)).cloned())
                        && !fields.is_empty()
                        && self.holds(&ty)
                        && !self.lent.contains_key(name)
                        && let Some(moved) = st
                        && let Some(o) = moved.own.iter_mut().find(|o| o.name == *name)
                    {
                        let part: Vec<String> = fields.iter().map(|f| f.0.clone()).collect();
                        if !o.handed.iter().any(|h| part.starts_with(h) && *h != part) {
                            let inside = (o.handed.iter())
                                .filter(|h| h.starts_with(&part))
                                .map(|h| h[part.len()..].to_vec())
                                .collect();
                            let old = Dropped {
                                name: name.clone(),
                                part: part.clone(),
                                except: inside,
                            };
                            o.handed.retain(|h| !h.starts_with(&part));
                            self.record(Site::Replace(addr(stmt)), vec![old]);
                        }
                    }
                    if let Some(moved) = st {
                        let path: Path =
                            (name.clone(), fields.iter().map(|f| f.0.clone()).collect());
                        // The value a field held is dropped by the assignment,
                        // unless it was taken.
                        let kept = !moved.gone.iter().any(|(p, _)| overlap(p, &path));
                        let old = self.types.get(&addr(value)).cloned();
                        let cause = old.and_then(|ty| {
                            let cause = undiscardable(self.defs, self.scope, &ty)?;
                            Some((ty, cause))
                        });
                        if let (true, Some((ty, cause)), Some(at)) = (kept, cause, fields.last()) {
                            let (why, fix) = why_dropped(self.defs, self.scope, &ty, cause);
                            let message = format!(
                                "`{}` is dropped here, as the assignment replaces it and nothing takes it{why}",
                                written(&path)
                            );
                            if let Some(error) = self.error(at.1, message) {
                                error.fix = Some(fix);
                            }
                        }
                        let Some(moved) = st else {
                            continue;
                        };
                        // A part that was taken and is assigned again is
                        // there to be taken again.
                        let retaken = moved.gone.iter().any(|(p, _)| *p == path);
                        if let (true, Some(ty), Some(at)) =
                            (retaken, self.types.get(&addr(value)), fields.last())
                            && undiscardable(self.defs, self.scope, ty).is_some()
                        {
                            moved.held.push(Held {
                                name: path.0.clone(),
                                fields: path.1.clone(),
                                pos: at.1,
                                ty: ty.clone(),
                            });
                        }
                        moved
                            .gone
                            .retain(|(p, _)| !(path.0 == p.0 && p.1.starts_with(&path.1)));
                    }
                }
                Stmt::Destructure { pattern, value, .. } => {
                    let l = self.eval(st, value, Flow::Bind);
                    self.bind_pattern(st, pattern, &l);
                    if let Some(ty) = self.types.get(&addr(value)) {
                        self.own_pattern(st, pattern, &ty.clone());
                    }
                }
                Stmt::Expr(value) => {
                    lent = self.eval(st, value, if last { flow } else { Flow::Lend });
                }
            }
        }
        lent
    }

    /// Gives a name a new value, which is gone nowhere.
    fn bind(&mut self, st: &mut State, name: &str, lent: Lent) {
        if !self.visible.iter().any(|(n, _)| n == name) {
            self.visible.push((name.to_string(), self.depth));
        }
        if let Some(frame) = self.inside.last_mut() {
            frame.own.insert(name.to_string());
        }
        // Every value the name may hold is dropped, as paths that met may
        // hold different ones.
        let old: Vec<Held> = (st.as_ref())
            .map(|m| m.held.iter().filter(|h| h.name == name).cloned().collect())
            .unwrap_or_default();
        if !old.is_empty() {
            self.dropped(&Some(Moved {
                held: old,
                ..Moved::default()
            }));
        }
        if let Some(moved) = st {
            moved.gone.retain(|(p, _)| p.0 != name);
            moved.held.retain(|h| h.name != name);
        }
        match lent {
            Some(by) => {
                self.lent.insert(name.to_string(), by);
            }
            None => {
                self.lent.remove(name);
            }
        }
    }

    fn bind_pattern(&mut self, st: &mut State, pattern: &Pattern, lent: &Lent) {
        match pattern {
            Pattern::Bind { name, .. } => self.bind(st, name, lent.clone()),
            Pattern::Case { inner, .. } => {
                if let Some(inner) = inner {
                    self.bind_pattern(st, inner, lent);
                }
            }
            Pattern::Tuple { items, .. } | Pattern::List { items, .. } => {
                for item in items {
                    self.bind_pattern(st, item, lent);
                }
            }
            Pattern::Number { .. } | Pattern::Text { .. } => {}
        }
    }

    /// Whether a pattern, matched against a value of type `ty`, names a part
    /// that has no `Copy`.
    fn binds_part(&self, pattern: &Pattern, ty: &Ty) -> bool {
        match (pattern, ty) {
            (Pattern::Bind { .. }, ty) => !copies(self.defs, self.scope, ty),
            (
                Pattern::Case {
                    name,
                    inner: Some(inner),
                    ..
                },
                Ty::Enum(e, args),
            ) => match self.defs.case_of(*e, name) {
                Some(c) => (self.defs.cases[*e][c].as_ref())
                    .is_some_and(|held| self.binds_part(inner, &held.subst(args))),
                None => false,
            },
            (Pattern::Tuple { items, .. }, Ty::Tuple(tys)) => {
                items.iter().zip(tys).any(|(p, t)| self.binds_part(p, t))
            }
            (Pattern::List { items, .. }, Ty::List(of)) => {
                items.iter().any(|p| self.binds_part(p, &of[0]))
            }
            _ => false,
        }
    }

    fn matching(&mut self, st: &mut State, m: &Match, flow: Flow) -> Lent {
        let part = (self.types.get(&addr(&m.of))).is_some_and(|ty| {
            let ty = ty.clone();
            m.arms.iter().any(|arm| self.binds_part(&arm.pattern, &ty))
        });
        // A name a pattern binds owns the part it is bound to, so a value
        // that is matched with one is given to the arms.
        let binds = m.arms.iter().any(|arm| binds_name(&arm.pattern));
        let scrutinee = if part {
            Flow::Bind
        } else if binds && self.place(&m.of).is_none() {
            Flow::Stored
        } else {
            Flow::Lend
        };
        let lent_scrutinee = self.eval(st, &m.of, scrutinee);
        if let (true, Some(path), Some(moved)) = (binds, self.place(&m.of), st.as_mut()) {
            let root = self.alias.get(&path.0).unwrap_or(&path.0).clone();
            moved.own.retain(|o| o.name != root && o.name != path.0);
        }
        let start = st.take();
        let mut end: State = None;
        let mut lent = None;
        let mut ends: Vec<(&Arm, State)> = Vec::new();
        for arm in &m.arms {
            let mut arm_st = start.clone();
            self.depth += 1;
            self.bind_pattern(&mut arm_st, &arm.pattern, &lent_scrutinee);
            if let Some(ty) = self.types.get(&addr(&m.of)) {
                self.own_pattern(&mut arm_st, &arm.pattern, &ty.clone());
            }
            lent = lent.or(self.block(&mut arm_st, &arm.body, flow));
            // What the pattern bound ends with the arm.
            let dying = self.dying(&mut arm_st, self.depth);
            self.leave();
            self.record(Site::Arm(addr(arm)), dying);
            ends.push((arm, arm_st));
        }
        // What an arm still owns that another handed on, it drops where it
        // ends, so that every path leaves with the same names owning.
        let kept: Vec<Own> = intersection(ends.iter().filter_map(|(_, st)| st.as_ref()));
        for (arm, arm_st) in ends {
            if let Some(moved) = &arm_st {
                self.record(Site::Arm(addr(arm)), extras(moved, &kept));
            }
            end = join(end, arm_st);
        }
        // A `match` with no arm, or an `if` with no `else`, runs no block
        // for the value that matches nothing.
        if m.arms.is_empty() {
            end = start;
        }
        *st = end;
        lent
    }

    fn looping(&mut self, st: &mut State, l: &Loop) {
        let (entry, saved) = (st.clone(), self.drops.clone());
        let root = l.over.as_ref().map(|over| match self.place(&over.value) {
            Some(path) => self.alias.get(&path.0).cloned().unwrap_or(path.0),
            None => held_name(l),
        });
        if let Some(root) = &root {
            self.consumed.remove(root);
        }
        self.looping_once(st, l);
        // A loop over a list that gives an element on may have given on some
        // of the others in an earlier round, so it owns none of them, wherever
        // in the block it is dropped. The loop is read again with the list
        // given on where it begins.
        if let Some(root) = root
            && self.consumed.contains(&root)
        {
            *self.drops = saved;
            *st = entry;
            self.consume = Some(root);
            self.looping_once(st, l);
        }
    }

    fn looping_once(&mut self, st: &mut State, l: &Loop) {
        if let Some(range) = &l.range {
            self.eval(st, &range.from, Flow::Lend);
            self.eval(st, &range.to, Flow::Lend);
        }
        if let Some(over) = &l.over {
            let made = self.place(&over.value).is_none()
                && (self.types.get(&addr(&over.value))).is_some_and(|ty| self.holds(ty));
            self.eval(
                st,
                &over.value,
                if made { Flow::Stored } else { Flow::Lend },
            );
        }
        if let Some(measure) = &l.decreases {
            self.eval(st, measure, Flow::Lend);
        }
        for invariant in &l.invariants {
            self.eval(st, invariant, Flow::Lend);
        }
        // The elements stay in the list, so the block is lent each one.
        let element = "this `loop`";
        // A loop over a value that is made where it is written holds it in a
        // name of its own until the loop ends, and what the block reads of an
        // element it reads of that value.
        let held = held_name(l);
        let root = l.over.as_ref().map(|over| match self.place(&over.value) {
            Some(path) => self.alias.get(&path.0).cloned().unwrap_or(path.0),
            None => held.clone(),
        });
        if let Some(over) = &l.over
            && self.place(&over.value).is_none()
            && let Some(ty) = self.types.get(&addr(&over.value)).cloned()
            && self.holds(&ty)
        {
            self.drops.held.insert(addr(l));
            self.track(st, &held, over.value.pos(), &ty);
        }
        if let (Some(root), Some(moved)) = (self.consume.take(), st.as_mut()) {
            moved.own.retain(|o| o.name != root);
        }
        let entry = st.clone();
        let before: Vec<Held> = entry.as_ref().map(|m| m.held.clone()).unwrap_or_default();
        let reused = self.reused.clone();
        self.breaks.push((self.depth + 1, Vec::new()));
        let saved = self.drops.clone();
        // The body is read again, from what is known at the head of the loop
        // when the loop goes round, until that stops changing. What is known
        // is a set of places given away and a set of values held, `join` only
        // adds to them, and each is among the names, fields and places of the
        // function, so it stops growing after a number of rounds bounded by
        // their count. The limit is far above that, so that a mistake in
        // `join` is a failure and not a loop that never ends. A mistake is
        // reported once, however many rounds read it. A value is known by the
        // statement that made it and not by where it has moved to, so a chain
        // of names that a round moves a value along is known after one round,
        // and no program built for this has needed more than three.
        let mut rounds: Vec<State> = Vec::new();
        let mut before_gone: Vec<(Path, Pos)> = Vec::new();
        let mut head = st.clone();
        loop {
            assert!(
                rounds.len() < ROUNDS,
                "what is known at a loop's head never settles"
            );
            let known = self.reused.len();
            // Only what the last look at the body found is where a name is
            // dropped, as it is the one made from what is known at the head
            // when the loop goes round.
            *self.drops = saved.clone();
            // What a `break` of an earlier look gave away was given away by a
            // round the loop can make, so it is gone after the loop whatever
            // a later look finds: a later look reads the body from what is
            // known at the head by then, and a value given away in it again is
            // reported and no longer recorded.
            if let Some((_, exits)) = self.breaks.last_mut() {
                for (moved, _) in exits.drain(..) {
                    for gone in moved.gone {
                        if !before_gone.iter().any(|(p, _)| *p == gone.0) {
                            before_gone.push(gone);
                        }
                    }
                }
            }
            let mut round = head.clone();
            if let (Some(range), Some(_)) = (&l.range, &round) {
                self.bind(&mut round, &range.name, None);
            }
            if let (Some(over), Some(_)) = (&l.over, &round) {
                self.bind(&mut round, &over.name, Some(element.to_string()));
                if let Some(root) = &root {
                    self.alias.insert(over.name.clone(), root.clone());
                }
            }
            self.block(&mut round, &l.body, Flow::Lend);
            let wider = join(head.clone(), round.clone());
            rounds.push(round);
            // The names used after they were given away are known too, as a
            // loop in the body reads them to tell what it is the first to find.
            if same(&wider, &head) && self.reused.len() == known {
                break;
            }
            head = wider;
        }
        if let Some(over) = &l.over {
            self.alias.remove(&over.name);
        }
        let exits = self
            .breaks
            .pop()
            .map(|(_, exits)| exits)
            .unwrap_or_default();
        // The loop ends where a `break` leaves it, and where its range ends.
        // A path that owns what another handed on drops it before it ends, so
        // that every path leaves with the same names owning.
        let natural = (l.range.is_some() || l.over.is_some()).then(|| head.clone());
        let kept: Vec<Own> = intersection(
            (exits.iter().map(|(moved, _)| moved))
                .chain(natural.iter().filter_map(|st| st.as_ref())),
        );
        for (moved, at) in &exits {
            self.record(Site::Leave(*at), extras(moved, &kept));
        }
        if let Some(Some(moved)) = &natural {
            self.record(Site::Done(addr(l)), extras(moved, &kept));
        }
        let mut after: State = None;
        for (exit, _) in exits {
            after = join(after, Some(exit));
        }
        if let Some(moved) = &mut after {
            for (path, at) in before_gone {
                if !moved.gone.iter().any(|(p, _)| *p == path) {
                    moved.gone.push((path, at));
                }
            }
        }
        if l.range.is_some() || l.over.is_some() {
            after = join(after, entry);
            for round in rounds {
                after = join(after, round);
            }
        }
        // A value the loop was entered with and gives away again in each
        // round is reported there, and not also as dropped before the first.
        if let Some(moved) = &mut after {
            moved.held.retain(|h| {
                let again = self.reused.contains(&h.name) && !reused.contains(&h.name);
                !(again && before.iter().any(|b| b.name == h.name && b.pos == h.pos))
            });
        }
        *st = after;
    }

    /// Reads a lambda where it is written, as the block it is. What it gives
    /// away of what it did not define or take is spent from here, and the
    /// lambda is called once, so it is passed to a parameter that takes its
    /// function. Its parameter owns what it is given when `takes` says so.
    fn lambda(
        &mut self,
        st: &mut State,
        l: &Lambda,
        takes: &[bool],
        (kept, spawned): (Kept, bool),
    ) {
        let outer = st
            .as_ref()
            .map(|m| m.held.iter().map(|h| (h.name.clone(), h.pos)).collect())
            .unwrap_or_default();
        self.depth += 1;
        self.inside.push(Frame {
            outer,
            depth: self.depth,
            ..Frame::default()
        });
        for (i, param) in l.params.iter().enumerate() {
            let taken = takes.get(i).copied().unwrap_or(false);
            let lent = (!taken).then(|| "this lambda".to_string());
            self.bind(st, &param.0, lent);
            // What a parameter that is taken is given must be taken.
            if let (true, Some(ty)) = (taken, self.types.get(&addr(param))) {
                self.own(st, &param.0, param.1, &ty.clone());
            }
        }
        self.block(st, &l.body, Flow::Returned);
        // What the lambda owns ends with it.
        let dying = self.dying(st, self.depth);
        if let Some(last) = l.body.last() {
            self.record(Site::After(addr(last)), dying);
        }
        self.leave();
        // What the lambda holds ends with it.
        self.dropped(st);
        if let (Some(moved), Some(frame)) = (st.as_mut(), self.inside.last()) {
            moved
                .held
                .retain(|h| frame.outer.iter().any(|(n, p)| *n == h.name && *p == h.pos));
        }
        let mut frame = self.inside.pop().unwrap_or_default();
        // A `mut` the lambda gives away and assigns again on every path out of
        // it holds a value when it returns, as it did when it began: nothing
        // was given away.
        let var = |name: &str| {
            (statements(&self.f.body).into_iter())
                .any(|s| matches!(s, Stmt::Define { name: n, var: true, .. } if n == name))
        };
        let gone_at_end: Vec<&str> = (st.as_ref())
            .map(|m| m.gone.iter().map(|(p, _)| p.0.as_str()).collect())
            .unwrap_or_default();
        let leaves = std::mem::take(&mut frame.leaves);
        frame.gives.retain(|(name, ..)| {
            let paid = var(name)
                && !gone_at_end.contains(&name.as_str())
                && leaves.iter().all(|gone| !gone.contains(name));
            !paid
        });
        if spawned {
            let mut names: Vec<&String> = Vec::new();
            for (name, ..) in &frame.gives {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            let dropped = (names.into_iter())
                .map(|name| Dropped {
                    name: name.clone(),
                    part: Vec::new(),
                    except: Vec::new(),
                })
                .collect();
            self.record(Site::Start(addr(l)), dropped);
        }
        if frame.gives.is_empty() {
            return;
        }
        if kept == Kept::Lent {
            let mut names: Vec<String> = Vec::new();
            for (name, ..) in &frame.gives {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
            let names = names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>();
            let message = format!(
                "this lambda gives away {}, and the function it is passed to may call it more than once",
                names.join(" and ")
            );
            if let Some(error) = self.error(l.pos, message) {
                error.fix =
                    Some("write `take` before the function type of that parameter".to_string());
            }
            return;
        }
        // A function that is taken is called at most once, so it may not call
        // the lambda, and what the lambda gives away is dropped.
        let lacks = frame.gives.iter().find_map(|(name, _, ty)| {
            if kept == Kept::Called {
                return None;
            }
            let ty = ty.as_ref()?;
            let cause = undiscardable(self.defs, self.scope, ty)?;
            Some((name.clone(), ty.clone(), cause))
        });
        if let Some((name, ty, cause)) = lacks {
            let (why, fix) = why_dropped(self.defs, self.scope, &ty, cause);
            let message = format!(
                "this lambda gives away `{name}`, and the function it is passed to may not call it, which drops `{name}`{why}"
            );
            if let Some(error) = self.error(l.pos, message) {
                error.fix = Some(fix);
            }
        }
        // A lambda inside a lambda that gives a name away is called once too.
        if let Some(outer) = self.inside.last_mut() {
            for give in frame.gives {
                if !outer.own.contains(&give.0) {
                    outer.gives.push(give);
                }
            }
        }
    }

    /// Reads a call: each argument is given away when its parameter is
    /// marked `take`, and lent otherwise, and a struct takes what it holds.
    fn call(&mut self, st: &mut State, call: &Call) {
        let defs = self.defs;
        let name = call.callee.as_str();
        // What each parameter is: marked `take`, and for a function, what
        // marks its own parameters.
        let params: Option<&[Param]> = match &call.qualifier {
            Some((q, at)) if defs.enum_at(*at, q).is_some() => None,
            Some((q, at)) => defs.trait_at(*at, q).and_then(|k| {
                let t = &defs.file.traits[k];
                traits::trait_function(t, name).map(|fi| t.functions[fi].params.as_slice())
            }),
            None if self
                .f
                .params
                .iter()
                .any(|p| p.name == name && p.ty.function) =>
            {
                None
            }
            None if defs.struct_at(call.name_pos, name).is_some() => None,
            None => match defs.function(call.name_pos, name) {
                Some(f) => Some(defs.file.functions[f].params.as_slice()),
                None => match defs.trait_fns_at(call.name_pos, name).map(Vec::as_slice) {
                    Some([(k, fi), ..]) => {
                        Some(defs.file.traits[*k].functions[*fi].params.as_slice())
                    }
                    _ => None,
                },
            },
        };
        let value_fn = self
            .f
            .params
            .iter()
            .find(|p| call.qualifier.is_none() && p.name == name && p.ty.function);
        let constructs = call
            .qualifier
            .as_ref()
            .is_some_and(|(q, at)| defs.enum_at(*at, q).is_some())
            || (call.qualifier.is_none()
                && value_fn.is_none()
                && defs.struct_at(call.name_pos, name).is_some());
        if let Some(f) = value_fn.filter(|f| f.take) {
            self.spend(st, &f.name, call.pos);
        }
        let handler_body = call.qualifier.is_none()
            && (super::effects_with_and_try::is_arena_call(defs, call)
                || defs.effect_named(&call.callee).is_some()
                || super::frames::is_frame_call(call)
                // The work of a child runs once, after the call has moved on,
                // and the body of a group runs once, in its scheduler.
                || (matches!(call.callee.as_str(), "spawn" | "tasks")
                    && defs
                        .function(call.name_pos, &call.callee)
                        .is_some_and(|f| super::of_std(defs.file.functions[f].pos))));
        let mut position = 0;
        let mut given: Vec<(usize, &Expr)> = Vec::new();
        for arg in &call.args {
            let ix = match (&arg.name, params) {
                (Some((n, _)), Some(ps)) => ps.iter().position(|p| p.name == *n),
                (None, Some(ps)) if arg.after => {
                    // A lambda after the call is for the last parameter given by position.
                    ps.iter()
                        .position(|p| p.by_name)
                        .unwrap_or(ps.len())
                        .checked_sub(1)
                }
                (None, Some(_)) => {
                    position += 1;
                    Some(position - 1)
                }
                _ => None,
            };
            let param = ix.and_then(|i| params.and_then(|ps| ps.get(i)));
            if let Some(i) = ix {
                given.push((i, &arg.value));
            }
            // A slice cannot change, so it is not given where a list is
            // taken, which the callee may grow.
            if param.is_some_and(|p| p.take && p.ty.name == "List")
                && matches!(self.types.get(&addr(&arg.value)), Some(Ty::Slice(_)))
                && let Some(error) = self.error(
                    arg.value.pos(),
                    format!(
                        "`{}` takes a list, which it may grow, and this is a slice, which cannot change",
                        call.callee
                    ),
                )
            {
                error.fix = Some(format!(
                    "write `copy({})`, which is a new list",
                    super::written_expr(&arg.value)
                ));
            }
            let flow = match (param, value_fn) {
                _ if constructs => Flow::Stored,
                // The body of a handler the compiler builds runs once, so a
                // function that is called once is given to it as it is.
                _ if handler_body
                    && matches!(&arg.value, Expr::Name { name: n, .. }
                        if self.f.params.iter().any(|p| p.name == *n && p.ty.function && p.take)) =>
                {
                    Flow::Taken(name)
                }
                // `push` takes its list and its item, `append` its list.
                _ if super::lists_and_text::gives_away(defs, call, arg) => Flow::Taken(name),
                (Some(p), _) if p.take => Flow::Taken(name),
                (None, Some(f)) => {
                    let at = if arg.name.is_none() && !arg.after {
                        position += 1;
                        position - 1
                    } else {
                        usize::MAX
                    };
                    if f.takes.get(at).copied().unwrap_or(false) {
                        Flow::Taken(name)
                    } else {
                        Flow::Lend
                    }
                }
                _ => Flow::Lend,
            };
            match &arg.value {
                // A lambda that holds names, stored in a field, takes the values.
                Expr::Lambda(l) if defs.env_of(l).is_some() => {
                    self.eval(st, &arg.value, Flow::Stored);
                }
                Expr::Lambda(l) => {
                    // A handler's lambda for an operation owns what the
                    // operation takes, as a function owns what it takes. The
                    // schedulers of `task` are left as they were.
                    let effect = defs
                        .effect_named(&call.callee)
                        .filter(|e| !defs.is_scheduler(*e));
                    let operation: Vec<bool> = match (&arg.name, effect) {
                        (Some((op, _)), Some(_)) => (defs.file.functions.iter())
                            .find(|f| {
                                f.name == *op
                                    && is_operation(f)
                                    && f.effects.first().is_some_and(|e| e.0 == call.callee)
                            })
                            .map(|f| f.params.iter().map(|p| p.take).collect())
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    let takes = param.map(|p| p.takes.as_slice()).unwrap_or(&operation);
                    let kept = if handler_body {
                        Kept::Called
                    } else if param.is_some_and(|p| p.take) {
                        Kept::Taken
                    } else {
                        Kept::Lent
                    };
                    let spawned = call.qualifier.is_none()
                        && call.callee == "spawn"
                        && defs
                            .function(call.name_pos, &call.callee)
                            .is_some_and(|f| super::of_std(defs.file.functions[f].pos));
                    self.lambda(st, l, takes, (kept, spawned));
                }
                value => {
                    self.eval(st, value, flow);
                }
            }
        }
        if !constructs && value_fn.is_none() && params.is_some() {
            self.generic(call, &given);
        }
        // Where the call is left because the task was asked to end, the function
        // ends what it owns, as it does where it returns.
        let base = self.inside.last().map_or(0, |f| f.depth);
        let owned: Vec<Dropped> = st.as_ref().map_or_else(Vec::new, |moved| {
            (moved.own.iter().filter(|o| o.depth >= base))
                .map(Dropped::whole)
                .collect()
        });
        // A function that holds a handle of `net`, or a `File`, while it may wait
        // must carry the effect of the handle, which a handler answers.
        let held = (st.as_ref()).and_then(|moved| {
            (moved.own.iter().filter(|o| o.depth >= base)).find_map(|o| {
                self.handles
                    .get(&o.name)
                    .map(|t| (o.name.clone(), t.clone()))
            })
        });
        if let Some((_, (ty, needed))) = held
            && self.may_wait(call)
            && let Some(effect) = needed.into_iter().find(|effect| {
                !(self.f.effects.iter()).any(|(e, _)| super::written_name(e) == *effect)
            })
        {
            let message = format!(
                "`{}` holds a `{ty}` while it may wait, so it must carry `{effect}`",
                self.f.name
            );
            if let Some(error) = self.error(call.pos, message) {
                error.fix = Some(format!("add `{effect}` to its effects"));
            }
        }
        self.record(Site::Cancel(addr(call)), owned);
    }

    /// Whether the function a call names carries `net`, `time`, or `task`, which
    /// are the effects a task waits in.
    fn may_wait(&self, call: &Call) -> bool {
        let defs = self.defs;
        call.qualifier.is_none()
            && defs.function(call.name_pos, &call.callee).is_some_and(|f| {
                defs.file.functions[f].effects.iter().any(|(e, _)| {
                    defs.effect_named(e).is_some_and(|at| {
                        let e = &defs.file.effects[at];
                        super::of_std(e.pos)
                            && matches!(
                                super::written_name(&e.name),
                                "net" | "fs" | "time" | "task"
                            )
                    })
                })
            })
    }

    /// Reads the call of a function that is called once: its first call
    /// spends it, and a second is a second use of a value given away.
    fn spend(&mut self, st: &mut State, name: &str, pos: Pos) {
        let Some(moved) = st else {
            return;
        };
        if let Some((_, at)) = moved.gone.iter().find(|(p, _)| p.0 == name) {
            let message = format!(
                "`{name}` is called here, after it was given away at {}:{}",
                at.line, at.col
            );
            if let Some(error) = self.error(pos, message) {
                error.fix = Some(format!(
                    "call `{name}` once, or write `{name}` without `take` so that it may be called again"
                ));
            }
            return;
        }
        moved.gone.push(((name.to_string(), Vec::new()), pos));
        if let Some(frame) = self.inside.last_mut()
            && !frame.own.contains(name)
        {
            frame.gives.push((name.to_string(), pos, None));
        }
    }

    /// Refuses a call that gives a type parameter of the function, or of the
    /// implementation it chooses, a type that lacks what a bound of the
    /// parameter asks: `Copy` or `Discard`. One mistake is reported for the
    /// call.
    fn generic(&mut self, call: &Call, given: &[(usize, &Expr)]) {
        let defs = self.defs;
        let name = call.callee.as_str();
        let of_trait = match &call.qualifier {
            Some((q, at)) => defs.trait_at(*at, q).and_then(|k| {
                let at = traits::trait_function(&defs.file.traits[k], name);
                at.map(|fi| (k, fi))
            }),
            None if defs.function(call.name_pos, name).is_some() => None,
            None => match defs.trait_fns_at(call.name_pos, name).map(Vec::as_slice) {
                Some([(k, fi), ..]) => Some((*k, *fi)),
                _ => None,
            },
        };
        let solve = |params: &[(String, Ty)], gives: &Ty, vars: usize| {
            let mut sol: Vec<Option<Ty>> = vec![None; vars];
            for (i, value) in given {
                if let (Some(t), Some((_, p))) = (self.types.get(&addr(*value)), params.get(*i))
                    && !t.unknown()
                {
                    unify(p, t, &mut sol);
                }
            }
            if let Some(t) = self.types.get(&addr(call))
                && !t.unknown()
            {
                unify(gives, t, &mut sol);
            }
            sol
        };
        // The scope that bounds the types, with the types it is given, and
        // what it is, as an error names it.
        let (bounds, sol, what): (&Scope, Vec<Option<Ty>>, String) = if let Some((k, fi)) = of_trait
        {
            let sig = &defs.trait_sigs[k][fi];
            let mut sol = solve(&sig.params, &sig.gives, sig.scope.names.len());
            // The function's own type parameters follow the trait's types.
            let own = sol.split_off(defs.file.traits[k].generics.len());
            let outs = &defs.outs[k];
            if sol.iter().zip(outs).any(|(t, out)| t.is_none() && !out) {
                return;
            }
            let n = sol.len();
            let types: Vec<Ty> = (sol.into_iter())
                .map(|t| t.unwrap_or(Ty::Unknown))
                .collect();
            let what = format!(
                "the implementation of `{}` chosen here",
                defs.file.traits[k].name
            );
            match defs.find(k, &types, self.scope, &mut Vec::new()) {
                Found::Yes(_, traits::By::Impl(i, chosen)) => {
                    let Some(info) = &defs.impls[i] else {
                        return;
                    };
                    let mut sol: Vec<Option<Ty>> = chosen.into_iter().map(Some).collect();
                    sol.extend(own);
                    (&info.fn_scopes[fi], sol, what)
                }
                // A bound promises the implementation, and the function's own
                // type parameters are bounded as the trait says.
                Found::Yes(..) if !own.is_empty() => {
                    let mut sol: Vec<Option<Ty>> = vec![None; n];
                    sol.extend(own);
                    (&sig.scope, sol, what)
                }
                _ => return,
            }
        } else if let Some(f) = defs.function(call.name_pos, name) {
            let sig = &defs.signatures[f];
            let sol = solve(&sig.params, &sig.gives, sig.scope.names.len());
            (&sig.scope, sol, format!("`{name}`"))
        } else {
            return;
        };
        for (i, t) in sol.iter().enumerate() {
            let Some(t) = t else {
                continue;
            };
            let mut lacks: Vec<&str> = Vec::new();
            if bounds.copy.get(i).copied().unwrap_or(false) && !copies(defs, self.scope, t) {
                lacks.push("Copy");
            }
            let discards = has((defs, self.scope), DISCARD, t, &mut Vec::new()).is_ok();
            if bounds.discard.get(i).copied().unwrap_or(false) && !discards {
                lacks.push("Discard");
            }
            if lacks.is_empty() {
                continue;
            }
            let param = bounds.names.get(i).cloned().unwrap_or_default();
            let message = format!(
                "`{}` has no {}, which {what} needs of `{param}`",
                defs.text(t, self.scope),
                lacks
                    .iter()
                    .map(|l| format!("`{l}`"))
                    .collect::<Vec<_>>()
                    .join(" and ")
            );
            // A type that lacks it only because of a type parameter of the
            // caller is mended by a bound there.
            let fix = lacks.iter().find_map(|l| {
                let which = if *l == "Copy" { COPY } else { DISCARD };
                let p = lacking(defs, self.scope, which, t)?;
                Some(format!("write `{}: {l}`", self.scope.names[p]))
            });
            if let Some(error) = self.error(call.pos, message) {
                error.fix = fix;
            }
            return;
        }
    }
}
