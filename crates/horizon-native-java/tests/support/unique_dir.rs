//! A temporary-directory suffix no other call in this test process shares.
//!
//! Fixtures name their scratch directory after the process id and the clock
//! in nanoseconds, then delete it when they finish. Parallel tests can read the
//! same clock value, and one test's cleanup then deletes the other's files
//! (`No such file or directory` on a class `javac` had just written). A
//! counter keeps every call's directory its own.
//!
//! Include it with `#[path = "support/unique_dir.rs"] mod unique_dir;`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The clock in nanoseconds, then this process's next counter value.
pub fn unique_suffix() -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let next = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("{nanos}-{next}")
}
