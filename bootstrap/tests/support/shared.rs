//! Work that tests share, made once in a run, which is counted in the steps and
//! the time of no test.
//!
//! The steps of a test are those its thread took, which `smith::work` counts.
//! Work made once for many tests is made on the thread of whichever of them
//! asks first, and which one that is depends on how the run's threads were
//! scheduled. So the steps of what is made here are taken off the steps of the
//! thread that made it, and the steps of a test are the same on every run
//! whatever ran before it or beside it. What is shared is made of what tests
//! do on their own, such as lowering a program, so a change that makes it more
//! work is seen in the tests that do it on their own.
//!
//! The time of a test is its time on the wall, and the same holds for it: the
//! time a thread spends here, making a value or waiting while another thread
//! makes it, is taken off the time of the test it runs. Otherwise the test that
//! asks first for a server that `smith build` builds takes the whole build, and
//! so does every test that asks while it is built, and a run that starts only a
//! few of them, as the time check's second run does, has them all wait for it.

use std::cell::Cell;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

thread_local! {
    /// The steps that this thread took to make shared work.
    static SHARED: Cell<u64> = const { Cell::new(0) };
    /// The time on the wall this thread spent making shared work or waiting for it.
    static WAITED: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    /// How many asks for shared work this thread is in, one inside another.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// The steps of this thread that are its own: those the compiler and the
/// evaluator took on it, less those of the shared work it made.
pub fn steps() -> u64 {
    smith::work::steps() - SHARED.get()
}

/// The time on the wall this thread has spent so far making shared work or
/// waiting for another thread to make it, which is no test's own.
pub fn waited() -> Duration {
    WAITED.get()
}

/// What `make` gives, with the steps it took counted as shared.
fn made<T>(make: impl FnOnce() -> T) -> T {
    let before = steps();
    let value = make();
    SHARED.set(SHARED.get() + (steps() - before));
    value
}

/// Ends an ask for shared work, and counts its time on the wall as shared when
/// it is the outermost, however it ends.
struct Asking(Instant);

impl Drop for Asking {
    fn drop(&mut self) {
        let depth = DEPTH.get() - 1;
        DEPTH.set(depth);
        if depth == 0 {
            WAITED.set(WAITED.get() + self.0.elapsed());
        }
    }
}

/// What `ask` gives, with the time it took on the wall counted as shared. An ask
/// inside another is counted once, in the outer.
fn timed<T>(ask: impl FnOnce() -> T) -> T {
    DEPTH.set(DEPTH.get() + 1);
    let _asking = Asking(Instant::now());
    ask()
}

/// A value made once in a run, by the first test that asks for it. A test that
/// asks while another makes it waits for it.
pub struct Once<T>(OnceLock<T>);

impl<T> Once<T> {
    pub const fn new() -> Once<T> {
        Once(OnceLock::new())
    }

    /// The value, which `make` makes if no test has.
    pub fn get(&self, make: impl FnOnce() -> T) -> &T {
        if let Some(value) = self.0.get() {
            return value;
        }
        timed(|| self.0.get_or_init(|| made(make)))
    }
}

/// Values made once in a run for each key, by the first test that asks for
/// the key. A test that asks while another makes the value of the same key
/// waits for it, and one that asks for another key does not.
pub struct Keyed<K, V>(Mutex<Vec<(K, Arc<OnceLock<V>>)>>);

impl<K: PartialEq, V: Clone> Keyed<K, V> {
    pub const fn new() -> Keyed<K, V> {
        Keyed(Mutex::new(Vec::new()))
    }

    /// The value for the key, which `make` makes if no test has.
    pub fn get(&self, key: K, make: impl FnOnce() -> V) -> V {
        let cell = {
            let mut all = self.0.lock().unwrap_or_else(|e| e.into_inner());
            match all.iter().find(|(k, _)| *k == key) {
                Some((_, cell)) => cell.clone(),
                None => {
                    let cell = Arc::new(OnceLock::new());
                    all.push((key, cell.clone()));
                    cell
                }
            }
        };
        if let Some(value) = cell.get() {
            return value.clone();
        }
        timed(|| cell.get_or_init(|| made(make)).clone())
    }
}
