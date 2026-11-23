---
type: specification
title: Memory of a task
description: Each child task gets its own arena, its result is built in its parent's, and a value lives as long as the task that created it.
status: partial
built:
  - "each child has an arena of its own, which is freed whole when the child ends, and its result is built in its parent's, so a program that spawns a child that builds a list and gives back a number asks the system for the same memory however many children it spawns"
  - "a task has memory of its own, in pieces of a pool that start at 64 bytes and double, so a task that waits with nothing built holds its frame and 64 bytes and `with arena()` in a task frees its block whole at its end even when the block waits, and a joined task gives its frame back to the pool when it is joined, and its stack too when its result holds no address, so 10,000 children one after another ask the system for what one does"
  - "a result that holds an address is not copied at `join`: the child's stack becomes the parent's, and is freed when the memory the parent built in its place would have been, so 10,000 children that each give a list, joined one after another, ask the system for what one does, and a list a function gives up to its caller is read there as it was built"
  - "not built: a child that is never joined giving its memory back, and a task for each connection of a server, which waits for the system"
---

# Memory of a task

Each task has an [arena](arenas.md) of its own.

- Each child task gets its own arena, and its result is built in the parent's. In `fetch_both` of [Tasks](tasks.md), the page that `fetch(a)` gives is built in the arena of `fetch_both`.
- A value lives as long as the task that created it, and tasks pass plain data to each other.
- A server is one task per connection with one arena per request. `serve` of [What a spawned task may use](task-sharing.md) starts a task for each `conn`, and `handle` opens `with arena()` for each request, which frees it whole when the request is answered.

```
fn read_request(conn: Conn) -> Request ! net, alloc
fn respond(conn: Conn, request: Request) ! net, alloc

fn handle(conn: Conn) ! net, alloc, diverge {
  loop {
    with arena()
    request = read_request(conn)
    respond(conn, request)
  }
}
```

## The forms

| What | Has its own arena |
| --- | --- |
| A child task | Yes, and its result is built in the parent's |
| A request of a server | Yes, from `with arena()` |

Next: [Handlers and tasks](task-handlers.md)
