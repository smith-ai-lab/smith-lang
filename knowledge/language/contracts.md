---
type: specification
title: Contracts
description: requires and ensures on a function, which state what the caller owes and what the function gives, and are proven when the program is built.
status: partial
built:
  - "`requires` and `ensures` on a function, each a comparison of whole numbers joined with `and`, `or`, and `not` over its parameters and the result's name, which `arith` proves of the arguments at every call and of what the function returns at each place it returns, and takes as facts in the other place"
  - "what `arith` knows of a `var` of a whole number in a function, which a contract and the code around it rely on alike: at each use, the condition of each `if` around it and what it was last given, where no assignment to it lies between that and the use on any path, outside a loop as inside one"
  - "`decreases` on a function, the measure that proves a function that calls itself ends, which is proven as [Termination](termination.md) says and never computed"
  - "a function that returns `bool`, carries no effect, and is one expression of comparisons joined with `and`, `or`, and `not`, or a call of another such function, as a contract: `requires f(args)` and `ensures f(args)` are proven and give facts as that expression with the arguments put in does, and a call that carries an effect is refused"
  - "a `requires` that calls a function that returns `bool` and carries no effect but `alloc`, with arguments written out whole (numbers, texts, lists and tuples of them, `true`, `false`) in the call or in a parameter of the function that was, which the kernel works out when the program is built: a call whose `requires` works out to `true` is proven, one that works out to `false` is refused with the call as written, and the comparison that gave `false` with where it is written and the numbers it compared, and one that takes more than 1000000 steps is refused naming the call and the limit; a function that only a `requires` or an `ensures` names is checked"
  - "a `requires` on a function of a trait, a call of another function of the trait that gives a `bool` on its parameters, which each call proves and which holds in the body of each `impl` of it; and a `requires` of any function that names a trait's function that gives a `bool`, as `requires Add.valid(x, x)`, which a caller proves for the type it calls with, by what the `impl` that type picks says when its `valid` is one expression of comparisons of whole numbers, and where the type is a type parameter of the caller, by a `requires` of the same call; an `ensures` of a trait's function is not built"
  - "a comment on the line above a `requires`, which is the message of a call that fails it, with the goal that failed on a `goal:` line under it, for a `requires` of a function, of a function of a trait at a call through any `impl`, and of one that calls a function the kernel works out; a `requires` with none is refused as it was"
  - "not an argument that is a name, a call, or an operation, for the kernel to work out, and not a float"
---

# Contracts

A signature can state what must hold. `requires` is owed by the caller, and `ensures` is owed by the function. Both are checked when the program is built, with [`arith`](arith.md).

```
fn gap(a: Byte, b: Byte) -> Byte
  requires a >= b
{
  a - b
}
```

`gap` takes two numbers and says that the first is not below the second. `gap(5, 3)` is `2`. The `requires` is also what proves `a - b` fits, since inside `gap` it is a fact.

The compiler only checks a contract. It never searches for a proof, and nothing is ever tested when the program runs in a proof's place. Most proofs are never written out: `arith` builds them, and where it cannot, the author supplies the proof, as [What arith proves](arith.md) says.

## What the caller owes

Every call proves the `requires` of the function it calls, of the arguments it passes. A call that cannot is refused where it is called.

```
use std.console (console, print)

fn main() <console, alloc> { print("{gap(3, 5)}") }
```

This is refused: "cannot prove that `3 >= 5`, which `gap` requires of its arguments", with the fix "check the arguments first with an `if`, or state it in a `requires`".

The caller proves it from what it knows. A condition it has checked is enough.

```
fn gap_or_zero(a: Byte, b: Byte) -> Byte {
  if a >= b { gap(a, b) } else { 0 }
}
```

`gap_or_zero(3, 5)` is `0`. Or it states the same `requires`, and passes the duty on to its own caller.

```
fn gap_again(a: Byte, b: Byte) -> Byte
  requires a >= b and a <= 100
{
  gap(a, b)
}
```

`gap_again(9, 3)` is `6`. A `requires` may join comparisons with `and`, `or`, and `not`.

## What a contract may compare

A `requires` or an `ensures` compares two values of any type that has [`Ord`](comparisons.md), as [Statements](statements.md#what-a-statement-may-compare) says, so `requires earlier <= later` is written for two `Instant`s as it is for two numbers.

## What the caller is told

A comment above a `requires` is the message a caller sees when a call fails it. The goal that failed is shown under the message. A `requires` with no comment shows only the goal.

```
fn gap(a: Byte, b: Byte) -> Byte
  // The first number must not be below the second.
  requires a >= b
  requires a <= 200
{
  a - b
}
```

`gap(3, 5)` is refused, and the caller sees this.

```
main.smith:11:38: The first number must not be below the second.
  goal: `3 >= 5`
  11 | fn main() <console, alloc> { print("{gap(3, 5)}") }
     |                                      ^
  fix: check the arguments first with an `if`, or state it in a `requires`
```

`gap(250, 3)` is refused as a `requires` with no comment is, with "cannot prove that `250 <= 200`, which `gap` requires of its arguments" and no `goal:` line. The comment is on its own line directly above the `requires`, as [Functions](functions.md#comments) says, and it is for the one `requires` that follows it: the next `requires` has none of its own, and a blank line between a comment and its `requires` leaves that one with none. A comment of several lines is shown as several lines. A comment above a `requires` of a function of a [trait](traits.md) is shown at a call through any `impl`, and one above a `requires` that calls a function the compiler works out is shown with what it found, as the `goal:`.

## A requires is a proof parameter

`requires e` is a proof parameter of the statement `e` with the default `arith` that no call writes, so these two differ only in whether a call may write it.

```
fn gap(a: Byte, b: Byte) -> Byte
  requires a >= b
{
  a - b
}

fn gap_named(a: Byte, b: Byte, p: a >= b = arith) -> Byte { a - b }
```

`gap(5, 3)` and `gap_named(5, 3)` are both `2`, and `gap(3, 5)` and `gap_named(3, 5)` are both refused with the goal `3 >= 5`. The two rules are these.

- A `requires` is never written at a call. It is not in the argument list, and `arith` proves it from what is in scope, proofs bound by a [`check`](check.md) or given by a lemma included.
- A proof parameter is in the argument list. With the default `arith` it may be left out, or passed by position: `gap_named(a, b, q)` gives the proof `q` that `a >= b`, as [Statements](statements.md#a-proof-parameter-may-have-a-default) says.

## What the function gives

`ensures` states something about the result. The signature gives the result a name, and the `ensures` is about that name.

```
fn clamp(x: Byte, lo: Byte, hi: Byte) -> r: Byte
  requires lo <= hi
  ensures lo <= r and r <= hi
{
  if x < lo { lo } else if x > hi { hi } else { x }
}
```

`clamp(9, 2, 5)` is `5`. The function proves its `ensures` at each place it returns. One that does not is refused.

```
fn clamp(x: Byte, lo: Byte, hi: Byte) -> r: Byte
  requires lo <= hi
  ensures lo <= r and r <= hi
{
  x
}
```

This is refused: "cannot prove that `lo <= r and r <= hi`, which `clamp` ensures, where `r` is `x`", with the fix "state what is needed in a `requires`, or check it first with an `if`".

A signature that has an `ensures` names its result.

A [`mut`](mut-parameters.md) parameter is named by its value when the function returns, and `old(x)` names the value it was given, so an `ensures` says what the function did to it.

```
struct Cursor {
  at: Nat
}

fn step(c: mut Cursor)
  requires c.at < 1000000
  ensures c.at == old(c).at + 1
{
  c.at = c.at + 1
}
```

The caller knows `c.at` is one more than it was, and nothing else about `c`. `old` is written only in an `ensures`, and only of a `mut` parameter: `old(x)` of any other name is refused, as it is the same before and after.

```
fn bigger(a: Byte) -> Byte
  ensures a + 1 <= 255
{
  a
}
```

This is refused: "`bigger` has an `ensures`, and its signature does not name its result".

## What the caller then knows

The caller takes the `ensures` as a fact about what the call gave. So a result that is known to be within bounds can be added to.

```
fn room(x: Byte) -> Byte {
  c = clamp(x, 0, 100)
  c + 100
}
```

`room(250)` is `200`. `clamp(x, 0, 100)` gave a `c` that is at most 100, so `c + 100` fits. With `clamp(x, 0, 200)` it is refused, as `c + 100` may be above 255.

## A requirement that is worked out

A predicate may be longer than `arith` reads, with a `loop` or a `match`. When every argument of the call is written out, the compiler works the predicate out when the program is built, one step at a time, and the call is proven if it gives `true`.

```
use std.bits (i32)
use std.console (console, print)

fn has(xs: List[i32], k: i32) -> bool {
  var found = false
  loop v in xs {
    if v == k {
      found = true
      break
    }
  }
  found
}

fn pick(xs: List[i32], k: i32) -> i32
  requires has(xs, k)
{
  k
}

fn main() <console, alloc> { print("{pick([1, 2, 3], 2)}") }
```

`pick([1, 2, 3], 2)` is `2`. With `5` in place of `2` it is refused: "`has([1, 2, 3], 5)` is false: its last comparison, `3 == 5` at 7:8, was false". The message says where the answer came from: the comparison that gave `false` and the numbers it compared. A predicate that does not end within 1000000 steps is refused with the call and the limit named, and is never taken as proven. An argument that is not written out leaves the call to a `check`, as any `requires` that is not proven.

## A function used as a value

A function type states no contract, so a function with a `requires` is only called by its name, and using its name as a value is an error.

```
use std.console (console, print)

fn apply(f: fn(Byte, Byte) -> Byte) -> Byte { f(5, 3) }

fn main() <console, alloc> { print("{apply(gap)}") }
```

This is refused: "`gap` has a `requires`, which a function type does not state, so it is only called by its name".

A function with only an `ensures` can be a value, and what it gives is then not known to the caller.

```
fn small(x: Byte) -> r: Byte
  ensures r < 10
{
  if x < 10 { x } else { 9 }
}

fn apply(f: fn(Byte) -> Byte) -> Byte { f(5) }
```

`apply(small)` is `5`. Inside `apply`, `f(5)` is any `u8`, since `f` is a function type and states no `ensures`.

## The forms

| Written | Who owes it | About |
| --- | --- | --- |
| `requires p` | The caller, at each call | The arguments |
| `ensures p` | The function, where it returns | The result, by the name the signature gives it |
| `fn f(a: T) -> r: U` | | Names the result `r`, for an `ensures` |

Next: [Termination](termination.md)
