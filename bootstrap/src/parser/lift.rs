//! A lambda that holds nothing lent is a function with a name in all but the
//! name. Where one is bound to a name, stored in a struct, or returned, it
//! becomes a function of its own whose name no source writes, and the lambda
//! is replaced by that name, so that it is a function value known when the
//! program is built, as a function with a name is.

use std::collections::HashSet;

use super::{Arg, Call, Expr, Function, Lambda, Param, Stmt, Struct, Type};

/// The name of the function a lambda becomes: where the lambda is written.
fn lifted_name(l: &Lambda) -> String {
    format!("lambda@{}:{}", l.pos.line, l.pos.col)
}

/// Replaces each lambda that is bound, stored in a struct, or returned, and
/// that holds nothing lent, by the name of a new function, which it adds.
pub(super) fn lift(functions: &mut Vec<Function>, structs: &[Struct]) {
    let structs: HashSet<&str> = structs
        .iter()
        .filter(|s| s.alias.is_none())
        .map(|s| s.name.as_str())
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
            structs: &structs,
            around: &around,
            generics: &generics,
        };
        let last = f.body.len().checked_sub(1);
        block(&cx, &mut f.body, last, &mut lifted);
    }
    functions.extend(lifted);
}

struct Cx<'a> {
    structs: &'a HashSet<&'a str>,
    /// The names the function defines and takes.
    around: &'a HashSet<String>,
    /// The type parameters of the function.
    generics: &'a HashSet<String>,
}

fn block(cx: &Cx, body: &mut [Stmt], returned: Option<usize>, out: &mut Vec<Function>) {
    for (i, stmt) in body.iter_mut().enumerate() {
        match stmt {
            Stmt::Define { value, .. } => {
                value_of(cx, value, true, out);
            }
            Stmt::Expr(value) => {
                value_of(cx, value, returned == Some(i), out);
            }
            Stmt::Assign { .. } | Stmt::Destructure { .. } => {}
        }
        if let Stmt::Define { value, .. } | Stmt::Expr(value) = stmt {
            nested(cx, value, out);
        }
    }
}

/// The blocks that hold bindings of their own: of a `match` and of a `loop`.
fn nested(cx: &Cx, value: &mut Expr, out: &mut Vec<Function>) {
    match value {
        Expr::Match(m) => {
            for arm in &mut m.arms {
                block(cx, &mut arm.body, None, out);
            }
        }
        Expr::Loop(l) => block(cx, &mut l.body, None, out),
        _ => {}
    }
}

/// Lifts the lambda that `value` is, or those it gives to the fields of a
/// struct it builds.
fn value_of(cx: &Cx, value: &mut Expr, kept: bool, out: &mut Vec<Function>) {
    match value {
        Expr::Lambda(l) if kept => {
            if let Some(f) = lift_lambda(cx, l) {
                let name = f.name.clone();
                let pos = l.pos;
                out.push(f);
                *value = Expr::Name { name, pos };
            }
        }
        Expr::Call(call) if is_struct(cx, call) => {
            for Arg {
                value: arg, after, ..
            } in &mut call.args
            {
                if !*after {
                    value_of(cx, arg, true, out);
                }
            }
        }
        _ => {}
    }
}

fn is_struct(cx: &Cx, call: &Call) -> bool {
    call.qualifier.is_none() && call.through.is_none() && cx.structs.contains(call.callee.as_str())
}

/// The function the lambda becomes, if it holds nothing lent and writes every
/// type that its function would need.
fn lift_lambda(cx: &Cx, l: &Lambda) -> Option<Function> {
    let params: Vec<Param> = l
        .params
        .iter()
        .zip(&l.types)
        .map(|((name, pos), ty)| {
            ty.clone().map(|ty| Param {
                name: name.clone(),
                pos: *pos,
                ty,
                default: None,
                take: false,
                takes: Vec::new(),
                effects: Vec::new(),
            })
        })
        .collect::<Option<_>>()?;
    let result = l.result.clone()?;
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
    type_names(&result, &mut named);
    if named
        .iter()
        .any(|n| cx.generics.contains(*n) && !own.contains(n))
    {
        return None;
    }
    let mut mine: HashSet<&str> = l.params.iter().map(|(n, _)| n.as_str()).collect();
    for stmt in crate::check::statements(&l.body) {
        if let Stmt::Define { name, .. } = stmt {
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
        result: Some(result),
        result_name: None,
        effects: Vec::new(),
        requires: Vec::new(),
        needs: Vec::new(),
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
