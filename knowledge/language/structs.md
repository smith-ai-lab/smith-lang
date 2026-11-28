---
type: specification
title: Structs
description: Structs with named fields and defaults, reading a field, assigning to a field of a mut, and a definition at the top level of a file.
status: partial
built:
  - "structs with fields and defaults, reading a field, and assigning to a field of a `mut` with `=`"
  - "a struct that takes a whole number as a parameter, written `[n: Nat]` or `[n: Pos]`, which a type names with the number written out, as `Box[10]`, and whose name is a bound in the type of a field, as `0..n`, and a construction that leaves the number out, which takes it from the type it must give"
  - "a field whose type is a statement, which holds a proof, as [A struct that holds a proof](proof-fields.md) says"
  - "a value at the top level of a file that states no type is refused, at its name, with the fix that writes a type after the name; one that states its type is not built yet"
---

# Structs

A struct is a value that holds several values, each under a name. It is declared with `struct`, and its fields are written one per line, each as `name: Type`.

```
struct Rect {
  w: i32
  h: i32
}
```

## Making one and reading a field

A struct is made by calling its name, with every field given by name. `.` reads a field.

```
use std.console (console, print)

fn wider(a: Rect, b: Rect) -> bool { a.w > b.w }

fn main() <console, alloc> {
  r = Rect(w: 3, h: 4)
  print("{r.w} {r.h}")
}
```

This prints `3 4`. A struct is a value like any other: it can be bound, passed to a function, and returned from one.

The fields are given by name, so the order they are given in is not the order they are declared in. `Rect(h: 4, w: 3)` is the same value as `Rect(w: 3, h: 4)`.

A call that leaves a field out, names one the struct does not have, gives a field a value of another type, or gives fields without names is refused.

```
r = Rect(w: 3)
```

This gives "`Rect` needs a value for `h`".

```
r = Rect(w: 3, h: 4, d: 1)
```

This gives "`Rect` has no field named `d`".

```
r = Rect(w: 3, h: "four")
```

This gives "this gives text, where a `i32` is needed".

```
r = Rect(3, 4)
```

This gives "the fields of `Rect` are given by name", with the fix "write `w: ` before it". So there is one way to build a struct, the one with names.

Reading a field the struct does not have is refused the same way: `r.depth` gives "`Rect` has no field named `depth`".

## One field to a line

A struct's fields are written one to a line. `struct Rect { w: i32, h: i32 }` is refused: "expected a line break or `}`, found `,`".

## Changing a field of a `mut`

A field of a [`mut`](mut.md) can be assigned with `=`.

```
use std.bits (i32)
use std.console (console, print)

fn widen(r: Rect, to: i32) -> Rect {
  mut wide = r
  wide.w = to
  wide
}

fn main() <console, alloc> {
  r = Rect(w: 3, h: 4)
  w = widen(r, 9)
  print("{r.w} {w.w} {w.h}")
}
```

This prints `3 9 4`. Nothing else can name the value a `mut` holds, so the change is seen through that `mut` alone: it now holds a copy of the struct with the field replaced, and `widen` leaves `r` as it was. A field of a field is assigned the same way, as `o.inner.v = 4` is for a `mut` named `o`.

The other forms of assignment, `+=`, `-=`, `*=`, and `/=`, work on a field as they do on a `mut`. With `wide.w += 2`, the field `w` of `wide` becomes the field plus 2.

A field that a [proof field](proof-fields.md#changing-a-field) or the type of another field names is not assigned: the value is built again. Assigning to a field of a name that is not a `mut` is refused.

```
use std.console (console)

fn main() <console, alloc> {
  r = Rect(w: 3, h: 4)
  r.w = 5
}
```

This gives "`r` is not a `mut`, so it cannot be assigned to", with the fix "declare it with `mut`: write `mut r` where it is defined". A value of the wrong type is refused as it is for a call: `r.w = "x"` on a `mut r` gives "this gives text, where a `i32` is needed". Assigning to a field the struct does not have gives "`Rect` has no field named `depth`".

## Defaults

A field may have a default, written after its type with `=`, as a [parameter's](functions.md) is. A default is a fixed value, and any field may have one, wherever it stands. Construction may then leave the field out.

```
use std.bits (i32)
use std.console (console, print)

struct Border {
  width: i32 = 1
  inset: i32 = 0
}

fn main() <console, alloc> {
  thin = Border()
  wide = Border(width: 4)
  print("{thin.width} {thin.inset} {wide.width} {wide.inset}")
}
```

This prints `1 0 4 0`. A field that comes after one with a default may have none, because fields are always given by name.

```
use std.bits (i32)
use std.console (console, print)

struct Margin {
  width: i32 = 1
  inset: i32
}

fn main() <console, alloc> {
  a = Margin(inset: 2)
  b = Margin(width: 4, inset: 3)
  print("{a.width} {a.inset} {b.width} {b.inset}")
}
```

This prints `1 2 4 3`. A field with no default still has to be given: `Margin()` gives "`Margin` needs a value for `inset`".

## A struct that takes a whole number

A struct may take a whole number as a parameter, written in brackets after its name as `[n: Nat]`. The number is known when the program is built, so it costs nothing when the program runs. Its name stands for the number in the type of a field, wherever a number may be written, as `0..most` is.

```
struct Box[most: Nat] {
  at: 0..most
}
```

A type names the number by writing it out. `Box[10]` is a `Box` whose `at` is a `0..10`, and a `Box[10]` is a `Box[3]` never: two types are one type exactly when their numbers are equal.

```
fn top() -> Box[10] { Box(at: 9) }
```

The construction leaves the number out and takes it from the type it must give, so `Box(at: 9)` is a `Box[10]` here, and `9` is proven to be below 10. `Box(at: 10)` is refused: "`10` does not fit `0..10`, which holds 0 to 9". A construction that nothing gives a type is refused: "the whole number `most` of `Box` is not known here, as no argument and no type that the call must give says it", with the fix "write it after the name, as `Box[4]`". `Box[4](at: 1)` writes it.

A number that must not be zero is declared `[n: Pos]`. `Row[0]` is refused for `struct Row[n: Pos]`: "`0` is not a `Pos`, which `n` of `Row` is". A function takes a number the same way, as [Inference](inference.md#a-whole-number-a-call-leaves-out) says.

## At the top level of a file

A value may be defined at the top level of a file, outside every function, and it states its type, as every other definition at the top level does: a function states its parameters and result, a struct its fields. What is written at the top level is the contract, and what is inside a body is solved, so changing a value from `100` to `200` cannot change its type where it is read. Here `unit` is a `Rect` that any function of the file can read.

```
unit: Rect = Rect(w: 1, h: 1)
```

A top-level value with no type is refused, at its name: `unit = Rect(w: 1, h: 1)` gives "a value at the top level states its type, and `unit` does not", with the fix that writes the type after the name, `unit: Rect = Rect(w: 1, h: 1)`. The compiler names the type in the fix once it builds top-level values, and until then the fix shows `T` where the type goes.

A top-level value is fixed. It is worked out when the program is built and kept in the executable, so it costs nothing when the program starts, and it cannot be assigned, as no name that is not a `mut` can.

It is made of literals, structs, enums, tuples, lists, other top-level values, and calls of functions that carry no effect. A list is allowed, though building one in a function needs `alloc`, since here the compiler builds it.

```
keywords: List[str] = ["fn", "if", "else", "loop", "match"]

fn area(r: Rect) -> Int { r.w * r.h }

unit_area: Int = area(unit)
```

A top-level value that needs an effect is refused: one that prints, reads a file, or calls a function that carries `diverge`. So is one that is defined by itself, directly or through another. What a program must do when it starts, it does in `main`, where its effects are in the signature. What a build must read from the project's files, a [build step](build-steps.md) reads.

## The forms

| Written | Is |
| --- | --- |
| `struct S { a: T }` | A struct with a field `a` of type `T`, one field to a line |
| `struct S { a: T = e }` | The same, where construction may leave `a` out and it is `e` |
| `S(a: e)` | A `S` whose field `a` is `e`, every field named |
| `s.a` | The field `a` of `s` |
| `s.a = e` | Replaces the field `a` of the `mut` `s` |
| `struct S[n: Nat] { a: 0..n }` | A struct whose field has a bound that is a whole number the type names |
| `S[10]` | The type of that struct for `n` as 10, which is no other type |
| `x: T = e` at the top level | A fixed value, worked out when the program is built, from what carries no effect |

Next: [Tuples](tuples.md)
