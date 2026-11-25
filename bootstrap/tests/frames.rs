//! A lambda given to `__frame` is lowered as a function of its own, with the
//! names it reads passed in and kept in the frame, and `__frame`, `__poll`,
//! `__result`, and `__wait` are calls only the library makes. For lambdas that
//! read names of every kind of value, with an `if`, a loop, and calls of
//! functions that wait, a frame polled to its end in every order of readiness
//! gives what the same work gives behind a function with a name, and makes the
//! same waits in the same order, in the evaluator and as an executable.

use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets::{runnable, runs_as_in};

pub const ONCE: &[Test] = &[
    (
        "a_frame_of_a_lambda_that_reads_names_gives_what_the_same_work_gives_in_every_order_of_readiness",
        a_frame_of_a_lambda_that_reads_names_gives_what_the_same_work_gives_in_every_order_of_readiness,
    ),
    (
        "the_calls_only_the_library_makes_are_refused_in_a_program",
        the_calls_only_the_library_makes_are_refused_in_a_program,
    ),
    (
        "a_frame_read_at_a_type_its_lambda_does_not_give_is_refused",
        a_frame_read_at_a_type_its_lambda_does_not_give_is_refused,
    ),
    (
        "a_frame_that_finish_gives_for_a_number_that_is_not_a_tasks_is_refused",
        a_frame_that_finish_gives_for_a_number_that_is_not_a_tasks_is_refused,
    ),
    (
        "a_name_a_lambda_takes_moves_into_the_frame_and_a_second_use_is_refused",
        a_name_a_lambda_takes_moves_into_the_frame_and_a_second_use_is_refused,
    ),
    (
        "a_frame_under_a_handler_that_holds_state_reads_and_changes_it",
        a_frame_under_a_handler_that_holds_state_reads_and_changes_it,
    ),
    (
        "a_frame_under_a_handler_without_state_reads_what_the_handler_holds",
        a_frame_under_a_handler_without_state_reads_what_the_handler_holds,
    ),
    (
        "a_program_that_makes_no_frame_is_built_as_it_was",
        a_program_that_makes_no_frame_is_built_as_it_was,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_gives_what_the_evaluator_says_of_frames_polled_in_every_order",
    every_executable_gives_what_the_evaluator_says_of_frames_polled_in_every_order,
)];

/// The modules a program here is built with: the ones it is given, and those of
/// the standard library that they import.
fn with_std<'a>(library: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut all = library.to_vec();
    for module in ["bits", "console", "int", "task", "list", "float"] {
        if !all.iter().any(|(name, _)| *name == module) {
            all.push((module, smith::library_text(module)));
        }
    }
    all
}

/// What the module of the library gives a program of the tests: the work a
/// lambda is, the waits it makes, and the functions that poll frames.
const HEAD: &str = "use std.bits (u8, u16, u32, u64, i8, i16, i32, i64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul, wrapping_narrow, wrapping_rem)\nuse std.float (sqrt)\nuse std.task (Frame, Handle, Poll, Wait)\n\nstruct Pair {\n  a: u8\n  b: u32\n}\n\nenum Shape {\n  circle: u16\n  square: u64\n  dot\n}\n\nfn fold_i8(x: i8) -> u64 { wrapping_narrow(x) }\n\nfn fold_i16(x: i16) -> u64 { wrapping_narrow(x) }\n\nfn fold_i32(x: i32) -> u64 { wrapping_narrow(x) }\n\nfn fold_i64(x: i64) -> u64 { wrapping_narrow(x) }\n\nfn fold_u8(x: u8) -> u64 {\n  w: u64 = x\n  w\n}\n\nfn fold_u16(x: u16) -> u64 {\n  w: u64 = x\n  w\n}\n\nfn fold_u32(x: u32) -> u64 {\n  w: u64 = x\n  w\n}\n\nfn fold_u64(x: u64) -> u64 {\n  w: u64 = x\n  w\n}\n\nfn fold_bool(x: bool) -> u64 { if x { 7 } else { 9 } }\n\nfn fold_f32(x: f32) -> u64 {\n  half: f32 = 1.5\n  if x == half { 2 } else { 3 }\n}\n\nfn fold_f64(x: f64) -> u64 {\n  half: f64 = 2.5\n  if x == half { 2 } else { 3 }\n}\n\nfn fold_text(x: str) -> u64 {\n  n: u64 = x.size\n  n\n}\n\nfn fold_pair(x: Pair) -> u64 {\n  a: u64 = x.a\n  b: u64 = x.b\n  wrapping_add(a, b)\n}\n\nfn fold_shape(x: Shape) -> u64 {\n  match x {\n    .circle(r) {\n      wide: u64 = r\n      wide\n    }\n    .square(s) { s }\n    .dot { 11 }\n  }\n}\n\nfn fold_list(xs: take List[u64]) -> u64 {\n  var sum: u64 = 0\n  loop v in xs { sum = wrapping_add(sum, v) }\n  sum\n}\n\nfn step0(acc: u64) -> u64 {\n  __wait(.read(Handle(number: acc)))\n  wrapping_add(acc, 1)\n}\n\nfn step1(acc: u64) -> u64 {\n  __wait(.write(Handle(number: acc)))\n  wrapping_add(acc, 2)\n}\n\nfn step2(acc: u64) -> u64 {\n  __wait(.time(acc))\n  wrapping_add(acc, 3)\n}\n\nfn step3(acc: u64) -> u64 {\n  __wait(.task(acc))\n  wrapping_add(acc, 4)\n}\n\nfn mix(hash: u64, kind: u64, handle: u64) -> u64 {\n  wrapping_add(wrapping_mul(hash, 1000003), wrapping_add(wrapping_mul(kind, 1000000007), handle))\n}\n\nfn fold_wait(hash: u64, w: Wait) -> u64 {\n  match w {\n    .read(h) { mix(hash, 0, h.number) }\n    .write(h) { mix(hash, 1, h.number) }\n    .time(t) { mix(hash, 2, t) }\n    .task(t) { mix(hash, 3, t) }\n    .read_by(h) { mix(hash, 4, h.number) }\n    .write_by(h) { mix(hash, 5, h.number) }\n  }\n}\n\n";

/// A kind of value a lambda may read.
#[derive(Clone, Copy, Debug)]
enum Value {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    Bool,
    F32,
    F64,
    Text,
    Pair,
    Shape,
    /// A list that the lambda takes.
    List,
}

const VALUES: [Value; 15] = [
    Value::U8,
    Value::U16,
    Value::U32,
    Value::U64,
    Value::I8,
    Value::I16,
    Value::I32,
    Value::I64,
    Value::Bool,
    Value::F32,
    Value::F64,
    Value::Text,
    Value::Pair,
    Value::Shape,
    Value::List,
];

impl Value {
    /// The definition of a name of this kind, with a value the generator chose.
    fn define(self, name: &str, n: usize) -> String {
        let small = n % 100;
        match self {
            Value::U8 => format!("  {name}: u8 = {small}\n"),
            Value::U16 => format!("  {name}: u16 = {}\n", n % 60000),
            Value::U32 => format!("  {name}: u32 = {}\n", n * 1000),
            Value::U64 => format!("  {name}: u64 = {}\n", n * 1_000_000_007),
            Value::I8 => format!("  {name}: i8 = {}\n", small % 100),
            Value::I16 => format!("  {name}: i16 = {}\n", n % 30000),
            Value::I32 => format!("  {name}: i32 = {}\n", n * 1000),
            Value::I64 => format!("  {name}: i64 = {}\n", n * 1_000_000_007),
            Value::Bool => format!("  {name}: bool = {}\n", n.is_multiple_of(2)),
            Value::F32 => format!("  {name}: f32 = {}.5\n", n % 4),
            Value::F64 => format!("  {name}: f64 = {}.25\n", n % 6),
            Value::Text => format!("  {name} = \"{}\"\n", "x".repeat(n % 9)),
            Value::Pair => format!("  {name} = Pair(a: {small}, b: {})\n", n * 77),
            Value::Shape => match n % 3 {
                0 => format!("  {name} = Shape.circle({})\n", n % 50000),
                1 => format!("  {name} = Shape.square({})\n", n * 12345),
                _ => format!("  {name}: Shape = .dot\n"),
            },
            Value::List => format!("  {name}: List[u64] = [{}, {}, {}]\n", n, n + 1, n * 3),
        }
    }

    /// What the number this name is read as is, as an expression.
    fn fold(self, name: &str) -> String {
        match self {
            Value::U8 => format!("fold_u8({name})"),
            Value::U16 => format!("fold_u16({name})"),
            Value::U32 => format!("fold_u32({name})"),
            Value::U64 => format!("fold_u64({name})"),
            Value::I8 => format!("fold_i8({name})"),
            Value::I16 => format!("fold_i16({name})"),
            Value::I32 => format!("fold_i32({name})"),
            Value::I64 => format!("fold_i64({name})"),
            Value::Bool => format!("fold_bool({name})"),
            Value::F32 => format!("fold_f32({name})"),
            Value::F64 => format!("fold_f64({name})"),
            Value::Text => format!("fold_text({name})"),
            Value::Pair => format!("fold_pair({name})"),
            Value::Shape => format!("fold_shape({name})"),
            Value::List => format!("fold_list({name})"),
        }
    }
}

/// What a lambda does, in the order it does it, on one number `acc`.
#[derive(Clone, Debug)]
enum Work {
    /// Mixes in the name that is the one of the lambda's names at the place.
    Read(usize),
    /// Waits, and then goes on with the number the wait left.
    Wait(usize),
    If(u64, Vec<Work>, Vec<Work>),
    Loop(u64, Vec<Work>),
}

#[derive(Clone, Debug)]
struct Lambda {
    names: Vec<(Value, usize)>,
    work: Vec<Work>,
}

fn works(g: &mut Gen, names: usize, depth: usize) -> Vec<Work> {
    (0..g.below(4))
        .map(|_| match g.below(if depth == 0 { 3 } else { 5 }) {
            0 if names > 0 => Work::Read(g.below(names)),
            0..=2 => Work::Wait(g.below(4)),
            3 => Work::If(g.below(3) as u64, works(g, names, depth - 1), works(g, names, depth - 1)),
            _ => Work::Loop(1 + g.below(3) as u64, works(g, names, depth - 1)),
        })
        .collect()
}

fn lambda(g: &mut Gen) -> Lambda {
    let names: Vec<(Value, usize)> = (0..g.below(7))
        .map(|_| (*g.pick(&VALUES), g.below(1000)))
        .collect();
    let work = works(g, names.len(), 2);
    Lambda { names, work }
}

impl Lambda {
    /// The definitions of the names, which are the names of the function around
    /// the lambda.
    fn defines(&self, k: usize) -> String {
        (self.names.iter().enumerate())
            .map(|(i, (v, n))| v.define(&format!("k{k}n{i}"), *n))
            .collect()
    }

    /// The lambda's work as lines. Every name is read once first, so a list that
    /// is taken is taken once, and then the work reads the names that are not
    /// lists as often as it says.
    fn body(&self, k: usize, indent: &str) -> String {
        let mut s = format!("{indent}var acc: u64 = 17\n");
        for (i, (v, _)) in self.names.iter().enumerate() {
            s += &format!(
                "{indent}acc = wrapping_add(wrapping_mul(acc, 31), {})\n",
                v.fold(&format!("k{k}n{i}"))
            );
        }
        self.works(k, &self.work, indent, 0, &mut s);
        s += &format!("{indent}acc\n");
        s
    }

    fn works(&self, k: usize, work: &[Work], indent: &str, depth: usize, s: &mut String) {
        for w in work {
            match w {
                Work::Read(i) => {
                    let (v, _) = self.names[*i];
                    if !matches!(v, Value::List) {
                        *s += &format!(
                            "{indent}acc = wrapping_add(wrapping_mul(acc, 31), {})\n",
                            v.fold(&format!("k{k}n{i}"))
                        );
                    }
                }
                Work::Wait(k) => *s += &format!("{indent}acc = step{k}(acc)\n"),
                Work::If(m, then, otherwise) => {
                    *s += &format!("{indent}if wrapping_rem(acc, 3) == {m} {{\n");
                    self.works(k, then, &format!("{indent}  "), depth + 1, s);
                    *s += &format!("{indent}}} else {{\n");
                    self.works(k, otherwise, &format!("{indent}  "), depth + 1, s);
                    *s += &format!("{indent}}}\n");
                }
                Work::Loop(times, body) => {
                    *s += &format!("{indent}loop i{depth} in 0..{times} {{\n");
                    *s += &format!(
                        "{indent}  acc = wrapping_add(acc, wrapping_narrow(i{depth}))\n"
                    );
                    self.works(k, body, &format!("{indent}  "), depth + 1, s);
                    *s += &format!("{indent}}}\n");
                }
            }
        }
    }
}

/// The module of a case: the work of each lambda behind a function with a name,
/// the frame of the lambda itself, and the functions that poll each set of
/// frames in each order.
fn module_of(case: &[Lambda]) -> String {
    let mut s = String::from(HEAD);
    for (k, l) in case.iter().enumerate() {
        // The same work behind a function that is called, with no frame of its
        // own for what it reads.
        s += &format!("fn plain{k}() -> u64 <alloc> {{\n{}{}}}\n\n", l.defines(k), l.body(k, "  "));
    }
    for how in ["made", "reference"] {
        s += &format!("pub fn drive_{how}(order: u64) <console, alloc> {{\n");
        for (k, l) in case.iter().enumerate() {
            if how == "made" {
                s += &format!("{}  f{k} = __frame({{\n{}  }})\n", l.defines(k), l.body(k, "    "));
            } else {
                s += &format!("  f{k} = __frame({{ plain{k}() }})\n");
            }
            s += &format!("  var done{k}: u64 = 0\n  var hash{k}: u64 = 0\n");
        }
        let poll = |k: usize, indent: &str| {
            format!(
                "{indent}if done{k} == 0 {{\n{indent}  match __poll(f{k}) {{\n{indent}    .done {{ done{k} = 1 }}\n{indent}    .waiting(w) {{ hash{k} = fold_wait(hash{k}, w) }}\n{indent}    .failed(kind) {{ done{k} = 1 }}\n{indent}  }}\n{indent}}}\n"
            )
        };
        let n = case.len();
        s += "  if order == 0 {\n";
        for k in 0..n {
            s += &format!("    loop i in 0..300 {{\n{}    }}\n", poll(k, "      "));
        }
        s += "  } else if order == 1 {\n    loop i in 0..300 {\n";
        for k in 0..n {
            s += &poll(k, "      ");
        }
        s += "    }\n  } else if order == 2 {\n    loop i in 0..300 {\n";
        for k in (0..n).rev() {
            s += &poll(k, "      ");
        }
        s += "    }\n  } else {\n    var turn: u64 = 0\n    loop i in 0..900 {\n";
        s += &format!("      pick: u64 = wrapping_rem(turn, {n})\n      turn = wrapping_add(turn, 1)\n");
        for k in 0..n {
            s += &format!("      if pick == {k} {{\n{}      }}\n", poll(k, "        "));
        }
        s += "    }\n  }\n";
        for k in 0..n {
            s += &format!("  r{k}: u64 = __result(f{k})\n  print(\"{{r{k}}} {{hash{k}}}\")\n");
        }
        s += "}\n\n";
    }
    s
}

fn main_of(orders: &[u64]) -> String {
    let mut s = String::from("use std.console (console, print)\nuse std.frames (drive_made, drive_reference)\n\nfn floats_equal(a: f32, b: f32, c: f64, d: f64) -> bool { a == b and c == d }\n\nfn main() <console, alloc> {\n  same = floats_equal(1.5, 1.5, 2.5, 2.5)\n");
    for o in orders {
        s += &format!("  print(\"made {o}\")\n  drive_made({o})\n  print(\"reference {o}\")\n  drive_reference({o})\n");
    }
    s += "}\n";
    s
}

/// What a program of the tests prints, or what refuses it.
fn lowered(source: &str, module: &str) -> Result<scalar::Program, String> {
    smith::lower_in(source.as_bytes(), &with_std(&[("frames", module)]))
        .map_err(|e| format!("{e:?}"))
}

fn run(program: &scalar::Program) -> scalar::Outcome {
    assert_eq!(scalar::check(program), Ok(()));
    let outcome = scalar::run(program);
    assert_eq!(outcome.status, 0);
    outcome
}

/// The lines each drive printed, by the name the program printed before them.
fn drives(said: &str) -> Vec<(String, Vec<String>)> {
    let mut all: Vec<(String, Vec<String>)> = Vec::new();
    for line in said.lines() {
        if line.starts_with("made ") || line.starts_with("reference ") {
            all.push((line.to_string(), Vec::new()));
        } else {
            all.last_mut().expect("a drive").1.push(line.to_string());
        }
    }
    all
}

/// The frame of the lambda gives what the lambda's work gives behind a function
/// with a name, with the same waits in the same order, in each of four orders
/// that tasks are ready in, and the four orders agree.
fn check_drives(said: &str, frames: usize) {
    let all = drives(said);
    assert_eq!(all.len(), 8, "{said}");
    let first = &all[1].1;
    assert_eq!(first.len(), frames, "{said}");
    for pair in all.chunks(2) {
        assert_eq!(pair[0].1, pair[1].1, "{said}");
        assert_eq!(&pair[0].1, first, "{said}");
    }
}

fn a_frame_of_a_lambda_that_reads_names_gives_what_the_same_work_gives_in_every_order_of_readiness() {
    generate::check(340, 14, |g: &mut Gen| (0..1 + g.below(3)).map(|_| lambda(g)).collect::<Vec<_>>(), |case| {
        let module = module_of(case);
        let source = main_of(&[0, 1, 2, 3]);
        let program = lowered(&source, &module).unwrap_or_else(|e| panic!("{e}\n{module}"));
        let said = String::from_utf8(run(&program).stdout).expect("text");
        check_drives(&said, case.len());
    });
}

fn every_executable_gives_what_the_evaluator_says_of_frames_polled_in_every_order() {
    let targets = runnable();
    generate::check(341, 4, |g: &mut Gen| (0..1 + g.below(3)).map(|_| lambda(g)).collect::<Vec<_>>(), |case| {
        let module = module_of(case);
        let source = main_of(&[0, 1, 2, 3]);
        let program = lowered(&source, &module).unwrap_or_else(|e| panic!("{e}\n{module}"));
        let said = run(&scalar::passes(&program));
        check_drives(&String::from_utf8(said.stdout.clone()).expect("text"), case.len());
        for &target in &targets {
            runs_as_in(source.as_bytes(), &with_std(&[("frames", &module)]), target, &said);
        }
    });
}

fn the_calls_only_the_library_makes_are_refused_in_a_program() {
    let uses = "use std.bits (u64)\nuse std.task (Frame, Handle, Poll, Wait)\n\n";
    for (name, body) in [
        ("__frame", "fn main() {\n  f = __frame({ 1 })\n}\n"),
        ("__poll", "fn main(f: Frame) {\n  p = __poll(f)\n}\n"),
        ("__wait", "fn main() {\n  __wait(.time(1))\n}\n"),
        ("__result", "fn main(f: Frame) {\n  r: u64 = __result(f)\n}\n"),
    ] {
        let source = format!("{uses}{body}");
        let errors = smith::lower(source.as_bytes()).expect_err(name);
        assert!(
            errors.iter().any(|e| e.message.contains(name)),
            "{name}: {errors:?}"
        );
    }
}

fn a_frame_read_at_a_type_its_lambda_does_not_give_is_refused() {
    let module = "use std.bits (u64, u8)\nuse std.task (Frame)\n\nfn seven() -> u64 { 7 }\n\npub fn wrong() -> u8 {\n  f = __frame({ seven() })\n  r: u8 = __result(f)\n  r\n}\n\npub fn right() -> u64 {\n  f = __frame({ seven() })\n  r: u64 = __result(f)\n  r\n}\n";
    let main = "use std.frames (right, wrong)\n\nfn main() {\n  a = right()\n  b = wrong()\n}\n";
    let error = lowered(main, module).expect_err("a frame read at another type");
    assert!(error.contains("__result") || error.contains("frame"), "{error}");
    assert!(error.contains("u8"), "{error}");
    let module = "use std.bits (u64)\nuse std.task (Frame)\n\nfn seven() -> u64 { 7 }\n\npub fn right() -> u64 {\n  f = __frame({ seven() })\n  r: u64 = __result(f)\n  r\n}\n";
    let main = "use std.frames (right)\n\nfn main() {\n  a = right()\n}\n";
    lowered(main, module).expect("a frame read at the type it gives");
}

/// `join` reads the frame that `finish` gives for the number of a `Task[T]` at
/// `T`, and no other frame is read where its lambda is not seen: a number that
/// is not read from a `Task` is refused.
fn a_frame_that_finish_gives_for_a_number_that_is_not_a_tasks_is_refused() {
    let module = "use std.bits (u64)\nuse std.task (finish, task)\n\npub fn unseen(n: u64) -> u64 <task> {\n  r: u64 = __result(finish(n))\n  r\n}\n";
    let main = "use std.frames (unseen)\nuse std.task (task)\n\nfn main() <task> {\n  a = unseen(1)\n}\n";
    let error = lowered(main, module).expect_err("a frame of a number nothing says is a task's");
    assert!(error.contains("what this frame gives is not known here"), "{error}");
}

fn a_name_a_lambda_takes_moves_into_the_frame_and_a_second_use_is_refused() {
    let module = "use std.bits (u64)\nuse std.int (wrapping_add)\nuse std.task (Frame)\n\nfn total(xs: take List[u64]) -> u64 {\n  var s: u64 = 0\n  loop v in xs { s = wrapping_add(s, v) }\n  s\n}\n\npub fn once() -> u64 <alloc> {\n  xs: List[u64] = [1, 2, 3]\n  f = __frame({ total(xs) })\n  r: u64 = __result(f)\n  r\n}\n\npub fn twice() -> u64 <alloc> {\n  xs: List[u64] = [1, 2, 3]\n  f = __frame({ total(xs) })\n  g = total(xs)\n  r: u64 = __result(f)\n  wrapping_add(r, g)\n}\n";
    let main = "use std.frames (once)\n\nfn main() <alloc> {\n  a = once()\n}\n";
    lowered(main, module).expect("a list taken once");
    let main = "use std.frames (twice)\n\nfn main() <alloc> {\n  a = twice()\n}\n";
    let error = lowered(main, module).expect_err("a list taken twice");
    assert!(error.contains("xs"), "{error}");
}

/// A handler of `count` that keeps state, and one that does not.
const HANDLERS: &str = "use std.bits (u64)\nuse std.int (wrapping_add)\nuse std.task (Frame, Poll, Wait)\n\npub effect sense {\n  pub fn reading() -> u64\n}\n\npub fn fixed[T](n: u64, body: fn() -> T <sense>) -> T {\n  sense(reading: { n }, body: body)\n}\n\npub fn counting[T](body: fn() -> T <sense>) -> T {\n  var n: u64 = 0\n  sense(\n    reading: {\n      n = wrapping_add(n, 1)\n      n\n    },\n    body: body,\n  )\n}\n\npub fn read() -> u64 <sense> {\n  f = __frame({ reading() })\n  match __poll(f) {\n    .done { }\n    .waiting(w) { }\n    .failed(kind) { }\n  }\n  r: u64 = __result(f)\n  r\n}\n";

fn a_frame_under_a_handler_that_holds_state_reads_and_changes_it() {
    let main = "use std.console (console, print)\nuse std.frames (counting, read)\n\nfn main() <console, alloc> {\n  with counting()\n  a = read()\n  b = read()\n  print(\"{a} {b}\")\n}\n";
    let program = lowered(main, HANDLERS).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(String::from_utf8(run(&program).stdout).unwrap(), "1 2\n");
}

fn a_frame_under_a_handler_without_state_reads_what_the_handler_holds() {
    let main = "use std.console (console, print)\nuse std.frames (fixed, read)\n\nfn main() <console, alloc> {\n  with fixed(42)\n  print(\"{read()}\")\n}\n";
    let program = lowered(main, HANDLERS).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(String::from_utf8(run(&program).stdout).unwrap(), "42\n");
}

fn a_program_that_makes_no_frame_is_built_as_it_was() {
    // A program with no frame has no function that waits, so `suspend` gives it
    // back as it was, and the passes after it see the program they always did.
    let program = smith::lower(b"use std.console (console, print)\n\nfn main() <console, alloc> {\n  print(\"hello\")\n}\n")
        .unwrap();
    let suspended = scalar::suspend(program.clone()).expect("no wait");
    assert_eq!(suspended.program, program);
    assert!(suspended.tasks.is_empty());
}
