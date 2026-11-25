---
type: specification
title: Type aliases
description: type NAME = TYPE, which gives a type a second name that is the same type, with type parameters, pub, and use, and what an alias does not bring.
status: built
---

# Type aliases

`type NAME = TYPE` names a type. The name is the same type as what it names, not a new one.

```
type Port = 0..65536

fn next(p: Port) -> Port { p }
```

A value of `Port` is accepted wherever a `0..65536` is needed, and the other way round, and every check gives the same answer for the one as for the other.

```
use std.console (console, print)

fn main() <console, alloc> {
  a: Port = 80
  b: 0..65536 = next(a)
  print("{b}")
}
```

This prints `80`. A type that must be told apart from what it holds is a `struct`, as [Structs](structs.md) says.

## Type parameters

An alias takes type parameters in brackets, written as a struct's are, and a use gives them in brackets.

```
type Pair[T] = (T, T)

fn swap(p: Pair[Int]) -> (Int, Int) {
  (a, b) = p
  (b, a)
}
```

`Pair[Int]` is `(Int, Int)`. A use with another number of type arguments is refused, with the count named.

```
fn f(p: Pair[Int, Int]) {}
```

The error is "`Pair` takes 1 type argument, and this gives 2".

## Exporting and importing

An alias is at the top of a module. `pub` exports it, and `use` imports it by its name, as for a struct.

```
pub type Port = 0..65536
```

```
use net (Port)
```

## An alias does not name itself

An alias that names itself, directly or through other aliases, is refused where it is declared, and the chain is named.

```
type A = B
type B = List[A]
```

The error is "the alias `A` names itself: A -> B -> A". A type that holds itself is written as a `struct` or an `enum`, as [Recursive types](recursive-types.md) says.

## An alias brings nothing

An alias has no `impl`, trait, or `without` of its own. It is the type it names, so `impl Fmt[Port]` is an `impl` for `0..65536`, which is where [An impl lives](impl-placement.md) says it goes.

An error names the alias where the program wrote the alias.

```
a: Port = 70000
```

The error is "`70000` does not fit `Port`, which holds 0 to 65535".

## The forms

| Written | Meaning |
| --- | --- |
| `type Port = 0..65536` | `Port` is `0..65536` |
| `type Pair[T] = (T, T)` | `Pair[Int]` is `(Int, Int)` |
| `pub type Port = ...` | `Port` is exported, and `use` imports it |
| `type A = B` and `type B = List[A]` | Refused: an alias names itself |
| `Pair[Int, Int]` | Refused: `Pair` takes 1 type argument |
| `impl Fmt[Port]` | An `impl` for `0..65536` |

Next: [Projects](projects.md)
