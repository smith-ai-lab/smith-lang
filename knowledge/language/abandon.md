---
type: specification
title: Abandon
description: The Abandon trait, whose function the scheduler calls on a value that a task owns when the task ends before it took the value, which a function that holds a use-exactly-once value across a wait needs its type to have.
status: planned
---

# Abandon

`Abandon` says what is done with a value that a task still owns when the task ends, so that a value used exactly once is never lost without a word.

```
trait Abandon[T] {
  fn abandon(t: take T)
}
```

The scheduler calls `abandon` once for each value without `Discard` that the task owns when it is [cancelled](cancellation.md) or [ends another way](task-ending.md), in the order the values were made, last first, under the handlers in force where the task was. A `Connection` and a `File` have an `Abandon` that closes them through the handler of `net` or `fs`, which is how they are closed today.

## A type that has one

A `Transaction` that is not committed is rolled back. Its `Abandon` says so.

```
struct Transaction {
  id: u64
}

without Copy for Transaction
without Discard for Transaction

fn begin(db: Db) -> Transaction <net>
fn debit(t: Transaction, amount: u64) <net>
fn commit(t: take Transaction) -> Result[(), DbError] <net>
fn rollback(t: take Transaction) -> Result[(), DbError] <net>

impl Abandon[Transaction] {
  fn abandon(t: take Transaction) <net> {
    _ = rollback(t)
  }
}

fn transfer(db: Db) -> Result[(), DbError] <net> {
  t = begin(db)
  debit(t, 100)
  commit(t)
}
```

If `transfer` is cancelled at `debit`, the scheduler calls `abandon` of `t`, and the transaction is rolled back, not left open.

## A function that holds a value while it may wait

A function that owns a value without `Discard` while it calls something that may wait needs the type of the value to have an `Abandon`, and carries the effects its `abandon` carries. Where the type has none, the function is refused, as the same function with no `impl` above is.

```
fn transfer(db: Db) -> Result[(), DbError] <net> {
  t = begin(db)
  debit(t, 100)
  commit(t)
}
```

This is refused at `debit`: "`transfer` holds a `Transaction` while it may wait, and a `Transaction` has no `Abandon`", with the fix "write an `impl Abandon[Transaction]`". With the `impl` and without `net` in the signature it is refused: "`transfer` holds a `Transaction` while it may wait, so it must carry `net`, which its `abandon` carries".

A function that holds the value where nothing may wait needs none, since the task cannot end there. A value that a function has given to another, or moved into a child, is not owned there, so `abandon` is called once, by the task that owns it.

## Where it may be written

An `impl Abandon[T]` lives in the module of `T` or of the trait, as every [`impl`](impl-placement.md) does, and there is one for a type. A type with `Copy` or `Discard` is never abandoned, so an `impl` for it is refused: "`Abandon` is for a value that cannot be dropped, and a `Key` can".

## The forms

| Written | Is |
| --- | --- |
| `impl Abandon[T] { fn abandon(t: take T) }` | What is done with a `T` that a task owns when it ends |
| a function that owns a `T` across a wait | Needs `Abandon[T]`, and the effects of its `abandon` |

Next: [A failed task](task-failures.md)
