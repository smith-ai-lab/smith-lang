//! Proof functions: a function whose result is a statement, which is checked
//! and never built.

use super::*;

impl<'a> Checker<'a> {
    /// Checks a proof function: its statement is a `bool` over its
    /// parameters. Its body is a proof, which the kernel checks.
    pub(super) fn proof_function(&mut self, f: usize) {
        let defs = self.defs;
        let (def, sig) = (&defs.file.functions[f], &defs.signatures[f]);
        let Some(statement) = def.result.as_ref().and_then(|t| t.statement.as_deref()) else {
            return;
        };
        let mut body = Body {
            f: Some(def),
            default: None,
            scope: &sig.scope,
            // A proof is never run, so the memory of what it names costs nothing.
            listed: vec!["alloc"],
            extra: Vec::new(),
            unknown_effect: false,
            missing: Vec::new(),
            missing_at: Vec::new(),
            used: Vec::new(),
            names: sig.params.clone(),
            params: sig.params.len(),
            vars: Vec::new(),
            numbers: Numbers::Off,
            decided: HashMap::new(),
            facts: proof::Facts::default(),
            tails: Vec::new(),
            gives: Ty::Nothing,
            contract: false,
            loops: 0,
            outer_loops: 0,
            returns: Ty::Nothing,
            returned: None,
            lambdas: 0,
            kept: 0,
            proofs: Vec::new(),
            call_proofs: HashMap::new(),
            parts: Vec::new(),
        };
        // What the proof's caller owes is what it knows.
        for requirement in &def.requires {
            self.assume(&mut body, requirement, true);
        }
        // A statement is exact arithmetic that is never worked out, so no sum
        // in it is asked to fit.
        body.contract = true;
        self.condition(&mut body, statement);
        body.contract = false;
        if !def.effects.is_empty() {
            self.errors.push(Error::new(
                def.pos,
                format!("`{}` is a proof, which carries no effect", def.name),
            ));
        }
        self.hold_params(&mut body, def);
        let mut names: Vec<String> = body.proofs.iter().map(|(n, _)| n.clone()).collect();
        self.proof_block(&mut body, &def.body, &mut names);
    }
}

impl<'a> Checker<'a> {
    /// Checks the steps of a proof: `same`, `arith` of the proofs named so far,
    /// a name given by `unfold` and the steps after it, or an `if` whose blocks
    /// are steps.
    fn proof_block(&mut self, body: &mut Body<'a>, stmts: &'a [Stmt], names: &mut Vec<String>) {
        match stmts {
            [Stmt::Expr(Expr::Name { name, .. })] if name == "same" => {}
            // A proof given before, which proves what it proves.
            [Stmt::Expr(Expr::Name { name, .. })] if names.contains(name) => {}
            [Stmt::Expr(Expr::Call(c))] if c.callee == "arith" && c.qualifier.is_none() => {
                for arg in &c.args {
                    match &arg.value {
                        Expr::Name { name, .. } if names.contains(name) => {}
                        other => self.errors.push(Error::new(
                            other.pos(),
                            "`arith` is handed the names of proofs given before it",
                        )),
                    }
                }
            }
            [Stmt::Expr(Expr::Match(m))] if matches!(m.form, Form::If { otherwise: true }) => {
                self.condition(body, &m.of);
                let known = body.facts.mark();
                for (k, arm) in m.arms.iter().enumerate() {
                    self.assume(body, &m.of, k == 0);
                    let mark = names.len();
                    self.proof_block(body, &arm.body, names);
                    names.truncate(mark);
                    body.facts.reset(&known);
                }
            }
            [
                Stmt::Define {
                    name,
                    value: Expr::Call(c),
                    var: false,
                    ..
                },
                rest @ ..,
            ] if c.callee == "unfold" && c.qualifier.is_none() => {
                match c.args.as_slice() {
                    [arg] if matches!(arg.value, Expr::Call(_)) => {
                        self.expr(body, &arg.value, &Want::Any);
                    }
                    _ => self.errors.push(Error::new(
                        c.pos,
                        "`unfold` is of one call of a function, as `unfold(f(x))`",
                    )),
                }
                names.push(name.clone());
                self.proof_block(body, rest, names);
            }
            [
                Stmt::Define {
                    name,
                    value: Expr::Call(c),
                    var: false,
                    ..
                },
                rest @ ..,
            ] if c.callee == "replace" && c.qualifier.is_none() => {
                // `replace(eq, p)` is handed two proofs given before it.
                match c.args.as_slice() {
                    [eq, p]
                        if [eq, p].iter().all(|a| {
                            matches!(&a.value, Expr::Name { name, .. } if names.contains(name))
                        }) => {}
                    _ => self.errors.push(Error::new(
                        c.pos,
                        "`replace` is handed two proofs given before it, as `replace(eq, p)`",
                    )),
                }
                names.push(name.clone());
                self.proof_block(body, rest, names);
            }
            [
                Stmt::Define {
                    name,
                    value: value @ Expr::Call(c),
                    var: false,
                    ..
                },
                rest @ ..,
            ] if self.proof_called(c).is_some() => {
                // A call of a proof function is a proof of its statement for
                // the arguments, and is checked as any call is, so a proof that
                // calls itself is held to its `decreases`.
                self.expr(body, value, &Want::Any);
                self.hold_called_proof(body, name, value);
                names.push(name.clone());
                self.proof_block(body, rest, names);
            }
            [first, ..] => self.errors.push(Error::new(
                stmt_pos(first),
                "this is not a step of a proof: `same`, `arith`, a name given by `unfold` or `replace`, or an `if` of steps",
            )),
            [] => self.errors.push(Error::new(
                body.f.map_or(Pos::START, |f| f.pos),
                "a proof has no step",
            )),
        }
    }
}

impl<'a> Checker<'a> {
    /// A name given by a call of a proof function is a proof of its statement
    /// with the call's arguments put in, which the call just proved.
    pub(super) fn hold_called_proof(&mut self, body: &mut Body<'a>, name: &str, value: &Expr) {
        let Expr::Call(c) = value else { return };
        if self.proof_called(c).is_none() {
            return;
        }
        if let Some(held) = body.call_proofs.get(&addr(c)).cloned() {
            body.proofs.push((name.to_string(), held));
        }
    }

    /// A statement names no `var`: a proof of one says nothing once the `var`
    /// holds another value. So an argument that names a `var` is refused where
    /// the statement of a proof a call gives, or is passed, is about it.
    pub(super) fn statements_name_no_var(
        &mut self,
        body: &Body<'a>,
        args: &[(&Expr, usize)],
        callee: &Function,
    ) {
        let result = callee.result.as_ref().and_then(|t| t.statement.as_deref());
        let mentions = |statement: &Expr, param: &str| {
            let mut all = Vec::new();
            walk(statement, &mut all);
            all.iter()
                .any(|e| matches!(e, Expr::Name { name, .. } if name == param))
        };
        let statements: Vec<&Expr> = result
            .into_iter()
            .chain(args.iter().filter_map(|&(_, place)| owed_by(callee, place)))
            .collect();
        for &(arg, place) in args {
            let param = callee.params[place].name.as_str();
            if callee.params[place].ty.statement.is_some()
                || !statements.iter().any(|s| mentions(s, param))
            {
                continue;
            }
            let mut all = Vec::new();
            walk(arg, &mut all);
            for e in all {
                if let Expr::Name { name, pos } = e
                    && body.vars.contains(name)
                {
                    self.errors.push(
                        Error::new(
                            *pos,
                            format!(
                                "a proof is of a statement that names no `var`, and `{name}` is one"
                            ),
                        )
                        .with_fix("bind its value to a name that is not a `var` first"),
                    );
                }
            }
        }
    }

    /// The proof function a call is of, if it is one.
    pub(super) fn proof_called(&self, c: &Call) -> Option<usize> {
        if c.qualifier.is_some() {
            return None;
        }
        let f = self.defs.function(c.name_pos, &c.callee)?;
        is_proof(&self.defs.file.functions[f]).then_some(f)
    }
}

fn stmt_pos(s: &Stmt) -> Pos {
    match s {
        Stmt::Define { pos, .. } | Stmt::Assign { pos, .. } => *pos,
        Stmt::Destructure { pattern, .. } => pattern.pos(),
        Stmt::Expr(e) => e.pos(),
    }
}

/// What a proof says, which is what a name that holds one is a proof of: the
/// statement as `arith` reads it, where it does, and as it is written.
#[derive(Clone)]
pub(super) struct Held {
    pub(super) formula: Option<super::arith::Formula>,
    pub(super) text: String,
}

/// The statement of the parameter at `place` that is a proof, which the parser
/// also put among the `requires`, so that it is checked as they are.
pub(super) fn owed_by(f: &Function, place: usize) -> Option<&Expr> {
    f.params[place].ty.statement.as_ref()?;
    let before = f.params[..place]
        .iter()
        .filter(|p| p.ty.statement.is_some())
        .count();
    f.requires.get(before)
}

impl<'a> Checker<'a> {
    /// What `e` says now, as a proof of it says.
    pub(super) fn held(&mut self, body: &mut Body<'a>, e: &Expr) -> Held {
        let mut facts = std::mem::take(&mut body.facts);
        let formula = self.said(body, &mut facts, e, true);
        body.facts = facts;
        Held {
            formula,
            text: proof::written_contract(&|_| None, e),
        }
    }

    /// The proofs a function is given are its parameters whose type is a
    /// statement, each a proof of it.
    pub(super) fn hold_params(&mut self, body: &mut Body<'a>, f: &'a Function) {
        for place in 0..f.params.len() {
            if let Some(statement) = owed_by(f, place) {
                let held = self.held(body, statement);
                body.proofs.push((f.params[place].name.clone(), held));
            }
        }
    }
}
