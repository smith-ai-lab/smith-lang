//! `std.fs` reads and writes files by the system's own calls on Linux, and over
//! files kept in memory under `memory_fs`, with the same source and the same
//! answers.
//!
//! The programs that the tests run are one text that a test fills with lines: a
//! test of what must be true of files runs it under `memory_fs`, in the
//! evaluator for the small sizes, and natively, on Linux, for every size, and
//! under `memory_fs` natively for the large ones, and compares what it printed
//! with what the test works out in Rust from the numbers it chose.
//!
//! The tests that make pipes and read modes run where the system has them, which
//! is Unix. Windows has no mode that makes a file one to run, and builds no wait
//! for a descriptor, so a file of the kind that waits is made on Unix alone.

#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::process::Command;
#[cfg(unix)]
use std::process::Stdio;

use smith::diag::Error;
use smith::target::Target;
use smith::{check, lower, scalar};

use crate::Test;
use crate::arguments::output_of;
use crate::generate::{self, Gen};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "a_copy_of_a_file_has_the_bytes_of_the_original_under_memory_fs",
        a_copy_of_a_file_has_the_bytes_of_the_original_under_memory_fs,
    ),
    (
        "a_header_and_a_read_past_the_end_give_the_bytes_there_are_under_memory_fs",
        a_header_and_a_read_past_the_end_give_the_bytes_there_are_under_memory_fs,
    ),
    (
        "a_file_that_is_not_there_is_missing_under_memory_fs",
        a_file_that_is_not_there_is_missing_under_memory_fs,
    ),
    (
        "a_handler_that_refuses_gives_denied_and_failed_as_values_and_no_operation_ends_the_program",
        a_handler_that_refuses_gives_denied_and_failed_as_values_and_no_operation_ends_the_program,
    ),
    (
        "each_case_of_an_fs_error_is_written_as_its_path_and_what_went_wrong",
        each_case_of_an_fs_error_is_written_as_its_path_and_what_went_wrong,
    ),
    (
        "a_file_that_is_not_closed_or_is_closed_twice_or_is_used_after_it_is_closed_does_not_compile",
        a_file_that_is_not_closed_or_is_closed_twice_or_is_used_after_it_is_closed_does_not_compile,
    ),
    (
        "a_function_that_holds_a_file_while_it_may_wait_and_does_not_carry_fs_is_refused",
        a_function_that_holds_a_file_while_it_may_wait_and_does_not_carry_fs_is_refused,
    ),
    (
        "every_target_but_one_builds_a_program_that_carries_fs_and_that_one_refuses_it_naming_itself",
        every_target_but_one_builds_a_program_that_carries_fs_and_that_one_refuses_it_naming_itself,
    ),
    (
        "a_program_that_carries_no_fs_asks_the_kernel_for_no_call_of_files",
        a_program_that_carries_no_fs_asks_the_kernel_for_no_call_of_files,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "a_copy_of_a_file_has_the_bytes_of_the_original_natively",
        a_copy_of_a_file_has_the_bytes_of_the_original_natively,
    ),
    (
        "a_header_and_a_read_past_the_end_give_the_bytes_there_are_natively",
        a_header_and_a_read_past_the_end_give_the_bytes_there_are_natively,
    ),
    (
        "a_file_that_is_not_there_a_directory_and_a_file_that_may_not_be_read_come_back_as_values_natively",
        a_file_that_is_not_there_a_directory_and_a_file_that_may_not_be_read_come_back_as_values_natively,
    ),
    (
        "a_main_that_returns_an_fs_error_with_try_says_what_failed_and_ends_with_status_1",
        a_main_that_returns_an_fs_error_with_try_says_what_failed_and_ends_with_status_1,
    ),
    (
        "an_error_of_an_operation_on_a_file_names_the_path_the_file_was_opened_with",
        an_error_of_an_operation_on_a_file_names_the_path_the_file_was_opened_with,
    ),
    (
        "a_file_written_as_executable_is_one_the_system_runs",
        a_file_written_as_executable_is_one_the_system_runs,
    ),
    (
        "a_task_that_waits_to_read_a_file_suspends_only_itself",
        a_task_that_waits_to_read_a_file_suspends_only_itself,
    ),
];

/// The sizes the evaluator runs, which cross the 64 bytes of a header.
const SMALL: &[usize] = &[0, 1, 2, 3, 63, 64, 65, 70];

/// The sizes a native program runs, which cross a page, and a million bytes.
const LARGE: &[usize] = &[0, 1, 63, 64, 65, 4095, 4096, 4097, 100000, 1000000];

/// The program, with the lines of `body` as what `run` does. Where the files
/// are is `dir`, the first part of every path the body writes as `DIR`.
fn program(body: &str, dir: &str, memory: bool) -> String {
    // Windows reads a path written with `/` as it reads one written with `\\`.
    let body = body.replace("DIR", &dir.replace('\\', "/"));
    let main = if memory {
        "fn main() <console, alloc> {\n  with memory_fs([])\n  run()\n}\n"
    } else {
        "fn main() <console, alloc, fs> {\n  run()\n}\n"
    };
    format!(
        "use std.bits (bit_and)
use std.console (console, print)
use std.fs (FsError, MemoryFile, create, create_file, fs, memory_fs, open, read, read_all, size, write, write_all)
use std.list (push)

fn byte_at(i: Nat) -> Byte {{
  low = bit_and(i, 255)
  bit_and(low * 7 + 3, 255)
}}

fn content(n: Nat) -> List[Byte] <alloc> {{
  mut made: List[Byte] = []
  loop i in 0..n {{ made = push(made, byte_at(i)) }}
  made
}}

fn head(path: str) -> Result[List[Byte], FsError] <fs, alloc> {{
  with f = open(path)
  read(f, 0..64)
}}

fn window(path: str, from: Nat, to: Nat) -> Result[List[Byte], FsError] <fs, alloc>
  requires from <= to
{{
  with f = open(path)
  read(f, from..to)
}}

fn copy(from: str, to: str) -> Result[(), FsError] <fs, alloc> {{
  bytes = try read_all(from)
  write_all(to, bytes)
}}

fn kind(e: FsError) -> str {{
  match e {{
    .missing(path) {{ \"missing\" }}
    .denied(path) {{ \"denied\" }}
    .failed(why) {{ \"failed\" }}
  }}
}}

fn run() <console, alloc, fs> {{
{body}}}

{main}"
    )
}

fn lowered(source: &str) -> scalar::Program {
    lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"))
}

/// What the program prints in the evaluator.
fn evaluated(source: &str) -> String {
    let out = scalar::run(&lowered(source));
    String::from_utf8(out.stdout).unwrap()
}

/// Whether the target builds a program that carries `fs`.
fn builds_files(target: Target) -> bool {
    let source = program("  print(\"a\")\n  match read_all(\"a\") {\n    .ok(_) { }\n    .err(_) { }\n  }\n", "d", false);
    let lowered = lowered(&source);
    std::panic::catch_unwind(|| target.emit(&lowered)).is_ok()
}

/// The targets that build files and that this machine can run.
fn native_targets() -> Vec<Target> {
    targets::runnable().into_iter().filter(|t| builds_files(*t)).collect()
}

/// What the program prints as an executable, for each target that builds files.
fn native(source: &str) -> Vec<String> {
    let mut all = Vec::new();
    for target in native_targets() {
        let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?} for {source}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let out = output_of(&bytes, &[]);
        assert!(
            out.status.success(),
            "{} ended with {} and said {:?}",
            target.name(),
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        all.push(String::from_utf8(out.stdout).unwrap());
    }
    all
}

/// A folder that nothing is in, for a program that writes files.
fn folder() -> std::path::PathBuf {
    let dir = crate::scratch::path("fs");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sizes_list(sizes: &[usize]) -> String {
    let each: Vec<String> = sizes.iter().map(|n| n.to_string()).collect();
    format!("[{}]", each.join(", "))
}

/// Writes a file of `n` bytes, copies it, and says whether the copy reads the
/// same bytes as the original, for each size.
fn copy_lines(sizes: &[usize]) -> String {
    format!(
        "  sizes: List[Nat] = {}
  loop n in sizes {{
    data = content(n)
    match write_all(\"DIR/a\", data) {{
      .ok(_) {{ }}
      .err(e) {{ print(\"write {{kind(e)}}\") }}
    }}
    match copy(\"DIR/a\", \"DIR/b\") {{
      .ok(_) {{ }}
      .err(e) {{ print(\"copy {{kind(e)}}\") }}
    }}
    match read_all(\"DIR/b\") {{
      .ok(got) {{ print(\"{{n}} {{got.size}} {{got == data}}\") }}
      .err(e) {{ print(\"read {{kind(e)}}\") }}
    }}
  }}
",
        sizes_list(sizes)
    )
}

fn copy_expected(sizes: &[usize]) -> String {
    sizes.iter().map(|n| format!("{n} {n} true\n")).collect()
}

fn a_copy_of_a_file_has_the_bytes_of_the_original_under_memory_fs() {
    let source = program(&copy_lines(SMALL), "m", true);
    assert_eq!(evaluated(&source), copy_expected(SMALL), "for {source}");
}

fn a_copy_of_a_file_has_the_bytes_of_the_original_natively() {
    let dir = folder();
    let natively = program(&copy_lines(LARGE), dir.to_str().unwrap(), false);
    for said in native(&natively) {
        assert_eq!(said, copy_expected(LARGE));
    }
    // The same source under `memory_fs`, built as an executable.
    let kept = program(&copy_lines(LARGE), "m", true);
    for said in native(&kept) {
        assert_eq!(said, copy_expected(LARGE));
    }
    std::fs::remove_dir_all(dir).ok();
}

/// The ranges a read is asked for, of a file of `n` bytes.
fn windows(n: usize) -> Vec<(usize, usize)> {
    vec![
        (0, 64),
        (0, 0),
        (3, 10),
        (n / 2, n + 10),
        (n, n + 5),
        (n + 100, n + 200),
        (n.saturating_sub(1), n + 1),
    ]
}

/// A line for each file size and range: what `head` and `window` gave.
fn window_lines(sizes: &[usize]) -> String {
    let mut lines = format!("  sizes: List[Nat] = {}\n  loop n in sizes {{\n    data = content(n)\n    match write_all(\"DIR/a\", data) {{\n      .ok(_) {{ }}\n      .err(e) {{ print(\"write {{kind(e)}}\") }}\n    }}\n", sizes_list(sizes));
    lines.push_str("    match head(\"DIR/a\") {\n      .ok(h) { print(\"head {n} {h.size}\") }\n      .err(e) { print(\"head {kind(e)}\") }\n    }\n  }\n");
    for n in sizes {
        for (from, to) in windows(*n) {
            // Nothing of an empty file is checked, and the comparison would be
            // one that no number can change.
            let check = if *n == 0 {
                String::new()
            } else {
                format!("        if {from} + k < {n} {{\n          if w[k] != byte_at({from} + k) {{ same = false }}\n        }}\n")
            };
            lines.push_str(&format!(
                "  match write_all(\"DIR/w{n}\", content({n})) {{\n    .ok(_) {{ }}\n    .err(e) {{ print(\"write {{kind(e)}}\") }}\n  }}\n  match window(\"DIR/w{n}\", {from}, {to}) {{\n    .ok(w) {{\n      mut same = true\n      loop k in 0..w.size {{\n{check}      }}\n      print(\"window {n} {from} {to} {{w.size}} {{same}}\")\n    }}\n    .err(e) {{ print(\"window {{kind(e)}}\") }}\n  }}\n"
            ));
        }
    }
    lines
}

fn window_expected(sizes: &[usize]) -> String {
    let mut said = String::new();
    for &n in sizes {
        said.push_str(&format!("head {n} {}\n", n.min(64)));
    }
    for &n in sizes {
        for (from, to) in windows(n) {
            let given = to.min(n).saturating_sub(from.min(n));
            said.push_str(&format!("window {n} {from} {to} {given} true\n"));
        }
    }
    said
}

fn a_header_and_a_read_past_the_end_give_the_bytes_there_are_under_memory_fs() {
    let source = program(&window_lines(SMALL), "m", true);
    assert_eq!(evaluated(&source), window_expected(SMALL), "for {source}");
}

fn a_header_and_a_read_past_the_end_give_the_bytes_there_are_natively() {
    let sizes = [0, 1, 63, 64, 65, 4096, 100000];
    let dir = folder();
    let natively = program(&window_lines(&sizes), dir.to_str().unwrap(), false);
    for said in native(&natively) {
        assert_eq!(said, window_expected(&sizes));
    }
    std::fs::remove_dir_all(dir).ok();
}

fn a_file_that_is_not_there_is_missing_under_memory_fs() {
    let body = "  match read_all(\"m/nothing\") {\n    .ok(_) { print(\"read\") }\n    .err(e) { print(kind(e)) }\n  }\n  match open_it(\"m/nothing\") {\n    .ok(_) { print(\"opened\") }\n    .err(e) { print(kind(e)) }\n  }\n";
    let source = program(body, "m", true).replace(
        "fn kind(",
        "fn open_it(path: str) -> Result[(), FsError] <fs> {\n  with f = open(path)\n  .ok(())\n}\n\nfn kind(",
    );
    assert_eq!(evaluated(&source), "missing\nmissing\n", "for {source}");
}

/// A handler that refuses every operation, one of the ways each time, and a
/// program that goes on after each refusal.
fn a_handler_that_refuses_gives_denied_and_failed_as_values_and_no_operation_ends_the_program() {
    let source = "use std.console (console, print)
use std.fs (FsError, fs, read_all, release, write_all)

fn kind(e: FsError) -> str {
  match e {
    .missing(path) { \"missing\" }
    .denied(path) { \"denied\" }
    .failed((path, why)) { why }
  }
}

fn tell(r: Result[List[Byte], FsError]) <console, alloc> {
  match r {
    .ok(_) { print(\"ok\") }
    .err(e) { print(kind(e)) }
  }
}

fn refusing[T](body: fn() -> T <fs>) -> T {
  fs(
    open_file: fn(path) { .err(.denied(path)) },
    create_file: fn(path, executable) { .err(.failed((path, \"disk\"))) },
    close_file: fn(f) { n = release(f) },
    size: fn(f) { .err(.missing(f.path)) },
    read: fn(f, range) { .err(.missing(f.path)) },
    write: fn(f, at, bytes) { .err(.denied(f.path)) },
    body: body,
  )
}

fn main() <console, alloc> {
  with refusing()
  tell(read_all(\"a\"))
  match write_all(\"a\", [1]) {
    .ok(_) { print(\"ok\") }
    .err(e) { print(kind(e)) }
  }
  print(\"after\")
}
";
    assert_eq!(evaluated(source), "denied\ndisk\nafter\n");
}

/// Each case written by its `Fmt`: the path, and then what went wrong.
fn each_case_of_an_fs_error_is_written_as_its_path_and_what_went_wrong() {
    let source = "use std.console (console, print)
use std.fs (FsError)

fn main() <console, alloc> {
  errors: List[FsError] = [
    .missing(\"missing.txt\"),
    .denied(\"secret.key\"),
    .failed((\"data.bin\", \"read: error 5\")),
  ]
  loop i in 0..errors.size { print(\"{errors[i]}\") }
}
";
    assert_eq!(
        evaluated(source),
        "missing.txt: no such file\nsecret.key: permission denied\ndata.bin: read: error 5\n"
    );
}

/// A `main` that returns the error of `read_all` with `try` writes what failed
/// to standard error, and ends with status 1.
fn a_main_that_returns_an_fs_error_with_try_says_what_failed_and_ends_with_status_1() {
    let dir = folder();
    let d = dir.to_str().unwrap().replace('\\', "/");
    let source = format!(
        "use std.console (console, print)
use std.fs (FsError, fs, read_all, write_all)

fn main() -> Result[(), FsError] <console, fs, alloc> {{
  bytes = try read_all(\"{d}/photo.jpg\")
  try write_all(\"{d}/photo-copy.jpg\", bytes)
  print(\"copied {{bytes.size}} bytes\")
  .ok(())
}}
"
    );
    for target in native_targets() {
        let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?} for {source}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let out = output_of(&bytes, &[]);
        assert_eq!(out.status.code(), Some(1), "{}", target.name());
        assert_eq!(String::from_utf8_lossy(&out.stdout), "", "{}", target.name());
        assert_eq!(
            String::from_utf8_lossy(&out.stderr).trim_end(),
            format!("{d}/photo.jpg: no such file"),
            "{}",
            target.name()
        );
    }
    std::fs::remove_dir_all(dir).ok();
}

/// A call on a `File` is not given the path, so the path that its error names is
/// the one the `File` keeps from `open`.
fn an_error_of_an_operation_on_a_file_names_the_path_the_file_was_opened_with() {
    let dir = folder();
    let d = dir.to_str().unwrap().replace('\\', "/");
    std::fs::write(dir.join("data.bin"), b"abc").unwrap();
    // A file that was opened to be read is refused a write.
    let source = format!(
        "use std.console (console, print)
use std.fs (FsError, fs, open, write)

fn main() -> Result[(), FsError] <console, fs, alloc> {{
  with f = open(\"{d}/data.bin\")
  match write(f, 0, [1]) {{
    .ok(_) {{ print(\"written\") }}
    .err(e) {{ print(\"{{e}}\") }}
  }}
  .ok(())
}}
"
    );
    for target in native_targets() {
        let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?} for {source}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let out = output_of(&bytes, &[]);
        let said = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
        assert!(said.starts_with(&format!("{d}/data.bin: ")), "{}: {said}", target.name());
        assert_ne!(said, format!("{d}/data.bin: "), "{}", target.name());
    }
    std::fs::remove_dir_all(dir).ok();
}

/// What a program that opens a path and says what happened prints natively.
fn a_file_that_is_not_there_a_directory_and_a_file_that_may_not_be_read_come_back_as_values_natively() {
    let dir = folder();
    let d = dir.to_str().unwrap();
    std::fs::create_dir(dir.join("nodir")).unwrap();
    // A file with no mode to read it is made where the system has modes, and a
    // user that may read every file is refused nothing, so there is no denial to see.
    let locked = make_locked(&dir);
    let mut body = "  match read_all(\"DIR/nothing\") {
    .ok(_) { print(\"read\") }
    .err(e) { print(kind(e)) }
  }
  match write_all(\"DIR/nowhere/file\", [1]) {
    .ok(_) { print(\"written\") }
    .err(e) { print(kind(e)) }
  }
  match read_all(\"DIR/nodir\") {
    .ok(_) { print(\"read\") }
    .err(e) { print(kind(e)) }
  }
"
    .to_string();
    let mut expected = "missing\nmissing\nfailed\n".to_string();
    if let Some(reads_everything) = locked {
        body.push_str(
            "  match read_all(\"DIR/locked\") {
    .ok(_) { print(\"read\") }
    .err(e) { print(kind(e)) }
  }
",
        );
        expected.push_str(if reads_everything { "read\n" } else { "denied\n" });
    }
    body.push_str("  print(\"after\")\n");
    expected.push_str("after\n");
    let source = program(&body, d, false);
    for said in native(&source) {
        assert_eq!(said, expected);
    }
    unlock(&dir);
    std::fs::remove_dir_all(dir).ok();
}

/// Makes a file nobody may read, and says whether this user reads it all the
/// same, on a system that has modes, and gives nothing on one that has none.
#[cfg(unix)]
fn make_locked(dir: &std::path::Path) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(dir.join("locked"), b"secret").unwrap();
    std::fs::set_permissions(dir.join("locked"), std::fs::Permissions::from_mode(0o000)).unwrap();
    Some(std::fs::File::open(dir.join("locked")).is_ok())
}

#[cfg(not(unix))]
fn make_locked(_: &std::path::Path) -> Option<bool> {
    None
}

#[cfg(unix)]
fn unlock(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir.join("locked"), std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn unlock(_: &std::path::Path) {}

fn a_file_written_as_executable_is_one_the_system_runs() {
    let dir = folder();
    let d = dir.to_str().unwrap();
    // A script the system runs: a shell script on Unix, which needs its mode, and a
    // batch file on Windows, which is run by its name.
    let (script, name) = if cfg!(windows) {
        ("@echo ran\\n", "run.cmd")
    } else {
        ("#!/bin/sh\\necho ran\\n", "run")
    };
    let body = format!(
        "  match write_all(\"DIR/plain\", \"{script}\".bytes) {{
    .ok(_) {{ print(\"plain\") }}
    .err(e) {{ print(kind(e)) }}
  }}
  match write_all(\"DIR/{name}\", \"{script}\".bytes, executable: true) {{
    .ok(_) {{ print(\"run\") }}
    .err(e) {{ print(kind(e)) }}
  }}
  match write_all(\"DIR/plain\", \"{script}\".bytes, executable: true) {{
    .ok(_) {{ print(\"again\") }}
    .err(e) {{ print(kind(e)) }}
  }}
"
    );
    let source = program(&body, d, false);
    for said in native(&source) {
        assert_eq!(said, "plain\nrun\nagain\n");
        let ran = Command::new(dir.join(name)).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&ran.stdout).trim_end(), "ran");
        // A file that exists and is written again as one to run becomes one.
        assert_runs_as_a_program(&dir.join("plain"));
        std::fs::remove_file(dir.join(name)).ok();
        std::fs::remove_file(dir.join("plain")).ok();
    }
    std::fs::remove_dir_all(dir).ok();
}

#[cfg(unix)]
fn assert_runs_as_a_program(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_ne!(mode & 0o111, 0, "{mode:o}");
}

/// A file of Windows has no mode, and is run by its name.
#[cfg(not(unix))]
fn assert_runs_as_a_program(path: &std::path::Path) {
    assert!(path.exists());
}

/// A file that has no places, as a pipe has none, is read from where it is, and
/// a task that waits for it lets the others run: the other task sleeps and
/// says so while the first waits, and the program ends only when the first has
/// its bytes, so it would never end if the wait held the thread.
#[cfg(not(unix))]
fn a_task_that_waits_to_read_a_file_suspends_only_itself() {
    // A task that waits for a descriptor is not built for Windows, and it has no
    // pipe made by name that a file is opened from.
}

#[cfg(unix)]
fn a_task_that_waits_to_read_a_file_suspends_only_itself() {
    use std::os::unix::fs::OpenOptionsExt;
    let source = "use std.console (console, print)
use std.fs (FsError, fs, open, read)
use std.instant (ms)
use std.task (join, spawn, task, tasks)
use std.time (sleep, time)

fn pull(path: str) -> Result[Nat, FsError] <fs, alloc, console> {
  with f = open(path)
  print(\"waiting\")
  bytes = try read(f, 0..5)
  .ok(bytes.size)
}

fn main() <console, alloc, fs, time> {
  with tasks()
  a = spawn { pull(\"PIPE\") }
  b = spawn {
    sleep(ms(50))
    print(\"other ran\")
    7
  }
  match join(a) {
    .ok(n) { print(\"got {n}\") }
    .err(_) { print(\"failed\") }
  }
  print(\"{join(b)}\")
}
";
    let dir = folder();
    let pipe = dir.join("pipe");
    let made = Command::new("mkfifo").arg(&pipe).status();
    if !matches!(made, Ok(s) if s.success()) {
        eprintln!("skipped: no mkfifo");
        return;
    }
    let source = source.replace("PIPE", pipe.to_str().unwrap());
    for target in native_targets() {
        let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let program = dir.join(format!("program-{}", target.name()));
        crate::executable::write(&program, &bytes);
        // Held open for reading and writing, so that the program's read finds a
        // writer and has to wait, and nothing blocks the test.
        let mut held = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(0)
            .open(&pipe)
            .unwrap();
        let mut child = Command::new(&program).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap());
        let mut first = String::new();
        let mut second = String::new();
        lines.read_line(&mut first).unwrap();
        lines.read_line(&mut second).unwrap();
        assert_eq!(first.trim_end(), "waiting");
        assert_eq!(second.trim_end(), "other ran");
        held.write_all(b"hello").unwrap();
        let mut rest = String::new();
        while lines.read_line(&mut rest).unwrap() > 0 {}
        let status = child.wait().unwrap();
        assert!(status.success(), "{} ended with {status}", target.name());
        assert_eq!(rest, "got 5\n7\n", "{}", target.name());
    }
    std::fs::remove_dir_all(dir).ok();
}

fn errors(source: &str) -> Vec<Error> {
    match check(source.as_bytes()) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

/// A function that opens a file and then runs the lines.
fn opened(lines: &str) -> String {
    format!(
        "use std.fs (File, FsError, close_file, fs, open_file, read, size)

fn use_it(path: str) -> Result[(), FsError] <fs, alloc> {{
  value = try open_file(path)
{lines}  .ok(())
}}

fn main() <fs, alloc> {{
  match use_it(\"a\") {{
    .ok(_) {{ }}
    .err(_) {{ }}
  }}
}}
"
    )
}

fn a_file_that_is_not_closed_or_is_closed_twice_or_is_used_after_it_is_closed_does_not_compile() {
    let uses = "  match size(value) {\n    .ok(_) { }\n    .err(_) { }\n  }\n";
    let ended = "  close_file(value)\n";
    let once = errors(&opened(&format!("{uses}{ended}")));
    assert!(once.is_empty(), "closed once: {once:?}");
    let not_ended = errors(&opened(uses));
    assert!(not_ended.iter().any(|e| e.message.contains("File")), "not closed: {not_ended:?}");
    assert!(!errors(&opened(&format!("{ended}{ended}"))).is_empty(), "closed twice");
    assert!(!errors(&opened(&format!("{ended}{uses}"))).is_empty(), "used after it is closed");
}

fn a_function_that_holds_a_file_while_it_may_wait_and_does_not_carry_fs_is_refused() {
    let waiting = |effects: &str| {
        format!(
            "use std.fs (File, close_file, file, fs)
use std.instant (ms)
use std.time (sleep, time)

fn pause(f: take File) -> File <{effects}> {{
  sleep(ms(10))
  f
}}

fn main() <fs, time> {{
  close_file(pause(file(3)))
}}
"
        )
    };
    let refused = match lower(waiting("time").as_bytes()) {
        Ok(_) => panic!("a program that is refused was built"),
        Err(errors) => format!("{errors:?}"),
    };
    assert!(
        refused.contains("`pause` holds a `File` while it may wait, so it must carry `fs`"),
        "{refused}"
    );
    assert!(refused.contains("add `fs` to its effects"), "{refused}");
    // It carries `fs` and is accepted.
    lowered(&waiting("fs, time"));
}

fn every_target_but_one_builds_a_program_that_carries_fs_and_that_one_refuses_it_naming_itself() {
    let source = program("  match read_all(\"a\") {\n    .ok(_) { print(\"ok\") }\n    .err(e) { print(kind(e)) }\n  }\n", "d", false);
    let lowered = lowered(&source);
    assert!(lowered.asks_the_files());
    let mut refused = Vec::new();
    for &target in Target::ALL.iter() {
        if let Err(why) = std::panic::catch_unwind(|| target.emit(&lowered)) {
            let message = why.downcast_ref::<String>().cloned().unwrap_or_default();
            assert!(message.contains(target.name()), "{message}");
            assert!(message.contains("fs"), "{message}");
            refused.push(target);
        }
    }
    assert_eq!(refused.len(), 1, "{refused:?}");
}

fn a_program_that_carries_no_fs_asks_the_kernel_for_no_call_of_files() {
    generate::check(
        8821,
        20,
        |g: &mut Gen| g.below(50),
        |&n| {
            let source = format!("use std.console (console, print)\n\nfn main() <console, alloc> {{\n  print(\"{{{n} + 1}}\")\n}}\n");
            assert!(!lowered(&source).asks_the_files(), "{source}");
        },
    );
    assert!(!lowered(include_str!("net_program.smith")).asks_the_files());
    assert!(!lowered(include_str!("time_program.smith")).asks_the_files());
}
