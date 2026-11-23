//! A function that suspends is a state machine whose frame holds what it needs
//! while it waits: the pass `suspend` makes it, and for every function with a
//! wait in it, under any `if`, `loop`, early return, and call of another
//! function that waits, the state machine does what the function does, whether
//! each wait is over at once or the task is suspended and resumed. The functions
//! are built in the scalar form from what a generator picks.

use smith::scalar::{
    self, Binary, Bits, Builder, Compare, End, Function, Op, Program, System, Type, Value,
};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_function_with_waits_as_a_state_machine_does_what_it_does_for_every_order_of_readiness",
        a_function_with_waits_as_a_state_machine_does_what_it_does_for_every_order_of_readiness,
    ),
    (
        "a_state_machine_gives_a_wait_each_time_the_function_waits_and_no_other_time",
        a_state_machine_gives_a_wait_each_time_the_function_waits_and_no_other_time,
    ),
    (
        "the_state_of_a_scheduler_goes_through_each_poll_and_no_frame_saves_it",
        the_state_of_a_scheduler_goes_through_each_poll_and_no_frame_saves_it,
    ),
    (
        "a_task_asked_to_end_leaves_by_the_exit_of_the_wait_it_is_in_and_each_function_it_is_in_leaves_by_its_own",
        a_task_asked_to_end_leaves_by_the_exit_of_the_wait_it_is_in_and_each_function_it_is_in_leaves_by_its_own,
    ),
    (
        "a_function_that_is_not_a_state_machine_is_never_asked_to_end",
        a_function_that_is_not_a_state_machine_is_never_asked_to_end,
    ),
    (
        "a_task_that_calls_one_that_does_not_wait_goes_on_with_the_state_it_gave",
        a_task_that_calls_one_that_does_not_wait_goes_on_with_the_state_it_gave,
    ),
    (
        "a_program_with_no_wait_is_not_changed",
        a_program_with_no_wait_is_not_changed,
    ),
    (
        "a_function_that_waits_and_calls_itself_is_refused",
        a_function_that_waits_and_calls_itself_is_refused,
    ),
    (
        "a_recursion_a_thousand_deep_with_a_wait_at_each_level_gives_the_right_number_and_the_right_waits",
        a_recursion_a_thousand_deep_with_a_wait_at_each_level_gives_the_right_number_and_the_right_waits,
    ),
    (
        "a_main_that_waits_is_refused",
        a_main_that_waits_is_refused,
    ),
    (
        "a_state_machine_has_no_wait_left_and_the_program_is_well_formed",
        a_state_machine_has_no_wait_left_and_the_program_is_well_formed,
    ),
];

const LONG: Type = Type::Int(Bits::B64);

/// What a function does, in the order it does it. It keeps one number, which each
/// statement changes, and the number it was given, which is read after every wait.
#[derive(Clone, Debug)]
enum Stmt {
    /// Adds the number and then flips the bits the first number has.
    Add(u64),
    /// Waits for the kind and the handle that the number and this make.
    Wait { kind: u64, mix: u64 },
    If {
        mask: u64,
        then: Vec<Stmt>,
        otherwise: Vec<Stmt>,
    },
    Loop {
        times: u64,
        body: Vec<Stmt>,
    },
    /// Calls a function made before this one with the number.
    Call(usize),
    /// Gives the number back when its bits under the mask are 0.
    Leave(u64),
    /// Keeps the number in the function's own bytes.
    Spill,
    /// Flips the number's bits with what the bytes hold.
    Reload,
    /// Calls the function itself with one less of the depth it was given, when
    /// that is not 0.
    Recurse,
}

#[derive(Clone, Debug)]
struct Case {
    functions: Vec<Vec<Stmt>>,
    given: u64,
    /// How deep the function may call itself.
    depth: u64,
}

fn statements(g: &mut Gen, depth: usize, calls: usize) -> Vec<Stmt> {
    (0..g.below(5)).map(|_| statement(g, depth, calls)).collect()
}

fn statement(g: &mut Gen, depth: usize, calls: usize) -> Stmt {
    let deep = depth == 0;
    match g.below(if deep { 6 } else { 9 }) {
        0 => Stmt::Add(g.below(1000) as u64),
        1 | 2 => Stmt::Wait {
            kind: g.below(4) as u64,
            mix: g.below(100) as u64,
        },
        3 => Stmt::Leave(1 << g.below(3)),
        4 if g.one_in(3) => Stmt::Recurse,
        4 => Stmt::Spill,
        5 => {
            if calls > 0 {
                Stmt::Call(g.below(calls))
            } else {
                Stmt::Reload
            }
        }
        6 => Stmt::If {
            mask: 1 << g.below(3),
            then: statements(g, depth - 1, calls),
            otherwise: statements(g, depth - 1, calls),
        },
        _ => Stmt::Loop {
            times: g.below(4) as u64,
            body: statements(g, depth - 1, calls),
        },
    }
}

fn cases(g: &mut Gen) -> Case {
    let count = 1 + g.below(3);
    let functions = (0..count)
        .map(|made| {
            let mut body = statements(g, 3, made);
            if made == 0 {
                body.insert(0, Stmt::Wait { kind: 1, mix: 7 });
            }
            body
        })
        .collect();
    Case {
        functions,
        given: g.below(1 << 16) as u64,
        depth: g.below(3) as u64,
    }
}

struct Made<'a> {
    b: Builder,
    x: Value,
    depth: Value,
    /// The place of the function being made, which `Recurse` calls.
    at: usize,
    scratch: Value,
    case: &'a Case,
}

impl Made<'_> {
    fn statements(&mut self, stmts: &[Stmt], mut acc: Value) -> Value {
        for s in stmts {
            acc = self.statement(s, acc);
        }
        acc
    }

    fn statement(&mut self, s: &Stmt, acc: Value) -> Value {
        match s {
            Stmt::Add(n) => {
                let n = self.b.int(Bits::B64, *n);
                let sum = self.b.binary(Binary::Add, acc, n);
                self.b.binary(Binary::Xor, sum, self.x)
            }
            Stmt::Wait { kind, mix } => {
                let kind = self.b.int(Bits::B64, *kind);
                let mix = self.b.int(Bits::B64, *mix);
                let handle = self.b.binary(Binary::Add, acc, mix);
                self.b.emit(
                    Op::System {
                        call: System::Suspend,
                        args: vec![kind, handle],
                    },
                    &[],
                );
                acc
            }
            Stmt::Spill => {
                self.b.store(self.scratch, acc);
                acc
            }
            Stmt::Reload => {
                let held = self.b.load(LONG, self.scratch);
                self.b.binary(Binary::Xor, acc, held)
            }
            Stmt::Call(f) => self.b.call(*f, vec![acc, self.depth], &[LONG])[0],
            Stmt::Recurse => {
                let (calls, _) = self.b.block(&[]);
                let (join, joined) = self.b.block(&[LONG]);
                let zero = self.b.int(Bits::B64, 0);
                let none = self.b.compare(Compare::Equal, self.depth, zero);
                self.b.switch(none, vec![(calls, vec![]), (join, vec![acc])]);
                self.b.at(calls);
                let one = self.b.int(Bits::B64, 1);
                let less = self.b.binary(Binary::Sub, self.depth, one);
                let again = self.b.call(self.at, vec![acc, less], &[LONG])[0];
                self.b.jump(join, vec![again]);
                self.b.at(join);
                joined[0]
            }
            Stmt::Leave(mask) => {
                let (leaves, _) = self.b.block(&[]);
                let (goes_on, _) = self.b.block(&[]);
                let on = self.zero_under(acc, *mask);
                self.b.switch(on, vec![(goes_on, vec![]), (leaves, vec![])]);
                self.b.at(leaves);
                self.b.ret(vec![acc]);
                self.b.at(goes_on);
                acc
            }
            Stmt::If {
                mask,
                then,
                otherwise,
            } => {
                let (yes, _) = self.b.block(&[]);
                let (no, _) = self.b.block(&[]);
                let (join, joined) = self.b.block(&[LONG]);
                let on = self.zero_under(acc, *mask);
                self.b.switch(on, vec![(no, vec![]), (yes, vec![])]);
                self.b.at(yes);
                let a = self.statements(then, acc);
                self.b.jump(join, vec![a]);
                self.b.at(no);
                let a = self.statements(otherwise, acc);
                self.b.jump(join, vec![a]);
                self.b.at(join);
                joined[0]
            }
            Stmt::Loop { times, body } => {
                let (header, params) = self.b.block(&[LONG, LONG]);
                let (inside, _) = self.b.block(&[]);
                let (out, outs) = self.b.block(&[LONG]);
                let zero = self.b.int(Bits::B64, 0);
                self.b.jump(header, vec![zero, acc]);
                self.b.at(header);
                let limit = self.b.int(Bits::B64, *times);
                let below = self.b.compare(Compare::LessU, params[0], limit);
                self.b
                    .switch(below, vec![(out, vec![params[1]]), (inside, vec![])]);
                self.b.at(inside);
                let a = self.statements(body, params[1]);
                let one = self.b.int(Bits::B64, 1);
                let next = self.b.binary(Binary::Add, params[0], one);
                self.b.jump(header, vec![next, a]);
                self.b.at(out);
                outs[0]
            }
        }
    }

    /// 1 when the bits of the number under the mask are 0.
    fn zero_under(&mut self, acc: Value, mask: u64) -> Value {
        let m = self.b.int(Bits::B64, mask);
        let under = self.b.binary(Binary::And, acc, m);
        let zero = self.b.int(Bits::B64, 0);
        self.b.compare(Compare::Equal, under, zero)
    }
}

fn function_of(case: &Case, at: usize, body: &[Stmt]) -> Function {
    let mut b = Builder::new();
    let [x, depth] = b.params(&[LONG, LONG])[..] else {
        unreachable!()
    };
    let scratch = b.emit(Op::Frame { bytes: 8 }, &[Type::Address])[0];
    b.store(scratch, x);
    let mut made = Made {
        b,
        x,
        depth,
        at,
        scratch,
        case,
    };
    let _ = made.case;
    let acc = made.statements(body, x);
    made.b.ret(vec![acc]);
    made.b.finish(&[LONG])
}

fn program_of(case: &Case) -> Program {
    let mut functions: Vec<Function> = case
        .functions
        .iter()
        .enumerate()
        .map(|(at, f)| function_of(case, at, f))
        .collect();
    let mut main = Builder::new();
    main.ret(vec![]);
    functions.push(main.finish(&[]));
    let main = functions.len() - 1;
    Program {
        data: Vec::new(),
        functions,
        main,
    }
}

/// What says that a wait is over, from the order the waits come in, so that two
/// runs that wait in the same order are answered the same.
struct Readiness(u64);

impl Readiness {
    fn answer(&mut self, kind: u64, handle: u64) -> bool {
        self.0 = (self.0 ^ kind.wrapping_mul(31) ^ handle)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) & 1 == 0
    }
}

fn waits_of(program: &Program) -> bool {
    (program.functions.iter().flat_map(|f| &f.blocks))
        .flat_map(|b| &b.instructions)
        .any(|i| {
            matches!(
                i.op,
                Op::System {
                    call: System::Suspend,
                    ..
                }
            )
        })
}

fn a_function_with_waits_as_a_state_machine_does_what_it_does_for_every_order_of_readiness() {
    generate::check(160, 150, cases, |case| {
        let program = program_of(case);
        assert_eq!(scalar::check(&program), Ok(()));
        if !waits_of(&program) {
            return;
        }
        let made = scalar::suspend(scalar::recur(program.clone()))
            .expect("a program that reaches itself");
        let poll_any = made.poll_any.expect("a program with a function that waits");
        // Each function that waits is a task, and each is polled through the
        // one function that finds the poll from the frame.
        for task in &made.tasks {
            for order in 0..4u64 {
                let args = [case.given, case.depth];
                let mut asks = Readiness(order);
                let (want, waits) =
                    scalar::call_waiting(&program, task.start, &args, &mut |k, h| {
                        asks.answer(k, h)
                    });
                let mut asks = Readiness(order);
                let (got, waited, waiting) =
                    scalar::poll_to_end(&made.program, poll_any, task, &args, &mut |k, h| {
                        asks.answer(k, h)
                    });
                assert_eq!(got, want, "order {order}");
                assert_eq!(waited, waits, "order {order}");
                assert_eq!(waiting, waits.len(), "order {order}");
            }
        }
    });
}

fn a_state_machine_gives_a_wait_each_time_the_function_waits_and_no_other_time() {
    // `Wait` first and last of a function that calls one that waits, so the waits
    // come from the function and from the callee, in the order of the calls.
    let case = Case {
        functions: vec![
            vec![Stmt::Wait { kind: 1, mix: 1 }, Stmt::Add(3)],
            vec![
                Stmt::Wait { kind: 2, mix: 0 },
                Stmt::Call(0),
                Stmt::Call(0),
                Stmt::Wait { kind: 3, mix: 0 },
            ],
        ],
        given: 5,
        depth: 0,
    };
    let program = program_of(&case);
    let made = scalar::suspend(program.clone()).expect("no call of itself");
    let task = made.tasks.iter().find(|t| t.start == 1).expect("a task");
    let (_, waits) = scalar::call_waiting(&program, 1, &[5, 0], &mut |_, _| false);
    let kinds: Vec<u64> = waits.iter().map(|w| w.kind).collect();
    assert_eq!(kinds, vec![2, 1, 1, 3]);
    assert!(waits.iter().all(|w| !w.ready));
    let poll_any = made.poll_any.expect("a function waits");
    let (_, waited, waiting) =
        scalar::poll_to_end(&made.program, poll_any, task, &[5, 0], &mut |_, _| false);
    assert_eq!(waited, waits);
    assert_eq!(waiting, 4);
}

fn a_program_with_no_wait_is_not_changed() {
    generate::check(161, 100, cases, |case| {
        let mut case = case.clone();
        for f in &mut case.functions {
            strip(f);
        }
        let program = program_of(&case);
        assert!(!waits_of(&program));
        let made = scalar::suspend(program.clone()).expect("nothing waits");
        assert_eq!(made.program, program);
        assert!(made.tasks.is_empty());
    });
}

fn strip(body: &mut Vec<Stmt>) {
    body.retain(|s| !matches!(s, Stmt::Wait { .. }));
    for s in body.iter_mut() {
        match s {
            Stmt::If {
                then, otherwise, ..
            } => {
                strip(then);
                strip(otherwise);
            }
            Stmt::Loop { body, .. } => strip(body),
            _ => {}
        }
    }
}

fn a_function_that_waits_and_calls_itself_is_refused() {
    let mut b = Builder::new();
    let [x] = b.params(&[LONG])[..] else {
        unreachable!()
    };
    let k = b.int(Bits::B64, 1);
    b.emit(
        Op::System {
            call: System::Suspend,
            args: vec![k, x],
        },
        &[],
    );
    let again = b.call(0, vec![x], &[LONG]);
    b.ret(again);
    let waits = b.finish(&[LONG]);
    let mut main = Builder::new();
    main.ret(vec![]);
    let program = Program {
        data: Vec::new(),
        functions: vec![waits, main.finish(&[])],
        main: 1,
    };
    assert!(scalar::suspend(program).is_err());
}

fn a_main_that_waits_is_refused() {
    let mut main = Builder::new();
    let k = main.int(Bits::B64, 1);
    main.emit(
        Op::System {
            call: System::Suspend,
            args: vec![k, k],
        },
        &[],
    );
    main.ret(vec![]);
    let program = Program {
        data: Vec::new(),
        functions: vec![main.finish(&[])],
        main: 0,
    };
    assert!(scalar::suspend(program).is_err());
}

fn a_state_machine_has_no_wait_left_and_the_program_is_well_formed() {
    generate::check(162, 100, cases, |case| {
        let program = program_of(case);
        let made = scalar::suspend(scalar::recur(program)).expect("no call of itself");
        assert_eq!(scalar::check(&made.program), Ok(()));
        assert!(!waits_of(&made.program));
        for task in &made.tasks {
            // The start and the poll take the frame, which is a multiple of 16.
            assert_eq!(task.bytes % 16, 0);
            let poll = &made.program.functions[task.poll];
            assert_eq!(poll.blocks[0].params.len(), 1);
            assert!(matches!(poll.blocks[0].end, End::Switch { .. }));
        }
    });
}

/// `down(n)`: 0 when `n` is 0, and otherwise the wait for `n`, and then 1 more
/// than `down(n - 1)`, so that each level waits and keeps what it needs.
fn down() -> Function {
    let mut b = Builder::new();
    let [n] = b.params(&[LONG])[..] else {
        unreachable!()
    };
    let (more, _) = b.block(&[]);
    let (stop, _) = b.block(&[]);
    let zero = b.int(Bits::B64, 0);
    let done = b.compare(Compare::Equal, n, zero);
    b.switch(done, vec![(more, vec![]), (stop, vec![])]);
    b.at(more);
    let kind = b.int(Bits::B64, 7);
    b.emit(
        Op::System {
            call: System::Suspend,
            args: vec![kind, n],
        },
        &[],
    );
    let one = b.int(Bits::B64, 1);
    let below = b.binary(Binary::Sub, n, one);
    let made = b.call(0, vec![below], &[LONG]);
    let sum = b.binary(Binary::Add, made[0], one);
    b.ret(vec![sum]);
    b.at(stop);
    b.ret(vec![zero]);
    b.finish(&[LONG])
}

fn a_recursion_a_thousand_deep_with_a_wait_at_each_level_gives_the_right_number_and_the_right_waits()
{
    let mut main = Builder::new();
    main.ret(vec![]);
    let program = Program {
        data: Vec::new(),
        functions: vec![down(), main.finish(&[])],
        main: 1,
    };
    // A function that waits and reaches itself is made a loop over a stack in
    // memory first, so it is given as such, and without that it is refused.
    assert!(scalar::suspend(program.clone()).is_err());
    let made = scalar::suspend(scalar::recur(program.clone())).expect("no call reaches itself");
    assert_eq!(scalar::check(&made.program), Ok(()));
    let task = &made.tasks[0];
    let poll_any = made.poll_any.expect("a function waits");
    for n in [0u64, 1, 5, 1000, 1500] {
        let mut asks = Readiness(n);
        let (want, waits) =
            scalar::call_waiting(&program, 0, &[n], &mut |k, h| asks.answer(k, h));
        let mut asks = Readiness(n);
        let (got, waited, waiting) =
            scalar::poll_to_end(&made.program, poll_any, task, &[n], &mut |k, h| {
                asks.answer(k, h)
            });
        assert_eq!(want, vec![n], "n {n}");
        assert_eq!(got, want, "n {n}");
        assert_eq!(waited, waits, "n {n}");
        assert_eq!(waiting, n as usize, "n {n}");
    }
}

/// A function whose last parameter and last result are the state of a
/// scheduler: it waits twice, and after each wait goes on with the state it is
/// given, which the scheduler may have changed while the task was suspended.
fn spend() -> Function {
    let mut b = Builder::new();
    let [x, state] = b.params(&[LONG, LONG])[..] else {
        unreachable!()
    };
    let kind = b.int(Bits::B64, 7);
    let first = b.emit(
        Op::System {
            call: System::Suspend,
            args: vec![kind, x, state],
        },
        &[LONG],
    )[0];
    let more = b.binary(Binary::Add, first, x);
    let kind = b.int(Bits::B64, 8);
    let second = b.emit(
        Op::System {
            call: System::Suspend,
            args: vec![kind, x, more],
        },
        &[LONG],
    )[0];
    let sum = b.binary(Binary::Add, second, x);
    b.ret(vec![sum, second]);
    b.finish(&[LONG, LONG])
}

/// A function that calls `spend`, and waits once more with the state it gave.
fn spend_and_wait() -> Function {
    let mut b = Builder::new();
    let [x, state] = b.params(&[LONG, LONG])[..] else {
        unreachable!()
    };
    let made = b.call(0, vec![x, state], &[LONG, LONG]);
    let kind = b.int(Bits::B64, 9);
    let after = b.emit(
        Op::System {
            call: System::Suspend,
            args: vec![kind, made[0], made[1]],
        },
        &[LONG],
    )[0];
    let sum = b.binary(Binary::Add, made[0], after);
    b.ret(vec![sum, after]);
    b.finish(&[LONG, LONG])
}

/// The scheduler's state is given to each poll and given back by it, and the
/// task goes on with what it is given after a wait, so what the scheduler does
/// to the state while the task is suspended is seen by the task, and what the
/// task does to it is seen by the scheduler after each poll. No frame holds it:
/// if one did, the task would go on with the state it had when it waited.
fn the_state_of_a_scheduler_goes_through_each_poll_and_no_frame_saves_it() {
    let mut main = Builder::new();
    main.ret(vec![]);
    let program = Program {
        data: Vec::new(),
        functions: vec![spend(), spend_and_wait(), main.finish(&[])],
        main: 2,
    };
    assert_eq!(scalar::check(&program), Ok(()));
    let frames = scalar::Frames {
        poll_any: None,
        starters: Vec::new(),
        through: vec![LONG],
    };
    let made = scalar::suspend_frames(program, &frames).expect("no call reaches itself");
    assert_eq!(scalar::check(&made.program), Ok(()));
    let poll_any = made.poll_any.expect("a function waits");
    let outer = made.tasks.iter().find(|t| t.start == 1).expect("a task");
    let mut state = vec![1u64];
    let mut seen = Vec::new();
    let (got, waited, waiting) = scalar::poll_through(
        &made.program,
        poll_any,
        outer,
        &[2],
        &mut state,
        &mut |state| {
            // The scheduler sees the state the task gave at its wait, and
            // changes it before the next poll.
            seen.push(state[0]);
            state[0] += 1000;
        },
        &mut |_, _| false,
    );
    // The task gave 1, 1003, and 2003 at its three waits, each a thousand more
    // than the one before it plus what the task added, and went on with what it
    // was given: 1001, 2003, and 3003.
    assert_eq!(seen, vec![1, 1003, 2003]);
    assert_eq!(state, vec![3003]);
    assert_eq!(got, vec![2005 + 3003]);
    assert_eq!(waiting, 3);
    let kinds: Vec<(u64, u64)> = waited.iter().map(|w| (w.kind, w.handle)).collect();
    assert_eq!(kinds, vec![(7, 2), (8, 2), (9, 2005)]);
}

/// A function that waits only for an odd number, and adds to the state either way.
fn sometimes() -> Function {
    let mut b = Builder::new();
    let [x, state, other] = b.params(&[LONG, LONG, LONG])[..] else {
        unreachable!()
    };
    let one = b.int(Bits::B64, 1);
    let low = b.binary(Binary::And, x, one);
    let zero = b.int(Bits::B64, 0);
    let even = b.compare(Compare::Equal, low, zero);
    let (waits, _) = b.block(&[]);
    let (joined, params) = b.block(&[LONG, LONG]);
    b.switch(even, vec![(waits, vec![]), (joined, vec![state, other])]);
    b.at(waits);
    let kind = b.int(Bits::B64, 5);
    let after = b.emit(
        Op::System {
            call: System::Suspend,
            args: vec![kind, x, state, other],
        },
        &[LONG, LONG],
    );
    b.jump(joined, after);
    b.at(joined);
    let more = b.binary(Binary::Add, params[0], x);
    b.ret(vec![x, more, params[1]]);
    b.finish(&[LONG, LONG, LONG])
}

/// A function that calls it and gives what it gave and the state.
fn calls_sometimes() -> Function {
    let mut b = Builder::new();
    let [x, state, other] = b.params(&[LONG, LONG, LONG])[..] else {
        unreachable!()
    };
    let made = b.call(0, vec![x, state, other], &[LONG, LONG, LONG]);
    let sum = b.binary(Binary::Add, made[0], x);
    b.ret(vec![sum, made[1], made[2]]);
    b.finish(&[LONG, LONG, LONG])
}

/// A task that calls one that is over at its first poll goes on with the state
/// that one gave, which is what the call gave of it.
fn a_task_that_calls_one_that_does_not_wait_goes_on_with_the_state_it_gave() {
    let mut main = Builder::new();
    main.ret(vec![]);
    let program = Program {
        data: Vec::new(),
        functions: vec![sometimes(), calls_sometimes(), main.finish(&[])],
        main: 2,
    };
    assert_eq!(scalar::check(&program), Ok(()));
    let frames = scalar::Frames {
        poll_any: None,
        starters: Vec::new(),
        through: vec![LONG, LONG],
    };
    let made = scalar::suspend_frames(program, &frames).expect("no call reaches itself");
    assert_eq!(scalar::check(&made.program), Ok(()));
    let poll_any = made.poll_any.expect("a function waits");
    let outer = made.tasks.iter().find(|t| t.start == 1).expect("a task");
    for (x, waits) in [(4u64, 0usize), (6, 0), (3, 1), (9, 1)] {
        let mut state = vec![100u64, 7];
        let (got, waited, waiting) = scalar::poll_through(
            &made.program,
            poll_any,
            outer,
            &[x],
            &mut state,
            &mut |state| state[0] += 1000,
            &mut |_, _| false,
        );
        // The state is 100, and 1000 more for each wait, and x is added to it.
        let expected_state = 100 + 1000 * waits as u64 + x;
        assert_eq!(state, vec![expected_state, 7], "x {x}");
        assert_eq!(got, vec![x + x], "x {x}");
        assert_eq!(waited.len(), waits, "x {x}");
        assert_eq!(waiting, waits, "x {x}");
    }
}

/// A function that waits twice, and after each wait leaves by an exit of its own
/// when the task has been asked to end: it gives 111 after the first wait and 222
/// after the second, and what it gave and the state when it is not asked to.
fn leaves_when_asked() -> Function {
    let mut b = Builder::new();
    let [x, state] = b.params(&[LONG, LONG]) [..] else {
        unreachable!()
    };
    let kind = b.int(Bits::B64, 7);
    let first = b.emit(
        Op::System {
            call: System::Suspend,
            args: vec![kind, x, state],
        },
        &[LONG],
    )[0];
    let asked = b.emit(
        Op::System {
            call: System::Cancelled,
            args: vec![],
        },
        &[LONG],
    )[0];
    let zero = b.int(Bits::B64, 0);
    let not_asked = b.compare(Compare::Equal, asked, zero);
    let (stop, _) = b.block(&[]);
    let (go, _) = b.block(&[]);
    b.switch(not_asked, vec![(stop, vec![]), (go, vec![])]);
    b.at(stop);
    let ended = b.int(Bits::B64, 111);
    b.ret(vec![ended, first]);
    b.at(go);
    let kind = b.int(Bits::B64, 8);
    let second = b.emit(
        Op::System {
            call: System::Suspend,
            args: vec![kind, x, first],
        },
        &[LONG],
    )[0];
    let asked = b.emit(
        Op::System {
            call: System::Cancelled,
            args: vec![],
        },
        &[LONG],
    )[0];
    let zero = b.int(Bits::B64, 0);
    let not_asked = b.compare(Compare::Equal, asked, zero);
    let (stop, _) = b.block(&[]);
    let (go, _) = b.block(&[]);
    b.switch(not_asked, vec![(stop, vec![]), (go, vec![])]);
    b.at(stop);
    let ended = b.int(Bits::B64, 222);
    b.ret(vec![ended, second]);
    b.at(go);
    let five = b.int(Bits::B64, 5);
    let sum = b.binary(Binary::Add, x, second);
    let sum = b.binary(Binary::Add, sum, five);
    b.ret(vec![sum, second]);
    b.finish(&[LONG, LONG])
}

/// A function that calls it and leaves by an exit of its own, giving 333, when
/// the task has been asked to end, as it must even if the call went on to its end.
fn calls_and_leaves_when_asked() -> Function {
    let mut b = Builder::new();
    let [x, state] = b.params(&[LONG, LONG]) [..] else {
        unreachable!()
    };
    let made = b.call(0, vec![x, state], &[LONG, LONG]);
    let asked = b.emit(
        Op::System {
            call: System::Cancelled,
            args: vec![],
        },
        &[LONG],
    )[0];
    let zero = b.int(Bits::B64, 0);
    let not_asked = b.compare(Compare::Equal, asked, zero);
    let (stop, _) = b.block(&[]);
    let (go, _) = b.block(&[]);
    b.switch(not_asked, vec![(stop, vec![]), (go, vec![])]);
    b.at(stop);
    let ended = b.int(Bits::B64, 333);
    b.ret(vec![ended, made[1]]);
    b.at(go);
    let thousand = b.int(Bits::B64, 1000);
    let sum = b.binary(Binary::Add, made[0], thousand);
    b.ret(vec![sum, made[1]]);
    b.finish(&[LONG, LONG])
}

/// A task that is asked to end after its first wait, or its second, or before it
/// starts, goes on from the wait it is in by the exit that follows it, and each
/// function that the task is the middle of leaves by its own exit, the innermost
/// first, however far in the chain it is asked: the bit is in the header of the
/// frame of the task, and each poll gives it to the frame of what it polls. A
/// task that is not asked goes on as it did, and each wait it makes is one wait.
fn a_task_asked_to_end_leaves_by_the_exit_of_the_wait_it_is_in_and_each_function_it_is_in_leaves_by_its_own() {
    let mut main = Builder::new();
    main.ret(vec![]);
    let program = Program {
        data: Vec::new(),
        functions: vec![leaves_when_asked(), calls_and_leaves_when_asked(), main.finish(&[])],
        main: 2,
    };
    assert_eq!(scalar::check(&program), Ok(()));
    let frames = scalar::Frames {
        poll_any: None,
        starters: Vec::new(),
        through: vec![LONG],
    };
    let made = scalar::suspend_frames(program, &frames).expect("no call reaches itself");
    assert_eq!(scalar::check(&made.program), Ok(()));
    let poll_any = made.poll_any.expect("a function waits");
    let run = |start: usize, after: Option<usize>| {
        let task = made.tasks.iter().find(|t| t.start == start).expect("a task");
        let mut state = vec![1u64];
        let (got, waited, waits) = scalar::poll_cancelling(
            &made.program,
            poll_any,
            task,
            &[2],
            &mut state,
            &mut |_| {},
            &mut |_, _| false,
            after,
        );
        let kinds: Vec<u64> = waited.iter().map(|w| w.kind).collect();
        (got, kinds, waits)
    };
    // The inner function by itself, which is the task, with the state 1 after
    // each wait, and 2 + 1 + 5 when it is not asked.
    assert_eq!(run(0, None), (vec![8], vec![7, 8], 2));
    assert_eq!(run(0, Some(0)), (vec![111], vec![7], 1));
    assert_eq!(run(0, Some(1)), (vec![111], vec![7], 1));
    assert_eq!(run(0, Some(2)), (vec![222], vec![7, 8], 2));
    // The caller of it, which gives what it gave plus 1000 when it is not asked,
    // and 333 whichever wait the inner function is in.
    assert_eq!(run(1, None), (vec![1008], vec![7, 8], 2));
    assert_eq!(run(1, Some(0)), (vec![333], vec![7], 1));
    assert_eq!(run(1, Some(1)), (vec![333], vec![7], 1));
    assert_eq!(run(1, Some(2)), (vec![333], vec![7, 8], 2));
}

/// A function that is not made a state machine reads the question as no, whether
/// or not another function of the program is, so a program that starts no task
/// has no such instruction left and its executable is as it was.
fn a_function_that_is_not_a_state_machine_is_never_asked_to_end() {
    let mut b = Builder::new();
    let asked = b.emit(
        Op::System {
            call: System::Cancelled,
            args: vec![],
        },
        &[LONG],
    )[0];
    b.ret(vec![asked]);
    let plain = b.finish(&[LONG]);
    let ended = || {
        let mut main = Builder::new();
        main.ret(vec![]);
        main.finish(&[])
    };
    for functions in [
        vec![plain.clone(), ended()],
        vec![plain, leaves_when_asked(), ended()],
    ] {
        let main = functions.len() - 1;
        let program = Program {
            data: Vec::new(),
            functions,
            main,
        };
        let frames = scalar::Frames {
            poll_any: None,
            starters: Vec::new(),
            through: vec![LONG],
        };
        let made = scalar::suspend_frames(program, &frames).expect("no call reaches itself");
        let asks = made.program.functions[0].blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.op, Op::System { call: System::Cancelled, .. }))
        });
        assert!(!asks);
        assert_eq!(scalar::check(&made.program), Ok(()));
    }
}
