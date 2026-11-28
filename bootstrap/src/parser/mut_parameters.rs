//! A parameter marked `mut` means taking the value and giving it back, as
//! `knowledge/language/mut-parameters.md` says, and it is lowered to that here: the
//! function takes the parameter and gives it back after its body and at each
//! `return`, before what the function gives if it gives a result. A call that
//! passes `mut x` on a line of its own assigns what the call gives back to
//! `x`, and one that is the whole right-hand side of a binding or an
//! assignment also binds or assigns what the function gives. The checker then
//! sees what a program that is written with `take` would be, and refuses what
//! `mut` refuses.

use super::*;

/// A place a call changes: the name, where it is, the fields after it, and the
/// item they name, if it is one.
pub(crate) type Place = (String, Pos, Vec<Named>, Element);

/// Lowers each function of `functions` that has a parameter marked `mut`,
/// and each call on a line of its own that passes a place `mut`. What is not
/// built of `mut` goes in `unbuilt`.
pub(super) fn lower(
    functions: &mut [Function],
    (impls, traits): (&[Impl], &[Trait]),
    errors: &mut Vec<Error>,
    unbuilt: &mut Vec<Error>,
) {
    let declared =
        (impls.iter().flat_map(|i| &i.functions)).chain(traits.iter().flat_map(|t| &t.functions));
    for f in declared {
        if let Some(p) = f.params.iter().find(|p| p.mutable) {
            unbuilt.push(Error::unsupported(
                p.pos,
                "`mut` in a trait or an implementation",
            ));
        }
    }
    for f in functions.iter_mut() {
        if f.params.iter().any(|p| p.mutable) {
            held_by_lambdas(f, errors);
            give_back(f, errors, unbuilt);
        }
        statements(&mut f.body);
    }
}

/// Refuses each lambda in a function that reads or assigns a parameter of the
/// function marked `mut`: the function gives it back when it returns, and
/// nothing else holds it.
fn held_by_lambdas(f: &mut Function, errors: &mut Vec<Error>) {
    let changed: Vec<String> = (f.params.iter())
        .filter(|p| p.mutable)
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
                    "`{name}` is marked `mut`, so `{}` gives it back when it returns, and a lambda cannot hold it",
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

/// Whether a function was lowered to give back the parameters it marks
/// `mut`, so that its result is not one its source writes.
pub(crate) fn gives_back(f: &Function) -> bool {
    f.params.iter().any(|p| p.mutable && p.take)
}

/// The result a function lowered to give back its parameters marked `mut`
/// gives besides them, as its source writes it after `->`. The lowered result
/// is the one parameter, or a tuple of the parameters, when there is none,
/// and a tuple of the parameters and it when there is one.
pub(crate) fn own_result(f: &Function) -> Option<&Type> {
    if !gives_back(f) {
        return None;
    }
    let changed: Vec<&Type> = (f.params.iter().filter(|p| p.mutable))
        .map(|p| &p.ty)
        .collect();
    let result = f.result.as_ref()?;
    match changed[..] {
        [one] if one == result => None,
        _ if result.args.len() == changed.len() + 1 => result.args.last(),
        _ => None,
    }
}

/// Makes a function with parameters marked `mut` take them and give them
/// back.
fn give_back(f: &mut Function, errors: &mut Vec<Error>, unbuilt: &mut Vec<Error>) {
    let first = f.params.iter().find(|p| p.mutable).expect("one is mut");
    if is_operation(f) {
        unbuilt.push(Error::unsupported(first.pos, "`mut` in an operation"));
        return;
    }
    if let Some(r) = f
        .result_name
        .as_ref()
        .map(|(_, at)| *at)
        .or(f.ensures.first().map(Expr::pos))
        && f.result.is_some()
    {
        unbuilt.push(Error::unsupported(
            r,
            "`ensures` or a named result in a function that changes a parameter marked `mut` and gives a result",
        ));
        return;
    }
    let changed: Vec<(String, Type)> = (f.params.iter_mut())
        .filter(|p| p.mutable)
        .map(|p| {
            p.take = true;
            (p.name.clone(), p.ty.clone())
        })
        .collect();
    let names: Vec<&str> = changed.iter().map(|(n, _)| n.as_str()).collect();
    let own = f.result.take();
    let mut types: Vec<Type> = changed.iter().map(|(_, t)| t.clone()).collect();
    types.extend(own.clone());
    f.result = Some(match &types[..] {
        [ty] => ty.clone(),
        _ => Type {
            name: String::new(),
            pos: changed[0].1.pos,
            args: types,
            function: false,
            range: None,
            statement: None,
            sizes: Vec::new(),
        },
    });
    let gives = own.is_some();
    returns(&mut f.body, &names, (&f.name, gives, errors));
    match f.body.last_mut() {
        // A body that ends with `return` gives the parameters back there.
        Some(Stmt::Expr(Expr::Return { .. })) => {}
        // What the body ends with comes after the parameters.
        Some(Stmt::Expr(last)) if gives => {
            let value = std::mem::replace(last, Expr::Break { pos: f.end });
            *last = given(&names, Some(value), f.end);
        }
        _ if gives => {}
        _ => f.body.push(Stmt::Expr(given(&names, None, f.end))),
    }
}

/// What a function gives back: the parameter marked `mut`, or a tuple of
/// them when it has more than one, and after them what it gives, when it
/// gives a result.
fn given(names: &[&str], value: Option<Expr>, pos: Pos) -> Expr {
    let name = |n: &&str| Expr::Name {
        name: n.to_string(),
        pos,
    };
    match (names, value) {
        ([one], None) => name(one),
        (_, value) => Expr::Tuple {
            pos,
            items: names.iter().map(name).chain(value).collect(),
        },
    }
}

/// The function a `return` is in, whether it gives a result, and the
/// mistakes found.
type Within<'f> = (&'f str, bool, &'f mut Vec<Error>);

/// Makes each `return` of the function, and not of a lambda in it, give back
/// the parameters marked `mut`, before its value when the function gives a
/// result. A `return` with a value is refused when the function gives
/// nothing, and one without when it gives a result.
fn returns(stmts: &mut [Stmt], names: &[&str], within: Within) {
    let (f, gives, errors) = within;
    for stmt in stmts {
        each_expr(stmt, &mut |e| returns_in(e, names, (f, gives, errors)));
    }
}

fn returns_in(e: &mut Expr, names: &[&str], (f, gives, errors): Within) {
    match e {
        Expr::Return { value, pos } => match value.take() {
            None if gives => errors.push(
                Error::new(
                    *pos,
                    format!("`{f}` gives a result, so `return` is written with a value"),
                )
                .with_fix("write the value after `return`"),
            ),
            None => *value = Some(Box::new(given(names, None, *pos))),
            Some(mut v) => {
                returns_in(&mut v, names, (f, gives, errors));
                if gives {
                    *value = Some(Box::new(given(names, Some(*v), *pos)));
                } else {
                    errors.push(
                        Error::new(
                            *pos,
                            format!("`{f}` returns nothing, so `return` is written alone"),
                        )
                        .with_fix("take away the value after `return`"),
                    );
                    *value = Some(v);
                }
            }
        },
        Expr::Lambda(_) => {}
        _ => {
            each_child(e, &mut |c| returns_in(c, names, (f, gives, errors)));
            each_block(e, &mut |b| returns(b, names, (f, gives, errors)));
        }
    }
}

/// Lowers each call that passes places `mut` in the statements, and in the
/// blocks inside them, that is a line of its own or the whole right-hand side
/// of a binding or an assignment.
fn statements(stmts: &mut Vec<Stmt>) {
    let mut out = Vec::with_capacity(stmts.len());
    for mut stmt in stmts.drain(..) {
        each_expr(&mut stmt, &mut blocks_in);
        match stmt {
            Stmt::Expr(Expr::Call(call)) if lowered(&call).is_some() => {
                out.extend(assigned(call, None));
            }
            Stmt::Define {
                name,
                pos,
                ty,
                var,
                value: Expr::Call(call),
            } if lowered(&call).is_some() => {
                let at = var.then_some(pos);
                let kept = match ty {
                    // A name and nothing more is bound with the parameters.
                    None => Kept::Bound(Pattern::Bind { name, pos, var: at }),
                    ty => Kept::Then(Box::new(move |value| Stmt::Define {
                        name,
                        pos,
                        ty,
                        var,
                        value,
                    })),
                };
                out.extend(assigned(call, Some(kept)));
            }
            Stmt::Assign {
                name,
                pos,
                fields,
                at,
                value: Expr::Call(call),
            } if lowered(&call).is_some() => {
                let kept = Kept::Then(Box::new(move |value| Stmt::Assign {
                    name,
                    pos,
                    fields,
                    at,
                    value,
                }));
                out.extend(assigned(call, Some(kept)));
            }
            stmt => out.push(stmt),
        }
    }
    *stmts = out;
}

/// What a call that passes places `mut` does with what its function gives:
/// binds it with the places, or gives it to a statement after them.
enum Kept {
    Bound(Pattern),
    Then(Box<dyn FnOnce(Expr) -> Stmt>),
}

fn blocks_in(e: &mut Expr) {
    each_block(e, &mut statements);
    each_child(e, &mut blocks_in);
}

/// The places a call passes `mut`, when it passes one and each is a place.
fn lowered(call: &Call) -> Option<Vec<Place>> {
    let places: Option<Vec<_>> = (call.args.iter())
        .filter(|a| a.mutable.is_some())
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

/// The statements a call that passes places `mut` is: the call, with what it
/// gives back assigned to each place, and what its function gives kept as
/// `kept` says.
fn assigned(mut call: Call, kept: Option<Kept>) -> Vec<Stmt> {
    let places = lowered(&call).expect("lowered");
    for a in call.args.iter_mut().filter(|a| a.mutable.is_some()) {
        a.given_back = true;
        a.keeps = kept.is_some();
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
    if places.len() == 1 && kept.is_none() {
        let place = places.into_iter().next().expect("one");
        return vec![assign(place, value)];
    }
    let bind = |name: String, pos| Pattern::Bind {
        name,
        pos,
        var: None,
    };
    // What the function gives is the last item of the tuple, bound as it is
    // written or held for the statement that takes it.
    let result = format!("mut@{}:{}.result", pos.line, pos.col);
    let (last, then) = match kept {
        None => (None, None),
        Some(Kept::Bound(p)) => (Some(p), None),
        Some(Kept::Then(then)) => {
            let held = Expr::Name {
                name: result.clone(),
                pos,
            };
            (Some(bind(result, pos)), Some(then(held)))
        }
    };
    let names: Vec<&str> = places.iter().map(|(n, ..)| n.as_str()).collect();
    let distinct = (names.iter().enumerate()).all(|(i, n)| !names[..i].contains(n));
    if distinct
        && places
            .iter()
            .all(|(_, _, f, at)| f.is_empty() && at.is_none())
    {
        let items = (places.into_iter().map(|(n, p, ..)| bind(n, p)))
            .chain(last)
            .collect();
        let made = Stmt::Destructure {
            pattern: Pattern::Tuple { pos, items },
            ty: None,
            value,
        };
        return std::iter::once(made).chain(then).collect();
    }
    let held = |i: usize| format!("mut@{}:{}.{i}", pos.line, pos.col);
    let items = ((0..places.len()).map(|i| bind(held(i), pos)))
        .chain(last)
        .collect();
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
    out.extend(then);
    out
}

/// Gives `visit` the expression each statement holds.
pub(super) fn each_expr(stmt: &mut Stmt, visit: &mut dyn FnMut(&mut Expr)) {
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
pub(super) fn each_child(e: &mut Expr, visit: &mut dyn FnMut(&mut Expr)) {
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
        Expr::For(_)
        | Expr::Unbuilt { .. }
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
/// `match`, the body of a loop or a `for`, and the body of a lambda.
pub(super) fn each_block(e: &mut Expr, visit: &mut dyn FnMut(&mut Vec<Stmt>)) {
    match e {
        Expr::Match(m) => {
            for arm in &mut m.arms {
                visit(&mut arm.body);
            }
        }
        Expr::Loop(l) => visit(&mut l.body),
        Expr::For(f) => visit(&mut f.round.body),
        Expr::Lambda(l) => visit(&mut l.body),
        _ => {}
    }
}
