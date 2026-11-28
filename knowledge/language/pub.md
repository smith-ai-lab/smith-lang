---
type: specification
title: Public and private
description: Everything in a module is private unless marked pub, on an item, on a single field, or on a single operation of an effect, and what that lets another module do with a struct.
status: partial
built:
  - "`pub` before a `fn`, `struct`, `enum`, or `trait` of a module of a project, which is then the name another module lists in a `use`, and the functions of a `pub` trait, which are listed by their own names"
  - "a name that is not `pub` refused in a `use` with its own error, and a name written with no `use` refused as any name that is not there"
  - "`pub` before a field of a struct of a module of a project or of the library, and a field without it refused, at its place, in another module when it is read, assigned, or built by a call of the struct"
  - "a field, `pub` or not, assigned only in the module that defines its struct, and refused in any other at the assignment, naming the field and that module, whether it is written `v.f = e`, `v.f.g = e`, `v.f[i] = e`, or `g(mut v.f)`"
  - "`pub` before an operation of an effect of a module of a project or of the library, and an operation without it private to its module, which no other module imports, calls, or handles"
  - "`pub` before a `fn` in the file that is built, which `smith build` accepts and which lets `project.smith` list its `main`"
  - "not `pub` before an `effect`, or before anything but a `fn` in the file that is built"
---

# Public and private

Everything in a module is private unless it is marked `pub`. A private definition can be used only inside its own module. A `pub` definition can be used by other modules, which import it as [Importing with use](use.md) describes.

```
pub fn commit(t: take Transaction) -> Result[(), DbError] <net>

fn next_id(db: Db) -> u64 <net>
```

Written in `db/transaction.smith`, `commit` can be imported by any module and `next_id` cannot. Code in the same file may use both.

```
pub fn begin(db: Db) -> Transaction <net> {
  Transaction(id: next_id(db))
}
```

## A private definition used from another module

Another module that names `next_id` is refused, since `next_id` is private.

```
use db.transaction (next_id)
use std.bits (u64)

fn fresh(db: Db) -> u64 <net> {
  next_id(db)
}
```

## What `pub` is written on

`pub` is written before the item it opens up: `pub fn`, `pub struct`, and `pub effect`, as in [Declaring an effect](effect-declarations.md).

```
pub struct Rect {
  pub w: f32
  pub h: f32
}
```

`pub` is also written on a single field of a struct, as `w` and `h` are above. A field with no `pub` is private, whatever the struct is. An operation of an effect is the same: it is private unless `pub` is written before it, whatever the effect is, as [Declaring an effect](effect-declarations.md#an-operation-is-private-unless-it-is-marked-pub) says.

```
pub struct Transaction {
  id: u64
}
```

`Transaction` can be imported, and its field `id` is private.

## Building and changing a struct from another module

A struct can be built or taken apart only by code that can see all of its fields. A field is assigned only in the module that defines its struct, `pub` or not, so no other module can break what that module keeps true of its values.

Every field of `Rect` is `pub`, so any module can build one and read its fields. Another module that wants a wider `Rect` builds one.

```
fn wider(r: Rect) -> Rect {
  Rect(w: r.w * 2.0, h: r.h)
}
```

Assigning a field of `Rect` in another module is refused, at the assignment, with the name of the field and of the module that defines `Rect`. A field of a field, and a field passed [`mut`](mut-parameters.md), are refused the same way.

```
fn widened(r: Rect) -> Rect {
  mut wide = r
  wide.w = r.w * 2.0
  wide
}
```

`Transaction` has a private field, so another module can hold a `Transaction` and pass it on, but cannot forge one or dispose of one.

```
fn transfer(db: Db) -> Result[(), DbError] <net> {
  t = begin(db)
  commit(take t)
}
```

`transfer` holds a `Transaction` that `begin` gave it, and passes it to `commit`. Building one is refused, since `id` is private.

```
fn forged() -> Transaction {
  Transaction(id: 7)
}
```

Assigning its field is refused for the same reason.

```
fn renumbered(db: Db) -> Transaction <net> {
  mut t = begin(db)
  t.id = 8
  t
}
```

This is what makes [`without`](without.md) and [proof fields](proof-fields.md) trustworthy. A `Transaction` is a value that must be taken exactly once, as [Values used exactly once](resources.md) describes, and a module that does not own it can neither make one nor dispose of one.

## The forms

| Written | Means |
| --- | --- |
| `fn f` | Private: used only inside its module |
| `pub fn f` | Public: any module may import it |
| `pub struct S` | The struct is public, and each field is private unless it has `pub` |
| `pub w: f32` | A public field: code outside the module can read it, and only its own module assigns it |
| `w: f32` | A private field: code outside the module cannot read it, or build or take apart the struct |

Next: [Importing with use](use.md)
