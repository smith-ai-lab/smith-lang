---
type: specification
title: Errors of the library
description: Every error type of the standard library is an enum whose cases hold what the message needs, such as the path or the address, and has a Fmt that writes that message, so a main that returns it with try says what failed.
---

# Errors of the library

An operation of the standard library that can fail gives a `Result`, and its error is an [enum](../language/enums.md) of the ways it fails. Each case holds what the operation was given that a message needs, so the error says what failed and what it failed on. A case that has more to say holds a tuple.

```
enum FsError {
  missing: str
  denied: str
  failed: (str, str)
}
```

`missing` and `denied` hold the path. `failed` holds the path and what the system said.

A program that reacts to a failure matches the case. One that only reports it does not look inside.

```
use std.fs (fs, read_all, FsError)

fn load(path: str) -> Result[List[Byte], FsError] <fs, alloc> {
  match read_all(path) {
    .ok(bytes) { .ok(bytes) }
    .err(e) {
      match e {
        .missing(p) { .ok([]) }
        .denied(p) { .err(e) }
        .failed(p) { .err(e) }
      }
    }
  }
}
```

`load` gives no bytes for a file that is not there and the error for anything else.

## The message

Each error type has a `Fmt`, so `"{e}"` is a line a person reads: what it was given, then what went wrong.

| Error | Message |
| --- | --- |
| `.missing("missing.txt")` | `missing.txt: no such file` |
| `.denied("secret.key")` | `secret.key: permission denied` |
| `.failed(("data.bin", "read: error 5"))` | `data.bin: read: error 5` |

A [`main`](../language/main.md) that returns a `Result` writes its error with this `Fmt`, so a program that reads its files with `try` says what failed with nothing more written.

```
use std.console (console, print)
use std.fs (fs, read_all, write_all, FsError)

fn main() -> Result[(), FsError] <console, fs, alloc> {
  bytes = try read_all("photo.jpg")
  try write_all("photo-copy.jpg", bytes)
  print("copied {bytes.size} bytes")
  .ok(())
}
```

Where `photo.jpg` is not there, this writes `photo.jpg: no such file` to standard error and ends with status 1.

## What each error holds

| Module | Error | Each case holds |
| --- | --- | --- |
| [`std.fs`](fs.md) | `FsError` | The path the operation was given, or the one the `File` was opened with |
| [`std.net`](net.md) | `NetError` | The host and the port, as `example.com:80`, that the operation was given, or the one the handle was opened with |
| [`std.fetch`](fetch.md) | `FetchError` | The URL of the request |

What a case holds is copied into it when the operation fails, so an operation that succeeds costs nothing for it. A handle that the error of a later operation names, as a `File` or a `Connection`, keeps what it was opened with.
