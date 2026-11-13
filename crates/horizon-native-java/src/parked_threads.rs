//! The threads a jar started, named in the run's report.
//!
//! The interpreter runs every call on the Rust stack, so it can't run a second
//! thread beside the one it is on (decided 2026-11-13). `Thread.start` stays a
//! counted unknown call, and the started thread's body never runs. A bare
//! `java/lang/Thread#start()V` count didn't say *what* never ran, though. Forge
//! Config API Port's night-config `FileWatcher` starts a watcher thread
//! (`FileWatcher$FsWatcher`) for every config it opens. That thread does
//! nothing observable inside the run (it waits for a file change), but an
//! update checker or a metrics server is a different story. The report has to
//! say which one it was.
//!
//! So `new Thread(..)` keeps the `Runnable` (and the name, when the
//! constructor is given one), and `start()` records a [`ParkedThread`] beside
//! the count. The probe's `--json` report lists them as `parked_threads`, and
//! the game's mod logs each one (`parked thread ..: started, its body never
//! runs`).
//!
//! The `Runnable` is named by what the runtime holds:
//! * an object of a jar class: its internal class name
//!   (`com/electronwill/nightconfig/core/file/FileWatcher$FsWatcher`);
//! * a lambda or method reference: its target, `owner#name`;
//! * `null` (no `Runnable`, so `Thread.run()` does nothing): `null`;
//! * any other host value: the class part of its marker, else the marker.
//!
//! A thread made without a name gets `Thread-<n>` from a JVM-wide counter,
//! which depends on every unnamed thread the whole game made, so its name is
//! reported as unknown (`None`) rather than invented.

use super::*;
use crate::jvm_dispatch::constructed_object_class;

/// A thread a jar started whose body the runtime never ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParkedThread {
    /// The name the jar gave the thread's constructor, if it gave one.
    pub name: Option<String>,
    /// What the thread would have run (see the module docs).
    pub runnable: String,
}

/// The `Runnable` and name each constructed `Thread` marker was given.
#[derive(Debug, Clone, Default)]
pub(crate) struct ThreadTable {
    made: BTreeMap<String, ParkedThread>,
    parked: Vec<ParkedThread>,
    /// How many of `parked` [`StdHost::take_new_parked_threads`] handed out.
    reported: usize,
}

/// How the report names a `Runnable` value.
fn runnable_name(value: &Value) -> String {
    let Some(marker) = expect_ref(value) else {
        return "null".into();
    };
    if marker.starts_with("@indy:") {
        // `@indy:<source>:<index>:<owner#name(descriptor)>:<captures>`.
        if let Some(target) = marker.split(':').nth(3) {
            return target
                .split_once('(')
                .map_or(target, |(name, _)| name)
                .to_owned();
        }
    }
    constructed_object_class(marker)
        .or_else(|| marker.split_once('#').map(|(class, _)| class))
        .unwrap_or(marker)
        .to_owned()
}

impl StdHost {
    /// `new Thread(..)`: keep the `Runnable` and the name the constructor was
    /// given. Every `Thread` constructor takes the `Runnable` as its first
    /// `Runnable` parameter and the name as its first `String` one.
    pub(crate) fn note_thread_made(&mut self, marker: &str, descriptor: &str, args: &[Value]) {
        let parameters = descriptor_arguments(descriptor);
        let at = |wanted: &str| {
            parameters
                .iter()
                .position(|parameter| parameter == wanted)
                .and_then(|index| args.get(index))
        };
        let runnable = at("Ljava/lang/Runnable;").map_or_else(|| "null".into(), runnable_name);
        let name = at("Ljava/lang/String;").and_then(expect_ref).cloned();
        self.threads
            .made
            .insert(marker.to_owned(), ParkedThread { name, runnable });
    }

    /// `thread.start()`: the body never runs, so name it in the report. The
    /// caller still counts the call.
    pub(crate) fn note_thread_parked(&mut self, marker: &str) {
        let thread = self
            .threads
            .made
            .get(marker)
            .cloned()
            .unwrap_or(ParkedThread {
                name: None,
                runnable: "unknown".into(),
            });
        self.threads.parked.push(thread);
    }

    /// The threads the jar started whose bodies never ran, in start order.
    pub fn parked_threads(&self) -> &[ParkedThread] {
        &self.threads.parked
    }

    /// The threads parked since the last call, for a caller that logs each
    /// one once (the game's mod, after every phase).
    pub fn take_new_parked_threads(&mut self) -> Vec<ParkedThread> {
        let new = self.threads.parked[self.threads.reported..].to_vec();
        self.threads.reported = self.threads.parked.len();
        new
    }
}
