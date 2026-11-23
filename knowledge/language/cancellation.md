---
type: specification
title: Cancellation
description: Ending a group's block is the one way to cancel a task, and a cancelled task ends at once if it is waiting and at its next suspension if it is running.
status: partial
built:
  - "when the block of a scheduler ends, in a native executable and under `test_scheduler`, each task that is still in its group is ended and the frame and the stack it held go back to the pool, the one started last first, so a child is ended before the task that started it, and a block that leaves children waiting for ever, 1,000 times over, asks the system for the memory it asks for 20 times over"
  - "a task is asked to end by a bit of its frame, which each function the task is in reads from its own frame, and after each call that may suspend the task a function leaves at once when the bit is set, with zeros for what it gives, as the call it is in is left by its own callee; the scheduler does not set the bit yet, and a call inside the block of a loop over a value has no such exit"
  - "not built: a task that is running and ends at its next suspension, the ending of the values a task held, a group opened inside a task that a scheduler runs, which ends its own children, and the block waiting for the children it cancelled"
---

# Cancellation

Ending a group's block is the one way to cancel. When the block of `with tasks()` ends, each child still running is cancelled. User code cannot end a task in any other way.

```
fn first_page(a: str, b: str) -> Result[List[Byte], NetError] ! net, alloc {
  with tasks()
  ta = spawn { fetch(a) }
  tb = spawn { fetch(b) }
  join(ta)
}
```

`first_page` joins `ta` only. When the block ends, `tb` is cancelled if it has not finished.

## A deadline

Giving up on work that takes too long is ordinary code: spawn the work and a wait, take the first to finish, and let the end of the block cancel the other. This fetches a page, or says that 5 seconds passed.

```
use std.fetch (FetchError, Request, Response, fetch, send)
use std.time (sleep, time)

enum Outcome {
  page: Result[Response[65536], FetchError]
  late
}

fn fetch_within_5_seconds(url: str) -> Result[Response[65536], FetchError] ! fetch, time, alloc {
  with tasks()
  work = spawn { Outcome.page(send(Request(url: url), 65536)) }
  wait = spawn {
    sleep(5000)
    Outcome.late
  }
  match first(work, [wait]) {
    .page(result) { result }
    .late { .err(.failed("the time ran out after 5 seconds")) }
  }
}
```

If the page comes first, the sleeping child is cancelled when the block ends, and it ends at once, as a waiting task does. If the 5 seconds come first, the fetch is cancelled the same way, and what `send` was waiting for is not waited for. A call of an operation takes no time to give up of its own: the deadline is the second child.

## When it takes effect

A cancelled task that is waiting, as for a read, ends at once. One that is running ends the next time it would suspend, so code that never suspends is never interrupted.

```
use std.bits (u64)
use std.int (wrapping_add)

fn busy(n: u64) -> u64 {
  var total: u64 = 0
  loop i in 0..n { total = wrapping_add(total, i) }
  total
}

fn abandon(n: u64) {
  with tasks()
  spawn { busy(n) }
  ()
}
```

`busy` carries no effect and so never suspends. The block of `abandon` ends at once, and the block waits until `busy` has ended on its own, as [Tasks](tasks.md) says.

## Its own children

A task's own children are cancelled with it.

```
fn outer(a: str) ! net, alloc {
  with tasks()
  spawn {
    with tasks()
    spawn { fetch(a) }
    ()
  }
  ()
}
```

When the block of `outer` ends, the child is cancelled, and the grandchild that fetches with it.

## The forms

| The task | Ends |
| --- | --- |
| Waiting, as for a read | At once |
| Running and about to suspend | When it would suspend |
| Running and never suspending | Not interrupted |
| Having children | They are cancelled with it |

Next: [How a task ends](task-ending.md)
