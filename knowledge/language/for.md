---
type: specification
title: for
description: A for describes an array by the value at every index, and each index gets its size from an array it reads or from the type the for is written for.
status: partial
built:
  - "`for i { e }` with one index whose size is a number, which it takes from each array that is a name read at the index alone, `a[i]`, and from the type the `for` is written for, as a result, an argument, or a binding that states its type; it is refused when two of these differ or none gives a size, the index is the range below the size, so `arith` proves a read at it or at arithmetic on it, `x[i + 1]`, which gives it no size, the block is refused when it needs an effect but `alloc`, and the array is held in the frame, so no memory is asked for"
  - "not a size that is a name, several indices, a block that assigns a `mut` outside it, or elements without `Copy`"
---

# `for`

`for i { e }` describes an array: the value `e` at every index `i`.

```
fn ramp() -> [4]f32 { for i { float(i) } }
```

`ramp()` is `[0.0, 1.0, 2.0, 3.0]`. The index `i` counts from `0`, as [Indexing](indexing.md) says of a place, and `float(i)` is the float with the value of `i`, as [Number types](number-types.md) says.

A `for` is a view, a rule for computing each element, so it allocates nothing by being written. [Array buffers](buffers.md) says when an array does get a buffer.

## Reading an array

The value may read an array at the index. This `for` is the array whose element `i` is twice element `i` of `a`.

```
fn doubled(a: [3]f32) -> [3]f32 { for i { a[i] * 2.0 } }
```

## Several indices

Several indices give several axes. `for i, j { e }` is a matrix, and `e` is its element at row `i` and column `j`.

```
fn grid() -> [2, 3]f32 { for i, j { float(i) * 3.0 + float(j) } }
```

`grid()` has two rows of three floats, and its element at `i, j` is `3i + j`, so the first row is `0.0, 1.0, 2.0` and the second is `3.0, 4.0, 5.0`.

## Where an index gets its size

An index gets its size from one of two places, and both are in the source.

**From the arrays it reads.** Where the index alone picks an element along an axis, as in `a[i]`, it has that axis's size.

```
fn halves(a: [3]f32) -> [3]f32 {
  b = for i { a[i] * 0.5 }
  b
}
```

The `i` of `b` is `3`, because it picks along the one axis of `a`, so `b` is a `[3]f32`. When an index is used this way on several arrays, their sizes must be equal.

```
fn product[n: Nat](a: [n]f32, b: [n]f32) -> [n]f32 { for i { a[i] * b[i] } }
```

With `a: [3]f32` and `b: [4]f32` the index would have two sizes, so it is refused.

```
fn mismatch(a: [3]f32, b: [4]f32) -> [3]f32 { for i { a[i] * b[i] } }
```

This is refused: "the index `i` has two sizes: 3 from `a`, and 4 from `b`".

**From the type it is written for.** Where the `for` is written in a place that expects an array, such as a function's result, an argument, or a binding that states its type, its indices take their sizes from that type. `ramp` above takes its `4` from its result. Here a binding states it.

```
evens: [4]f32 = for i { float(i) * 2.0 }
```

When both places give a size, the two must agree. The `i` below is `3` from `a` and `4` from the result, so it is refused: "the index `i` has two sizes: 3 from `a`, and 4 from the type the `for` is written for".

```
fn widen(a: [3]f32) -> [4]f32 { for i { a[i] } }
```

An index that gets a size from neither is refused. Nothing gives the `i` below a size, since no array is read at it and no type is stated.

```
fn lost() {
  x = for i { 1.0 }
}
```

This is refused: "the index `i` has no size, as no array is read at it alone and no type is stated for the `for`", with the fix "read an array at `i` alone, as `a[i]`, or state the type of the `for`, as `x: [4]f32 = for i { ... }`".

## Arithmetic on an index

Arithmetic on an index, as in `x[i + 1]`, never gives it a size. The compiler does not solve for one. The access is instead proven to be in range, as [What arith proves](arith.md) describes. So the size comes from the type.

```
fn shifted(x: [5]f32) -> [4]f32 { for i { x[i + 1] } }
```

`i` is `4`, from the result, and `arith` proves that `i + 1` is below `5`. Without a type stated, the `i` of `y = for i { x[i + 1] }` has no size, and it is refused as `lost` is.

## No order

The iterations of a `for` are independent, so every `for` may run across SIMD lanes, threads, or a GPU. The keyword states that order does not matter, where a [`loop`](loop.md) runs in order. What follows from this for a block that changes a `mut` is in [Accumulation](accumulation.md).

The same holds for effects. A `for` body may only allocate: it carries no effect but `alloc`, since an effect has an order and the iterations have none.

```
use std.console (console, print)
use std.time (now, time)

fn stamps() -> [4]u64 <time> { for i { now() } }

fn lines(n: Nat) <console> { for i { print("{i}") } }
```

Both are refused: "a `for` may only allocate, and `now` needs the `time` effect", with the fix "write a `loop`, which runs in order". Work in order is a `loop`, as the writes are here.

```
use std.console (console, print)

fn lines(n: Nat) <console, alloc> {
  loop i in 0..n { print("{i}") }
}
```

## The forms

| Written | Is |
| --- | --- |
| `for i { e }` | The array whose element at `i` is `e` |
| `for i, j { e }` | The matrix whose element at `i, j` is `e` |
| `a[i]` in the block | Gives `i` the size of the axis of `a` |
| A stated type for the `for` | Gives each index the size of its axis |
| `x[i + 1]` in the block | Gives `i` no size |
| An effect other than `alloc` in the block | Refused: write a `loop` |

Next: [Operators on arrays](element-wise.md)
