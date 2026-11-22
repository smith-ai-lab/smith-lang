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
pub(super) fn poll_any_stub() -> form::Function {
    stub(
        "<poll any>",
        &[form::Type::Address],
        &[form::Type::Int(form::Bits::B8), WORD, WORD],
    )
}

impl<'a> Lowering<'a> {
    /// The values of a type's zero, for a call that is refused.
    fn zeros(&self, out: &mut Out<'a>, ty: &Ty) -> Vec<form::Value> {
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
                out.emit(
                    form::Op::System {
                        call: form::System::Suspend,
                        args: vec![kind, wait[1]],
                    },
                    Vec::new(),
                );
                (Ty::Nothing, Vec::new())
            }
            Kind::Poll => {
                let (_, frame) = self.expr(out, &call.args[0].value, env);
                let function = self.place(Body::PollAny);
                let status = form::Type::Int(form::Bits::B8);
                let given = out.emit(
                    form::Op::Call {
                        function,
                        args: vec![frame[0]],
                    },
                    vec![status, WORD, WORD],
                );
                // `.done` is the first case and `.waiting` the second, and what
                // the second holds is a `Wait`: its case, and the handle.
                let kind = out.narrow(given[1], form::Bits::B8);
                let values = vec![given[0], kind, given[2]];
                debug_assert_eq!(self.scalars(&ty), vec![status, status, WORD]);
                (ty, values)
            }
            Kind::Result => {
                let (_, frame) = self.expr(out, &call.args[0].value, env);
                let types = self.scalars(&ty);
                let (places, _) = form::result_places(&types);
                let mut values = Vec::new();
                for (t, at) in types.into_iter().zip(places) {
                    let address = out.moved(frame[0], at);
                    values.push(out.emit(form::Op::Load { ty: t, address }, vec![t])[0]);
                }
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
        let own = self.capture(out, 0, &[Func::Lambda(lambda, at, lambda_env.clone())]);
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
        let holds_state = (out.handlers.iter())
            .filter_map(|h| h.def.as_ref())
            .any(|d| {
                d.frames
                    .iter()
                    .any(|(_, names)| names.iter().any(|c| c.assigned))
            });
        if holds_state {
            self.errors.push(Error::unsupported(
                call.pos,
                "a lambda made a frame under a handler that holds state,",
            ));
            return (frame.clone(), self.zeros(out, &frame));
        }
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
        for h in &out.handlers {
            let def = h.def.clone().expect("made");
            for (i, (orig, names)) in def.frames.iter().enumerate() {
                let frame = h.frames.get(i).copied().unwrap_or(*orig);
                for c in names.iter().filter(|c| c.func.is_none()) {
                    let at = (out.frames[frame].iter())
                        .rposition(|b| b.name == c.name)
                        .expect("a name the functions of the handler use");
                    args.extend(out.frames[frame][at].values.iter().copied());
                }
            }
            context.push((h.id, def));
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
        }));
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
        // What the lambda assigns is refused where it is checked, so nothing
        // of it is given back.
        let mut slots = Vec::new();
        let map = self.push_frames(&mut out, &def.own, &mut slots);
        out.at = (map.iter())
            .find(|(from, _)| *from == def.frame)
            .expect("the frame the lambda is written in")
            .1;
        for (id, handler) in &def.context {
            let below = out.handlers.len();
            let map = self.push_frames(&mut out, handler, &mut Vec::new());
            out.handlers.push(Handler {
                id: *id,
                effect: handler.effect,
                operations: handler.operations.iter().map(|f| moved(f, &map)).collect(),
                def: Some(handler.clone()),
                frames: map.iter().map(|(_, frame)| *frame).collect(),
                below,
            });
        }
        let lambda: &'a Lambda = def.lambda;
        let code = Code::Stmts(&lambda.body);
        let mut result = self.leaving(&mut out, code, &def.env, (&def.gives, true), Vec::new());
        if def.gives == Ty::Nothing {
            result.clear();
        }
        out.blocks[out.current].end = form::End::Return(result);
        form::Function {
            types: out.types,
            results: self.scalars(&def.gives),
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
