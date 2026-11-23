//! What the operations of a handler store in its state, a list that they push
//! onto, is built in a region of the state's own, which the block that installed
//! the handler gives back. A child that adds to it never builds it in its own
//! stack, which is given back when the child is joined, so the list is read
//! whole after every child that pushed to it has ended and others have built
//! over what they gave back, and after the handler's block, where it is given out.

use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate;
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built",
        a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built,
    ),
    (
        "a_list_a_handler_gives_out_of_its_block_is_read_whole_after_more_is_built",
        a_list_a_handler_gives_out_of_its_block_is_read_whole_after_more_is_built,
    ),
    (
        "two_handlers_that_hold_lists_are_in_force_at_once",
        two_handlers_that_hold_lists_are_in_force_at_once,
    ),
    (
        "a_handler_that_holds_a_list_and_is_installed_inside_a_child_under_the_scheduler_is_refused_as_not_built_yet",
        a_handler_that_holds_a_list_and_is_installed_inside_a_child_under_the_scheduler_is_refused_as_not_built_yet,
    ),
    (
        "a_text_built_at_run_time_a_struct_and_an_enum_that_hold_memory_are_kept_in_the_region_as_a_list_is",
        a_text_built_at_run_time_a_struct_and_an_enum_that_hold_memory_are_kept_in_the_region_as_a_list_is,
    ),
    (
        "a_block_of_a_handler_that_holds_a_list_gives_its_memory_back_when_it_ends",
        a_block_of_a_handler_that_holds_a_list_gives_its_memory_back_when_it_ends,
    ),
    (
        "a_handler_of_net_keeps_what_write_was_given_while_listen_serves_two_connections",
        a_handler_of_net_keeps_what_write_was_given_while_listen_serves_two_connections,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_gives_what_the_evaluator_gives_for_a_handler_whose_state_holds_a_list",
    every_executable_gives_what_the_evaluator_gives_for_a_handler_whose_state_holds_a_list,
)];

const USES: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.int (wrapping_add, wrapping_mul)\nuse std.list (freeze, push)\nuse std.net (Connection, Listener, NetError, accept, close, connection, handle, listen, listener, net, release, write)\nuse std.task (join, spawn, task, tasks, test_scheduler)\n\n";

fn program(items: &str) -> String {
    format!("{USES}{items}")
}

fn run(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(scalar::check(&program), Ok(()), "for {source}");
    let outcome = scalar::run(&program);
    assert_eq!(outcome.status, 0, "{} for {source}", String::from_utf8_lossy(&outcome.stdout));
    let passed = scalar::run(&scalar::passes(&program));
    assert_eq!(outcome.stdout, passed.stdout, "for {source}");
    String::from_utf8(outcome.stdout).expect("text")
}

fn refused(source: &str) -> String {
    match lower(source.as_bytes()) {
        Ok(_) => panic!("a program that is refused was built: {source}"),
        Err(errors) => format!("{errors:?}"),
    }
}

/// A handler whose state is a list, which `note` pushes onto and `all` gives,
/// which moves it out of the state, so `all` is the only operation that reads it.
const LOG: &str = "effect log {
  fn note(v: u64)
  fn all() -> List[u64] ! alloc
}

fn recording[T](body: fn() -> T ! log) -> T ! alloc {
  var seen: List[u64] = []
  log(
    note: fn(v) {
      seen = push(seen, v)
    },
    all: {
      seen
    },
    body: body,
  )
}

";

/// The same state, which `count` adds up and gives nothing out of.
const COUNTED: &str = "effect log {
  fn note(v: u64)
  fn count() -> u64
}

fn recording[T](body: fn() -> T ! log) -> T ! alloc {
  var seen: List[u64] = []
  log(
    note: fn(v) {
      seen = push(seen, v)
    },
    count: {
      var s: u64 = 0
      loop i in 0..seen.size { s = wrapping_add(s, seen[i]) }
      s
    },
    body: body,
  )
}

";

/// The functions that build memory around it.
const BUILD: &str = "fn build(n: u64, k: u64) -> List[u64] ! alloc {
  var acc: List[u64] = []
  loop i in 0..n { acc = push(acc, wrapping_add(wrapping_mul(i, i), k)) }
  acc
}

fn total(xs: List[u64]) -> u64 {
  var s: u64 = 0
  loop i in 0..xs.size { s = wrapping_add(s, xs[i]) }
  s
}

fn noise(k: u64) -> u64 ! alloc {
  total(build(300, k))
}

";

/// A child's work that pushes to the state and builds around it.
const PUSHER: &str = "fn pusher(v: u64) -> u64 ! log, alloc {
  note(v)
  note(wrapping_add(v, 100))
  noise(v)
}

";

/// What a list built as `build` builds adds up to.
fn built(n: u64, k: u64) -> u64 {
    (0..n).map(|i| i * i + k).sum()
}

/// The children that push to the state are joined and their stacks are given
/// back, other children build over them, and the state is read after: under a
/// group and under the scheduler of a test of each seed, in the order the seed
/// picks. The list is the four numbers the children pushed, in some order.
fn a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built() {
    generate::check(1043, 3, |g| (g.below(8) as u64, 1 + g.below(50) as u64), |&(seed, v)| {
        let expected = format!("4 {}\n", v + (v + 100) + (v + 1) + (v + 101));
        for (opening, run_effects, main_effects) in [
            ("with tasks()".to_string(), "alloc", "console, alloc"),
            (format!("with test_scheduler({seed})"), "alloc, diverge", "console, alloc, diverge"),
        ] {
            let source = program(&format!(
                "{LOG}{BUILD}{PUSHER}fn run() -> List[u64] ! {run_effects} {{\n  with recording()\n  {opening}\n  a = spawn {{ pusher({v}) }}\n  n1 = spawn {{ noise(5) }}\n  b = spawn {{ pusher({v} + 1) }}\n  n2 = spawn {{ noise(6) }}\n  join(a)\n  join(n1)\n  join(b)\n  join(n2)\n  c = spawn {{ noise(7) }}\n  d = spawn {{ noise(8) }}\n  join(c)\n  join(d)\n  all()\n}}\n\nfn main() ! {main_effects} {{\n  xs = run()\n  print(\"{{xs.size}} {{total(xs)}}\")\n}}\n"
            ));
            assert_eq!(run(&source), expected, "for {source}");
        }
    });
}

/// `all` gives the list out of the block of the handler, which is where the
/// region goes with it, and the caller builds more before it reads the list.
fn a_list_a_handler_gives_out_of_its_block_is_read_whole_after_more_is_built() {
    generate::check(1044, 3, |g| (g.below(8) as u64, 1 + g.below(50) as u64), |&(seed, k)| {
        let source = program(&format!(
            "{LOG}{BUILD}{PUSHER}fn run() -> List[u64] ! alloc, diverge {{\n  with recording()\n  with test_scheduler({seed})\n  a = spawn {{ pusher(1) }}\n  b = spawn {{ pusher(2) }}\n  join(a)\n  join(b)\n  c = spawn {{ noise(3) }}\n  join(c)\n  all()\n}}\n\nfn main() ! console, alloc, diverge {{\n  xs = run()\n  ys = build(500, {k})\n  zs = build(400, 1)\n  print(\"{{xs.size}} {{total(xs)}} {{total(ys)}} {{total(zs)}}\")\n}}\n"
        ));
        let expected = format!("4 206 {} {}\n", built(500, k), built(400, 1));
        assert_eq!(run(&source), expected, "for {source}");
    });
}

/// Two handlers that each keep a list are in force together, and each child
/// pushes to both.
fn two_handlers_that_hold_lists_are_in_force_at_once() {
    generate::check(1045, 3, |g| g.below(8) as u64, |&seed| {
        let source = program(&format!(
            "{LOG}{BUILD}{PUSHER}effect tally {{\n  fn bump(v: u64)\n  fn tallied() -> List[u64] ! alloc\n}}\n\nfn counting[T](body: fn() -> T ! tally) -> T ! alloc {{\n  var marks: List[u64] = []\n  tally(\n    bump: fn(v) {{\n      marks = push(marks, wrapping_mul(v, 10))\n    }},\n    tallied: {{\n      marks\n    }},\n    body: body,\n  )\n}}\n\nfn both(v: u64) -> u64 ! log, tally, alloc {{\n  bump(v)\n  pusher(v)\n}}\n\nfn run() -> List[u64] ! alloc, diverge {{\n  with counting()\n  with recording()\n  with test_scheduler({seed})\n  a = spawn {{ both(1) }}\n  n = spawn {{ noise(5) }}\n  b = spawn {{ both(2) }}\n  join(a)\n  join(n)\n  join(b)\n  c = spawn {{ noise(9) }}\n  join(c)\n  first = all()\n  second = tallied()\n  var joined: List[u64] = []\n  loop i in 0..first.size {{ joined = push(joined, first[i]) }}\n  loop i in 0..second.size {{ joined = push(joined, second[i]) }}\n  joined\n}}\n\nfn main() ! console, alloc, diverge {{\n  xs = run()\n  print(\"{{xs.size}} {{total(xs)}}\")\n}}\n"
        ));
        assert_eq!(run(&source), "6 236\n", "for {source}");
    });
}

/// A handler installed inside a child, under the scheduler that runs it, is not
/// one the scheduler's own code runs under, so the state it holds is not given to
/// the polls the scheduler makes. It is refused until that is built, and never
/// built to read what is not there.
fn a_handler_that_holds_a_list_and_is_installed_inside_a_child_under_the_scheduler_is_refused_as_not_built_yet() {
    let source = program(&format!(
        "{LOG}{BUILD}{PUSHER}fn inner(base: u64) -> u64 ! alloc, task {{\n  with recording()\n  with tasks()\n  a = spawn {{ pusher(base) }}\n  b = spawn {{ pusher(wrapping_add(base, 1)) }}\n  join(a)\n  join(b)\n  xs = all()\n  wrapping_add(xs.size, total(xs))\n}}\n\nfn run() -> u64 ! alloc, diverge {{\n  with test_scheduler(3)\n  x = spawn {{ inner(1) }}\n  y = spawn {{ inner(10) }}\n  wrapping_add(join(x), join(y))\n}}\n\nfn main() ! console, alloc, diverge {{\n  print(\"{{run()}}\")\n}}\n"
    ));
    let errors = refused(&source);
    assert!(errors.contains("installed under its scheduler"), "{errors}");
    assert!(errors.contains("unbuilt: true"), "{errors}");
}

/// Every type whose state can hold memory is kept in the region, not only a list:
/// a text that is built where it is assigned, a struct that holds a list and a
/// text, and an enum whose case holds a list, each changed by a child and read
/// after others have built over what the child gave back.
const KEEPING: &str = "struct Box {
  items: List[u64]
  name: str
}

enum Held {
  nothing
  some: List[u64]
}

effect keep {
  fn add(v: u64)
  fn said() -> str ! alloc
  fn boxed() -> Box ! alloc
  fn held() -> Held ! alloc
}

fn keeping[T](body: fn() -> T ! keep) -> T ! alloc {
  var text: str = \"\"
  var box: Box = Box(items: [], name: \"b\")
  var kept: Held = .nothing
  keep(
    add: fn(v) {
      text = \"{text}{v},\"
      box = Box(items: push(box.items, v), name: \"{box.name}{v}\")
      kept = .some([v, wrapping_add(v, 1)])
    },
    said: {
      text
    },
    boxed: {
      box
    },
    held: {
      kept
    },
    body: body,
  )
}

";

fn a_text_built_at_run_time_a_struct_and_an_enum_that_hold_memory_are_kept_in_the_region_as_a_list_is() {
    generate::check(1046, 3, |g| (g.below(8) as u64, 1 + g.below(90) as u64), |&(seed, v)| {
        let source = program(&format!(
            "{BUILD}{KEEPING}fn fill(v: u64) -> u64 ! keep {{\n  loop i in 0..20 {{ add(wrapping_add(v, i)) }}\n  v\n}}\n\nfn run() -> (str, Box, Held) ! alloc, diverge {{\n  with keeping()\n  with test_scheduler({seed})\n  a = spawn {{ fill({v}) }}\n  n = spawn {{ noise(1) }}\n  join(a)\n  join(n)\n  c = spawn {{ noise(2) }}\n  join(c)\n  d = spawn {{ noise(3) }}\n  join(d)\n  e = spawn {{ noise(4) }}\n  join(e)\n  f = spawn {{ noise(5) }}\n  join(f)\n  (said(), boxed(), held())\n}}\n\nfn main() ! console, alloc, diverge {{\n  (t, b, h) = run()\n  zs = build(400, 1)\n  print(\"{{t}} {{b.name}} {{b.items.size}}\")\n  match h {{\n    .nothing {{ print(\"none\") }}\n    .some(xs) {{ print(\"held {{xs.size}} {{zs.size}}\") }}\n  }}\n}}\n"
        ));
        let numbers: Vec<String> = (0..20).map(|i| (v + i).to_string()).collect();
        let expected = format!("{} b{} 20\nheld 2 400\n", numbers.iter().map(|n| format!("{n},")).collect::<String>(), numbers.concat());
        assert_eq!(run(&source), expected, "for {source}");
    });
}

/// A block that installs the handler, with children that push to it, is run for
/// many rounds, and asks the system for no more than it does for a few: what the
/// block's region held is given back to the pool where the block ends, as a
/// child's is.
fn a_block_of_a_handler_that_holds_a_list_gives_its_memory_back_when_it_ends() {
    let asked = |rounds: u64| {
        let source = program(&format!(
            "{COUNTED}{BUILD}{PUSHER}fn main() ! console, alloc {{\n  var sum: u64 = 0\n  loop r in 0..{rounds} {{\n    with recording()\n    with tasks()\n    a = spawn {{ note(r) }}\n    b = spawn {{ note(wrapping_add(r, 1)) }}\n    join(a)\n    join(b)\n    c = spawn {{ total(build(20, r)) }}\n    join(c)\n    sum = wrapping_add(sum, count())\n  }}\n  print(\"{{sum}}\")\n}}\n"
        ));
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let (outcome, requests) = scalar::run_counting(&program, usize::MAX);
        assert_eq!(outcome.status, 0);
        assert_eq!(outcome.stdout, format!("{}\n", rounds * rounds).into_bytes());
        requests.len()
    };
    let few = asked(10);
    let many = asked(1000);
    assert_eq!(few, many, "{few} requests for 10 rounds and {many} for 1000");
}

/// What `write` of `net` is given is kept in a handler's state, and `listen`
/// serves two connections under it with no socket opened: each connection is a
/// task of its own that writes, and the handler's list has both writes, which the
/// test reads after the handler's block, for each seed of the scheduler.
const NET: &str = "fn recording[T](body: fn() -> T ! net) -> (T, List[List[Byte]]) ! alloc {
  var kept: List[List[Byte]] = []
  result = net(
    listen_on: fn(host, port) { .ok(listener(1)) },
    accept: fn(l) { .ok(connection(wrapping_add(handle(l), 1))) },
    stop: fn(l) { n = release(l) },
    connect_to: fn(host, port) { .err(.refused) },
    read: fn(c, most) { .ok([]) },
    write: fn(c, bytes) {
      var copied: List[Byte] = []
      loop i in 0..bytes.size { copied = push(copied, bytes[i]) }
      kept = push(kept, copied)
      .ok(())
    },
    close: fn(c) { n = release(c) },
    bind_to: fn(host, port) { .err(.refused) },
    send: fn(s, to, bytes) { .err(.closed) },
    receive: fn(s, most) { .err(.closed) },
    unbind: fn(s) { n = release(s) },
    body: body,
  )
  (result, kept)
}

fn session(c: take Connection, tag: Byte) ! net, alloc {
  sent = write(c, freeze([tag, 33]))
  close(c)
}

fn serve(l: Listener) -> Result[u64, NetError] ! net, alloc, task {
  match accept(l) {
    .err(e) { .err(e) }
    .ok(c1) {
      match accept(l) {
        .err(e) {
          close(c1)
          .err(e)
        }
        .ok(c2) {
          a = spawn { session(c1, 65) }
          b = spawn { session(c2, 66) }
          join(a)
          join(b)
          n = spawn { noise(3) }
          join(n)
          .ok(2)
        }
      }
    }
  }
}

";

fn net_program(seed: u64) -> String {
    program(&format!(
        "{BUILD}{NET}fn run() -> (Result[u64, NetError], List[List[Byte]]) ! alloc, diverge {{\n  with recording()\n  with test_scheduler({seed})\n  listen(\"localhost\", 80, serve)\n}}\n\nfn main() ! console, alloc, diverge {{\n  (served, writes) = run()\n  var bytes: u64 = 0\n  loop i in 0..writes.size {{\n    inner = writes[i]\n    bytes = wrapping_add(bytes, inner.size)\n  }}\n  print(\"{{writes.size}} {{bytes}}\")\n  match served {{\n    .ok(n) {{ print(\"served {{n}}\") }}\n    .err(e) {{ print(\"failed\") }}\n  }}\n}}\n"
    ))
}

fn a_handler_of_net_keeps_what_write_was_given_while_listen_serves_two_connections() {
    for seed in 0..8 {
        assert_eq!(run(&net_program(seed)), "2 4\nserved 2\n", "for seed {seed}");
    }
}

/// The executables of every target give what the evaluator gives, for a list in
/// the state of a handler that children push to, and for the writes of `net`.
fn every_executable_gives_what_the_evaluator_gives_for_a_handler_whose_state_holds_a_list() {
    let sources = [
        program(&format!(
            "{LOG}{BUILD}{PUSHER}fn run() -> List[u64] ! alloc, diverge {{\n  with recording()\n  with test_scheduler(3)\n  a = spawn {{ pusher(1) }}\n  n = spawn {{ noise(5) }}\n  b = spawn {{ pusher(2) }}\n  join(a)\n  join(n)\n  join(b)\n  all()\n}}\n\nfn main() ! console, alloc, diverge {{\n  xs = run()\n  ys = build(500, 9)\n  print(\"{{xs.size}} {{total(xs)}} {{total(ys)}}\")\n}}\n"
        )),
        net_program(5),
    ];
    for source in sources {
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
        let said = scalar::run(&scalar::passes(&program));
        assert_eq!(said.status, 0, "for {source}");
        for target in runnable() {
            runs_as(source.as_bytes(), target, &said);
        }
    }
}
