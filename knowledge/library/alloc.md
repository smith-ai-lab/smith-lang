---
type: specification
title: Alloc
description: std.alloc, with arena, which gives the rest of a block an arena of its own that is freed whole when the block ends.
status: partial
built:
  - "`std.alloc` has `arena`, with which `with arena()` handles `alloc` for the rest of its block, as [The alloc effect and arenas](../language/arenas.md#with-arena) says"
---

# Alloc

The module `std.alloc` has one function, `arena`. The effect `alloc` is declared by no module and needs no import, as [The effects Smith provides](../language/provided-effects.md) says, but the function that handles it is imported.

```
fn arena[T](body: fn() -> T <alloc>) -> T
```

`arena` runs `body` with `alloc` handled by a fresh arena. What `body` allocates and does not hand out is freed whole when it ends, and its value is built in the memory outside, as is anything it assigns to a `var` outside it. A function that calls `arena` does not carry the `alloc` that `body` needs. It is written with [`with`](../language/with.md).

```
use std.alloc (arena)

fn squares_total(n: u64) -> u64 {
  with arena()
  sum_of_squares(n)
}
```

`squares_total` carries no effect, though `sum_of_squares` carries `alloc`, as [The alloc effect and arenas](../language/arenas.md#what-a-function-leaves-behind) shows. Its list of squares is freed when `squares_total` returns.

`arena` is a handler of `alloc`, which has no operation, so its body is `alloc(body: body)`: the effect's name called with the body and nothing else, as [Handlers](../language/handlers.md) says a handler is written.
