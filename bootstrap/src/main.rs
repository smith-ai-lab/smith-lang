//! The `smith` command: `build`, `run`, and `check`, over the compiler in
//! `lib.rs`.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use smith::diag;
use smith::project;
use smith::target::Target;
use smith::timings::Timings;

const USAGE: &str =
    "usage: smith [--timings] build [--target <target>] [--map] [<file>.smith | <task>]
       smith [--timings] <task> [--<flag> <value>...]
       smith [--timings] run [--target <target>] [<file>.smith [<word>...]]
       smith [--timings] check [<file>.smith]
       smith --version";

/// The version a release was built as, set by the release workflow. A build
/// that is not a release has none.
const VERSION: &str = match option_env!("SMITH_VERSION") {
    Some(version) => version,
    None => "dev",
};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match command(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// What a `build` or `run` command line asks for.
struct Request {
    target: Target,
    entry: PathBuf,
    /// The task of `project.smith` the executable starts at, when a name that is
    /// not a file's was given.
    task: Option<String>,
    timings: bool,
    /// Whether `smith build` writes the map of the executable beside it.
    map: bool,
    /// What follows the file's name in `smith run`, which are the words the
    /// program is started with.
    words: Vec<OsString>,
}

/// An argument of the compiler's own, which is text. The words a program is
/// started with are not, and are never read as text here.
fn text(arg: &OsStr) -> Result<&str, String> {
    arg.to_str().ok_or_else(|| {
        format!(
            "the argument `{}` is not valid UTF-8",
            arg.to_string_lossy()
        )
    })
}

/// The error for a flag of `smith` written after the command.
fn flag_of_smith(flag: &str, command: &str) -> String {
    format!("`{flag}` is a flag of `smith`, written before the command: smith {flag} {command}")
}

fn command(args: &[OsString]) -> Result<ExitCode, String> {
    // The flags of `smith` come right after `smith`, before the command.
    let mut timings = false;
    let mut args = args;
    while let Some((flag, rest)) = args.split_first() {
        let flag = text(flag)?;
        match flag {
            "--version" if rest.is_empty() && !timings => {
                println!("smith {VERSION}");
                return Ok(ExitCode::SUCCESS);
            }
            "--timings" if timings => return Err("`--timings` is given twice".to_string()),
            "--timings" => timings = true,
            "--watch" => return Err("smith --watch is not built yet".to_string()),
            "--target" => {
                return Err(
                    "`--target` is a flag of `build` and `run`, written after the command: smith build --target <target>"
                        .to_string(),
                );
            }
            _ if flag.starts_with('-') => {
                return Err(format!(
                    "unknown flag `{flag}`; the flags of smith are --timings and --watch\n{USAGE}"
                ));
            }
            _ => break,
        }
        args = rest;
    }
    let Some((name, rest)) = args.split_first() else {
        return Err(USAGE.to_string());
    };
    let name = text(name)?;
    match name {
        "build" => {
            let request = request(rest, false, timings)?;
            build(&request)?;
            Ok(ExitCode::SUCCESS)
        }
        "run" => {
            let request = request(rest, true, timings)?;
            if Target::host() != Some(request.target) {
                return Err(format!(
                    "smith run runs only programs for this machine, and {} is not its target",
                    request.target.name()
                ));
            }
            let executable = build(&request)?;
            run(&executable, &request.words)
        }
        "check" => check(rest, timings),
        "new" | "test" | "fmt" | "vendor" | "hole" | "find" | "deps" | "cost" => {
            Err(format!("smith {name} is not supported yet"))
        }
        task => run_task(task, rest, timings),
    }
}

/// What a command line asks for. When `with_words`, every word after the
/// file's name is for the program and no flag is read among them.
fn request(args: &[OsString], with_words: bool, timings: bool) -> Result<Request, String> {
    let mut target = None;
    let mut entry = None;
    let mut task = None;
    let mut map = false;
    let mut words = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let arg = text(arg)?;
        if arg == "--target" {
            let Some(name) = args.next() else {
                return Err("--target needs a target name".to_string());
            };
            let name = text(name)?;
            target = Some(Target::from_name(name).ok_or_else(|| {
                let names: Vec<_> = Target::ALL.iter().map(|t| t.name()).collect();
                format!(
                    "unknown target `{name}`; the targets are {}",
                    names.join(", ")
                )
            })?);
        } else if arg == "--timings" || arg == "--watch" {
            let command = if with_words { "run" } else { "build" };
            return Err(flag_of_smith(arg, command));
        } else if arg == "--map" && !with_words {
            map = true;
        } else if arg.starts_with("--") {
            return Err(format!("unknown flag `{arg}`\n{USAGE}"));
        } else if entry.is_some() || task.is_some() {
            return Err(format!("only one file can be built\n{USAGE}"));
        } else if !with_words && !arg.ends_with(".smith") {
            task = Some(arg.to_string());
        } else {
            entry = Some(PathBuf::from(arg));
            if with_words {
                words = args.by_ref().cloned().collect();
            }
        }
    }
    let target = match target {
        Some(target) => target,
        None => Target::host().ok_or_else(|| {
            format!(
                "this machine, {}, is not a target; choose one with --target",
                Target::host_description()
            )
        })?,
    };
    Ok(Request {
        target,
        entry: entry.unwrap_or_else(|| PathBuf::from("main.smith")),
        task,
        timings,
        map,
        words,
    })
}

/// Checks the entry, `main.smith` unless a file is named, and reports its
/// errors, or else its warnings, writing no file. It fails if there are
/// any. With `--timings` it then prints how long each stage took.
fn check(args: &[OsString], timings: bool) -> Result<ExitCode, String> {
    let mut entry = None;
    for arg in args {
        let arg = text(arg)?;
        if arg == "--timings" || arg == "--watch" {
            return Err(flag_of_smith(arg, "check"));
        } else if arg.starts_with("--") {
            return Err(format!("unknown flag `{arg}`\n{USAGE}"));
        } else if entry.is_some() {
            return Err(format!("only one file can be checked\n{USAGE}"));
        } else {
            entry = Some(PathBuf::from(arg));
        }
    }
    let whole_project = entry.is_none();
    let entry = entry.unwrap_or_else(|| PathBuf::from("main.smith"));
    let (_, source) = read_entry(&entry)?;
    let files = Files::in_current_folder(&entry)?;
    let project = files.project();
    let mut stages = Timings::new();
    let found = match smith::check_project_timed(&source, &project, &mut stages) {
        Ok(warnings) => warnings,
        Err(errors) => errors,
    };
    let mut report = files.render(&entry.to_string_lossy(), &source, &found);
    let mut failed = !found.is_empty();
    // A file that no `use` reached is read too, and what is wrong in it said,
    // though no build reads it.
    if whole_project {
        for name in files.unreached() {
            if files.read.borrow().contains(&name) {
                continue;
            }
            let found = smith::check_module(&name, &project);
            failed |= !found.is_empty();
            report.push_str(&files.render(&entry.to_string_lossy(), &source, &found));
        }
    }
    if timings {
        report.push_str(&stages.report());
        report.push('\n');
    }
    eprint!("{report}");
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// The files of the project a command runs in, which is the folder it runs in:
/// the module `db.transaction` is the file `db/transaction.smith` below it.
struct Files {
    root: PathBuf,
    /// The module the entry is, when its path below the root gives it one.
    entry: Option<String>,
    /// The modules a `use` has read, in the order they were asked for.
    read: RefCell<BTreeSet<String>>,
    /// The text of the module `project`, when a task is built: the file
    /// `project.smith` without the lines the compiler reads no further.
    project: Option<String>,
}

impl Files {
    fn in_current_folder(entry: &Path) -> Result<Files, String> {
        let root = std::env::current_dir().map_err(|e| format!("cannot find this folder: {e}"))?;
        let relative = entry.strip_prefix(&root).unwrap_or(entry);
        let mut parts: Vec<String> = Vec::new();
        for part in relative.with_extension("").components() {
            match part {
                Component::Normal(name) => parts.push(name.to_string_lossy().into_owned()),
                Component::CurDir => {}
                _ => {
                    parts.clear();
                    break;
                }
            }
        }
        let entry = (!parts.is_empty()).then(|| parts.join("."));
        Ok(Files {
            root,
            entry,
            read: RefCell::new(BTreeSet::new()),
            project: None,
        })
    }

    fn project(&self) -> smith::Project<'_> {
        smith::Project {
            files: self,
            entry: self.entry.as_deref(),
        }
    }
}

impl smith::Source for Files {
    /// The text of the file of the module with the name, which is noted as
    /// read.
    fn source(&self, module: &str) -> Option<String> {
        if let (Some(text), "project") = (&self.project, module) {
            self.read.borrow_mut().insert(module.to_string());
            return Some(text.clone());
        }
        let path = self
            .root
            .join(format!("{}.smith", module.replace('.', "/")));
        let bytes = std::fs::read(path).ok()?;
        self.read.borrow_mut().insert(module.to_string());
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Files {
    /// Writes errors, showing the line of a module from its file.
    fn render(&self, file: &str, source: &[u8], errors: &[diag::Error]) -> String {
        diag::render_with(file, source, errors, &|module| match module {
            path if path.ends_with(".smith") => std::fs::read(self.root.join(path)).map_or_else(
                |_| String::new(),
                |bytes| String::from_utf8_lossy(&bytes).into_owned(),
            ),
            std => smith::library_text(std).to_string(),
        })
    }

    /// The modules of the project's files that no `use` has read, which are
    /// all the files below the root but the entry and the hidden folders.
    fn unreached(&self) -> Vec<String> {
        let mut found = BTreeSet::new();
        self.collect(&self.root, &mut Vec::new(), &mut found);
        let read = self.read.borrow();
        found
            .into_iter()
            .filter(|m| !read.contains(m) && Some(m) != self.entry.as_ref())
            .collect()
    }

    fn collect(&self, folder: &Path, above: &mut Vec<String>, found: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                above.push(name);
                self.collect(&path, above, found);
                above.pop();
            } else if let Some(stem) = name.strip_suffix(".smith")
                && !stem.is_empty()
                && !stem.contains('.')
            {
                let mut module = above.clone();
                module.push(stem.to_string());
                found.insert(module.join("."));
            }
        }
    }
}

/// The stem of an entry's name, and its text.
fn read_entry(entry: &Path) -> Result<(&std::ffi::OsStr, Vec<u8>), String> {
    let shown = entry.display();
    let stem = match (entry.extension(), entry.file_stem()) {
        (Some(ext), Some(stem)) if ext == "smith" && !stem.is_empty() => stem,
        _ => return Err(format!("{shown}: a Smith file's name ends in .smith")),
    };
    let source = std::fs::read(entry).map_err(|e| format!("{shown}: {e}"))?;
    Ok((stem, source))
}

/// Builds the entry, and with `--timings` prints how long each stage took:
/// after the errors when the build fails, and before returning when it
/// succeeds.
fn build(request: &Request) -> Result<PathBuf, String> {
    let mut timings = Timings::new();
    let result = build_timed(request, &mut timings);
    match (result, request.timings) {
        (Ok(path), true) => {
            eprintln!("{}", timings.report());
            Ok(path)
        }
        (Err(message), true) => Err(format!("{message}\n{}", timings.report())),
        (result, false) => result,
    }
}

/// Builds the entry into an executable in the current folder, and returns
/// its path.
fn build_timed(request: &Request, timings: &mut Timings) -> Result<PathBuf, String> {
    if let Some(task) = &request.task {
        return build_task(task, request, timings);
    }
    let entry = &request.entry;
    let (stem, source) = read_entry(entry)?;
    let files = Files::in_current_folder(entry)?;
    let built = smith::compile_project_files_timed(
        &source,
        &files.project(),
        request.target,
        request.map,
        timings,
    )
    .map_err(|errors| {
        let file = entry.to_string_lossy();
        let text = files.render(&file, &source, &errors);
        text.strip_suffix('\n').unwrap_or(&text).to_string()
    })?;
    let cwd = files.root.clone();
    let name = if stem == "main" {
        cwd.file_name().unwrap_or(stem)
    } else {
        stem
    };
    let output = cwd.join(request.target.executable_name(&name.to_string_lossy()));
    timings
        .stage("write", || {
            write_executable(&output, &built.executable, request.target.is_machine_code())?;
            for (extension, bytes) in &built.beside {
                write_executable(&output.with_extension(extension), bytes, false)?;
            }
            if let Some(map) = built.map_text() {
                write_executable(&output.with_extension("map"), map.as_bytes(), false)?;
            }
            Ok(())
        })
        .map_err(|e: std::io::Error| format!("{}: {e}", output.display()))?;
    Ok(output)
}

/// Writes the executable, replacing any file already there, and lets the
/// system run it when it is a program for a chip.
fn write_executable(path: &Path, bytes: &[u8], runs: bool) -> std::io::Result<()> {
    if path.is_file() {
        std::fs::remove_file(path)?;
    }
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    if runs {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    let _ = runs;
    Ok(())
}

/// Runs a built executable with the words it is started with, and exits as it
/// does.
fn run(executable: &Path, words: &[OsString]) -> Result<ExitCode, String> {
    let status = std::process::Command::new(executable)
        .args(words)
        .status()
        .map_err(|e| format!("{}: {e}", executable.display()))?;
    let code = status.code().unwrap_or(1);
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
}

/// The nearest folder at or above this one that holds a `project.smith`.
fn find_root() -> Option<PathBuf> {
    let here = std::env::current_dir().ok()?;
    here.ancestors()
        .find(|folder| folder.join("project.smith").is_file())
        .map(Path::to_path_buf)
}

/// What the `project.smith` of the folder says, or the errors in it as they
/// are printed.
fn tasks_of(root: &Path) -> Result<project::Read, String> {
    let file = root.join("project.smith");
    let source = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let text = String::from_utf8_lossy(&source).into_owned();
    project::read(&text).map_err(|errors| {
        let shown = diag::render_with("project.smith", text.as_bytes(), &errors, &|_| text.clone());
        shown.strip_suffix('\n').unwrap_or(&shown).to_string()
    })
}

/// The folder of the nearest `project.smith` and what it says, when it has a
/// task of this name. Otherwise the one reason it has not: there is no
/// `project.smith`, it does not compile, or it has no such task.
fn project_with_task(task: &str) -> Result<(PathBuf, project::Read), String> {
    let Some(root) = find_root() else {
        let here =
            std::env::current_dir().map_or_else(|_| ".".to_string(), |d| d.display().to_string());
        return Err(format!(
            "no project.smith was found in {here} or any folder above it, and `{task}` would be a task of project.smith"
        ));
    };
    let read = tasks_of(&root)?;
    if read.task(task).is_none() {
        let file = root.join("project.smith");
        let names: Vec<&str> = read.tasks.iter().map(|t| t.name.as_str()).collect();
        let has = if names.is_empty() {
            "it has no tasks".to_string()
        } else {
            format!("its tasks are: {}", names.join(", "))
        };
        return Err(format!("{} has no task `{task}`; {has}", file.display()));
    }
    Ok((root, read))
}

/// Builds the executable that starts at a task of `project.smith`, which is
/// named after the package, in the folder that holds the file.
fn build_task(task: &str, request: &Request, timings: &mut Timings) -> Result<PathBuf, String> {
    let (root, read) = project_with_task(task)?;
    let found = read.task(task).expect("project_with_task found it");
    let program = found.program();
    let files = Files {
        root: root.clone(),
        entry: None,
        read: RefCell::new(BTreeSet::new()),
        project: Some(read.module.clone()),
    };
    let built = smith::compile_project_files_timed(
        program.as_bytes(),
        &files.project(),
        request.target,
        request.map,
        timings,
    )
    .map_err(|errors| {
        let errors = found.placed(project::explain(errors));
        let text = files.render("project.smith", program.as_bytes(), &errors);
        text.strip_suffix('\n').unwrap_or(&text).to_string()
    })?;
    let name = read.package.clone().unwrap_or_else(|| {
        root.file_name()
            .map_or_else(|| "main".to_string(), |n| n.to_string_lossy().into_owned())
    });
    let output = root.join(request.target.executable_name(&name));
    timings
        .stage("write", || {
            write_executable(&output, &built.executable, request.target.is_machine_code())?;
            for (extension, bytes) in &built.beside {
                write_executable(&output.with_extension(extension), bytes, false)?;
            }
            Ok(())
        })
        .map_err(|e: std::io::Error| format!("{}: {e}", output.display()))?;
    Ok(output)
}

/// Builds a task for this machine and runs it with the flags it is given.
fn run_task(task: &str, flags: &[OsString], timings: bool) -> Result<ExitCode, String> {
    let (_, read) = project_with_task(task)?;
    let found = read.task(task).expect("project_with_task found it");
    if let Some(flag) = found.flag_of_smith(flags) {
        return Err(flag_of_smith(&flag, task));
    }
    let target = Target::host().ok_or_else(|| {
        format!(
            "this machine, {}, is not a target",
            Target::host_description()
        )
    })?;
    let request = Request {
        target,
        entry: PathBuf::from("main.smith"),
        task: Some(task.to_string()),
        timings,
        map: false,
        words: flags.to_vec(),
    };
    let executable = build(&request)?;
    run(&executable, flags)
}
