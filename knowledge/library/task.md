---
type: specification
title: Task
description: std.task, with tasks, spawn, and join, which open a task group, start a child in it, and wait for the child's value.
status: partial
built:
  - "`std.task` has `tasks`, `spawn`, and `join`, with which `with tasks()` opens a group for the rest of its block, `spawn { e }` starts a child that runs `e` under the handlers in force where it is written, with an arena of its own, and `join(t)` gives the child's value, as [Tasks](../language/tasks.md) says"
  - "not built: `first`, and a child that runs at the same time as its parent"
---

# Task

The module `std.task` has three functions, `tasks`, `spawn`, and `join`. The effect `spawn` is declared by no module and needs no import, as [The effects Smith provides](../language/provided-effects.md) says, but the functions are imported.

```
fn tasks[T](body: take fn() -> T ! spawn) -> T
fn spawn[T](work: take fn() -> T) -> Task[T] ! spawn
fn join[T](task: take Task[T]) -> T
```

- `tasks` runs `body` with `spawn` handled, so a function that opens a group with [`with`](../language/with.md) does not carry `spawn`. It is a handler of `spawn`, which has no operation, so its body is `spawn(body: body)`: the effect's name called with the body and nothing else, as [Handlers](../language/handlers.md) says a handler is written.
- `spawn` starts `work` as a child of the nearest group and gives the child, a `Task[T]`. The lambda `work` is taken, so what it gives away moves into the child, and it is refused where it assigns a `var` of the function it is written in, as [What a spawned task may use](../language/task-sharing.md) says. The child has an arena of its own, as [Memory of a task](../language/task-arenas.md) says.
- `join` waits for the child and gives its value. It takes the child, so a child is joined once.

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
