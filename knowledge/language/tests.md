---
type: specification
title: Tests
description: Tests found by convention, a function named test_ that returns a Result, the expect check, and a test file as an ordinary module.
status: planned
---

# Tests

Tests are found by convention. A file named `x_test.smith` holds tests for `x.smith`, and every function in it named `test_...` is a test.

```
use std.test (Failed, expect)

fn test_one_and_one() -> Result[(), Failed] {
  expect { 1 + 1 == 2 }
}
```

Written in `x_test.smith`, `test_one_and_one` is a test of `x.smith`. A test that takes nothing is a single example, and it runs once.

## A test returns a Result

A test returns a `Result`, and `expect` is the one check. `expect { ... }` is the check.

```
use model.tokenizer (encode)
use std.test (Failed, expect)

fn test_empty() -> Result[(), Failed] {
  expect { len(encode("")) == 0 }
}
```

The test file for `model/tokenizer.smith` is `model/tokenizer_test.smith`, and `encode` is a function of that module.

## What `expect` reports

`expect` receives its lambda as [code](code.md), so a failed comparison reports both of its sides. A test that fails on `len(encode("")) == 0` says what `len(encode(""))` was and what `0` was.

## Several checks

Several checks chain with [`try`](try.md). A check that fails ends the test, and the last check is the test's result.

```
use model.tokenizer (decode, encode)
use std.test (Failed, expect)

fn test_tokens() -> Result[(), Failed] {
  tokens = encode("hello")
  try expect { len(tokens) <= len("hello") }
  expect { decode(tokens) == "hello" }
}
```

## What a test carries

A test is an ordinary function. It lists its effects as any function does, and the system's handlers answer them, as they answer `main`. A test that fakes one effect installs a handler of it with [`with`](with.md), and every other effect stays real.

```
use std.http (get, listen, routes)
use std.test (Failed, expect)

fn test_a_request_is_answered() -> Result[(), Failed] <alloc> {
  with other_side()
  with listen(port: 80)
  with routes()
  get("/hello", hello)
  expect { true }
}
```

`other_side` is a handler of [`net`](../library/net.md#making-a-value-in-a-handler) that makes a `Connection` with `connection` and answers `read` with the bytes of a request, so the server is tested with no socket and no port. The test carries `alloc` and nothing else, and `alloc` is answered by the system as it is for `main`.

## A test file is an ordinary module

A test file is an ordinary module: it imports with `use`, as [Importing with use](use.md) describes, and it sees only public names. A test checks behaviour through the interface, while [contracts](contracts.md) and proofs check the internals.

```
use model.tokenizer (encode, split_words)
```

If `split_words` is private to `model.tokenizer`, this `use` is refused, as [Public and private](pub.md) describes.

Test files are left out of normal builds, so `smith build` never compiles `x_test.smith`.

## The forms

| Written | Means |
| --- | --- |
| `x_test.smith` | The tests of `x.smith` |
| `fn test_name() -> Result[(), Failed]` | A test: a single example that runs once |
| `expect { e }` | The check, which reports both sides of a comparison when it fails |
| `try expect { e }` | A check that ends the test when it fails |
| `fn test_name() -> Result[(), Failed] <e>` | A test that carries the effect `e`, which the system answers unless the test installs a handler of it |
| `fn test_name(key: take Key)` or `fn test_name(x: T)` | A property, as [Properties](properties.md) describes |

Next: [Random values for tests](random.md)
