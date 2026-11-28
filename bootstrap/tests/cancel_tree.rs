//! For every tree of tasks that hold connections across waits, and every point
//! each task is at when the block of the scheduler ends, each connection that a
//! task opened is closed exactly once, by the task or by the end of its block, a
//! task's own children are ended before it, and no frame is read after it was
//! freed. A handler of `net` that the test writes says each close, and the
//! evaluator fails on a read of a frame that was freed.

use std::collections::{HashMap, HashSet};

use smith::{lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_1_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(0),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_2_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(1),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_3_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(2),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_4_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(3),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_5_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(4),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_6_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(5),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_7_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(6),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_8_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(7),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_9_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(8),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_10_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(9),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_11_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(10),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_12_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(11),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_13_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(12),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_14_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(13),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_15_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(14),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_16_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(15),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_17_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(16),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_18_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(17),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_19_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(18),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_20_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(19),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_21_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(20),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_22_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(21),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_23_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(22),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_24_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(23),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_25_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(24),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_26_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(25),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_27_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(26),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_28_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(27),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_29_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(28),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_30_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(29),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_31_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(30),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_32_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(31),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_33_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(32),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_34_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(33),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_35_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(34),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_36_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(35),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_37_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(36),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_38_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(37),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_39_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(38),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_40_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(39),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_41_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(40),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_42_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(41),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_43_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(42),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_44_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(43),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_45_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(44),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_46_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(45),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_47_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(46),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_48_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(47),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_49_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(48),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_50_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(49),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_51_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(50),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_52_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(51),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_53_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(52),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_54_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(53),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_55_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(54),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_56_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(55),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_57_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(56),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_58_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(57),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_59_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(58),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_60_of_60",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(59),
    ),
];

/// A task: the connection its parent opened and gave it, if one did, and its
/// steps, each of which opens a connection, may start a child, and waits.
#[derive(Debug, Clone)]
struct Node {
    given: Option<u64>,
    steps: Vec<Step>,
}

#[derive(Debug, Clone)]
struct Step {
    opens: u64,
    child: Option<Node>,
    wait: u64,
}

/// A tree and which of its waits the test makes ready, in the order it makes
/// them, the seed of the scheduler, and whether the first task of the block waits
/// for the others to run: one that does not has the block end before any child
/// ran, whatever it was given.
#[derive(Debug)]
struct Case {
    tree: Node,
    ready: Vec<u64>,
    seed: u64,
    waits: bool,
    /// The task, by its number in the order the functions are written, and the
    /// step of it that asks for more memory than the system gives, so that it
    /// ends by a failure there, when the tree has a task that is not the first.
    fails: Option<(usize, usize)>,
}

struct Ids {
    next: u64,
}

impl Ids {
    fn take(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

fn node(g: &mut Gen, ids: &mut Ids, given: Option<u64>, depth: usize) -> Node {
    let steps = 1 + g.below(3);
    Node {
        given,
        steps: (0..steps)
            .map(|_| {
                let opens = ids.take();
                let child = if depth < 3 && g.below(2) == 0 {
                    // A child is given a connection its parent opens for it, or
                    // opens its own.
                    let gift = (g.below(2) == 0).then(|| ids.take());
                    Some(node(g, ids, gift, depth + 1))
                } else {
                    None
                };
                Step { opens, child, wait: ids.take() }
            })
            .collect(),
    }
}

/// How many tasks the tree has.
fn count(n: &Node) -> usize {
    1 + n.steps.iter().filter_map(|s| s.child.as_ref()).map(count).sum::<usize>()
}

/// How many steps the task that is `which` in the order the functions are written has.
fn steps_of(n: &Node, which: usize) -> usize {
    fn find<'a>(n: &'a Node, which: usize, at: &mut usize) -> Option<&'a Node> {
        let me = *at;
        *at += 1;
        if me == which {
            return Some(n);
        }
        n.steps.iter().filter_map(|s| s.child.as_ref()).find_map(|c| find(c, which, at))
    }
    find(n, which, &mut 0).map_or(1, |n| n.steps.len())
}

fn waits(n: &Node, into: &mut Vec<u64>) {
    for s in &n.steps {
        into.push(s.wait);
        if let Some(c) = &s.child {
            waits(c, into);
        }
    }
}

fn case(g: &mut Gen) -> Case {
    let mut ids = Ids { next: 0 };
    let gift = (g.below(2) == 0).then(|| ids.take());
    let tree = node(g, &mut ids, gift, 0);
    let mut all = Vec::new();
    waits(&tree, &mut all);
    // Some of the waits are made ready, in an order of the generator's.
    let mut ready = Vec::new();
    for w in all {
        if g.below(3) == 0 {
            ready.insert(g.below(ready.len() + 1), w);
        }
    }
    // One of the tasks that are not the first, at one of its steps, fails.
    let tasks = count(&tree);
    let fails = (tasks > 1 && g.below(2) == 0).then(|| {
        let at = 1 + g.below(tasks - 1);
        (at, g.below(steps_of(&tree, at)))
    });
    Case { tree, ready, seed: g.below(1000) as u64, waits: g.below(3) != 0, fails }
}

/// The function of each node, by the number it is given in the order it is met.
fn functions(n: &Node, k: &mut usize, out: &mut String, fails: Option<(usize, usize)>) -> usize {
    let me = *k;
    *k += 1;
    let mut body = String::new();
    let mut joins = Vec::new();
    for (i, s) in n.steps.iter().enumerate() {
        body += &format!("  a{i} = opened({})\n", s.opens);
        if let Some(c) = &s.child {
            let at = functions(c, k, out, fails);
            match c.given {
                Some(id) => {
                    body += &format!("  g{i} = opened({id})\n  t{i} = spawn {{ n{at}(take g{i}) }}\n");
                }
                None => body += &format!("  t{i} = spawn {{ n{at}() }}\n"),
            }
            joins.push(i);
        }
        if fails == Some((me, i)) {
            body += "  more = eat()\n";
        }
        body += &format!("  test_ready({})\n", s.wait);
    }
    body += "  mut total: u64 = 0\n";
    for i in joins {
        // A child that failed gives no value, and its parent goes on.
        body += &format!(
            "  match outcome(take t{i}) {{\n    .ok(v) {{ total = wrapping_add(total, v) }}\n    .err(failure) {{ }}\n  }}\n"
        );
    }
    for i in (0..n.steps.len()).rev() {
        body += &format!("  close(take a{i})\n");
    }
    if n.given.is_some() {
        body += "  close(take c0)\n";
    }
    body += "  total\n";
    let params = if n.given.is_some() { "c0: take Connection" } else { "" };
    *out += &format!("fn n{me}({params}) -> u64 <net, task, console, alloc> {{\n{body}}}\n\n");
    me
}

fn program(c: &Case) -> String {
    let mut functions_text = String::new();
    let mut k = 0;
    let root = functions(&c.tree, &mut k, &mut functions_text, c.fails);
    let start = match c.tree.given {
        Some(id) => format!("  g = opened({id})\n  _ = spawn {{ n{root}(take g) }}"),
        None => format!("  _ = spawn {{ n{root}() }}"),
    };
    let wait = if c.waits { "  late = test_ready_by(999999, ms(10))\n" } else { "" };
    let ready: Vec<String> = c.ready.iter().map(u64::to_string).collect();
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.instant (ms)
use std.int (wrapping_add)
use std.net (Connection, close, connection, handle, net, release)
use std.list (push)
use std.task (Descriptor, join, outcome, spawn, task, test_ready, test_ready_by, test_scheduler_with_io)

// Asks for more memory than the system gives, so that the task that calls it ends.
fn eat() -> u64 <alloc> {{
  mut xs: List[u64] = []
  loop i in 0..9223372036854775807 {{
    xs = push(take xs, i)
  }}
  0
}}

fn opened(id: Descriptor) -> Connection <console, alloc> {{
  print(\"opened {{id}}\")
  connection(id)
}}

{functions_text}fn run() -> u64 <net, diverge, alloc, console> {{
  with test_scheduler_with_io({}, [{}])
{start}
{wait}  print(\"root done\")
  1
}}

fn main() <console, alloc, diverge> {{
  r = net(
    listen_on: fn(host, port) {{ .err(.refused(host)) }},
    accept: fn(l) {{ .err(.closed(l.host)) }},
    stop: fn(l) {{ n = release(take l) }},
    connect_to: fn(host, port) {{ .err(.refused(host)) }},
    read: fn(c, most) {{ .err(.closed(c.host)) }},
    write: fn(c, bytes) {{ .ok(()) }},
    close: fn(c) {{
      print(\"closed {{handle(c)}}\")
      n = release(take c)
    }},
    bind_to: fn(host, port) {{ .err(.refused(host)) }},
    send: fn(s, to, bytes) {{ .err(.closed(s.host)) }},
    receive: fn(s, most) {{ .err(.closed(s.host)) }},
    unbind: fn(s) {{ n = release(take s) }},
    body: {{ run() }},
  )
  print(\"{{r}}\")
}}
",
        c.seed,
        ready.join(", ")
    )
}

/// The connections each task owns, by the node's number in the order of
/// `functions`: its steps' own, the one it was given, and the one it gave its
/// child, which the child owns once it has been given it, and the parent until
/// then. The tree's edges: each node and its parent.
fn owners(n: &Node, k: &mut usize, parent: Option<usize>, owner: &mut HashMap<u64, usize>, up: &mut HashMap<usize, usize>) {
    let me = *k;
    *k += 1;
    if let Some(p) = parent {
        up.insert(me, p);
    }
    if let Some(id) = n.given {
        owner.insert(id, me);
    }
    for s in &n.steps {
        owner.insert(s.opens, me);
        if let Some(c) = &s.child {
            owners(c, k, Some(me), owner, up);
        }
    }
}

fn every_connection_a_generated_tree_of_tasks_opened_is_closed_once(part: u64) {
    generate::check_part(4417, 60, part, 60, case, |c| {
        let source = program(c);
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
        assert_eq!(scalar::check(&program), Ok(()), "for {source}");
        let (outcome, _) = scalar::run_poisoning_freed_frames(&scalar::passes(&program), 1 << 20);
        let text = String::from_utf8(outcome.stdout).expect("text");
        assert_eq!(outcome.status, 0, "{text} for {source}");
        let lines: Vec<&str> = text.lines().collect();
        let mut opened = HashSet::new();
        let mut closed: Vec<u64> = Vec::new();
        let mut after_root: Vec<u64> = Vec::new();
        let mut done = false;
        for line in &lines {
            if let Some(n) = line.strip_prefix("opened ") {
                assert!(opened.insert(n.parse::<u64>().unwrap()), "{text} for {source}");
            } else if let Some(n) = line.strip_prefix("closed ") {
                let n = n.parse::<u64>().unwrap();
                closed.push(n);
                if done {
                    after_root.push(n);
                }
            } else if *line == "root done" {
                done = true;
            }
        }
        // Each connection that was opened is closed once, and none that was not.
        let mut seen = HashSet::new();
        for n in &closed {
            assert!(opened.contains(n), "{n} was closed and not opened: {text} for {source}");
            assert!(seen.insert(*n), "{n} was closed twice: {text} for {source}");
        }
        for n in &opened {
            assert!(seen.contains(n), "{n} was left open: {text} for {source}");
        }
        // Of the connections closed when the block ended, those of a task's
        // descendants are closed before those of the task.
        let (mut owner, mut up, mut k) = (HashMap::new(), HashMap::new(), 0);
        owners(&c.tree, &mut k, None, &mut owner, &mut up);
        let at: HashMap<u64, usize> = after_root.iter().enumerate().map(|(i, n)| (*n, i)).collect();
        for (n, i) in &at {
            let mut task = owner[n];
            while let Some(&parent) = up.get(&task) {
                task = parent;
                for (m, j) in &at {
                    if owner[m] == task {
                        assert!(i < j, "{n} of a descendant was closed after {m}: {text} for {source}");
                    }
                }
            }
        }
    });
}
