---
type: specification
title: Errors of the library
description: Every error type of the standard library holds what its message needs, such as the path or the address, and has a Fmt that writes that message, so a main that returns it with try says what failed.
---

# Errors of the library

An operation of the standard library that can fail gives a `Result`, and its error is a value that says what failed and what it failed on. Each error type of a module that does something outside the program is a struct of two fields: what the operation was given that the message needs, and `kind`, an enum of the ways it fails.

```
struct FsError {
  path: str
  kind: FsErrorKind
}

enum FsErrorKind {
  missing
  denied
  failed: str
}
```

A program that reacts to a failure matches `kind`. One that only reports it does not look inside.

```
use std.fs (fs, read_all, FsError)

fn load(path: str) -> Result[List[Byte], FsError] <fs, alloc> {
  match read_all(path) {
    .ok(bytes) { .ok(bytes) }
    .err(e) {
      match e.kind {
        .missing { .ok([]) }
        .denied { .err(e) }
        .failed(why) { .err(e) }
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
| `FsError(path: "missing.txt", kind: .missing)` | `missing.txt: no such file` |
| `FsError(path: "secret.key", kind: .denied)` | `secret.key: permission denied` |
| `FsError(path: "data.bin", kind: .failed("read: error 5"))` | `data.bin: read: error 5` |

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

| Module | Error | Holds |
| --- | --- | --- |
| [`std.fs`](fs.md) | `FsError` | The path the operation was given, or the one the `File` was opened with |
| [`std.net`](net.md) | `NetError` | The host and the port, as `example.com:80`, that the operation was given, or the one the handle was opened with |
| [`std.fetch`](fetch.md) | `FetchError` | The URL of the request |

What an error holds is copied into it when the operation fails, so an operation that succeeds costs nothing for it. A handle that the error of a later operation names, as a `File` or a `Connection`, keeps what it was opened with.
