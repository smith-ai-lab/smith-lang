---
type: specification
title: Changing a value in place
description: A mut parameter, which a function changes for its caller, means taking the value and giving the new one back, and is marked mut at the call.
status: partial
built:
  - "`mut` before a parameter's type, lowered to taking the value and giving it back, so a call that changes a value is built to the code of the call that takes it and assigns what it gives back"
  - "a function with a `mut` parameter that gives a result, lowered to giving the values back before the result, and its call on a line of its own or as the whole right-hand side of a binding or an assignment, with a call anywhere else refused with where it may stand, and a result that nothing uses refused as any such value is"
  - "`f(mut x)` on a line of its own, of a `mut`, a field of one, or an item of a list held by one, and of several places that do not overlap"
  - "a call that leaves out `mut` where the parameter is `mut` refused with the call to write, `mut` where the parameter is not refused, and what is not a `mut`, a field, or an item refused"
  - "one place passed twice, or a place and a part of it, refused"
  - "a parameter marked `mut`, or a part of it without `Copy`, refused where it is stored, bound to another name, given to a `take` parameter, or returned, and any lambda that reads or assigns it refused"
  - "what `arith` knew of a value before a call that changes it not known after, but what the function [ensures](contracts.md#what-the-function-gives) of it"
  - "`mut` before a parameter of a trait's function, of an `impl`'s, of an operation of an effect, and of a function type that is the type of a parameter, each lowered as a function's is, so a program is built to the code of the one written with `take` and the value added to the result, and a call of each, a trait's function called in a function bound by the trait among them, marks `mut` and keeps what it gives as a function's call does"
  - "an `impl` that marks `mut` a parameter its trait's function does not, or does not mark one it does, refused at the parameter, naming the trait"
  - "a lambda that marks a parameter `mut` with its type, passed where a function type or an operation marks it, which changes it as a function does, and a lambda that marks a parameter the function type or the operation does not, or does not mark one it does, refused at the parameter; a lambda that changes a parameter and gives a result with its result's type left out, one bound to a name, and `mut` in a function type that is not the type of a parameter are not built"
---

# Changing a value in place

A parameter marked `mut` is a value the function changes for its caller. The call is marked `mut` too, so a reader sees which names a line changes.

```
struct Cursor {
  at: Nat
  line: Nat
}

fn step(c: mut Cursor) {
  c.at = c.at + 1
}

fn main() {
  mut c = Cursor(at: 0, line: 1)
  step(mut c)
  step(mut c)
}
```

After the two calls, `c.at` is `2`.

## What it means

`mut` means exactly what taking the value and giving it back means. These two are the same function, called the same way, and they are built to the same code.

```
fn step(c: mut Cursor) {
  c.at = c.at + 1
}

fn step(c: take Cursor) -> Cursor {
  mut next = c
  next.at = next.at + 1
  next
}
```

The first is called `step(mut c)`, and the second `c = step(take c)`. Every value has one owner, so no other name can see `c` while `step` runs, and the change is made where `c` is, with nothing copied. A fact about `c` that the caller knew before the call is not known after it, as for any name that is assigned, except what `step` [ensures](contracts.md#what-the-function-gives) of it.

## Where the change is seen

- **The call is marked.** `step(c)` is refused when the parameter is `mut`, with the fix "write `step(mut c)`", and `mut c` is refused where the parameter is not `mut`. A line that changes a name says so.
- **What is passed can be assigned.** It is a `mut`, or a field or an item of one, as `step(mut cs[0])` or `step(mut doc.cursor)`. `step(mut Cursor(at: 0, line: 1))` is refused, as nothing would see the change.
- **One place is passed once.** `swap(mut a, mut a)` is refused, and so is `swap(mut doc, mut doc.cursor)`, as both name the cursor. Two places that are not one are passed together.
- **The call stands alone.** A call that passes `mut` is a line of its own, or, when its function gives a result, the whole right-hand side of a binding or an assignment, as [Giving a result](#giving-a-result) shows. Anywhere else, as an argument of another call, an operand, or a condition, it is refused with where it may stand.
- **The parameter is not kept.** A function cannot store a `mut` parameter, return it, or give it to a lambda that outlives the call. It changes the value and gives it back when it returns, and nothing else holds it.

## Giving a result

A function that changes a `mut` parameter can also give a result, as a lexer's `next` changes its cursor and gives the token it read.

```
use std.console (console, print)
use std.bits (u8)

struct Cursor {
  at: u8
}

fn next(c: mut Cursor) -> u8 {
  was = c.at
  if c.at < 9 { c.at = c.at + 1 }
  was
}

fn main() <console, alloc> {
  mut c = Cursor(at: 0)
  _ = next(mut c)
  t = next(mut c)
  print("{c.at} {t}")
}
```

This prints `2 1`. Such a function means taking the value and giving it back before what the body gives: `fn next(c: mut Cursor) -> u8` is `fn next(c: take Cursor) -> (Cursor, u8)`, with each `return v` giving `(c, v)`, and `t = next(mut c)` is `(c, t) = next(c)`. With several `mut` parameters, they are given back in their order before the result. They are built to the same code.

What the function gives is used as any value is. It is bound or assigned by the whole line, as `t = next(mut c)`, `mut t = next(mut c)`, or `d.last = next(mut c)`, and `show(next(mut c))` is refused. `next(mut c)` on a line of its own is refused, as a `u8` that nothing uses is, and `_ = next(mut c)` throws it away. A function that gives nothing is called on a line of its own, and binding or assigning its call is refused.

## In a trait, a function type, and an effect

A trait's function, a function type, and an operation of an [effect](effect-declarations.md) mark a parameter `mut` as a function does, and it means what it means for a function. `fn step(t: mut T)` in a trait is `fn step(t: take T) -> T`, `fn(mut Count)` is `fn(take Count) -> Count`, and with a result `fn(mut Count) -> u8` is `fn(take Count) -> (Count, u8)`. They are built to the same code.

```
use std.console (console, print)
use std.bits (u8)

struct Count {
  n: u8
}

trait Step[T] {
  fn step(t: mut T)
}

impl Step[Count] {
  fn step(t: mut Count) {
    if t.n < 9 { t.n = t.n + 1 }
  }
}

fn twice[T: Step](t: mut T) {
  step(mut t)
  step(mut t)
}

fn main() <console, alloc> {
  mut c = Count(n: 0)
  twice(mut c)
  print("{c.n}")
}
```

This prints `2`. A trait's function is called with `mut` as any function is, in a function that the trait bounds as anywhere else.

- **An `impl` marks what its trait marks.** `fn step(t: Count)` in the `impl` above is refused: "`step` changes `t` for its caller in `Step`, so an implementation marks it `mut`", with the fix "write `t: mut Count`". A parameter that the trait's function does not mark, marked `mut` in an `impl`, is refused too: "`t` is marked `mut`, where `step` does not change it in `Step`".
- **A lambda marks what its function type marks.** A lambda writes `mut` before the type of a parameter it changes, as a function does: `fn(t: mut Count) { t.n = 0 }`. Passed where a function type marks the parameter `mut`, a lambda that does not mark it is refused: "the function this lambda is passed as changes `t` for its caller, so the lambda marks it `mut`", and one that marks a parameter the function type does not mark is refused too.
- **A handler's lambda marks what its operation marks.** An operation `fn bump(c: mut Count)` is called as `bump(mut c)`, and the lambda a handler gives for it is written `fn(c: mut Count) { ... }`, held to the operation as a lambda is to a function type.

```
fn apply(f: fn(mut Count), c: mut Count) {
  f(mut c)
  f(mut c)
}
```

`apply(fn(t: mut Count) { if t.n < 9 { t.n = t.n + 1 } }, mut c)` adds 2 to `c.n`, as `twice(mut c)` does.

## The forms

| Written | Means |
| --- | --- |
| `fn f(x: mut T)` | `f` changes `x` for its caller |
| `f(mut x)` | A call that changes `x` |
| `f(mut xs[i])`, `f(mut s.field)` | A call that changes an item or a field |
| `fn f(x: mut T)` and `fn f(x: take T) -> T` | The same function |
| `t = f(mut x)`, `s.field = f(mut x)` | A call that changes `x` and keeps what `f` gives |
| `fn f(x: mut T) -> R` and `fn f(x: take T) -> (T, R)` | The same function |
| `fn(mut T)` and `fn(take T) -> T` | The same function type |
| `fn(x: mut T) { e }` | A lambda that changes `x` for its caller |

Next: [Values used exactly once](resources.md)
