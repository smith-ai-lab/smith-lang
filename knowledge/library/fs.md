---
type: specification
title: Files
description: The fs effect, for files, a file open for a block, ranges of it read as values, and reading and writing a whole file.
status: partial
built:
  - "`std.fs` declares `fs`, with the six operations, `FsError`, and `File`, a struct with the system's handle and `path`, the path it was opened with, and neither `Copy` nor `Discard`, so a `File` that is not closed, is closed twice, or is used after it is closed is refused where it is compiled"
  - "`open` and `create`, written for `with`, which lend the file to the rest of the block and close it afterwards on every path, and `read_all` and `write_all`, written with the operations as the page shows them"
  - "on Linux, on x86-64 and arm64, the system answers every operation with the kernel's own calls: `openat`, `pread64`, `pwrite64`, `lseek`, `fchmod`, and `close`, with `read` and `write` for a file that has no places; a file that is not there is `missing`, one the program may not read or write is `denied`, and any other refusal is `failed` with the call and the system's error number, as `read: error 21` for a directory, each holding the path, and no operation ends the program"
  - "`read` gives the bytes that lie in the range, fewer where the file ends first and none past its end, and reads a range of a file of a million bytes with one call; `write_all` of `executable: true` makes a file that the system runs, also one that existed"
  - "a task in a group that calls `read` or `write` suspends when the file would make it wait, as a pipe does with nothing in it, and the others run meanwhile, as [Files that have no places](#files-that-have-no-places) says; a function that holds a `File` while it may wait and does not carry `fs` is refused where it is compiled"
  - "`memory_fs`, a handler of `fs` over files kept in memory, with `file`, `handle`, and `release`, by which a handler that a test writes makes, reads the number of, and ends a `File`, as [Handlers](#handlers-of-fs) says"
  - "on macOS, on x86-64 and arm64, the system answers with the wrappers of `libSystem` of the same names, `openat`, `pread`, `pwrite`, `lseek`, `fchmod`, `read`, `write`, and `close`, with the flags and the error numbers of that kernel, and on Windows, on x86-64 and arm64, with `CreateFileW`, which the path is given to in UTF-16, `ReadFile` and `WriteFile`, which a place is given to by an `OVERLAPPED`, `GetFileSizeEx`, and `CloseHandle`, with each refusal read as the same `missing`, `denied`, or `failed` as on Linux; a program that does not carry `fs` imports none of them, and a file written as `executable: true` is one that macOS runs, and one that Windows runs by its name, which has no mode"
  - "a `File` that a task holds, directly, in a struct, a list, or a lambda that never ran, is closed by the handler in force, once, when the task is cancelled or fails, as a handle of `net` is"
  - "`FsError` has a `Fmt` that writes the message of [Errors of the library](errors.md), so a `main` that returns a `Result` of it and uses `try` writes `photo.jpg: no such file` to standard error and ends with status 1 on each target `fs` is built for"
  - "not built: WebAssembly, where a program that carries `fs` is refused naming the target, and the wait of a task for a file that has no places on Windows"
---

# Files

The module `std.fs` declares the `fs` [effect](../language/effect-declarations.md). A program reads a file by asking for a range of its bytes, which it gets as a value.

Most programs read a whole file, or write one.

```
use std.fs (fs, read_all, write_all, FsError)

fn copy(from: str, to: str) -> Result[(), FsError] <fs, alloc> {
  bytes = try read_all(from)
  write_all(to, bytes)
}
```

| Function | Does |
| --- | --- |
| `read_all(path)` | Gives every byte of the file |
| `write_all(path, bytes, executable: false)` | Makes the file, or replaces it, with these bytes. `executable` makes it one the system will run |

Both are written in Smith with the operations below, and a program that needs less than a whole file uses those. `open` is written for [`with`](../language/with.md): it lends the file to the rest of the block and closes it afterwards, on every path.

```
use std.fs (fs, open, read, size, FsError)

fn header(path: str) -> Result[List[Byte], FsError] <fs, alloc> {
  with f = open(path)
  read(f, 0..64)
}
```

`read` gives the bytes of the file that lie in the range, which are fewer than the range asks for where the file ends first, and none past its end. A file too large to hold is read a range at a time. Each range is a new value, and one that replaces the last in a `var` uses the same memory.

```
enum FsError {
  missing: str
  denied: str
  failed: (str, str)
}

effect fs {
  fn open_file(path: str) -> Result[File, FsError]
  fn create_file(path: str, executable: bool = false) -> Result[File, FsError]
  fn close_file(f: take File)
  fn size(f: File) -> Result[Nat, FsError]
  fn read(f: File, range: Range) -> Result[List[Byte], FsError] <alloc>
  fn write(f: File, at: Nat, bytes: List[Byte]) -> Result[(), FsError]
}
```

| Operation | Does |
| --- | --- |
| `open_file` | Opens a file that exists, to read it |
| `create_file` | Makes a file, or empties one that exists, to write it |
| `close_file` | Closes the file, which cannot be used again |
| `size` | Gives how many bytes the file holds |
| `read` | Gives the bytes in a range of the file |
| `write` | Puts the bytes in the file from the place `at` |

Each case of `FsError` holds the path, and `failed` holds what the system said besides it, as [Errors of the library](errors.md) says. `open_file` and `create_file` hold the path they were given. `size`, `read`, and `write` hold the path the `File` was opened with, which the `File` keeps, so a `read` of a file opened as `data.bin` fails with the path `data.bin`. A `File` shows it as the field `path`, which a handler reads, and a call that succeeds does nothing for it. A `File` that a handler made with `file` has the path `""`.

`FsError` has a `Fmt`, so `"{e}"` is `missing.txt: no such file` for `.missing("missing.txt")`, `secret.key: permission denied` for `.denied("secret.key")`, and `data.bin: read: error 5` for `.failed(("data.bin", "read: error 5"))`.

- **A place in a file is a number the program holds.** A file has no position of its own that a read moves, so two tasks read one file without disturbing each other, and reading any part costs the same as reading the next.
- **What is read is a value.** It never changes after it is read. If another program changes the file, a later read gives other bytes, as reading the console twice gives two lines.
- **A `File` is used exactly once,** as [Values used exactly once](../language/resources.md) says. So a file that is not closed, is closed twice, or is used after it is closed does not compile. `open` and `create` take care of it, and most code writes only the `with` line.
- **How a range is fetched is not the program's business.** The system may read it, read ahead, or map the file into memory and copy from there. The source is the same, and [under tasks](../language/tasks.md) a read suspends its task.
- **Every failure is a value.** No operation ends the program.

## Files that have no places

A file that has no places, as a pipe has none, cannot be read from a place, so `read` gives what it has at the time, at most as many bytes as the range is long, and the range's start says nothing. `size` of it is refused with `failed`. A task in a [group](../language/tasks.md) opens each file so that a call that would wait gives the task back, and the system's scheduler waits for the file with the other tasks' waits, so a read of a pipe that nothing has written to yet holds up no other task. A regular file never makes a task wait.

## Handlers of `fs`

A handler supplies the six operations, so a test runs code that reads and writes files over files kept in memory, as `memory_fs` does under [Handlers](../language/handlers.md).

```
use std.fs (FsError, MemoryFile, memory_fs, read_all)

fn fixture() -> Result[List[Byte], FsError] <alloc> {
  with memory_fs([MemoryFile(path: "model.toml", bytes: [1, 2, 3])])
  read_all("model.toml")
}
```

`memory_fs(files, body)` takes `files`, a `Files`, which is a list of `MemoryFile`, each a `path` and its `bytes`. A file is found by its path as a whole, so `a/b` is one name and no directory. `create_file` adds a file or empties the one there is, `write` puts bytes in from the place and fills what the file did not hold with zeros, and nothing in memory is refused. A file that `open_file` of it is not given is `.missing(path)`.

A handler that refuses builds the case with what it was given: the path for `open_file` and `create_file`, and the path of the `File` it is lent, `f.path`, for the others.

```
use std.fs (fs, release)

fn refusing[T](body: fn() -> T <fs>) -> T {
  fs(
    open_file: fn(path) { .err(.denied(path)) },
    create_file: fn(path, executable) { .err(.failed((path, "disk"))) },
    close_file: fn(f) { n = release(f) },
    size: fn(f) { .err(.missing(f.path)) },
    read: fn(f, range) { .err(.missing(f.path)) },
    write: fn(f, at, bytes) { .err(.denied(f.path)) },
    body: body,
  )
}
```

A handler that a test writes makes a `File` from a number of its own choosing with `file`, reads the number of the one it is lent with `handle`, and ends the one it is given with `release`, as the handlers of [`net`](net.md#making-a-value-in-a-handler) do for a connection. The system's handler answers each operation given a `File` it did not open with `failed`, and makes no system call with it.
