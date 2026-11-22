---
type: specification
title: Time
description: The time effect, by which a program reads the clock and waits for a time, and how a wait is cancelled.
status: partial
built:
  - "`std.time` declares `time`, with `now` and `sleep` as the page shows them"
  - "on Linux and macOS, on x86-64 and arm64, the system answers both: `now` gives the milliseconds since 1970 of the system's clock, and `sleep` blocks for at least the milliseconds it is given; a handler answers both on every target"
  - "not Windows, a `sleep` that suspends its task, or one that a cancelled task ends"
---

# Time

The module `std.time` declares the `time` [effect](../language/effect-declarations.md). It is how a program reads the clock and waits for a time.

```
effect time {
  fn now() -> u64
  fn sleep(ms: u64)
}
```

| Operation | Does |
| --- | --- |
| `now` | Gives the clock's reading, in milliseconds |
| `sleep` | Waits for `ms` milliseconds, and then returns |

```
use std.bits (u64)
use std.time (now, sleep, time)

fn pause_and_read() -> u64 ! time {
  sleep(250)
  now()
}
```

- `sleep` suspends the task that calls it, as a wait for a file or the network does, so it does not hold up the other tasks. A task that is cancelled while it sleeps ends at once, as [Cancellation](../language/cancellation.md) says of any task that is waiting, and the rest of the `sleep` does not run.
- A function that calls `sleep` lists `time`, as one that calls `now` does, and a function that lists no effect never waits for a time.
- A handler answers both operations, so a test gives the clock the readings it wants and a `sleep` that returns at once, with no wait.
- The first of a child that sleeps and a child that works is how a program gives up on work that takes too long, as [Cancellation](../language/cancellation.md#a-deadline) shows.
