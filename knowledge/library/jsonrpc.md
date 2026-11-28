---
type: specification
title: JSON-RPC
description: std.jsonrpc, which reads and writes the messages of JSON-RPC 2.0, a request, a notification, and a response, as values, with the parameters and results left as RawJson.
status: partial
built:
  - "`std.jsonrpc`, written in Smith, with `Id`, `Params`, `Request`, `Notification`, `Failure`, `Response`, `Message`, and `Received`, `object`, `array`, `params_json`, `read`, `write`, `write_batch`, and the five failures the standard names, which reads an object, a batch, and each member of a batch on its own, never gives a message the standard does not allow, and writes only what `read` takes: parameters that are an object or an array, and a batch of one message or more"
  - "not `use std.jsonrpc (read)` or `(write)` as the example writes it: a name a program lists leaks into the library modules it reaches, so it is refused, and the module is imported as `use std.jsonrpc` with `jsonrpc.read(text)`"
---

# JSON-RPC

The module `std.jsonrpc` reads and writes the messages of JSON-RPC 2.0, the form in which one program asks another to run something by name. The Model Context Protocol and the Language Server Protocol both send such messages, so the module is on its own and [`std.mcp`](mcp.md) is built on it. It knows nothing of how a message travels: a line, the body of an HTTP request, or bytes after a header are the business of whoever carries it.

## A message

```
enum Id {
  number: Int
  text: str
  null
}

struct Object {
  json: RawJson
}

struct Array {
  json: RawJson
}

enum Params {
  none
  object: Object
  array: Array
}

struct Request {
  id: Id
  method: str
  params: Params
}

struct Notification {
  method: str
  params: Params
}

struct Failure {
  code: Int
  message: str
  data: Option[RawJson]
}

struct Response {
  id: Id
  outcome: Result[RawJson, Failure]
}

enum Message {
  request: Request
  notification: Notification
  response: Response
}
```

- A `Request` asks for `method` to be run and is answered by one `Response` with the same `id`. A `Notification` asks and is never answered.
- `params` is `Params`: none, an object, or an array, as the standard says, and never another value. An `Object` and an `Array` hold a [`RawJson`](json.md#rawjson) in a private field, so only `object(raw)` and `array(raw)` make one, and each gives `.some` of the `Params` only when `raw` is an object or an array, as it is named, and `.none` for any other value. A program cannot write `.object(raw)` or `.some(raw)` for a `raw` it has: the one is refused because `Object` has a private field, and the other because `Params` has no case `some`. `params_json(p)` is `.some` of the value that the parameters are, and `.none` for `.none`.
- The result of a `Response` and `data` of a `Failure` are `RawJson`: the text of one JSON value, not taken apart. Whoever knows the method reads it into its own type with `from_json`, so this module needs no type of any protocol built on it.
- An `Id` is what the asker chose. `.null` is the id of a `Response` to a request whose id could not be read.
- A `Failure` is how a request fails: a code, a sentence, and anything more the answerer wants to say.

## Reading

```
enum Received {
  one: Message
  many: List[Result[Message, Failure]]
}

fn read(text: str) -> Result[Received, Failure] <alloc>
```

`read` takes the text of one JSON value. An object is one message. An array is a batch, and each member is read on its own, so one bad member does not lose the rest.

```
use std.jsonrpc (Message, Received, read)

fn method_of(text: str) -> str <alloc> {
  match read(text) {
    .ok(.one(.request(r))) { r.method }
    .ok(.one(.notification(n))) { n.method }
    _ { "" }
  }
}
```

`method_of("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}")` is `"ping"`.

`read` never gives a message that the standard does not allow, and what it refuses is a `Failure` ready to send back:

- Text that is not JSON is `.err` of the failure `parse_error()`.
- A value that is no message is `.err` of `invalid_request()`: one with no `"jsonrpc": "2.0"`, a `method` that is not a text, an `id` that is not a text, a whole number, or `null`, `params` that are neither an object nor an array, a response with both a result and an error or with neither, and an array with nothing in it.
- An id written with a fraction or an exponent is refused, as two programs do not agree on when such numbers are equal.

Where the standard is silent, `read` does this:

- A request whose `id` is `null` is a `Request` with `.null`, and a message with no `id` is a `Notification`.
- A member the standard does not name is passed over and not kept. A `method` that begins with `rpc.` is read as any other.
- A number `id` is a whole number of `Int`: one beyond `Int` is refused, as one with a fraction is, and `-0` is `0`.
- `params` that is `null`, or is no object or array, is refused. `params` of `[]` is `.array` and of `{}` is `.object`, and a message with no `params` has `.none`.
- A member given twice is refused.
- `"jsonrpc"` is the text `2.0` however it is written, so `"2\u002e0"` is read.
- A `Response` has an `id`, and exactly one of `result` and `error`. A `result` of `null` is a result. An `error` is an object with a whole `code` of `Int` and a text `message`, and `data` of any value, `null` too, is `.some`. A message with `method` and also `result` or `error` is refused.
- Text after the value, text of nothing or of spaces, and text that is not JSON are `parse_error()`. A value that is no object or array, such as `3`, `"x"`, or `null`, is `invalid_request()`.
- A member of a batch that is no object, or is an array, is `.err(invalid_request())`, and the others are read.
- A message that is read is as a `Request`, `Notification`, or `Response` whatever the order of its members, and `params` and every `RawJson` keep the text they came as, spaces in them too.

## Writing

```
fn write(to: take Text, message: Message) -> Text <alloc>
fn write_batch(to: take Text, messages: List[Message]) -> Text <alloc>
  requires messages.size > 0
```

`write` appends one message to a [`Text`](text.md), with `"jsonrpc":"2.0"` first, and `write_batch` appends several as one array. `write_batch` requires that `messages` is not empty, as a batch of nothing is an array that `read` refuses: a call with `[]` is refused where it is written, with "cannot prove that `[].size > 0`, which `write_batch` requires of its arguments", and what answers a batch sends nothing when it has nothing to say. So every message that can be made is one `read` takes: for every message `m`, reading what `write` wrote gives `m`, and for every batch `ms` that `write_batch` is given, reading what it wrote gives a `.many` of the messages of `ms`, each as `.ok`.

## The failures the standard names

```
fn parse_error() -> Failure
fn invalid_request() -> Failure
fn method_not_found(method: str) -> Failure <alloc>
fn invalid_params(why: str) -> Failure <alloc>
fn internal_error() -> Failure
```

Each gives the `Failure` with the code the standard gives it, -32700, -32600, -32601, -32602, and -32603. The codes from -32768 to -32000 are the standard's, except -32000 to -32099, which it leaves to whoever builds on it; the Model Context Protocol takes -32020 to -32099 of them. A protocol chooses any other code outside the standard's.

## The forms

| Written | Is |
| --- | --- |
| `read(text)` | One message, or a batch with each member read on its own, or the `Failure` to send back |
| `write(to, m)`, `write_batch(to, ms)` | The message, or the batch, appended to a `Text` |
| `Request`, `Notification`, `Response` | The three messages |
| `Params` | None, an object, or an array, made by `object(raw)` and `array(raw)` |
| `Id` | A whole number, a text, or `null` |
| `parse_error()`, `invalid_request()`, `method_not_found(m)`, `invalid_params(why)`, `internal_error()` | The failures the standard names |
