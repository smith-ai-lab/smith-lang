---
type: specification
title: Statements
description: A statement is a type built from what data is built from, a proof is a value of it, and a boolean expression is the statement that it is true, so there is no second language for proofs.
status: partial
built:
  - "a proof function, a function whose result is a statement, whose body is steps the kernel checks: `same`; `arith(p, q)`, which gives an equality of whole numbers that is the sum of the equalities `p` and `q` say, each times a number, with a product of unknowns taken as one unknown of its own; a name given by `unfold(f(x))`, which says that `f(x)` equals the body of `f` with `x` put in, where each condition the body tests is decided by the cases the proof is in; a name given by `replace(eq, p)`, which is the statement `p` proves with the right side of `eq`, a proof of `x == y`, put where `x` is written in it, in every place, and is refused when `x` is not written in it; and an `if` that splits the proof into cases, each of which knows which it is in; a name given by a call of a proof function, which is a proof of its statement for the arguments, so that a proof that calls itself on a smaller value, with its `decreases` proven as for any function, is an induction: `gauss` and the sum of a double; `arith` takes the size of `push(xs, x)` as one more than that of `xs`, and the size of `xs[a..b]` of a slice as `b - a` where the proof knows `a <= b <= xs.size`, from its `requires` or the case it is in; a statement as a parameter or a field is not built"
---

# Statements

A statement is a type, and a proof of it is a value of that type. A boolean expression is the statement that it is true, which is the kind the pages before this one use.

```
fn first(xs: List[Byte], p: xs.size > 0) -> Byte { xs[0] }
```

`xs.size > 0` is a statement, and `p` is a proof of it. A call passes one, as `first(xs, q)` does with a proof `q`. Most statements a program needs are of this kind, and [`arith`](arith.md) finds their proofs, so a parameter that has the default `arith` or a `requires` leaves the proof out of the call.

## A proof parameter may have a default

A proof parameter has a default as any parameter may, written after its type with `=`, and the one default a proof has is `arith`.

```
fn first(xs: List[Byte], p: xs.size > 0 = arith) -> Byte { xs[0] }
```

`first([7, 8])` is `7`, since `arith` proves `[7, 8].size > 0` where the call is written. `first(xs, q)` passes a proof `q` of `xs.size > 0` and leaves `arith` out. A call that leaves the proof out where `arith` cannot prove it is refused with the goal for the call's arguments: "cannot prove that `ys.size > 0`, which `first` requires of its arguments". A proof parameter with no default is always passed. The default is written where the parameter is declared, so no proof is passed that the declaration does not show.

`requires e` is the same as a proof parameter with the default `arith` that no call writes, as [Contracts](contracts.md#a-requires-is-a-proof-parameter) says.

## What a statement may compare

A statement compares two values of any type that has [`Ord`](comparisons.md), and not only numbers. So `earlier <= later` is a statement for two `Instant`s, and a `requires` may state it.

```
fn wait_after(earlier: Instant, later: Instant) -> Duration
  requires earlier <= later
{
  between(earlier, later)
}
```

A caller proves it as it proves any statement: with an `if`, with a [`check`](check.md), or with a fact it already has.

## Why a statement is a type

A statement about one value is a boolean: `i < xs.size` is true or it is not. A statement about every value of some kind is not one. "Every element of `xs` is below `n`" cannot be worked out once and held as `true`, and "for every two lists, `append(a, b)` has the size of both" cannot be worked out at all.

So a statement is a type, and what is built from statements is built as data is. There is no second language for proofs: the constructs are the ones a program already has.

| In words | Is the type | A proof of it is |
| --- | --- | --- |
| `e` is true | The boolean expression `e` | Found by `arith`, or given by a [`check`](check.md) |
| P and Q | A tuple or a struct of the two | A proof of each, as `(p, q)` |
| P or Q | An enum with a case for each | One case, holding its proof |
| P gives Q | A function type from P to Q | A function that makes a proof of Q from one of P |
| For every `x`, P | A function type whose result names its parameter | A function that takes `x` and gives the proof for it |
| There is an `x` with P | A struct that holds `x` and the proof | The value and its proof, as [`Cursor`](proof-fields.md) is |
| Never | An enum with no case | Nothing: there is no value of it |

## A statement about every one

A function type may name its parameters, so that the type of what it gives can speak of them. That is how "for every" is written.

```
struct Sorted {
  xs: List[Byte]
  ordered: fn(i: Nat, j: Nat, p: i < j and j < xs.size) -> xs[i] <= xs[j]
}
```

A `Sorted` is a list together with a proof that it is in order. The proof is a function: given any two places `i` and `j`, and a proof that `i` comes before `j` inside the list, it gives a proof that the element at `i` is not above the one at `j`. A function that takes a `Sorted` knows its list is in order, as a function that takes a [`Cursor`](proof-fields.md) knows its place is not past the end.

## A proof by induction

A proof about every number, or every list, is a function that calls itself, and it is held to the rule every function is held to: it [ends](termination.md). The call on the smaller value is the step of the induction, and the check that the calls end is what makes it one.

```
fn size_of_append(a: List[Byte], b: List[Byte]) -> append(a, b).size == a.size + b.size
  decreases b.size
```

`append` adds at the end, so the call on the smaller value is the one on `b` without its last element, and the measure is `b.size`. The size of that piece is what a [slice](ranges.md) and a `push` give, as [What is trusted](trust.md#what-the-kernel-does-by-itself) says. `size_of_append` is declared here without its body. Its result is a statement that names its parameters, and a call of it gives the caller that statement for the two lists it passes.

## The steps of a proof

The body of a proof is made of a few steps, each a function whose result is a statement. The kernel knows these and nothing else, and it [takes no step of its own](trust.md#what-the-kernel-does-by-itself).

| Step | Gives a proof that |
| --- | --- |
| `same` | `a == a`, and any statement with nothing unknown in it, which the kernel runs |
| `unfold(f(x))` | `f(x)` equals the body of `f` with `x` put in |
| `replace(eq, p)` | The statement `p` proves, with the right side of `eq` put where its left side stood |
| `arith(p, q)` | A fact about numbers that follows from the proofs it is handed |

Nothing else is new. An `if` or a `match` splits a proof into cases, and each branch may use the case it is in. A call of the function by itself, on a smaller value, is the step of an induction.

`replace(eq, p)` takes two proofs named before it, and gives a proof of the statement `p` proves, with the right side of the equality `eq` proves put in every place its left side is written. With `a(n)` defined as `b(n)` and `b(n)` as `n + 1`:

```
opened = unfold(b(n))          // b(n) == n + 1
chain = unfold(a(n))           // a(n) == b(n)
now = replace(opened, chain)   // a(n) == n + 1
```

It is refused, naming both, when the left side is not written in the statement of `p`. It is a step like the others: it gives a proof and never changes the statement the proof owes.

```
fn sum_to(n: Nat) -> Nat {
  if n == 0 { 0 } else { n + sum_to(n - 1) }
}

fn gauss(n: Nat) -> 2 * sum_to(n) == n * (n + 1)
  decreases n
{
  if n == 0 {
    same
  } else {
    earlier = gauss(n - 1)
    opened = unfold(sum_to(n))
    arith(earlier, opened)
  }
}
```

When `n` is `0` both sides have nothing unknown in them, so `same` proves the statement by running them. Otherwise `earlier` is the statement for `n - 1`, `opened` says that `sum_to(n)` is `n + sum_to(n - 1)`, and `arith` gets the statement for `n` from the two.

Whatever is cleverer than a step is a [function over code](code.md) that writes steps, such as one that unfolds and replaces until nothing changes. It is outside the kernel, so what it writes is checked like any proof.

## The kinds stay apart in use, not in kind

- A statement that is a boolean expression can be worked out, so `arith` may decide it and a `check` may test it when the program runs. This is the everyday kind, and nothing about it changes.
- A statement that is a function type cannot be worked out. It is proven once, by a function someone writes, and the kernel checks that function, as [What is trusted](trust.md) says. The compiler does not look for it.

A proof is written once. The compiler checks proofs and never searches for one, so a program takes no longer to build because it is proven.

## Two values that are the same

`a == b` is the statement that `a` and `b` are the same value. `==` means nothing looser, as a program does not write `Eq`, which [Comparisons](comparisons.md) says. So a proof of `a == b` lets one stand where the other stood, in any statement and in any code.

## The forms

| Written | Is |
| --- | --- |
| `p: a < 10` | A proof that the boolean expression is true |
| `p: a < 10 = arith` | The same, which a call may leave out where `arith` proves it |
| `(p, q)` | A proof of both |
| `fn(p: P) -> Q` | A proof that P gives Q |
| `fn(i: Nat, p: i < n) -> xs[i] < m` | A proof for every `i` below `n` |
| A struct with a value and a proof | A proof that there is such a value |
| `same`, `unfold(f(x))`, `replace(eq, p)`, `arith(p, q)` | The steps a proof is made of |

Next: [hole](hole.md)
