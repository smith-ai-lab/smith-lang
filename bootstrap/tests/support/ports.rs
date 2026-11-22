//! The ports of the servers a test starts. The tests run on many threads, and
//! every connection a test makes takes a port of the system's own range from
//! the system, so a port that a test chose by asking the system for one and
//! giving it back can be taken before the server binds it. A port here is
//! chosen from a range below the system's, which no connection takes, and no
//! two servers of a run are given the same one. A port that another program
//! holds is still possible, so a server that ends because its port is taken is
//! started again on the next, and a server that ends for any other reason
//! fails the test with what it wrote to standard error.

use std::process::Child;
use std::sync::atomic::{AtomicU32, Ordering};

/// The first port of the range, which is below where Linux (32768) and macOS
/// (49152) take the ports of connections from.
const FIRST: u32 = 20000;
const COUNT: u32 = 10000;
/// How many ports a server is tried on before the test fails.
const TRIES: usize = 16;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A port no other server of this run has been given. Runs at the same time
/// start at other places in the range.
pub fn next() -> u16 {
    let step = NEXT.fetch_add(1, Ordering::Relaxed);
    (FIRST + (std::process::id() % COUNT + step) % COUNT) as u16
}

/// The ports to try for a server, one after another.
pub fn all() -> impl Iterator<Item = u16> {
    std::iter::repeat_with(next)
}

/// How a server that was started on a port came out.
pub enum Start<T> {
    Up(T),
    /// The server ended because the port is held by another program.
    Taken,
    /// The server ended for another reason, said as the failure of the test.
    Failed(String),
}

/// Starts a server on the first of `ports` that it can bind. `start` is given a
/// port and says how the server came out.
pub fn serve<T>(ports: impl IntoIterator<Item = u16>, mut start: impl FnMut(u16) -> Start<T>) -> T {
    for port in ports.into_iter().take(TRIES) {
        match start(port) {
            Start::Up(server) => return server,
            Start::Taken => continue,
            Start::Failed(why) => panic!("{why}"),
        }
    }
    panic!("no server was started in {TRIES} tries: every port it was given was taken")
}

/// What a server that has ended wrote to standard error, for the message of a
/// test that fails because it ended.
pub fn said(child: &mut Child) -> String {
    use std::io::Read;
    let mut said = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut said);
    }
    said
}
