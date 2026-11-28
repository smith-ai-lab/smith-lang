//! A lambda that holds nothing lent is a function with a name in all but the
//! name. Where one is bound to a name, stored in a struct, or returned, it
//! becomes a function of its own whose name no source writes, and the lambda
//! is replaced by that name, so that it is a function value known when the
//! program is built, as a function with a name is.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::{Call, Expr, Function, Generic, Lambda, Param, Stmt, Struct, Type};

/// The name of the function a lambda becomes: where the lambda is written.
fn lifted_name(l: &Lambda) -> String {
    format!("lambda@{}:{}", l.pos.line, l.pos.col)
}

/// Replaces each lambda that is bound, stored in a struct, or returned, and
/// that holds nothing lent, by the name of a new function, which it adds.
pub(super) fn lift(functions: &mut Vec<Function>, structs: &mut Vec<Struct>) {
    let envs = RefCell::new(Vec::new());
    let named: HashMap<&str, &Struct> = structs
        .iter()
        .filter(|s| s.alias.is_none())
        .map(|s| (s.name.as_str(), s))
        .collect();
    let mut lifted = Vec::new();
    for f in functions.iter_mut() {
        let mut around: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
        for stmt in crate::check::statements(&f.body) {
            match stmt {
                Stmt::Define { name, .. } => {
                    around.insert(name.clone());
                }
                Stmt::Destructure { .. } | Stmt::Assign { .. } | Stmt::Expr(_) => {}
            }
        }
        let generics: HashSet<String> = f.generics.iter().map(|g| g.name.clone()).collect();
        let cx = Cx {
            structs: &named,
            around: &around,
            generics: &generics,
            generic_list: &f.generics,
            envs: &envs,
        };
        let last = f.body.len().checked_sub(1);
        let result = f.result.as_ref().filter(|t| t.function);
        block(&cx, &mut f.body, last.map(|i| (i, result)), &mut lifted);
    }
    functions.extend(lifted);
    structs.extend(envs.into_inner());
}

struct Cx<'a> {
    structs: &'a HashMap<&'a str, &'a Struct>,
    /// The names the function defines and takes.
    around: &'a HashSet<String>,
    /// The type parameters of the function.
    generics: &'a HashSet<String>,
    /// The type parameters of the function, in order.
    generic_list: &'a [Generic],
    /// The structs that hold what the lambdas bound to a name read.
    envs: &'a RefCell<Vec<Struct>>,
}

/// What a lambda is kept as, where it is written: not kept, kept with nothing to
/// say what its types are, or kept as the function type that decides them, with
/// the type parameters of the struct that the type is written in.
#[derive(Clone, Copy)]
enum Kept<'a> {
    No,
    Anywhere,
    /// Bound directly to a name, which the calls of it may give the types of.
    Bound,
    Like(&'a Type, &'a [Generic]),
}

fn block(
    cx: &Cx,
    body: &mut [Stmt],
    returned: Option<(usize, Option<&Type>)>,
    out: &mut Vec<Function>,
) {
    for (i, stmt) in body.iter_mut().enumerate() {
        match stmt {
            Stmt::Define { value, .. } => {
                let kept = if matches!(value, Expr::Lambda(_)) {
                    Kept::Bound
                } else {
                    Kept::Anywhere
                };
                value_of(cx, value, kept, out)
            }
            Stmt::Expr(value) => {
                let kept = match returned {
                    Some((at, Some(ty))) if at == i => Kept::Like(ty, &[]),
                    Some((at, None)) if at == i => Kept::Anywhere,
                    _ => Kept::No,
                };
                value_of(cx, value, kept, out);
            }
            Stmt::Assign { .. } | Stmt::Destructure { .. } => {}
        }
        if let Stmt::Define { value, .. } | Stmt::Expr(value) = stmt {
            nested(cx, value, out);
        }
    }
}

/// The blocks that hold bindings of their own: of a `match`, a `loop`, and a
/// `for`.
fn nested(cx: &Cx, value: &mut Expr, out: &mut Vec<Function>) {
    match value {
        Expr::Match(m) => {
            for arm in &mut m.arms {
                block(cx, &mut arm.body, None, out);
            }
        }
        Expr::Loop(l) => block(cx, &mut l.body, None, out),
        Expr::For(f) => block(cx, &mut f.round.body, None, out),
        _ => {}
    }
}

/// Lifts the lambda that `value` is, or those it gives to the fields of a
/// struct it builds.
fn value_of(cx: &Cx, value: &mut Expr, kept: Kept, out: &mut Vec<Function>) {
    match value {
        Expr::Lambda(l) if !matches!(kept, Kept::No) => {
            if let Some(f) = lift_lambda(cx, l, kept) {
                let name = f.name.clone();
                let pos = l.pos;
                out.push(f);
                *value = Expr::Name { name, pos };
            } else if let Some(env) = env_lambda(cx, l, kept) {
                cx.envs.borrow_mut().push(env);
            } else if matches!(kept, Kept::Bound)
                && !l.params.is_empty()
                && (l.types.iter().any(Option::is_none) || l.result.is_none())
                && let Some(env) = env_struct(cx, l, true)
            {
                cx.envs.borrow_mut().push(env);
            }
        }
        Expr::Call(call) if is_struct(cx, call) => {
            let def = cx.structs[call.callee.as_str()];
            for (i, arg) in call.args.iter_mut().enumerate() {
                if arg.after {
                    continue;
                }
                let field = match &arg.name {
                    Some((name, _)) => def.fields.iter().find(|f| &f.name == name),
                    None => def.fields.get(i),
                };
                let kept = match field.and_then(|f| f.function.as_ref()) {
                    Some(ty) => Kept::Like(ty, &def.generics),
                    None => Kept::Anywhere,
                };
                value_of(cx, &mut arg.value, kept, out);
            }
        }
        _ => {}
    }
}

fn is_struct(cx: &Cx, call: &Call) -> bool {
    call.qualifier.is_none()
        && call.through.is_none()
        && cx.structs.contains_key(call.callee.as_str())
}

/// The type of each parameter of a lambda and of its result, where it leaves
/// some out: those the function type that decides them has, when the lambda has
/// as many parameters. None when one is left out and nothing decides it.
fn written_types(l: &Lambda, kept: Kept) -> Option<(Vec<Type>, Option<Type>)> {
    let like = match kept {
        Kept::Like(ty, _) if ty.args.len() == l.params.len() + 1 => Some(ty),
        _ => None,
    };
    let params = l
        .types
        .iter()
        .enumerate()
        .map(|(i, ty)| ty.clone().or_else(|| like.map(|t| t.args[i].clone())))
        .collect::<Option<Vec<Type>>>()?;
    let result = match (&l.result, like) {
        (Some(r), _) => Some(r.clone()),
        (None, Some(t)) => Some(t.args[t.args.len() - 1].clone()).filter(|r| !r.name.is_empty()),
        (None, None) => return None,
    };
    Some((params, result))
}

/// Whether a type that a function type writes names a type parameter of the
/// struct whose field it is, which a lambda written elsewhere cannot name.
fn names_struct_generic(kept: Kept, types: &[&Type]) -> bool {
    let Kept::Like(_, generics) = kept else {
        return false;
    };
    let mut named = HashSet::new();
    for t in types {
        type_names(t, &mut named);
    }
    named.iter().any(|n| generics.iter().any(|g| g.name == *n))
}

/// The function the lambda becomes, if it holds nothing lent and writes every
/// type that its function would need.
fn lift_lambda(cx: &Cx, l: &Lambda, kept: Kept) -> Option<Function> {
    // The type a parameter or the result is left out with is the one the
    // function type that decides it has, when the lambda has as many parameters.
    let (types, result) = written_types(l, kept)?;
    let params: Vec<Param> = l
        .params
        .iter()
        .zip(types)
        .map(|((name, pos), ty)| Param {
            name: name.clone(),
            pos: *pos,
            ty,
            default: None,
            take: false,
            mutable: false,
            takes: Vec::new(),
            effects: Vec::new(),
            by_name: false,
        })
        .collect();
    let own: HashSet<&str> = l.generics.iter().map(|g| g.name.as_str()).collect();
    let params: Vec<Param> = params
        .into_iter()
        .map(|p| Param {
            // A lambda owns what it takes, so a parameter of a type of its own
            // is given away to the lambda.
            take: own.contains(p.ty.name.as_str()) && p.ty.args.is_empty(),
            ..p
        })
        .collect();
    let mut named = HashSet::new();
    for p in &params {
        type_names(&p.ty, &mut named);
    }
    if let Some(result) = &result {
        type_names(result, &mut named);
    }
    if names_struct_generic(
        kept,
        &params
            .iter()
            .map(|p| &p.ty)
            .chain(&result)
            .collect::<Vec<_>>(),
    ) {
        return None;
    }
    if named
        .iter()
        .any(|n| cx.generics.contains(*n) && !own.contains(n))
    {
        return None;
    }
    let mut mine: HashSet<&str> = l.params.iter().map(|(n, _)| n.as_str()).collect();
    for stmt in crate::check::statements(&l.body) {
        if let Stmt::Define { name, .. } = stmt {
            // A name of the function that is defined again is assigned.
            if cx.around.contains(name) && !mine.contains(name.as_str()) {
                return None;
            }
            mine.insert(name);
        }
    }
    let mut used = Vec::new();
    for stmt in crate::check::statements(&l.body) {
        match stmt {
            Stmt::Define { value, .. } | Stmt::Destructure { value, .. } => {
                crate::check::walk(value, &mut used)
            }
            Stmt::Assign { name, value, .. } => {
                if cx.around.contains(name) && !mine.contains(name.as_str()) {
                    return None;
                }
                crate::check::walk(value, &mut used)
            }
            Stmt::Expr(value) => crate::check::walk(value, &mut used),
        }
    }
    for e in used {
        let name = match e {
            Expr::Name { name, .. } => name,
            Expr::Call(c) if c.qualifier.is_none() && c.through.is_none() => &c.callee,
            _ => continue,
        };
        if cx.around.contains(name) && !mine.contains(name.as_str()) {
            return None;
        }
    }
    Some(Function {
        name: lifted_name(l),
        pos: l.pos,
        generics: l.generics.clone(),
        params,
        result,
        result_name: None,
        effects: Vec::new(),
        requires: Vec::new(),
        needs: Vec::new(),
        notes: Vec::new(),
        ensures: Vec::new(),
        decreases: Vec::new(),
        body: l.body.clone(),
        end: l.end,
    })
}

fn type_names<'a>(t: &'a Type, out: &mut HashSet<&'a str>) {
    out.insert(&t.name);
    for a in &t.args {
        type_names(a, out);
    }
}

/// The name of the struct that holds what a lambda reads: where it is written.
pub(crate) fn env_name(pos: super::Pos) -> String {
    format!("env@lambda@{}:{}", pos.line, pos.col)
}

/// The struct that holds what a lambda that is bound, stored, or returned reads,
/// with the types it leaves out written as the function type that decides them
/// has them. None as `env_struct` says, and when a type is left out that nothing
/// decides.
fn env_lambda(cx: &Cx, l: &mut Lambda, kept: Kept) -> Option<Struct> {
    let (types, result) = written_types(l, kept)?;
    if names_struct_generic(kept, &types.iter().chain(&result).collect::<Vec<_>>()) {
        return None;
    }
    let mut written = l.clone();
    written.types = types.into_iter().map(Some).collect();
    written.result = result;
    let env = env_struct(cx, &written, false)?;
    *l = written;
    Some(env)
}

/// The generic struct that holds what a lambda that is bound, stored, or
/// returned reads of the function it is written in: the type parameters of the function, then one for
/// each name it reads, and a field for each name. No source writes it, and
/// the checker gives it the types the names have. None when the lambda leaves a
/// type out, assigns a name of its function, or reads none. A lambda that is
/// `inferred` may leave types out, which the calls of it give, and may read
/// nothing.
fn env_struct(cx: &Cx, l: &Lambda, inferred: bool) -> Option<Struct> {
    let written = l.result.is_some() && l.types.iter().all(Option::is_some);
    if !l.generics.is_empty() || (!inferred && !written) {
        return None;
    }
    let mut mine: HashSet<&str> = l.params.iter().map(|(n, _)| n.as_str()).collect();
    for stmt in crate::check::statements(&l.body) {
        if let Stmt::Define { name, .. } = stmt {
            // A name of the function that is defined again is assigned.
            if cx.around.contains(name) && !mine.contains(name.as_str()) {
                return None;
            }
            mine.insert(name);
        }
    }
    let outside = |name: &String| cx.around.contains(name) && !mine.contains(name.as_str());
    let mut reads: Vec<(&String, super::Pos)> = Vec::new();
    let mut used = Vec::new();
    for stmt in crate::check::statements(&l.body) {
        match stmt {
            Stmt::Define { value, .. } | Stmt::Destructure { value, .. } | Stmt::Expr(value) => {
                crate::check::walk(value, &mut used)
            }
            Stmt::Assign { name, value, .. } => {
                if outside(name) {
                    return None;
                }
                crate::check::walk(value, &mut used)
            }
        }
    }
    for e in used {
        match e {
            Expr::Name { name, pos } if outside(name) => {
                if !reads.iter().any(|(n, _)| *n == name) {
                    reads.push((name, *pos));
                }
            }
            Expr::Call(c) => {
                if outside(&c.callee) && c.qualifier.is_none() && c.through.is_none() {
                    return None;
                }
                if c.qualifier.as_ref().is_some_and(|(q, _)| outside(q)) {
                    return None;
                }
            }
            _ => {}
        }
    }
    if reads.is_empty() && !inferred {
        return None;
    }
    let mut generics = cx.generic_list.to_vec();
    let mut fields = Vec::new();
    for (name, pos) in reads {
        let implicit = format!("read@{name}");
        generics.push(Generic {
            name: implicit.clone(),
            pos,
            out: None,
            bound: None,
            default: None,
        });
        fields.push(super::Field {
            name: name.clone(),
            pos,
            ty: Type {
                name: implicit,
                pos,
                args: Vec::new(),
                function: false,
                range: None,
                statement: None,
                sizes: Vec::new(),
            },
            function: None,
            default: None,
            public: None,
        });
    }
    Some(Struct {
        name: env_name(l.pos),
        pos: l.pos,
        generics,
        fields,
        alias: None,
    })
}
