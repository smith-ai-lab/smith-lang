---
type: specification
title: Requests to another server
description: The fetch effect of std.fetch, by which a program sends an HTTP request to another server and gets its response, and what answers it natively and in a WebAssembly module.
status: partial
built:
  - "`Method` of `std.fetch`, the enum of `get`, `post`, `put`, `delete`, `patch`, and `head`, which `Request.method` of [`std.http`](http.md) is"
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

struct Request {
  url: str
  method: Method = .get
  headers: List[(str, str)] = []
  body: str = ""
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

enum Method {
  get
  post
  put
  delete
  patch
  head
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

`get`, `post`, `put`, `patch`, and `delete` each send a request with the method of their name, and take the limit on the body as a type parameter, as `get[65536](url)` does. Each gives `Result[Response[limit], FetchError]`, and is written over `send`.

```
fn get[limit: Nat](url: str, headers: List[(str, str)] = []) -> Result[Response[limit], FetchError] <fetch, alloc> {
  send(Request(url: url, method: .get, headers: headers), limit)
}

fn post[limit: Nat](url: str, body: str = "", headers: List[(str, str)] = []) -> Result[Response[limit], FetchError] <fetch, alloc> {
  send(Request(url: url, method: .post, headers: headers, body: body), limit)
}
```

`put` and `patch` are `post` with their own method, and `delete` is `get` with its own. The limit is written at every call, and no function has a default for it, so `get(url)` is refused: "`get` needs a limit on the body, which a type parameter gives", with the fix "write `get[65536](url)`". A limit that is not a `Nat` is refused as any type argument is.

`head` takes no limit, as the response to it has no body: `head(url)` gives `Result[Response[0], FetchError]`, which has the status and the headers of what a `get` of the same address would give.

```
fn head(url: str, headers: List[(str, str)] = []) -> Result[Response[0], FetchError] <fetch, alloc> {
  send(Request(url: url, method: .head, headers: headers), 0)
}
```

A request that needs another method, or a header and a body together in a way these do not, is a `Request` given to `send`.

## What answers it

What answers `fetch` is the [deployment](../language/deployments.md) and not the program.

- A test answers it with a handler of its own that gives the responses it wants, so a program that asks other servers is tested with no network.
- In a [WebAssembly module](../compiler.md#the-host-of-a-webassembly-module) `send` is an import, and the host makes the request. On [Cloudflare](cloudflare.md) that is the Worker's own way of asking, so a deployment there leaves `fetch` to it, as it leaves `http`.
- A native executable answers it with a handler written in Smith over `net`. For an address that begins `http` it writes the request to a socket. For one that begins `https` it speaks TLS 1.3, which is written in Smith in the library, on [`std.sha2`](sha2.md) and the arithmetic of [wide numbers](../language/arithmetic.md#more-than-64-bits), and it trusts the certificates the system trusts. Nothing of the system's own TLS, and no library that is not Smith, is called.

## The forms

| Written | Is |
| --- | --- |
| `get[n](u)` | The response of the server at `u`, with a body of at most `n` bytes, or why there is none; `post`, `put`, `patch`, and `delete` are the same with their own method |
| `post[n](u, body: b, headers: h)` | The same, with a body and headers sent |
| `head(u)` | The status and headers of the server at `u`, with no body, and no limit |
| `send(Request(url: u), n)` | The same as `get[n](u)`, for a request built by hand |
| `Request(url: u, method: .post, headers: h, body: b)` | A request with a method, headers, and a body, for `send` |
