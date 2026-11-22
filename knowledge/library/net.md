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
  - "a handler of `net` answers an operation that gives a `Result` with an error, with no system call, on every target, and `stop`, `close`, and `unbind` with nothing; a handler that gives a `Listener`, a `Connection`, or a `Socket` is not built, as its handle is private to `std.net`"
  - "on Linux a call of `read`, `write`, or `accept` in a task of a group suspends the task, which waits in `epoll` for the connection while other tasks run, and a call with no group blocks, as it did; macOS blocks until #967"
  - "not Windows, or a name looked up"
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
| `listen_on` | Begins to take connections at a port of this machine. Port 0 asks the system for one that is free |
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

## What answers it

- A native executable answers `net` with the system's own calls, for every system Smith builds for, and a call that waits suspends its task.
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
