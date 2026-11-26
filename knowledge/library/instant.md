---
type: specification
title: Instants and durations
description: std.instant, with Instant, a point in time, and Duration, a span of time, two types that hold milliseconds in a private field so that a point is never used as a span, or a number as either.
status: partial
built:
  - "`std.instant`: `Instant` and `Duration`, each with a private field of `0..4611686018427387904` milliseconds; `ms`, `seconds`, `instant`, `milliseconds`, and `since_1970`; `after`, `between`, and `until`; and the comparison of two of one type"
  - "`now`, `sleep`, `x509.Certificate.not_before` and `not_after`, `wait_for_time`, `wait_to_read_by`, `wait_to_write_by`, `test_ready_by`, and `time_left` use them"
  - "not built: the claims of `std.jwt`, which has no code yet; and the `Wait.time` and `limit` of `std.task`, and the sums of the name lookup of `std.net`, which are the milliseconds of the library's own scheduler and lookup and not times that a program holds"
---

# Instants and durations

The module `std.instant` has the two types that every time of the library is. An `Instant` is a point in time, as milliseconds since the start of 1970. A `Duration` is a span of time, as milliseconds. They are two types and not two numbers, so a point is never used as a span, and a number of seconds is never used as a number of milliseconds.

```
pub struct Instant {
  ms: 0..4611686018427387904
}

pub struct Duration {
  ms: 0..4611686018427387904
}
```

The field of each is private, as [Public and private](../language/pub.md) says: no other module reads it, assigns it, or builds the struct by calling its name. Each holds `0..2^62` milliseconds, which is about 146 million years, so that a point and a span added always fit a `u64` before they are checked, and which holds every time that a certificate or a clock can say.

| Function | Gives |
| --- | --- |
| `ms(n)` | The `Duration` of `n` milliseconds, for `n` in `0..2^62` |
| `seconds(n)` | The `Duration` of `n` seconds, for `n` in `0..4611686018427387` |
| `instant(n)` | The `Instant` that is `n` milliseconds after the start of 1970, for `n` in `0..2^62` |
| `milliseconds(d)` | The milliseconds of a `Duration`, for what writes one down or hands one to the system |
| `since_1970(i)` | The milliseconds from the start of 1970 to an `Instant`, for the same |
| `after(at, span)` | `.some` of the `Instant` a `Duration` after `at`, and `.none` when that is past `2^62` milliseconds |
| `between(earlier, later)` | `.some` of the `Duration` from `earlier` to `later`, and `.none` when `later` is before `earlier` |
| `until(deadline, now)` | The `Duration` from `now` to `deadline`, and `ms(0)` once the deadline has passed, which is what it is for: the time left to wait |

```
use std.instant (Duration, between, ms)
use std.time (now, sleep, time)

fn how_long() -> Option[Duration] <time, alloc> {
  start = now()
  sleep(ms(250))
  between(start, now())
}
```

A subtraction that has no answer is `.none` and not a zero, so a later point taken from an earlier one is a case the program handles and never a number that looks right. `until` is the one function that answers none for a deadline that has passed, as the time left to wait is none then.

## There are no operators

An `Instant` and a `Duration` have no `+`, `-`, `*`, or `/`: each is refused with "there is no implementation", `Instant + Instant` among them. `after` and `between` are what add and subtract, and give an `Option` because the answer may not be a time.

`==`, `<`, and the rest compare two `Instant`s or two `Duration`s. An `Instant` compared with a `Duration`, or with a number, is refused, as two types meet in no comparison.

## What reads and writes a time

An `Instant` and a `Duration` are what these take and give:

- `now()` gives an `Instant` and `sleep(d)` takes a `Duration`, as [Time](time.md) says.
- `not_before` and `not_after` of an x509 `Certificate` are `Instant`s, so `now() < c.not_after` is a comparison of two `Instant`s and is right.
- `wait_for_time`, `wait_to_read_by`, `wait_to_write_by`, and `test_ready_by` of [`std.task`](task.md) take a `Duration`, and `time_left(deadline, now)` takes two `Instant`s and gives a `Duration`.

A handler of `time` answers `now` with an `Instant` and takes a `Duration` in `sleep`, so a test builds a reading with `instant(n)` and reads a span with `milliseconds(d)`.
