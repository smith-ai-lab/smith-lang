---
type: specification
title: How a task ends
description: Three things end a task before its work is done, and what is dropped and closed when one does.
status: partial
built:
  - "a handle of `net` that a task owns when it is asked to end is closed by the handler in force, in a struct, a tuple, an array, an enum, or a list, and a task that is asked to end before it ran ends what its lambda was given, and a function that holds one while it may wait and does not carry `net` is refused where it is compiled"
  - "a task that runs out of memory, or makes a console write the system refuses, ends and the program goes on, on x86-64 and arm64, and a `join` of it ends the program with the status 1"
  - "not built: a failed task does not yet close the handles it opened since its last wait, nor the children it started since then, and `join` of it does not yet fail the joiner; nor are the handles of `fs`, nor WebAssembly, where a failure traps"
---

# How a task ends

Three things end a task before its work is done. Two are failures: running out of memory, and a console write that the system refuses, as when output is piped to a program that has stopped reading. The third is being [cancelled](cancellation.md).

```
use std.console (console, print)

fn log_all(lines: List[str]) ! console, alloc {
  loop line in lines {
    print(line)
  }
}
```

If the system refuses a `print` of `log_all`, the task that runs it ends there, and the lines after it are never printed.

## What happens in each case

In each case the task's arena is dropped whole, the `fs` and `net` handlers close the handles it left open, and the code it was running sees nothing and runs no further. No other task shares its memory, so none is left in a broken state.

```
fn header(path: str) -> Result[List[Byte], FsError] ! fs, alloc {
  with f = open(path)
  read(f, 0..64)
}
```

`open` and `read` are those of [`std.fs`](../library/fs.md). If the task running `header` ends in the middle of `read`, the `fs` handler closes `f`, and `header` does not return.

## A handle held while a task may wait

A task that is cancelled while it waits is ended by the handlers: each handle of `net` that the function it is in owns is closed through the handler in force, a test's handler included, once. So a function that owns a handle, or a value that holds one, while it calls something that may wait must carry `net`, and one that does not is refused where it is compiled.

```
use std.net (Connection)
use std.time (sleep, time)

fn relay(c: take Connection) -> Connection ! time {
  sleep(10)
  c
}
```

`relay` is refused at `sleep`: "`relay` holds a `Connection` while it may wait, so it must carry `net`", and the fix is "add `net` to its effects". A handle that a function has given to another function, or moved into a child, is not owned there, so it is closed once, by the task that owns it.

## Who ends a task

Only the operating system's handlers end a task. A handler written in Smith returns from each operation, as [Async](async.md) says.

## The forms

| What ended the task | Which |
| --- | --- |
| Running out of memory | A failure |
| A console write the system refuses | A failure |
| Being cancelled | Not a failure |

Next: [A failed task](task-failures.md)
