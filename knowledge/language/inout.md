---
type: specification
title: Changing a value in place
description: An inout parameter, which a function changes for its caller, means taking the value and giving the new one back, and is marked inout at the call.
status: planned
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
