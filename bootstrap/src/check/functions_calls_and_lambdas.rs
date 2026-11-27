//! Checks functions, calls, and lambdas: a function's signature, parameters,
//! and defaults, what it returns, a call and its arguments, a lambda, a
//! function used as a value, and calls that close a cycle.

use super::*;

impl<'a> Checker<'a> {
    /// At a call of a function from itself, `arith` must prove that the
    /// measure of its `decreases`, worked out from the arguments, is not below
    /// zero and is smaller than the measure now. A call that is not the last
    /// thing the function does keeps a frame, in the memory of `alloc`, so the
    /// function must carry `alloc`. Every call of a function that has no
    /// `decreases` is refused where `recursion` looks for them, and is not
    /// looked at here.
    ///
    /// A call of another function that has a `decreases` is held to the same
    /// rule, with the measure the callee states, and the goal that cannot be
    /// proven is kept: it is a mistake only when the callee calls the function
    /// back, as `recursion` finds.
    fn call_decreases(&mut self, body: &mut Body<'a>, call: &Call, callee: usize, sig: &Signature) {
        let defs = self.defs;
        let Some(own) = body.f else { return };
        let target = &*defs.file.functions[callee];
        let itself = std::ptr::eq(own, target);
        if itself {
            if body.lambdas > 0 && !body.contract {
                self.errors.push(Error::unsupported(
                    call.pos,
                    "a call of a function from itself inside a lambda",
                ));
                return;
            }
            // A proof is never run, so what it does last costs nothing.
            if !tail_positions(own).contains(&call.pos) && !body.contract && !is_proof(own) {
                self.needs(
                    body,
                    "alloc",
                    call.pos,
                    "a call of itself that is not the last thing the function does",
                );
            }
        }
        if own.decreases.is_empty()
            || target.decreases.is_empty()
            || (!itself && own.decreases.len() != target.decreases.len())
            || body.lambdas > 0
            || body.contract
            || matches!(body.numbers, Numbers::Finding(_))
        {
            return;
        }
        // A call with an argument for no parameter is reported, and proves nothing.
        let Some(places) = params_of(&sig.params, sig.by_position(), &call.args) else {
            return;
        };
        let mut given: Vec<Option<&Expr>> = vec![None; target.params.len()];
        for (arg, place) in call.args.iter().zip(places) {
            given[place] = Some(&arg.value);
        }
        // What a parameter is at the call is its argument, or its default when
        // the call leaves it out.
        let args: Vec<(&str, &Expr)> = (target.params.iter().zip(&given))
            .filter_map(|(p, g)| g.or(p.default.as_ref()).map(|e| (p.name.as_str(), e)))
            .collect();
        let outcome = match (
            self.measure_of(body, &own.decreases, &[]),
            self.measure_of(body, &target.decreases, &args),
        ) {
            (Some(before), Some(after)) => self.decreased_by(body, &own.decreases, &before, &after),
            _ => Err(proof::Unproven::Smaller(
                own.decreases
                    .iter()
                    .map(written_expr)
                    .collect::<Vec<_>>()
                    .join(", "),
            )),
        };
        let goal = match outcome {
            Ok(()) => return,
            Err(proof::Unproven::Smaller(m)) => format!("`{m}` is smaller at this call"),
            Err(proof::Unproven::NotBelowZero(m)) => {
                format!("`{m}` is not below zero at this call")
            }
        };
        let error = Error::new(
            call.pos,
            format!("cannot prove that {goal}, so `{}` may not end", own.name),
        )
        .with_fix(
            "check what the call needs first with an `if`, or make each call make the \
             measure smaller",
        );
        if itself {
            self.errors.push(error);
        } else if let Some(from) = defs
            .file
            .functions
            .iter()
            .position(|f| std::ptr::eq(&**f, own))
            && !self
                .cross
                .iter()
                .any(|(a, b, e)| (*a, *b) == (from, callee) && *e == error)
        {
            self.cross.push((from, callee, error));
        }
    }

    /// Checks the parts of the `decreases` of a function: each is a whole number
    /// of an unsigned type over the parameters, which is never computed when
    /// the program runs.
    pub(super) fn measure_parts(&mut self, body: &mut Body<'a>, f: &Function) {
        for e in &f.decreases {
            // It is an exact number, never computed, so what it does with a
            // number that does not fit is nothing the program asks of `arith`.
            let contract = std::mem::replace(&mut body.contract, true);
            let gives = self.expr(body, e, &Want::Any);
            body.contract = contract;
            // A number that is written is of the type the other parts are, as
            // the `2` of `decreases toks.size - at, 2` is.
            let written = matches!(
                e,
                Expr::Number {
                    negative: false,
                    ..
                }
            );
            if !written
                && !matches!(gives, Ty::Int(t) if !t.signed())
                && !matches!(gives, Ty::Unknown | Ty::Flex(_))
            {
                let given = self.defs.describe(&gives, body.scope);
                self.errors.push(Error::new(
                    e.pos(),
                    format!(
                        "`decreases` is a whole number of an unsigned type, and this is {given}"
                    ),
                ));
            }
        }
    }

    /// Checks parameters, each named once in `f`, and the types of the
    /// parameters and the result, among the type parameters of `scope`.
    pub(super) fn params(&mut self, f: &Function, scope: &Scope) {
        for (i, p) in f.params.iter().enumerate() {
            if f.params[..i].iter().any(|q| q.name == p.name) {
                self.errors.push(Error::new(
                    p.pos,
                    format!("`{}` is already defined in `{}`", p.name, f.name),
                ));
            }
            self.param_ty(&p.ty, scope);
        }
        match &f.result {
            // A function that gives a function gives the one function it names.
            Some(t) if t.function => {
                if let Some(Ty::Named(g)) = self.defs.returned_function(f, scope) {
                    self.returned_effects(t.pos, g);
                }
                if self.defs.returned_function(f, scope).is_none()
                    && self.defs.returned_parameter(f).is_none()
                {
                    self.errors.extend(self.which_function(f, scope, t.pos));
                }
            }
            // A struct that holds functions is the one the body builds.
            Some(t) if self.defs.implicit_result(t, scope).is_some() => {
                let u = self.defs.implicit_result(t, scope).expect("checked");
                if self.defs.returned_struct(f, u, scope).is_none() {
                    self.errors.extend(self.which_function(f, scope, t.pos));
                }
            }
            Some(t) => {
                self.ty(t, scope);
            }
            None => {}
        }
    }

    /// The error for a function whose result is a function, or holds one, that is
    /// not known from its body: a lambda that is given and reads a name that is
    /// not a parameter, whose type is not known before the body is checked, or a
    /// choice between functions. A lambda that reads a value that is lent is
    /// refused where it is written, and says so there.
    fn which_function(&self, f: &Function, scope: &Scope, at: Pos) -> Option<Error> {
        let defs = self.defs;
        let mut found = Vec::new();
        for tail in super::tails(&f.body) {
            super::walk(tail, &mut found);
        }
        let lambdas: Vec<&Lambda> = found
            .into_iter()
            .filter_map(|e| match e {
                Expr::Lambda(l) if defs.env_of(l).is_some() => Some(&**l),
                _ => None,
            })
            .collect();
        let local = lambdas.iter().find_map(|l| {
            let s = defs.env_of(l)?;
            let field = defs.file.structs[s]
                .fields
                .iter()
                .find(|field| !f.params.iter().any(|p| p.name == field.name))?;
            Some((l.pos, field.name.as_str()))
        });
        // Each lambda that is given reads a parameter whose value is lent.
        let lent = !lambdas.is_empty()
            && lambdas.iter().all(|l| {
                let s = defs.env_of(l).expect("checked");
                defs.file.structs[s].fields.iter().any(|field| {
                    (f.params.iter().find(|p| p.name == field.name)).is_some_and(|p| {
                        !ownership::copies(defs, scope, &defs.quietly(&p.ty, scope))
                    })
                })
            });
        match local {
            Some((pos, name)) => Some(Error::new(
                pos,
                format!(
                    "a lambda that is given reads only the parameters of its function, as \
                     what a function gives is known before its body is checked, and this one \
                     reads `{name}`"
                ),
            )),
            None if lent => None,
            None => Some(Error::new(at, WHICH_FUNCTION)),
        }
    }

    /// Reports the effects of function `g`, written at `value`, where it goes
    /// into a function type, which `place` says, as a function type states no
    /// effect. Whoever calls what is stored is checked against the type alone.
    pub(super) fn stored_effects(&mut self, value: &Expr, g: usize, place: &str) {
        let defs = self.defs;
        let signature = &defs.signatures[g];
        if let Some(effect) = signature.effects.first() {
            let name = written_name(&defs.file.functions[g].name);
            self.errors.push(Error::new(
                value.pos(),
                format!(
                    "`{name}` needs the `{}` effect, and {place} states none, so a function that \
                     needs one is not stored there",
                    written_name(effect)
                ),
            ));
        }
    }

    /// Reports the effect of function `g`, which a function gives as its result,
    /// whose type states none.
    fn returned_effects(&mut self, pos: Pos, g: usize) {
        let defs = self.defs;
        if let Some(effect) = defs.signatures[g].effects.first() {
            let name = written_name(&defs.file.functions[g].name);
            self.errors.push(Error::new(
                pos,
                format!(
                    "`{name}` needs the `{}` effect, and this result type states none, so a \
                     function that needs one is not given",
                    written_name(effect)
                ),
            ));
        }
    }

    /// Checks the signature of function `f`: its type parameters, each
    /// named by a parameter's type, its types, and its list of effects.
    pub(super) fn signature(&mut self, f: usize) {
        let defs = self.defs;
        let def = &defs.file.functions[f];
        if defs.function_names.get("main") != Some(&f) {
            self.generics(&def.generics, &format!("`{}`", def.name), Owner::Function);
            for g in &def.generics {
                let named = (def.params.iter())
                    .any(|p| type_names(&p.ty).iter().any(|(n, _)| *n == g.name))
                    || defs.named_by_out(def, &g.name);
                if !named {
                    self.errors.push(Error::unsupported(
                        g.pos,
                        "a type parameter that no parameter's type names",
                    ));
                }
            }
        }
        defs.requirements(&def.needs, &defs.signatures[f].scope, &mut self.errors);
        self.params(def, &defs.signatures[f].scope);
        self.defaults(def, &defs.signatures[f]);
        self.contracts(def, &defs.signatures[f]);
        self.effects(self.defs.file.listed(def.pos, &def.effects));
        self.parameter_effects(def);
    }

    /// Checks a default: it uses no name, carries no effect, and gives
    /// `needed`, among the type parameters of `scope`. `params` are the
    /// parameters of the function it is in, if it is in one.
    pub(super) fn default(
        &mut self,
        value: &Expr,
        needed: &Ty,
        scope: &'a Scope,
        params: Vec<&'a str>,
        extra: Vec<String>,
    ) {
        let mut body = Body {
            f: None,
            default: Some(params),
            scope,
            listed: Vec::new(),
            extra,
            unknown_effect: false,
            missing: Vec::new(),
            missing_at: Vec::new(),
            used: Vec::new(),
            names: Vec::new(),
            params: 0,
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
        };
        let gives = self.expr(&mut body, value, &Want::Ty(needed.clone()));
        self.expect(&mut body, value, &gives, needed);
    }

    /// Checks the defaults of the parameters of `f`, whose signature is
    /// `sig`: the parameters with one come after those without, and each
    /// default is as `default` requires.
    pub(super) fn defaults(&mut self, f: &'a Function, sig: &'a Signature) {
        let names: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
        let mut first = None;
        let positional = (f.params.iter())
            .position(|p| p.by_name)
            .unwrap_or(f.params.len());
        for (i, (p, (_, ty))) in f.params.iter().zip(&sig.params).enumerate() {
            // A final parameter before `;` that takes a function may come after
            // those with defaults, as a lambda after a call is for it.
            let last_fn = i + 1 == positional && p.ty.function;
            // The parameters after `;` are given by name, in any order.
            let ordered = i < positional;
            match (&p.default, first) {
                (None, Some(_)) if last_fn => {}
                // The default of a proof is `arith`, which the parser reads and
                // which is no value.
                (Some(value), _) if p.ty.statement.is_some() => {
                    self.proof_names.insert(addr(value));
                    first = first.or(Some(p.name.as_str()));
                }
                (Some(value), _) => {
                    first = first.or(Some(p.name.as_str()));
                    let extra = sig.fn_effects.get(i).cloned().unwrap_or_default();
                    self.default(value, ty, &sig.scope, names.clone(), extra);
                }
                (None, Some(q)) if ordered => self.errors.push(
                    Error::new(
                        p.pos,
                        format!(
                            "`{}` has no default, and comes after `{q}`, which has one",
                            p.name
                        ),
                    )
                    .with_fix(format!(
                        "move `{}` before `{q}`, or give it a default",
                        p.name
                    )),
                ),
                (None, _) => {}
            }
        }
    }

    /// Reports a body that ends with a statement that gives nothing, the
    /// one at `pos`, where `f` returns the type written `t`.
    pub(super) fn ends_with_nothing(&mut self, f: &Function, pos: Pos, t: &str) {
        self.errors.push(Error::new(
            pos,
            format!(
                "`{}` returns `{t}`, and its body ends with a binding or an assignment, \
                 which gives nothing",
                f.name
            ),
        ));
    }

    /// Checks a `return`: what it gives is the result of the function or the
    /// lambda around it, and a `return` with no value is where that gives
    /// nothing.
    pub(super) fn returning(&mut self, body: &mut Body<'a>, value: Option<&Expr>, pos: Pos) {
        let defs = self.defs;
        let needed = body.returns.clone();
        match value {
            None => {
                if needed != Ty::Nothing && !needed.unknown() {
                    let needed = defs.describe(&needed, body.scope);
                    self.errors.push(
                        Error::new(
                            pos,
                            format!("this `return` has no value, where {needed} is needed"),
                        )
                        .with_fix("write the value after `return`"),
                    );
                }
            }
            Some(value) => {
                let want = match &needed {
                    Ty::Nothing => Want::Unused,
                    t if t.unknown() => Want::Any,
                    t => Want::Ty(t.clone()),
                };
                let gives = self.expr(body, value, &want);
                match (&needed, &body.returned) {
                    (Ty::Nothing, _) => {
                        if !matches!(gives, Ty::Nothing | Ty::Unknown) {
                            let given = defs.describe(&gives, body.scope);
                            self.errors.push(Error::new(
                                value.pos(),
                                format!("this `return` gives {given}, where nothing is needed"),
                            ));
                        }
                    }
                    (t, Some(first)) if t.unknown() => {
                        let first = first.clone();
                        self.expect(body, value, &gives, &first);
                    }
                    (t, None) if t.unknown() => body.returned = Some(gives),
                    (t, _) => self.expect(body, value, &gives, t),
                }
                if body.lambdas == 0 && !body.tails.is_empty() {
                    self.ensure(body, value);
                }
            }
        }
    }

    /// Settles the types of lambda `l` among those `wanted` of it, the
    /// result last, which are unknown where what it is passed to leaves them
    /// open: a type it writes takes the place of one that is open, and one
    /// that is not the type wanted is an error at the type.
    pub(super) fn written_types(&mut self, body: &mut Body<'a>, l: &Lambda, wanted: &mut [Ty]) {
        let defs = self.defs;
        let written = l.types.iter().chain(std::iter::once(&l.result));
        for (slot, t) in wanted.iter_mut().zip(written) {
            let Some(t) = t else { continue };
            let ty = self.ty(t, body.scope);
            if ty == Ty::Unknown {
                continue;
            }
            if agrees(slot, &ty) {
                *slot = ty;
            } else {
                self.errors.push(Error::new(
                    t.pos,
                    format!(
                        "this is {}, where the function type needs {}",
                        defs.describe(&ty, body.scope),
                        defs.describe(slot, body.scope)
                    ),
                ));
            }
        }
    }

    /// Binds a name to a lambda that reads names of its function, which is
    /// called in the same body.
    pub(super) fn bind_env<'e>(
        &mut self,
        body: &mut Body<'a>,
        name: &str,
        pos: Pos,
        l: &'e Lambda,
    ) -> End<'e> {
        let held = self.held_lambda(body, l, "bound to a name", false);
        if body.names.iter().any(|(n, _)| n == name) {
            self.errors.push(self.defined_twice(body, name, pos));
        } else {
            body.names.push((name.to_string(), held));
        }
        End::Binding(pos)
    }

    /// Checks a lambda that reads names of its function, where it is bound to a
    /// name, stored in a field, or returned, which `place` says. Its body is
    /// checked where it is written, and it has the type of the struct that holds
    /// what it reads, at the types those names have. A lambda that is `kept`,
    /// stored or returned, carries no effect, as the type that holds it states
    /// none.
    pub(super) fn held_lambda(
        &mut self,
        body: &mut Body<'a>,
        l: &Lambda,
        place: &str,
        kept: bool,
    ) -> Ty {
        let defs = self.defs;
        let s = defs.env_of(l).expect("a struct for the lambda");
        let fields = &defs.file.structs[s].fields;
        let own = defs.file.structs[s].generics.len() - fields.len();
        let mut args: Vec<Ty> = (0..own).map(Ty::Param).collect();
        let mut lent = false;
        for field in fields {
            let ty = match body.names.iter().rfind(|(n, _)| *n == field.name) {
                Some((_, ty)) => ty.clone(),
                None => Ty::Unknown,
            };
            // A lambda that is kept takes the values it reads, which the ownership
            // check gives away. One that a `var` holds could change or move after a
            // lambda that is bound reads it, so it is refused as one that is lent.
            if !ty.unknown()
                && !ownership::copies(defs, body.scope, &ty)
                && !kept
                && body.vars.contains(&field.name)
            {
                lent = true;
            }
            args.push(ty);
        }
        if lent {
            self.errors.push(Error::new(
                l.pos,
                format!("a lambda cannot be {place}, as it is only passed to a call"),
            ));
        }
        if !args[own..].iter().any(Ty::unknown) {
            self.probed.insert(s, args[own..].to_vec());
        }
        let inferred = leaves_a_type_out(l);
        // A lambda that leaves types out has the types the calls of it gave, as
        // the look before found them.
        let wanted = match defs.env_lambda(s) {
            Some((_, items)) if inferred => {
                let mut items = items.clone();
                // What the body gives is the result, whatever the look before
                // found it was.
                if l.result.is_none() {
                    *items.last_mut().expect("a result") = Ty::Unknown;
                }
                items
            }
            _ => vec![Ty::Unknown; l.params.len() + 1],
        };
        let mut refused = false;
        if inferred && defs.env_unsettled.contains(&s) {
            refused = true;
            self.errors.push(Error::new(
                l.pos,
                "the parameters of this lambda are inferred from calls that pass what other calls of it give, so each look at them is wider than the one before, and they are written, as `fn(v: f32) -> f32 { ... }`",
            ));
        } else if inferred
            && !self.probing
            && (wanted[..l.params.len()].iter().any(Ty::unknown)
                || defs.env_calls.get(&s).is_none_or(Vec::is_empty))
        {
            refused = true;
            self.errors.push(Error::new(
                l.pos,
                "a lambda bound to a name writes the type of each parameter and of its result, as `fn(v: f32) -> f32 { ... }`, unless a call of it in the same body gives them",
            ));
        }
        self.from_calls = defs.env_calls.get(&s).cloned().filter(|_| inferred);
        let listed = kept.then(|| std::mem::take(&mut body.listed));
        body.kept += usize::from(kept);
        let checked = self.lambda(body, l, &wanted, false);
        body.kept -= usize::from(kept);
        if inferred {
            self.gave
                .insert(s, std::mem::replace(&mut self.lambda_gave, Ty::Unknown));
        }
        if let Some(listed) = listed {
            body.listed = listed;
        }
        match checked {
            _ if refused => Ty::Unknown,
            Ty::Fn(_) if !lent => Ty::Struct(s, args),
            // What the calls of it pass is not known yet, so it has no type.
            _ if inferred && !lent => Ty::Struct(s, args),
            _ => Ty::Unknown,
        }
    }

    /// Checks a lambda passed where a function is needed whose parameters
    /// and result, the result last, are of the types `wanted`, which may be
    /// partly known, and gives its type. Its body is checked where it is
    /// written, against the effects of the function it is written in, and
    /// it may assign that function's `var`s.
    pub(super) fn lambda(
        &mut self,
        body: &mut Body<'a>,
        l: &Lambda,
        wanted: &[Ty],
        quiet: bool,
    ) -> Ty {
        let defs = self.defs;
        let params = wanted.len() - 1;
        let mut wanted = wanted.to_vec();
        if l.params.len() == params {
            self.written_types(body, l, &mut wanted);
        }
        let wanted = &wanted[..];
        let (result, params) = wanted.split_last().expect("a result");
        if l.params.len() != params.len() {
            let needed = Ty::Fn(wanted.to_vec());
            self.errors.push(Error::new(
                l.pos,
                format!(
                    "this lambda takes {}, where {} is needed, which takes {}",
                    count(l.params.len(), "parameter"),
                    defs.describe(&needed, body.scope),
                    params.len()
                ),
            ));
            return Ty::Unknown;
        }
        let outer = body.names.len();
        let vars = body.vars.len();
        let known = body.facts.mark();
        let assigned = self.assigned_vars(body, &[&l.body]);
        self.forget_all(body);
        let loops = body.loops;
        body.outer_loops += std::mem::take(&mut body.loops);
        let mut types = Vec::new();
        let from_calls = self.from_calls.take();
        for (i, ((name, pos), ty)) in l.params.iter().zip(params).enumerate() {
            let twice = l.params[..i].iter().any(|(n, _)| n == name)
                || body.names.iter().any(|(n, _)| n == name);
            if twice {
                self.errors.push(self.defined_twice(body, name, *pos));
            }
            types.push(ty.clone());
            // The ownership check reads the type of a parameter from here.
            self.types.insert(addr(&l.params[i]), ty.clone());
            if let Some(calls) = &from_calls
                && l.types[i].is_none()
            {
                self.lambda_from.insert(body.names.len(), calls.clone());
            }
            body.names.push((name.clone(), ty.clone()));
        }
        // A block whose type is not settled by the call it is passed to is
        // checked once to find what it gives, and again with that settled, so
        // that a `try` in it knows what the block leaves with.
        let (names_in, vars_in) = (body.names.len(), body.vars.len());
        let mut settled = result.clone();
        // Only a block with a `try` in it needs the second look, and one that
        // is looked at to find what it gives is not looked at twice inside.
        let has_try = exprs(values(&l.body))
            .into_iter()
            .any(|e| matches!(e, Expr::Match(m) if matches!(m.form, Form::Try)));
        let mut again = !quiet && has_try && !self.discovering && settled.unknown();
        // A block that was looked at already, to find what it gives, is not
        // looked at again for that.
        if again && let Some(found) = self.found.get(&addr(l)) {
            settled = found.clone();
            again = false;
        }
        let (end, returned) = loop {
            let want = match &settled {
                Ty::Nothing => Want::Unused,
                Ty::Unknown if quiet => Want::Ty(Ty::Unknown),
                // An enum whose type arguments are partly known is wanted as
                // it is, so that `.ok(())` finds the error type the signature
                // gives.
                t @ Ty::Enum(..) if t.unknown() => Want::Ty(t.clone()),
                t if t.unknown() => Want::Any,
                t => Want::Ty(t.clone()),
            };
            // What the first look at the block leaves with has a hole where
            // the call has not said, which is filled so that a `try` in the
            // block still gives what it holds.
            let returns = if again || (self.discovering && has_try) {
                filled(&settled)
            } else {
                settled.clone()
            };
            let outer_returns = std::mem::replace(&mut body.returns, returns);
            let outer_returned = body.returned.take();
            body.lambdas += 1;
            let before = self.errors.len();
            let outer_discovering = self.discovering;
            self.discovering = outer_discovering || again;
            let end = self.block(body, &l.body, &want);
            // A `try` in a block that the call does not give a type is refused
            // for what the block gives, which is the type it leaves with.
            if result.unknown() && !settled.unknown() {
                let gives = defs.text(&settled, body.scope);
                for error in &mut self.errors[before..] {
                    if error
                        .message
                        .starts_with("there is no implementation of `Try` for")
                    {
                        error.message = format!(
                            "this block gives `{gives}`, which `try` cannot return a failure from"
                        );
                    }
                }
            }
            self.discovering = outer_discovering;
            body.lambdas -= 1;
            body.returns = outer_returns;
            let returned = std::mem::replace(&mut body.returned, outer_returned);
            if again
                && let End::Value(gives, _) = &end
                && !gives.unknown()
            {
                again = false;
                self.errors.truncate(before);
                body.names.truncate(names_in);
                body.vars.truncate(vars_in);
                body.facts.reset(&known);
                settled = gives.clone();
                continue;
            }
            if self.discovering
                && has_try
                && settled.unknown()
                && let End::Value(gives, _) = &end
                && !gives.unknown()
            {
                self.found.insert(addr(l), gives.clone());
            }
            break (end, returned);
        };
        let result = match returned {
            Some(t) if settled.unknown() => &t.clone(),
            _ => &settled,
        };
        let gives = match (end, result) {
            _ if proof::ends_in_exit(&l.body) => result.clone(),
            (End::Value(gives, value), Ty::Nothing) => {
                if !matches!(gives, Ty::Nothing | Ty::Unknown) {
                    self.expect(body, value, &gives, result);
                }
                Ty::Nothing
            }
            (_, Ty::Nothing) => Ty::Nothing,
            (End::Value(gives, value), result) if !result.unknown() => {
                self.expect(body, value, &gives, result);
                result.clone()
            }
            (End::Value(gives, _), _) => gives,
            (End::Empty, result) if !result.unknown() => {
                let needed = defs.describe(result, body.scope);
                self.errors.push(Error::new(
                    l.end,
                    format!(
                        "this lambda's body is empty, which gives nothing, where {needed} is needed"
                    ),
                ));
                Ty::Unknown
            }
            (End::Binding(pos), result) if !result.unknown() => {
                let needed = defs.describe(result, body.scope);
                self.errors.push(Error::new(
                    pos,
                    format!(
                        "this lambda ends with a binding or an assignment, which gives nothing, \
                         where {needed} is needed"
                    ),
                ));
                Ty::Unknown
            }
            (End::Empty | End::Binding(_), _) => Ty::Nothing,
        };
        body.names.truncate(outer);
        self.lambda_from.retain(|at, _| *at < outer);
        body.vars.truncate(vars);
        body.facts.reset(&known);
        self.forget(body, &assigned);
        body.outer_loops -= loops;
        body.loops = loops;
        self.lambda_gave = gives.clone();
        types.push(gives);
        Ty::func(types)
    }

    /// Checks the name of function `f`, at `pos`, passed as a value, and
    /// gives its type. A function type states no effect, so the effects of
    /// `f` are required of the function the name is written in, as those
    /// of a lambda that calls it are.
    pub(super) fn function_value(
        &mut self,
        body: &mut Body<'a>,
        expr: &Expr,
        f: usize,
        pos: Pos,
    ) -> Ty {
        let defs = self.defs;
        let sig = &defs.signatures[f];
        let name = &defs.file.functions[f].name;
        let error = if !sig.scope.names.is_empty() && !name.starts_with("lambda@") {
            Error::unsupported(pos, "passing a function with type parameters as a value")
        } else if sig.params.iter().any(|(_, t)| t.holds_fn()) {
            Error::unsupported(pos, "passing a function that takes a function as a value")
        } else if !defs.file.functions[f].requires.is_empty() {
            Error::new(
                pos,
                format!(
                    "`{name}` has a `requires`, which a function type does not state, so it is \
                     only called by its name"
                ),
            )
        } else {
            for effect in &sig.effects {
                self.needs(body, effect, pos, &format!("`{name}`"));
            }
            self.fn_values.insert(addr(expr), f);
            let mut items: Vec<Ty> = sig.params.iter().map(|(_, t)| t.clone()).collect();
            items.push(sig.gives.clone());
            if items.contains(&Ty::Unknown) {
                // A type the file does not name, as the `Stream` of `print`,
                // would pass for any type, and lower to no value.
                self.errors.push(Error::unsupported(
                    pos,
                    "passing a function with a type this file does not name as a value",
                ));
                return Ty::Unknown;
            }
            return Ty::func(items);
        };
        self.errors.push(error);
        Ty::Unknown
    }

    /// Checks a call of a name that holds a function whose parameters and
    /// result, the result last, are of the types `items`. Its arguments are
    /// given by position, and it carries no effect.
    pub(super) fn value_call(&mut self, body: &mut Body<'a>, call: &Call, items: &[Ty]) -> Ty {
        self.value_calls.insert(addr(call));
        let (gives, params) = items.split_last().expect("a result");
        let name = &call.callee;
        let mut given = vec![false; params.len()];
        let mut passed = vec![Ty::Unknown; params.len()];
        let free = std::mem::take(&mut self.free_args);
        let mut next = 0;
        for arg in &call.args {
            let at = if arg.after {
                params.len().checked_sub(1)
            } else {
                next += 1;
                Some(next - 1).filter(|i| *i < params.len())
            };
            let error = if let Some((_, pos)) = &arg.name {
                Some(Error::new(
                    *pos,
                    format!(
                        "the arguments of `{name}` are given by position, as it is a function a name holds"
                    ),
                ))
            } else {
                match at {
                    Some(i) if given[i] => Some(Error::new(
                        arg.value.pos(),
                        format!("the argument for the last parameter of `{name}` is given twice"),
                    )),
                    Some(i) if arg.after && !matches!(params[i], Ty::Fn(_) | Ty::Unknown) => {
                        Some(Error::new(
                            arg.value.pos(),
                            format!(
                                "a lambda after a call is for its last parameter, and that of \
                                 `{name}` takes no function"
                            ),
                        ))
                    }
                    Some(_) => None,
                    None => Some(Error::new(
                        arg.value.pos(),
                        format!(
                            "`{name}` takes {}, and this is one more",
                            count(params.len(), "argument")
                        ),
                    )),
                }
            };
            if let Some(error) = error {
                self.errors.push(error);
                self.expr(body, &arg.value, &Want::Ty(Ty::Unknown));
                continue;
            }
            let i = at.expect("a parameter");
            given[i] = true;
            // What a call passes where nothing says the type is the smallest range
            // that holds it, so that a number written out is its own range.
            let free = free && params[i].unknown();
            let want = if free {
                Want::Any
            } else {
                Want::Ty(params[i].clone())
            };
            let mut value = self.expr(body, &arg.value, &want);
            if free
                && let Ty::Int(_) = value
                && !matches!(arg.value, Expr::Name { .. })
                && let Some((low, high)) = self.known_range(body, &arg.value)
                && let Some(exact) = Int::between(low, high)
            {
                value = Ty::Int(exact);
            }
            self.expect(body, &arg.value, &value, &params[i]);
            passed[i] = value;
        }
        self.last_args = passed;
        let missing = given.iter().filter(|g| !**g).count();
        if missing > 0 {
            self.errors.push(Error::new(
                call.end,
                format!(
                    "`{name}` takes {}, and this call gives {}",
                    count(params.len(), "argument"),
                    params.len() - missing
                ),
            ));
        }
        gives.clone()
    }

    /// Reports a type given for a type parameter that holds a function,
    /// and gives whether there is one.
    pub(super) fn function_for_parameter(&mut self, sol: &[Option<Ty>], pos: Pos) -> bool {
        if sol.iter().flatten().any(Ty::holds_fn) {
            self.errors.push(Error::unsupported(
                pos,
                "a function given for a type parameter",
            ));
            return true;
        }
        false
    }

    /// Checks the arguments of a call that names nothing there is, for the
    /// mistakes in them.
    pub(super) fn unknown_call(&mut self, body: &mut Body<'a>, call: &Call) -> Ty {
        for arg in &call.args {
            self.expr(body, &arg.value, &Want::Ty(Ty::Unknown));
        }
        Ty::Unknown
    }

    /// The type of a call of a function that gives its parameter `i`, which is a
    /// function: the type whose one value is the function the argument is.
    fn argument_function(&mut self, body: &Body<'a>, call: &Call, sig: &Signature, i: usize) -> Ty {
        match self.argument_named(body, call, sig, i) {
            (Some(named), _) => named,
            (None, value) => {
                self.errors.push(Error::new(value.pos(), WHICH_FUNCTION));
                Ty::Unknown
            }
        }
    }

    /// The type whose one value is the function that the argument for parameter
    /// `i` is, when it is one, and the argument.
    fn argument_named<'c>(
        &self,
        body: &Body<'a>,
        call: &'c Call,
        sig: &Signature,
        i: usize,
    ) -> (Option<Ty>, &'c Expr) {
        let places = params_of(&sig.params, sig.by_position(), &call.args).unwrap_or_default();
        let a = places.iter().position(|&p| p == i).unwrap_or(0);
        let value = &call.args[a].value;
        let known = match value {
            _ if self.fn_values.contains_key(&addr(value)) => {
                Some(Ty::Named(self.fn_values[&addr(value)]))
            }
            Expr::Name { name, .. } => match body.names.iter().rfind(|(n, _)| n == name) {
                Some((_, t @ Ty::Named(_))) => Some(t.clone()),
                _ => None,
            },
            _ => match self.types.get(&addr(value)) {
                Some(t @ Ty::Named(_)) => Some(t.clone()),
                _ => None,
            },
        };
        (known, value)
    }

    /// What the field a call is written on holds, when the call is
    /// `name.field(...)` and the type of `name` is a struct whose field holds a
    /// function: the function, once it is known, and otherwise the type of the
    /// function the field is written with. Gives with it the type of `name`.
    fn function_in_field(&self, body: &Body<'a>, call: &Call) -> Option<(Held, Ty)> {
        let (q, _) = call.qualifier.as_ref()?;
        let (_, receiver) = body.names.iter().rfind(|(n, _)| n == q)?;
        Some((self.held_in(receiver, &call.callee)?, receiver.clone()))
    }

    /// What the field `name` of a value of type `receiver` holds, when it
    /// holds a function.
    fn held_in(&self, receiver: &Ty, name: &str) -> Option<Held> {
        let Ty::Struct(s, args) = receiver else {
            return None;
        };
        let defs = self.defs;
        let i = defs.file.structs[*s]
            .fields
            .iter()
            .position(|f| f.name == name)?;
        Some(match defs.fields[*s][i].subst(args) {
            Ty::Named(g) => Held::Named(g),
            // A lambda that reads names is of the type of its own parameters.
            Ty::Struct(e, held) if defs.env_lambda(e).is_some() => {
                Held::Items(defs.env_items(e, &held).expect("checked"))
            }
            _ => Held::Items(
                defs.field_items[*s][i]
                    .as_ref()?
                    .iter()
                    .map(|t| t.subst(args))
                    .collect(),
            ),
        })
    }

    pub(super) fn call(&mut self, body: &mut Body<'a>, call: &Call, want: &Want) -> Ty {
        let defs = self.defs;
        if self.passed_mut(body, call) {
            return self.unknown_call(body, call);
        }
        let name = call.callee.as_str();
        // A field that holds a function with a name is called as that function.
        // The call that gives the function, as in `chosen()(x)`.
        let through = match call.through.as_deref() {
            Some(Expr::Field { .. }) | None => None,
            Some(inner) => match self.expr(body, inner, &Want::Any) {
                Ty::Named(g) => Some(g),
                Ty::Unknown => return self.unknown_call(body, call),
                Ty::Struct(e, held) if defs.env_lambda(e).is_some() => {
                    let items = defs.env_items(e, &held).expect("checked");
                    let given = self.value_call(body, call, &items);
                    self.value_calls.remove(&addr(call));
                    return given;
                }
                other => {
                    let given = defs.describe(&other, body.scope);
                    self.errors.push(Error::new(
                        inner.pos(),
                        format!("this gives {given}, which is not a function that can be called"),
                    ));
                    return self.unknown_call(body, call);
                }
            },
        };
        // The field of a name or of what an expression gives, as in
        // `sale.discount(x)` and `shop.pricing.discount(x)`.
        let field = match call.through.as_deref() {
            Some(Expr::Field { of, .. }) => match self.expr(body, of, &Want::Any) {
                Ty::Unknown => return self.unknown_call(body, call),
                receiver => {
                    let held = self.held_in(&receiver, &call.callee);
                    if held.is_none() {
                        self.errors.push(Error::unsupported(
                            call.name_pos,
                            "calling what a field gives",
                        ));
                        return self.unknown_call(body, call);
                    }
                    held.map(|h| (h, receiver))
                }
            },
            _ if through.is_some() => None,
            _ => self.function_in_field(body, call),
        };
        if let Some((held, receiver)) = &field {
            self.field_receivers.insert(addr(call), receiver.clone());
            if let Held::Items(items) = held {
                let given = self.value_call(body, call, items);
                self.value_calls.remove(&addr(call));
                return given;
            }
        }
        let in_field = through.or(match field {
            Some((Held::Named(g), _)) => Some(g),
            _ => None,
        });
        if let (Some((q, at)), None) = (&call.qualifier, in_field) {
            if let Some(e) = defs.enum_at(*at, q)
                && !body.names.iter().any(|(n, _)| n == q)
            {
                return self.qualified_case(body, call, e, want);
            }
            let Some(k) = defs.trait_at(*at, q) else {
                self.errors.push(if body.names.iter().any(|(n, _)| n == q) {
                    Error::unsupported(call.name_pos, "calling what a field gives")
                } else {
                    Error::new(*at, format!("there is no trait named `{q}`"))
                        .with_fix("to write a name after a module, import the module with `use`")
                });
                return self.unknown_call(body, call);
            };
            let t = &defs.file.traits[k];
            let Some(fi) = traits::trait_function(t, name) else {
                self.errors.push(Error::new(
                    call.name_pos,
                    format!("`{q}` has no function named `{name}`"),
                ));
                return self.unknown_call(body, call);
            };
            return self.trait_call(body, call, k, fi, want);
        }
        if let Some((_, Ty::Struct(s, args))) = body.names.iter().rfind(|(n, _)| n == name)
            && call.qualifier.is_none()
            && let Some(items) = defs.env_items(*s, args)
        {
            let s = *s;
            let untyped = defs
                .env_lambda(s)
                .is_some_and(|(l, _)| leaves_a_type_out(l));
            self.free_args = untyped;
            let given = self.value_call(body, call, &items);
            if untyped {
                let passed = std::mem::take(&mut self.last_args);
                self.seen
                    .entry(s)
                    .or_default()
                    .push((call.pos, written_call(call), passed));
            }
            return given;
        }
        if let Some((_, Ty::Fn(items))) = body.names.iter().rfind(|(n, _)| n == name) {
            let items = items.clone();
            return self.value_call(body, call, &items);
        }
        if let Some(s) = defs.struct_at(call.name_pos, name) {
            return self.construction(body, call, s, want);
        }
        if is_ending_call(call, "__eprint") {
            let mut print = Signature::print();
            // A server of the library that cannot listen says why to standard
            // error and ends the program, as a console that refuses a write
            // does, so the function that does it carries no `console`.
            if of_std(call.pos) {
                print.effects.clear();
            }
            self.arguments(body, call, &print, true, None, None);
            return Ty::Nothing;
        }
        if is_ending_call(call, "__end") {
            let end = Signature::end();
            self.arguments(body, call, &end, true, None, None);
            return Ty::Nothing;
        }
        if is_kernel(call) {
            return self.kernel(body, call);
        }
        if let Some(kind) = super::frames::kind(call) {
            return self.frames(body, call, kind, want);
        }
        if is_intrinsic(call) {
            return self.intrinsic(body, call);
        }
        if is_push(defs, call) {
            return self.push(body, call, want);
        }
        if is_freeze(defs, call) {
            return self.freeze(body, call, want);
        }
        if is_utf8(defs, call) {
            return self.utf8(body, call);
        }
        if let Some(narrowing) = narrowing(defs, call) {
            return self.narrow(body, call, narrowing, want);
        }
        if is_float_call(defs, call) {
            return self.float_of(body, call, want);
        }
        if let (Some(effect), false) = (
            defs.effect_named(name),
            defs.function(call.name_pos, name).is_some(),
        ) {
            return self.handler_call(body, call, effect);
        }
        if is_arena_call(defs, call) {
            return self.arena_call(body, call);
        }
        // A name that holds the function `g` by the type `g` is called as `g` is.
        let name = match through {
            Some(g) => written_name(&defs.file.functions[g].name),
            None => name,
        };
        let named = in_field.or_else(|| {
            body.names
                .iter()
                .rfind(|(n, _)| n == name)
                .and_then(|(_, t)| match t {
                    Ty::Named(g) => Some(*g),
                    _ => None,
                })
        });
        if let Some(f) = named.or_else(|| defs.function(call.name_pos, name)) {
            let sig = &defs.signatures[f];
            // `quiet` waits for tasks that no handle names, which the language
            // does not give a program: it is for the servers of the library.
            if matches!(name, "quiet" | "timed_out" | "limit")
                && defs.is_operation(f)
                && of_std(defs.file.functions[f].pos)
                && !of_std(call.pos)
            {
                self.errors.push(
                    Error::new(
                        call.name_pos,
                        format!("`{name}` is for the library, since a program waits for a task by its handle"),
                    )
                    .with_fix("`join` the handle of each task that is waited for"),
                );
                self.refusals += 1;
                return self.unknown_call(body, call);
            }
            if named.is_some() {
                self.named_calls.insert(addr(call), f);
                if call.args.len() < sig.params.len() {
                    self.errors.push(Error::unsupported(
                        call.pos,
                        "leaving out an argument of a function called by its type",
                    ));
                    return self.unknown_call(body, call);
                }
            }
            if self.effect_in_default(body, call, sig) {
                return self.unknown_call(body, call);
            }
            let refused = self.refusals;
            let refused_errors = self.errors.len();
            // A call that changes a value with `mut` gives back what it is
            // passed, so what it gives is settled by its arguments alone, and
            // a mistake in them is the one mistake of the line.
            let gives_back = call.args.iter().any(|a| a.given_back);
            let placed = match want {
                Want::Ty(t) if !gives_back => Some(t),
                _ => None,
            };
            let is_print = defs.provided(f).is_some_and(|p| p.through.is_none());
            let mut sol = self.arguments(body, call, sig, is_print, None, placed);
            if gives_back && self.errors.len() > refused_errors {
                return Ty::Unknown;
            }
            if name == "spawn" && of_std(defs.file.functions[f].pos) {
                self.child_assigns(body, call);
            }
            defs.complete_outs(&sig.scope.requires, &mut sol, body.scope);
            for (i, d) in sig.param_defaults.iter().enumerate() {
                if let (None, Some(d)) = (&sol[i], d) {
                    let ty = d.solved(&sol);
                    sol[i] = (!ty.unknown()).then_some(ty);
                }
            }
            if self.refusals != refused || self.function_for_parameter(&sol, call.pos) {
                return Ty::Unknown;
            }
            for r in &sig.scope.requires {
                let types: Vec<Ty> = r.types.iter().map(|t| t.solved(&sol)).collect();
                if types.contains(&Ty::Unknown) {
                    continue;
                }
                match defs.meets_types(r.of, &types, body.scope, &mut Vec::new()) {
                    Found::No => {
                        let because = defs.blame(r.of, &types, body.scope);
                        self.errors.push(Error::new(
                            call.pos,
                            format!(
                                "there is no implementation of `{}` for {}, which `{name}` \
                                 needs{because}",
                                defs.file.traits[r.of].name,
                                defs.types_of(r.of, &types, body.scope)
                            ),
                        ));
                    }
                    Found::Two(a, b) => self.errors.push(defs.ambiguous(call.pos, a, b)),
                    Found::Yes(..) | Found::Unknown => {}
                }
            }
            self.contract_call(body, call, &defs.file.functions[f], sig);
            self.call_decreases(body, call, f, sig);
            // A division that wraps is of a divisor that is not zero.
            if matches!(name, "wrapping_div" | "wrapping_rem")
                && of_std(defs.file.functions[f].pos)
                && self.errors.len() == refused_errors
            {
                self.wrapping_division(body, call, sig);
            }
            if let Some(i) = sig.gives_param {
                return self.argument_function(body, call, sig, i);
            }
            let gives = sig.gives.solved(&sol);
            if let (false, Ty::Struct(u, mut held)) = (sig.gives_held.is_empty(), gives.clone()) {
                // The functions of parameters are those of the arguments.
                let first = held.len() - defs.implicit[u];
                for &(at, i) in &sig.gives_held {
                    let (named, value) = self.argument_named(body, call, sig, i);
                    match named {
                        Some(Ty::Named(g)) => {
                            self.stored_effects(value, g, "the type of the result it is stored in");
                            held[first + at] = Ty::Named(g);
                        }
                        _ => {
                            self.errors.push(Error::new(value.pos(), WHICH_FUNCTION));
                            return Ty::Unknown;
                        }
                    }
                }
                return Ty::structure(u, held);
            }
            return gives;
        }
        let mut of = (defs.trait_fns_at(call.name_pos, name))
            .cloned()
            .unwrap_or_default();
        if let [(k, _)] = of[..] {
            of.extend(defs.rivals(k, name, call.pos));
        }
        match of.as_slice() {
            [(k, fi)] => self.trait_call(body, call, *k, *fi, want),
            of if !of.is_empty() => {
                let traits: Vec<&str> = (of.iter())
                    .map(|(k, _)| written_name(&defs.file.traits[*k].name))
                    .collect();
                let fixes: Vec<String> = traits.iter().map(|t| format!("`{t}.{name}`")).collect();
                self.errors.push(
                    Error::new(
                        call.pos,
                        format!("`{name}` is a function of {}", names(&traits)),
                    )
                    .with_fix(format!("write {}", fixes.join(" or "))),
                );
                self.unknown_call(body, call)
            }
            _ => {
                if let Some(given) = self.left_out_numbers(body, call, want) {
                    return given;
                }
                // What is not known was reported where it was bound.
                if !matches!(
                    body.names.iter().rfind(|(n, _)| n == name),
                    Some((_, Ty::Unknown))
                ) {
                    self.errors.push(Error::new(
                        call.pos,
                        format!("there is no function named `{name}`"),
                    ));
                }
                self.unknown_call(body, call)
            }
        }
    }

    /// Before a lambda is checked against the function a trait's function
    /// takes, gives the types that are not yet known but which the
    /// implementation the known ones choose fixes: its `out` types, and the
    /// type the call gives when the call's value is what is wanted. Gives
    /// whether an implementation was not found, which is reported.
    pub(super) fn completed(
        &mut self,
        body: &Body,
        call: &Call,
        (k, want): (usize, &Want),
        sig: &Signature,
        sol: &mut [Option<Ty>],
    ) -> bool {
        let defs = self.defs;
        let outs = &defs.outs[k];
        // The type the call gives is that of what the lambda gives, so it
        // may be known only from what is wanted.
        let gives = match &sig.gives {
            Ty::Param(r) if !outs[*r] => Some(*r),
            _ => None,
        };
        if let (Some(r), Want::Ty(wanted)) = (gives, want)
            && sol[r].is_none()
            && !wanted.unknown()
        {
            sol[r] = Some(wanted.clone());
        }
        if (sol.iter().zip(outs).enumerate())
            .any(|(i, (t, out))| !out && t.is_none() && Some(i) != gives)
        {
            return false;
        }
        let types: Vec<Ty> = sol
            .iter()
            .map(|t| t.as_ref().map_or(Ty::Unknown, Ty::concrete))
            .collect();
        match defs.fitting(k, &types) {
            Some(all) => {
                for (t, a) in sol.iter_mut().zip(all) {
                    if a != Ty::Unknown {
                        t.get_or_insert(a);
                    }
                }
                false
            }
            None => {
                self.errors
                    .push(self.no_implementation(body, call, k, &types));
                true
            }
        }
    }

    /// Checks the effects and the arguments of a call of a function with
    /// this signature, and gives the type each of its type parameters
    /// stands for, where the arguments say.
    pub(super) fn arguments(
        &mut self,
        body: &mut Body<'a>,
        call: &Call,
        signature: &Signature,
        is_print: bool,
        via: Option<(usize, &Want)>,
        placed: Option<&Ty>,
    ) -> Vec<Option<Ty>> {
        let outer = self.using.replace(call.callee.clone());
        let given = self.arguments_of(body, call, signature, is_print, via, placed);
        self.using = outer;
        given
    }

    fn arguments_of(
        &mut self,
        body: &mut Body<'a>,
        call: &Call,
        signature: &Signature,
        is_print: bool,
        via: Option<(usize, &Want)>,
        placed: Option<&Ty>,
    ) -> Vec<Option<Ty>> {
        let vars = &signature.scope;
        // What the place of the call's result says the type parameters are,
        // which only a literal that nothing else decides takes.
        let mut from_place: Vec<Option<Ty>> = vec![None; vars.names.len()];
        if let Some(t) = placed {
            seed(&signature.gives, t, &mut from_place);
        }
        let mut sol: Vec<Option<Ty>> = match self.seeded.take() {
            Some(seeded) if seeded.len() == vars.names.len() => seeded,
            _ => vec![None; vars.names.len()],
        };
        let comparing = call.args.len() == 2
            && matches!(&call.qualifier, Some((q, _)) if q == "Ord" || q == "Eq")
            && matches!(call.callee.as_str(), "compare" | "eq");
        for effect in &signature.effects {
            self.needs(body, effect, call.pos, &format!("`{}`", call.callee));
        }
        let mut given: Vec<bool> = vec![false; signature.params.len()];
        // The whole number literals whose type is not known when they are
        // reached, which are checked after the other arguments, with the
        // place of the parameter each is for.
        let mut deferred: Vec<(usize, &Expr)> = Vec::new();
        let errors_before = self.errors.len();
        let mut lambdas: Vec<(usize, &Lambda)> = Vec::new();
        // An argument that needs the type the others say is checked after them,
        // so a call that waits would be checked after it.
        let needs_others = (call.args.iter()).any(|a| {
            matches!(
                a.value,
                Expr::Lambda(_) | Expr::Case { .. } | Expr::List { .. }
            )
        });
        // How many parameters take an argument by position, and how many of
        // them a block after the call leaves to arguments written in the
        // parentheses.
        let by_position = signature.named_from.unwrap_or(signature.params.len());
        let block = by_position > 0
            && call.args.iter().any(|a| a.after)
            && matches!(signature.params[by_position - 1].1, Ty::Fn(_) | Ty::Unknown);
        let positional = by_position - usize::from(block);
        let mut named = false;
        // Whether an argument named a parameter there is not. The mistake is
        // one, so a parameter written as a pattern, which no name can reach,
        // is not also reported missing.
        let mut unknown = false;
        let mut too_many = false;
        let mut next = 0;
        for arg in &call.args {
            // An argument `print` does not take yet is not looked into.
            let mut look_into = true;
            // The parameter the argument is for, if it is for one.
            let mut param = None;
            let callee = &call.callee;
            if arg.after {
                // A lambda after a call is for its last parameter before `;`.
                let last = by_position.checked_sub(1);
                let error = match last {
                    None if signature.params.is_empty() => Some(format!(
                        "`{callee}` takes no arguments, so no lambda goes after the call"
                    )),
                    None => Some(format!(
                        "no lambda goes after a call of `{callee}`, as every parameter of it is given by name"
                    )),
                    Some(i) if !matches!(signature.params[i].1, Ty::Fn(_) | Ty::Unknown) => {
                        // The parameter is not then reported missing.
                        given[i] = true;
                        let last = if by_position < signature.params.len() {
                            "last parameter before the `;`"
                        } else {
                            "last parameter"
                        };
                        Some(format!(
                            "a lambda after a call is for its last parameter, and {}, the {last} \
                             of `{callee}`, takes no function",
                            param_words(&signature.params[i].0, i)
                        ))
                    }
                    Some(i) if given[i] => Some(format!(
                        "the {} argument is given twice",
                        param_words(&signature.params[i].0, i)
                    )),
                    Some(i) => {
                        given[i] = true;
                        param = Some(i);
                        None
                    }
                };
                if let Some(message) = error {
                    self.errors.push(Error::new(arg.value.pos(), message));
                    self.refusals += 1;
                    continue;
                }
            } else {
                match &arg.name {
                    None if named => self.errors.push(
                        Error::new(
                            arg.value.pos(),
                            "an argument by position comes after a named one",
                        )
                        .with_fix("name it, or move it before the named arguments"),
                    ),
                    None if next < positional => {
                        given[next] = true;
                        param = Some(next);
                        next += 1;
                    }
                    None if signature
                        .named_from
                        .is_some_and(|k| k + (next - positional) < given.len()) =>
                    {
                        // The argument fills a parameter that is given by name.
                        let i = signature.named_from.expect("checked") + (next - positional);
                        next += 1;
                        look_into = false;
                        given[i] = true;
                        let name = &signature.params[i].0;
                        self.errors.push(
                            Error::new(
                                arg.value.pos(),
                                format!("`{name}` of `{callee}` is given by name"),
                            )
                            .with_fix(format!(
                                "write `{}`",
                                self.call_by_name(call, signature, positional)
                            )),
                        );
                    }
                    None if is_print && signature.params.len() == 1 => {
                        look_into = false;
                        if !too_many {
                            self.errors.push(Error::unsupported(
                                arg.value.pos(),
                                "the `end` and `to` arguments of `print`",
                            ));
                        }
                        too_many = true;
                    }
                    None => {
                        if !too_many {
                            self.errors.push(Error::new(
                                arg.value.pos(),
                                format!(
                                    "`{}` takes {} arguments, and this is one more",
                                    call.callee,
                                    given.len()
                                ),
                            ));
                        }
                        too_many = true;
                    }
                    Some((name, pos)) => {
                        named = true;
                        match signature.params.iter().position(|(p, _)| p == name) {
                            Some(i) if signature.named_from.is_some_and(|k| i < k) => {
                                given[i] = true;
                                look_into = false;
                                self.errors.push(
                                    Error::new(
                                        *pos,
                                        format!("`{name}` of `{callee}` is given by position"),
                                    )
                                    .with_fix(format!(
                                        "write `{}`",
                                        self.call_with_name_taken_off(call, name)
                                    )),
                                );
                            }
                            Some(i) if given[i] => self.errors.push(Error::new(
                                *pos,
                                format!("the `{name}` argument is given twice"),
                            )),
                            Some(i) => {
                                given[i] = true;
                                param = Some(i);
                            }
                            None if is_print
                                && signature.params.len() == 1
                                && (name == "end" || name == "to") =>
                            {
                                look_into = false;
                                self.errors.push(Error::unsupported(
                                    *pos,
                                    "the `end` and `to` arguments of `print`",
                                ));
                            }
                            None => {
                                unknown = true;
                                self.errors.push(Error::new(
                                    *pos,
                                    format!("`{}` has no parameter named `{name}`", call.callee),
                                ))
                            }
                        }
                    }
                }
            }
            if !look_into {
                continue;
            }
            if let (Some(i), Expr::Lambda(l)) = (param, &arg.value) {
                let pattern = &signature.params[i].1;
                if !matches!(pattern, Ty::Fn(_) | Ty::Unknown) {
                    let needed = self.defs.describe_pattern(pattern, &sol, vars, body.scope);
                    // A lambda has no name, so it has no `Function`.
                    let named = matches!(pattern, Ty::Param(p) if vars.requires.iter().any(
                        |r| r.of == self.defs.function && r.types.contains(&Ty::Param(*p))));
                    let message = if named {
                        "this is a lambda, and `Function` is given to a function with a name"
                            .to_string()
                    } else {
                        format!("this is a lambda, where {needed} is needed")
                    };
                    self.errors.push(Error::new(l.pos, message));
                    continue;
                }
            }
            if let (Some(via), Some(_), Expr::Lambda(_)) = (via, param, &arg.value)
                && !self.no_impl
            {
                self.no_impl = self.completed(body, call, via, signature, &mut sol);
            }
            // A lambda's parameters take the default of a type that nothing
            // before it settled.
            if let (Some(i), Expr::Lambda(_)) = (param, &arg.value) {
                for (p, d) in signature.param_defaults.iter().enumerate() {
                    if let (None, Some(d), true) = (&sol[p], d, signature.params[i].1.mentions(p)) {
                        let ty = d.solved(&sol);
                        sol[p] = (!ty.unknown()).then_some(ty);
                    }
                }
            }
            let quiet = self.no_impl || matches!(via, Some((_, Want::Ty(Ty::Unknown))));
            let wanted = match param {
                Some(_) if quiet => Want::Ty(Ty::Unknown),
                Some(i) => match signature.params[i].1.partial(&sol) {
                    // A type that is not known yet is not one a mistake
                    // already explains.
                    Ty::Unknown if signature.params[i].1 != Ty::Unknown => Want::Any,
                    t => Want::Ty(t),
                },
                None => Want::Ty(Ty::Unknown),
            };
            // A block whose type nothing else settles is of the type the place
            // of the call needs, as the value of a function is.
            let wanted = match (param, &arg.value, &wanted) {
                (Some(i), Expr::Lambda(_), Want::Ty(Ty::Fn(items)))
                    if items.last().is_some_and(Ty::unknown) =>
                {
                    match signature.params[i].1.partial(&from_place) {
                        Ty::Fn(placed)
                            if placed.last().is_some_and(|t| !t.unknown())
                                && placed.len() == items.len() =>
                        {
                            let mut items = items.clone();
                            *items.last_mut().expect("a result") =
                                placed.last().cloned().expect("a result");
                            Want::Ty(Ty::Fn(items))
                        }
                        _ => wanted,
                    }
                }
                _ => wanted,
            };
            if let (Some(i), Want::Any) = (param, &wanted)
                && (matches!(arg.value, Expr::Number { .. })
                    || float_written(&arg.value)
                    || matches!(&arg.value, Expr::Match(m) if literal_blocks(m)))
            {
                deferred.push((i, &arg.value));
                continue;
            }
            // A call whose result a literal in it may decide, for a parameter
            // nothing says the type of yet, waits for the other arguments,
            // unless one of them needs what the earlier ones say.
            if let (Some(i), Expr::Call(_), Want::Any) = (param, &arg.value, &wanted)
                && !needs_others
            {
                deferred.push((i, &arg.value));
                continue;
            }
            // A list written out whose element type is not settled yet.
            if let (Some(i), Expr::List { .. }, Want::Ty(t)) = (param, &arg.value, &wanted)
                && *t != Ty::Unknown
                && t.unknown()
            {
                deferred.push((i, &arg.value));
                continue;
            }
            if let (Some(i), Expr::Lambda(l), Want::Ty(Ty::Fn(_))) = (param, &arg.value, &wanted) {
                lambdas.push((i, l));
            }
            let wanted = own_range(&arg.value, wanted, comparing);
            let mark = body.extra.len();
            body.extra
                .extend(self.lambda_effects(signature, param, &arg.value));
            let defs = self.defs;
            let written = param.and_then(|i| signature.written.get(i).cloned().flatten());
            let here = param.map_or(Ty::Unknown, |i| signature.params[i].1.clone());
            let gives = defs.writing(&here, written.clone(), || {
                self.expr(body, &arg.value, &wanted)
            });
            body.extra.truncate(mark);
            match param {
                Some(i) => {
                    let pattern = &signature.params[i].1;
                    self.comparing = comparing;
                    defs.writing(&here, written, || {
                        self.fit(body, &arg.value, &gives, pattern, &mut sol, vars)
                    });
                    self.comparing = false;
                }
                None if gives == Ty::Nothing => {
                    self.errors.push(self.nothing_where(&arg.value, "text"));
                }
                None => {}
            }
        }
        // A literal takes the type the other arguments settle. When they do
        // not, only the first of them is reported.
        let mut reported = false;
        let (calls, numbers): (Vec<_>, Vec<_>) =
            (deferred.into_iter()).partition(|(_, value)| matches!(value, Expr::Call(_)));
        for (i, value) in calls.into_iter().chain(numbers) {
            let pattern = &signature.params[i].1;
            let wanted = match pattern.partial(&sol) {
                // A list written out for a parameter that nothing else settles
                // has no type to be taken as.
                t if t.unknown() && matches!(value, Expr::List { .. }) => {
                    if !reported {
                        reported = true;
                        self.errors.push(
                            Error::new(
                                value.pos(),
                                "a list is written where a list of a known type is needed",
                            )
                            .with_fix("write its type, as in `xs: List[Nat] = [1, 2, 3]`"),
                        );
                    }
                    Want::Ty(Ty::Unknown)
                }
                // A whole number that fits an `i64` is one when nothing
                // decides, with no mistake to report, so it is given its type.
                t if t.unknown() && !pattern.partial(&from_place).unknown() => {
                    Want::Ty(pattern.partial(&from_place))
                }
                t if t.unknown() && matches!(value, Expr::Call(_)) => Want::Any,
                t if t.unknown()
                    && (fits_i64(value)
                        || matches!(value, Expr::Match(m) if literal_blocks(m))) =>
                {
                    Want::Any
                }
                t if t.unknown() && reported => Want::Ty(Ty::Unknown),
                t if t.unknown() => {
                    reported = true;
                    Want::Any
                }
                t => Want::Ty(t),
            };
            let defs = self.defs;
            let written = signature.written.get(i).cloned().flatten();
            let here = signature.params[i].1.clone();
            let wanted = own_range(value, wanted, comparing);
            let gives = defs.writing(&here, written.clone(), || self.expr(body, value, &wanted));
            self.comparing = comparing;
            defs.writing(&here, written, || {
                self.fit(body, value, &gives, pattern, &mut sol, vars)
            });
            self.comparing = false;
        }
        // The two sides of a comparison that are two ranges are taken as the
        // smallest range that holds both.
        if comparing
            && let [Ty::Int(a), Ty::Int(b)] = call
                .args
                .iter()
                .map(|a| {
                    self.types
                        .get(&addr(&a.value))
                        .cloned()
                        .unwrap_or(Ty::Unknown)
                })
                .collect::<Vec<_>>()[..]
            && a != b
            && let Some(h) = a.hull(b)
        {
            for (arg, int) in call.args.iter().zip([a, b]) {
                if int != h {
                    self.widened.insert(addr(&arg.value), h);
                }
            }
        }
        // A lambda whose parameters' types nothing decided, and which writes
        // none of them.
        if self.errors.len() == errors_before
            && let Some(l) = lambdas.iter().find_map(|&(i, l)| {
                let Ty::Fn(items) = signature.params[i].1.partial(&sol) else {
                    return None;
                };
                (l.types.iter().zip(&items))
                    .any(|(t, w)| t.is_none() && *w == Ty::Unknown)
                    .then_some(l)
            })
        {
            self.errors.push(
                Error::new(
                    l.pos,
                    "nothing decides the types of this lambda's parameters, so it writes them",
                )
                .with_fix("write each type, as in `fn(v: T) { ... }`"),
            );
        }
        let missing: Vec<String> = (signature.params.iter().enumerate())
            .zip(given.iter().zip(&signature.defaults))
            .filter(|(_, (given, default))| !**given && !**default)
            .filter(|((_, (p, _)), _)| !(unknown && p.contains('@')))
            .map(|((i, (p, _)), _)| param_words(p, i))
            .collect();
        let all_by_name = !missing.is_empty()
            && signature
                .named_from
                .is_some_and(|k| (0..k).all(|i| given[i] || signature.defaults[i]));
        if is_print && !missing.is_empty() {
            self.errors.push(
                Error::new(call.end, "`print` needs the text to write")
                    .with_fix("write `print(\"hello\")`"),
            );
        } else if all_by_name {
            self.errors.push(Error::new(
                call.end,
                format!("`{}` needs {}", call.callee, sentence(missing)),
            ));
        } else if !missing.is_empty() {
            self.errors.push(Error::new(
                call.end,
                format!(
                    "`{}` needs an argument for {}",
                    call.callee,
                    sentence(missing)
                ),
            ));
        }
        sol
    }

    /// Checks a circle of functions that call each other, which the call at
    /// `pos` from `f` to `callee` closes among the functions being walked: each has a
    /// `decreases` of as many parts as the others, each call between two of
    /// them was proven smaller by the measure the callee states, and when one
    /// of the calls is not the last thing a function does, each carries `alloc`,
    /// as the frames are kept in its memory.
    fn circle(&mut self, f: usize, callee: usize, pos: Pos) {
        let defs = self.defs;
        let at = self.walking.iter().position(|&m| m == callee);
        let Some(at) = at else { return };
        let mut members: Vec<usize> = self.walking[at..].to_vec();
        members.push(f);
        let calls: Vec<Pos> = self.walked_calls[at..]
            .iter()
            .copied()
            .chain([pos])
            .collect();
        let mut sorted = members.clone();
        sorted.sort_unstable();
        if self.circles.contains(&sorted) {
            return;
        }
        self.circles.push(sorted);
        let name = |m: usize| format!("`{}`", defs.file.functions[m].name);
        let names = listed(&members.iter().map(|&m| name(m)).collect::<Vec<_>>());
        let without: Vec<String> = (members.iter())
            .filter(|&&m| defs.file.functions[m].decreases.is_empty())
            .map(|&m| name(m))
            .collect();
        if !without.is_empty() {
            self.errors.push(
                Error::new(
                    pos,
                    format!(
                        "{names} call each other, and a function in a circle of functions needs a `decreases`, which {} {}",
                        listed(&without),
                        if without.len() == 1 { "has not" } else { "have none" }
                    ),
                )
                .with_fix(
                    "write `decreases` after the signature, with a whole number that is smaller at each call, and as many parts as the other functions of the circle have",
                ),
            );
            return;
        }
        let parts = defs.file.functions[members[0]].decreases.len();
        if members
            .iter()
            .any(|&m| defs.file.functions[m].decreases.len() != parts)
        {
            let counts = (members.iter())
                .map(|&m| format!("{} has {}", name(m), defs.file.functions[m].decreases.len()))
                .collect::<Vec<_>>()
                .join(", ");
            self.errors.push(
                Error::new(
                    pos,
                    format!(
                        "{names} call each other, and the `decreases` of each must have the same number of parts: {counts}"
                    ),
                )
                .with_fix("give each function of the circle a measure of the same number of parts"),
            );
            return;
        }
        for k in 0..members.len() {
            let (from, to) = (members[k], members[(k + 1) % members.len()]);
            let found: Vec<Error> = (self.cross.iter())
                .filter(|(a, b, _)| (*a, *b) == (from, to))
                .map(|(_, _, e)| e.clone())
                .collect();
            self.errors.extend(found);
        }
        let keeps_a_frame = members
            .iter()
            .zip(&calls)
            .any(|(&m, call)| !tail_positions(&defs.file.functions[m]).contains(call));
        if keeps_a_frame {
            for &m in &members {
                let f = &defs.file.functions[m];
                if !f.effects.iter().any(|(e, _)| e == "alloc") {
                    self.errors.push(
                        Error::new(
                            f.pos,
                            format!(
                                "{} is in a circle of functions that call each other, which keeps a frame at a call that is not the last thing a function does, so it needs the `alloc` effect, which `{}` does not declare",
                                name(m),
                                f.name
                            ),
                        )
                        .with_fix(format!("add `alloc` to the effects of `{}`", f.name)),
                    );
                }
            }
        }
    }

    /// Reports each call that closes a cycle of calls between functions
    /// written on their own, in a walk of the calls from `f`. `path` is the
    /// functions being walked, and `done` those already walked.
    ///
    /// The calls in a default that a call leaves out are calls of the
    /// function the call is written in, as the default is filled in there.
    pub(super) fn recursion(&mut self, f: usize, path: &mut Set<Item>, done: &mut Set<Item>) {
        path.insert(Item::Fn(f));
        let defs = self.defs;
        let mut all = Vec::new();
        let mut cycles = Vec::new();
        // The functions passed as values, which may be called.
        let mut passed = Vec::new();
        for e in exprs(values(&defs.file.functions[f].body)) {
            if let Some(&g) = self.fn_values.get(&addr(e)) {
                passed.push((e.pos(), g));
            }
            let Expr::Call(call) = e else {
                continue;
            };
            if self.value_calls.contains(&addr(call)) {
                continue;
            }
            all.push(call);
            defs.defaults_called(call, &mut Vec::new(), &mut all, &mut cycles);
        }
        self.errors.extend(cycles.into_iter().map(default_cycle));
        let called = (all.into_iter())
            .filter(|call| call.qualifier.is_none())
            .filter_map(|call| {
                let callee = self.defs.function(call.name_pos, call.callee.as_str())?;
                Some((call.pos, callee))
            });
        let reached: Vec<(Pos, usize)> = called.chain(passed).collect();
        let own = &defs.file.functions[f];
        for (pos, callee) in reached {
            if callee == f {
                // A call of the function from itself ends only by the measure
                // of its `decreases`, which `call_decreases` proves smaller
                // where the call is checked.
                if own.decreases.is_empty() {
                    self.errors.push(
                        Error::new(
                            pos,
                            format!(
                                "`{}` calls itself, and a function that calls itself needs a `decreases`",
                                own.name
                            ),
                        )
                        .with_fix(
                            "write `decreases` after the signature, with a whole number that is smaller at each call",
                        ),
                    );
                }
            } else if path.contains(&Item::Fn(callee)) {
                self.circle(f, callee, pos);
            } else if !done.contains(&Item::Fn(callee)) {
                self.walking.push(f);
                self.walked_calls.push(pos);
                self.recursion(callee, path, done);
                self.walked_calls.pop();
                self.walking.pop();
            }
        }
        path.remove(&Item::Fn(f));
        done.insert(Item::Fn(f));
    }
}

/// Where the calls are in the body of `f` that are the last thing it does: the
/// call it ends with, the calls the blocks of its last `if` or `match` end
/// with, and the call a `return` gives.
fn tail_positions(f: &Function) -> Vec<Pos> {
    let mut out = Vec::new();
    tail_calls(&f.body, &mut out);
    for e in exprs(values(&f.body)) {
        if let Expr::Return { value: Some(v), .. } = e
            && let Expr::Call(c) = &**v
        {
            out.push(c.pos);
        }
    }
    out
}

/// Where the calls are that a block ends with, which are the last thing a
/// function does when the block is its body: the call it ends with, or the
/// calls the blocks of its last `if` or `match` end with, and the call a
/// `return` gives.
fn tail_calls(stmts: &[Stmt], out: &mut Vec<Pos>) {
    match stmts.last() {
        Some(Stmt::Expr(Expr::Match(m))) if matches!(m.form, Form::If { .. } | Form::Match) => {
            for arm in &m.arms {
                tail_calls(&arm.body, out);
            }
        }
        Some(Stmt::Expr(Expr::Call(c))) => out.push(c.pos),
        _ => {}
    }
}

/// Names written as a sentence says them: `a`, `a` and `b`, or `a`, `b`, and `c`.
fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// The type with each argument of an enum that is not known made `()`.
fn filled(ty: &Ty) -> Ty {
    match ty {
        Ty::Unknown => Ty::Nothing,
        Ty::Enum(e, args) => Ty::Enum(*e, args.iter().map(filled).collect()),
        other => other.clone(),
    }
}

/// What a field that holds a function is, where a call is written on it.
enum Held {
    /// The function, which is known.
    Named(usize),
    /// The types of the parameters and the result of a function that is not
    /// known yet, since the type parameter that stands for it is not settled.
    Items(Vec<Ty>),
}

impl<'a> Checker<'a> {
    /// What an argument is written as: a block as the source has it, and any
    /// other expression as `written_expr` says.
    fn argument_text(&self, arg: &Arg) -> String {
        match &arg.value {
            Expr::Lambda(l) => self
                .source_between(l.pos, l.end)
                .unwrap_or_else(|| "{ ... }".to_string()),
            value if arg.mutable.is_some() => format!("mut {}", written_expr(value)),
            value => written_expr(value),
        }
    }

    /// The source from `from` through the character at `to`, for a place in the
    /// file that is built.
    fn source_between(&self, from: Pos, to: Pos) -> Option<String> {
        if from.line >= STEP || to.line >= STEP {
            return None;
        }
        let text = self.defs.file.text.as_ref()?;
        let lines: Vec<&str> = text.split('\n').collect();
        let mut out = String::new();
        for line in from.line..=to.line {
            let chars = lines.get(line - 1)?.chars();
            let skip = if line == from.line { from.col - 1 } else { 0 };
            let take = if line == to.line {
                to.col - skip.min(to.col)
            } else {
                usize::MAX
            };
            let part: String = chars.skip(skip).take(take).collect();
            // A block on several lines is advised on one, as a line break is no
            // part of a text.
            if line > from.line {
                out.push(' ');
            }
            out.push_str(if line > from.line {
                part.trim_start()
            } else {
                &part
            });
        }
        Some(out)
    }

    /// The call as it is written, each argument as it is, with those that the
    /// closure gives a name for written with it, and a block after the call
    /// after it.
    pub(super) fn call_written(
        &self,
        call: &Call,
        name_of: &dyn Fn(usize, &Arg) -> Option<String>,
    ) -> String {
        let mut args = Vec::new();
        let mut after = String::new();
        for (i, arg) in call.args.iter().enumerate() {
            if arg.after {
                after = format!(" {}", self.argument_text(arg));
                continue;
            }
            let text = self.argument_text(arg);
            args.push(match name_of(i, arg) {
                Some(name) => format!("{name}: {text}"),
                None => text,
            });
        }
        let qualifier = call
            .qualifier
            .as_ref()
            .map_or(String::new(), |(q, _)| format!("{q}."));
        let parentheses = if args.is_empty() && !after.is_empty() {
            String::new()
        } else {
            format!("({})", args.join(", "))
        };
        format!("{qualifier}{}{parentheses}{after}", call.callee)
    }

    /// The call with each argument by position that is for a parameter after
    /// the `;` written with that parameter's name.
    fn call_by_name(&self, call: &Call, signature: &Signature, positional: usize) -> String {
        let first = signature.named_from.unwrap_or(signature.params.len());
        let mut place = 0;
        let mut names: HashMap<usize, String> = HashMap::new();
        for (i, arg) in call.args.iter().enumerate() {
            if arg.name.is_none() && !arg.after {
                if place >= positional
                    && let Some((name, _)) = signature.params.get(first + place - positional)
                {
                    names.insert(i, name.clone());
                }
                place += 1;
            }
        }
        self.call_written(call, &|i, arg| {
            names
                .get(&i)
                .cloned()
                .or_else(|| arg.name.as_ref().map(|(n, _)| n.clone()))
        })
    }

    /// The call with the argument for the parameter `name` written without it.
    fn call_with_name_taken_off(&self, call: &Call, name: &str) -> String {
        self.call_written(call, &|_, arg| match &arg.name {
            Some((n, _)) if n != name => Some(n.clone()),
            _ => None,
        })
    }
}
