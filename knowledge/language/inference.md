---
type: specification
title: Inference
description: What is written at the top level is the contract and what is inside a body is solved from everything the body says, how a range is the smallest that holds every value a name is given, and the errors that say what was inferred.
status: partial
built:
  - "a `var` with no stated type given the smallest range that holds the value it starts as and every value it is given, solved in the order the body is read, so that `n = n + 1` twice is a `0..3`, and what `arith` knows of the value last given still proves what it proved"
  - "an error that names the range inferred, each value it was inferred from with its line, the assignment that set its top, and the use that needs less"
  - "a `var` that starts as a number written out and is given a value that depends on it where the range is not known, in a loop or after a branch, refused with the range of the first use as the range to state, and a `var` that starts as a value that has a type, and is given itself, keeping the range of that value"
  - "not a lambda's parameters inferred from the calls of a lambda bound to a name"
  - "not `[]` and `.none` settled by a later use in the same body"
  - "not a block with a `try` that says the type it gives"
---

# Inference

What is written at the top level is the contract, and what is inside a body is solved. The parameters, the result, and the effects of a function, a field, an operation of an effect, and a top-level value state their types. Inside a body, a binding, a `var`, a lambda and its parameters, a literal, and a type argument are not written, and each takes the type that everything the body says about it needs.

```
fn grow(start: 0..5) -> 0..11 {
  var x = start
  x = 9
  x
}
```

`grow(2)` is `9`. `x` is given `start`, a `0..5`, and `9`, so its range is the smallest that holds both, `0..10`, which the result holds.

## A whole number takes the smallest range that holds every value it is given

Each value given to a name is a lower bound on its range: the range holds it. Each use is an upper bound: an argument of a parameter, a result, a field, or an operator that needs the range to be valid. The range of a name is the smallest that meets every lower bound, and a lower bound that passes an upper bound is an error.

```
fn pick(c: bool) -> 0..10 {
  var x = 0
  if c { x = 5 } else { x = 9 }
  x
}
```

`x` is given `0`, `5`, and `9`, so it is a `0..10`, which the result holds. `var x = 0` does not make `x` a `0..1`.

A binding that is not a `var` is given one value, so it has the range of that value, as [Bindings](bindings.md) says.

## An error says what was inferred

Every error that involves an inferred range says that it was inferred, from what, and where.

```
fn takes(level: 0..6) -> 0..6 { level }

fn pick(c: bool) -> 0..6 {
  var x = 0
  if c { x = 5 } else { x = 9 }
  takes(x)
}
```

This is refused: "`x` is a `0..10`, inferred from `0` at line 4, `x = 5` at line 5, and `x = 9` at line 5, and its top is set by `x = 9` at line 5; the use at line 6 needs a `0..6`". The error names the range inferred, the assignment that set its top, and the use that needs less. The fix is to give `x` nothing above 5, or to check it first, as [Integer arithmetic](arithmetic.md#checking-first) shows.

## A var given a value one after another

Code with no loop is solved in order, as it is read. Each assignment is a new step, whose range is known from the steps before it, and the range of the `var` is the smallest that holds every step.

```
fn count() -> 0..3 {
  var n = 0
  n = n + 1
  n = n + 1
  n
}
```

`count()` is `2`. The steps are `0`, `0 + 1`, and `1 + 1`, so `n` is a `0..3`. What is known of the value a `var` was last given still proves what it proved, as [`arith`](arith.md) says, so a `var` that is last given a value that fits a narrower range enters it.

```
fn takes(level: 0..6) -> 0..6 { level }

fn last_given() -> 0..6 {
  var x = 0
  x = 9
  x = 5
  takes(x)
}
```

`last_given()` is `5`. `x` is a `0..10`, and it was last given `5`, which a `0..6` holds. After an `if`, a `match`, or a loop, which of the values it was last given is not known, so the range of the `var` is what enters.

## A value that depends on itself in a loop

A `var` that a [`loop`](loop.md) gives a value that depends on the `var` itself has no range that this page infers yet. Where it starts as a number written out, it is refused, with the range to state.

```
fn sum_to(n: 0..11) -> 0..101 {
  var acc = 0
  loop i in 0..n { acc = acc + i }
  acc
}
```

This is refused: "`acc` is changed by itself in this loop, so its range is not inferred yet", with the fix "state the range, as `var acc: 0..101 = 0`", the range of the first use that needs less. A value that does not depend on the `var` is no such case, so `loop i in 0..n { last = i }` gives `last` the range of `i`.

A `var` that starts as a value that has a type keeps that range when it is given itself, and each assignment is proven to fit it.

```
fn steps(from: 0..100) -> 0..100 {
  var i = from
  loop j in 0..3 { if i < 99 { i = i + 1 } }
  i
}
```

`steps(5)` is `8`. `i` is a `0..100`, the range of `from`, and the `if` shows that `i + 1` fits it.

## A lambda's parameters

A [lambda](lambdas.md) written in a function has the type parameters of the function. A lambda that is passed where a function type decides its parameters takes them from it. A lambda that is bound to a name and called in the same body has its parameters solved from those calls and from its uses: each argument is a lower bound of the range of the parameter, and each use of the parameter inside the lambda is an upper bound.

```
fn doubled(a: 0..5) -> 0..9 {
  double = fn(v) { v * 2 }
  double(a)
}
```

`doubled(4)` is `8`. `v` is a `0..5`, the range of the one argument, so `v * 2` is a `0..9`.

```
fn takes(b: Byte) -> Byte { b }

fn pass() -> Byte {
  go = fn(v) { takes(v) }
  go(300)
}
```

This is refused: "`v` is a `300..301`, inferred from the call `go(300)` at line 5, and `takes` at line 4 needs a `0..256`". A lambda that is returned, stored, or passed where no function type decides its parameters writes them, as `fn(v: Byte) { takes(v) }`.

## A list or an option that a later use settles

A value that says nothing of its element type takes it from what the body does with it. `[]` and `.none` are settled by a later use in the same body, and the element is the smallest range that holds every value put in.

```
fn one() -> List[1..2] {
  var xs = []
  xs = push(xs, 1)
  xs
}
```

`one()` is `[1]`. The element type of `xs` is the range of the `1` that is pushed, as it is of any name, and a later `push` of `5` would make it `1..6`. A list written out is a `List[Nat]` where one is expected, so `total([1, 2, 3])` compiles for a `total` that takes a `List[Nat]`, as [Literals](literals.md) says of an array.

Where nothing in the body settles the type, it is refused, with where to state it.

```
fn none() {
  xs = []
}
```

This is refused: "the type of the elements of `xs` is not settled by anything in `none`: state it, as `xs: List[Nat] = []`".

## A block with a try

The type of a block is what it gives. A `try` in a block returns a failure from the block, so the block must give a type that [`try`](try.md) can return a failure from.

```
use std.bits (u64)

fn around[T](body: fn() -> T) -> T { body() }

fn step(n: u64) -> Result[u64, str] { .ok(n) }

fn rounds(i: u64) -> Result[u64, str] {
  r = around {
    x = try step(i)
    x
  }
  .ok(0)
}
```

This is refused at the `try`: "this block gives `u64`, which `try` cannot return a failure from". The error names the type the block gives.

## The forms

| Written | Is |
| --- | --- |
| `var x = 0`, then `x = 5` and `x = 9` | A `0..10`, the smallest range that holds each value given |
| `n = n + 1`, twice, with no loop | Solved in order: `0`, `1`, and `2` give a `0..3` |
| A `var` that a loop gives a value that depends on it | Refused, with the range to state |
| `double = fn(v) { v * 2 }` and `double(a)` | `v` has the range of what it is called with |
| `xs = []` and a later `push` | The element is the range of what is pushed |
| A value that nothing settles | Refused, with where to state its type |

Next: [`if`](if.md)
