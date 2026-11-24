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
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_1_of_4",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(0),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_2_of_4",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(1),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_3_of_4",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(2),
    ),
    (
        "every_connection_a_generated_tree_of_tasks_opened_is_closed_once_whatever_point_each_task_is_cancelled_at_part_4_of_4",
        || every_connection_a_generated_tree_of_tasks_opened_is_closed_once(3),
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
    Case { tree, ready, seed: g.below(1000) as u64, waits: g.below(3) != 0 }
}

/// The function of each node, by the number it is given in the order it is met.
fn functions(n: &Node, k: &mut usize, out: &mut String) -> usize {
    let me = *k;
    *k += 1;
    let mut body = String::new();
    let mut joins = Vec::new();
    for (i, s) in n.steps.iter().enumerate() {
        body += &format!("  a{i} = opened({})\n", s.opens);
        if let Some(c) = &s.child {
            let at = functions(c, k, out);
            match c.given {
                Some(id) => {
                    body += &format!("  g{i} = opened({id})\n  t{i} = spawn {{ n{at}(g{i}) }}\n");
                }
                None => body += &format!("  t{i} = spawn {{ n{at}() }}\n"),
            }
            joins.push(i);
        }
        body += &format!("  test_ready({})\n", s.wait);
    }
    body += "  var total: u64 = 0\n";
    for i in joins {
        body += &format!("  total = wrapping_add(total, join(t{i}))\n");
    }
    for i in (0..n.steps.len()).rev() {
        body += &format!("  close(a{i})\n");
    }
    if n.given.is_some() {
        body += "  close(c0)\n";
    }
    body += "  total\n";
    let params = if n.given.is_some() { "c0: take Connection" } else { "" };
    *out += &format!("fn n{me}({params}) -> u64 ! net, task, console, alloc {{\n{body}}}\n\n");
    me
}

fn program(c: &Case) -> String {
    let mut functions_text = String::new();
    let mut k = 0;
    let root = functions(&c.tree, &mut k, &mut functions_text);
    let start = match c.tree.given {
        Some(id) => format!("  g = opened({id})\n  spawn {{ n{root}(g) }}"),
        None => format!("  spawn {{ n{root}() }}"),
    };
    let wait = if c.waits { "  late = test_ready_by(999999, 10)\n" } else { "" };
    let ready: Vec<String> = c.ready.iter().map(u64::to_string).collect();
    format!(
        "use std.bits (u64)
use std.console (console, print)
use std.int (wrapping_add)
use std.net (Connection, close, connection, handle, net, release)
use std.task (join, spawn, task, test_ready, test_ready_by, test_scheduler_with_io)

fn opened(id: u64) -> Connection ! console, alloc {{
  print(\"opened {{id}}\")
  connection(id)
}}

{functions_text}fn run() -> u64 ! net, diverge, alloc, console {{
  with test_scheduler_with_io({}, [{}])
{start}
{wait}  print(\"root done\")
  1
}}

fn main() ! console, alloc, diverge {{
  r = net(
    listen_on: fn(host, port) {{ .err(.refused) }},
    accept: fn(l) {{ .err(.closed) }},
    stop: fn(l) {{ n = release(l) }},
    connect_to: fn(host, port) {{ .err(.refused) }},
    read: fn(c, most) {{ .err(.closed) }},
    write: fn(c, bytes) {{ .ok(()) }},
    close: fn(c) {{
      print(\"closed {{handle(c)}}\")
      n = release(c)
    }},
    bind_to: fn(host, port) {{ .err(.refused) }},
    send: fn(s, to, bytes) {{ .err(.closed) }},
    receive: fn(s, most) {{ .err(.closed) }},
    unbind: fn(s) {{ n = release(s) }},
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
    generate::check_cases(4417, part * 15, 15, case, |c| {
        let source = program(c);
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
        assert_eq!(scalar::check(&program), Ok(()), "for {source}");
        let (outcome, _) = scalar::run_poisoning_freed_frames(&scalar::passes(&program));
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
