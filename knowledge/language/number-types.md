---
type: specification
title: Number types
description: Every whole number is a range written a..b, the floats, the names Int, Nat, Pos, and Byte, and how two numbers meet.
status: partial
built:
  - "not `Pos`, which no file can write yet"
  - "`f32` and `f64`, as the type of a parameter, a result, a field, an item of a list, and a binding, `+`, `-`, `*`, and `/` of two floats of one type, a `-` before a float, `float(n)` of a whole number, which is the `f32` or `f64` its use decides and an `f64` when nothing does, the text of a float in a string and in `repr`, and an error where a whole number and a float meet in an operator, on every target; not `f16` or `bf16`, not an `f32` beside an `f64`, and not `%` of floats"
  - "`i8` to `i64` and `u8` to `u64`, `==`, `!=`, `<`, `<=`, `>`, and `>=` between two of one type, and an error where two types meet in an operator"
  - "`Int`, `Nat`, and `Byte`, in every file with no import, the same types as `i64`, `u64`, and `u8`, `Nat` the type of the `size` of a `List` and of a `str` and of a `loop` variable over `0..xs.size`, and `use std.bits (u8, u16, u32, u64, i8, i16, i32, i64)`, or any of them, the names of sizes, which a file that writes one with no import is refused"
  - "`a..b` written as a type, with whole numbers for its bounds, held by a parameter, a result, a field, a list, an `Option`, and a tuple, an empty range as a type, a range of 65 to 128 bits written as a type, held in two words, a literal that must be in the range, and `==`, `<`, and the rest between two of one range, and the text of one. A range is one type of its bounds, so one range meets another only where their bounds are the same, and a range whose second bound is below its first refused"
  - "a range taken where a range that holds every number of it is needed, with nothing written, at a binding, a `mut`, an argument, a field, an item of a list, a result, and a block of an `if` or a `match` given where a type is needed, kept at the size of the wider range, and the smallest range that holds the blocks of an `if` or a `match` that give ranges where no type is needed"
  - "a range taken where a narrower one is needed, at the same places, which compiles when a condition or a `requires` shows that it fits, and is refused otherwise with an error that says what could not be proven"
  - "`==`, `!=`, `<`, `<=`, `>`, and `>=` between two different ranges, as the numbers they hold are, whatever their bounds, among them an `Int` and a `Nat`, which no type of 64 bits holds both of, and two ranges of 65 to 128 bits"
  - "a range whose bound is the name of a parameter, `0..n` or `m..256`, as the type of a parameter: the body knows that the parameter is below `n` or at least `m`, as a `requires` says, and a call compiles when its argument is shown to be, and is the empty type where `n` is not above the first bound"
  - "a name for a bound in the type of a result, as `0..n` or `m..256`, which the function is shown to give and a caller knows, and in the type of a binding made with `=`, which is shown where it is written and known wherever the name is used, and a name in the type of a field, of a `mut`, or inside another type refused with the reason, and a `mut` or an unbound name refused as a bound"
  - "the `size` of a name as a bound, `0..xs.size` or `xs.size..256`, wherever a name is a bound, where `xs` is a `List` or a `Slice` that is a parameter or an `=` binding, and no other field and no bound that is worked out, such as `0..n + 1` or `0..g.rows`, which is refused with \"a bound is a whole number, a name, or the `size` of a name\""
  - "not the `size` of a `str` or of an array as a bound, as no fact is known of the size of a text"
  - "a `mut` with no stated type given the smallest range that holds every value it is given"
  - "a trait implemented for a range had by every range it holds, at a call, a bound of a type parameter, which is then that range, and inside an `Option`, a struct, a tuple, and a `List`, a second implementation for a range that overlaps one already written refused naming it, implementations for two sizes that overlap, of which a range uses the size it is kept at, and `Wrapping` had only by a range that fills a size; `Eq`, `Ord`, and `Fmt` of a range of more than 64 bits are those of the 64 bits of its sign, as no implementation for 128 bits can be written"
---

# Number types

Every whole number is a range: the whole numbers from one number up to but not including another, written `a..b`. The compiler has one kind of whole number, the range, and the floats `f16`, `bf16`, `f32`, and `f64`, which are not ranges.

```
use std.console (console, print)

fn main() <console, alloc> {
  small: 0..256 = 200
  low: -5..6 = -3
  print("{small} {low}")
}
```

This prints `200 -3`. `0..256` holds 0 to 255, and `-5..6` holds -5 to 5. A type is written after a colon, as a parameter's is.

Say what a number holds in its type, as narrowly as the function means it. A parameter that is an index is a `0..xs.size`, not a `Nat` with a `requires`, and a sum is left as the range arithmetic gives it. A sum that may not fit is checked with an `if`, as [Integer arithmetic](arithmetic.md#checking-first) shows, and `saturating_` and `wrapping_` are for arithmetic that is meant to saturate or to wrap. None of these is a way to make a sum fit a type it would have fitted with a narrower one.

A name with no stated type has the range that its body gives it: the smallest that holds every value it is given, as [Inference](inference.md) says. `n = 5` is a `5..6`, and a `mut` given `0` and then `9` is a `0..10`.

## A range as a type

`a..b` holds `a` and every whole number above it up to `b`, and it excludes `b`, as a range is written in [Ranges and slices](ranges.md). Its least value is `a` and its greatest is `b - 1`.

```
fn brightness(level: 0..101) -> 0..256 { level }
```

`brightness(100)` is `100`. A value of a range is accepted wherever a range that holds every value of it is expected, with nothing written, so the `level` of `0..101` is a `0..256` where the result needs one. There is no `convert` between whole numbers.

A range with no number, as `5..5` or `0..n` where `n` is `0`, is a type too. No value can be given to it, so a function that takes one cannot be called.

```
fn never(x: 5..5) -> Nat { 0 }
```

`never` is accepted, and no call of it is, as no value is a `5..5`.

A range whose second bound is a name below its first holds no number either. `a..n` is every whole number that is at least `a` and below `n`, and when `n` is below `a` there is none, as when the two are equal. Nothing is proved and nothing is checked when the program runs.

```
fn spare(from: Nat, n: Nat, x: from..n) -> Nat { x }
```

`spare` is accepted. Where `n` is below `from`, no value is a `from..n`, so it cannot be called there.

A range written with two numbers, the second below the first, is refused, as both are in view and it can only be a mistake. An empty range is written with the two equal.

```
fn wrong(x: 5..3) -> Nat { 0 }
```

`wrong` is refused, because `3` is below `5`.

## A name as a bound

A bound of a range is a whole number or a name, and a name is allowed in three places: the type of a parameter, the type of a result, and the type of a binding. The name is a parameter, or a binding made before with `=`, which never changes.

```
fn last(n: 1..1000) -> 0..n { n - 1 }

fn middle(xs: List[Nat]) -> Nat
  requires xs.size > 0
{
  n = xs.size
  i: 0..n = n / 2
  xs[i]
}
```

`last(10)` is `9`, and the caller holds a value it knows is below `10`, with no `ensures` written. In `middle`, `i` is known to be below `n` wherever it is used, so `xs[i]` needs no proof of its own. A result or a binding is given such a type as any value enters a narrower range: where it is shown to fit, and refused otherwise.

```
fn first_free(n: Nat) -> 0..n { 0 }
```

`first_free` is refused, because `0` is not below `n` when `n` is `0`.

A bound may also be the `size` of a name, written `xs.size`, where `xs` is a parameter or an `=` binding that is a `List`, a `Slice`, a `str`, or an array. So a function that indexes says in its signature what it needs.

```
fn pick(xs: List[Nat], i: 0..xs.size) -> Nat { xs[i] }
```

`pick([4, 5, 6], 2)` is `6`. `xs[i]` needs no proof, as `i` is below `xs.size` by its type, and `pick(xs, 3)` on a list of three is refused where it is called. The size of a list that a function is lent does not change while it holds it, so the bound means one number for the whole function.

`size` is the only field a bound may name, and a bound is never worked out: `0..grid.rows`, `0..n + 1`, and `0..count(xs)` are each refused, the first with "a bound is a whole number, a name, or the `size` of a name". A binding gives any other number a name first.

```
m = n + 1
i: 0..m = n
```

A name is not a bound in the type of a field, of a `mut`, or inside another type such as `List[0..n]`.

```
struct Cursor {
  size: Nat
  at: 0..size
}
```

`Cursor` is refused, because a field's type names no other field. What holds between two fields of a struct is said by a [proof field](proof-fields.md), which is the one way to say it. A `mut` can change, so a type that named one would change with it, and a `mut` is refused as a bound for the same reason.

A value goes the other way only where it is shown to fit, or taken in by `narrow` or `clamp`, as [Integer arithmetic](arithmetic.md#where-a-value-enters-a-narrower-range) says.

```
fn dim(level: 0..256) -> 0..101 { level }
```

This is refused, as `0..256` holds 101 to 255 and `0..101` does not.

## The traits of a range

A range has every trait that is implemented for a range that holds it. So a trait written for `Byte` is had by a `0..10`, as a `0..10` is taken wherever a `Byte` is needed.

```
trait Double[T] {
  fn double(x: T) -> Nat
}

impl Double[Byte] {
  fn double(x: Byte) -> Nat { x * 2 }
}

fn twice(x: 0..10) -> Nat { double(x) }
```

`twice(3)` is `6`. The implementation is given `x` as the `Byte` it was written for, with nothing written.

Two implementations for ranges that overlap are refused, whether one range is inside the other or each holds numbers of the other. So the implementation a call uses never depends on how narrow a range was inferred. This is refused with the `impl` above.

```
impl Double[0..100] {
  fn double(x: 0..100) -> Nat { x + x }
}
```

The error is "`Double` is already implemented for `Byte`, which `0..100` is inside", and so it is for `-50..50` beside `0..100`, which overlap in `0..50`: "`Double` is already implemented for `0..100`, which `-50..50` overlaps". A variant for a smaller range gets a function of its own, as `double_small`, which takes that range and is called with a value that is proven to be in it.

```
fn double_small(x: 0..100) -> Nat { x + x }
```

`Eq`, `Ord`, `Fmt`, `Repr`, and `Hash` are had by every range in this way and no other. Each is written once for each size, as the operators are. The sizes are kept apart, so implementations for two of them may overlap, and a range uses the one for the size it is kept at.

A type parameter whose bound a range meets in this way is the range of the implementation, so a generic call gives what a direct one gives.

```
use std.bits (Bits, bit_not)

fn flip[T: Bits](a: T) -> T { bit_not(a) }
```

With `x: 0..10 = 3`, `flip(x)` has `T` as `u8`, as `bit_not(x)` uses the `Bits` of `u8`, and is the `u8` `252`. So `y: 0..10 = flip(x)` is refused, as a narrower range is wherever it is not shown to fit. This holds through the bound of an `impl` too: an `impl[T: Step] Step[Option[T]]` for an `Option[0..10]`, where `Step` is written for `Byte`, is the one for `Option[Byte]`.

`Wrapping` is the one exception. It is had only by a range that fills a size, as [Arithmetic](arithmetic.md#wrapping) says, since wrapping is where a size wraps. So `wrapping_add` of a `0..101` is refused: "there is no implementation of `Wrapping` for `0..101`".

## Three names

A range is a type, and a name for one is only a way to write it. The prelude has three, for what a number is for.

| Name | Is | For |
| --- | --- | --- |
| `Int` | `-9223372036854775808..9223372036854775808`, the 64-bit signed range | A number |
| `Nat` | `0..18446744073709551616`, the 64-bit range from 0 | A count or a size |
| `Pos` | `1..18446744073709551616`, the 64-bit range from 1 | A count or a size that is not zero |
| `Byte` | `0..256` | A byte |

```
fn longer(a: List[Byte], b: List[Byte]) -> Nat {
  if a.size > b.size { a.size } else { b.size }
}
```

No name of a whole number type is in the compiler. The names of sizes are in the module [`std.bits`](../library/bits.md), and a file that works with sizes imports them.

| Name | Is |
| --- | --- |
| `u8`, `u16`, `u32`, `u64` | `0..256`, `0..65536`, `0..4294967296`, `0..18446744073709551616` |
| `i8`, `i16`, `i32`, `i64` | `-128..128`, `-32768..32768`, `-2147483648..2147483648`, `-9223372036854775808..9223372036854775808` |

```
use std.bits (u64)

fn count(n: Nat) -> u64 { n }
```

`Nat` and `u64` are one type with two names, so `count` converts nothing. So are `Int` and `i64`, and `Byte` and `u8`.

## Floats

`f16`, `bf16`, `f32`, and `f64` hold numbers with a point. A float is written as [Literals](literals.md) says. A float is not a range, and a range is not a float: one becomes the other only through a call, as `float(n)` makes a float of a whole number.

```
fn half(n: Int) -> f64 { float(n) / 2.0 }
```

`half(5)` is `2.5`. `n / 2.0` with an `Int` and an `f64` is refused, as an operator takes two ranges or two floats.

## Comparing two numbers

`==` and `!=` ask whether two numbers are equal, and `<`, `<=`, `>`, and `>=` how they stand. Each gives a `bool`, and is a trait function, as [Comparisons](comparisons.md) says. Any two ranges are compared, whatever their bounds.

```
use std.console (console, print)

fn main() <console, alloc> {
  x: 0..256 = 200
  y: -5..6 = 3
  print("{x == 200} {x < y} {x != y}")
}
```

This prints `true false true`. A range is not compared with a float, as it is not added to one.

`Fmt`, `Eq`, `Ord`, `Serialize`, and the like are written once, for every range.

## How a range is kept

How many bytes a range takes, and in what form, is the compiler's choice, and a program cannot observe it. [How a whole number is kept](../compiler.md#how-a-whole-number-is-kept) says what it is. A range whose number of values is small is small, whatever its bounds, so `1900..2100` is one byte.

## The forms

| Written | Is |
| --- | --- |
| `a..b` | The whole numbers from `a` up to but not including `b` |
| `Int`, `Nat`, `Pos`, `Byte` | The 64-bit signed range, the 64-bit range from 0, the 64-bit range from 1, and `0..256`, with no import |
| `u8` to `u64`, `i8` to `i64` | The ranges that fill 8 to 64 bits, imported from `std.bits` |
| `f16`, `bf16`, `f32`, `f64` | A float of 16, 16, 32, or 64 bits |
| A name with no stated type | The smallest range that holds every value it is given |
| A range where a wider one is expected | Accepted, with nothing written |
| A range where a narrower one is expected | Accepted where it is shown to fit, or taken in by `narrow` or `clamp`, refused otherwise |
| `a == b`, `a < b`, and the rest | A `bool`, for any two ranges |
| A range and a float in an operator | Refused: `float(n)` makes one of the other |

Next: [Integer arithmetic](arithmetic.md)
