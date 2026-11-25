//! Lowers functions, calls, and lambdas: a function and its name, `return`
//! and what leaves a function or a lambda early, and a call, which is lowered
//! in place, with its arguments.

use super::frames::holds_state;
use super::*;

/// The names a block writes where it uses a name: as a name, as the function
/// a call calls, or as what it assigns.
pub(super) fn used_names(body: &[Stmt]) -> Vec<&str> {
    let mut names = Vec::new();
    for e in exprs(values(body)) {
        match e {
            Expr::Name { name, .. } => names.push(name.as_str()),
            Expr::Call(call) if call.qualifier.is_none() => names.push(call.callee.as_str()),
            _ => {}
        }
    }
    for s in statements(body) {
        if let Stmt::Assign { name, .. } | Stmt::Define { name, .. } = s {
            names.push(name.as_str());
        }
    }
    names
}

/// The function with the frame it is written in numbered as `map` says.
pub(super) fn moved<'a>(func: &Func<'a>, map: &[(usize, usize)]) -> Func<'a> {
    let to = |frame: &usize| {
        map.iter()
            .find(|(from, _)| from == frame)
            .expect("a frame of the handler")
            .1
    };
    match func {
        Func::Lambda(l, frame, env) => Func::Lambda(l, to(frame), env.clone()),
        Func::Each(l, frame, env) => Func::Each(l, to(frame), env.clone()),
        Func::Named(instance) => Func::Named(instance.clone()),
    }
}

/// Whether a `return` is written in a block, and not in a lambda in it,
/// which leaves the lambda.
pub(super) fn has_return(body: &[Stmt]) -> bool {
    stops(body, false, true)
}

/// Whether the block of a `loop` can answer `.stop`: it has a `break` that
/// is not in a loop of its own, or a `return` that is not in a lambda.
pub(super) fn can_stop(body: &[Stmt]) -> bool {
    stops(body, true, true)
}

/// Whether a block holds a `break` that ends the loop it is the block of, if
/// `breaks` asks for one, or a `return` that leaves the function around it,
/// if `returns` asks for one.
pub(super) fn stops(body: &[Stmt], breaks: bool, returns: bool) -> bool {
    fn one(e: &Expr, breaks: bool, returns: bool) -> bool {
        let any = |es: &mut dyn Iterator<Item = &Expr>, b: bool, r: bool| {
            let mut found = false;
            for e in es {
                found = found || one(e, b, r);
            }
            found
        };
        match e {
            Expr::Break { .. } => breaks,
            Expr::Return { .. } => returns,
            Expr::Call(call) => any(&mut call.args.iter().map(|a| &a.value), breaks, returns),
            Expr::Field { of, .. } => one(of, breaks, returns),
            Expr::Arith { left, right, .. } => {
                one(left, breaks, returns) || one(right, breaks, returns)
            }
            Expr::Case { payload, .. } => payload.as_ref().is_some_and(|p| one(p, breaks, returns)),
            Expr::Match(m) => {
                one(&m.of, breaks, returns)
                    || m.arms
                        .iter()
                        .any(|a| any(&mut values(&a.body), breaks, returns))
            }
            Expr::Tuple { items, .. } | Expr::List { items, .. } => {
                any(&mut items.iter(), breaks, returns)
            }
            Expr::Index { of, index, .. } => {
                one(of, breaks, returns) || one(index, breaks, returns)
            }
            Expr::Slice { of, from, to, .. } => {
                one(of, breaks, returns)
                    || [from, to]
                        .into_iter()
                        .flatten()
                        .any(|end| one(end, breaks, returns))
            }
            // A `return` in a lambda leaves the lambda.
            Expr::Lambda(l) => any(&mut values(&l.body), false, false),
            // A `break` in a loop ends that loop.
            Expr::Loop(l) => {
                let heads = (l.range.iter().flat_map(|r| [&r.from, &r.to]))
                    .chain(l.over.iter().map(|o| &o.value))
                    .chain(l.decreases.iter());
                heads.into_iter().any(|e| one(e, breaks, returns))
                    || any(&mut values(&l.body), false, returns)
            }
            Expr::Interp { parts, .. } => parts
                .iter()
                .any(|p| matches!(p, Part::Expr(e) if one(e, breaks, returns))),
            Expr::Text { .. }
            | Expr::Number { .. }
            | Expr::Float { .. }
            | Expr::Name { .. }
            | Expr::Unbuilt { .. } => false,
        }
    }
    values(body).any(|e| one(e, breaks, returns))
}

/// Whether an effect is one that waits for the system: `net` and `time` of the
/// library.
pub(super) fn of_a_waiting_effect(defs: &Defs, effect: &str) -> bool {
    defs.effect_named(effect).is_some_and(|at| {
        let e = &defs.file.effects[at];
        crate::check::of_std(e.pos) && matches!(crate::check::written_name(&e.name), "net" | "time")
    })
}

/// The names that were put in their frames without parameters: the frame, the
/// place in it, the scalars, and the depth of the region if the name is one.
pub(super) type Later = Vec<(usize, usize, Vec<form::Type>, Option<usize>)>;

/// Makes the parameters of the names that were put in their frames without any,
/// in the order they were put.
pub(super) fn finish_params<'a>(lowering: &mut Lowering<'a>, out: &mut Out<'a>, later: Later) {
    for (frame, at, types, region) in later {
        out.frames[frame][at].values = out.params(&types);
        if let Some(depth) = region {
            let address = out.frames[frame][at].values[0];
            lowering.name_region(out, address, depth);
        }
    }
}

impl<'a> Lowering<'a> {
    pub(super) fn function(&mut self, instance: &Instance) -> form::Function {
        self.function_with(instance, &[])
    }

    /// `function`, for a copy that runs under the handlers of `context`.
    fn function_with(
        &mut self,
        instance: &Instance,
        context: &[(HandlerId, Rc<HandlerDef<'a>>)],
    ) -> form::Function {
        let defs = self.typed.defs;
        if let Template::Given(k, fi) = instance.template {
            let name = self.given_name(k, fi);
            let ty = defs.text(&instance.args[0], &Scope::default());
            return self
                .given(k, fi, &instance.args)
                .named(spaceless(format!("<{name}[{ty}]>")));
        }
        let view = defs.view(instance.template);
        let name = self.name_of(instance);
        let mut out = Out::new();
        out.written = form::names::intern(&name);
        let mut homed = HashSet::new();
        indexed_names(&view.f.body, &mut homed);
        out.homed.insert(out.written, homed);
        out.place = form::Place::of(&name);
        for (name, ty) in &view.params {
            let ty = ty.subst(&instance.args);
            let types = self.scalars(&ty);
            let values = out.params(&types);
            out.define(name, ty, values);
        }
        let mut slots: Vec<Slot> = Vec::new();
        let order = self.state_last(
            context
                .iter()
                .map(|(_, def)| (def.effect, holds_state(def))),
        );
        let mut maps: Vec<Vec<(usize, usize)>> = vec![Vec::new(); context.len()];
        let mut made: Vec<Vec<Slot>> = (0..context.len()).map(|_| Vec::new()).collect();
        let mut later = Vec::new();
        for i in order {
            maps[i] = self.push_frames(&mut out, &context[i].1, &mut made[i], Some(&mut later));
        }
        finish_params(self, &mut out, later);
        // What it gives back is in the order it takes what it is given.
        let order = self.state_last(
            context
                .iter()
                .map(|(_, def)| (def.effect, holds_state(def))),
        );
        for i in order {
            slots.append(&mut made[i]);
        }
        for ((id, def), map) in context.iter().zip(maps) {
            let below = out.handlers.len();
            out.handlers.push(Handler {
                id: *id,
                effect: def.effect,
                operations: def.operations.iter().map(|f| moved(f, &map)).collect(),
                def: Some(def.clone()),
                frames: map.iter().map(|(_, frame)| *frame).collect(),
                below,
            });
        }
        let gives = view.gives.subst(&instance.args);
        let code = Code::Stmts(&view.f.body);
        out.give_back = slots.clone();
        // A `return` gives back the names the function gives back as they are
        // where it leaves, so the frames they are in are noted there.
        let limits: Vec<usize> = match slots.is_empty() {
            true => Vec::new(),
            false => out.frames.iter().map(Vec::len).collect(),
        };
        let mut result = self.leaving(&mut out, code, &instance.args, (&gives, true), limits);
        if view.gives == Ty::Nothing {
            result.clear();
        }
        let mut results = self.scalars(&gives);
        // What it gives back of the names the functions of its handlers assign,
        // as they are at its end.
        for slot in &slots {
            let at = out.frames[slot.frame]
                .iter()
                .rposition(|b| b.name == slot.name)
                .expect("a name the functions of the handler use");
            result.extend(out.frames[slot.frame][at].values.iter().copied());
            results.extend(slot.types.iter().copied());
        }
        out.blocks[out.current].end = form::End::Return(result);
        form::Function {
            types: out.types,
            results,
            blocks: out.blocks,
            cold: false,
            name,
        }
    }

    /// Makes a frame of `out` for each frame a handler's functions use, with the
    /// names they use in it: a name that stands for values is a parameter, one
    /// that holds a function holds it as it is, moved to the new frames. Gives
    /// which new frame each of the old ones is, and notes in `slots` each name
    /// the functions assign.
    pub(super) fn push_frames(
        &mut self,
        out: &mut Out<'a>,
        def: &HandlerDef<'a>,
        slots: &mut Vec<Slot>,
        mut later: Option<&mut Later>,
    ) -> Vec<(usize, usize)> {
        let mut map: Vec<(usize, usize)> = Vec::new();
        for (orig, _) in &def.frames {
            map.push((*orig, out.frames.len()));
            out.frames.push(Vec::new());
        }
        for ((_, names), (_, frame)) in def.frames.iter().zip(&map) {
            for c in names {
                // What a handler's functions assign is state, which goes after
                // what they only read, so its parameters are made once every
                // handler's names are in their frames.
                let defer = c.assigned && c.func.is_none() && later.is_some();
                let (values, func) = match &c.func {
                    Some(func) => (Vec::new(), Some(moved(func, &map))),
                    None if defer => {
                        if let Some(later) = later.as_deref_mut() {
                            later.push((
                                *frame,
                                out.frames[*frame].len(),
                                c.scalars.clone(),
                                (c.name == REGION).then_some(c.depth),
                            ));
                        }
                        (Vec::new(), None)
                    }
                    None => {
                        let values = out.params(&c.scalars);
                        if c.name == REGION {
                            self.name_region(out, values[0], c.depth);
                        }
                        (values, None)
                    }
                };
                if c.assigned {
                    slots.push(Slot {
                        frame: *frame,
                        name: c.name.clone(),
                        types: c.scalars.clone(),
                    });
                }
                out.frames[*frame].push(Bound {
                    name: c.name.clone(),
                    ty: c.ty.clone(),
                    values,
                    func,
                    home: None,
                });
            }
        }
        map
    }

    /// What the map of an executable calls the copy of a function: its module
    /// and its name as written, and the types it is made for in brackets.
    pub(super) fn name_of(&self, instance: &Instance) -> String {
        let defs = self.typed.defs;
        // The types it is made for: those the implementation is for, and then
        // the function's own.
        let (pos, name, types) = match instance.template {
            Template::Free(f) => (
                defs.file.functions[f].pos,
                &defs.file.functions[f].name,
                instance.args.clone(),
            ),
            Template::Impl(i, fi) => {
                let info = defs.impls[i]
                    .as_ref()
                    .expect("an implementation that is known");
                let f = &defs.file.impls[i].functions[info.fns[fi].expect("implemented")];
                let own = instance.args.iter().skip(info.scope.names.len());
                let types = (info.args.iter())
                    .map(|ty| ty.subst(&instance.args))
                    .chain(own.cloned())
                    .collect();
                (f.pos, &f.name, types)
            }
            Template::Given(..) => unreachable!("a given implementation is named by its kind"),
        };
        let module = if of_prelude(pos) {
            "prelude"
        } else {
            let number = module_of(pos);
            (defs.file.modules.iter())
                .find(|(n, _)| *n == number)
                .map_or("main", |(_, label)| label.as_str())
        };
        let mut text = format!("{module}.{}", written_name(name));
        if !types.is_empty() {
            let args: Vec<String> = (types.iter())
                .map(|ty| defs.text(ty, &Scope::default()))
                .collect();
            text.push_str(&format!("[{}]", args.join(",")));
        }
        spaceless(text)
    }

    /// Lowers the body of a function or of a lambda, or the block of a `loop`
    /// over a value, which gives `gives`, and gives the values it gives. A
    /// `return` in it ends its block with them when `whole` says the body is
    /// the whole of a function, and otherwise jumps to the block where what
    /// the body gives is known, which also knows what the names that
    /// outlive the body, `limits` of each frame, are then.
    pub(super) fn leaving(
        &mut self,
        out: &mut Out<'a>,
        code: Code<'a>,
        env: &[Ty],
        (gives, whole): (&Ty, bool),
        limits: Vec<usize>,
    ) -> Vec<form::Value> {
        let kept = match code {
            Code::Each(l, _) => self.kept_of(out, l),
            Code::Stmts(_) => None,
        };
        out.returns.push(Returns {
            gives: gives.clone(),
            limits: limits.clone(),
            ends: Vec::new(),
            kept,
            of: match code {
                Code::Each(l, _) => Some(addr(l)),
                Code::Stmts(_) => None,
            },
        });
        let result = match code {
            Code::Stmts(body) => self.block(out, body, env),
            Code::Each(l, read) => {
                out.loops.push(Round {
                    carried: Vec::new(),
                    leaving: Vec::new(),
                    body: true,
                });
                self.block(out, &l.body, env);
                out.loops.pop();
                if read {
                    vec![self.flow(out, false)]
                } else {
                    Vec::new()
                }
            }
        };
        let mut ends = out.returns.pop().expect("pushed").ends;
        if ends.is_empty() {
            return result;
        }
        let types = self.scalars(gives);
        for end in &mut ends {
            end.args.truncate(types.len());
        }
        let reached = !out.dead.contains(&out.current);
        if whole {
            // What follows the last `return` ends with the values it gave, as
            // it is reached by nothing and the end of a function takes some.
            let last = ends.last().map(|end| end.args.clone());
            for end in ends {
                // The names the function gives back are given as they are where
                // the `return` leaves.
                let mut args = end.args;
                for slot in &out.give_back {
                    let at = (out.frames[slot.frame].iter())
                        .rposition(|b| b.name == slot.name)
                        .expect("a name the function gives back");
                    args.extend(end.names[slot.frame][at].iter().copied());
                }
                out.blocks[end.block].end = form::End::Return(args);
            }
            return if reached {
                result
            } else {
                last.unwrap_or_default()
            };
        }
        // What follows the last `return` is reached by nothing, and jumps with
        // the values the `return` gave, as a jump has to give some.
        let (args, names) = match ends.last() {
            Some(last) if !reached => (last.args.clone(), last.names.clone()),
            _ => (result, self.names_now(out, &limits)),
        };
        ends.push(Leaving {
            block: out.current,
            args,
            names,
        });
        self.join_returns(out, &types, ends, &limits)
    }

    /// Ends the block being filled by leaving the function or the lambda
    /// being lowered with `values`. What follows is reached by nothing.
    pub(super) fn leave(&mut self, out: &mut Out<'a>, values: Vec<form::Value>) {
        let limits = out.returns.last().expect("checked").limits.clone();
        let names = self.names_now(out, &limits);
        let block = out.current;
        let returns = out.returns.last_mut().expect("checked");
        returns.ends.push(Leaving {
            block,
            args: values,
            names,
        });
        out.current = out.new_block();
        out.dead.push(out.current);
    }

    /// Makes a block where the blocks of `ends`, which give it values, meet,
    /// and gives the values it takes. The names that outlive the body and
    /// are not the same at every end are taken too, and have their values
    /// there from then on.
    pub(super) fn join_returns(
        &mut self,
        out: &mut Out<'a>,
        types: &[form::Type],
        ends: Vec<Leaving>,
        limits: &[usize],
    ) -> Vec<form::Value> {
        let mut changed = Vec::new();
        for (f, &n) in limits.iter().enumerate() {
            for at in 0..n {
                if ends.iter().any(|e| e.names[f][at] != ends[0].names[f][at]) {
                    changed.push((f, at));
                }
            }
        }
        let mut all = types.to_vec();
        for &(f, at) in &changed {
            all.extend(ends[0].names[f][at].iter().map(|v| out.types[v.0]));
        }
        let sizes: Vec<usize> = (changed.iter())
            .map(|&(f, at)| ends[0].names[f][at].len())
            .collect();
        let ends = (ends.into_iter())
            .map(|mut end| {
                for &(f, at) in &changed {
                    end.args.extend(end.names[f][at].clone());
                }
                (end.block, end.args)
            })
            .collect();
        let params = out.join(&all, ends);
        let mut at_param = types.len();
        for ((f, at), size) in changed.into_iter().zip(sizes) {
            out.frames[f][at].values = params[at_param..at_param + size].to_vec();
            at_param += size;
        }
        params[..types.len()].to_vec()
    }

    /// An argument for a parameter, as the name the parameter is then.
    pub(super) fn argument(
        &mut self,
        out: &mut Out<'a>,
        value: &'a Expr,
        (name, ty): &(String, Ty),
        env: &[Ty],
    ) -> Bound<'a> {
        let (values, func) = if let Ty::Fn(_) = ty {
            let func = match value {
                Expr::Lambda(l) => Func::Lambda(l, out.at, env.to_vec()),
                Expr::Name { name, .. } => match out.find(name) {
                    Some(at) => out.names()[at].func.clone().expect("a function"),
                    None => Func::Named(Instance {
                        template: Template::Free(self.typed.fn_values[&addr(value)]),
                        args: Vec::new(),
                    }),
                },
                // A field that holds a function is the function its struct holds.
                Expr::Field { of, name, .. }
                    if matches!(self.typed.type_of(of, env), Ty::Struct(..)) =>
                {
                    let Ty::Struct(s, args) = self.typed.type_of(of, env) else {
                        unreachable!("matched")
                    };
                    let defs = self.typed.defs;
                    let at = (defs.file.structs[s].fields.iter())
                        .position(|f| &f.name == name)
                        .expect("checked");
                    let Ty::Named(g) = defs.fields[s][at].subst(&args) else {
                        unreachable!("checked")
                    };
                    self.expr(out, value, env);
                    Func::Named(Instance {
                        template: Template::Free(g),
                        args: Vec::new(),
                    })
                }
                // What gives a function with a name, a call or a field, is run for
                // what it does, and gives no value.
                _ => match self.typed.type_of(value, env) {
                    Ty::Named(g) => {
                        self.expr(out, value, env);
                        Func::Named(Instance {
                            template: Template::Free(g),
                            args: Vec::new(),
                        })
                    }
                    _ => unreachable!("checked"),
                },
            };
            (Vec::new(), Some(func))
        } else {
            (self.passed(out, value, ty, env), None)
        };
        Bound {
            name: name.clone(),
            ty: ty.clone(),
            values,
            func,
            home: None,
        }
    }

    /// Runs a function a name holds, with these arguments, which gives `gives`:
    /// a lambda's body is lowered in place, in the frame it is written in,
    /// and a function with a name is called.
    pub(super) fn run_func(
        &mut self,
        out: &mut Out<'a>,
        func: Func<'a>,
        args: Vec<Bound<'a>>,
        gives: &Ty,
        quiet: bool,
    ) -> Vec<form::Value> {
        let args = args.into_iter();
        match func {
            Func::Named(instance) => {
                let prepared = self.copy_for(out, &instance);
                let args: Vec<form::Value> = args.flat_map(|a| a.values).collect();
                self.finish_call(out, prepared, args, gives)
            }
            Func::Lambda(lambda, frame, lambda_env) => {
                let caller = std::mem::replace(&mut out.at, frame);
                let before = out.names().len();
                let mut limits: Vec<usize> = out.frames.iter().map(Vec::len).collect();
                limits[frame] = before;
                for ((name, _), mut arg) in lambda.params.iter().zip(args) {
                    arg.name = name.clone();
                    out.names_mut().push(arg);
                }
                let code = Code::Stmts(&lambda.body);
                let exits = out.exits;
                let result = self.leaving(out, code, &lambda_env, (gives, false), limits);
                out.names_mut().truncate(before);
                out.at = caller;
                // A lambda that was left because the task was asked to end leaves
                // the function that runs it too.
                if out.exits > exits {
                    self.cancel_exit(out);
                }
                result
            }
            Func::Each(each, frame, each_env) => {
                let caller = std::mem::replace(&mut out.at, frame);
                let before = out.names().len();
                let mut limits: Vec<usize> = out.frames.iter().map(Vec::len).collect();
                limits[frame] = before;
                let over = each.over.as_ref().expect("a loop over a value");
                for mut arg in args {
                    arg.name = over.name.clone();
                    out.names_mut().push(arg);
                }
                let code = Code::Each(each, !quiet);
                let result = self.leaving(out, code, &each_env, (gives, false), limits);
                out.names_mut().truncate(before);
                out.at = caller;
                result
            }
        }
    }

    /// Binds a name to a lambda that reads names of its function: the name
    /// holds the values of the names it read, as of now, and the lambda, whose
    /// body is lowered where it is called.
    pub(super) fn bind_env(&mut self, out: &mut Out<'a>, name: &str, l: &'a Lambda, env: &[Ty]) {
        let defs = self.typed.defs;
        let s = defs.env_of(l).expect("a struct for the lambda");
        let fields = &defs.file.structs[s].fields;
        let own = defs.file.structs[s].generics.len() - fields.len();
        let mut args: Vec<Ty> = env[..own].to_vec();
        let mut values = Vec::new();
        for field in fields {
            let at = out.find(&field.name).expect("checked");
            args.push(out.names()[at].ty.clone());
            values.extend(out.read(at));
        }
        out.define(name, Ty::Struct(s, args), values);
        let at = out.find(name).expect("defined");
        out.names_mut()[at].func = Some(Func::Lambda(l, out.at, env.to_vec()));
    }

    /// Lowers a call of a name that holds a function. A lambda's body is
    /// lowered in place, in the frame it is written in, with its parameters
    /// standing for the arguments.
    pub(super) fn call_value(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        self.call_value_as(out, call, env, false)
    }

    /// `call_value`, where `quiet` says that nothing reads what the block of
    /// a `loop` gives, which cannot be `.stop`.
    pub(super) fn call_value_as(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        env: &[Ty],
        quiet: bool,
    ) -> (Ty, Vec<form::Value>) {
        let at = out.find(&call.callee).expect("checked");
        let local = &out.names()[at];
        let func = local.func.clone().expect("a function");
        let mut held = Vec::new();
        let items = match local.ty.clone() {
            Ty::Fn(items) => items,
            Ty::Struct(s, args) => {
                // The names the lambda read stand for the values it holds.
                let defs = self.typed.defs;
                let own = defs.file.structs[s].generics.len() - defs.file.structs[s].fields.len();
                let mut from = 0;
                for (field, ty) in defs.file.structs[s].fields.iter().zip(&defs.fields[s]) {
                    let ty = ty.subst(&args);
                    let size = self.size(&ty);
                    held.push(Bound {
                        name: field.name.clone(),
                        ty,
                        values: local.values[from..from + size].to_vec(),
                        func: None,
                        home: None,
                    });
                    from += size;
                }
                self.typed.env_items[&s]
                    .iter()
                    .map(|t| t.subst(&args[..own]))
                    .collect()
            }
            _ => unreachable!("checked"),
        };
        let (gives, params) = items.split_last().expect("a result");
        let mut args: Vec<Option<Bound<'a>>> = vec![None; params.len()];
        let mut next = 0;
        for arg in &call.args {
            let at = if arg.after {
                params.len() - 1
            } else {
                next += 1;
                next - 1
            };
            let param = (String::new(), params[at].clone());
            args[at] = Some(self.argument(out, &arg.value, &param, env));
        }
        let args = args.into_iter().map(|a| a.expect("given")).collect();
        let before = out.names().len();
        out.names_mut().extend(held);
        let mut result = self.run_func(out, func, args, gives, quiet);
        out.names_mut().truncate(before);
        if *gives == Ty::Nothing {
            result.clear();
        }
        (gives.clone(), result)
    }

    /// Lowers a call of the name of an effect: the functions given for its
    /// operations are in force while the body runs, in place, and no longer.
    pub(super) fn handle(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        effect: usize,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let typed = self.typed;
        let signature = typed.defs.handler_signature(effect);
        let places = params_of(&signature.params, &call.args).expect("checked");
        let mut given: Vec<Option<Bound<'a>>> = vec![None; signature.params.len()];
        for (arg, at) in call.args.iter().zip(places) {
            given[at] = Some(self.argument(out, &arg.value, &signature.params[at], env));
        }
        let mut given: Vec<Bound<'a>> = given.into_iter().map(|a| a.expect("given")).collect();
        let body = given.pop().expect("a body").func.expect("a function");
        let operations: Vec<Func<'a>> = (given.into_iter())
            .map(|b| b.func.expect("a function"))
            .collect();
        let below = out.handlers.len();
        let id = self.handler_id(call, out);
        // What the operations of a handler store in its state is built in a
        // region of its own, which the handler's block ends, so that a child
        // that adds to the state never puts it in a stack that is given back
        // when the child ends.
        let (def, region) = self.region_of_handler(out, effect, &operations);
        if let Some(frame) = region {
            out.regions.push(MadeRegion {
                handler: id,
                frame,
                escapes: false,
            });
        }
        out.handlers.push(Handler {
            id,
            effect,
            operations,
            def,
            frames: Vec::new(),
            below,
        });
        let gives = typed.type_of(expr, env);
        let mut result = self.run_func(out, body, Vec::new(), &gives, false);
        out.handlers.truncate(below);
        if gives == Ty::Nothing {
            result.clear();
        }
        (gives, result)
    }

    /// The handler's definition and its region, when its operations assign a
    /// name that holds memory: the region is made here, and is a name of the
    /// function the operations are written in, which goes through each poll as the
    /// rest of the state does.
    fn region_of_handler(
        &mut self,
        out: &mut Out<'a>,
        effect: usize,
        operations: &[Func<'a>],
    ) -> (Option<Rc<HandlerDef<'a>>>, Option<usize>) {
        if self.typed.defs.is_scheduler(effect) {
            return (None, None);
        }
        let mut def = self.capture(out, effect, operations);
        let holds = |c: &Captured<'a>| c.assigned && c.scalars.contains(&form::Type::Address);
        let Some(at) = def
            .frames
            .iter()
            .position(|(_, names)| names.iter().any(holds))
        else {
            return (None, None);
        };
        let address = out.emit(
            form::Op::System {
                call: form::System::RegionNew,
                args: Vec::new(),
            },
            vec![form::Type::Address],
        )[0];
        let depth = out.handlers.len();
        self.name_region(out, address, depth);
        let frame = def.frames[at].0;
        out.frames[frame].push(Bound {
            name: REGION.to_string(),
            ty: Ty::Nothing,
            values: vec![address],
            func: None,
            home: None,
        });
        def.frames[at].1.push(Captured {
            name: REGION.to_string(),
            ty: Ty::Nothing,
            assigned: true,
            scalars: vec![form::Type::Address],
            func: None,
            depth,
        });
        (Some(Rc::new(def)), Some(frame))
    }

    /// Says that the address is a name of the frame of a region that is `depth`
    /// handlers that have a region inside the outermost, so that the memory that a
    /// region keeps is built in a frame that is made where it is asked for.
    pub(super) fn name_region(&mut self, out: &mut Out<'a>, frame: form::Value, depth: usize) {
        let depth = out.int(form::Bits::B64, depth as i128);
        out.emit(
            form::Op::System {
                call: form::System::RegionKeep,
                args: vec![frame, depth],
            },
            Vec::new(),
        );
    }

    /// The region of the state of the handler whose operation is being lowered,
    /// when it has one and `name` is a name of its state that holds memory.
    pub(super) fn region_for(&self, out: &Out<'a>, name: &str) -> Option<(form::Value, usize)> {
        let operating = out.operating.last()?;
        let (i, _) = operating
            .def
            .frames
            .iter()
            .enumerate()
            .find(|(_, (_, names))| {
                names.iter().any(|c| {
                    c.name == name && c.assigned && c.scalars.contains(&form::Type::Address)
                })
            })?;
        let (orig, names) = &operating.def.frames[i];
        let region = names.iter().find(|c| c.name == REGION)?;
        let frame = operating.frames.get(i).copied().unwrap_or(*orig);
        let at = out.frames[frame].iter().rposition(|b| b.name == REGION)?;
        Some((out.frames[frame][at].values[0], region.depth))
    }

    /// Lowers a call of `alloc`: the body runs in place, between the opening
    /// and the closing of an arena, which holds what the body allocates and
    /// does not hand out. A `return` in the body leaves the body, so it ends
    /// at the closing, and a `break` is refused where the body is checked.
    pub(super) fn arena(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let param = (String::new(), Ty::Fn(Vec::new()));
        let body = (self.argument(out, &call.args[0].value, &param, env).func).expect("a function");
        let gives = self.typed.type_of(expr, env);
        let token = out.emit(
            form::Op::System {
                call: form::System::ArenaOpen,
                args: Vec::new(),
            },
            vec![form::Type::Int(form::Bits::B8)],
        )[0];
        let mut result = self.run_func(out, body, Vec::new(), &gives, false);
        out.emit(
            form::Op::System {
                call: form::System::ArenaClose,
                args: vec![token],
            },
            Vec::new(),
        );
        if gives == Ty::Nothing {
            result.clear();
        }
        (gives, result)
    }

    /// Lowers a call of an operation of an effect: the function the handler in
    /// force gives for it, run in place with the call's arguments, under the
    /// handlers that were in force outside that handler.
    pub(super) fn operation_call(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        instance: Instance,
        gives: Ty,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let typed = self.typed;
        let defs = typed.defs;
        let Template::Free(f) = instance.template else {
            unreachable!("an operation is a function")
        };
        let (effect, at) = defs.operation_of(f);
        let Some(handler) = out.handlers.iter().rposition(|h| h.effect == effect) else {
            // No handler is in force, so the target answers it.
            assert!(
                defs.provided(f).is_some(),
                "an effect that is handled where it is asked of"
            );
            if let Some(through) = defs.provided(f).and_then(|p| p.through) {
                return self.answered_through(out, call, f, through, gives, env);
            }
            return self.provided_answer(out, call, instance, gives, env);
        };
        let params = defs.params(instance.template);
        let mut args: Vec<Option<Bound<'a>>> = vec![None; params.len()];
        for (arg, at) in call
            .args
            .iter()
            .zip(params_of(&params, &call.args).expect("checked"))
        {
            args[at] = Some(self.argument(out, &arg.value, &params[at], env));
        }
        for (at, default) in defs.left_out(call) {
            args[at] = Some(self.argument(out, default, &params[at], &[]));
        }
        let args = args.into_iter().map(|a| a.expect("given")).collect();
        self.through_handler(out, (handler, effect, at), instance, args, gives)
    }

    /// Runs the function the handler gives for an operation, in place, with the
    /// arguments made, under the handlers that were in force outside it.
    pub(super) fn through_handler(
        &mut self,
        out: &mut Out<'a>,
        (handler, effect, at): (usize, usize, usize),
        instance: Instance,
        args: Vec<Bound<'a>>,
        gives: Ty,
    ) -> (Ty, Vec<form::Value>) {
        let typed = self.typed;
        let defs = typed.defs;
        let func = match out.handlers[handler].operations[at].clone() {
            // The type parameters an operation has of its own are those of the
            // call, after the type parameters of the function the handler is
            // written in, where the lambda was checked against them.
            Func::Lambda(lambda, frame, mut lambda_env) if !instance.args.is_empty() => {
                let (before, all) = defs.own_parameters(effect, at);
                let base = typed.lambda_base[&addr(lambda)];
                lambda_env.resize(base + before, Ty::Nothing);
                lambda_env.extend(instance.args.iter().cloned());
                lambda_env.resize(base + all, Ty::Nothing);
                Func::Lambda(lambda, frame, lambda_env)
            }
            func => func,
        };
        if out.handlers[handler].def.is_none() {
            let (effect, operations) = (
                out.handlers[handler].effect,
                out.handlers[handler].operations.clone(),
            );
            let def = self.capture(out, effect, &operations);
            out.handlers[handler].def = Some(Rc::new(def));
        }
        out.operating.push(Operating {
            def: out.handlers[handler].def.clone().expect("made"),
            frames: out.handlers[handler].frames.clone(),
        });
        let outside = out.handlers.split_off(out.handlers[handler].below);
        let mut result = self.run_func(out, func, args, &gives, false);
        out.handlers.extend(outside);
        out.operating.pop();
        // What an operation gives out may be memory of the region of the
        // state, so the region is not given back where the handler's block
        // ends, but goes with what the block gives.
        if result.iter().any(|v| out.types[v.0] == form::Type::Address) {
            let id = out.handlers[handler].id;
            if let Some(made) = out.regions.iter_mut().rev().find(|r| r.handler == id) {
                made.escapes = true;
            }
        }
        if gives == Ty::Nothing {
            result.clear();
        }
        (gives, result)
    }

    /// Ends a handle of `net`, as the program that owns it would by calling
    /// `end`, which is `close`, `stop`, or `unbind`: through the handler in force,
    /// or the function the library answers it with where none is.
    pub(super) fn end_handle(
        &mut self,
        out: &mut Out<'a>,
        end: &str,
        ty: &Ty,
        values: &[form::Value],
    ) {
        let defs = self.typed.defs;
        let f = (0..defs.file.functions.len())
            .find(|&f| {
                let g = &defs.file.functions[f];
                g.name == end
                    && defs.is_operation(f)
                    && crate::check::of_std(g.pos)
                    && crate::check::written_name(&g.effects[0].0) == "net"
            })
            .expect("an operation of `net` that ends the handle");
        let instance = Instance {
            template: Template::Free(f),
            args: Vec::new(),
        };
        let (effect, at) = defs.operation_of(f);
        let (name, _) = defs.params(instance.template).remove(0);
        let arg = Bound {
            name,
            ty: ty.clone(),
            values: values.to_vec(),
            func: None,
            home: None,
        };
        match out.handlers.iter().rposition(|h| h.effect == effect) {
            Some(handler) => {
                self.through_handler(out, (handler, effect, at), instance, vec![arg], Ty::Nothing);
            }
            None => {
                let through = defs
                    .provided(f)
                    .and_then(|p| p.through)
                    .expect("the library answers the end of a handle");
                let g = defs
                    .function(defs.file.functions[f].pos, through)
                    .expect("the module of the effect has the function that answers it");
                let instance = Instance {
                    template: Template::Free(g),
                    args: Vec::new(),
                };
                self.run_func(out, Func::Named(instance), vec![arg], &Ty::Nothing, false);
            }
        }
    }

    /// Lowers a call of an operation that no handler answers and that the
    /// library answers in Smith, with the function of its module that is
    /// named `through`, which asks the kernel for plain numbers and builds
    /// what the operation gives. The function runs in place of the operation,
    /// with the call's arguments, under the handlers in force.
    fn answered_through(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        f: usize,
        through: &str,
        gives: Ty,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let defs = self.typed.defs;
        let g = defs
            .function(defs.file.functions[f].pos, through)
            .expect("the module of the effect has the function that answers it");
        let instance = Instance {
            template: Template::Free(g),
            args: Vec::new(),
        };
        let params = defs.params(instance.template);
        let mut args: Vec<Option<Bound<'a>>> = vec![None; params.len()];
        for (arg, at) in call
            .args
            .iter()
            .zip(params_of(&params, &call.args).expect("checked"))
        {
            args[at] = Some(self.argument(out, &arg.value, &params[at], env));
        }
        let args = args.into_iter().map(|a| a.expect("given")).collect();
        let mut result = self.run_func(out, Func::Named(instance), args, &gives, false);
        if gives == Ty::Nothing {
            result.clear();
        }
        (gives, result)
    }

    /// Lowers a call of an operation that the target answers because no handler
    /// is in force. `print` of `console` writes its text and then `end` to
    /// standard output, or to standard error when `to` is `.stderr`, and a `to`
    /// that is left out is standard output and is not worked out. `fill` of
    /// `entropy` has the system fill the bytes of its list and gives the list.
    fn provided_answer(
        &mut self,
        out: &mut Out<'a>,
        call: &'a Call,
        instance: Instance,
        gives: Ty,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let defs = self.typed.defs;
        let Template::Free(f) = instance.template else {
            unreachable!("an operation is a function")
        };
        let params = defs.params(instance.template);
        let mut args: Vec<Option<Vec<form::Value>>> = vec![None; params.len()];
        for (arg, at) in call
            .args
            .iter()
            .zip(params_of(&params, &call.args).expect("checked"))
        {
            args[at] = Some(self.passed(out, &arg.value, &params[at].1, env));
        }
        if defs.provided(f).is_some_and(|p| p.operation == "fill") {
            let list = args[0].take().expect("given");
            self.call_runtime(out, Runtime::Entropy, vec![list[0], list[1]], Vec::new());
            return (gives, list);
        }
        // What a default builds is the place of the call, as for the text.
        let place = out.place;
        for (at, default) in defs.left_out(call) {
            // The stream a call leaves out is standard output.
            if at != 2 {
                let from = out.blocks[out.current].instructions.len();
                args[at] = Some(self.passed(out, default, &params[at].1, &[]));
                for i in &mut out.blocks[out.current].instructions[from..] {
                    i.at = place;
                }
            }
        }
        if defs.provided(f).is_some_and(|p| p.operation == "input") {
            let prompt = args[0].take().expect("given");
            return (gives.clone(), self.input_line(out, &gives, &prompt));
        }
        let text = args[0].take().expect("given");
        let end = args[1].take().expect("given");
        let write = |out: &mut Out<'a>, stream: form::Stream| {
            for args in [text.clone(), end.clone()] {
                out.emit(
                    form::Op::System {
                        call: form::System::Write(stream),
                        args,
                    },
                    Vec::new(),
                );
            }
        };
        match args[2].take() {
            None => write(out, form::Stream::Stdout),
            Some(to) => {
                let arms = out.switch(to[0], 2);
                let mut ends = Vec::new();
                for (arm, stream) in [(0, form::Stream::Stdout), (1, form::Stream::Stderr)] {
                    out.current = arms[arm];
                    write(out, stream);
                    ends.push((out.current, Vec::new()));
                }
                out.join(&[], ends);
            }
        }
        (Ty::Nothing, Vec::new())
    }

    /// The `Option` that `input` of `console` gives: the runtime writes the
    /// prompt and reads a line, and says whether there was one.
    fn input_line(
        &mut self,
        out: &mut Out<'a>,
        ty: &Ty,
        prompt: &[form::Value],
    ) -> Vec<form::Value> {
        let defs = self.typed.defs;
        let Ty::Enum(e, args) = ty else {
            unreachable!("checked")
        };
        let types = [
            form::Type::Int(form::Bits::B8),
            form::Type::Address,
            form::Type::Int(form::Bits::B64),
        ];
        let line = self.call_runtime(
            out,
            Runtime::Input,
            vec![prompt[0], prompt[1]],
            types.to_vec(),
        );
        let layout = self.layout(*e, args);
        let some = defs.case_of(defs.option, "some").expect("a case");
        let none = defs.case_of(defs.option, "none").expect("a case");
        let (some_tag, none_tag) = (
            out.int(form::Bits::B8, some as i128),
            out.int(form::Bits::B8, none as i128),
        );
        let mut values = vec![out.choose(line[0], some_tag, none_tag)];
        for (k, ty) in layout.places.iter().enumerate() {
            let held = layout.cases[some].iter().position(|place| *place == k);
            values.push(match held {
                Some(i) => line[1 + i],
                None => out.zero(*ty),
            });
        }
        values
    }

    /// The number of the handler a call of an effect's name installs, which is
    /// the same wherever the same call is lowered under the same handlers.
    fn handler_id(&mut self, call: &'a Call, out: &Out<'a>) -> HandlerId {
        let key = (addr(call), out.handlers.iter().map(|h| h.id).collect());
        let next = self.handler_ids.len() as HandlerId;
        *self.handler_ids.entry(key).or_insert(next)
    }

    /// What a handler is made of: the names its functions use, and those that
    /// the functions they hold use in turn, found in the frames of `out`.
    pub(super) fn capture(
        &mut self,
        out: &Out<'a>,
        effect: usize,
        operations: &[Func<'a>],
    ) -> HandlerDef<'a> {
        let mut frames: Vec<(usize, Vec<Captured<'a>>)> = Vec::new();
        let mut work: Vec<(usize, &'a [Stmt])> = Vec::new();
        let reach = |func: &Func<'a>, frames: &mut Vec<(usize, Vec<Captured<'a>>)>| {
            let (frame, body) = match func {
                Func::Lambda(l, frame, _) => (*frame, &l.body[..]),
                Func::Each(l, frame, _) => (*frame, &l.body[..]),
                Func::Named(_) => return None,
            };
            if !frames.iter().any(|(f, _)| *f == frame) {
                frames.push((frame, Vec::new()));
            }
            Some((frame, body))
        };
        for func in operations {
            work.extend(reach(func, &mut frames));
        }
        while let Some((frame, body)) = work.pop() {
            let assigned: Vec<(&str, ())> = (statements(body).into_iter())
                .filter_map(|s| match s {
                    Stmt::Assign { name, .. } | Stmt::Define { name, .. } => {
                        Some((name.as_str(), ()))
                    }
                    _ => None,
                })
                .collect();
            for name in used_names(body) {
                let Some(at) = out.frames[frame].iter().rposition(|b| b.name == name) else {
                    continue;
                };
                let entry = frames.iter_mut().find(|(f, _)| *f == frame).expect("made");
                let writes = assigned.iter().any(|(n, _)| *n == name);
                if let Some(c) = entry.1.iter_mut().find(|c| c.name == name) {
                    c.assigned |= writes && c.func.is_none() && !c.scalars.is_empty();
                    continue;
                }
                let bound = &out.frames[frame][at];
                let scalars = match bound.func {
                    Some(_) => Vec::new(),
                    None => self.scalars(&bound.ty),
                };
                entry.1.push(Captured {
                    name: name.to_string(),
                    ty: bound.ty.clone(),
                    assigned: writes && bound.func.is_none() && !bound.values.is_empty(),
                    scalars,
                    func: bound.func.clone(),
                    depth: 0,
                });
                let held = bound.func.clone();
                if let Some(func) = held {
                    work.extend(reach(&func, &mut frames));
                }
            }
        }
        // The state of a scheduler goes through each poll, and is given back by
        // it whether the functions of the scheduler assign it or not.
        if self.typed.defs.is_scheduler(effect) {
            for (_, names) in &mut frames {
                for c in names.iter_mut() {
                    c.assigned = c.func.is_none() && !c.scalars.is_empty();
                }
            }
        }
        HandlerDef {
            effect,
            operations: operations.to_vec(),
            frames,
        }
    }

    /// The place of the function a call runs, and the values it takes after
    /// its own arguments. A function that carries an effect a handler answers
    /// is built for the handlers it runs under, and takes what the functions of
    /// those handlers use; any other is one copy.
    pub(super) fn copy_for(&mut self, out: &mut Out<'a>, instance: &Instance) -> Prepared {
        let defs = self.typed.defs;
        // The effects a function carries, and those of each function whose name
        // the copy is made for, since it calls them through `Function`.
        let mut named: Vec<usize> = Vec::new();
        fn names(t: &Ty, found: &mut Vec<usize>) {
            if let Ty::Named(g) = t {
                found.push(*g);
            }
            t.args().iter().for_each(|a| names(a, found));
        }
        instance.args.iter().for_each(|t| names(t, &mut named));
        let mut effects: Vec<&String> = match instance.template {
            Template::Free(f) if !defs.is_operation(f) => {
                defs.signatures[f].effects.iter().collect()
            }
            Template::Free(_) => return self.copy_without_handlers(instance),
            _ => Vec::new(),
        };
        for g in &named {
            effects.extend(defs.signatures[*g].effects.iter());
        }
        // The network and the clock suspend a task, and a copy that calls them
        // under a scheduler is given the scheduler's state, as one that calls
        // `spawn` is.
        let suspends = effects.iter().any(|e| of_a_waiting_effect(defs, e));
        let asks_the_scheduler = effects
            .iter()
            .any(|e| defs.effect_named(e).is_some_and(|at| defs.is_scheduler(at)));
        let mut handled: Vec<usize> = effects
            .into_iter()
            .filter_map(|e| {
                // An effect the target answers is handled where a handler is
                // in force for it, and answered by the target where none is.
                let at = defs.effect_named(e)?;
                (defs.handled(e) || out.handlers.iter().any(|h| h.effect == at)).then_some(at)
            })
            .collect();
        let polls = matches!(instance.template, Template::Free(f)
            if !defs.is_operation(f) && self.polls(f, &mut Vec::new()));
        let scheduler = (out.handlers.iter().rev())
            .find(|h| defs.is_scheduler(h.effect))
            .map(|h| h.effect);
        if let (true, Some(scheduler)) = (suspends, scheduler) {
            handled.push(scheduler);
        }
        let may_suspend = scheduler.is_some() && (suspends || asks_the_scheduler) || polls;
        if polls || (suspends && scheduler.is_some()) {
            // A function that waits or polls is given the state of every handler
            // that holds any, as it is given that of the scheduler it waits under.
            for i in 0..out.handlers.len() {
                if out.handlers[i].def.is_none() {
                    let (effect, operations) =
                        (out.handlers[i].effect, out.handlers[i].operations.clone());
                    let def = self.capture(out, effect, &operations);
                    out.handlers[i].def = Some(Rc::new(def));
                }
            }
            for h in &out.handlers {
                if h.def.as_deref().is_some_and(holds_state) {
                    handled.push(h.effect);
                }
            }
        }
        handled.sort_unstable();
        handled.dedup();
        if handled.is_empty() {
            return Prepared {
                function: self.place(Body::Copy(instance.clone())),
                reads: Vec::new(),
                slots: Vec::new(),
                suspends: false,
            };
        }
        let upto = (handled.iter())
            .map(|e| {
                let at = (out.handlers.iter().rposition(|h| h.effect == *e))
                    .expect("an effect that is handled where a function that carries it is called");
                at + 1
            })
            .max()
            .expect("an effect");
        for i in 0..upto {
            if out.handlers[i].def.is_none() {
                let (effect, operations) =
                    (out.handlers[i].effect, out.handlers[i].operations.clone());
                let def = self.capture(out, effect, &operations);
                out.handlers[i].def = Some(Rc::new(def));
            }
        }
        let ids: Vec<HandlerId> = out.handlers[..upto].iter().map(|h| h.id).collect();
        self.contexts.entry(ids.clone()).or_insert_with(|| {
            (out.handlers[..upto].iter())
                .map(|h| (h.id, h.def.clone().expect("made")))
                .collect()
        });
        let mut reads = Vec::new();
        let mut state_reads = Vec::new();
        let mut slots = Vec::new();
        let order = self.state_last(
            out.handlers[..upto]
                .iter()
                .map(|h| (h.effect, h.def.as_deref().is_some_and(holds_state))),
        );
        for at in order {
            let h = &out.handlers[at];
            let def = h.def.as_ref().expect("made");
            for (i, (orig, names)) in def.frames.iter().enumerate() {
                let frame = h.frames.get(i).copied().unwrap_or(*orig);
                for c in names.iter().filter(|c| c.func.is_none()) {
                    let at = out.frames[frame]
                        .iter()
                        .rposition(|b| b.name == c.name)
                        .expect("a name the functions of the handler use");
                    if c.assigned {
                        state_reads.push((frame, at));
                        slots.push(Slot {
                            frame,
                            name: c.name.clone(),
                            types: c.scalars.clone(),
                        });
                    } else {
                        reads.push((frame, at));
                    }
                }
            }
        }
        // What the functions assign is given after what they only read.
        reads.extend(state_reads);
        let function = self.place(Body::Under(instance.clone(), ids));
        Prepared {
            function,
            reads,
            slots,
            suspends: may_suspend,
        }
    }

    /// The function that `Function.call` calls, when the instance is that call
    /// and the function carries an effect that a handler answers.
    pub(super) fn handled_function_call(
        &self,
        out: &Out<'a>,
        instance: &Instance,
    ) -> Option<usize> {
        let defs = self.typed.defs;
        let Template::Given(k, 2) = instance.template else {
            return None;
        };
        if k != defs.function {
            return None;
        }
        let Some(Ty::Named(f)) = instance.args.first() else {
            return None;
        };
        (defs.signatures[*f].effects.iter())
            .any(|e| {
                defs.handled(e)
                    || defs
                        .effect_named(e)
                        .is_some_and(|at| out.handlers.iter().any(|h| h.effect == at))
            })
            .then_some(*f)
    }

    /// One copy of the function, for no handler.
    fn copy_without_handlers(&mut self, instance: &Instance) -> Prepared {
        Prepared {
            function: self.place(Body::Copy(instance.clone())),
            reads: Vec::new(),
            slots: Vec::new(),
            suspends: false,
        }
    }

    /// Calls a copy with its arguments and what it takes besides them, which
    /// gives its results, and sets the names it gives back to the values it
    /// gave.
    pub(super) fn finish_call(
        &mut self,
        out: &mut Out<'a>,
        prepared: Prepared,
        mut args: Vec<form::Value>,
        gives: &Ty,
    ) -> Vec<form::Value> {
        for &(frame, at) in &prepared.reads {
            args.extend(out.frames[frame][at].values.iter().copied());
        }
        let mut types = self.scalars(gives);
        let own = types.len();
        for slot in &prepared.slots {
            types.extend(slot.types.iter().copied());
        }
        let mut results = out.emit(
            form::Op::Call {
                function: prepared.function,
                args,
            },
            types,
        );
        let mut given = results.split_off(own).into_iter();
        for slot in &prepared.slots {
            let at = out.frames[slot.frame]
                .iter()
                .rposition(|b| b.name == slot.name)
                .expect("a name the functions of the handler use");
            out.frames[slot.frame][at].values = given.by_ref().take(slot.types.len()).collect();
        }
        // Where a failure goes up through the call, the function ends what it
        // owns there, as it ends it where the task is asked to end.
        if self.owns_across(out) {
            self.exit_on(out, form::System::Unwinding, true);
        }
        if prepared.suspends {
            self.cancel_exit(out);
        }
        results
    }

    /// Whether the function owns something at the call it is lowering that a
    /// task that is asked to end ends: a child, or a handle.
    fn owns_across(&mut self, out: &mut Out<'a>) -> bool {
        // The calls that end what the function owns, in a landing or in the exit
        // of a task that is asked to end, have no landing of their own, since a
        // failure in one is a second failure.
        if out.returns.last().is_none() || self.asking || self.closing {
            return false;
        }
        let Some(&site) = out.sites.last() else {
            return false;
        };
        let Some(items) = self
            .typed
            .drops
            .at
            .get(&crate::check::ownership::Site::Cancel(site))
        else {
            return false;
        };
        let items = items.clone();
        items.iter().any(|item| {
            let Some(at) = out.find(&item.name) else {
                return false;
            };
            let bound = &out.names()[at];
            let handles = self.holds_handle(&bound.ty, &mut Vec::new());
            !(bound.home.is_some() || (self.holds_none(&bound.ty) && !handles))
                && self.part_of(&bound.ty, &item.part).is_some()
        })
    }

    /// Where the task may be asked to end, after a call it may be suspended in:
    /// when it has been, the function is left at once, with zeros for what it
    /// gives, which nothing reads, since whatever called it takes its own exit
    /// as soon as the call is over. The values the function owns that end by
    /// themselves are ended by their handlers. Inside the block of a loop over
    /// a value no exit is written, as the block does not leave the function.
    pub(super) fn cancel_exit(&mut self, out: &mut Out<'a>) {
        self.exit_on(out, form::System::Cancelled, false);
    }

    /// The exit of `cancel_exit`, taken when what `asked` gives is not 0: where
    /// the task is asked to end, which a call that waits is followed by, and where
    /// a failure goes up through a call, which `landing` says: the landing ends
    /// what the function owns, and goes on up.
    pub(super) fn exit_on(&mut self, out: &mut Out<'a>, asked: form::System, landing: bool) {
        let Some(returns) = out.returns.last() else {
            return;
        };
        let (kept, in_block, gives) = (returns.kept, returns.of.is_some(), returns.gives.clone());
        let asked = out.emit(
            form::Op::System {
                call: asked,
                args: Vec::new(),
            },
            vec![form::Type::Int(form::Bits::B64)],
        )[0];
        let none = out.zero(form::Type::Int(form::Bits::B64));
        let not_asked = out.compare(form::Compare::Equal, asked, none);
        let blocks = out.switch(not_asked, 2);
        out.current = blocks[0];
        // What the function owns that may hold a child is asked to end, as the
        // children of a task that ends are.
        if let Some(&site) = out.sites.last() {
            let before = std::mem::replace(&mut self.asking, true);
            self.drop_at(out, crate::check::ownership::Site::Cancel(site));
            self.asking = before;
        }
        if landing {
            out.emit(
                form::Op::System {
                    call: form::System::Resume,
                    args: Vec::new(),
                },
                Vec::new(),
            );
        }
        if let Some(kept) = kept {
            // In the block of a loop over a value that has a `return`: as that
            // `return` does, the block answers `.stop` and the function gives
            // zeros once `each` is over.
            let types: Vec<form::Type> = out.frames[kept.frame][kept.value]
                .values
                .iter()
                .map(|v| out.types[v.0])
                .collect();
            let zeros: Vec<form::Value> = types.into_iter().map(|t| out.zero(t)).collect();
            let ran = self.truth(out, true);
            out.frames[kept.frame][kept.ran].values = vec![ran];
            out.frames[kept.frame][kept.value].values = zeros;
            let stop = self.flow(out, true);
            self.leave(out, vec![stop]);
        } else if in_block {
            let stop = self.flow(out, true);
            self.leave(out, vec![stop]);
        } else {
            let zeros = self.zeros(out, &gives);
            self.leave(out, zeros);
        }
        // What `leave` makes to follow the exit is reached by nothing, and goes
        // where the call goes on, so that it is a block that is well formed.
        let after = out.current;
        out.blocks[after].end = form::End::Jump(form::Jump {
            block: blocks[1],
            args: Vec::new(),
        });
        out.current = blocks[1];
        if !landing {
            out.exits += 1;
        }
    }

    /// Whether a block calls a function that carries an effect that may suspend
    /// the task: the network, the clock, or the scheduler.
    pub(super) fn may_suspend(&self, body: &[Stmt]) -> bool {
        let defs = self.typed.defs;
        crate::check::exprs(crate::check::values(body))
            .into_iter()
            .filter_map(|e| match e {
                Expr::Call(c) => Some(c),
                _ => None,
            })
            .any(|c| {
                c.qualifier.is_none()
                    && crate::check::frames::kind(c).is_none()
                    && defs.function(c.name_pos, &c.callee).is_some_and(|g| {
                        defs.signatures[g].effects.iter().any(|e| {
                            of_a_waiting_effect(defs, e)
                                || defs.effect_named(e).is_some_and(|at| defs.is_scheduler(at))
                        })
                    })
            })
    }

    /// Lowers a copy of a function for the handlers it runs under: its own
    /// parameters, and then the values the functions of each handler use, which
    /// are in frames made as the ones they were written in.
    pub(super) fn function_under(
        &mut self,
        instance: &Instance,
        ids: &[HandlerId],
    ) -> form::Function {
        let context = self.contexts[ids].clone();
        self.function_with(instance, &context)
    }
}
