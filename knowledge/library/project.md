---
type: specification
title: Project
description: std.project, whose Package is what project.smith says a project is called.
status: partial
built:
  - "`std.project` with `Package`, a struct of its `name`, which `project.smith` writes as `package = Package(name: \"calc\")` and which the compiler reads for the name of the executable of a task"
  - "not `build`, `Built`, or `BuildError`, which wait on a task that calls the compiler"
---

# Project

`std.project` holds what `project.smith` is written with, as [Projects](../language/projects.md) and [Project tasks](../language/project-tasks.md) describe.

```
use std.project (Package)

package = Package(name: "calc")
```

`Package` has the `name` of the package. The executable of a [task](../language/project-tasks.md) is named after it, as [Projects](../language/projects.md#the-names-a-project-gives) says.
