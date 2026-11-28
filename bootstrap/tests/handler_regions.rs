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
use crate::lowered_once::{ROUNDS, SEED, Written};
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built_part_1_of_3",
        || a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built(0),
    ),
    (
        "a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built_part_2_of_3",
        || a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built(1),
    ),
    (
        "a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built_part_3_of_3",
        || a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built(2),
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
        "a_list_a_child_builds_before_it_hands_it_to_an_operation_that_stores_it_is_kept_in_the_region",
        a_list_a_child_builds_before_it_hands_it_to_an_operation_that_stores_it_is_kept_in_the_region,
    ),
    (
        "the_whole_state_of_generated_handlers_matches_a_model_after_children_that_store_what_was_built_anywhere_part_1_of_8",
        || the_whole_state_of_generated_handlers_matches_a_model(0),
    ),
    (
        "the_whole_state_of_generated_handlers_matches_a_model_after_children_that_store_what_was_built_anywhere_part_2_of_8",
        || the_whole_state_of_generated_handlers_matches_a_model(1),
    ),
    (
        "the_whole_state_of_generated_handlers_matches_a_model_after_children_that_store_what_was_built_anywhere_part_3_of_8",
        || the_whole_state_of_generated_handlers_matches_a_model(2),
    ),
    (
        "the_whole_state_of_generated_handlers_matches_a_model_after_children_that_store_what_was_built_anywhere_part_4_of_8",
        || the_whole_state_of_generated_handlers_matches_a_model(3),
    ),
    (
        "the_whole_state_of_generated_handlers_matches_a_model_after_children_that_store_what_was_built_anywhere_part_5_of_8",
        || the_whole_state_of_generated_handlers_matches_a_model(4),
    ),
    (
        "the_whole_state_of_generated_handlers_matches_a_model_after_children_that_store_what_was_built_anywhere_part_6_of_8",
        || the_whole_state_of_generated_handlers_matches_a_model(5),
    ),
    (
        "the_whole_state_of_generated_handlers_matches_a_model_after_children_that_store_what_was_built_anywhere_part_7_of_8",
        || the_whole_state_of_generated_handlers_matches_a_model(6),
    ),
    (
        "the_whole_state_of_generated_handlers_matches_a_model_after_children_that_store_what_was_built_anywhere_part_8_of_8",
        || the_whole_state_of_generated_handlers_matches_a_model(7),
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
    run_lowered(&program, source)
}

/// What `run` gives for a program that is lowered and checked, which `source` is
/// the text of.
fn run_lowered(program: &scalar::Program, source: &str) -> String {
    let outcome = scalar::run(program);
    assert_eq!(outcome.status, 0, "{} for {source}", String::from_utf8_lossy(&outcome.stdout));
    let passed = scalar::run(&scalar::passes(program));
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
  fn all() -> List[u64] <alloc>
}

fn recording[T](body: fn() -> T <log>) -> T <alloc> {
  mut seen: List[u64] = []
  log(
    note: fn(v) {
      push(mut seen, v)
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

fn recording[T](body: fn() -> T <log>) -> T <alloc> {
  mut seen: List[u64] = []
  log(
    note: fn(v) {
      push(mut seen, v)
    },
    count: {
      mut s: u64 = 0
      loop i in 0..seen.size { s = wrapping_add(s, seen[i]) }
      s
    },
    body: body,
  )
}

";

/// The functions that build memory around it.
const BUILD: &str = "fn build(n: u64, k: u64) -> List[u64] <alloc> {
  mut acc: List[u64] = []
  loop i in 0..n { push(mut acc, wrapping_add(wrapping_mul(i, i), k)) }
  acc
}

fn total(xs: List[u64]) -> u64 {
  mut s: u64 = 0
  loop i in 0..xs.size { s = wrapping_add(s, xs[i]) }
  s
}

fn noise(k: u64) -> u64 <alloc> {
  total(build(300, k))
}

";

/// A child's work that pushes to the state and builds around it.
const PUSHER: &str = "fn pusher(v: u64) -> u64 <log, alloc> {
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
fn a_list_in_a_handlers_state_is_read_whole_after_the_children_that_pushed_to_it_have_ended_and_others_have_built(part: u64) {
    generate::check_part(1043, 3, part, 3, |g| (g.below(8) as u64, 1 + g.below(50) as u64), |&(seed, v)| {
        let expected = format!("4 {}\n", v + (v + 100) + (v + 1) + (v + 101));
        for (opening, run_effects, main_effects) in [
            ("with tasks()".to_string(), "alloc", "console, alloc"),
            (format!("with test_scheduler({seed})"), "alloc, diverge", "console, alloc, diverge"),
        ] {
            let source = program(&format!(
                "{LOG}{BUILD}{PUSHER}fn run() -> List[u64] <{run_effects}> {{\n  with recording()\n  {opening}\n  a = spawn {{ pusher({v}) }}\n  n1 = spawn {{ noise(5) }}\n  b = spawn {{ pusher({v} + 1) }}\n  n2 = spawn {{ noise(6) }}\n  _ = join(take a)\n  _ = join(take n1)\n  _ = join(take b)\n  _ = join(take n2)\n  c = spawn {{ noise(7) }}\n  d = spawn {{ noise(8) }}\n  _ = join(take c)\n  _ = join(take d)\n  all()\n}}\n\nfn main() <{main_effects}> {{\n  xs = run()\n  print(\"{{xs.size}} {{total(xs)}}\")\n}}\n"
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
            "{LOG}{BUILD}{PUSHER}fn run() -> List[u64] <alloc, diverge> {{\n  with recording()\n  with test_scheduler({seed})\n  a = spawn {{ pusher(1) }}\n  b = spawn {{ pusher(2) }}\n  _ = join(take a)\n  _ = join(take b)\n  c = spawn {{ noise(3) }}\n  _ = join(take c)\n  all()\n}}\n\nfn main() <console, alloc, diverge> {{\n  xs = run()\n  ys = build(500, {k})\n  zs = build(400, 1)\n  print(\"{{xs.size}} {{total(xs)}} {{total(ys)}} {{total(zs)}}\")\n}}\n"
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
            "{LOG}{BUILD}{PUSHER}effect tally {{\n  fn bump(v: u64)\n  fn tallied() -> List[u64] <alloc>\n}}\n\nfn counting[T](body: fn() -> T <tally>) -> T <alloc> {{\n  mut marks: List[u64] = []\n  tally(\n    bump: fn(v) {{\n      push(mut marks, wrapping_mul(v, 10))\n    }},\n    tallied: {{\n      marks\n    }},\n    body: body,\n  )\n}}\n\nfn both(v: u64) -> u64 <log, tally, alloc> {{\n  bump(v)\n  pusher(v)\n}}\n\nfn run() -> List[u64] <alloc, diverge> {{\n  with counting()\n  with recording()\n  with test_scheduler({seed})\n  a = spawn {{ both(1) }}\n  n = spawn {{ noise(5) }}\n  b = spawn {{ both(2) }}\n  _ = join(take a)\n  _ = join(take n)\n  _ = join(take b)\n  c = spawn {{ noise(9) }}\n  _ = join(take c)\n  first = all()\n  second = tallied()\n  mut joined: List[u64] = []\n  loop i in 0..first.size {{ push(mut joined, first[i]) }}\n  loop i in 0..second.size {{ push(mut joined, second[i]) }}\n  joined\n}}\n\nfn main() <console, alloc, diverge> {{\n  xs = run()\n  print(\"{{xs.size}} {{total(xs)}}\")\n}}\n"
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
        "{LOG}{BUILD}{PUSHER}fn inner(base: u64) -> u64 <alloc, task> {{\n  with recording()\n  with tasks()\n  a = spawn {{ pusher(base) }}\n  b = spawn {{ pusher(wrapping_add(base, 1)) }}\n  _ = join(take a)\n  _ = join(take b)\n  xs = all()\n  wrapping_add(xs.size, total(xs))\n}}\n\nfn run() -> u64 <alloc, diverge> {{\n  with test_scheduler(3)\n  x = spawn {{ inner(1) }}\n  y = spawn {{ inner(10) }}\n  wrapping_add(join(take x), join(take y))\n}}\n\nfn main() <console, alloc, diverge> {{\n  print(\"{{run()}}\")\n}}\n"
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
  fn said() -> str <alloc>
  fn boxed() -> Box <alloc>
  fn held() -> Held <alloc>
}

fn keeping[T](body: fn() -> T <keep>) -> T <alloc> {
  mut text: str = \"\"
  mut box: Box = Box(items: [], name: \"b\")
  mut kept: Held = .nothing
  keep(
    add: fn(v) {
      text = \"{text}{v},\"
      push(mut box.items, v)
      box = Box(items: box.items, name: \"{box.name}{v}\")
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
            "{BUILD}{KEEPING}fn fill(v: u64) -> u64 <keep> {{\n  loop i in 0..20 {{ add(wrapping_add(v, i)) }}\n  v\n}}\n\nfn run() -> (str, Box, Held) <alloc, diverge> {{\n  with keeping()\n  with test_scheduler({seed})\n  a = spawn {{ fill({v}) }}\n  n = spawn {{ noise(1) }}\n  _ = join(take a)\n  _ = join(take n)\n  c = spawn {{ noise(2) }}\n  _ = join(take c)\n  d = spawn {{ noise(3) }}\n  _ = join(take d)\n  e = spawn {{ noise(4) }}\n  _ = join(take e)\n  f = spawn {{ noise(5) }}\n  _ = join(take f)\n  (said(), boxed(), held())\n}}\n\nfn main() <console, alloc, diverge> {{\n  (t, b, h) = run()\n  zs = build(400, 1)\n  print(\"{{t}} {{b.name}} {{b.items.size}} {{total(b.items)}}\")\n  match h {{\n    .nothing {{ print(\"none\") }}\n    .some(xs) {{ print(\"held {{xs.size}} {{total(xs)}} {{zs.size}}\") }}\n  }}\n}}\n"
        ));
        let numbers: Vec<String> = (0..20).map(|i| (v + i).to_string()).collect();
        let expected = format!(
            "{} b{} 20 {}\nheld 2 {} 400\n",
            numbers.iter().map(|n| format!("{n},")).collect::<String>(),
            numbers.concat(),
            (0..20).map(|i| v + i).sum::<u64>(),
            2 * v + 39
        );
        assert_eq!(run(&source), expected, "for {source}");
    });
}

/// What is kept in the state may be built before the operation is called, or
/// before the assignment in it: the caller builds a list and gives it to an
/// operation that pushes it onto a list of lists, so the memory of the list that
/// the caller built is the memory of the region, whichever stack it was asked of.
fn a_list_a_child_builds_before_it_hands_it_to_an_operation_that_stores_it_is_kept_in_the_region() {
    generate::check(1047, 3, |g| (g.below(8) as u64, 1 + g.below(40) as u64), |&(seed, k)| {
        let source = program(&format!(
            "{BUILD}effect shelf {{\n  fn put(xs: take List[u64])\n  fn shelved() -> List[List[u64]] <alloc>\n}}\n\nfn shelving[T](body: fn() -> T <shelf>) -> T <alloc> {{\n  mut shelves: List[List[u64]] = []\n  shelf(\n    put: fn(xs) {{\n      push(mut shelves, take xs)\n    }},\n    shelved: {{\n      shelves\n    }},\n    body: body,\n  )\n}}\n\nfn stocker(n: u64, k: u64) -> u64 <shelf, alloc> {{\n  xs = build(n, k)\n  put(take xs)\n  1\n}}\n\nfn run() -> List[List[u64]] <alloc, diverge> {{\n  with shelving()\n  with test_scheduler({seed})\n  a = spawn {{ stocker(40, {k}) }}\n  b = spawn {{ stocker(50, {k}) }}\n  _ = join(take a)\n  _ = join(take b)\n  n = spawn {{ noise(1) }}\n  _ = join(take n)\n  m = spawn {{ noise(2) }}\n  _ = join(take m)\n  shelved()\n}}\n\nfn main() <console, alloc, diverge> {{\n  all = run()\n  zs = build(400, 3)\n  mut sum: u64 = 0\n  loop i in 0..all.size {{\n    inner = all[i]\n    sum = wrapping_add(sum, total(inner))\n  }}\n  print(\"{{all.size}} {{sum}} {{total(zs)}}\")\n}}\n"
        ));
        let expected = format!("2 {} {}\n", built(40, k) + built(50, k), built(400, 3));
        assert_eq!(run(&source), expected, "for {source}");
    });
}

/// What a child of a generated program does to the state of the handler `STORE`.
#[derive(Clone, Debug, PartialEq)]
enum Act {
    /// Builds a list and gives it to `put`, which pushes it onto a list of lists.
    Put(u64, u64),
    /// Calls `grow`, which builds a list in a loop of its own and pushes it.
    Grow(u64, u64),
    /// Builds a list and gives it, with a number, to `pair`, which pushes a struct
    /// that holds both.
    Pair(u64, u64, u64),
    /// Pushes a number onto a list of numbers.
    Note(u64),
    /// Builds a list that nothing keeps, over what an earlier child gave back.
    Noise(u64),
}

/// A handler that keeps three states that hold memory, and the operations that
/// store a value built by the caller, a value built by the operation in a loop
/// before it is stored, a struct that holds a list, and a number.
const STORE: &str = "struct Pair {
  items: List[u64]
  tag: u64
}

effect store {
  fn put(xs: take List[u64])
  fn grow(n: u64, k: u64)
  fn pair(xs: take List[u64], tag: u64)
  fn note(v: u64)
  fn shelved() -> List[List[u64]] <alloc>
  fn pairs() -> List[Pair] <alloc>
  fn noted() -> List[u64] <alloc>
}

fn storing[T](body: fn() -> T <store>) -> T <alloc> {
  mut shelves: List[List[u64]] = []
  mut held: List[Pair] = []
  mut notes: List[u64] = []
  store(
    put: fn(xs) {
      push(mut shelves, take xs)
    },
    grow: fn(n, k) {
      mut acc: List[u64] = []
      loop i in 0..n { push(mut acc, wrapping_add(wrapping_mul(i, i), k)) }
      push(mut shelves, take acc)
    },
    pair: fn(xs, tag) {
      push(mut held, Pair(items: xs, tag: tag))
    },
    note: fn(v) {
      push(mut notes, v)
    },
    shelved: {
      shelves
    },
    pairs: {
      held
    },
    noted: {
      notes
    },
    body: body,
  )
}

";

/// What the state holds after a program of children has run, as sums that do not
/// depend on the order the children ran in, and that read every number kept.
fn model(children: &[Vec<Act>]) -> String {
    let (mut shelves, mut shelf, mut pairs, mut pair, mut notes, mut note) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    for act in children.iter().flatten() {
        match *act {
            Act::Put(n, k) | Act::Grow(n, k) => {
                shelves += 1;
                shelf += n * 1_000_003 + built(n, k);
            }
            Act::Pair(n, k, tag) => {
                pairs += 1;
                pair += tag * 7919 + n * 1_000_003 + built(n, k);
            }
            Act::Note(v) => {
                notes += 1;
                note += v * 31;
            }
            Act::Noise(_) => {}
        }
    }
    format!("{shelves} {shelf} {pairs} {pair} {notes} {note} {}", built(400, 3))
}

fn child(case: usize, at: usize, acts: &[Act]) -> String {
    let mut s = format!("fn child{case}_{at}() -> u64 <store, alloc> {{\n");
    for (i, act) in acts.iter().enumerate() {
        s += &match *act {
            Act::Put(n, k) => format!("  xs{i} = build({n}, {k})\n  put(take xs{i})\n"),
            Act::Grow(n, k) => format!("  grow({n}, {k})\n"),
            Act::Pair(n, k, tag) => format!("  xs{i} = build({n}, {k})\n  pair(take xs{i}, {tag})\n"),
            Act::Note(v) => format!("  note({v})\n"),
            Act::Noise(k) => format!("  junk{i} = noise({k})\n"),
        };
    }
    s + "  1\n}\n\n"
}

/// The text of a program with a function for each of the cases, which runs its
/// children under the scheduler of its seed and says what the state is, and a
/// `main` that calls them in order, so that the cases are lowered once.
fn generated(cases: &[(u64, Vec<Vec<Act>>)]) -> String {
    let mut s = format!("{BUILD}{STORE}");
    for (case, (seed, children)) in cases.iter().enumerate() {
        for (at, acts) in children.iter().enumerate() {
            s += &child(case, at, acts);
        }
        s += &format!("fn run{case}() -> (List[List[u64]], List[Pair], List[u64]) <alloc, diverge> {{\n  with storing()\n");
        s += &format!("  with test_scheduler({seed})\n");
        for at in 0..children.len() {
            s += &format!("  t{at} = spawn {{ child{case}_{at}() }}\n");
        }
        for at in 0..children.len() {
            s += &format!("  _ = join(take t{at})\n");
        }
        // Others build over what the children gave back before the state is read.
        s += "  n1 = spawn { noise(11) }\n  _ = join(take n1)\n  n2 = spawn { noise(12) }\n  _ = join(take n2)\n";
        s += "  (shelved(), pairs(), noted())\n}\n\n";
        s += &format!("fn report{case}() <console, alloc, diverge> {{\n  (shelves, held, notes) = run{case}()\n  zs = build(400, 3)\n");
        s += "  mut shelf: u64 = 0\n  loop i in 0..shelves.size {\n    inner = shelves[i]\n    shelf = wrapping_add(shelf, wrapping_add(wrapping_mul(inner.size, 1000003), total(inner)))\n  }\n";
        s += "  mut pair: u64 = 0\n  loop i in 0..held.size {\n    p = held[i]\n    pair = wrapping_add(pair, wrapping_add(wrapping_mul(p.tag, 7919), wrapping_add(wrapping_mul(p.items.size, 1000003), total(p.items))))\n  }\n";
        s += "  mut note: u64 = 0\n  loop i in 0..notes.size { note = wrapping_add(note, wrapping_mul(notes[i], 31)) }\n";
        s += "  print(\"{shelves.size} {shelf} {held.size} {pair} {notes.size} {note} {total(zs)}\")\n}\n\n";
    }
    s += "fn main() <console, alloc, diverge> {\n";
    for case in 0..cases.len() {
        s += &format!("  report{case}()\n");
    }
    s += "}\n";
    program(&s)
}

/// What each of the cases says, as `generated` runs them as one program.
fn said_by(cases: &[(u64, Vec<Vec<Act>>)]) -> Vec<String> {
    let said = run(&generated(cases));
    let lines: Vec<String> = said.lines().map(String::from).collect();
    assert_eq!(lines.len(), cases.len(), "{said}");
    lines
}

fn generate_children(g: &mut generate::Gen) -> (u64, Vec<Vec<Act>>) {
    let seed = g.below(16) as u64;
    let count = 1 + g.below(4);
    let children = (0..count)
        .map(|_| {
            let acts = 1 + g.below(4);
            (0..acts)
                .map(|_| {
                    let (n, k) = (1 + g.below(60) as u64, g.below(50) as u64);
                    match g.below(5) {
                        0 => Act::Put(n, k),
                        1 => Act::Grow(n, k),
                        2 => Act::Pair(n, k, 1 + g.below(30) as u64),
                        3 => Act::Note(1 + g.below(500) as u64),
                        _ => Act::Noise(k),
                    }
                })
                .collect()
        })
        .collect();
    (seed, children)
}

/// Children of any shape store, under the scheduler of a test of any seed, what
/// they built before the operation, what they built in the operation in a loop,
/// a struct that holds a list, and a number, and after other children have built
/// over what they gave back the whole state is what a model says.
fn the_whole_state_of_generated_handlers_matches_a_model(part: u64) {
    // The cases of this part are lowered as one program, and a case that
    // shrinking tries is lowered alone.
    let (from, to) = (48 * part / 8, 48 * (part + 1) / 8);
    let all: Vec<(u64, Vec<Vec<Act>>)> = (from..to).map(|case| generate::sample(1048, case, generate_children)).collect();
    let known = said_by(&all);
    generate::check_part(1048, 48, part, 8, generate_children, |(seed, children)| {
        let said = match all.iter().position(|c| c.0 == *seed && c.1 == *children) {
            Some(at) => known[at].clone(),
            None => said_by(&[(*seed, children.clone())]).remove(0),
        };
        assert_eq!(said, model(children), "for {seed} {children:?}");
    });
}

/// A block that installs the handler, with children that push to it, is run for
/// many rounds, and asks the system for no more than it does for a few: what the
/// block's region held is given back to the pool where the block ends, as a
/// child's is.
fn a_block_of_a_handler_that_holds_a_list_gives_its_memory_back_when_it_ends() {
    // The text is the same for both numbers of rounds but the number, so it is
    // lowered once.
    let written = Written::new(program(&format!(
        "{COUNTED}{BUILD}{PUSHER}fn main() <console, alloc> {{\n  mut sum: u64 = 0\n  loop r in 0..{ROUNDS} {{\n    with recording()\n    with tasks()\n    a = spawn {{ note(r) }}\n    b = spawn {{ note(wrapping_add(r, 1)) }}\n    _ = join(take a)\n    _ = join(take b)\n    c = spawn {{ total(build(20, r)) }}\n    _ = join(take c)\n    sum = wrapping_add(sum, count())\n  }}\n  print(\"{{sum}}\")\n}}\n"
    )));
    let asked = |rounds: u64| {
        let (outcome, requests) = scalar::run_counting(&written.with(&[(ROUNDS, rounds)]), usize::MAX);
        assert_eq!(outcome.status, 0);
        assert_eq!(outcome.stdout, format!("{}\n", rounds * rounds).into_bytes());
        requests.len()
    };
    let few = asked(10);
    let many = asked(400);
    assert_eq!(few, many, "{few} requests for 10 rounds and {many} for 400");
}

/// What `write` of `net` is given is kept in a handler's state, and `listen`
/// serves two connections under it with no socket opened: each connection is a
/// task of its own that writes, and the handler's list has both writes, which the
/// test reads after the handler's block, for each seed of the scheduler.
const NET: &str = "fn recording[T](body: fn() -> T <net>) -> (T, List[List[Byte]]) <alloc> {
  mut kept: List[List[Byte]] = []
  result = net(
    listen_on: fn(host, port) { .ok(listener(1)) },
    accept: fn(l) { .ok(connection(2)) },
    stop: fn(l) { n = release(take l) },
    connect_to: fn(host, port) { .err(.refused(host)) },
    read: fn(c, most) { .ok(.ended) },
    write: fn(c, bytes) {
      mut copied: List[Byte] = []
      loop i in 0..bytes.size { push(mut copied, bytes[i]) }
      push(mut kept, take copied)
      .ok(())
    },
    close: fn(c) { n = release(take c) },
    bind_to: fn(host, port) { .err(.refused(host)) },
    send: fn(s, to, bytes) { .err(.closed(s.host)) },
    receive: fn(s, most) { .err(.closed(s.host)) },
    unbind: fn(s) { n = release(take s) },
    body: body,
  )
  (result, kept)
}

fn session(c: take Connection, tag: Byte) <net, alloc> {
  sent = write(c, freeze([tag, 33]))
  close(take c)
}

fn serve(l: Listener) -> Result[u64, NetError] <net, alloc, task> {
  match accept(l) {
    .err(e) { .err(e) }
    .ok(c1) {
      match accept(l) {
        .err(e) {
          close(take c1)
          .err(e)
        }
        .ok(c2) {
          a = spawn { session(take c1, 65) }
          b = spawn { session(take c2, 66) }
          _ = join(take a)
          _ = join(take b)
          n = spawn { noise(3) }
          _ = join(take n)
          .ok(2)
        }
      }
    }
  }
}

";

fn net_program(seed: u64) -> String {
    program(&format!(
        "{BUILD}{NET}fn run() -> (Result[u64, NetError], List[List[Byte]]) <alloc, diverge> {{\n  with recording()\n  with test_scheduler({seed})\n  listen(\"localhost\", .at(80), serve)\n}}\n\nfn main() <console, alloc, diverge> {{\n  (served, writes) = run()\n  mut bytes: u64 = 0\n  mut sum: u64 = 0\n  loop i in 0..writes.size {{\n    inner = writes[i]\n    bytes = wrapping_add(bytes, inner.size)\n    loop j in 0..inner.size {{\n      b: u64 = inner[j]\n      sum = wrapping_add(sum, b)\n    }}\n  }}\n  zs = build(300, 5)\n  print(\"{{writes.size}} {{bytes}} {{sum}} {{total(zs)}}\")\n  match served {{\n    .ok(n) {{ print(\"served {{n}}\") }}\n    .err(e) {{ print(\"failed\") }}\n  }}\n}}\n"
    ))
}

fn a_handler_of_net_keeps_what_write_was_given_while_listen_serves_two_connections() {
    // The seed is the only number the text of each seed does not share.
    let written = Written::new(net_program(SEED));
    for seed in 0..8 {
        assert_eq!(
            run_lowered(&written.with(&[(SEED, seed)]), &written.source),
            format!("2 4 197 {}\nserved 2\n", built(300, 5)),
            "for seed {seed}"
        );
    }
}

/// The executables of every target give what the evaluator gives, for a list in
/// the state of a handler that children push to, and for the writes of `net`.
fn every_executable_gives_what_the_evaluator_gives_for_a_handler_whose_state_holds_a_list() {
    let sources = [
        program(&format!(
            "{LOG}{BUILD}{PUSHER}fn run() -> List[u64] <alloc, diverge> {{\n  with recording()\n  with test_scheduler(3)\n  a = spawn {{ pusher(1) }}\n  n = spawn {{ noise(5) }}\n  b = spawn {{ pusher(2) }}\n  _ = join(take a)\n  _ = join(take n)\n  _ = join(take b)\n  all()\n}}\n\nfn main() <console, alloc, diverge> {{\n  xs = run()\n  ys = build(500, 9)\n  print(\"{{xs.size}} {{total(xs)}} {{total(ys)}}\")\n}}\n"
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
