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
        _ => None,
    }
}

/// A call of `__frame`, whose lambda is given to the frame and runs after the
/// call that made it has moved on, so what it gives away moves into the frame.
pub(super) fn is_frame_call(call: &Call) -> bool {
    kind(call) == Some(Kind::Frame)
}

/// A call of `__result`, which takes the frame it reads.
pub(super) fn takes_frame(call: &Call) -> bool {
    kind(call) == Some(Kind::Result)
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
            scope: Scope::default(),
            params: vec![(name.to_string(), ty)],
            defaults: vec![false],
            param_defaults: Vec::new(),
            gives,
            effects: Vec::new(),
            fn_effects: vec![Vec::new()],
        };
        match kind {
            Kind::Frame => {
                // The lambda may carry the effects of the function it is
                // written in and no others, as the work of `spawn` does.
                let signature = Signature {
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
        let error = match made.and_then(|c| self.frame_gives.get(&addr(c))) {
            Some(gives) if gives == read || gives.unknown() || read.unknown() => return,
            Some(gives) => format!(
                "this frame is read as `{}`, and the lambda it was made of gives `{}`",
                text(read),
                text(gives)
            ),
            None => "what this frame gives is not known here, since it is not the call of `__frame`, or a name that one call of it defined".to_string(),
        };
        self.errors.push(Error::new(arg.value.pos(), error));
    }
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
