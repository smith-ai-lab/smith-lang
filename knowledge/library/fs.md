---
type: specification
title: Files
description: The fs effect, for files, a file open for a block, ranges of it read as values, and reading and writing a whole file.
status: partial
built:
  - "`std.fs` declares `fs`, with the six operations, `FsError`, and `File`, a struct with one private field, the system's handle, and neither `Copy` nor `Discard`, so a `File` that is not closed, is closed twice, or is used after it is closed is refused where it is compiled"
  - "`open` and `create`, written for `with`, which lend the file to the rest of the block and close it afterwards on every path, and `read_all` and `write_all`, written with the operations as the page shows them"
  - "on Linux, on x86-64 and arm64, the system answers every operation with the kernel's own calls: `openat`, `pread64`, `pwrite64`, `lseek`, `fchmod`, and `close`, with `read` and `write` for a file that has no places; a file that is not there is `missing`, one the program may not read or write is `denied`, and any other refusal is `failed` with the call and the system's error number, as `read: error 21` for a directory, and no operation ends the program"
  - "`read` gives the bytes that lie in the range, fewer where the file ends first and none past its end, and reads a range of a file of a million bytes with one call; `write_all` of `executable: true` makes a file that the system runs, also one that existed"
  - "a task in a group that calls `read` or `write` suspends when the file would make it wait, as a pipe does with nothing in it, and the others run meanwhile, as [Files that have no places](#files-that-have-no-places) says; a function that holds a `File` while it may wait and does not carry `fs` is refused where it is compiled"
  - "`memory_fs`, a handler of `fs` over files kept in memory, with `file`, `handle`, and `release`, by which a handler that a test writes makes, reads the number of, and ends a `File`, as [Handlers](#handlers-of-fs) says"
  - "not built: macOS and Windows, where a program that carries `fs` is refused naming the target, and closing a `File` that a task holds when the task is cancelled or fails, which are #1151"
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
  missing
  denied
  failed: str
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

`memory_fs(files, body)` takes `files`, a `Files`, which is a list of `MemoryFile`, each a `path` and its `bytes`. A file is found by its path as a whole, so `a/b` is one name and no directory. `create_file` adds a file or empties the one there is, `write` puts bytes in from the place and fills what the file did not hold with zeros, and nothing in memory is refused.

A handler that a test writes makes a `File` from a number of its own choosing with `file`, reads the number of the one it is lent with `handle`, and ends the one it is given with `release`, as the handlers of [`net`](net.md#making-a-value-in-a-handler) do for a connection. The system's handler answers each operation given a `File` it did not open with `failed`, and makes no system call with it.
