---
type: specification
title: Project tasks
description: A public function in project.smith, run by name, whose parameters are its command-line flags and whose effects say what it may do.
status: partial
built:
  - "a public function of `project.smith`, read as the module `project` from the nearest `project.smith` at or above the folder the command runs in, built with `smith build <task>` and run with `smith <task> --<flag> <value>`, whose parameters of `str`, a whole number, `bool`, one with a default, and an `Option` are its flags, spelled as the parameter is, with a `bool` bare and one with a default or of type `Option` left out as the page says"
  - "a flag spelled another way, a flag given twice, a value that does not read as its parameter's type, and a missing required flag, each one error that names the flag, and a task that gives `.err(e)` printing `e` and ending with a status that is not 0"
  - "a task named as a command, refused where it is written"
  - "a parameter of a task that is written before the `;`, refused where it is written, as a flag is given by name"
  - "a name that is no command and no task, said as one of three errors: no `project.smith`, one that does not compile, or one with no task of that name"
  - "`smith --watch <task>`, on Linux and macOS, which runs a task again after each change, as [Watching](command-line.md#watching) says, stops a task that carries `diverge` when the next run is built, and keeps the socket that a task listens on from one run to the next, so that no connection is refused or reset while the program is built again, and the old run, which is told it is closed by `accept` giving `.closed`, serves the connections it took"
  - "not `build` of `std.project` called from a task, `exec`, a parameter of another type, a task with more than six parameters that have a default, or a project that has grown into several packages, or a task watched on Windows"
---

# Project tasks

A task is a public function in `project.smith`, run by name. `smith deploy --env staging` runs the task `deploy` with `env` set to `"staging"`.

```
pub fn deploy(; env: str) -> Result[(), DeployError] <fs, net, alloc> {
  binary = try build(package, target: .linux_x64)
  upload(binary, to: env)
}
```

Every public function in `project.smith` is a command.

## Its parameters are its flags

A task's parameters are named flags. `env` is the flag `--env`. A flag is spelled exactly as its parameter is, so `batch_size` is `--batch_size`. No other spelling is taken: `--batch-size` names no parameter.

- A `bool` is a bare flag.
- A parameter with a default, or of type `Option`, may be left out.
- There are no positional arguments, so every parameter of a task is written after the `;`. A task with a parameter before it is refused: "`env` of the task `deploy` is a flag, which is given by name, so it is written after `;`".

```
pub fn deploy(; env: str, verbose: bool, region: str = "eu", note: Option[str]) -> Result[(), DeployError] <fs, net, alloc>
```

`smith deploy --env staging` leaves out the other three flags. `smith deploy --env staging --verbose --region us` gives `verbose` and `region`.

## Its effects say what it may do

A task declares its effects like any function. A task that runs another program needs the `exec` effect.

```
pub fn format_all() <exec>
```

## Building in a task

`build` of `std.project` builds the package, as `smith build` does, and gives what it built.

```
fn build(package: Package; target: Target, start: fn() = main) -> Result[Built, BuildError] <fs, alloc>

struct Built {
  target: Target
  path: str
  beside: List[str]
}

struct BuildError {
  errors: List[str]
}
```

- `target` is a case of `Target`, as `.linux_x64` or `.wasm`, and `start` is the function the executable starts at, which is `main` when none is given, as [Deployments](deployments.md) says.
- A `Built` says where the executable was written, as `path`, and what was written with it, as `beside`: for `.wasm`, the file of JavaScript that loads the module. A task hands it on, as `upload(binary, to: env)` does above.
- A `BuildError` holds what `smith build` would have printed, one text for each error, with its file, line, and column. A task that fails with one prints them and ends with a status that is not 0.
- `build` carries `fs` and `alloc`, as it reads the sources and writes the executable, so a task that builds carries them.

## A task that fails

A `Result` error prints its message and exits non-zero. When `deploy` above gives an error, `smith deploy` prints the error's message and exits non-zero.

## Watching a task

`smith --watch serve --port 9000` runs the task `serve` as `smith serve --port 9000` does, and runs it again each time a file it was built from changes, as [Watching](command-line.md#watching) says. A task that carries `diverge` never ends by itself, so it is the one kind of run that is not let end.

- **A task that carries `diverge` is stopped** when the next run is built.
- **A socket the task listens on is kept by `smith --watch`** and given to the next run, which listens on the same address. A run that is stopped is told so by its listener: `accept` gives `.closed`, so `listen` takes no more connections, serves those it took, and returns, as [HTTP server](../library/http.md#where-the-requests-come-from) says. A connection made in the meantime waits in the socket for the next run, so a page loaded while the program is built again is answered, by one run or the other, and never refused.
- **Nothing a run holds in memory is given to the next.** What it keeps through `store` is kept by the handler of `store`, as between two runs of `smith serve`.

## A name that is no task

`smith <name>`, for a name that is not a built-in command, either runs the task or gives exactly one error. `unknown command` is never said: a word that starts with `-` is no name, and `smith --nope` says "unknown flag `--nope`; the flags of smith are --timings and --watch".

- With no `project.smith` in the folder or any folder above it, the error names the folder, says that none was found, and says that `<name>` would be a task of it.
- With a `project.smith` that has no `pub fn <name>`, the error says that the file, with its path, has no task `<name>`, and lists the tasks it has.
- With a `project.smith` that does not compile, the errors are those of the file, as `smith build <name>` prints them.

All three end with a status that is not 0.

```
$ smith serve
/work/notes/project.smith has no task `serve`; its tasks are: deploy, format_all
```

## The names of commands

Built-in command names are reserved, so a task named `build` is a compile error.

```
pub fn build() <fs>
```

## A project that has grown

A project that has grown looks like this:

```
serving/
  project.smith
  main.smith
  main_test.smith
  model/
    gpt.smith
    gpt_test.smith
  routes/
    health.smith
  generated/
    routes.smith
  vendor/
    toml/
      parse.smith
```

`project.smith` holds the package, its vendored folders, its build steps, and its tasks. A task that installs handlers and calls `main` is a [deployment](deployments.md). `model/gpt_test.smith` holds the tests of `model/gpt.smith`, as [Tests](tests.md) describes.

## The forms

| Written | Means |
| --- | --- |
| `build(package, target: t)` | The package built for `t`, as a `Built`, or the errors of the build |
| `pub fn name(...)` in `project.smith` | The command `smith name` |
| `--env staging` | The parameter `env`, given `"staging"` |
| `--batch_size 64` | The parameter `batch_size`, spelled as it is in the source |
| `verbose: bool` | A bare flag, `--verbose` |
| `region: str = "eu"` or `note: Option[str]` | A flag that may be left out |
| `<exec>` | The task may run another program |
| `smith --watch name --flag value` | `smith name --flag value`, run again each time a file it was built from changes, with a task that carries `diverge` stopped and its sockets kept |
| A task named like a built-in command | A compile error |
| `smith name`, with no task `name` | The one reason: no `project.smith`, one that does not compile, or no such task |

Next: [Tests](tests.md)
