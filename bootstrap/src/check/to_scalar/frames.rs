//! Lowers the calls the library's scheduler is written with.
//!
//! A lambda given to `__frame` runs after the call that made it has moved on, so
//! it cannot be lowered in place, as every other lambda is. It is lowered as a
//! function of its own. Each name of the function around it that it reads, and
//! each name that the functions of the handlers in force use, is found by looking
//! at its body, and what the name stands for is a parameter of the new function:
//! the frame holds those values from the start and the function reads them from
//! it. A function that the lowering placed for the pass `suspend` makes the
//! frame: it asks for the bytes the frame needs, which only that pass knows, and
//! starts the function in them.

use super::functions_calls_and_lambdas::moved;
use super::*;
use crate::check::frames::Kind;

/// A lambda that was made a frame: what its function needs of where it was
/// written.
pub(super) struct LambdaDef<'a> {
    pub lambda: &'a Lambda,
    /// The types that stand for the type parameters of the definition it is
    /// written in, and the type it gives.
    pub env: Vec<Ty>,
    pub gives: Ty,
    /// The frame of the function around it that it is written in.
    pub frame: usize,
    /// The names of the function around it that it reads, by frame.
    pub own: HandlerDef<'a>,
    /// The handlers in force where it is written.
    pub context: Vec<(HandlerId, Rc<HandlerDef<'a>>)>,
    /// What the function that makes its frame takes: the scalars of those names.
    pub params: Vec<form::Type>,
    /// The names of the function around it that it assigns, which it gives back
    /// after what it gives, and which the function reads the frame in gets back
    /// when the frame is read.
    pub assigned: Vec<Slot>,
}

const WORD: form::Type = form::Type::Int(form::Bits::B64);

/// A function that is well formed and does nothing, which the pass `suspend`
/// puts the function it writes in the place of.
fn stub(name: &str, params: &[form::Type], results: &[form::Type]) -> form::Function {
    let mut types = params.to_vec();
    let mut instructions = Vec::new();
    let mut gives = Vec::new();
    for ty in results {
        types.push(*ty);
        let value = form::Value(types.len() - 1);
        gives.push(value);
        let op = match ty {
            form::Type::Address => form::Op::Constant(form::Constant::Null),
            _ => form::Op::Constant(form::Constant::Int(0)),
        };
        instructions.push(form::Instruction {
            results: vec![value],
            op,
            at: form::Place::NONE,
        });
    }
    form::Function {
        types,
        results: results.to_vec(),
        blocks: vec![form::Block {
            params: (0..params.len()).map(form::Value).collect(),
            instructions,
            end: form::End::Return(gives),
        }],
        cold: false,
        name: name.to_string(),
    }
    .named(name)
}

/// The function that polls any frame, as the lowering holds it until `suspend`
/// writes it: it takes the address of the frame and gives the status, and the
/// kind and handle of the wait.
pub(super) fn poll_any_stub(through: &[form::Type]) -> form::Function {
    let params: Vec<form::Type> = std::iter::once(form::Type::Address)
        .chain(through.iter().copied())
        .collect();
    let mut results = through.to_vec();
    results.extend([form::Type::Int(form::Bits::B8), WORD, WORD]);
    stub("<poll any>", &params, &results)
}

/// Whether the functions of a handler assign a name of the function around it,
/// so that the names it uses are state that goes through each poll.
pub(super) fn holds_state(def: &HandlerDef) -> bool {
    def.frames
        .iter()
        .any(|(_, names)| names.iter().any(|c| c.assigned))
}

impl<'a> Lowering<'a> {
    /// The names that are the state of the scheduler at the code being lowered:
    /// those of the nearest handler of `task` that holds state, among the
    /// handlers whose operation is being lowered and those in force, which is
    /// given to each poll and given back by it, and which no frame saves.
    pub(super) fn scheduler_state(&mut self, out: &mut Out<'a>) -> Vec<Slot> {
        let defs = self.typed.defs;
        let mut found: Vec<(Rc<HandlerDef<'a>>, Vec<usize>)> = (out.operating.iter().rev())
            .filter(|o| defs.is_scheduler(o.def.effect))
            .map(|o| (o.def.clone(), o.frames.clone()))
            .collect();
        for i in (0..out.handlers.len()).rev() {
            if !defs.is_scheduler(out.handlers[i].effect) {
                continue;
            }
            if out.handlers[i].def.is_none() {
                let (effect, operations) =
                    (out.handlers[i].effect, out.handlers[i].operations.clone());
                let def = self.capture(out, effect, &operations);
                out.handlers[i].def = Some(Rc::new(def));
            }
            found.push((
                out.handlers[i].def.clone().expect("made"),
                out.handlers[i].frames.clone(),
            ));
        }
        // The names that other handlers assign go through each poll too, before
        // those of the scheduler, in the order the handlers were installed.
        let mut others: Vec<(Rc<HandlerDef<'a>>, Vec<usize>)> = Vec::new();
        for i in 0..out.handlers.len() {
            if defs.is_scheduler(out.handlers[i].effect) {
                continue;
            }
            if out.handlers[i].def.is_none() {
                let (effect, operations) =
                    (out.handlers[i].effect, out.handlers[i].operations.clone());
                let def = self.capture(out, effect, &operations);
                out.handlers[i].def = Some(Rc::new(def));
            }
            let def = out.handlers[i].def.clone().expect("made");
            if holds_state(&def) {
                others.push((def, out.handlers[i].frames.clone()));
            }
        }
        let mut slots = Vec::new();
        let state_of = |def: &HandlerDef<'a>, frames: &[usize], slots: &mut Vec<Slot>| {
            for (i, (orig, names)) in def.frames.iter().enumerate() {
                let frame = frames.get(i).copied().unwrap_or(*orig);
                for c in names.iter().filter(|c| c.assigned) {
                    slots.push(Slot {
                        frame,
                        name: c.name.clone(),
                        types: c.scalars.clone(),
                    });
                }
            }
        };
        for (def, frames) in &others {
            state_of(def, frames, &mut slots);
        }
        // The state that goes through the polls is that of the code a scheduler
        // is in force for. Code that no scheduler is in force for, a function
        // that carries only the effect of a handler that holds state, is given
        // the state of that handler and no more, and is no other state.
        let scheduled = !found.is_empty();
        for (def, frames) in found {
            let before = slots.len();
            state_of(&def, &frames, &mut slots);
            if slots.len() > before {
                break;
            }
        }
        let types: Vec<form::Type> = slots.iter().flat_map(|s| s.types.iter().copied()).collect();
        if !types.is_empty() {
            match &self.through {
                None => {
                    self.through = Some(types);
                    self.through_scheduled = scheduled;
                }
                Some(known) if *known == types => self.through_scheduled |= scheduled,
                // What an earlier place gave with no scheduler in force is the
                // state of the handlers there, which a scheduler's is added to.
                Some(_) if scheduled && !self.through_scheduled => {
                    self.through = Some(types);
                    self.through_scheduled = true;
                }
                Some(_) if scheduled => {
                    self.errors.push(Error::unsupported(
                        Pos { line: 1, col: 1 },
                        "two schedulers that hold different state in one program,",
                    ));
                }
                Some(_) => {}
            }
        }
        slots
    }

    /// Whether a function polls a frame, itself or by a function it calls, so
    /// that it must be given the state that goes through each poll.
    pub(super) fn polls(&mut self, f: usize, seen: &mut Vec<usize>) -> bool {
        if let Some(&known) = self.polling.get(&f) {
            return known;
        }
        if seen.contains(&f) {
            return false;
        }
        seen.push(f);
        let defs = self.typed.defs;
        let view = defs.view(Template::Free(f));
        let calls: Vec<&Call> = crate::check::exprs(crate::check::values(&view.f.body))
            .into_iter()
            .filter_map(|e| match e {
                Expr::Call(c) => Some(c),
                _ => None,
            })
            .collect();
        let mut found = false;
        for c in calls {
            if crate::check::frames::kind(c) == Some(crate::check::frames::Kind::Poll) {
                found = true;
                break;
            }
            if c.qualifier.is_none()
                && crate::check::frames::kind(c).is_none()
                && let Some(g) = defs.function(c.name_pos, &c.callee)
                && !defs.is_operation(g)
                && self.polls(g, seen)
            {
                found = true;
                break;
            }
        }
        seen.pop();
        self.polling.insert(f, found);
        found
    }

    /// The order in which the handlers of a copy or a frame give and take what
    /// their functions use: those that hold state last, which a scheduler does,
    /// and the rest as they are. Each is its effect and whether it holds state.
    pub(super) fn state_last(&self, handlers: impl Iterator<Item = (usize, bool)>) -> Vec<usize> {
        let handlers: Vec<(usize, bool)> = handlers.collect();
        let defs = self.typed.defs;
        let scheduler = |i: &usize| defs.is_scheduler(handlers[*i].0);
        let (state, rest): (Vec<usize>, Vec<usize>) =
            (0..handlers.len()).partition(|i| handlers[*i].1 || scheduler(i));
        // The state of a scheduler is the very last, which `suspend` finds.
        let (scheduled, others): (Vec<usize>, Vec<usize>) = state.into_iter().partition(scheduler);
        rest.into_iter().chain(others).chain(scheduled).collect()
    }

    /// The values of the names of a state, in order.
    pub(super) fn state_values(&self, out: &Out<'a>, state: &[Slot]) -> Vec<form::Value> {
        let mut values = Vec::new();
        for slot in state {
            let at = (out.frames[slot.frame].iter())
                .rposition(|b| b.name == slot.name)
                .expect("a name of the scheduler's state");
            values.extend(out.frames[slot.frame][at].values.iter().copied());
        }
        values
    }

    /// Sets the names of a state to the values a call gave of it.
    pub(super) fn set_state(&self, out: &mut Out<'a>, state: &[Slot], given: Vec<form::Value>) {
        let mut given = given.into_iter();
        for slot in state {
            let at = (out.frames[slot.frame].iter())
                .rposition(|b| b.name == slot.name)
                .expect("a name of the scheduler's state");
            out.frames[slot.frame][at].values = given.by_ref().take(slot.types.len()).collect();
        }
    }

    /// The values of a type's zero, for a call that is refused.
    pub(super) fn zeros(&self, out: &mut Out<'a>, ty: &Ty) -> Vec<form::Value> {
        self.scalars(ty).into_iter().map(|t| out.zero(t)).collect()
    }

    pub(super) fn frames_call(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        kind: Kind,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let ty = self.typed.type_of(expr, env);
        match kind {
            Kind::Frame => self.make_frame(out, expr, call, env),
            Kind::Wait => {
                // The kind of the wait is the number of the case, and the handle
                // is what every case holds, a number of 64 bits.
                let (_, wait) = self.expr(out, &call.args[0].value, env);
                debug_assert_eq!(wait.len(), 2, "a `Wait` is a case and a number");
                let kind = out.widen(false, wait[0]);
                // The state of the scheduler goes out with the wait, and what
                // the task goes on with is what the poll that resumes it is
                // given.
                let state = self.scheduler_state(out);
                let mut args = vec![kind, wait[1]];
                args.extend(self.state_values(out, &state));
                let types: Vec<form::Type> =
                    state.iter().flat_map(|s| s.types.iter().copied()).collect();
                let after = out.emit(
                    form::Op::System {
                        call: form::System::Suspend,
                        args,
                    },
                    types,
                );
                self.set_state(out, &state, after);
                (Ty::Nothing, Vec::new())
            }
            Kind::Poll => {
                let (_, frame) = self.expr(out, &call.args[0].value, env);
                let function = self.place(Body::PollAny);
                let status = form::Type::Int(form::Bits::B8);
                // The state of the scheduler goes in with the frame's address,
                // and comes back first.
                let state = self.scheduler_state(out);
                let mut args = vec![frame[0]];
                args.extend(self.state_values(out, &state));
                let mut types: Vec<form::Type> =
                    state.iter().flat_map(|s| s.types.iter().copied()).collect();
                types.extend([status, WORD, WORD]);
                let mut given = out.emit(form::Op::Call { function, args }, types);
                let status_values = given.split_off(given.len() - 3);
                self.set_state(out, &state, given);
                let given = status_values;
                // `.done` is the first case and `.waiting` the second, and what
                // the second holds is a `Wait`: its case, and the handle.
                let kind = out.narrow(given[1], form::Bits::B8);
                let values = vec![given[0], kind, given[2]];
                debug_assert_eq!(self.scalars(&ty), vec![status, status, WORD]);
                (ty, values)
            }
            Kind::Drop => {
                // The frame of a child that has ended and is not joined: its
                // stack and its frame go back to the pool, and what it gave is
                // not read.
                let (_, frame) = self.expr(out, &call.args[0].value, env);
                out.emit(
                    form::Op::System {
                        call: form::System::TaskFree,
                        args: vec![frame[0]],
                    },
                    Vec::new(),
                );
                (Ty::Nothing, Vec::new())
            }
            Kind::Scheduled => {
                let state = self.scheduler_state(out);
                (ty, vec![self.truth(out, !state.is_empty())])
            }
            Kind::Address => {
                // The number of a frame is where its bytes are, kept by storing
                // the address in a place of the function and reading it back as
                // a whole number, as the form has no instruction between them.
                let (_, frame) = self.expr(out, &call.args[0].value, env);
                let slot = out.emit(form::Op::Frame { bytes: 8 }, vec![form::Type::Address])[0];
                out.emit(
                    form::Op::Store {
                        ty: form::Type::Address,
                        address: slot,
                        value: frame[0],
                    },
                    Vec::new(),
                );
                let number = out.emit(
                    form::Op::Load {
                        ty: WORD,
                        address: slot,
                    },
                    vec![WORD],
                )[0];
                (ty, vec![number])
            }
            Kind::Word | Kind::SetWord => {
                // The address of a word of the header of the frame at a number,
                // found as `At` finds the frame.
                let (_, number) = self.expr(out, &call.args[0].value, env);
                let (_, which) = self.expr(out, &call.args[1].value, env);
                let slot = out.emit(form::Op::Frame { bytes: 8 }, vec![form::Type::Address])[0];
                out.emit(
                    form::Op::Store {
                        ty: WORD,
                        address: slot,
                        value: number[0],
                    },
                    Vec::new(),
                );
                let frame = out.emit(
                    form::Op::Load {
                        ty: form::Type::Address,
                        address: slot,
                    },
                    vec![form::Type::Address],
                )[0];
                let eight = out.int(form::Bits::B64, 8);
                let by = out.binary(form::Binary::Mul, which[0], eight);
                let first = out.int(form::Bits::B64, form::SCHEDULER_AT as i128);
                let from = out.binary(form::Binary::Add, by, first);
                let address = out.binary(form::Binary::Add, frame, from);
                if kind == Kind::Word {
                    let word = out.emit(form::Op::Load { ty: WORD, address }, vec![WORD])[0];
                    (ty, vec![word])
                } else {
                    let (_, value) = self.expr(out, &call.args[2].value, env);
                    out.emit(
                        form::Op::Store {
                            ty: WORD,
                            address,
                            value: value[0],
                        },
                        Vec::new(),
                    );
                    (Ty::Nothing, Vec::new())
                }
            }
            Kind::At => {
                // The frame at a number, whose size nothing reads again.
                let (_, number) = self.expr(out, &call.args[0].value, env);
                let slot = out.emit(form::Op::Frame { bytes: 8 }, vec![form::Type::Address])[0];
                out.emit(
                    form::Op::Store {
                        ty: WORD,
                        address: slot,
                        value: number[0],
                    },
                    Vec::new(),
                );
                let address = out.emit(
                    form::Op::Load {
                        ty: form::Type::Address,
                        address: slot,
                    },
                    vec![form::Type::Address],
                )[0];
                let bytes = out.int(form::Bits::B64, 0);
                (ty, vec![address, bytes])
            }
            Kind::Result => {
                let (_, frame) = self.expr(out, &call.args[0].value, env);
                let mut types = self.scalars(&ty);
                let own = types.len();
                // The frame of a block that assigns names of its function gives
                // them back after its value, and the function goes on with them.
                let given = match &call.args[0].value {
                    Expr::Name { name, .. } => (out.roots.iter().rev())
                        .find(|(n, _)| n == name)
                        .map(|(_, k)| self.lambdas[*k].assigned.clone())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                types.extend(given.iter().flat_map(|s| s.types.iter().copied()));
                let (places, _) = form::result_places(&types);
                let mut values = Vec::new();
                for (t, at) in types.into_iter().zip(places) {
                    let address = out.moved(frame[0], at);
                    values.push(out.emit(form::Op::Load { ty: t, address }, vec![t])[0]);
                }
                let back = values.split_off(own);
                self.set_state(out, &given, back);
                // A task that gave no address has built nothing that its result
                // holds, so its pieces go back to the pool once it is read. What
                // holds an address is built in them, so they go to the one that
                // reads it, with no byte moved.
                let call = if self.scalars(&ty).contains(&form::Type::Address) {
                    form::System::TaskMove
                } else {
                    form::System::TaskFree
                };
                out.emit(
                    form::Op::System {
                        call,
                        args: vec![frame[0]],
                    },
                    Vec::new(),
                );
                (ty, values)
            }
        }
    }

    /// Lowers `__frame(work)`: the lambda becomes a function of its own, and the
    /// call makes the frame that runs it, given the values of the names it reads.
    fn make_frame(
        &mut self,
        out: &mut Out<'a>,
        expr: &'a Expr,
        call: &'a Call,
        env: &[Ty],
    ) -> (Ty, Vec<form::Value>) {
        let frame = self.typed.type_of(expr, env);
        let arg = &call.args[0].value;
        let param = (String::new(), Ty::Fn(Vec::new()));
        let func = self.argument(out, arg, &param, env).func;
        let Some(Func::Lambda(lambda, at, lambda_env)) = func else {
            self.errors.push(Error::unsupported(
                call.pos,
                "a frame of a function that is not a lambda",
            ));
            return (frame.clone(), self.zeros(out, &frame));
        };
        let Ty::Fn(items) = self.typed.type_of(arg, env) else {
            unreachable!("checked")
        };
        let gives = items.last().expect("a result").clone();
        let own = self.capture(
            out,
            usize::MAX,
            &[Func::Lambda(lambda, at, lambda_env.clone())],
        );
        // What the function reads is passed by value, so what it assigns of the
        // function around it would be lost: a name it assigns is refused.
        let mut assigned = Vec::new();
        for (orig, names) in &own.frames {
            for c in names.iter().filter(|c| c.assigned) {
                assigned.push(Slot {
                    frame: *orig,
                    name: c.name.clone(),
                    types: c.scalars.clone(),
                });
            }
        }
        // A block that a group is made of assigns them, and the frame of one is
        // read where the block ends. A lambda made a frame in any other place
        // runs when the function around it has gone on, so what it assigns would
        // be lost.
        if !self.defining_frame {
            for (_, names) in &own.frames {
                if let Some(c) = names.iter().find(|c| c.assigned) {
                    self.errors.push(Error::unsupported(
                        call.pos,
                        &format!(
                            "a lambda made a frame that assigns `{}`, a name of the function around it,",
                            c.name
                        ),
                    ));
                    return (frame.clone(), self.zeros(out, &frame));
                }
            }
        }
        // What the function reads is passed by value, so a name kept in the
        // bytes of the function around it cannot be read from another.
        for (orig, names) in &own.frames {
            for c in names.iter().filter(|c| c.func.is_none()) {
                let at = out.frames[*orig].iter().rposition(|b| b.name == c.name);
                if at.is_some_and(|at| out.frames[*orig][at].home.is_some()) {
                    self.errors.push(Error::unsupported(
                        call.pos,
                        "a lambda made a frame that reads an array indexed where it is not known when the program is built,",
                    ));
                    return (frame.clone(), self.zeros(out, &frame));
                }
            }
        }
        for i in 0..out.handlers.len() {
            if out.handlers[i].def.is_none() {
                let (effect, operations) =
                    (out.handlers[i].effect, out.handlers[i].operations.clone());
                let def = self.capture(out, effect, &operations);
                out.handlers[i].def = Some(Rc::new(def));
            }
        }
        // A handler that holds state and is installed under the scheduler that
        // runs the child is one the scheduler's own code does not run under, so
        // the poll it makes is not given that state.
        let first_scheduler = out
            .handlers
            .iter()
            .position(|h| self.typed.defs.is_scheduler(h.effect));
        if let Some(scheduler) = first_scheduler
            && out.handlers[scheduler + 1..]
                .iter()
                .any(|h| h.def.as_deref().is_some_and(holds_state))
        {
            self.errors.push(Error::unsupported(
                call.pos,
                "a lambda made a frame under a handler that holds state and is installed under its scheduler,",
            ));
            return (frame.clone(), self.zeros(out, &frame));
        }
        // The state that goes through the polls is the same in each place that
        // asks for it.
        let _ = self.scheduler_state(out);
        let mut args = Vec::new();
        for (orig, names) in &own.frames {
            for c in names.iter().filter(|c| c.func.is_none()) {
                let at = (out.frames[*orig].iter())
                    .rposition(|b| b.name == c.name)
                    .expect("a name the lambda reads");
                args.extend(out.frames[*orig][at].values.iter().copied());
            }
        }
        let mut context = Vec::new();
        // The names the functions only read are given in the order the function
        // the lambda becomes takes them, which has those of a handler that holds
        // state after the others.
        let order = self.state_last(
            out.handlers
                .iter()
                .map(|h| (h.effect, h.def.as_deref().is_some_and(holds_state))),
        );
        for at in order {
            let h = &out.handlers[at];
            let def = h.def.clone().expect("made");
            for (i, (orig, names)) in def.frames.iter().enumerate() {
                let frame = h.frames.get(i).copied().unwrap_or(*orig);
                for c in names.iter().filter(|c| c.func.is_none() && !c.assigned) {
                    let at = (out.frames[frame].iter())
                        .rposition(|b| b.name == c.name)
                        .expect("a name the functions of the handler use");
                    args.extend(out.frames[frame][at].values.iter().copied());
                }
            }
        }
        for h in &out.handlers {
            context.push((h.id, h.def.clone().expect("made")));
        }
        let params: Vec<form::Type> = args.iter().map(|v| out.types[v.0]).collect();
        let k = self.lambdas.len();
        self.lambdas.push(Rc::new(LambdaDef {
            lambda,
            env: lambda_env,
            gives,
            frame: at,
            own,
            context,
            params,
            assigned,
        }));
        self.made_frame = Some(k);
        let function = self.add(Body::Lambda(k));
        let place = self.add(Body::Starter(k));
        self.starters.push(form::Starter { place, function });
        let results = out.emit(
            form::Op::Call {
                function: place,
                args,
            },
            vec![form::Type::Address, WORD],
        );
        (frame, results)
    }

    /// The function a lambda that was made a frame becomes: it takes the values
    /// of the names the lambda reads, and those the functions of the handlers in
    /// force where it was written use, and gives what the lambda gives.
    pub(super) fn lambda_function(&mut self, k: usize) -> form::Function {
        let def = self.lambdas[k].clone();
        let name = format!("<frame {k}>");
        let mut out = Out::new();
        out.written = form::names::intern(&name);
        let mut homed = HashSet::new();
        indexed_names(&def.lambda.body, &mut homed);
        out.homed.insert(out.written, homed);
        out.place = form::Place::of(&name);
        let mut own_slots = Vec::new();
        let map = self.push_frames(&mut out, &def.own, &mut own_slots, None);
        out.at = (map.iter())
            .find(|(from, _)| *from == def.frame)
            .expect("the frame the lambda is written in")
            .1;
        // The values of a handler's names are parameters, in the order of the
        // handlers, except that those of a scheduler come last, after what the
        // lambda takes, and are given back last, so that `suspend` finds them
        // after what the function takes and gives of its own.
        let order = self.state_last(def.context.iter().map(|(_, h)| (h.effect, holds_state(h))));
        let mut maps: Vec<Vec<(usize, usize)>> = vec![Vec::new(); def.context.len()];
        let mut state: Vec<Slot> = Vec::new();
        let mut later = Vec::new();
        for i in order {
            let handler = &def.context[i].1;
            let mut slots = Vec::new();
            maps[i] = self.push_frames(&mut out, handler, &mut slots, Some(&mut later));
            if self.typed.defs.is_scheduler(handler.effect) || holds_state(handler) {
                state.extend(slots);
            }
        }
        super::functions_calls_and_lambdas::finish_params(self, &mut out, later);
        for ((id, handler), map) in def.context.iter().zip(maps) {
            let below = out.handlers.len();
            out.handlers.push(Handler {
                id: *id,
                effect: handler.effect,
                operations: handler.operations.iter().map(|f| moved(f, &map)).collect(),
                def: Some(handler.clone()),
                frames: map.iter().map(|(_, frame)| *frame).collect(),
                below,
            });
        }
        // What it gives back is what it assigns of the function around it, and
        // then the scheduler's state.
        out.give_back = own_slots.iter().chain(&state).cloned().collect();
        let lambda: &'a Lambda = def.lambda;
        let code = Code::Stmts(&lambda.body);
        let limits: Vec<usize> = match out.give_back.is_empty() {
            true => Vec::new(),
            false => out.frames.iter().map(Vec::len).collect(),
        };
        let mut result = self.leaving(&mut out, code, &def.env, (&def.gives, true), limits);
        if def.gives == Ty::Nothing {
            result.clear();
        }
        let mut results = self.scalars(&def.gives);
        // What it assigns of the function around it is given back after what it
        // gives, and then the scheduler's state.
        result.extend(self.state_values(&out, &own_slots));
        results.extend(own_slots.iter().flat_map(|s| s.types.iter().copied()));
        // What the scheduler's state is at the end is given back.
        result.extend(self.state_values(&out, &state));
        results.extend(state.iter().flat_map(|s| s.types.iter().copied()));
        out.blocks[out.current].end = form::End::Return(result);
        form::Function {
            types: out.types,
            results,
            blocks: out.blocks,
            cold: false,
            name,
        }
    }

    /// The function that makes the frame of a lambda, as the lowering holds it
    /// until `suspend` writes it: it takes the values the lambda's function takes
    /// and gives the address of the frame and how many bytes it has.
    pub(super) fn starter_function(&mut self, k: usize) -> form::Function {
        stub(
            &format!("<frame {k} start>"),
            &self.lambdas[k].params,
            &[form::Type::Address, WORD],
        )
    }
}
