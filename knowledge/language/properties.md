---
type: specification
title: Properties
description: A test that takes a Key, or typed parameters, runs many times on random inputs, is shrunk to the smallest failure, and is proved first when the checker can.
status: partial
built:
  - "a test that takes `key: take Key`, which `smith test` runs 100 times, each with a key of its own made from the seed of the run, until it fails, and says passed with the number of its runs"
  - "a failure shrunk by replaying with smaller choices, as [When it fails](#when-it-fails) says, and said with the seed, which `smith test --seed` gives the same failure with"
  - "not typed parameters, or a property proved before it is run"
---

# Properties

A test that takes a `Key` is a property. It runs many times, building its inputs with ordinary random functions.

```
use model.tokenizer (decode, encode)
use std.random (Key)
use std.test (Failed, expect)

fn test_round_trip(key: take Key) -> Result[(), Failed] {
  text = random_text(take key, max: 200)
  expect { decode(encode(text)) == text }
}
```

`test_round_trip` runs 100 times, with a new key each time, so `text` is a different text each time. The key of each run is made from the seed of the run of the tests, so a run with the same seed gives the same texts. A property that passes every time is said to have passed with the number of its runs.

## When it fails

Under `smith test` the key is backed by a recorded sequence of choices. A key has a place in the tree of the keys `split` makes: the key a property is given is at 0, and `split` of the key at `p` gives the keys at `2p + 1` and `2p + 2`. `bits` of a key gives the choice recorded for its place, or draws one and records it, so `bits` stays a function of its key alone. The choices of a run are those it drew, in the order it drew them.

A failure is shrunk by replaying the test with smaller choices, and every shrunk input is one the test's own code built. One sequence of choices is smaller than another when it has fewer choices, or as many and the first that differs is less, read as a `u64`. A replay is kept when it fails with smaller choices, and the shrinking ends when no replay it tries is kept. The failure says the smallest failing input and the seed.

```
use std.bits (u64)
use std.random (Key, bits)
use std.test (Failed, expect)

fn test_small(key: take Key) -> Result[(), Failed] {
  n: u64 = bits(take key) % 64
  expect { n < 10 }
}
```

This fails with `n` as `10`, the least choice that fails, whichever choice failed first.

If `decode(encode(text)) == text` fails for a text of 140 characters, the test is replayed with smaller choices until the text is as small as it can be while it still fails. Each text it tries was built by `random_text`, so each is a text the test could have been given.

## Typed parameters

Typed parameters are shorthand. Each is drawn from a split key with `random`, the one function of the [`Random`](random.md) trait.

```
use std.bits (i32)

fn test_order(a: i32, b: i32) -> Result[(), Failed] {
  expect { (a < b) == (b > a) }
}
```

This is the same as writing the keys out.

```
use std.bits (i32)

fn test_order(key: take Key) -> Result[(), Failed] {
  (ka, kb) = split(take key)
  a: i32 = random(ka)
  b: i32 = random(kb)
  expect { (a < b) == (b > a) }
}
```

A type that a program defines can be a parameter with nothing more written, since it has `Random`. A parameter whose type has no `Random` is refused.

```
fn test_point(p: Point) -> Result[(), Failed] {
  expect { p == p }
}
```

This is a test when `Point` has `Random`, and is refused after `without Random for Point`.

Typed parameters are written when each input is a random value of its type. A `Key` is written when an input must be built by a function that takes more than the key, as `random_text(take key, max: 200)` does.

## Proved before it is run

The checker tries to prove a property first and runs it only when it cannot.

```
fn test_not_not(b: bool) -> Result[(), Failed] {
  expect { not not b == b }
}
```

The checker proves this for every `b`, so the property is not run.

## The forms

| Written | Means |
| --- | --- |
| `fn test_x(key: take Key)` | A property: runs 100 times, with inputs built from `key` |
| `fn test_x(a: A, b: B)` | The same, with each parameter drawn from a split key with `random` |
| A failure | Shrunk by replaying with smaller choices |
| A property the checker proves | Not run |

Next: [Running tests](running-tests.md)
