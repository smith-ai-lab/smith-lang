---
type: specification
title: Requests to another server
description: The fetch effect of std.fetch, by which a program sends an HTTP request to another server and gets its response, and what answers it natively and in a WebAssembly module.
status: partial
built:
  - "`Method` of [`std.message`](message.md), the enum of `get`, `post`, `put`, `delete`, `patch`, and `head`, which `Request.method` of [`std.http`](http.md) is, and `Payload` with its makers `json`, `form`, `text`, `html`, and `bytes`, which a request's body is"
  - "not the effect `fetch`, `Request`, `Response`, `Body`, `FetchError`, or any handler"
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

`keys_of("https://login.example.com")` asks that server for the document at that path and gives its bytes, which are at most 65536, or what went wrong.

## The effect

```
effect fetch {
  fn send(request: Request, limit: Nat) -> Result[Response[limit], FetchError]
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
  headers: List[(str, str)] = []
}

struct Upload {
  url: str
  payload: Payload
  headers: List[(str, str)] = []
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

Each case holds the URL of the request, as [Errors of the library](errors.md) says, and `failed` also holds what was said, and `too_big` the limit. `FetchError` has a `Fmt` that writes the URL and then what went wrong.

| Error | Message |
| --- | --- |
| `.address("example.com")` | `example.com: not an address` |
| `.unreachable("https://example.com/keys")` | `https://example.com/keys: server not reached` |
| `.untrusted("https://example.com/keys")` | `https://example.com/keys: certificate not trusted` |
| `.failed(("https://example.com/keys", "status 500"))` | `https://example.com/keys: status 500` |
| `.too_big(("https://example.com/keys", 65536))` | `https://example.com/keys: body longer than 65536 bytes` |

- `send` gives the response of the server, whatever its status: a 404 is `.ok` of a response whose status is 404. It gives `.err` only when there is no response to give.
- `address` is a `url` that is not one. `unreachable` is a server that could not be found or did not answer. `untrusted` is a server reached over `https` whose certificate does not show it is the server the address names. `failed` is anything else, with what was said.
- **The body has a limit.** `send` takes the most bytes of body it will read. It stops reading once the body would pass the limit and gives `.err(.too_big((url, limit)))`, and no more than the limit is held. The body it gives is a `Body[limit]`: the bytes, with the [proof](../language/proof-fields.md) that there are at most `limit` of them, so a function that needs a body of at most `n` bytes takes a `Body[n]`, or a `Response[n]`, and [`arith`](../language/arith.md) checks every call of it. A body is bytes, which [`std.utf8`](utf8.md) reads as text where it is text.
- **An address that begins `https` is always checked.** No argument and no setting sends a request to a server whose certificate is not right, so `untrusted` cannot be turned off.
- A response that sends the program to another address is given as it is, with its status and its `location`, and `send` follows nothing.

## The functions of a method

Each method is a function that takes only what that method has, so a request that is wrong cannot be written. `get`, `head`, and `delete` take a `url` and `headers`, and have no body parameter. `post`, `put`, and `patch` take a `url`, a body that is required, and `headers`.

`get`, `post`, `put`, `patch`, and `delete` take the limit on the body as a type parameter, `[limit: Nat]`, fixed when the program is compiled, as `get[65536](url)` does. Each gives `Result[Response[limit], FetchError]`, and is written over `send`.

```
fn get[limit: Nat](url: str; headers: List[(str, str)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
fn delete[limit: Nat](url: str; headers: List[(str, str)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
fn post[limit: Nat](url: str, payload: Payload; headers: List[(str, str)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
fn put[limit: Nat](url: str, payload: Payload; headers: List[(str, str)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
fn patch[limit: Nat](url: str, payload: Payload; headers: List[(str, str)] = []) -> Result[Response[limit], FetchError] <fetch, alloc>
```

`head` takes no limit, as its response has no body. It gives a `HeadResponse`, which has a status and headers and no body field.

```
fn head(url: str; headers: List[(str, str)] = []) -> Result[HeadResponse, FetchError] <fetch, alloc>

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

A call that does not fit is refused where it is written, with a message that names the method. `get(url, body: x)` is refused: "`get` has no body", a `post` with no body is refused: "`post` needs a body", and `seen.body` is refused: "a `HeadResponse` has no body". A limit is written at every call: `get(url)` is refused: "`get` needs a limit on the body, which a type parameter gives", with the fix "write `get[65536](url)`".

## A body

A body is a `Payload` of [`std.message`](message.md#a-payload), made by one of five functions, and only by them.

| Written | Is |
| --- | --- |
| `json(value)` | The JSON of any value with [`Serialize`](serialize.md), with the content type `application/json` |
| `form(pairs)` | A list of `(str, str)` as `application/x-www-form-urlencoded` |
| `text(s)` | The text `s`, with the content type `text/plain; charset=utf-8` |
| `html(page)` | An [`Html`](html.md), with the content type `text/html; charset=utf-8` |
| `bytes(data, content_type)` | The bytes `data`, with the content type `content_type`, a `HeaderValue` |

The `content-type` and `content-length` of the request are the payload's, so they cannot disagree with it. A `headers` entry that names either is refused where it is written when the name is written out, because `header_name` refuses both, and is `.err(.failed(...))` before anything is sent when it is not known until the program runs, because `check_name` does.

`send` takes the enum `Request`, so a handler of `fetch` matches the case and gets exactly what that method has: `.get(u)` has the `Url`, and `.post(u)` has the `Upload` with its `Payload`.

## What answers it

What answers `fetch` is the [deployment](../language/deployments.md) and not the program.

- A test answers it with a handler of its own that gives the responses it wants, so a program that asks other servers is tested with no network.
- In a [WebAssembly module](../compiler.md#the-host-of-a-webassembly-module) `send` is an import, and the host makes the request. On [Cloudflare](cloudflare.md) that is the Worker's own way of asking, so a deployment there leaves `fetch` to it, as it leaves `http`.
- A native executable answers it with a handler written in Smith over `net`. For an address that begins `http` it writes the request to a socket. For one that begins `https` it speaks TLS 1.3, which is written in Smith in the library, on [`std.sha2`](sha2.md) and the arithmetic of [wide numbers](../language/arithmetic.md#more-than-64-bits), and it trusts the certificates the system trusts. Nothing of the system's own TLS, and no library that is not Smith, is called.

## The forms

| Written | Is |
| --- | --- |
| `get[n](u)` | The response of the server at `u`, with a body of at most `n` bytes, or why there is none; `delete` is the same |
| `post[n](u, json(x))` | The same, with a body sent; `put` and `patch` are the same |
| `head(u)` | The status and headers of the server at `u`, with no body, and no limit |
| `json(x)`, `form(p)`, `text(s)`, `html(h)`, `bytes(d, t)` | A `Payload` of `std.message` for `post`, `put`, or `patch` |
