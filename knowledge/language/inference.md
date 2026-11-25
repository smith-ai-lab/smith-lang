---
type: specification
title: Inference
description: What is written at the top level is the contract and what is inside a body is solved from everything the body says, how a range is the smallest that holds every value a name is given, and the errors that say what was inferred.
status: partial
built:
  - "a `var` with no stated type given the smallest range that holds the value it starts as and every value it is given, solved in the order the body is read, so that `n = n + 1` twice is a `0..3`, and what `arith` knows of the value last given still proves what it proved"
  - "an error that names the range inferred, each value it was inferred from with its line, the assignment that set its top, and the use that needs less"
  - "a `var` that starts as a number written out and is given itself plus a value, or itself less a value, in a loop or after a branch, given the range its linear templates over the count of passes bound, where the passes are those of an array, a range, or a `List`, a `str`, or a `Slice` of a size below 2 to the 48, and refused, with the range, why, and the choices, where that range is not finite within 64 bits"
  - "a `var` that starts as a number written out and is given a value that depends on it in any other way, in a loop or after a branch, refused with the range of the first use as the range to state, and a `var` that starts as a value that has a type, and is given itself, keeping the range of that value"
  - "an `invariant` of a `loop`, which states a fact that `arith` proves before the first pass and at the end of each pass, as [`loop`](loop.md#an-invariant) says"
  - "a list written `[]` and `.none` given to a name, whose element is the smallest range that holds what is pushed or given to it and the type a use of it needs, and refused with where to state the type where nothing settles it"
  - "a block with a `try` that the call gives no type, refused with the type the block gives"
  - "the parameters of a lambda bound to a name that leaves their types out, each the smallest range that holds every argument of the calls of it in the same body, joined before the lambda is checked once with it, with its result what its body gives, and an error that names the range, the calls it was inferred from, and the use that needs less"
  - "a lambda bound to a name that leaves a type out, with no call of it in the same body to give the types, or called with what its own calls give so that the range grows with each look, refused with where to write them"
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

## A name a loop changes by itself

A `var` that starts as a number written out, and that a [`loop`](loop.md) gives itself plus a value, or itself less a value, has a range found with linear templates over the count of passes.

```
fn sum_of(xs: [3]Byte) -> 0..766 {
  var acc = 0
  loop x in xs { acc = acc + x }
  acc
}
```

`sum_of([200, 100, 255])` is `555`. One pass adds a `0..256`, so at most `255`, and there are three passes, so `acc` is a `0..766`.

For each such `var` the solver tries one fixed set of templates and searches for nothing else. Where the `var` starts as a `start_lo..start_hi`, and one pass of a loop changes it by a `lo..hi`, then after `n` passes it is within

```
start_lo + n * lo <= x
x <= start_hi + n * hi
```

and each holds before the first pass and after each pass. The change of one pass is worked out from the body, in the order it is read: an assignment changes the `var` by what it adds or takes, which may be nothing where it is in an `if`; a loop in the pass changes it by its range times its passes; and one thing and then another change it by the sum of both. So a pass that adds `30` in an inner loop and then, in an `if`, takes `1`, changes the `var` by `29..30`, and the `var` is at its least when the `1` is taken after each `30`. The range of the `var` is what holds every value it has after any part of any pass, so what a pass takes away is charged against what the same pass added before it, and not against the start. A value given to the `var` that does not read it, as `acc = 0` is, is another start, and then each assignment counts at its own `lo..hi` for as many times as it runs. A `break` ends the loop after a part of a pass, which the range holds.

The number of passes is known when the program is built, and is one of these:

- The size of an array, as `[3]Byte` is.
- The greatest end of a range less its least start, so `loop i in 0..n` with `n` a `0..11` has at most `10`.
- For a `List`, a `str`, or a `Slice`, the bound on its size, which is below 2 to the 48, as each element has at least a byte and no address of x86-64 or arm64 has more than 48 bits. A `requires` that bounds the size gives a smaller bound. A count and a sum of bytes are then proven.

```
fn evens(xs: List[Nat]) -> 0..281474976710656 {
  var n = 0
  loop x in xs { if x % 2 == 0 { n = n + 1 } }
  n
}
```

This is accepted: `n` is a `0..281474976710656`, which is `0..2^48`, as `xs` has fewer than 2 to the 48 elements and one pass adds at most `1`. A `var` that counts down is solved the same way, from its start by what a pass takes away.

```
fn down(d: 1..4) -> 70..101 {
  var n = 100
  loop i in 0..10 { n = n - d }
  n
}
```

`n` is a `70..101`: at most `10` passes take at most `3` each.

```
fn netted(c: bool) -> 0..301 {
  var s = 0
  loop i in 0..10 {
    loop j in 0..10 { s = s + 3 }
    if c { s = s - 1 }
  }
  s
}
```

`netted(true)` is `290`. Each pass of the outer loop changes `s` by `29..30`, so `s` is a `0..301`.

A `var` whose range is not finite within 64 bits is refused, with the range, why, and the choices.

```
fn total(xs: List[Nat]) -> Nat {
  var acc = 0
  loop x in xs { acc = acc + x }
  acc
}
```

This is refused: "`acc` is changed by itself in this loop, and its range is not finite within 64 bits: `acc = acc + x` adds a `Nat`, at most 281474976710655 times", with the fix "write `checked_add`, which gives an `Option` and costs one comparison, where it is seen; or `wrapping_add`; or a `requires` that bounds the size". The compiler adds no check that the program did not write. A loop over a type of a program's own that implements [`Iterate`](iterate.md), or an open-ended loop, has no count of passes, and a `var` it adds to is refused the same way.

A change the templates do not cover, as a product or a value that depends on the `var` in any other way, is refused: "`n` is changed by itself in this loop, so its range is not inferred", with the range of the first use as the range to state.

```
fn doubled(n: 0..11) -> 0..2049 {
  var acc = 1
  loop i in 0..n { acc = acc * 2 }
  acc
}
```

A `var` that starts as a value that has a type keeps that range when it is given itself, and each assignment is proven to fit it.

```
fn steps(from: 0..100) -> 0..100 {
  var i = from
  loop j in 0..3 { if i < 99 { i = i + 1 } }
  i
}
```

`steps(5)` is `8`. `i` is a `0..100`, the range of `from`, and the `if` shows that `i + 1` fits it.

## When to write an invariant

The templates bound one name by the passes of a loop. They do not find how one name stands to another, or a bound that each pass keeps and that the passes do not add up to. A program, most often one written by a model, states that once, as an [`invariant`](loop.md#an-invariant), and nothing runs for it.

A name doubled on each pass has no template, so its range is stated, and a second name doubled with it is known to be the same number only if the loop says so.

```
fn gap(n: Nat) -> Nat {
  var a: 0..65 = 1
  var b: 0..65 = 1
  loop i in 0..n invariant a == b {
    if a < 33 {
      a = a + a
      b = b + b
    }
  }
  a - b
}
```

`gap(9)` is `0`. `a == b` holds when the loop starts, and a pass that doubles one doubles the other, so it holds at the end of the pass. Inside the `if`, `a < 33` and `a == b` show that `b + b` fits a `0..65`, and after the loop `a == b` shows that `a - b` fits a `Nat`. Without the invariant `b + b` and `a - b` are refused. Write one when a use of a name needs a fact about it that its range does not give, and the fact is a comparison of whole numbers that `arith` can prove from the start and from one pass. An invariant that cannot be proven is refused where it is stated, so it is never a promise the compiler takes on trust.

## A lambda's parameters

A [lambda](lambdas.md) written in a function has the type parameters of the function. A lambda that is passed where a function type decides its parameters takes them from it. A lambda that is bound to a name and called in the same body has its parameters solved from those calls and from its uses: each argument is a lower bound of the range of the parameter, and each use of the parameter inside the lambda is an upper bound. The parameter is the smallest range that holds every argument, joined before the lambda is checked, and the lambda is checked once with that range and is lowered as one function, as a `var` is solved from every value it is given.

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

This is refused: "`v` is a `300..301`, inferred from the call `go(300)` at line 5, and `takes` at line 4 needs a `0..256`". A lambda that is returned, stored, or passed where no function type decides its parameters, and whose parameters no call in the body gives, writes them, as `fn(v: Byte) { takes(v) }`, and is refused with where to write them. So is one whose calls pass what other calls of it give, as `y = inc(a)` then `inc(y)` for `inc = fn(v) { v + 1 }`, since its parameter would be wider with each look.

## A list or an option that a later use settles

A value that says nothing of its element type takes it from what the body does with it. `[]` and `.none` are settled by a later use in the same body, and the element is the smallest range that holds every value put in.

```
use std.list (push)

fn one() -> List[1..2] <alloc> {
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

This is refused: "an array of nothing has no element to say its type, so it is written where a type is expected", with the fix "write its type, as in `xs: List[Nat] = []`".

A use settles it too: `xs = []` that is passed to a function whose parameter is a `List[0..256]` is that list. An option settles the same way, so `var o = .none` with `o = .some(7)` in one branch and `o` as the result of a function that gives an `Option[Nat]` is an `Option[Nat]`. A use gives the type where it holds everything that is put in, and where it does not, the elements are what is put in and the use is refused.

```
use std.list (push)

fn small(xs: List[0..3]) -> Nat { xs.size }

fn main() <alloc> {
  var xs = []
  xs = push(xs, 7)
  n = small(xs)
}
```

This is refused: "`xs` is a `List[7..8]`, inferred from `push(xs, 7)` at line 7; the use at line 8 needs a `List[0..3]`". A list that is only pushed to for another name is settled the same way, as `ys = push(xs, 4)` settles `xs`.

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
| `acc = acc + x` in a loop, `acc` starting as a number written out | The start, and what the passes add or take away, by a template |
| A `var` a loop changes in a way no template covers | Refused, with the range to state |
| `loop i in a..b invariant e { ... }` | `e` is proven before the first pass and at the end of each pass, and is known in the block and after the loop |
| `double = fn(v) { v * 2 }` and `double(a)` | `v` has the range of what it is called with |
| `xs = []` and a later `push` | The element is the range of what is pushed |
| A value that nothing settles | Refused, with where to state its type |

Next: [`if`](if.md)
