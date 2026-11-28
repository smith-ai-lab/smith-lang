---
type: specification
title: Operators
description: An arithmetic operator as a call of a trait function, operands of different types, and derive, which writes an impl field by field.
status: partial
built:
  - "`a + b`, `a - b`, and `a * b` as calls of `Add.add`, `Sub.sub`, and `Mul.mul` where the two sides are not both whole numbers or both floats: an `impl` of the trait for the two types, whose `valid` is `true` or a comparison of the whole numbers it is given, makes the operator work on a type of a program's own, with the third type of the trait as its result, and no `impl` is one error that names the trait and both types"
  - "a type parameter that `Add`, `Sub`, or `Mul` bounds has the operator, `valid` is proven by a `requires` of the same call or at the call by the `impl` of the type, and for `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, and `u64` it is that the answer fits, which the `impl` of the prelude says with a function the compiler builds, so the operator is one instruction in a copy for a whole number type; a product of two sides that are not numbers known is not proven"
  - "`Div` and `Rem`, declared by the prelude as `Add` is, with an `impl` for each of `i8` to `i64` and `u8` to `u64` whose `valid` is that `a / b` or `a % b` of the type compiles: the divisor is not zero and, for a signed type, the least number is not divided by -1, which `%` asks only of `i64`; `Div.div(a, b)` and `Rem.rem(a, b)` are called by name, and `a / b` and `a % b` where a side is a type parameter bound by `Div` or `Rem` are calls of them, proven and made one instruction in a copy for a whole number type as `+` is, with a signed `%` of fewer than 64 bits taken to twice the size first"
  - "a whole number's `+`, `-`, `*`, `/`, and `%`, where both sides are whole numbers, as [Integer arithmetic](arithmetic.md) says, which is not a call; `/` and `%` of a type of a program's own, `^`, `-a`, floats, and `derive` are not built"
---

# Operators

An operator is a call of a trait function. `a + b` means `Add.add(a, b)`, so a type has `+` when it has an `impl` of `Add`. The arithmetic operators are `+`, `-`, `*`, `/`, `%` for what is left over from dividing, and `^` for a power.

## The trait behind each operator

Each arithmetic operator has one trait, and the trait has one function.

| Written | Means | Trait |
| --- | --- | --- |
| `a + b` | `add(a, b)` | `Add` |
| `a - b` | `sub(a, b)` | `Sub` |
| `a * b` | `mul(a, b)` | `Mul` |
| `a / b` | `div(a, b)` | `Div` |
| `a % b` | `rem(a, b)` | `Rem` |
| `a ^ b` | `pow(a, b)` | `Pow` |
| `-a` | `neg(a)` | `Neg` |

The six that take two operands have one shape: the types of the two operands, and the type of the result, which they fix. Each has two functions: `valid`, which says for which operands the operator has an answer, and the function that gives it, which requires `valid`.

```
trait Add[A, B = A, out C = A] {
  fn valid(a: A, b: B) -> bool
  fn add(a: A, b: B) -> C
    requires valid(a, b)
}
```

`Sub`, `Mul`, `Div`, `Rem`, and `Pow` are the same with their own function. `Neg` takes one operand:

```
trait Neg[A, out C = A] {
  fn valid(a: A) -> bool
  fn neg(a: A) -> C
    requires valid(a)
}
```

`-a` has a trait of its own and is not zero less `a`, as a type may be turned around without having a zero.

## An operator on a type of your own

`Rect` has `+` once `Add` is implemented for it. `Add[Rect]` means `Add[Rect, Rect, Rect]`, as [`out` parameters](out-parameters.md) says.

```
struct Rect {
  w: Int
  h: Int
}

impl Add[Rect] {
  fn valid(a: Rect, b: Rect) -> bool { Add.valid(a.w, b.w) and Add.valid(a.h, b.h) }
  fn add(a: Rect, b: Rect) -> Rect { Rect(w: a.w + b.w, h: a.h + b.h) }
}
```

Two `Rect`s are added when both of their sides are, so `valid` says that, and with it known the two `+` inside `add` are proven.

Now `unit + unit` is a `Rect`, and it means `add(unit, unit)`. The operator is the form that is written, and the call of `add` is accepted because `add` is an ordinary trait function.

```
fn twice[T: Add](x: T) -> T
  requires Add.valid(x, x)
{
  x + x
}

big: Rect = twice(unit)
```

A type with no `impl` of `Add` has no `+`. `x + x` in `twice` is allowed only because `T` is bound by `Add`, as [Bounds and `requires`](bounds.md) says.

## When an operator has an answer

An operator compiles only where its `valid` is proven, as `x[i]` compiles only where the `valid` of [`Index`](indexing.md#indexing-is-a-trait) is. `valid` is used to prove, and is never run for an operator.

- For a whole number type, `valid` of `Add`, `Sub`, and `Mul` is that the answer fits the type, which is the rule of [Integer arithmetic](arithmetic.md): nothing overflows silently, and nothing is checked at run time. `valid` of `Div` and `Rem` is that the second operand is not zero, and that the answer fits.
- For a float, `valid` is `true`, as a float has an answer for every pair.
- For a type of a program's own, it is what its `impl` says, as `Rect` above.

Inside a function with a type parameter nothing is known of `T`, so nothing proves that `x + x` has an answer. The function says so in a `requires`, as `twice` does, and each caller proves it for the type it calls with. `twice(200)` with a `u8` is refused where it is called, as `200 + 200` written there would be. Without the `requires`, `twice` itself is refused at its `+`.

## Operands of different types

The operands of an arithmetic operator may differ in type, as `Add` has a `B` that is not always `A`. So a number times an array, or a number times a struct of weights, is one more `impl` of `Mul`.

```
struct Weights {
  a: f32
  b: f32
}

impl Mul[f32, Weights, Weights] {
  fn valid(k: f32, w: Weights) -> bool { true }
  fn mul(k: f32, w: Weights) -> Weights { Weights(a: k * w.a, b: k * w.b) }
}
```

`2.0 * w` is a `Weights`. The type of the result is the `out` parameter, so it is fixed by the types of the operands.

## derive

`derive Add for Circle` writes the `impl` that adds each field to the same field of the other.

```
struct Circle {
  r: Int
}

derive Add for Circle
```

This is the `impl` written out field by field, and it is the short way to write it.

## The forms

| Written | Is |
| --- | --- |
| `a + b` | `Add.add(a, b)`, which compiles where `Add.valid(a, b)` is proven |
| `a - b`, `a * b`, `a / b`, `a % b`, `a ^ b` | `Sub.sub`, `Mul.mul`, `Div.div`, `Rem.rem`, `Pow.pow`, each shaped as `Add` is |
| `-a` | `Neg.neg(a)` |
| `impl Add[Rect] { ... }` | `+` for two `Rect`s |
| `impl Mul[f32, Weights, Weights] { ... }` | `*` for a number and a `Weights` |
| `derive Add for Circle` | An `Add` that works field by field |

Next: [Comparisons](comparisons.md)
