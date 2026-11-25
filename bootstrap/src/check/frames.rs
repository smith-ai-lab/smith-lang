//! Checks the four calls the library's scheduler is written with, which only
//! `std/` may call: `__frame` makes a lambda a `Frame`, `__poll` runs a frame to
//! its next wait or its end, `__result` reads what a finished frame gave, and
//! `__wait` suspends the task that runs it. A program never sees one: a call
//! outside the library is a call of a function that is not there.

use super::effects_with_and_try::arena_signature;
use super::*;

/// Which of the four calls a call is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Frame,
    Poll,
    Result,
    Wait,
    Address,
    At,
    Scheduled,
    Word,
    SetWord,
    Drop,
    Fail,
}

/// Which call a call is, when it is one that only the library may make.
pub(super) fn kind(call: &Call) -> Option<Kind> {
    if call.qualifier.is_some() || !of_std(call.pos) {
        return None;
    }
    match call.callee.as_str() {
        "__frame" => Some(Kind::Frame),
        "__poll" => Some(Kind::Poll),
        "__result" => Some(Kind::Result),
        "__wait" => Some(Kind::Wait),
        "__address" => Some(Kind::Address),
        "__frame_at" => Some(Kind::At),
        "__scheduled" => Some(Kind::Scheduled),
        "__word" => Some(Kind::Word),
        "__set_word" => Some(Kind::SetWord),
        "__drop" => Some(Kind::Drop),
        "__fail" => Some(Kind::Fail),
        _ => None,
    }
}

/// A call of `__frame`, whose lambda is given to the frame and runs after the
/// call that made it has moved on, so what it gives away moves into the frame.
pub(super) fn is_frame_call(call: &Call) -> bool {
    kind(call) == Some(Kind::Frame)
}

/// A call of `__result`, which takes the frame it reads, or of `__drop`, which
/// takes the frame it gives back.
pub(super) fn takes_frame(call: &Call) -> bool {
    matches!(kind(call), Some(Kind::Result | Kind::Drop))
}

impl<'a> Checker<'a> {
    /// Checks a call of one of the four. Each is given what `std.task` defines,
    /// so a program that does not have that module cannot call them.
    pub(super) fn frames(
        &mut self,
        body: &mut Body<'a>,
        call: &Call,
        kind: Kind,
        want: &Want,
    ) -> Ty {
        let defs = self.defs;
        if kind == Kind::Scheduled {
            // Whether a scheduler that holds state is in force where the call is
            // written, which is known when the copy is built, so an `if` on it
            // is lowered as the one block that holds.
            return self.defs.bool_ty();
        }
        let (Some(frame), Some(poll), Some(wait)) = (defs.frame, defs.poll, defs.wait) else {
            self.errors.push(Error::new(
                call.pos,
                format!(
                    "`{}` needs `std.task`, which this program does not use",
                    call.callee
                ),
            ));
            return self.unknown_call(body, call);
        };
        let frame = Ty::Struct(frame, Vec::new());
        let one = |name: &str, ty: Ty, gives: Ty| Signature {
            written: Vec::new(),
            scope: Scope::default(),
            params: vec![(name.to_string(), ty)],
            defaults: vec![false],
            param_defaults: Vec::new(),
            gives,
            gives_param: None,
            effects: Vec::new(),
            fn_effects: vec![Vec::new()],
        };
        match kind {
            Kind::Frame => {
                // The lambda may carry the effects of the function it is
                // written in and no others, as the work of `spawn` does.
                let signature = Signature {
                    written: Vec::new(),
                    params: vec![("work".to_string(), Ty::Fn(vec![Ty::Param(0)]))],
                    gives: frame,
                    fn_effects: vec![Vec::new()],
                    ..arena_signature()
                };
                let sol = self.arguments(body, call, &signature, false, None, None);
                if let Some(Some(gives)) = sol.first() {
                    self.frame_gives.insert(addr(call), gives.clone());
                }
                self.child_assigns(body, call);
                signature.gives.solved(&sol)
            }
            Kind::Poll => {
                let signature = one("frame", frame, Ty::Enum(poll, Vec::new()));
                self.arguments(body, call, &signature, false, None, None);
                signature.gives
            }
            Kind::Address => {
                let signature = one("frame", frame, Ty::Int(Int::U64));
                self.arguments(body, call, &signature, false, None, None);
                signature.gives
            }
            Kind::At => {
                let signature = one("number", Ty::Int(Int::U64), frame);
                self.arguments(body, call, &signature, false, None, None);
                signature.gives
            }
            Kind::Word | Kind::SetWord => {
                // A word of the header of the frame of the task with the number,
                // which a scheduler keeps its queues in, so that it holds no
                // memory of its own that a task could free.
                let number = ("number".to_string(), Ty::Int(Int::U64));
                let which = ("which".to_string(), Ty::Int(Int::U64));
                let mut params = vec![number, which];
                let gives = if kind == Kind::Word {
                    Ty::Int(Int::U64)
                } else {
                    params.push(("value".to_string(), Ty::Int(Int::U64)));
                    Ty::Nothing
                };
                let signature = Signature {
                    written: Vec::new(),
                    scope: Scope::default(),
                    defaults: vec![false; params.len()],
                    fn_effects: vec![Vec::new(); params.len()],
                    params,
                    param_defaults: Vec::new(),
                    gives: gives.clone(),
                    gives_param: None,
                    effects: Vec::new(),
                };
                self.arguments(body, call, &signature, false, None, None);
                gives
            }
            Kind::Scheduled => unreachable!("answered before the frame's types are read"),
            Kind::Drop => {
                // A frame whose result nothing reads, given back whole.
                let signature = one("frame", frame, Ty::Nothing);
                self.arguments(body, call, &signature, false, None, None);
                Ty::Nothing
            }
            Kind::Fail => {
                // Ends the task that runs it by a failure, as running out of
                // memory does, which the number says.
                let signature = one("failure", Ty::Int(Int::U64), Ty::Nothing);
                self.arguments(body, call, &signature, false, None, None);
                // It gives whatever its place asks for, since nothing follows it.
                match want {
                    Want::Ty(t) if !t.unknown() => t.clone(),
                    _ => Ty::Nothing,
                }
            }
            Kind::Wait => {
                let signature = one("wait", Ty::Enum(wait, Vec::new()), Ty::Nothing);
                self.arguments(body, call, &signature, false, None, None);
                Ty::Nothing
            }
            Kind::Result => {
                // The type a frame is read at is the type of the place the
                // value goes to, as the `T` of `join` is.
                let gives = match want {
                    Want::Ty(t) if !t.unknown() => t.clone(),
                    _ => {
                        self.errors.push(Error::new(
                            call.pos,
                            "`__result` gives the type of the place its value goes to, and none is known here",
                        ));
                        return self.unknown_call(body, call);
                    }
                };
                let signature = one("frame", frame, gives.clone());
                self.arguments(body, call, &signature, false, None, None);
                self.read_at_its_type(body, call, &gives);
                gives
            }
        }
    }

    /// Refuses a frame read at a type its lambda does not give. The frame is
    /// known by where it is made: the call of `__frame` that is the argument, or
    /// the one that the name it is read through was made by.
    fn read_at_its_type(&mut self, body: &Body<'a>, call: &Call, read: &Ty) {
        let Some(arg) = call.args.first() else {
            return;
        };
        let made: Option<&Call> = match &arg.value {
            Expr::Call(c) if is_frame_call(c) => Some(c),
            Expr::Name { name, .. } => body.f.and_then(|f| made_by(&f.body, name)),
            _ => None,
        };
        let text = |t: &Ty| self.defs.text(t, body.scope);
        let gives = made
            .and_then(|c| self.frame_gives.get(&addr(c)))
            .or_else(|| self.of_a_task(arg))
            .or_else(|| {
                // A name that the one call of `finish` for the number of a
                // task defined, so that the frame may be looked at before it is
                // read.
                let Expr::Name { name, .. } = &arg.value else {
                    return None;
                };
                let call = body.f.and_then(|f| defined_by_finish(&f.body, name))?;
                self.of_a_task_call(call)
            });
        let error = match gives {
            Some(gives) if gives == read || gives.unknown() || read.unknown() => return,
            Some(gives) => format!(
                "this frame is read as `{}`, and the lambda it was made of gives `{}`",
                text(read),
                text(gives)
            ),
            None => "what this frame gives is not known here, since it is not the call of `__frame`, or a name that one call of it defined, or what `finish` gives for the number of a `Task`".to_string(),
        };
        self.errors.push(Error::new(arg.value.pos(), error));
    }
}

impl<'a> Checker<'a> {
    /// What the frame gives that `finish` gives for the number of a `Task[T]`,
    /// which is `T`, since only `spawn` of `std.task` makes a `Task[T]`, and from
    /// a frame of a lambda that gives `T`.
    fn of_a_task(&self, arg: &Arg) -> Option<&Ty> {
        let Expr::Call(finish) = &arg.value else {
            return None;
        };
        self.of_a_task_call(finish)
    }

    /// The same, for the call of `finish` itself.
    fn of_a_task_call(&self, finish: &Call) -> Option<&Ty> {
        if finish.callee != "finish" || finish.qualifier.is_some() || !of_std(finish.pos) {
            return None;
        }
        let [
            Arg {
                value: Expr::Field { of, name, .. },
                ..
            },
        ] = &finish.args[..]
        else {
            return None;
        };
        match self.types.get(&addr(&**of))? {
            Ty::Struct(s, args) if name == "number" && Some(*s) == self.defs.task => args.first(),
            _ => None,
        }
    }
}

/// The call of `finish` that the one definition of `name` in a function is the
/// value of, when `name` is defined once and never assigned again.
fn defined_by_finish<'b>(body: &'b [Stmt], name: &str) -> Option<&'b Call> {
    let mut found: Option<&Call> = None;
    let mut times = 0;
    for stmt in statements(body) {
        match stmt {
            Stmt::Define {
                name: n,
                var: false,
                value: Expr::Call(c),
                ..
            } if n == name => {
                times += 1;
                found = (c.callee == "finish").then_some(c);
            }
            Stmt::Define { name: n, .. } | Stmt::Assign { name: n, .. } if n == name => times += 1,
            _ => {}
        }
    }
    found.filter(|_| times == 1)
}

/// The call of `__frame` that the one definition of `name` in a function is the
/// value of, when `name` is defined once and never assigned again.
fn made_by<'b>(body: &'b [Stmt], name: &str) -> Option<&'b Call> {
    let mut found: Option<&Call> = None;
    let mut times = 0;
    for stmt in statements(body) {
        match stmt {
            Stmt::Define {
                name: n,
                var: false,
                value: Expr::Call(c),
                ..
            } if n == name => {
                times += 1;
                found = is_frame_call(c).then_some(c);
            }
            Stmt::Define { name: n, .. } | Stmt::Assign { name: n, .. } if n == name => times += 1,
            _ => {}
        }
    }
    found.filter(|_| times == 1)
}
