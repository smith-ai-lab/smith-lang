---
type: specification
title: Lambdas
description: A function with no name, written where it is passed to a call.
status: partial
built:
  - "lambdas passed to a call, after it or by name, and function types on parameters"
  - "a lambda that writes the types of its parameters and of its result"
  - "a lambda that gives away a value of the function it is written in, passed to a parameter that takes its function"
  - "a lambda that reads and assigns no name of the function it is written in and writes the type of each parameter and of its result, bound to a name, stored in a field, and returned, each known when the program is built, and one that writes type parameters after `fn`"
  - "a lambda that leaves out the type of a parameter or of its result where it is stored in a field or returned, which it takes from the function type of the field or of the result, and a choice between lambdas given to a name, refused as a choice between functions is"
  - "a lambda that reads names of its function, holds the values they have as fields of the value that holds it, bound to a name and called in the same body, and stored in a field or returned when what it reads is copied, as the paragraph below says"
  - "not a lambda that leaves out a type where it is bound, which is refused with where to write it, and not the parameters of a lambda bound to a name inferred from the calls of it, as [Inference](inference.md#a-lambdas-parameters) says (#1138, #1139)"
---

# Lambdas

A lambda is a function with no name, written where it is passed to a call.

## One function, written three ways

A lambda is a function without its name, and a function type is a function without the names of its parameters and without its block.

```
// a function
fn double(v: f32) -> f32 { v * 2.0 }

// a lambda: the function without its name
fn(v: f32) -> f32 { v * 2.0 }

// its type: without the parameters' names and the block
fn(f32) -> f32
```

## A function that takes a function

A function says that it takes another function in a parameter's type. A function type is written `fn(T) -> U`: the types of the parameters in parentheses, then the type of the result. The parameter is called like any function.

```
fn twice(x: f32, f: fn(f32) -> f32) -> f32 { f(f(x)) }
```

`twice` takes a float and a function from one float to another, and calls the function two times.

A function that gives nothing has no arrow, as `fn(f32)`, and one that takes nothing is `fn() -> f32`. The effects of a function type follow its result, as a function writes them, `fn(f32) -> f32 <console>`, and a parameter the function owns is `fn(take f32) -> f32`. A list of types in parentheses without `fn` is always a [tuple](tuples.md).

## Writing a lambda

A lambda is the word `fn`, the names of its parameters in parentheses, and a block. It is written after the call's `)`, on the same line, and it is the call's last argument.

```
fn quarter(x: f32) -> f32 {
  twice(x) fn(v) { v * 0.5 }
}
```

Here `fn(v) { v * 0.5 }` is the `f` of `twice`, so `quarter(8.0)` is `2.0`.

- The types of a lambda's parameters and of its result are those of the function type it is passed as, so `v` is an `f32`. A lambda may write them as a function does, `fn(v: f32) -> f32 { v * 0.5 }`, each one or none. A type that is written and is not the one the function type needs is refused at the type.
- A lambda uses the type parameters of the function it is written in, and may write its own, as [Where a lambda may go](#where-a-lambda-may-go) shows. A lambda bound to a name and called in the same body has its parameters inferred from those calls, as [Inference](inference.md#a-lambdas-parameters) says. When nothing decides the type of its parameters, it writes them.
- A lambda has as many parameters as that function type. `twice(x) fn(a, b) { a }` is refused: "this lambda takes 2 parameters, where a `fn(f32) -> f32` is needed, which takes 1".
- The value of a lambda is the last expression of its block, as for a function. Its block may have several lines.

```
fn nudge(x: f32) -> f32 {
  twice(x) fn(v) {
    half = v * 0.5
    half + 1.0
  }
}
```

A lambda with several parameters names each.

```
fn combine(a: f32, b: f32, f: fn(f32, f32) -> f32) -> f32 { f(a, b) }

fn total(a: f32, b: f32) -> f32 {
  combine(a, b) fn(x, y) { x + y }
}
```

## Using what is around it

A lambda uses the names of the function it is written in. This one uses `rate`.

```
fn taxed_twice(x: f32, rate: f32) -> f32 {
  twice(x) fn(v) { v * rate }
}
```

It may assign that function's [`var`s](var.md) too. This counts how many times `twice` calls it, and gives `2.0`.

```
fn calls(x: f32) -> f32 {
  var count = 0.0
  twice(x) fn(v) {
    count = count + 1.0
    v
  }
  count
}
```

A lambda may give away a value of the function it is written in, as an argument of a `take` parameter. It can then be called once, and is passed only to a parameter that takes its function, as [A function that is called once](take.md#a-function-that-is-called-once) says.

## A lambda with no parameters

A lambda that takes no parameters is the block alone, `{ e }`, with no `fn` and no parentheses.

```
fn unless(skip: bool, otherwise: f32, body: fn() -> f32) -> f32 {
  if skip { otherwise } else { body() }
}

fn fee(is_member: bool, w: f32, h: f32) -> f32 {
  unless(is_member, 0.0) { area(w, h) * 0.1 }
}
```

## A lambda as the only argument

When the lambda is the only argument, the call has no parentheses. So a function can read like a part of the language. `do` runs a few lines for their value.

```
fn do[T](body: fn() -> T) -> T { body() }

fn rect_area(left: f32, right: f32, top: f32, bottom: f32) -> f32 {
  do {
    w = right - left
    h = bottom - top
    w * h
  }
}
```

The same holds for a lambda with parameters.

```
fn at_five(f: fn(f32) -> f32) -> f32 { f(5.0) }

fn ten() -> f32 { at_five fn(v) { v * 2.0 } }
```

## Several lambdas in one call

Only the last argument is written after the call. A call that takes several lambdas passes the others by name, inside the parentheses.

```
fn pick(first: bool, a: fn() -> f32, b: fn() -> f32) -> f32 {
  if first { a() } else { b() }
}

fn longer_side(wide: bool, w: f32, h: f32) -> f32 {
  pick(wide, a: { w }) { h }
}
```

## Where a lambda is written

Braces are the body of a construct or a lambda, and nothing else. Four rules follow.

- **A lambda begins on the line where the call's arguments end.** On the next line it is not part of the call, and a block there is refused, as a block that stands alone is not an expression.
- **The last lambda goes after the call.** Written inside the parentheses, as in `twice(x, fn(v) { v * 0.5 })`, it means the same, and the formatter moves it after the call, so a program has one form.
- **In the head of an `if`, a `match`, or a `loop`, `{` opens that construct's body.** So `if any { true } { ... }` is refused, since the first `{` is taken as the body of the `if`. A call that takes a lambda there is put in parentheses.

  ```
  fn any(f: fn() -> bool) -> bool { f() }

  fn sign(x: f32) -> f32 {
    if (any { x > 0.0 }) { 1.0 } else { -1.0 }
  }
  ```
- **A block that stands alone is not an expression.** `x = { 1.0 }` is refused. A few lines that give a value are passed to a function, as to `do` above.

## Where a lambda may go

A lambda is a value, so it goes where a value goes: it is passed to a call, bound to a name, stored in a field, and returned. One rule decides which lambdas may leave the function they are written in, and it is the rule for every value: [a lent value is not stored or returned](take.md#lending).

A lambda is lent the names around it that it reads or assigns, except those it reads whose values are copied, of which it holds a copy. So a lambda that assigns a name of its function, or reads one whose value is not copied, holds something lent, and it is passed to a parameter of a function type and nowhere else. The function that receives it may call it, or pass it on to such a parameter of another function, and may not store or return it, whether its parameter lends the lambda or takes it. So the lambda never outlives the names it uses, and what it does happens inside the call it is written at.

```
fn scaled(x: f32, rate: f32) -> f32 {
  twice(x) fn(v) { v * rate }
}
```

A lambda that uses no name around it, or only names it [takes](take.md#a-function-that-is-called-once) or names whose values are copied, holds nothing lent, and may be kept. Here `halve` is bound, then passed.

```
fn quarter(x: f32) -> f32 {
  halve = fn(v: f32) -> f32 { v * 0.5 }
  twice(x, halve)
}
```

A lambda may write type parameters in brackets after `fn`, as a function writes them after its name, and each use settles them, as each call of a function does.

```
fn both() -> (Int, str) {
  same = fn[T](x: T) -> T { x }
  (same(1), same("a"))
}
```

`both()` is `(1, "a")`: `T` is `i64` in the first call and `str` in the second. A lambda passed where a function type is needed has its type parameters settled by that type. No parameter asks for a function whose type parameters are still open, so no function type writes brackets.

A lambda that uses no name around it is a function with a name in all but the name, so it goes where a [function value](function-values.md) goes, as into a field whose type is a function type. A lambda that takes names holds them, and has a type of its own, so a field that holds it has a type parameter for it. A type parameter never stands for a lambda that holds something lent.

```
struct Step[F] {
  run: F
}
```

A lambda that takes nothing is a block alone, `{ 1.0 }`, and a block alone is a lambda only as the argument of a call, so `x = { 1.0 }` is refused, as a block that stands alone is not an expression, and `fn() { 1.0 }` is refused too, as a lambda with no parameters is a block alone. A lambda that is bound takes at least one parameter.

A lambda that reads names of its function holds the values they have where it is written, as the fields of the value that holds it, so a later assignment of a `var` it reads does not change it: with `k: f32 = 3.0` and `f = fn(x: f32) -> f32 { x * k }`, `f(2.0)` gives `6.0`. It is bound, stored in a field, and returned as a lambda that reads nothing is, has a type of its own as it has, and costs nothing, as a call of it is its body written where it is called.

```
struct Pricing {
  discount: fn(f32) -> f32
}

fn scaler(k: f32) -> fn(f32) -> f32 {
  fn(x: f32) -> f32 { x * k }
}

fn sale(rate: f32) -> Pricing {
  Pricing(discount: fn(v: f32) -> f32 { v - rate })
}
```

`scaler(3.0)(2.0)` is `6.0`, and `sale(1.0).discount(5.0)` is `4.0`. Each call of `scaler` and of `sale` gives a lambda that holds the values it was called with. A lambda that is returned reads the parameters of its function and no other name, since what a function gives is known before its body is checked: one that reads a name the function defines is refused, "a lambda that is given reads only the parameters of its function, as what a function gives is known before its body is checked, and this one reads `doubled`". Where it is stored or returned, it holds only what is copied, as a value that is lent to the lambda would outlive what lends it. A lambda that is bound is called in the body it is written in, so what it reads that is not copied, such as a list a function is lent, is held as it is, and is lent to the lambda for as long as the lambda can be called: giving the name away, as an argument of a `take` parameter, before the last call of the lambda is refused as a use after it was given away, at the call, saying that the lambda holds it, and giving it away after the last call is fine. It is not returned, or stored in what a function returns, by the name it is bound to, as what it holds is lent. One that reads a `var` that is not copied, or assigns a name of its function, is refused as one that holds something lent is. Where it is stored in a field or returned, it may leave out the type of a parameter and of its result, and takes each from the function type of the field or of the result: `Pricing(discount: fn(x) { x * 0.5 })`, and `fn chosen() -> fn(f32) -> f32 { fn(x) { x * 0.5 } }`. Where it is bound to a name, no function type decides its types, so it writes them: `f = fn(v) { v * 2.0 }` is refused, "a lambda bound to a name writes the type of each parameter and of its result, since no function type decides them". A lambda that is bound and leaves a type out, with its parameters inferred from the calls of it, is not built yet.

A lambda that holds something lent is refused where it is bound, stored, or returned: "a lambda cannot be bound to a name, as it is only passed to a call", and its two likes for a field and a result. A lambda that needs an effect is refused where it is stored or returned, as a function with a name is, since the type of a field states none.

Two lambdas where one type is needed are refused as two functions with names are, since each lambda is a type of its own: "which function this is depends on what the program does when it runs, and a function value must be known when the program is built".

A lambda that is passed to a call is inlined: it costs what its block costs, written where the function calls it.

Two keywords of later pages stop at a lambda. [`try`](try.md) inside a lambda returns from the lambda, and `break` does not reach a [`loop`](loop.md) outside it.

## The forms

| Written | Is |
| --- | --- |
| `f(a) fn(x) { e }` | A lambda that takes `x`, as the last argument of `f` |
| `f(a) fn(x, y) { e }` | A lambda that takes `x` and `y` |
| `f(a) fn(x: T) -> R { e }` | A lambda that writes the type of `x` and of its result |
| `fn(T) -> R` | The type of a function that takes a `T` and gives an `R` |
| `f(a) { e }` | A lambda that takes nothing |
| `f { e }` and `f fn(x) { e }` | A lambda as the only argument |
| `f(a, g: { e }) { e2 }` | Two lambdas: `g` by name, and the last after the call |

Next: [Function values](function-values.md)
