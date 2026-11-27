//! A parameter marked `inout` means taking the value and giving it back, as
//! `knowledge/language/inout.md` says, and it is lowered to that here: the
//! function takes the parameter and gives it back after its body and at each
//! `return`, and a call that passes `inout x` on a line of its own assigns
//! what the call gives back to `x`. The checker then sees what a program that
//! is written with `take` would be, and refuses what `inout` refuses.

use super::*;

/// A place a call changes: the name, where it is, the fields after it, and the
/// item they name, if it is one.
pub(crate) type Place = (String, Pos, Vec<Named>, Element);

/// Lowers each function of `functions` that has a parameter marked `inout`,
/// and each call on a line of its own that passes a place `inout`. What is not
/// built of `inout` goes in `unbuilt`.
pub(super) fn lower(
    functions: &mut [Function],
    (impls, traits): (&[Impl], &[Trait]),
    errors: &mut Vec<Error>,
    unbuilt: &mut Vec<Error>,
) {
    let declared =
        (impls.iter().flat_map(|i| &i.functions)).chain(traits.iter().flat_map(|t| &t.functions));
    for f in declared {
        if let Some(p) = f.params.iter().find(|p| p.inout) {
            unbuilt.push(Error::unsupported(
                p.pos,
                "`inout` in a trait or an implementation",
            ));
        }
    }
    for f in functions.iter_mut() {
        if f.params.iter().any(|p| p.inout) {
            held_by_lambdas(f, errors);
            gives_back(f, errors, unbuilt);
        }
        statements(&mut f.body);
    }
}

/// Refuses each lambda in a function that reads or assigns a parameter of the
/// function marked `inout`: the function gives it back when it returns, and
/// nothing else holds it.
fn held_by_lambdas(f: &mut Function, errors: &mut Vec<Error>) {
    let changed: Vec<String> = (f.params.iter())
        .filter(|p| p.inout)
        .map(|p| p.name.clone())
        .collect();
    let mut lambdas = Vec::new();
    for stmt in &mut f.body {
        each_expr(stmt, &mut |e| lambdas_in(e, &mut lambdas));
    }
    for mut l in lambdas {
        let mut held = Vec::new();
        names_in_block(&mut l.body, &changed, &l.params, &mut held);
        if let Some((name, pos)) = held.into_iter().min_by_key(|(_, p)| (p.line, p.col)) {
            errors.push(Error::new(
                pos,
                format!(
                    "`{name}` is marked `inout`, so `{}` gives it back when it returns, and a lambda cannot hold it",
                    f.name
                ),
            ));
        }
    }
}

/// Copies of the lambdas in an expression, the outermost of each nest.
fn lambdas_in(e: &mut Expr, out: &mut Vec<Lambda>) {
    if let Expr::Lambda(l) = e {
        out.push((**l).clone());
        return;
    }
    each_child(e, &mut |c| lambdas_in(c, out));
    each_block(e, &mut |b| {
        for stmt in b {
            each_expr(stmt, &mut |c| lambdas_in(c, out));
        }
    });
}

/// The names among `changed` an expression reads, and those its blocks
/// assign, with where they are, except those that `params` stands for.
fn names_in(e: &mut Expr, changed: &[String], params: &[Named], out: &mut Vec<(String, Pos)>) {
    match e {
        Expr::Name { name, pos } if changed.contains(name) => {
            if !params.iter().any(|(p, _)| p == name) {
                out.push((name.clone(), *pos));
            }
        }
        _ => {
            each_child(e, &mut |c| names_in(c, changed, params, out));
            each_block(e, &mut |b| names_in_block(b, changed, params, out));
        }
    }
}

/// The names among `changed` a block reads or assigns, as `names_in` says.
fn names_in_block(
    stmts: &mut [Stmt],
    changed: &[String],
    params: &[Named],
    out: &mut Vec<(String, Pos)>,
) {
    for stmt in stmts {
        if let Stmt::Define { name, pos, .. } | Stmt::Assign { name, pos, .. } = stmt
            && changed.contains(name)
            && !params.iter().any(|(p, _)| p == name)
        {
            out.push((name.clone(), *pos));
        }
        each_expr(stmt, &mut |c| names_in(c, changed, params, out));
    }
}

/// Makes a function with parameters marked `inout` take them and give them
/// back.
fn gives_back(f: &mut Function, errors: &mut Vec<Error>, unbuilt: &mut Vec<Error>) {
    let first = f.params.iter().find(|p| p.inout).expect("one is inout");
    if is_operation(f) {
        unbuilt.push(Error::unsupported(first.pos, "`inout` in an operation"));
        return;
    }
    if f.result.is_some() {
        unbuilt.push(Error::unsupported(
            first.pos,
            "a function that changes a parameter marked `inout` and gives a result",
        ));
        return;
    }
    let changed: Vec<(String, Type)> = (f.params.iter_mut())
        .filter(|p| p.inout)
        .map(|p| {
            p.take = true;
            (p.name.clone(), p.ty.clone())
        })
        .collect();
    let names: Vec<&str> = changed.iter().map(|(n, _)| n.as_str()).collect();
    f.result = Some(match &changed[..] {
        [(_, ty)] => ty.clone(),
        _ => Type {
            name: String::new(),
            pos: first_pos(&changed),
            args: changed.iter().map(|(_, t)| t.clone()).collect(),
            function: false,
            range: None,
            statement: None,
            length: None,
        },
    });
    returns(&mut f.body, &names, (&f.name, errors));
    f.body.push(Stmt::Expr(given(&names, f.end)));
}

fn first_pos(changed: &[(String, Type)]) -> Pos {
    changed[0].1.pos
}

/// What a function gives back: the parameter marked `inout`, or a tuple of
/// them when it has more than one.
fn given(names: &[&str], pos: Pos) -> Expr {
    let name = |n: &&str| Expr::Name {
        name: n.to_string(),
        pos,
    };
    match names {
        [one] => name(one),
        _ => Expr::Tuple {
            pos,
            items: names.iter().map(name).collect(),
        },
    }
}

/// The function a `return` is in, and the mistakes found.
type Within<'f> = (&'f str, &'f mut Vec<Error>);

/// Makes each bare `return` of the function, and not of a lambda in it, give
/// back the parameters marked `inout`. A `return` with a value is refused, as
/// the function gives nothing.
fn returns(stmts: &mut [Stmt], names: &[&str], within: Within) {
    let (f, errors) = within;
    for stmt in stmts {
        each_expr(stmt, &mut |e| returns_in(e, names, (f, errors)));
    }
}

fn returns_in(e: &mut Expr, names: &[&str], (f, errors): Within) {
    match e {
        Expr::Return { value, pos } => match value {
            None => *value = Some(Box::new(given(names, *pos))),
            Some(v) => {
                errors.push(
                    Error::new(
                        *pos,
                        format!("`{f}` returns nothing, so `return` is written alone"),
                    )
                    .with_fix("take away the value after `return`"),
                );
                returns_in(v, names, (f, errors));
            }
        },
        Expr::Lambda(_) => {}
        _ => {
            each_child(e, &mut |c| returns_in(c, names, (f, errors)));
            each_block(e, &mut |b| returns(b, names, (f, errors)));
        }
    }
}

/// Lowers each call on a line of its own in the statements, and in the
/// blocks inside them, that passes places `inout`.
fn statements(stmts: &mut Vec<Stmt>) {
    let mut out = Vec::with_capacity(stmts.len());
    for mut stmt in stmts.drain(..) {
        each_expr(&mut stmt, &mut blocks_in);
        match stmt {
            Stmt::Expr(Expr::Call(call)) if lowered(&call).is_some() => {
                out.extend(assigned(call));
            }
            stmt => out.push(stmt),
        }
    }
    *stmts = out;
}

fn blocks_in(e: &mut Expr) {
    each_block(e, &mut statements);
    each_child(e, &mut blocks_in);
}

/// The places a call passes `inout`, when it passes one and each is a place.
fn lowered(call: &Call) -> Option<Vec<Place>> {
    let places: Option<Vec<_>> = (call.args.iter())
        .filter(|a| a.inout.is_some())
        .map(|a| changed_place(&a.value))
        .collect();
    places.filter(|p| !p.is_empty())
}

/// The place an expression names, if it names one: a name, or a field or an
/// item of one.
pub(crate) fn changed_place(e: &Expr) -> Option<Place> {
    match e {
        Expr::Name { name, pos } => Some((name.clone(), *pos, Vec::new(), None)),
        e => place(e),
    }
}

/// The statements a call that passes places `inout` is: the call, with what it
/// gives back assigned to each place.
fn assigned(mut call: Call) -> Vec<Stmt> {
    let places = lowered(&call).expect("lowered");
    for a in call.args.iter_mut().filter(|a| a.inout.is_some()) {
        a.given_back = true;
    }
    let assign = |(name, pos, fields, at): Place, value| {
        if fields.is_empty() && at.is_none() {
            Stmt::Define {
                name,
                pos,
                ty: None,
                var: false,
                value,
            }
        } else {
            Stmt::Assign {
                name,
                pos,
                fields,
                at,
                value,
            }
        }
    };
    let pos = call.pos;
    let value = Expr::Call(call);
    if places.len() == 1 {
        let place = places.into_iter().next().expect("one");
        return vec![assign(place, value)];
    }
    let bind = |name: String, pos| Pattern::Bind {
        name,
        pos,
        var: None,
    };
    let names: Vec<&str> = places.iter().map(|(n, ..)| n.as_str()).collect();
    let distinct = (names.iter().enumerate()).all(|(i, n)| !names[..i].contains(n));
    if distinct
        && places
            .iter()
            .all(|(_, _, f, at)| f.is_empty() && at.is_none())
    {
        let items = places.into_iter().map(|(n, p, ..)| bind(n, p)).collect();
        return vec![Stmt::Destructure {
            pattern: Pattern::Tuple { pos, items },
            ty: None,
            value,
        }];
    }
    let held = |i: usize| format!("inout@{}:{}.{i}", pos.line, pos.col);
    let items = (0..places.len()).map(|i| bind(held(i), pos)).collect();
    let mut out = vec![Stmt::Destructure {
        pattern: Pattern::Tuple { pos, items },
        ty: None,
        value,
    }];
    for (i, place) in places.into_iter().enumerate() {
        let at = place.1;
        out.push(assign(
            place,
            Expr::Name {
                name: held(i),
                pos: at,
            },
        ));
    }
    out
}

/// Gives `visit` the expression each statement holds.
fn each_expr(stmt: &mut Stmt, visit: &mut dyn FnMut(&mut Expr)) {
    match stmt {
        Stmt::Define { value, .. } | Stmt::Destructure { value, .. } | Stmt::Expr(value) => {
            visit(value)
        }
        Stmt::Assign { value, at, .. } => {
            if let Some(at) = at {
                visit(&mut at.0);
                visit(&mut at.1);
            }
            visit(value);
        }
    }
}

/// Gives `visit` each expression an expression holds directly, and not those
/// of its blocks.
fn each_child(e: &mut Expr, visit: &mut dyn FnMut(&mut Expr)) {
    match e {
        Expr::Call(call) => {
            if let Some(inner) = &mut call.through {
                visit(inner);
            }
            for arg in &mut call.args {
                visit(&mut arg.value);
            }
        }
        Expr::Interp { parts, .. } => {
            for part in parts {
                if let Part::Expr(e) = part {
                    visit(e);
                }
            }
        }
        Expr::Field { of, .. } => visit(of),
        Expr::Case {
            payload: Some(p), ..
        } => visit(p),
        Expr::Match(m) => visit(&mut m.of),
        Expr::Arith { left, right, .. } => {
            visit(left);
            visit(right);
        }
        Expr::Tuple { items, .. } | Expr::List { items, .. } => {
            for item in items {
                visit(item);
            }
        }
        Expr::Index { of, index, .. } => {
            visit(of);
            visit(index);
        }
        Expr::Slice { of, from, to, .. } => {
            visit(of);
            for end in [from, to].into_iter().flatten() {
                visit(end);
            }
        }
        Expr::Loop(l) => {
            if let Some(r) = &mut l.range {
                visit(&mut r.from);
                visit(&mut r.to);
            }
            if let Some(o) = &mut l.over {
                visit(&mut o.value);
            }
        }
        Expr::Return { value: Some(v), .. } => visit(v),
        Expr::Unbuilt { .. }
        | Expr::Text { .. }
        | Expr::Number { .. }
        | Expr::Float { .. }
        | Expr::Name { .. }
        | Expr::Case { payload: None, .. }
        | Expr::Lambda(_)
        | Expr::Break { .. }
        | Expr::Return { value: None, .. } => {}
    }
}

/// Gives `visit` each block an expression holds directly: the arms of a
/// `match`, the body of a loop, and the body of a lambda.
fn each_block(e: &mut Expr, visit: &mut dyn FnMut(&mut Vec<Stmt>)) {
    match e {
        Expr::Match(m) => {
            for arm in &mut m.arms {
                visit(&mut arm.body);
            }
        }
        Expr::Loop(l) => visit(&mut l.body),
        Expr::Lambda(l) => visit(&mut l.body),
        _ => {}
    }
}
