//! Work that tests share, made once in a run, which is counted in the steps of
//! no test.
//!
//! The steps of a test are those its thread took, which `smith::work` counts.
//! Work made once for many tests is made on the thread of whichever of them
//! asks first, and which one that is depends on how the run's threads were
//! scheduled. So the steps of what is made here are taken off the steps of the
//! thread that made it, and the steps of a test are the same on every run
//! whatever ran before it or beside it. What is shared is made of what tests
//! do on their own, such as lowering a program, so a change that makes it more
//! work is seen in the tests that do it on their own.

use std::cell::Cell;
use std::sync::{Mutex, OnceLock};

thread_local! {
    /// The steps that this thread took to make shared work.
    static SHARED: Cell<u64> = const { Cell::new(0) };
}

/// The steps of this thread that are its own: those the compiler and the
/// evaluator took on it, less those of the shared work it made.
pub fn steps() -> u64 {
    smith::work::steps() - SHARED.get()
}

/// What `make` gives, with the steps it took counted as shared.
fn made<T>(make: impl FnOnce() -> T) -> T {
    let before = steps();
    let value = make();
    SHARED.set(SHARED.get() + (steps() - before));
    value
}

/// A value made once in a run, by the first test that asks for it.
pub struct Once<T>(OnceLock<T>);

impl<T> Once<T> {
    pub const fn new() -> Once<T> {
        Once(OnceLock::new())
    }

    /// The value, which `make` makes if no test has.
    pub fn get(&self, make: impl FnOnce() -> T) -> &T {
        self.0.get_or_init(|| made(make))
    }
}

/// Values made once in a run for each key, by the first test that asks for
/// the key. A test that asks while another makes the value waits for it.
pub struct Keyed<K, V>(Mutex<Vec<(K, V)>>);

impl<K: PartialEq, V: Clone> Keyed<K, V> {
    pub const fn new() -> Keyed<K, V> {
        Keyed(Mutex::new(Vec::new()))
    }

    /// The value for the key, which `make` makes if no test has.
    pub fn get(&self, key: K, make: impl FnOnce() -> V) -> V {
        let mut all = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, value)) = all.iter().find(|(k, _)| *k == key) {
            return value.clone();
        }
        let value = made(make);
        all.push((key, value.clone()));
        value
    }
}
