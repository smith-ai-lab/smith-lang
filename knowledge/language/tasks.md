---
type: specification
title: Tasks
description: A task group opened with tasks(), children started with spawn and waited for with join, and the task effect.
status: partial
built:
  - "`with tasks()`, `spawn`, and `join`, imported from `std.task`, with `fetch_one` and `fetch_both` as the page writes them, under a handler of `fetch` that a test writes: `join(t)` is what the child's lambda gave"
  - "a child's lambda runs exactly once if it is joined, a child that is not joined is cancelled when the block ends, and nothing of a child runs after its group's block has ended"
  - "a function that spawns with no group of its own carries `task`, and its caller that opens the group does not; leaving `task` out is refused: \"`spawn` needs the `task` effect, which `start` does not declare\""
  - "`task` is declared by `std.task` and is imported as `net` is, `use std.task (task, join, spawn, tasks)`; a group that no scheduler is in force of is the scheduler of its children, which wait in `epoll` on Linux and in `kqueue` on macOS, as [Task](../library/task.md) says, and a group inside a task that a scheduler runs hands its children to that scheduler, so no child is found still running when the block ends"
  - "only `std.task` handles `task`, with `tasks()` and the schedulers it has, and a program that writes a handler of it is refused"
  - "the operations of `net` suspend the task that calls them in a group, `read`, `write`, and `accept` waiting for the connection while other tasks run, on Linux and on macOS, and a function that opens a group needs no more than before"
  - "`with test_scheduler(seed)` of `std.task` runs the tasks in an order that the seed picks, so that a test finds a mistake of order: one seed always gives one order, and a child may start a grandchild and join it, as [Task](../library/task.md) says"
  - "not built: `first`, and a child that runs at the same time as its parent, and a child that runs under a handler that holds state, which is refused as not built yet, as [Handlers and tasks](task-handlers.md) says"
---

# Tasks

Concurrent work runs in tasks, and there are no raw threads or locks. Data is immutable or uniquely owned, so data races cannot be written. `with tasks()` opens a task group. `spawn` takes the child's work as a [lambda](lambdas.md) and starts it in the nearest group, and `join` waits for the child and gives its value.

`fetch` is shown by its signature alone. A call such as `fetch(a)` carries the `net` effect.

```
use std.task (join, spawn, tasks)

fn fetch(url: str) -> Result[List[Byte], NetError] ! net, alloc

fn fetch_one(url: str) -> Result[List[Byte], NetError] ! net, alloc {
  with tasks()
  t = spawn { fetch(url) }
  join(t)
}
```

`t` is the child, and `join(t)` is what `fetch(url)` gave. `join` takes the child, and a `Task` has no `Copy`, so a child is joined once: a second `join(t)` is refused as a use of `t` after it was given away. A `Task` has `Discard`, so a child that is never joined is allowed.

## Two children

A child runs at the same time as its parent, so two spawns run two children together. Each is joined for its value.

```
use std.task (join, spawn, tasks)

fn fetch_both(a: str, b: str) -> Result[(List[Byte], List[Byte]), NetError] ! net, alloc {
  with tasks()
  ta = spawn { fetch(a) }
  tb = spawn { fetch(b) }
  pa = try join(ta)
  pb = try join(tb)
  .ok((pa, pb))
}
```

`join` gives what the child gave, here a `Result`, so [`try`](try.md) takes it.

## The first of several

`first` waits for the first of several children of the same group to finish, and gives its value. It takes one child and the list of the others, so that it is never asked to wait for none, and the children give one type.

```
use std.task (join, spawn, tasks)

fn quickest(a: str, b: str, c: str) -> Result[List[Byte], NetError] ! net, alloc {
  with tasks()
  ta = spawn { fetch(a) }
  tb = spawn { fetch(b) }
  tc = spawn { fetch(c) }
  first(ta, [tb, tc])
}
```

`quickest` gives the page of whichever server answers first. When the children do different work, the work gives one enum, and the enum says which child finished.

- A child that `first` waits for belongs to the nearest group, as for `join`, and one of another group is refused.
- The child that finished counts as joined, so `join` of it, or `first` with it again, is refused. The others are not waited for: they keep running until they are joined, or until the block ends and cancels them.
- When two children have finished together, the one that comes first, `a` and then `rest` in order, is the one `first` gives, so the result is the same every run.

## No child outlives the block

When the block of `with tasks()` ends, the children still running are cancelled, and the block waits until they have ended. A child whose work must finish is joined, as above. This function does not join its child, so the child is cancelled when the block ends, and the function gives nothing.

```
use std.task (join, spawn, tasks)

fn warm(url: str) ! net, alloc {
  with tasks()
  spawn { fetch(url) }
  ()
}
```

[Cancellation](cancellation.md) says what cancelling does.

## A function that spawns without a group

A function that spawns without opening its own group spawns into its caller's, and it carries the `task` effect, which `std.task` declares. The caller that opens the group does not carry it.

```
use std.bits (i32)
use std.task (spawn, task, tasks)

fn start(n: i32) ! task {
  spawn { n * 2 }
  ()
}

fn start_two() {
  with tasks()
  start(1)
  start(2)
}
```

Leaving `task` out of `start` is refused, as is any effect left out of a signature: "`spawn` needs the `task` effect, which `start` does not declare", with the fix "write `fn start(n: i32) ! task`".

## The forms

| Written | Is |
| --- | --- |
| `use std.task (join, spawn, tasks)` | Brings in the three functions, which a group needs; a function that carries the effect imports `task` too |
| `with tasks()` | A task group for the rest of the block |
| `spawn { e }` | A child that runs `e`, in the nearest group |
| `join(t)` | The value of the child `t`, after waiting for it |
| `first(t, rest)` | The value of the first of `t` and the children in `rest` to finish |
| `! task` | A function that spawns into its caller's group |

Next: [What a spawned task may use](task-sharing.md)
