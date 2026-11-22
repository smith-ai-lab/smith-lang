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
        let top = case.functions.len() - 1;
        let made = scalar::suspend(scalar::recur(program.clone())).expect("a program that reaches itself");
        // A function that never reaches a wait is not made a state machine.
        let Some(task) = made.tasks.iter().find(|t| t.start == top) else {
            return;
        };
        for order in 0..4u64 {
            let mut asks = Readiness(order);
            let (want, waits) = scalar::call_waiting(&program, top, &[case.given, case.depth], &mut |k, h| {
                asks.answer(k, h)
            });
            let mut asks = Readiness(order);
            let (got, waited, waiting) = scalar::poll_to_end(
                &made.program,
                task.start,
                task.poll,
                task.bytes,
                &[case.given, case.depth],
                &mut |k, h| asks.answer(k, h),
            );
            assert_eq!(got, want, "order {order}");
            assert_eq!(waited, waits, "order {order}");
            assert_eq!(waiting, waits.len(), "order {order}");
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
    let (_, waited, waiting) = scalar::poll_to_end(
        &made.program,
        task.start,
        task.poll,
        task.bytes,
        &[5, 0],
        &mut |_, _| false,
    );
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
    for n in [0u64, 1, 5, 1000, 1500] {
        let mut asks = Readiness(n);
        let (want, waits) =
            scalar::call_waiting(&program, 0, &[n], &mut |k, h| asks.answer(k, h));
        let mut asks = Readiness(n);
        let (got, waited, waiting) = scalar::poll_to_end(
            &made.program,
            task.start,
            task.poll,
            task.bytes,
            &[n],
            &mut |k, h| asks.answer(k, h),
        );
        assert_eq!(want, vec![n], "n {n}");
        assert_eq!(got, want, "n {n}");
        assert_eq!(waited, waits, "n {n}");
        assert_eq!(waiting, n as usize, "n {n}");
    }
}
