---
type: specification
title: Handlers and tasks
description: A child runs under the handlers in force where it was spawned, and several tasks may call one handler, whose operations run one at a time.
status: partial
built:
  - "a child runs under the handlers in force where it was spawned when a handler holds no state, as `fixed` of [Handlers](handlers.md) holds a value it only reads, so every child reads the one value"
  - "a child under a handler that holds state, as `two_readings` and `two_pairs` of this page are under `fake_clock`: the handler's state goes through each poll, so each child reads and changes the one state"
  - "not built: a handler that holds state installed under its scheduler, an operation of a handler that waits or locks, and the operations of a handler that run one at a time when they wait, until the region of [the compiler](../compiler.md#effects) is"
---

# Handlers and tasks

A child runs under the [handlers](handlers.md) in force where it was spawned, with an arena of its own, as [Memory of a task](task-arenas.md) says. So the `fake_clock` of Handlers reaches into the children below.

```
use std.bits (u64)
use std.task (join, spawn, tasks)

fn two_readings() -> (u64, u64) {
  with fake_clock()
  with tasks()
  ta = spawn { now() }
  tb = spawn { now() }
  (join(ta), join(tb))
}
```

## One operation at a time

Several tasks may call one handler. Its operations run one at a time, so the handler's `var`s are never assigned by two tasks at once. Here `fake_clock` adds one to `ticks` for each reading, and two tasks never read the same `ticks`: `two_readings()` is `(1, 2)` or `(2, 1)`, and never `(1, 1)`.

Each operation sees the handler's state whole and leaves it whole, as a read or a write does on a real file system.

## The order is not fixed

The order in which tasks reach a handler is not fixed, so a sequence of operations in one task can be interleaved with another task's. In `two_readings`, either child may call `now()` first.

```
use std.bits (u64)
use std.task (join, spawn, tasks)

fn two_pairs() -> ((u64, u64), (u64, u64)) {
  with fake_clock()
  with tasks()
  ta = spawn { (now(), now()) }
  tb = spawn { (now(), now()) }
  (join(ta), join(tb))
}
```

`two_pairs()` may be `((1, 2), (3, 4))`, and it may be `((1, 3), (2, 4))`, as the readings of the two children interleave.

Next: [Async](async.md)
