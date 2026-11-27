---
type: specification
title: Requests to another server
description: The fetch effect of std.fetch, by which a program sends an HTTP request to another server and gets its response, one function for each method, and what answers it natively and in a WebAssembly module.
status: partial
built:
  - "`std.fetch` with the effect `fetch`, whose operation `send` takes a `Request` and gives a `Reply` or a `FetchError`, `Request` with a case for each of `get`, `head`, `delete`, `post`, `put`, and `patch`, each holding a `Url` or an `Upload` with the address, the most bytes of body to read, the headers, and for a body its `Payload`, `Reply`, `Response[most]` with its `Body[most]`, `HeadResponse`, and `FetchError` with its `Fmt` as the table says"
  - "`get`, `delete`, `post`, `put`, and `patch`, each with the limit `[limit: Nat = 1048576]` and `headers: List[(HeaderName, HeaderValue)] = []` after the `;`, giving `Response[limit]`, which holds the body only when it has at most `limit` bytes, and `head`, which gives a `HeadResponse` and takes no limit; a call that does not fit its method, as `get(url, body: x)`, a `post` with no body, and the `.body` of what `head` gives, is refused where it is written with a message that names the method"
  - "`client`, the handler of a native executable, which answers `fetch` for an address that begins `http://` by writing the request to a connection of its own through `net` and reading the answer framed by its length, in chunks, or by the end of the connection, in pieces of any size, on Linux, macOS, and Windows, and holds no more than `limit` bytes of body: a body that the head says is longer is not read, and one that comes longer is `.too_big`; a connection is closed once, when the send ends or the task that sends is cancelled; a call that waits suspends its task and no other, where `client` is installed inside the group, as [Handlers and tasks](../language/task-handlers.md) says"
  - "a handler of `fetch` that a test writes, which gives each case exactly what its method has and a `Reply` that no network made, so a program that asks other servers is tested with no network, and a reply with a body past the limit the request carries is `.too_big` whatever the handler meant"
  - "`host`, the handler of a WebAssembly module, which frames the request as bytes and gives it to the two imports `fetch.send` and `fetch.take` of the host, and reads the answer the host gives back as `client` does, with the same limit; under `node` the host makes the request with a child process and waits for it"
  - "not a request over `https`, which is `.err(.failed((url, \"https is not built yet\")))` until TLS 1.3 is built, or `untrusted`"
---

# Requests to another server

The module `std.fetch` declares the `fetch` [effect](../language/effect-declarations.md), by which a program asks another server for something over HTTP. A program says what it asks, and the deployment says how the request travels, as it does for [`http`](http.md), which is the other direction.

```
use std.fetch (FetchError, fetch, get)

fn keys_of(issuer: str) -> Result[List[Byte], FetchError] <fetch, alloc> {
  response = try get[65536]("{issuer}/.well-known/jwks.json")
  if response.status == 200 { .ok(response.body.bytes) } else { .err(.failed((issuer, "status {response.status}"))) }
}
```

`keys_of("http://login.example.com")` asks that server for the document at that path and gives its bytes, which are at most 65536, or what went wrong.

## The functions of a method

Each method is a function that takes only what that method has, so a request that is wrong cannot be written. `get`, `head`, and `delete` take a `url` and `headers`, and have no body parameter. `post`, `put`, and `patch` take a `url`, a body that is required, and `headers`. What comes before the `;` is given by position and what comes after it by name, as [Functions](../language/functions.md#parameters-by-position-and-by-name) says.

`get`, `post`, `put`, `patch`, and `delete` take the limit on the body as a [type parameter](../language/inference.md#a-whole-number-a-call-leaves-out), `[limit: Nat]`, fixed when the program is compiled, with the default `1048576`, 1 MiB, so `get(url)` is `get[1048576](url)`. Each gives `Result[Response[limit], FetchError]`.

```
fn get[limit: Nat = 1048576](url: str; headers: List[(HeaderName, HeaderValue)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
fn delete[limit: Nat = 1048576](url: str; headers: List[(HeaderName, HeaderValue)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
fn post[limit: Nat = 1048576](url: str, payload: take Payload; headers: List[(HeaderName, HeaderValue)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
fn put[limit: Nat = 1048576](url: str, payload: take Payload; headers: List[(HeaderName, HeaderValue)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
fn patch[limit: Nat = 1048576](url: str, payload: take Payload; headers: List[(HeaderName, HeaderValue)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
```

`head` takes no limit, as its response has no body. It gives a `HeadResponse`, which has a status and headers and no body field.

```
fn head(url: str; headers: List[(HeaderName, HeaderValue)] = []) -> Result[HeadResponse, FetchError] <fetch, alloc>

struct HeadResponse {
  status: 100..600
  headers: List[(str, str)]
}
```

```
use std.fetch (fetch, get, head, post)
use std.message (json)

fn demo(item: Item) -> Result[(), FetchError] <fetch, alloc> {
  page = try get[65536]("http://example.com/keys")
  made = try post[4096]("http://api.example.com/items", json(item))
  seen = try head("http://example.com/keys")
  .ok(())
}
```

A call that does not fit is refused where it is written, with a message that names the method. `get(url, body: x)` is refused: "`get[1048576]` has no parameter named `body`", a `post` with no body is refused: "`post[1048576]` needs an argument for `payload`", and `seen.body` is refused: "`HeadResponse` has no field named `body`".

The `headers` of a request are a `HeaderName` and a `HeaderValue` of [`std.message`](message.md#a-header), the types a response of [`std.http`](http.md) uses. `content-type` and `content-length` are the payload's, so `header_name("Content-Type")` is refused where it is written, and `check_name` of a name known when the program runs gives `written_by_the_library`, so no request is sent with two types or two lengths.

## A body

A body is a `Payload` of [`std.message`](message.md#a-payload), made by `json(value)`, `form(pairs)`, `text(s)`, `html(page)`, or `bytes(data, content_type)`, and only by them. The request has the payload's type as `content-type` and its size as `content-length`, so they cannot disagree with it.

## The effect

```
effect fetch {
  fn send(request: Request) -> Result[Reply, FetchError]
}

enum Request {
  get: Url
  head: Url
  delete: Url
  post: Upload
  put: Upload
  patch: Upload
}

struct Url {
  url: str
  most: Nat
  headers: List[(HeaderName, HeaderValue)] = []
}

struct Upload {
  url: str
  most: Nat
  payload: Payload
  headers: List[(HeaderName, HeaderValue)] = []
}

struct Reply {
  status: 100..600
  headers: List[(str, str)]
  body: List[Byte]
}

struct Response[most: Nat] {
  status: 100..600
  headers: List[(str, str)]
  body: Body[most]
}

struct Body[most: Nat] {
  bytes: List[Byte]
  inside: bytes.size <= most
}

enum FetchError {
  address: str
  unreachable: str
  untrusted: str
  failed: (str, str)
  too_big: (str, Nat)
}
```

`send` takes the enum `Request`, so a handler of `fetch` matches the case and gets exactly what that method has: `.get(u)` has the `Url`, and `.post(u)` has the `Upload` with its `Payload`. `most` is the limit the function was given, which the handler reads the body up to. `send` gives a `Reply` and not a `Response`, since an operation of an effect cannot name a type by a value it is given; the function of the method makes the `Response[limit]` from it, when the body has at most `limit` bytes, and is `.too_big` when it has more, so the proof of a `Body` is never taken on trust from a handler.

Each case of `FetchError` holds the URL of the request, as [Errors of the library](errors.md) says, and `failed` also holds what was said, and `too_big` the limit. `FetchError` has a `Fmt` that writes the URL and then what went wrong.

| Error | Message |
| --- | --- |
| `.address("example.com")` | `example.com: not an address` |
| `.unreachable("http://example.com/keys")` | `http://example.com/keys: server not reached` |
| `.untrusted("https://example.com/keys")` | `https://example.com/keys: certificate not trusted` |
| `.failed(("http://example.com/keys", "status 500"))` | `http://example.com/keys: status 500` |
| `.too_big(("http://example.com/keys", 65536))` | `http://example.com/keys: body longer than 65536 bytes` |

- A `send` gives the response of the server, whatever its status: a 404 is `.ok` of a response whose status is 404. It gives `.err` only when there is no response to give.
- `address` is a `url` that is not one: it does not begin `http://` or `https://`, has no host, holds a space or `@` in its host, or has a port that is not a number from 1 to 65535. `unreachable` is a server that could not be found, or did not take the connection. `untrusted` is a server reached over `https` whose certificate does not show it is the server the address names. `failed` is anything else, with what was said, as a server that closed the connection before it answered or sent no answer.
- **The body has a limit.** The function takes the most bytes of body it will read. It stops reading once the body would pass the limit and gives `.err(.too_big((url, limit)))`, and keeps no more than the limit. A head that says the body is longer is not followed by a read of the body. The body it gives is a `Body[limit]`: the bytes, with the [proof](../language/proof-fields.md) that there are at most `limit` of them, so a function that needs a body of at most `n` bytes takes a `Body[n]`, or a `Response[n]`, and [`arith`](../language/arith.md) checks every call of it. A body is bytes, which [`std.utf8`](utf8.md) reads as text where it is text.
- **An address that begins `https` is `.err(.failed((url, "https is not built yet")))`** until TLS 1.3 is built, and no byte is sent.
- A response that sends the program to another address is given as it is, with its status and its `location`, and a send follows nothing.
- A request is written as `HTTP/1.1`, with the header `host` of the address unless the program gave one, and `connection: close`, so one connection serves one request.

## What answers it

What answers `fetch` is the [deployment](../language/deployments.md) and not the program.

- A test answers it with a handler of its own that gives the replies it wants, so a program that asks other servers is tested with no network.
- A native executable answers it with `client`, a handler written in Smith over `net`, which a deployment installs with `with client()`. For an address that begins `http://` it writes the request to a socket and reads the answer with [`std.http1`](http1.md). It is installed inside the group of tasks that send, so a send suspends its own task while it waits, and a task that is cancelled closes its connection.

```
use std.fetch (client, get)
use std.task (join, spawn, tasks)

fn both(a: str, b: str) -> (Result[Response[65536], FetchError], Result[Response[65536], FetchError]) <net, alloc> {
  with tasks()
  with client()
  ta = spawn { get[65536](a) }
  tb = spawn { get[65536](b) }
  (join(ta), join(tb))
}
```

- In a [WebAssembly module](../compiler.md#the-host-of-a-webassembly-module) the handler is `host()` in place of `client()`: `with host()` turns each send into the two imports of the host, which are bytes in and bytes out, so a module has them exactly when it uses `host`. On [Cloudflare](cloudflare.md) the Worker supplies the same two imports with its own way of asking, so a deployment there leaves `fetch` to it, as it leaves `http`.

## The forms

| Written | Is |
| --- | --- |
| `get[n](u)` | The response of the server at `u`, with a body of at most `n` bytes, or why there is none; `delete` is the same |
| `post[n](u, json(x))` | The same, with a body sent; `put` and `patch` are the same |
| `head(u)` | The status and headers of the server at `u`, with no body, and no limit |
| `get(u, headers: [...])` | A request with the headers after the `;` given by name |
| `with client()` | A native executable's answer to `fetch` |
