//! A `File` that a task owns when it is asked to end, or when it fails, is closed by the
//! handler of `fs` in force, once: one that a function holds, one in a struct
//! of two, one in a list, and one that its lambda was given before it ran. A
//! function that holds a file while it may wait and does not carry `fs` is
//! refused where it is compiled.

use smith::{lower, scalar};

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "a_cancelled_task_closes_the_file_it_holds_through_the_handler_in_force_once",
        a_cancelled_task_closes_the_file_it_holds_through_the_handler_in_force_once,
    ),
    (
        "a_cancelled_task_closes_both_files_of_a_struct_it_holds",
        a_cancelled_task_closes_both_files_of_a_struct_it_holds,
    ),
    (
        "a_cancelled_task_closes_each_file_of_a_list_it_holds",
        a_cancelled_task_closes_each_file_of_a_list_it_holds,
    ),
    (
        "a_task_cancelled_before_it_ran_closes_the_file_its_lambda_was_given",
        a_task_cancelled_before_it_ran_closes_the_file_its_lambda_was_given,
    ),
    (
        "a_file_that_is_closed_by_its_function_is_not_closed_again_when_no_task_is_asked_to_end",
        a_file_that_is_closed_by_its_function_is_not_closed_again_when_no_task_is_asked_to_end,
    ),
    (
        "a_file_a_task_holds_when_it_fails_is_closed_by_the_handler_once",
        a_file_a_task_holds_when_it_fails_is_closed_by_the_handler_once,
    ),
    (
        "a_cancelled_task_closes_the_file_of_memory_fs_as_release",
        a_cancelled_task_closes_the_file_of_memory_fs_as_release,
    ),
];

/// A handler of `fs` that prints the number of each file it is asked to close,
/// so the program says what the handler was asked.
fn program(functions: &str, tasks: &str) -> String {
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.instant (ms)
use std.int (wrapping_add)
use std.fs (close_file, file, fs, handle, release, File)
use std.list (freeze, push)
use std.task (join, spawn, task, test_ready, test_ready_by, test_scheduler_with_io)

{functions}
fn run(seed: u64) -> u64 <fs, diverge, alloc> {{
  with test_scheduler_with_io(seed, [])
{tasks}
  late = test_ready_by(99, ms(10))
  1
}}

fn main(args: List[str]) <console, alloc, diverge> {{
  r = fs(
    open_file: fn(path) {{ .err(.missing(path)) }},
    create_file: fn(path, mode) {{ .err(.missing(path)) }},
    close_file: fn(f) {{
      print(\"closed {{handle(f)}}\")
      n = release(take f)
    }},
    size: fn(f) {{ .err(.missing(f.path)) }},
    read: fn(f, range) {{ .err(.missing(f.path)) }},
    write: fn(f, at, bytes) {{ .err(.missing(f.path)) }},
    body: {{ run(args.size) }},
  )
  print(\"{{r}}\")
}}
"
    )
}

/// What the program prints when it is started with `seeds` words, for each
/// number of them from 0: the number of words is the seed of the scheduler,
/// so that one program is built once and run for each seed.
fn said_for(source: &str, seeds: usize) -> Vec<Vec<String>> {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(scalar::check(&program), Ok(()), "for {source}");
    let program = scalar::passes(&program);
    (0..seeds)
        .map(|seed| {
            let outcome = scalar::run_with_words(&program, &vec![Vec::new(); seed]);
            let text = String::from_utf8(outcome.stdout).expect("text");
            assert_eq!(outcome.status, 0, "{text} for {source}");
            text.lines().map(str::to_string).collect()
        })
        .collect()
}

fn said(source: &str) -> Vec<String> {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(scalar::check(&program), Ok(()), "for {source}");
    let (outcome, _) = scalar::run_counting(&scalar::passes(&program), 1 << 22);
    let text = String::from_utf8(outcome.stdout).expect("text");
    assert_eq!(outcome.status, 0, "{text} for {source}");
    text.lines().map(str::to_string).collect()
}

fn sorted(mut lines: Vec<String>) -> Vec<String> {
    lines.sort();
    lines
}

const HOLD: &str = "fn hold(n: u64) -> u64 <fs, task> {
  f = file(n)
  test_ready(n)
  close_file(take f)
  n
}
";

fn a_cancelled_task_closes_the_file_it_holds_through_the_handler_in_force_once() {
    let source = program(HOLD, "  _ = spawn { hold(5) }\n  _ = spawn { hold(6) }");
    for (seed, said) in said_for(&source, 8).into_iter().enumerate() {
        assert_eq!(sorted(said), ["1", "closed 5", "closed 6"], "seed {seed}");
    }
}

const PAIR: &str = "struct Pair {
  a: File
  b: File
}

fn hold_two(n: u64) -> u64 <fs, task> {
  p = Pair(a: file(n), b: file(wrapping_add(n, 1)))
  test_ready(n)
  close_file(take p.a)
  close_file(take p.b)
  n
}
";

fn a_cancelled_task_closes_both_files_of_a_struct_it_holds() {
    let source = program(PAIR, "  _ = spawn { hold_two(10) }");
    for (seed, said) in said_for(&source, 8).into_iter().enumerate() {
        assert_eq!(sorted(said), ["1", "closed 10", "closed 11"], "seed {seed}");
    }
}

const LIST: &str = "fn hold_list(n: u64) -> List[File] <fs, task, alloc> {
  mut fs: List[File] = []
  fs = push(take fs, file(n))
  fs = push(take fs, file(wrapping_add(n, 1)))
  test_ready(n)
  fs
}
";

fn a_cancelled_task_closes_each_file_of_a_list_it_holds() {
    let source = program(LIST, "  t = spawn { hold_list(20) }");
    for (seed, said) in said_for(&source, 8).into_iter().enumerate() {
        assert_eq!(sorted(said), ["1", "closed 20", "closed 21"], "seed {seed}");
    }
}

const SERVE: &str = "fn serve(f: take File) -> u64 <fs, task> {
  test_ready(1)
  close_file(take f)
  3
}
";

/// The group ends before the child it started has run, so the file that its
/// lambda was given is closed by the child's first check, once, and the child
/// does nothing else.
fn a_task_cancelled_before_it_ran_closes_the_file_its_lambda_was_given() {
    let source = program(SERVE, "  f = file(9)\n  _ = spawn { serve(take f) }")
            .replace("  late = test_ready_by(99, ms(10))\n", "");
    for (seed, said) in said_for(&source, 8).into_iter().enumerate() {
        assert_eq!(sorted(said), ["1", "closed 9"], "seed {seed}");
    }
}

/// A task that is never asked to end, which waits for its handle and then closes
/// it itself, closes it once: the handler is asked once, and by the function.
fn a_file_that_is_closed_by_its_function_is_not_closed_again_when_no_task_is_asked_to_end() {
    let source = program(HOLD, "  _ = spawn { hold(4) }")
        .replace("test_scheduler_with_io(seed, [])", "test_scheduler_with_io(seed, [4])");
    assert_eq!(sorted(said(&source)), ["1", "closed 4"]);
}

/// A task that runs out of memory while it holds a file ends, and the handler
/// closes the file once, two calls deep as well, and its parent is told.
fn a_file_a_task_holds_when_it_fails_is_closed_by_the_handler_once() {
    let functions = "fn eat() -> u64 <alloc> {
  mut xs: List[u64] = []
  loop i in 0..9223372036854775807 {
    xs = push(take xs, i)
  }
  0
}

fn inner(n: u64) -> u64 <fs, alloc> {
  f = file(n)
  x = eat()
  close_file(take f)
  x
}

fn outer(n: u64) -> u64 <fs, alloc> {
  f = file(n)
  x = inner(wrapping_add(n, 1))
  close_file(take f)
  x
}
";
    let tasks = "  child = spawn { outer(5) }
  match outcome(take child) {
    .ok(n) { print(\"a value\") }
    .err(.out_of_memory) { print(\"out of memory\") }
    .err(.refused_write) { print(\"refused write\") }
  }
";
    let source = program(functions, tasks)
        .replace("  late = test_ready_by(99, ms(10))\n", "")
        .replace("use std.task (join,", "use std.task (outcome, join,")
        .replace("test_scheduler_with_io(seed, [])", "test_scheduler(3)")
        .replace("test_ready_by, test_scheduler_with_io)", "test_ready_by, test_scheduler_with_io, test_scheduler)")
        .replace("fn run(seed: u64) -> u64 <fs, diverge, alloc>", "fn run(seed: u64) -> u64 <fs, diverge, alloc, console>");
    // The function that failed ends what it owns first, and then its caller.
    assert_eq!(
        said(&source),
        ["closed 6", "closed 5", "out of memory", "1"]
    );
}

/// Under `memory_fs` the closing is the handler's `release`, which a file of
/// the memory ends without a system call, so a cancelled task leaves no file
/// open and the program ends.
fn a_cancelled_task_closes_the_file_of_memory_fs_as_release() {
    let source = "use std.bits (u64)
use std.console (console, print)
use std.fs (MemoryFile, fs, memory_fs, open_file, close_file)
use std.instant (ms)
use std.task (spawn, task, test_ready, test_ready_by, test_scheduler_with_io)

fn hold() -> u64 <fs, task, alloc> {
  match open_file(\"a\") {
    .ok(f) {
      test_ready(5)
      close_file(take f)
      1
    }
    .err(e) { 0 }
  }
}

fn run() -> u64 <fs, diverge, alloc> {
  with test_scheduler_with_io(0, [])
  _ = spawn { hold() }
  late = test_ready_by(99, ms(10))
  1
}

fn main() <console, alloc, diverge, fs> {
  with memory_fs([MemoryFile(path: \"a\", bytes: [1])])
  print(\"{run()}\")
}
";
    assert_eq!(said(source), ["1"]);
}
