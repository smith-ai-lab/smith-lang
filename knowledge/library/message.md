---
type: specification
title: Messages of HTTP
description: std.message, which holds what a request and a response have in common: the method, a header's name and value, and a payload, whose content type and length come only from how it was made.
status: partial
built:
  - "`std.message`, which `std.http` imports, with `Method`, `HeaderName`, `HeaderValue`, and `HeaderError`, `header_name` and `check_name`, `header_value` and `check_value`, and `Payload` with `json`, `form`, `text`, `html`, and `bytes`, and `content_type`, `size`, and `content`, which read one"
  - "`header_name` and `check_name` refuse `content-type`, `content-length`, `connection`, and `transfer-encoding` in any case of their letters, the first where it is written and the second as `written_by_the_library`"
---

# Messages of HTTP

`std.message` holds what a request and a response share, so that [`std.http`](http.md), which answers requests, and [`std.fetch`](fetch.md), which sends them, both import it and neither imports the other.

```
use std.message (Payload, text)

fn greeting() -> Payload <alloc> {
  text("hello")
}
```

## A method

`Method` is the enum of `get`, `post`, `put`, `delete`, `patch`, and `head`. A request has one, so a method that is not one of them cannot be written.

## A header

A header is a name and a value, and each is a type that holds only what may be written.

```
use std.message (HeaderName, HeaderValue, header_name, header_value)

fn language() -> (HeaderName, HeaderValue) <alloc> {
  (header_name("Accept-Language"), header_value("en"))
}
```

- A `HeaderName` holds a token in small letters: letters, digits, and `!#$%&'*+-.^_`|~`, and at least one. `header_name("Accept-Language")` is `accept-language`.
- A `HeaderValue` holds a text with no line break, so that a header never adds a line of its own.
- `header_name` and `header_value` take a text written out, and refuse any other where the call is written: the call and its text are in the message. `header_name("a b")` is refused, and so is `header_value` of a text with a line break. The fields are private, so no other function makes either type.
- **Four names cannot be a name at all,** in any case of their letters. `content-type` and `content-length` are what the body of a message says, and `connection` and `transfer-encoding` are what the library says of how it is carried, so a message never says two lengths or two kinds. `header_name("Content-Type")` is refused where it is written, with the call and its text in the message.
- A text known only when the program runs, as one read from a request, is made by `check_name` or `check_value`, which give a `Result` whose error is a `HeaderError`: `not_a_token`, `written_by_the_library`, or `line_break`, each holding the text, with a `Fmt` that writes the text and what is wrong. `check_name("content-type")` is `.err(.written_by_the_library("content-type"))`.

## A payload

A payload is the body of a message together with the type that says what it is. Its content type and its length come only from how it was made, so the two headers that describe a body are never written by hand and never disagree with it.

```
use std.bits (u64)
use std.html (Html)
use std.message (Payload, bytes, form, header_value, html, json, text)

struct Item {
  id: u64
}

fn kinds(item: Item, page: Html) -> List[Payload] <alloc> {
  [json(item), form([("name", "Ana Lee")]), text("hello"), html(page), bytes([137, 80], header_value("image/png"))]
}
```

| Written | Is |
| --- | --- |
| `json(value)` | The JSON of any value with [`Serialize`](serialize.md), with the content type `application/json` |
| `form(pairs)` | A list of `(str, str)` as `application/x-www-form-urlencoded`: each name and value has a letter, a digit, or one of `*-._` as it is, a space as `+`, and any other byte as `%` and two capital hexadecimal digits, a name and its value are joined by `=`, and the pairs by `&` |
| `text(s)` | The text `s`, with the content type `text/plain; charset=utf-8` |
| `html(page)` | An [`Html`](html.md), with the content type `text/html; charset=utf-8`, so a text is never taken for a page |
| `bytes(data, content_type)` | The bytes `data`, with the content type `content_type`, a `HeaderValue` |

`Payload` has private fields, and only these five functions make one. A response of [`std.http`](http.md) holds one as its `body` only where its status may have a body: a status of 1xx, 204, or 304 has none, so a `WithBody` of one is refused where it is written. `content_type(p)` is its type as a `HeaderValue`, `size(p)` is the number of its bytes, which is the `content-length` of the message that carries it, and `content(p)` is its bytes.

## The forms

| Written | Is |
| --- | --- |
| `header_name("Accept")`, `header_value("en")` | A header's name and value, refused where written when they are not allowed |
| `check_name(t)`, `check_value(t)` | The same for a text known when the program runs, as a `Result` |
| `json(x)`, `form(p)`, `text(s)`, `html(h)`, `bytes(d, t)` | A `Payload` |
| `content_type(p)`, `size(p)`, `content(p)` | What a `Payload` holds |
