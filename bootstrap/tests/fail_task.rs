//! `Fail` ends the task that runs it by a failure, which a 64-bit number says: the
//! poll of a task that a scheduler runs gives the status of a task that ended by
//! a failure, and which, whatever depth of calls it ended at and whether or not it
//! had waited, and a program that no poll runs ends with the status 1. The form
//! refuses a `Fail` whose failure is not a number of 64 bits.

use smith::scalar::{self, Bits, Builder, Function, Op, Program, System, Type};

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "a_task_that_ends_by_a_failure_gives_the_failure_to_its_poll_at_any_depth_and_after_any_wait",
        a_task_that_ends_by_a_failure_gives_the_failure_to_its_poll_at_any_depth_and_after_any_wait,
    ),
    (
        "a_failure_that_no_poll_catches_ends_the_program_with_the_status_1",
        a_failure_that_no_poll_catches_ends_the_program_with_the_status_1,
    ),
    (
        "a_failure_that_is_not_a_number_of_64_bits_is_refused",
        a_failure_that_is_not_a_number_of_64_bits_is_refused,
    ),
];

const LONG: Type = Type::Int(Bits::B64);

/// A task that waits once and then ends by the failure 7.
fn waits_then_fails() -> Function {
    let mut b = Builder::new();
    let [x, state] = b.params(&[LONG, LONG])[..] else {
        unreachable!()
    };
    let kind = b.int(Bits::B64, 8);
    let second = b.emit(
        Op::System {
            call: System::Suspend,
            args: vec![kind, x, state],
        },
        &[LONG],
    )[0];
    let seven = b.int(Bits::B64, 7);
    b.fail(seven);
    b.ret(vec![x, second]);
    b.finish(&[LONG, LONG])
}

/// A function that calls the one before it, and then goes on to give a number
/// that nothing reads, since the callee never returns.
fn calls_it() -> Function {
    let mut b = Builder::new();
    let [x, state] = b.params(&[LONG, LONG])[..] else {
        unreachable!()
    };
    let made = b.call(0, vec![x, state], &[LONG, LONG]);
    let one = b.int(Bits::B64, 1);
    let sum = b.binary(scalar::Binary::Add, made[0], one);
    b.ret(vec![sum, made[1]]);
    b.finish(&[LONG, LONG])
}

fn a_task_that_ends_by_a_failure_gives_the_failure_to_its_poll_at_any_depth_and_after_any_wait() {
    let mut main = Builder::new();
    main.ret(vec![]);
    let program = Program {
        data: Vec::new(),
        functions: vec![waits_then_fails(), calls_it(), main.finish(&[])],
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
    // The task is the function that waits, or the one that calls it: either way
    // the poll gives the failure after the one wait, and not before it.
    for start in [0, 1] {
        let task = made.tasks.iter().find(|t| t.start == start).expect("a task");
        let mut state = vec![1u64];
        let (failed, waited) = scalar::poll_failing(
            &made.program,
            poll_any,
            task,
            &[2],
            &mut state,
            &mut |_, _| false,
        );
        assert_eq!(failed, Some(7), "the task that starts at {start}");
        assert_eq!(waited.len(), 1, "the task that starts at {start}");
    }
}

fn a_failure_that_no_poll_catches_ends_the_program_with_the_status_1() {
    let mut b = Builder::new();
    let three = b.int(Bits::B64, 3);
    b.fail(three);
    b.ret(vec![]);
    let program = Program {
        data: Vec::new(),
        functions: vec![b.finish(&[])],
        main: 0,
    };
    assert_eq!(scalar::check(&program), Ok(()));
    let (outcome, _) = scalar::run_counting(&program, usize::MAX);
    assert_eq!(outcome.status, 1);
}

fn a_failure_that_is_not_a_number_of_64_bits_is_refused() {
    let mut b = Builder::new();
    let three = b.int(Bits::B8, 3);
    b.fail(three);
    b.ret(vec![]);
    let program = Program {
        data: Vec::new(),
        functions: vec![b.finish(&[])],
        main: 0,
    };
    assert!(scalar::check(&program).is_err());
}
