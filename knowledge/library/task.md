---
type: specification
title: Task
description: std.task, with the effect task, and tasks, spawn, and join, which open a task group, start a child in it, and wait for the child's value.
status: partial
built:
  - "`std.task` declares the effect `task`, with the operations `start` and `finish`, and has `tasks`, `spawn`, and `join`, with which `with tasks()` opens a group for the rest of its block, `spawn { e }` starts a child that runs `e` under the handlers in force where it is written, and `join(t)` gives the child's value, as [Tasks](../language/tasks.md) says"
  - "the group, and the system's scheduler for a program with no group, run a child to its end in `start` and keep nothing, and a task that waits ends the program with a line on standard error, until the scheduler of part (c) of the roadmap issue is built"
  - "`test_scheduler(seed, body)` is a scheduler written in Smith, for tests: `body` is its first task, it runs the tasks that may run in an order that `seed` picks, a task that joins another is suspended until that one has ended, and one seed always gives one order, so a child that starts a grandchild and joins it gives the same value for every seed, and some seed shows each order of two tasks; it carries `alloc` and `diverge`"
  - "not built: `first`, a child that runs at the same time as its parent, a child that runs under a handler that holds state other than the scheduler, the input and output a test gives to `test_scheduler`, and the scheduler of Linux with `epoll`"
---

# Task

The module `std.task` declares the effect `task`, and has three functions, `tasks`, `spawn`, and `join`. A program imports them as it imports `net`, `use std.task (task, join, spawn, tasks)`, and `task` only where a signature lists it.

```
pub effect task {
  pub fn start(work: take Frame) -> u64
  pub fn finish(child: u64) -> Frame
}

fn tasks[T](body: take fn() -> T ! task) -> T
fn spawn[T](work: take fn() -> T) -> Task[T] ! task
fn join[T](child: take Task[T]) -> T ! task
```

- The two operations are what a scheduler answers, and a scheduler is a handler of them. Only `std.task` handles `task`: a program that calls the effect's name is refused, "`task` is handled only by the schedulers of `std.task`", since a handler that is not one of them could give `finish` a number that no `start` gave, or a child's frame for another's number. A program opens a group with `tasks()`. `start` takes the frame of a child and gives the number the scheduler knows the child by, and `finish` waits until the child with that number has ended and gives its frame back, once. They take and give a `Frame` and a number, and no `T`, so that a handler of `task` is not generic: `spawn` and `join` are the functions of the library that know the type of the value.
- `tasks` is a handler of `task`, so a function that opens a group with [`with`](../language/with.md) does not carry `task`. It holds no state: its `start` runs the child to its end, and its `finish` gives the frame at the number.
- `spawn` starts `work` as a child of the nearest group and gives the child, a `Task[T]`, which holds only the number `start` gave, and is made nowhere else, so a `Task[T]` is always of a frame of a lambda that gives `T`. The lambda `work` is taken, so what it gives away moves into the child, and it is refused where it assigns a `var` of the function it is written in, as [What a spawned task may use](../language/task-sharing.md) says.
- `join` waits for the child and gives its value, by reading the frame that `finish` gives for the number of the `Task[T]` at `T`, which the compiler accepts for that frame and no other, as [the compiler](../compiler.md#effects) says. It takes the child, so a child is joined once.
- `test_scheduler` is a handler of `task`, with a seed, for a test to install with `with test_scheduler(seed)`: the rest of the block is its first task. It starts nothing until a task is waited for, and a task that joins another waits, and is run again once that one has ended, in an order that the seed picks among the tasks that may run, with no thread and no real wait. A program under it is the same source as under any other scheduler. Its state is the list of the tasks it started and the list of those that ended, which goes through each poll, as [the compiler](../compiler.md#effects) says.
- `std.task` also defines `Frame`, `Wait`, `Handle`, and `Poll`, which a scheduler is written with beside the compiler's `__frame`, `__poll`, `__result`, `__wait`, `__address`, and `__frame_at`. A program has no use for them, and cannot make a `Frame`.

```
use std.bits (u64)
use std.int (wrapping_mul)
use std.task (join, spawn, tasks)

fn doubled(n: u64) -> u64 {
  with tasks()
  t = spawn { wrapping_mul(n, 2) }
  join(t)
}
```
