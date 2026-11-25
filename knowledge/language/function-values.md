---
type: specification
title: Function values
description: A function with a name used as a value, which can be passed, stored, and returned.
status: partial
built:
  - "a function with a name passed to a call"
  - "a function with a name bound to a name, stored in a field, and returned, each known when the program is built, and refused where which function it is depends on what the program does"
  - "a lambda that reads and assigns no name of its function, bound, stored, and returned in the same way, as [Lambdas](lambdas.md#where-a-lambda-may-go) says"
---

# Function values

A function with a name is a value. Its name is passed wherever a function type is expected.

```
fn half(x: f32) -> f32 { x * 0.5 }

fn twice(x: f32, f: fn(f32) -> f32) -> f32 { f(f(x)) }

fn quarter(x: f32) -> f32 { twice(x, half) }
```

`half` is written as it is named, with no parentheses, and nothing is called. `twice` receives it as `f`, so `quarter(8.0)` is `2.0`.

## What a name is worth as a value

A function's type is the types of its parameters and of its result, written as in [Lambdas](lambdas.md). The type of `half` is `fn(f32) -> f32`, and a name is accepted where that type is expected and nowhere else.

```
fn positive(x: f32) -> bool { x > 0.0 }

fn wrong(x: f32) -> f32 { twice(x, positive) }
```

This is refused: "this gives a `fn(f32) -> bool`, where a `fn(f32) -> f32` is needed". A function that takes two floats is refused the same way, as `fn(f32, f32) -> f32`.

Writing the call instead of the name passes what the call gives. `twice(x, half(1.0))` is refused: "this gives a `f32`, where a `fn(f32) -> f32` is needed".

## A function that needs more than it is given

A function value is called with the arguments of its type and no others, so the function must take exactly those.

```
fn off(x: f32, rate: f32) -> f32 { x - rate }

fn price(x: f32, rate: f32, discount: fn(f32, f32) -> f32) -> f32 {
  discount(x, rate)
}
```

`price(10.0, 3.0, off)` is `7.0`. The function that receives `off` is the one that holds `rate` and passes it, since the value `off` holds nothing of its own. Nothing is captured: a function sees its parameters and what it defines, and nothing of the function it is passed from.

```
fn rated(x: f32) -> f32 { x - rate }
```

This is refused: "there is no name `rate` defined before this in `rated`". A [lambda](lambdas.md) is the way to use the names around a call, and a function with a name is the way to pass something that stands alone.

When a stored function needs data to work with, the data is stored beside it and passed to it, as `rate` is above.

## Defaults

A default belongs to a function's name and not to its type, so a function passed as a value takes every argument.

```
fn scale(x: f32, by: f32 = 2.0) -> f32 { x * by }
```

The type of `scale` is `fn(f32, f32) -> f32`, so `twice(8.0, scale)` is refused: "this gives a `fn(f32, f32) -> f32`, where a `fn(f32) -> f32` is needed". A lambda gives the default, since the call inside it is an ordinary call.

```
fn quarter_scale(x: f32) -> f32 { twice(x) fn(v) { scale(v) } }
```

## Binding, storing, and returning a function

Whatever gives a function can be called, and a call is written after a name, a field, or another call alike. A function with a name is a value, so it can be bound to a name and called by that name.

```
fn eight() -> f32 {
  f = half
  f(8.0)
}
```

`eight()` is `4.0`.

A function with a name can also be stored and returned, because it holds nothing that belongs to another function, and so can a [lambda that holds nothing lent](lambdas.md#where-a-lambda-may-go). A function stored in a field is called by reading the field and calling what it gives. Smith has no methods, so `a.b(c)` always means that.

```
struct Pricing {
  discount: fn(f32) -> f32
}

fn sale_price(x: f32) -> f32 {
  sale = Pricing(discount: half)
  sale.discount(x)
}
```

`sale_price(8.0)` is `4.0`.

A function that returns a function says so in its return type, which is a function type, and what it returns is called the same way.

```
fn chosen() -> fn(f32) -> f32 { half }

fn chosen_half(x: f32) -> f32 { chosen()(x) }
```

`chosen_half(8.0)` is `4.0`. The result may also be bound first, as `f = chosen()` and then `f(x)`.

## Which function it is is known when the program is built

A value of a function type is one function, and which one is known when the program is built, so a call of it is a call of that function. A struct with a field of a function type is a type for each function it holds, as a struct with a type parameter is a type for each type it holds, and the field takes no bytes. The function is never written: `Pricing` is the type of every `Pricing`, and a function that takes one takes whichever it is given, as a function with a type parameter does. It is built once for each function its callers' values hold.

```
fn double(x: f32) -> f32 { x * 2.0 }

fn priced(p: Pricing, x: f32) -> f32 { p.discount(x) }

fn both() -> (f32, f32) {
  (priced(Pricing(discount: half), 8.0), priced(Pricing(discount: double), 8.0))
}
```

`both()` is `(4.0, 16.0)`. A struct that holds a `Pricing` holds the function it holds in turn. A function that returns a function, or a `Pricing`, gives what its body ends with: a function with a name, a construction of a `Pricing`, a name bound to one of those, a call of a function that gives one, or a parameter, whose function is then the argument's at each call, as in `fn wrap(f: fn(f32) -> f32) -> Pricing { Pricing(discount: f) }`, where `wrap(half).discount(3.0)` is `1.5`. A name bound to a function is that function, and whatever gives a function, a call or a field of any value, is called by writing the arguments after it: `chosen()(x)`, `same(p).discount(x)`, and `shop.pricing.discount(x)`.

Two values that hold different functions where one type is needed are refused, since which function it is would depend on what the program does when it runs. These are the arms of an `if` or a `match`, two elements of a list, a `var` given one function and then another, and a function that returns one of two functions by a branch.

```
fn pick(wide: bool) -> Pricing {
  if wide { Pricing(discount: double) } else { Pricing(discount: half) }
}
```

This is refused at the first `Pricing(discount: double)`: "which function this is depends on what the program does when it runs, and a function value must be known when the program is built". A choice between functions is made where the function is called, by calling each one.

A function type states no effect, so a function that needs one is refused where it goes into a field or is given as a result, since whoever calls what is stored is checked against the type alone. `Pricing(discount: loud)` for a `loud` that needs `console` is refused: "`loud` needs the `console` effect, and the type of the field `discount` states none, so a function that needs one is not stored there". A function passed to a call is checked at the call, as [Effects](effects.md) says.

## The forms

| Written | Is |
| --- | --- |
| `half` | The function named `half`, as a value of type `fn(f32) -> f32` |
| `half(1.0)` | A call, which gives what `half` gives, not a function |
| `twice(x, half)` | A function with a name passed to a call |
| `f = half` then `f(x)` | A function bound to a name, and called by it |
| `discount: half` | A function with a name stored in a field |
| `sale.discount(x)` | The function in a field, called |
| `-> fn(f32) -> f32` | A return type that is a function |
| `chosen()(x)` | The function a call returns, called |
| `fn(v) { scale(v) }` | A lambda, the way to pass a function that has defaults |

Next: [`with`](with.md)
