---
type: specification
title: Changing a value in place
description: A mut parameter, which a function changes for its caller, means taking the value and giving the new one back, and is marked mut at the call.
status: partial
built:
  - "`mut` before a parameter's type, in a function that gives nothing, lowered to taking the value and giving it back, so a call that changes a value is built to the code of the call that takes it and assigns what it gives back"
  - "`f(mut x)` on a line of its own, of a `mut`, a field of one, or an item of a list held by one, and of several places that do not overlap"
  - "a call that leaves out `mut` where the parameter is `mut` refused with the call to write, `mut` where the parameter is not refused, and what is not a `mut`, a field, or an item refused"
  - "one place passed twice, or a place and a part of it, refused"
  - "a parameter marked `mut`, or a part of it without `Copy`, refused where it is stored, bound to another name, given to a `take` parameter, or returned, and any lambda that reads or assigns it refused"
  - "what `arith` knew of a value before a call that changes it not known after"
---

# Changing a value in place

A parameter marked `mut` is a value the function changes for its caller. The call is marked `mut` too, so a reader sees which names a line changes.

```
struct Cursor {
  at: Nat
  line: Nat
}

fn step(c: mut Cursor) {
  c.at = c.at + 1
}

fn main() {
  mut c = Cursor(at: 0, line: 1)
  step(mut c)
  step(mut c)
}
```

After the two calls, `c.at` is `2`.

## What it means

`mut` means exactly what taking the value and giving it back means. These two are the same function, called the same way, and they are built to the same code.

```
fn step(c: mut Cursor) {
  c.at = c.at + 1
}

fn step(c: take Cursor) -> Cursor {
  mut next = c
  next.at = next.at + 1
  next
}
```

The first is called `step(mut c)`, and the second `c = step(c)`. Every value has one owner, so no other name can see `c` while `step` runs, and the change is made where `c` is, with nothing copied. A fact about `c` that the caller knew before the call is not known after it, as for any name that is assigned.

## Where the change is seen

- **The call is marked.** `step(c)` is refused when the parameter is `mut`, with the fix "write `step(mut c)`", and `mut c` is refused where the parameter is not `mut`. A line that changes a name says so.
- **What is passed can be assigned.** It is a `mut`, or a field or an item of one, as `step(mut cs[0])` or `step(mut doc.cursor)`. `step(mut Cursor(at: 0, line: 1))` is refused, as nothing would see the change.
- **One place is passed once.** `swap(mut a, mut a)` is refused, and so is `swap(mut doc, mut doc.cursor)`, as both name the cursor. Two places that are not one are passed together.
- **The parameter is not kept.** A function cannot store a `mut` parameter, return it, or give it to a lambda that outlives the call. It changes the value and gives it back when it returns, and nothing else holds it.

## The forms

| Written | Means |
| --- | --- |
| `fn f(x: mut T)` | `f` changes `x` for its caller |
| `f(mut x)` | A call that changes `x` |
| `f(mut xs[i])`, `f(mut s.field)` | A call that changes an item or a field |
| `fn f(x: mut T)` and `fn f(x: take T) -> T` | The same function |

Next: [Values used exactly once](resources.md)
