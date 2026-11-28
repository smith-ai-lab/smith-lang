---
type: specification
title: Console
description: The console effect, for a program's standard input, output, and error, and how to handle it.
status: partial
built:
  - "`std.console` declares `Stream`, with `stdout` and `stderr`, and the effect `console` with `print(text, end, to)`, which a program calls with `end` and `to` as written or left out, `end` being `\"\\n\"` and `to` being `.stdout`; the compiler builds `print` from this declaration"
  - "with no handler in force the target writes the text and then `end` to the stream `to`, which is chosen when the program runs where it is not written as `.stdout` or left out"
  - "a handler of `console` gives a lambda for `print` that takes `text`, `end`, and `to`, as written and as defaulted, and it answers a call of `print` in the code under `with`, in a function that carries `console`, and in a function that one calls, under the handlers outside it; a handler that does not forward a call to the console outside it prints nothing"
  - "`input(prompt)` of `std.console`, which a program calls with `prompt` as written or left out, `\"\"` being the default; with no handler in force the target writes the prompt to standard output before it reads, then gives `.some` of the next line of standard input without its line break, `\\n` or `\\r\\n` ending a line, and a last line that has no line break is a line, and `.none` when standard input has ended; a line of any length is given whole, and reading takes from standard input only the bytes of the line"
  - "`input` on every native target, which are Linux, macOS, and Windows on x86-64 and arm64, and on the evaluator, and in a handler, whose lambda for it, such as `fn(prompt) { input(prompt) }`, hands the call to the console outside it"
  - "not `input` in a WebAssembly module, which is refused with one message"
---

# Console

The module `std.console` declares the `console` [effect](../language/effect-declarations.md). It covers the three standard streams of a program, which the operating system grants together.

```
enum Stream {
  stdout
  stderr
}

effect console {
  fn print(text: str; end: str = "\n", to: Stream = .stdout)
  fn input(prompt: str = "") -> Option[str] <alloc>
}
```

| Operation | Does |
| --- | --- |
| `print` | Writes the text and then `end` to the stream `to` |
| `input` | Writes the prompt to standard output, then returns the next line of standard input without its line break, or `.none` at the end of input |

```
print("hello world")
print("loading", end: "")
print("could not open the file", to: .stderr)
```

- `print`, `input`, `Stream`, and the effect `console` are imported from `std.console`.
- The console is a program's real output, which another program may read, so the text is written as it is given and nothing is formatted.
- Each operation is whole. A line that one task prints is never split by another task's output.
- A write that the system refuses [ends the task](../language/task-ending.md) that made it, as running out of memory does, with the failure `refused_write`. A parent that joins it ends by the same failure unless it uses `outcome`, as [A failed task](../language/task-failures.md) says, and in `main` the program ends with a failing status. A handler written in Smith never does this.

```
use std.console (console, input, print)

fn greet() <console, alloc> {
  match input("name: ") {
    .some(name) { print("hello {name}") }
    .none { print("hello") }
  }
}
```

A handler supplies both operations. This one keeps everything printed to standard output for a test to look at, and passes the rest to the console outside it, which is why it carries `console` itself.

```
use std.console (console, input)
use std.list (push)

fn capture[T](body: fn() -> T <console>) -> (T, List[str]) <console, alloc> {
  mut seen: List[str] = []
  r = console(
    print: fn(text, end, to) {
      match to {
        .stdout { seen = push(seen, "{text}{end}") }
        .stderr { print(text, end: end, to: .stderr) }
      }
    },
    input: fn(prompt) { input(prompt) },
    body: body,
  )
  (r, seen)
}
```
