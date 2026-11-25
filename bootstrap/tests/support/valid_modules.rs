//! Every module the compiler writes for WebAssembly is checked by `node`
//! before `emit` gives it back, so a module of the wrong types fails the test
//! that built it, and not a later load of it. One `node` lives for the run and
//! is asked about each module, as one for each would cost the time of its
//! start for each of the thousand modules the tests build. A machine with no
//! `node` checks nothing, as the tests that run a module are skipped there.

use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

/// Reads each module as its length in four bytes and its bytes, and says `ok`
/// or why the engine takes it for invalid, in one line.
const SCRIPT: &str = r#"
let held = Buffer.alloc(0);
process.stdin.on("data", (chunk) => {
  held = Buffer.concat([held, chunk]);
  for (;;) {
    if (held.length < 4) break;
    const size = held.readUInt32LE(0);
    if (held.length < 4 + size) break;
    const bytes = held.subarray(4, 4 + size);
    held = held.subarray(4 + size);
    let said = "ok";
    try {
      new WebAssembly.Module(bytes);
    } catch (e) {
      said = "error: " + String(e.message).replace(/\s+/g, " ");
    }
    process.stdout.write(said + "\n");
  }
});
"#;

struct Validator {
    asked: ChildStdin,
    said: BufReader<ChildStdout>,
}

/// The `node` of the run: not started, or not found, or started.
static VALIDATOR: Mutex<Option<Option<Validator>>> = Mutex::new(None);

fn start() -> Option<Validator> {
    let mut node = Command::new("node")
        .arg("-e")
        .arg(SCRIPT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    Some(Validator {
        asked: node.stdin.take()?,
        said: BufReader::new(node.stdout.take()?),
    })
}

pub fn check(module: &[u8]) -> Result<(), String> {
    let mut held = VALIDATOR.lock().unwrap_or_else(|e| e.into_inner());
    let Some(validator) = held.get_or_insert_with(start) else {
        return Ok(());
    };
    validator
        .asked
        .write_all(&(module.len() as u32).to_le_bytes())
        .and_then(|()| validator.asked.write_all(module))
        .and_then(|()| validator.asked.flush())
        .map_err(|e| format!("node was not asked: {e}"))?;
    let mut line = String::new();
    validator
        .said
        .read_line(&mut line)
        .map_err(|e| format!("node did not answer: {e}"))?;
    match line.trim_end() {
        "ok" => Ok(()),
        "" => Err("node ended before it answered".to_string()),
        why => Err(why.to_string()),
    }
}

/// Makes every module a test builds pass `node` before it is given back, with
/// `node` already started and answering. The tests that start first all build a
/// module, and one waits for the start of `node` with the lock held while the
/// others wait for the lock, so a start that is slow on a loaded machine would
/// be taken for a test that does not end. Here it is before any test begins.
/// The time it takes is printed, so that a run says what the start cost.
pub fn install() {
    let begin = std::time::Instant::now();
    let empty = [0, 0x61, 0x73, 0x6d, 1, 0, 0, 0];
    check(&empty).expect("node takes the empty module");
    println!(
        "node started and checked a module in {:.0} ms",
        begin.elapsed().as_secs_f64() * 1000.0
    );
    smith::target::check_modules_with(check);
}
