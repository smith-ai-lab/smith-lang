---
type: specification
title: The network
description: The net effect of std.net, for connections and datagrams, each a value used exactly once, with a write that sends everything and a read that gives what has come.
status: partial
built:
  - "`std.net` declares `net`, with the operations, `Address`, and `NetError` as the page shows them, and `Listener`, `Connection`, and `Socket`, each a struct with one private field, the system's handle, and neither `Copy` nor `Discard`"
  - "`listen`, `connect`, and `bind`, written for `with`, which lend what they open to the rest of the block and end it afterwards on every path"
  - "on Linux and macOS, on x86-64 and arm64, the system answers every operation, and a call that waits blocks, except in a group of tasks on Linux, which the next line says; a host is `localhost`, a number written with dots, or a number written with colons, and any other name is `failed` with the one message that looking up a name is not built"
  - "`failed` holds the call's name and the system's error number, as `connect: error 111` on Linux and `connect: error 61` on macOS; `refused`, `in_use`, and `closed` are the system's refusal of a connection, a port that is held, and a write to a connection the other side closed"
  - "a handler of `net` answers an operation that gives a `Result` with an error, with no system call, on every target, and `stop`, `close`, and `unbind` with nothing, and a handler that gives a `Listener`, a `Connection`, or a `Socket` makes it with `listener`, `connection`, or `socket` and reads its number with `handle`, as [Making a value in a handler](#making-a-value-in-a-handler) says"
  - "the system's handler answers every operation given a value it did not open with `failed` and makes no system call with it, on Linux, macOS, and Windows"
  - "on Linux and on macOS a call of `read`, `write`, or `accept` in a task of a group suspends the task, which waits in `epoll` on Linux and in `kqueue` on macOS for the connection while other tasks run, and a call with no group blocks, as it did"
  - "a listener from `listen_on` is made to hold up to 4096 connections that nothing has accepted, or as many as the system allows when that is fewer, so a thousand connections that arrive before the server takes the first are all made at once and none waits for the system to ask again"
  - "`release`, a function of the trait `Release` that has an `impl` for `Listener`, `Connection`, and `Socket`, taking its value, ending it with no system call, and giving back the number of the handle it held"
  - "not built: a handler that keeps state and is under `listen` of `std.http`, which is refused as not built yet, as [Handlers and tasks](../language/task-handlers.md) says, and a handler that assigns a name under `stdio` of `std.mcp`, which stops the compiler"
  - "on Windows, on x86-64 and arm64, the system answers every operation through `ws2_32.dll`, which only a program that carries `net` asks the loader for, and every call waits where it is, as it did on Linux before tasks, so a task that waits for a connection is not built there and waits for an issue of its own, completion ports; `failed` holds Winsock's number, as `connect: error 10061`, and `refused`, `in_use`, and `closed` are the same refusals by their numbers, `in_use` being 10048 or 10013, which Windows gives for a port that a socket holds for itself alone, as a listener does there; the clock is not built there"
  - "not built: a name looked up, which on Windows will call `GetAddrInfoW` of `ws2_32.dll`, as it will call the system's own lookup on macOS, beside the issue for looking up a name"
---

# The network

The module `std.net` declares the `net` [effect](../language/effect-declarations.md), by which a program talks over the network: connections, which carry bytes in order both ways, and datagrams, which are sent one at a time. Most programs never call it. A server asks for requests through [`http`](http.md) and a client asks another server through [`fetch`](fetch.md), and the handlers of those are what use `net`.

```
use std.net (NetError, connect, net, read, write)

fn ask(host: str, question: str) -> Result[List[Byte], NetError] ! net, alloc {
  with c = connect(host, 7)
  try write(c, question.bytes)
  read(c, 4096)
}
```

`ask` opens a connection to port 7 of the host, sends the text, and gives the bytes that come back. The connection is closed when the block ends, on every path.

A call that has to wait, as `read` does for bytes that have not come, reads as any call does. Under the system's handler it suspends its task, as [Async](../language/async.md) says, and other tasks run.

## The effect

```
effect net {
  fn listen_on(host: str, port: 0..65536) -> Result[Listener, NetError]
  fn accept(l: Listener) -> Result[Connection, NetError]
  fn stop(l: take Listener)

  fn connect_to(host: str, port: 0..65536) -> Result[Connection, NetError]
  fn read(c: Connection, most: Nat) -> Result[List[Byte], NetError] ! alloc
  fn write(c: Connection, bytes: Slice[Byte]) -> Result[(), NetError]
  fn close(c: take Connection)

  fn bind_to(host: str, port: 0..65536) -> Result[Socket, NetError]
  fn send(s: Socket, to: Address, bytes: Slice[Byte]) -> Result[(), NetError]
  fn receive(s: Socket, most: Nat) -> Result[(Address, List[Byte]), NetError] ! alloc
  fn unbind(s: take Socket)
}

struct Address {
  host: str
  port: 0..65536
}

enum NetError {
  unknown_host: str
  refused
  in_use
  closed
  failed: str
}
```

| Operation | Does |
| --- | --- |
| `listen_on` | Begins to take connections at a port of this machine. Port 0 asks the system for one that is free. The system holds up to 4096 connections that `accept` has not yet taken, or as many as it allows when that is fewer, so a burst of connections arrives without a wait |
| `accept` | Waits for the next connection to a listener, and gives it |
| `connect_to` | Opens a connection to a port of a host |
| `read` | Waits for bytes and gives those that have come, at most `most`. No bytes means the other side has finished sending |
| `write` | Sends every byte, or fails |
| `bind_to` | Takes a port of this machine for datagrams |
| `send` | Sends the bytes as one datagram to an address |
| `receive` | Waits for the next datagram and gives it with where it came from. One longer than `most` is cut to `most` |
| `stop`, `close`, `unbind` | End a listener, a connection, or a socket, which cannot be used again |

- **A write sends everything.** There is no count of how many bytes went: `write` gives `.ok` when all of them were handed to the system, and `.err` otherwise, so no program has to send the rest.
- **A `Listener`, a `Connection`, and a `Socket` are used exactly once,** as [Values used exactly once](../language/resources.md) says. One that is not ended, is ended twice, or is used after it is ended does not compile.
- **A host is a name or a number,** as `example.com`, `192.0.2.7`, or `::1`. Finding the number of a name is the handler's work, and `unknown_host` is a name it could not find.
- **Every failure is a value.** `refused` is a host that takes no connection at the port, `in_use` is a port another program holds, and `closed` is a connection the other side ended while this side wrote.

## With a block

`listen`, `connect`, and `bind` are written for [`with`](../language/with.md), as `open` of [`std.fs`](fs.md) is: each lends what it opens to the rest of the block and ends it afterwards, on every path. Most code writes these and none of `stop`, `close`, and `unbind`.

```
use std.list (freeze)
use std.net (Connection, NetError, accept, close, listen, net, read, write)

fn turn(c: Connection) -> Result[(), NetError] ! net, alloc {
  bytes = try read(c, 4096)
  write(c, freeze(bytes))
}

fn echo_once(port: 0..65536) -> Result[(), NetError] ! net, alloc {
  with l = listen("::", port)
  c = try accept(l)
  result = turn(c)
  close(c)
  result
}
```

`echo_once` takes one connection, sends back what it reads, and ends. The connection is closed on every path, as a `try` that left before `close(c)` would drop it. `write` takes a `Slice`, which `freeze` makes of the bytes `read` gives.

## Ending a value in a handler

A handler's lambda for `stop`, `close`, or `unbind` takes the value, as [Handlers](../language/handlers.md) says, and ends it with `release`, which makes no system call and gives back the number of the handle the value held, so a handler knows which one it was given.

```
use std.net (net, release)

fn ended[T](body: fn() -> T ! net) -> T {
  net(
    listen_on: fn(host, port) { .err(.refused) },
    accept: fn(l) { .err(.refused) },
    stop: fn(l) { n = release(l) },
    connect_to: fn(host, port) { .err(.refused) },
    read: fn(c, most) { .err(.closed) },
    write: fn(c, bytes) { .err(.closed) },
    close: fn(c) { n = release(c) },
    bind_to: fn(host, port) { .err(.refused) },
    send: fn(s, to, bytes) { .err(.closed) },
    receive: fn(s, most) { .err(.closed) },
    unbind: fn(s) { n = release(s) },
    body: body,
  )
}
```

The system's handler never calls them. A value that the system's handler opened and that is released leaks its handle and does not close it, so only a handler that a test writes calls them.

## Making a value in a handler

A handler that plays the other side of a connection gives the program a `Connection`, a `Listener`, or a `Socket`. It makes each from a number of its own choosing with `connection`, `listener`, and `socket`, and `handle` gives the number back from the value it is lent, so it knows which one a call is about.

```
use std.net (connection, handle, listener, net, release)

fn one_client[T](body: fn() -> T ! net) -> T {
  net(
    listen_on: fn(host, port) { .ok(listener(1)) },
    accept: fn(l) { .ok(connection(handle(l) + 1)) },
    stop: fn(l) { n = release(l) },
    connect_to: fn(host, port) { .err(.refused) },
    read: fn(c, most) { .ok([]) },
    write: fn(c, bytes) { .ok(()) },
    close: fn(c) { n = release(c) },
    bind_to: fn(host, port) { .err(.refused) },
    send: fn(s, to, bytes) { .err(.closed) },
    receive: fn(s, most) { .err(.closed) },
    unbind: fn(s) { n = release(s) },
    body: body,
  )
}
```

Under `one_client`, `accept` gives a connection whose handle is 2, and `read` gives no bytes, which is a connection the other side has finished with. A handler may keep what `write` is given in a `var`, as [Handlers](../language/handlers.md#a-handler-that-keeps-state) says.

- **A number the system did not open reaches nothing.** The system's handler answers `listen`, `accept`, `read`, `write`, `send`, and `receive` with `failed`, whose message says the handle is not one the system opened, when it is given a value that `listener`, `connection`, or `socket` made, and makes no system call with it. `stop`, `close`, and `unbind` end such a value and close nothing. A program that makes `connection(1)` and writes to it writes nothing to standard output, under any handler.
- **A value the system opened is not made again.** `handle` gives its number, and `connection` of that number is a value the system did not open.

## What answers it

- A native executable answers `net` with the system's own calls: the kernel's on Linux, `libSystem`'s on macOS, and `ws2_32.dll`'s on Windows, which a program asks the loader for only when it carries `net`. A call that waits suspends its task on Linux, with `epoll`, and on macOS, with `kqueue`, and waits where it is on Windows, which a task waiting for the system there will change, as an issue of its own.
- A server that takes bursts of connections on a Mac needs what the system's defaults do not give. macOS holds 128 connections that nothing has accepted and gives a process 256 descriptors, so a server of a thousand connections at once raises `kern.ipc.somaxconn`, with `sudo sysctl -w kern.ipc.somaxconn=4096`, and `ulimit -n`, before it starts, as the tests of the machine jobs do. Linux holds 4096 and gives a process a thousand or more, and a program that wants more raises the descriptors the same way.
- A test answers it with a handler that plays the other side, so a server and a client are tested with no network and no port.
- In a [WebAssembly module](../compiler.md#the-host-of-a-webassembly-module) the operations are imports. A host that gives a module no raw connection, as [Cloudflare](cloudflare.md) does not, supplies none of them, and a program that carries `net` to its start is not built for it: there `http` and `fetch` are answered by the host and `net` is not needed.

## The forms

| Written | Is |
| --- | --- |
| `with l = listen(host, port)` | A listener for the rest of the block |
| `with c = connect(host, port)` | A connection for the rest of the block |
| `with s = bind(host, port)` | A socket for datagrams for the rest of the block |
| `accept(l)` | The next connection, which the program closes |
| `read(c, most)`, `write(c, bytes)` | Bytes that have come, and bytes sent, all of them |
| `send(s, to, bytes)`, `receive(s, most)` | One datagram out, and the next one in |
