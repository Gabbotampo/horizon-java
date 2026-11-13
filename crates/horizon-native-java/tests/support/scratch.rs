#![allow(dead_code)]

//! A scratch directory a fixture owns, removed when the guard drops.
//!
//! Cargo runs the tests of one binary on parallel threads of one process, so
//! a directory named only after the process id is shared by every test that
//! builds into it: one test's `remove_dir_all` deleted another's half-built
//! jar (a flaky `NotFound`, horizon-java PR #146). Each [`Scratch`] gets its
//! own `horizon-native-java-<label>-<pid>-<n>` directory, `n` counted per
//! process, and removes it on drop, also when the test panics, so the suites
//! no longer leave thousands of directories behind in `TMPDIR`.
//!
//! Include it with `#[path = "support/scratch.rs"] mod scratch;`.

use std::{
    ffi::OsStr,
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh, empty directory under [`std::env::temp_dir`], deleted with
/// everything in it when this guard drops.
///
/// It derefs to a path inside the directory: the directory itself from
/// [`Scratch::new`], or one entry of it after [`Scratch::into_path`], so a
/// helper can hand back "the jar it built" while the guard keeps the jar's
/// directory alive exactly as long as the caller holds it.
pub struct Scratch {
    root: PathBuf,
    path: PathBuf,
}

impl Scratch {
    /// Create `horizon-native-java-<label>-<pid>-<n>`, unique to this call.
    pub fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // A process killed before its guards dropped can leave a directory
        // whose name a later process with the same pid would reuse.
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self {
            path: root.clone(),
            root,
        }
    }

    /// The directory this guard removes.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Point the guard at `path`, which lies inside its directory (e.g. the
    /// jar a helper built there); the whole directory is still removed on
    /// drop.
    pub fn into_path(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        assert!(
            path.starts_with(&self.root),
            "{path:?} is outside {:?}",
            self.root
        );
        self.path = path;
        self
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<OsStr> for Scratch {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}
