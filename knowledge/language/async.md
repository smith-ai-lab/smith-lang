---
type: specification
title: Async
description: All I/O is async and reads as sequential code, suspending a task under an async handler and blocking under a blocking one.
status: partial
built:
  - "a call that waits for a connection, `read`, `write`, or `accept` of `net`, suspends its task under the scheduler in force, on Linux with `epoll` and on macOS, on both chips, with `kqueue`, and blocks under a blocking handler, as the source is the same for both"
---

# Async

All I/O is async and reads as sequential code. A call such as `fetch(a)` carries the `net` effect, and the source is the same whichever handler runs it.

```
fn fetch_pair(a: str, b: str) -> Result[(List[Byte], List[Byte]), NetError] ! net, alloc {
  pa = try fetch(a)
  pb = try fetch(b)
  .ok((pa, pb))
}
```

Under the scheduler in force the call suspends its task. Under a blocking handler it blocks. `fetch_pair` is written once for both.

## Who may suspend a task

The scheduler in force suspends a task. The scheduler is a handler of [`task`](tasks.md), and which one is in force is decided by the program that installs it, as for any handler, so the source is the same under every scheduler. Every other handler written in Smith returns from each operation, so a call under one does not suspend. `fixed_clock` of [Handlers](handlers.md) is such a handler: `now()` under it gives its value and returns.

## What it compiles to

- A function with a suspending effect compiles to a state machine whose frame lives in its task's arena. Tasks need no stacks of their own. `fetch_pair` is such a function, since it carries `net`, which suspends under the scheduler. [The compiler](../compiler.md#effects) says how the state machine is made.
- Pure code compiles as ordinary functions. `reading` of [Handlers](handlers.md) carries no effect, so it is an ordinary function.

## The forms

| What the call is under | What it does |
| --- | --- |
| The scheduler in force | Suspends its task |
| A blocking handler of the operating system | Blocks |
| A handler written in Smith | Returns from the operation |

Next: [Cancellation](cancellation.md)
