---
type: specification
title: Test
description: std.test, with expect, the check of a test, and Failed and Sides, which say where a check failed and what each side of a comparison was.
status: partial
built:
  - "`std.test` with `expect`, `Failed`, and `Sides`, as this page says"
  - "not `expect` declared as a function over code: it is declared `fn expect(check: fn() -> bool) -> Result[(), Failed]`, and the compiler writes out each call with its check in braces after it, which is the only call it accepts"
---

# Test

`std.test` holds `expect`, the check of a [test](../language/tests.md), and what a check that fails gives.

```
use std.test (Failed, expect)

fn test_one_and_one() -> Result[(), Failed] {
  expect { 1 + 1 == 2 }
}
```

`expect` gives `.ok(())` when its check is `true`, and otherwise a `Failed`. It is given its check in braces after it, as one expression, because it reads the code of the check, as [Functions over code](../language/code.md) says. `expect(holds)` with a check held in a name is refused at the call.

```
pub struct Failed {
  pub path: str
  pub line: Nat
  pub column: Nat
  pub check: str
  pub sides: Option[Sides]
}

pub struct Sides {
  pub left: str
  pub right: str
}
```

A `Failed` holds the file of the `expect`, its line and column, and the check as it is written. When the check is a comparison, with `==`, `!=`, `<`, `<=`, `>`, or `>=`, `sides` holds what each side was, as `repr` writes it, and each side is worked out once. Any other check, `not (a == b)` among them, has no sides. A test that lists no `alloc` can fail: the failure asks for its memory under a handler of its own.
