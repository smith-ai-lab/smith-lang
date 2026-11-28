---
type: specification
title: Running tests
description: smith test, which runs every test or the ones chosen, in parallel, and what a failure and a run print.
status: partial
built:
  - "`smith test`, which finds every file `x_test.smith` below the folder it runs in, but in hidden folders, `smith test <file>`, which runs the tests of that file, and `smith test --name <text>`, which runs the tests whose name holds the text, of every file or of the file given"
  - "all tests chosen run in one program, each as a task of [Tasks](tasks.md) with an arena of its own, as [Memory of a task](task-arenas.md) says: they run in waves of as many as the machine has processors, and a wave's tests run in turns while one of them waits, so a test that has ended holds no memory and every test gives the result it gives alone; a child does not yet run at the same time as its parent, so a test that does not wait runs to its end before the next starts"
  - "the program is built of the files of the tests chosen and what they reach, so a mistake in a module that none of them imports is not said; a file of tests that is refused is said, counts as one failed, and the others are built again without it, and a mistake in a module that is not a file of tests refuses the run"
  - "what a run prints: `passed` or `failed` with the file and the name of each test, on standard output, what each failure says, on standard error, how many passed and failed, and then `slowest:` and the five slowest tests, each with how long it took in milliseconds; a test that ends by running out of memory, or by a write that is refused, fails, and the others go on; the run ends with status 1 when a test fails or a file of tests is refused, and 0 otherwise"
  - "a test's effects, which are the real ones, and a handler a test installs with `with`, which answers in their place"
  - "a test that takes a `Key`, which is a [property](properties.md) and is said as `passed` with `(100 runs)` after its name, and a failure of one, which ends with the seed of the run"
  - "`smith test --seed <n>`, which runs with the seed `n`, so that a run gives the same keys, and the same results, as the run whose failure said that seed; a run given no seed makes one of its own, which differs from one run to the next"
---

# Running tests

`smith test` runs every test of the project.

```
smith test
```

All tests run in one process, each as a task with its own arena, in parallel. A run says of each test whether it passed, prints what each failure says, and ends with how many passed and failed, and then its slowest tests and how long each took. It ends with status 1 when a test failed, and 0 when every test passed.

## Choosing tests

`smith test` takes the tests to run.

```
smith test                           everything
smith test model/gpt_test.smith      one file
smith test --name round_trip         tests whose name matches
smith test --seed 4127               replay a failing run
```

Running one test compiles only what that test reaches, so it stays fast in a large project.

## When a test fails

A failure prints the smallest failing input and the seed. `smith test --seed` with that seed replays the run.

```
smith test --seed 4127
```

## What a test may do

A test is a function like any other, and it declares its effects as one. What it declares is the real thing, as it is for `main`: a test that lists `fs` reads and writes the files of the machine.

```
use std.fs (fs, read_all)
use std.test (Failed, expect)

fn test_reads_the_cases() -> Result[(), Failed] <fs, alloc> {
  bytes = try read_all("cases/hello.smith")
  expect { bytes.size > 0 }
}
```

Nothing is put in an effect's place without being written. A test that must not touch the machine handles the effect itself, as [Handlers](handlers.md) describes for `memory_fs`, and then it does not list the effect.

```
fn test_config() -> Result[(), Failed] <alloc> {
  with memory_fs(fixtures)
  cfg = try load("model.toml")
  expect { cfg.layers == 12 }
}
```

So the signature says which kind a test is: one that lists `fs`, `net`, or `time` touches the machine, and one that lists none of them gives the same answer on every machine.

## The forms

| Written | Means |
| --- | --- |
| `smith test` | Runs every test |
| `smith test <file>` | Runs the tests of that file |
| `smith test --name <text>` | Runs the tests whose name matches |
| `smith test --seed <n>` | Replays a run with that seed |
| `fn test_x() <e>` | A test with the effects `e`, which are the real ones |
| `with memory_fs(files)` in a test | The test handles `fs` itself, and does not list it |

Next: [A transformer](transformer.md)
