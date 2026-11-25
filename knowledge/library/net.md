---
type: specification
title: The network
description: The net effect of std.net, for connections and datagrams, each a value used exactly once, with a write that sends everything and a read that gives what has come.
status: partial
built:
  - "`std.net` declares `net`, with the operations, `Address`, and `NetError` as the page shows them, and `Listener`, `Connection`, and `Socket`, each a struct with the system's handle, the `host` and `port` it was opened with, and neither `Copy` nor `Discard`"
  - "`listen`, `connect`, and `bind`, written for `with`, which lend what they open to the rest of the block and end it afterwards on every path"
  - "on Linux and macOS, on x86-64 and arm64, the system answers every operation, and a call that waits blocks, except in a group of tasks on Linux, which the next line says; a host is `localhost`, a number written with dots, or a number written with colons, and any other name is looked up by `connect_to` on Linux, as [Looking up a name](#looking-up-a-name) says, and is `failed` with the one message that looking up a name is not built on macOS and in `listen`, `bind`, and `send`"
  - "`failed` holds the address and the call's name and the system's error number, as `connect: error 111` on Linux and `connect: error 61` on macOS; `refused`, `in_use`, and `closed` are the system's refusal of a connection, a port that is held, and a write to a connection the other side closed; each case holds the host and the port, as `example.com:80`, and `NetError` has a `Fmt` that writes them with what went wrong"
  - "a handler of `net` answers an operation that gives a `Result` with an error, with no system call, on every target, and `stop`, `close`, and `unbind` with nothing, and a handler that gives a `Listener`, a `Connection`, or a `Socket` makes it with `listener`, `connection`, or `socket` and reads its number with `handle`, as [Making a value in a handler](#making-a-value-in-a-handler) says"
  - "the system's handler answers every operation given a value it did not open with `failed` and makes no system call with it, on Linux, macOS, and Windows"
  - "on Linux and on macOS a call of `read`, `write`, or `accept` in a task of a group suspends the task, which waits in `epoll` on Linux and in `kqueue` on macOS for the connection while other tasks run, and a call with no group blocks, as it did"
  - "a listener from `listen_on` is made to hold up to 4096 connections that nothing has accepted, or as many as the system allows when that is fewer, so a thousand connections that arrive before the server takes the first are all made at once and none waits for the system to ask again"
  - "`release`, a function of the trait `Release` that has an `impl` for `Listener`, `Connection`, and `Socket`, taking its value, ending it with no system call, and giving back the number of the handle it held"
  - "not built: a handler that keeps state and is under `listen` of `std.http`, which is refused as not built yet, as [Handlers and tasks](../language/task-handlers.md) says, and a handler that assigns a name under `stdio` of `std.mcp`, which stops the compiler"
  - "on Windows, on x86-64 and arm64, the system answers every operation through `ws2_32.dll`, which only a program that carries `net` asks the loader for, and every call waits where it is, so a task that waits for a connection is not built there and waits for an issue of its own, completion ports; `failed` holds Winsock's number, as `connect: error 10061`, and `refused`, `in_use`, and `closed` are the same refusals by their numbers, `in_use` being 10048 or 10013, which Windows gives for a port that a socket holds for itself alone, as a listener does there; the clock is not built there"
  - "`connect_to` of the system with a name that is not `localhost` or a number looks it up on Linux, on x86-64 and arm64, in Smith, in `std.dns` and `std.net`: `/etc/hosts`, and then the nameservers of `/etc/resolv.conf` over UDP, with each behaviour that [Looking up a name](#looking-up-a-name) tables, and tries each address it finds, in its order, until one connects; `resolve` of `std.net` does it for the text of the two files, the port of the nameservers, and the address its errors name, that a caller gives, which is how a test puts a nameserver on a port of 127.0.0.1, and the system's handler calls it with the files, the port 53, and the host and port of the connection"
  - "a lookup in a task of a group suspends the task while it waits for a nameserver, with `wait_to_read_by` until the time to send the queries again, or the end of the time, and the other tasks run meanwhile, as a test of a task that ticks beside a lookup that no nameserver answers shows; the connection to a nameserver over TCP, for an answer cut short, waits for its bytes in the same way but is opened and written with the thread, and a lookup with no group blocks, as a call of `read` does"
  - "not built: a name looked up on macOS or Windows, which will call the system's own lookup, `GetAddrInfoW` of `ws2_32.dll` on Windows, where `connect_to` says what it said"
---

# The network

The module `std.net` declares the `net` [effect](../language/effect-declarations.md), by which a program talks over the network: connections, which carry bytes in order both ways, and datagrams, which are sent one at a time. Most programs never call it. A server asks for requests through [`http`](http.md) and a client asks another server through [`fetch`](fetch.md), and the handlers of those are what use `net`.

```
use std.net (NetError, connect, net, read, write)

fn ask(host: str, question: str) -> Result[List[Byte], NetError] <net, alloc> {
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
  fn read(c: Connection, most: Nat) -> Result[List[Byte], NetError] <alloc>
  fn write(c: Connection, bytes: Slice[Byte]) -> Result[(), NetError]
  fn close(c: take Connection)

  fn bind_to(host: str, port: 0..65536) -> Result[Socket, NetError]
  fn send(s: Socket, to: Address, bytes: Slice[Byte]) -> Result[(), NetError]
  fn receive(s: Socket, most: Nat) -> Result[(Address, List[Byte]), NetError] <alloc>
  fn unbind(s: take Socket)
}

struct Address {
  host: str
  port: 0..65536
}

enum NetError {
  unknown_host: str
  refused: str
  in_use: str
  closed: str
  failed: (str, str)
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
- **Each case holds the host and the port,** written as `example.com:80`, as [Errors of the library](errors.md) says, and `failed` also holds what the system said. `listen_on`, `connect_to`, and `bind_to` hold the host and port they were given, and `send` the address it was given. `accept`, `read`, and `write` hold those of the `Listener` or `Connection`, and `receive` those of the `Socket`, which each keeps as the fields `host` and `port`, which a handler reads. A connection that `accept` gives keeps those of its listener. A value that a handler made has the host `""` and the port 0, and a call that succeeds does nothing for them.

`NetError` has a `Fmt` that writes the address and then what went wrong, as the table shows.

| Error | Message |
| --- | --- |
| `.unknown_host("example.com:80")` | `example.com:80: unknown host` |
| `.refused("example.com:80")` | `example.com:80: connection refused` |
| `.in_use("0.0.0.0:80")` | `0.0.0.0:80: address in use` |
| `.closed("example.com:80")` | `example.com:80: connection closed` |
| `.failed(("example.com:80", "connect: error 110"))` | `example.com:80: connect: error 110` |

## With a block

`listen`, `connect`, and `bind` are written for [`with`](../language/with.md), as `open` of [`std.fs`](fs.md) is: each lends what it opens to the rest of the block and ends it afterwards, on every path. Most code writes these and none of `stop`, `close`, and `unbind`.

```
use std.list (freeze)
use std.net (Connection, NetError, accept, close, listen, net, read, write)

fn turn(c: Connection) -> Result[(), NetError] <net, alloc> {
  bytes = try read(c, 4096)
  write(c, freeze(bytes))
}

fn echo_once(port: 0..65536) -> Result[(), NetError] <net, alloc> {
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

fn ended[T](body: fn() -> T <net>) -> T {
  net(
    listen_on: fn(host, port) { .err(.refused("{host}:{port}")) },
    accept: fn(l) { .err(.refused("{l.host}:{l.port}")) },
    stop: fn(l) { n = release(l) },
    connect_to: fn(host, port) { .err(.refused("{host}:{port}")) },
    read: fn(c, most) { .err(.closed("{c.host}:{c.port}")) },
    write: fn(c, bytes) { .err(.closed("{c.host}:{c.port}")) },
    close: fn(c) { n = release(c) },
    bind_to: fn(host, port) { .err(.refused("{host}:{port}")) },
    send: fn(s, to, bytes) { .err(.closed("{to.host}:{to.port}")) },
    receive: fn(s, most) { .err(.closed("{s.host}:{s.port}")) },
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

fn one_client[T](body: fn() -> T <net>) -> T {
  net(
    listen_on: fn(host, port) { .ok(listener(1)) },
    accept: fn(l) { .ok(connection(handle(l) + 1)) },
    stop: fn(l) { n = release(l) },
    connect_to: fn(host, port) { .err(.refused("{host}:{port}")) },
    read: fn(c, most) { .ok([]) },
    write: fn(c, bytes) { .ok(()) },
    close: fn(c) { n = release(c) },
    bind_to: fn(host, port) { .err(.refused("{host}:{port}")) },
    send: fn(s, to, bytes) { .err(.closed("{to.host}:{to.port}")) },
    receive: fn(s, most) { .err(.closed("{s.host}:{s.port}")) },
    unbind: fn(s) { n = release(s) },
    body: body,
  )
}
```

Under `one_client`, `accept` gives a connection whose handle is 2, and `read` gives no bytes, which is a connection the other side has finished with. A handler may keep what `write` is given in a `var`, as [Handlers](../language/handlers.md#a-handler-that-keeps-state) says.

- **A number the system did not open reaches nothing.** The system's handler answers `listen`, `accept`, `read`, `write`, `send`, and `receive` with `failed`, whose message says the handle is not one the system opened, when it is given a value that `listener`, `connection`, or `socket` made, and makes no system call with it. `stop`, `close`, and `unbind` end such a value and close nothing. A program that makes `connection(1)` and writes to it writes nothing to standard output, under any handler.
- **A value the system opened is not made again.** `handle` gives its number, and `connection` of that number is a value the system did not open.

## Looking up a name

On Linux a native executable links no C library, so it has no `getaddrinfo`, and `std.net` looks a host name up itself, in Smith, in the system's handler of `net`. `connect_to` with a name that is neither `localhost` nor a number written with dots or colons asks `/etc/hosts` and then the nameservers of `/etc/resolv.conf` over UDP, and tries each address it finds, in the order below, until one connects. It gives `unknown_host` when the name has no address, and `failed` when no nameserver answered in time. A handler written in Smith that answers `connect_to` reads no file and asks no one. On macOS and Windows the system's own lookup is called, which those systems already link.

What follows is taken from how glibc (`resolv/`, `nss/nss_files`) and musl (`src/network/lookup_name.c`, `res_msend.c`, `resolvconf.c`) do it. Their documentation and source were read for what they do, and no code is copied. Each behaviour says which one Smith follows.

| Behaviour | Smith | Follows | Why |
| --- | --- | --- | --- |
| Order | `/etc/hosts`, and DNS only when the file has no address for the name | both | `/etc/nsswitch.conf` is not read, as in musl: it names plugins, and plugins are out of scope |
| `/etc/hosts` | A line is an address, then names, up to a `#`; a name matches a whole word, ignoring case; every matching line gives its address, in file order | glibc for case, musl for the rest | A host name has no case |
| A file that is missing or cannot be read | Treated as empty | both | |
| `resolv.conf` lines | `nameserver`, `search`, `domain`, and `options` with `ndots:`, `timeout:`, and `attempts:`; any other line or option is ignored, and a `timeout:` or `attempts:` of 0 is 1 | both | |
| Nameservers | The first three that are addresses; with none, `127.0.0.1` | both | |
| `search` and `domain` | The last of either line gives the list of domains; with none, no domain is added, and the machine's own name is not used | musl | |
| Defaults and limits | `ndots` 1, at most 15; `timeout` 5 seconds, at most 30; `attempts` 2, at most 5 | glibc for the limits | The smaller limits stop a file from making a program wait for minutes |
| Which names are tried | A name with `ndots` dots or more, or ending in a dot, is tried as it is first and then with each domain; any other is tried with each domain first and then as it is. A name or a domain longer than 253 characters, or with a label longer than 63 or empty, is not asked | both | |
| Which records | `A` and `AAAA` of each name, in two queries with ids that differ, sent from one socket | musl | |
| Which nameservers | All at once, and the first accepted answer of each query is the one used | musl | A first nameserver that does not answer costs the whole timeout in glibc, which asks one after another |
| Time and tries | The timeout is the whole wait for each name that is asked; the queries are sent again every timeout divided by the attempts, to every nameserver, until each is answered or the time is gone; a `SERVFAIL` is sent again at once, four times at most, for all the queries of a name together | musl | glibc waits for the timeout of each nameserver in each of its tries, which is minutes with a dead nameserver |
| A truncated answer | When the `TC` bit is set, the query is asked again of that nameserver over TCP, a length of two bytes before each message, and the answer over TCP is the one used; with no answer over TCP, there is none | both | |
| Which answers are believed | An answer is believed when it came from the address and port a query was sent to, repeats the id of the query and its question, has the answer bit set, and has the code 0 or 3 (`NOERROR` or `NXDOMAIN`); any other is ignored, as is a message of fewer than twelve bytes | musl for the address and the id, glibc for the question, neither for the answer bit | A message that someone sends without seeing the query has to match all of them |
| The ids | Each id is two bytes that `getrandom` of Linux fills, and the two queries of a name never share one; a lookup that gets no bytes says so and asks nothing | musl asks for random ids; `getrandom` is a call and not a file, which can fail to open | `net` does not carry `entropy`, so a program that never looks a name up asks for no random bytes, and `resolve` is given the bytes by a function, so that a test fixes the ids |
| Which records of an answer are used | An address in an `A` or `AAAA` record whose owner is the name asked or a name reached from it by `CNAME` records of the same answer, in any order of the answer; every other record is ignored | neither | Both take every address of the right type whatever its owner |
| An answer that is not well formed | A message is read only as far as it was given: a count, a length, or a name that runs past the end ends the reading and the answer is not believed; a pointer inside a name goes only to an earlier place of the message, so no name loops | musl for the bounds | |
| An answer with no address | `NXDOMAIN` or no address of either type goes on to the next name; no answer in time, or `SERVFAIL`, ends the lookup with `failed` | glibc | musl tries the next name after any failure, and a name that exists would be missed |
| The order of the addresses | Those of `/etc/hosts` as they are, otherwise the `A` records and then the `AAAA` records, each as the nameserver gave them | neither | Both sort by RFC 6724, which needs the machine's own addresses |

`unknown_host` holds the name that was asked, with the port of the connection, as `example.com:80`. `failed` holds that address and `lookup: no nameserver answered in time` when the time was gone, and `lookup: the nameserver failed` after a `SERVFAIL`.

`std.dns` holds what needs no socket: `query` makes the message of a question and `read_reply` reads an answer as the table says, `hosts` and `config` read the two files, `candidates` gives the names to ask, and `ipv4` and `ipv6` read a number written with dots or colons, which `std.net` uses for a host too. A program has no use for them. The system's handler reads the files with the calls `openat` and `read`, which only Linux has.

Not done: a cache of answers, DNS over TLS or HTTPS, mDNS (`.local` names), and the plugins of `nsswitch.conf`.

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
