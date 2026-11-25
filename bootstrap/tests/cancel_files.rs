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
fn program(functions: &str, tasks: &str, seed: u64) -> String {
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.int (wrapping_add)
use std.fs (close_file, file, fs, handle, release, File)
use std.list (freeze, push)
use std.task (join, spawn, task, test_ready, test_ready_by, test_scheduler_with_io)

{functions}
fn run() -> u64 <fs, diverge, alloc> {{
  with test_scheduler_with_io({seed}, [])
{tasks}
  late = test_ready_by(99, 10)
  1
}}

fn main() <console, alloc, diverge> {{
  r = fs(
    open_file: fn(path) {{ .err(.missing(path)) }},
    create_file: fn(path, executable) {{ .err(.missing(path)) }},
    close_file: fn(f) {{
      print(\"closed {{handle(f)}}\")
      n = release(f)
    }},
    size: fn(f) {{ .err(.missing(f.path)) }},
    read: fn(f, range) {{ .err(.missing(f.path)) }},
    write: fn(f, at, bytes) {{ .err(.missing(f.path)) }},
    body: {{ run() }},
  )
  print(\"{{r}}\")
}}
"
    )
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
  close_file(f)
  n
}
";

fn a_cancelled_task_closes_the_file_it_holds_through_the_handler_in_force_once() {
    for seed in 0..8 {
        let source = program(HOLD, "  spawn { hold(5) }\n  spawn { hold(6) }", seed);
        assert_eq!(
            sorted(said(&source)),
            ["1", "closed 5", "closed 6"],
            "seed {seed}"
        );
    }
}

const PAIR: &str = "struct Pair {
  a: File
  b: File
}

fn hold_two(n: u64) -> u64 <fs, task> {
  p = Pair(a: file(n), b: file(wrapping_add(n, 1)))
  test_ready(n)
  close_file(p.a)
  close_file(p.b)
  n
}
";

fn a_cancelled_task_closes_both_files_of_a_struct_it_holds() {
    for seed in 0..8 {
        let source = program(PAIR, "  spawn { hold_two(10) }", seed);
        assert_eq!(
            sorted(said(&source)),
            ["1", "closed 10", "closed 11"],
            "seed {seed}"
        );
    }
}

const LIST: &str = "fn hold_list(n: u64) -> List[File] <fs, task, alloc> {
  var fs: List[File] = []
  fs = push(fs, file(n))
  fs = push(fs, file(wrapping_add(n, 1)))
  test_ready(n)
  fs
}
";

fn a_cancelled_task_closes_each_file_of_a_list_it_holds() {
    for seed in 0..8 {
        let source = program(LIST, "  t = spawn { hold_list(20) }", seed);
        assert_eq!(
            sorted(said(&source)),
            ["1", "closed 20", "closed 21"],
            "seed {seed}"
        );
    }
}

const SERVE: &str = "fn serve(f: take File) -> u64 <fs, task> {
  test_ready(1)
  close_file(f)
  3
}
";

/// The group ends before the child it started has run, so the file that its
/// lambda was given is closed by the child's first check, once, and the child
/// does nothing else.
fn a_task_cancelled_before_it_ran_closes_the_file_its_lambda_was_given() {
    for seed in 0..8 {
        let source = program(SERVE, "  f = file(9)\n  spawn { serve(f) }", seed)
            .replace("  late = test_ready_by(99, 10)\n", "");
        assert_eq!(sorted(said(&source)), ["1", "closed 9"], "seed {seed}");
    }
}

/// A task that is never asked to end, which waits for its handle and then closes
/// it itself, closes it once: the handler is asked once, and by the function.
fn a_file_that_is_closed_by_its_function_is_not_closed_again_when_no_task_is_asked_to_end() {
    let source = program(HOLD, "  spawn { hold(4) }", 0)
        .replace("test_scheduler_with_io(0, [])", "test_scheduler_with_io(0, [4])");
    assert_eq!(sorted(said(&source)), ["1", "closed 4"]);
}

/// A task that runs out of memory while it holds a file ends, and the handler
/// closes the file once, two calls deep as well, and its parent is told.
fn a_file_a_task_holds_when_it_fails_is_closed_by_the_handler_once() {
    let functions = "fn eat() -> u64 <alloc> {
  var xs: List[u64] = []
  loop i in 0..9223372036854775807 {
    xs = push(xs, i)
  }
  0
}

fn inner(n: u64) -> u64 <fs, alloc> {
  f = file(n)
  x = eat()
  close_file(f)
  x
}

fn outer(n: u64) -> u64 <fs, alloc> {
  f = file(n)
  x = inner(wrapping_add(n, 1))
  close_file(f)
  x
}
";
    let tasks = "  child = spawn { outer(5) }
  match outcome(child) {
    .ok(n) { print(\"a value\") }
    .err(.out_of_memory) { print(\"out of memory\") }
    .err(.refused_write) { print(\"refused write\") }
  }
";
    let source = program(functions, tasks, 3)
        .replace("  late = test_ready_by(99, 10)\n", "")
        .replace("use std.task (join,", "use std.task (outcome, join,")
        .replace("test_scheduler_with_io(3, [])", "test_scheduler(3)")
        .replace("test_ready_by, test_scheduler_with_io)", "test_ready_by, test_scheduler_with_io, test_scheduler)")
        .replace("fn run() -> u64 <fs, diverge, alloc>", "fn run() -> u64 <fs, diverge, alloc, console>");
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
use std.task (spawn, task, test_ready, test_ready_by, test_scheduler_with_io)

fn hold() -> u64 <fs, task, alloc> {
  match open_file(\"a\") {
    .ok(f) {
      test_ready(5)
      close_file(f)
      1
    }
    .err(e) { 0 }
  }
}

fn run() -> u64 <fs, diverge, alloc> {
  with test_scheduler_with_io(0, [])
  spawn { hold() }
  late = test_ready_by(99, 10)
  1
}

fn main() <console, alloc, diverge, fs> {
  with memory_fs([MemoryFile(path: \"a\", bytes: [1])])
  print(\"{run()}\")
}
";
    assert_eq!(said(source), ["1"]);
}
