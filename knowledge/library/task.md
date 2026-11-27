---
type: specification
title: Task
description: std.task, with the effect task, and tasks, spawn, and join, which open a task group, start a child in it, and wait for the child's value.
status: partial
built:
  - "`std.task` declares the effect `task`, with the operations `start` and `finish`, and has `tasks`, `spawn`, and `join`, with which `with tasks()` opens a group for the rest of its block, `spawn { e }` starts a child that runs `e` under the handlers in force where it is written, and `join(t)` gives the child's value, as [Tasks](../language/tasks.md) says"
  - "a group that no scheduler is in force of is the scheduler of its children, with `epoll` on Linux and `kqueue` on macOS: it runs each child until it waits, and when every child waits for the system it waits in `epoll_wait` or `kevent`, on the one thread of the program, so a connection that says nothing holds up no other; a group inside a task that a scheduler runs hands each `spawn` to that scheduler, and never opens a second loop; a program with no group is built as it was"
  - "`test_scheduler(seed, body)` is a scheduler written in Smith, for tests: `body` is its first task, it runs the tasks that may run in an order that `seed` picks, a task that joins another is suspended until that one has ended, and one seed always gives one order, so a child that starts a grandchild and joins it gives the same value for every seed, and some seed shows each order of two tasks; it carries `diverge` and builds nothing, since a scheduler holds only numbers"
  - "`test_scheduler_with_io(seed, ready, body)` is the same with the I/O of a test: the handles of `ready` become ready one at a time, in that order, each when no task can run, and wake the tasks that wait for it with `test_ready(handle)`, so a test fixes the order in which tasks that wait for the system go on, for every seed"
  - "a child under a handler that holds state, whose state goes through each poll, and whose memory, as a list that an operation pushes onto, is built in a region of its own that the block that installs the handler gives back, as [the compiler](../compiler.md#effects) says"
  - "`task` has a third operation, `quiet`, for the library alone, which a program that calls it is refused: it waits until every task other than the one that asks has ended, and the schedulers answer it, so that `listen` of `std.http` serves the connections it took before it returns; a program waits for a task by its handle, as before"
  - "when the block of a scheduler ends, each task of its group that has not ended is ended and its memory given back, as [Cancellation](../language/cancellation.md) says"
  - "a task waits for a time with `.time(ms)`, which is ms milliseconds from when it waits, and for a handle until a time with `.read_by` and `.write_by`, whichever comes first, whose milliseconds the task gives to the scheduler by `limit` just before it waits, so that the handle and the milliseconds are each held whole; `task` has two more operations, `timed_out` and `limit`, for the library alone, of which `timed_out` says whether the task that asks was woken by its time and not by its handle, and `wait_to_read_by(handle, ms)` and `wait_to_write_by` give that as `false`; the group waits in `epoll_wait` or `kevent` with the timeout of the nearest time and reads the clock only while a task waits for a time, and the scheduler of a test goes its own time, which starts at 0, forward to the nearest time when no task can run, offering a handle of `ready` first when a task waits for it; the time a task waits for is in the fourth word of its frame's header, so the schedulers ask for no memory for it"
  - "a task that is cancelled, by the end of the block of its scheduler or by a parent that ends, ends each child it holds and each handle of `net` that it owns, closed by the handler in force, and what a task that has not run yet was given, as [Cancellation](../language/cancellation.md) and [How a task ends](../language/task-ending.md) say"
  - "`join` of a child that ended by a failure ends the task that joins it by the same failure, and `outcome(t)` waits as `join` does and gives `.ok` of the value or `.err` of a `Failure`, `out_of_memory` or `refused_write`, so the task that calls it goes on, as [A failed task](../language/task-failures.md) says"
  - "not built: the ending of the values a cancelled task held other than children and handles of `net`, as [Cancellation](../language/cancellation.md) says"
  - "not built: `first`, which gives the value of the child that finished and the others as `(T, List[Task[T]])`, a child that runs at the same time as its parent, and a handler that holds state installed under the scheduler of a child, which is refused as not built yet"
---

# Task

The module `std.task` declares the effect `task`, and has three functions, `tasks`, `spawn`, and `join`. A program imports them as it imports `net`, `use std.task (task, join, spawn, tasks)`, and `task` only where a signature lists it.

```
pub effect task {
  pub fn start(work: take Frame) -> u64
  pub fn finish(child: u64) -> Frame
}

fn tasks[T](body: take fn() -> T <task>) -> T
fn spawn[T](work: take fn() -> T) -> Task[T] <task>
fn join[T](child: take Task[T]) -> T <task>
fn outcome[T](child: take Task[T]) -> Result[T, Failure] <task>
fn first[T](child: take Task[T], rest: take List[Task[T]]) -> (T, List[Task[T]]) <task>

pub enum Failure { out_of_memory, refused_write }
```

- The two operations are what a scheduler answers, and a scheduler is a handler of them. Only `std.task` handles `task`: a program that calls the effect's name is refused, "`task` is handled only by the schedulers of `std.task`", since a handler that is not one of them could give `finish` a number that no `start` gave, or a child's frame for another's number. A program opens a group with `tasks()`. `start` takes the frame of a child and gives the number the scheduler knows the child by, and `finish` waits until the child with that number has ended and gives its frame back, once. A task that is asked to end while it waits asks the child to end and goes on waiting, so the frame it is given is of a child that has ended. They take and give a `Frame` and a number, and no `T`, so that a handler of `task` is not generic: `spawn` and `join` are the functions of the library that know the type of the value.
- `tasks` is a handler of `task`, so a function that opens a group with [`with`](../language/with.md) does not carry `task`. Where no scheduler is in force, which the compiler knows when it builds the copy, the group is the scheduler of its children: its state is its lists of the tasks it started and ended and of those that wait, with the descriptor of `epoll` or `kqueue`, and it runs one child at a time until the child waits, then another, and waits in `epoll_wait` or `kevent` only when every child waits for the system, so the program holds one thread however many connections it serves. Where a scheduler is in force, because the group is opened by a child that a group or `test_scheduler` runs, each `spawn` of the group is handed to that scheduler, and `join` waits there, so two groups, one inside the other, share one loop and a task of the outer group runs while the inner one waits.
- `spawn` starts `work` as a child of the nearest group and gives the child, a `Task[T]`, which holds only the number `start` gave, and is made nowhere else, so a `Task[T]` is always of a frame of a lambda that gives `T`. The lambda `work` is taken, so what it gives away moves into the child, and it is refused where it assigns a `var` of the function it is written in, as [What a spawned task may use](../language/task-sharing.md) says.
- `join` waits for the child and gives its value, by reading the frame that `finish` gives for the number of the `Task[T]` at `T`, which the compiler accepts for that frame and no other, as [the compiler](../compiler.md#effects) says. It takes the child, so a child is joined once: a `Task[T]` has no `Copy`, which `std.task` says with `without Copy for Task`, so a second `join` of one handle is refused as any use of a value after it was given away, and the handle keeps `Discard`, so it may be dropped.
- `time_left(deadline, now)` is for the library and its tests: the `Duration` from the `Instant` `now` to the `Instant` `deadline`, and none when the time has come. A group asks the same of each task that waits for a time, with the one `now` it reads for a turn, so each task is woken or waited for. The waits take a `Duration` and not a number: `wait_for_time`, `wait_to_read_by`, `wait_to_write_by`, and `test_ready_by`.
- `outcome` waits for the child as `join` does, and gives `.ok` of its value, or `.err` of the `Failure` that ended it, so the task that calls it goes on, where `join` of a child that ended by a failure ends the joining task by the same failure. `Failure` has a case for each failure that [ends a task](../language/task-ending.md), and a failure added later is a case added.
- `test_scheduler` is a handler of `task`, with a seed, for a test to install with `with test_scheduler(seed)`: the rest of the block is its first task. It starts nothing until a task is waited for, and a task that joins another waits, and is run again once that one has ended, in an order that the seed picks among the tasks that may run, with no thread and no real wait. `test_scheduler_with_io(seed, ready)` is the same with the I/O a test gives: a task that calls `test_ready(handle)` waits until the handle is made ready, the handles of `ready` are made ready in its order, each when no task can run, and a handle that a task does not wait for yet is not ready before it does. A program under it is the same source as under any other scheduler. Its state is the list of the tasks it started and the list of those that ended, which goes through each poll, as [the compiler](../compiler.md#effects) says.
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
