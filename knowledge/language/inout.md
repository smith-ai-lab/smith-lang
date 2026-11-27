---
type: specification
title: Changing a value in place
description: An inout parameter, which a function changes for its caller, means taking the value and giving the new one back, and is marked inout at the call.
status: partial
built:
  - "`inout` before a parameter's type, in a function that gives nothing, lowered to taking the value and giving it back, so a call that changes a value is built to the code of the call that takes it and assigns what it gives back"
  - "`f(inout x)` on a line of its own, of a `var`, a field of one, or an item of a list held by one, and of several places that do not overlap"
  - "a call that leaves out `inout` where the parameter is `inout` refused with the call to write, `inout` where the parameter is not refused, and what is not a `var`, a field, or an item refused"
  - "one place passed twice, or a place and a part of it, refused"
  - "a parameter marked `inout`, or a part of it without `Copy`, refused where it is stored, bound to another name, given to a `take` parameter, or returned, and any lambda that reads or assigns it refused"
  - "what `arith` knew of a value before a call that changes it not known after"
---

# Changing a value in place

A parameter marked `inout` is a value the function changes for its caller. The call is marked `inout` too, so a reader sees which names a line changes.

```
struct Cursor {
  at: Nat
  line: Nat
}

fn step(c: inout Cursor) {
  c.at = c.at + 1
}

fn main() {
  var c = Cursor(at: 0, line: 1)
  step(inout c)
  step(inout c)
}
```

After the two calls, `c.at` is `2`.

## What it means

`inout` means exactly what taking the value and giving it back means. These two are the same function, called the same way, and they are built to the same code.

```
fn step(c: inout Cursor) {
  c.at = c.at + 1
}

fn step(c: take Cursor) -> Cursor {
  var next = c
  next.at = next.at + 1
  next
}
```

The first is called `step(inout c)`, and the second `c = step(c)`. Every value has one owner, so no other name can see `c` while `step` runs, and the change is made where `c` is, with nothing copied. A fact about `c` that the caller knew before the call is not known after it, as for any name that is assigned.

## Where the change is seen

- **The call is marked.** `step(c)` is refused when the parameter is `inout`, with the fix "write `step(inout c)`", and `inout c` is refused where the parameter is not `inout`. A line that changes a name says so.
- **What is passed can be assigned.** It is a `var`, or a field or an item of one, as `step(inout cs[0])` or `step(inout doc.cursor)`. `step(inout Cursor(at: 0, line: 1))` is refused, as nothing would see the change.
- **One place is passed once.** `swap(inout a, inout a)` is refused, and so is `swap(inout doc, inout doc.cursor)`, as both name the cursor. Two places that are not one are passed together.
- **The parameter is not kept.** A function cannot store an `inout` parameter, return it, or give it to a lambda that outlives the call. It changes the value and gives it back when it returns, and nothing else holds it.

## The forms

| Written | Means |
| --- | --- |
| `fn f(x: inout T)` | `f` changes `x` for its caller |
| `f(inout x)` | A call that changes `x` |
| `f(inout xs[i])`, `f(inout s.field)` | A call that changes an item or a field |
| `fn f(x: inout T)` and `fn f(x: take T) -> T` | The same function |

Next: [Values used exactly once](resources.md)
