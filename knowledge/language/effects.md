---
type: specification
title: Effects
description: An effect as something a function does outside itself, how a signature lists the effects a function may perform, and what a caller must have.
status: partial
built:
  - "`console`, `alloc`, and `diverge` in a signature, each listed once and required of every caller that needs it, and checked where a lambda is written"
  - "not an effect on a function type"
  - "a function with effects passed by its name, whose effects are required of the function its name is written in"
  - "an effect the program declares, in a signature, and required of every caller of its operations, as `console` is"
  - "a warning at an effect a function lists and nothing in it needs, with the signature without it as the fix, not given for a function of an `impl` or one passed by its name as a value"
  - "a group, `effects NAME = <e1, e2>`, which `pub` exports and `use` imports as any name, which may name other groups, which stands wherever effects are listed, and which is refused when it names itself; the warning at a group nothing in it uses; the fix that writes a group's name"
---

# Effects

An effect is something a function does outside itself, and a signature lists each one the function may do, after its return type in angle brackets.

```
use std.console (console, print)

fn main() <console> {
  print("hello")
}
```

`<console>` says that `main` may use the console, and `print` does. This prints `hello`.

## A function with no effect

A function that lists no effects is pure. It does nothing outside itself, so it can be called anywhere.

```
use std.bits (i32)

fn first(a: i32, b: i32) -> i32 { a }
```

## Effects as permissions

A function without an effect cannot perform it, so an effect works as a permission. A pure function that prints is refused.

```
use std.console (console, print)

fn greet(name: str) {
  print("hello")
}

fn main() <console> {
  greet("Ana")
}
```

This gives "`print` needs the `console` effect, which `greet` does not declare", with the fix "write `fn greet(name: str) <console>`".

## Listing more than one

Effects are listed after the return type, separated by commas, in any order.

```
use std.console (console, print)

fn shout(name: str) -> str <console, alloc> {
  print("hey")
  "hello {name}"
}
```

`<alloc, console>` means the same. An effect is named once in a list, so naming it again is refused.

```
use std.console (console, print)

fn main() <console, console> {
  print("hi")
}
```

This gives "the `console` effect is already listed", with the fix "write `<console>`". The list is a set, so its order means nothing, and two signatures that differ only in the order of their effects have the same type. `<>` is refused, with the fix "write no effects, and leave out `<>`".

## What a caller needs

A caller must have every effect its callees use. `main` below calls `shout`, which needs `console` and `alloc`.

```
use std.console (console, print)

fn main() <console, alloc> {
  print(shout("Ana"))
}
```

This prints `hey` and then `hello Ana`. A caller that lacks one of them is refused.

```
use std.console (console, print)

fn main() <console> {
  print(shout("Ana"))
}
```

This gives "`shout` needs the `alloc` effect, which `main` does not declare", with the fix "write `fn main() <console, alloc>`".

A call states no effect. `shout("Ana")` is written as it would be for a pure function, because an effect is a parameter that the compiler passes for you, and it costs nothing at run time. So a signature says everything a function may do, while the code that calls it stays plain.

## Who writes the list

The stored signature always states the effect list. The compiler proposes it while the function is being written, as the fix beside each refusal above does. A [lambda](lambdas.md) has no signature, and its effects are inferred.

## A lambda's effects

A lambda's effects are checked where it is written: whatever its body does must be allowed by the function it sits in, and the function receiving it says nothing about them. `do` below runs a lambda and states no effect.

```
use std.console (console, print)

fn do[T](body: fn() -> T) -> T { body() }

fn main() <console> {
  do { print("hi") }
}
```

This prints `hi`, because `main` allows `console`. With `fn main() {` in its place the program is refused: "`print` needs the `console` effect, which `main` does not declare".

A function that receives a lambda never lists the effects of the lambda, and no signature can say "whatever the lambda does". There are no effect variables. This is sound because the function that receives a lambda can call it and pass it on, and cannot store or return it, so the lambda runs only inside the call it is written at. A [lambda that is kept](lambdas.md#where-a-lambda-may-go) is a function stored in a value, and states its effects as the next section says.

A function with a name that is passed is checked the same way, since passing `hi` is passing `fn() { hi() }`.

```
use std.console (console, print)

fn hi() <console> { print("hi") }

fn main() <console> {
  do(hi)
}
```

This prints `hi`. The effects of `hi` are required of `main`, where its name is written, and `do` states none. With `fn main() {` it is refused with "`hi` needs the `console` effect, which `main` does not declare", at the name, with the fix "write `fn main() <console>`". A name inside a lambda asks the function the lambda is written in, at any depth.

## A function stored in a value

A function stored in a value states its effects in its type, after the result as in a signature, and is pure when none are written.

```
struct Button {
  label: str
  on_press: fn() <console>
}
```

`on_press` is a function that may use the console. A field of type `fn()` would be pure. What is stored has exactly the effects its type states: `Button(label: "ok", on_press: hi)` is accepted for `hi` above, and a function that prints is refused for a field of type `fn()`, because whoever calls the field later is checked against the type alone. Only a function that is passed has its effects checked where it is written.

## An effect that is listed and not needed

An effect in a signature is a permission, and every caller must carry it. So a function that lists an effect nothing in it needs is asking its callers for more than it uses, and `smith check` says so.

```
use std.console (console, print)

fn main() <console, alloc> {
  print("hello")
}
```

`smith check` warns: "`main` lists the `alloc` effect, and nothing in it needs it", with the fix "write `fn main() <console>`". It is a warning, as an unused import is, so `smith build` and `smith run` still build the program, and `smith check` fails.

Three functions may list an effect they do not need, and are not warned of. A function of an `impl` has the signature its trait gives, and one implementation may need less than another. A function passed where a parameter's type allows an effect need not use it. And a function whose body holds a [`hole`](hole.md) is not finished. A `pub` function is warned of as any other: an effect is not listed to keep a place for later.

## Naming a set of effects

A program that lists the same effects in many signatures names them once, as a group, at the top level of a module.

```
use std.console (console, print)

effects server = <console, alloc>

fn handle(name: str) -> str <server> {
  print("hey")
  "hello {name}"
}

fn main() <server> {
  print(handle("Ana"))
}
```

This prints `hey` and then `hello Ana`. A group stands wherever an effect may be written: in a signature, in the type of a function, and in an operation of an effect. A list means the union of what it names, as a set, so `<server, console>` is `<console, alloc>`, and two signatures that differ only in whether they write a group or the effects it holds have the same type. A program written with groups builds the executable of the same program with the groups written out.

A group may name other groups. A group that overlaps another, or that holds an effect the list also names, is accepted, and the effect counts once. Naming the same effect or the same group twice in one list is refused, as above. A group that names itself, directly or through other groups, is refused at its declaration, with the chain: "the group `a` names itself: a -> b -> a".

`pub effects server = <console, alloc>` exports the group, and `use net (server)` imports it, by its name or as another with `as`, as any name.

`smith check` warns of a group in a signature only when nothing in the group is used: "`handle` lists `server`, and nothing in it needs any of its effects". A group that is partly used is not warned of. Every fix for a missing effect in one function writes the whole list the function needs, once all of them are added. Where a fix writes effects, and a group in scope holds exactly the effects it would write beyond those the signature lists, the fix writes the group's name.

## The forms

| Written | Means |
| --- | --- |
| `fn f() { ... }` | `f` is pure |
| `fn f() <console> { ... }` | `f` may use the console |
| `fn f() -> T <console, alloc> { ... }` | The effects follow the return type, in any order, each once |
| `fn f() <server> { ... }` | `f` may use every effect the group `server` names |
| `fn(A) -> B <console>` | A stored function that may use the console |
| `fn(A) -> B` | A stored function that is pure |

Next: [Declaring an effect](effect-declarations.md)
