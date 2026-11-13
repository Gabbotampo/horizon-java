//! `WatchService` over the file model, delivering the events JDK 25's Linux
//! watch service delivers for the run's own file changes.
//!
//! The old model answered `poll()` with null and built keys that "will never
//! report an event", on the grounds that nothing writes behind the mod's back.
//! But the mod itself writes. On a JVM, a process that writes, creates,
//! deletes or moves a file in a directory it watches gets those events back
//! (inotify does not filter by process). A config mod that saves its file and
//! then polls sees `ENTRY_MODIFY` and reloads. Answering "no event" there was
//! a silent wrong answer.
//!
//! **What JDK 25 delivers.** Measured with `jdk-25.0.5+8` on Linux
//! (`sun.nio.fs.LinuxWatchService`, elysium), and read from its `src.zip`
//! (`LinuxWatchService`, `AbstractPoller`, `AbstractWatchService`,
//! `AbstractWatchKey`). Each system call the JDK makes raises one inotify
//! event in the file's directory:
//!
//! | operation | events, in order |
//! |---|---|
//! | open for writing with truncation (`writeString`, `write`, `newBufferedWriter`, `newOutputStream`, `FileWriter`, `FileOutputStream`) | `ENTRY_CREATE` if the file was new, else `ENTRY_MODIFY` (the truncation, even of an empty file) |
//! | open for appending | `ENTRY_CREATE` if the file was new, else nothing |
//! | each write of at least one byte (a writer's flush or close) | `ENTRY_MODIFY` |
//! | `createFile`, `createDirectory`, `File.mkdir`, `File.createNewFile` | `ENTRY_CREATE` |
//! | `createDirectories`, `File.mkdirs` | `ENTRY_CREATE` for each level that was missing, in its parent |
//! | `delete`, `deleteIfExists`, `File.delete` of an existing path | `ENTRY_DELETE` |
//! | `move` (a rename) | `ENTRY_DELETE` of the source, then `ENTRY_CREATE` of the target; a non-atomic `REPLACE_EXISTING` deletes an existing target first |
//! | `copy` | `ENTRY_DELETE` of an existing target under `REPLACE_EXISTING`, `ENTRY_CREATE`, then `ENTRY_MODIFY` if there were bytes or `COPY_ATTRIBUTES` |
//!
//! A key's mask filters these: `ENTRY_CREATE` covers creates and renames in,
//! `ENTRY_DELETE` deletes and renames out, `ENTRY_MODIFY` writes and attribute
//! changes. `AbstractWatchKey.signalEvent` then merges them:
//! * an event of the same kind and context as the last pending one only
//!   increments that event's count, as does any event after an `OVERFLOW`;
//! * an `ENTRY_MODIFY` increments the pending `ENTRY_MODIFY` of the same
//!   context unless another kind of event for that context came after it;
//! * past 512 pending events, the list is replaced by one `OVERFLOW`.
//!
//! The first event signals the key. A signalled key is queued once on its
//! service, and `reset` queues it again while events are pending. When a
//! watched directory is deleted, its key is invalidated and signalled.
//!
//! **What is exact here, and what is counted.**
//! * Events are delivered from the model's own changes, with the kinds,
//!   order, contexts and merges above. Paths are the file model's: two
//!   spellings of one file are two files, as for every other model call.
//! * `poll()`, `poll(timeout, unit)` and `take()` answer the settled state:
//!   every event of every change made before the call. **Named departure:** a
//!   JVM's poller thread delivers asynchronously. Measured on the oracle, a
//!   `poll()` right after `writeString` found the key in 169 of 200 runs, and
//!   a `poll(1, SECONDS)` batch lacked the trailing `ENTRY_MODIFY` in 10 of
//!   200. The settled answer is the one the JVM gives once its poller has
//!   caught up.
//! * `WatchEvent.count()` is answered only for an `ENTRY_CREATE`/
//!   `ENTRY_DELETE` made of a single event. Otherwise the JVM's count depends
//!   on timing: the kernel merges identical unread events, and `S2` on the
//!   oracle (three writes) counted 2. Writes also split by buffer size. Those
//!   cases are counted.
//! * A poll/take/pollEvents is counted while a writer over a file in a
//!   watched directory holds text it hasn't flushed. Whether the JVM has
//!   written that text yet depends on the writer's buffer.
//! * A change the model can't map to exact events (a `commons-io` write,
//!   moving a directory, `Files.write` of lines) marks the keys watching that
//!   directory, and their later polls and `pollEvents` are counted.
//! * `take()` with nothing pending would block for ever: counted. Modifiers,
//!   a null kind array, a null `TimeUnit` and a `WatchService` that is not
//!   this model's are counted too.

use super::*;
use std::collections::VecDeque;

const KIND_PREFIX: &str = "watch-kind/";
const SERVICE_PREFIX: &str = "watch-service/";
const KEY_PREFIX: &str = "watch-key/";
const EVENT_PREFIX: &str = "watch-event/";
/// `AbstractWatchKey`'s default `MAX_EVENT_LIST_SIZE`.
const MAX_EVENTS: usize = 512;

/// One of `StandardWatchEventKinds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Create,
    Delete,
    Modify,
    Overflow,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Create => "ENTRY_CREATE",
            Kind::Delete => "ENTRY_DELETE",
            Kind::Modify => "ENTRY_MODIFY",
            Kind::Overflow => "OVERFLOW",
        }
    }
}

/// A change the file model made to a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    Create,
    Delete,
    Modify,
    /// A change whose exact events the model can't name.
    Unmodelled,
}

#[derive(Clone, Debug)]
struct Event {
    kind: Kind,
    /// The file name relative to the watched directory; none for `OVERFLOW`.
    context: Option<String>,
    /// How many raw events merged into this one.
    count: u32,
}

#[derive(Clone, Debug)]
struct Key {
    service: String,
    /// The watched directory, as the model names it.
    directory: String,
    /// The `Path` the key was first registered with (`watchable()`).
    watchable: Value,
    kinds: Vec<Kind>,
    valid: bool,
    signalled: bool,
    events: Vec<Event>,
    /// Pending `ENTRY_MODIFY` events by context: indexes into `events`.
    last_modify: BTreeMap<String, usize>,
    /// A change in this directory had no exact events: count what it reports.
    inexact: bool,
}

#[derive(Clone, Debug, Default)]
struct Service {
    closed: bool,
    ready: VecDeque<String>,
}

/// Every watch service, key and delivered event of a run, and what the
/// writers wrote so far.
#[derive(Clone, Debug, Default)]
pub(crate) struct WatchState {
    services: BTreeMap<String, Service>,
    keys: BTreeMap<String, Key>,
    /// Events `pollEvents` handed out: frozen once delivered.
    delivered: BTreeMap<String, Event>,
    next: usize,
    /// Files a writer opened (so a JVM created them) that the model holds no
    /// text for yet.
    opened: BTreeSet<String>,
    /// How much of each file writer's text has reached the file.
    written: BTreeMap<String, usize>,
}

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

/// `(directory, name)` of a path, as the model names them.
fn split(path: &str) -> (String, String) {
    let path = path.trim_end_matches('/');
    match path.rsplit_once('/') {
        Some(("", name)) => ("/".into(), name.into()),
        Some((directory, name)) => (directory.into(), name.into()),
        None => (String::new(), path.into()),
    }
}

/// A directory as keys name it: no trailing slash, `"."` as `""`.
fn directory_name(path: &str) -> String {
    match path.trim_end_matches('/') {
        "" if path.starts_with('/') => "/".into(),
        "." => String::new(),
        other => other.into(),
    }
}

impl WatchState {
    fn marker(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }

    /// `AbstractWatchKey.signalEvent`, then `signal`.
    fn signal_event(&mut self, key_marker: &str, kind: Kind, context: Option<String>) {
        let Some(key) = self.keys.get_mut(key_marker) else {
            return;
        };
        let (mut kind, mut context) = (kind, context);
        if let Some(last) = key.events.last_mut() {
            if last.kind == Kind::Overflow || (last.kind == kind && last.context == context) {
                last.count += 1;
                return;
            }
            if let Some(context) = &context {
                if kind == Kind::Modify {
                    if let Some(&at) = key.last_modify.get(context) {
                        key.events[at].count += 1;
                        return;
                    }
                } else {
                    key.last_modify.remove(context);
                }
            }
            if key.events.len() >= MAX_EVENTS {
                kind = Kind::Overflow;
                context = None;
            }
        }
        if kind == Kind::Overflow {
            key.events.clear();
            key.last_modify.clear();
        } else if kind == Kind::Modify
            && let Some(context) = &context
        {
            key.last_modify.insert(context.clone(), key.events.len());
        }
        key.events.push(Event {
            kind,
            context,
            count: 1,
        });
        self.signal(key_marker);
    }

    /// Queue a ready key on its service.
    fn signal(&mut self, key_marker: &str) {
        let Some(key) = self.keys.get_mut(key_marker) else {
            return;
        };
        if key.signalled {
            return;
        }
        key.signalled = true;
        let service = key.service.clone();
        if let Some(service) = self.services.get_mut(&service) {
            service.ready.push_back(key_marker.to_owned());
        }
    }

    /// The valid keys watching `directory`.
    fn watching(&self, directory: &str) -> Vec<String> {
        self.keys
            .iter()
            .filter(|(_, key)| key.valid && key.directory == directory)
            .map(|(marker, _)| marker.clone())
            .collect()
    }

    /// Whether a writer over a file in `directory` holds text it hasn't
    /// written.
    fn unflushed_in(&self, directory: &str, writers: &BTreeMap<String, String>) -> bool {
        writers.iter().any(|(sink, text)| {
            sink.strip_prefix("file:").is_some_and(|path| {
                split(path).0 == directory
                    && text.len() > self.written.get(path).copied().unwrap_or(0)
            })
        })
    }
}

impl<H: Host> Runtime<'_, H> {
    /// Raise the events a change to `path` makes in the directories watching
    /// it. Every file model call that changes a file reports here.
    pub(crate) fn watch_change(&mut self, path: &str, change: Change) {
        let path = path.trim_end_matches('/');
        if change == Change::Delete {
            self.watch.opened.remove(path);
        }
        if self.watch.keys.is_empty() {
            return;
        }
        let (directory, name) = split(path);
        for key in self.watch.watching(&directory) {
            let kinds = self.watch.keys[&key].kinds.clone();
            let kind = match change {
                Change::Create => Kind::Create,
                Change::Delete => Kind::Delete,
                Change::Modify => Kind::Modify,
                Change::Unmodelled => {
                    if let Some(key) = self.watch.keys.get_mut(&key) {
                        key.inexact = true;
                    }
                    continue;
                }
            };
            if kinds.contains(&kind) {
                self.watch.signal_event(&key, kind, Some(name.clone()));
            }
        }
        // The path is itself a watched directory.
        for key in self.watch.watching(&directory_name(path)) {
            match change {
                // inotify's `IN_IGNORED`: the key is invalidated and signalled.
                Change::Delete => {
                    if let Some(entry) = self.watch.keys.get_mut(&key) {
                        entry.valid = false;
                    }
                    self.watch.signal(&key);
                }
                Change::Unmodelled => {
                    if let Some(entry) = self.watch.keys.get_mut(&key) {
                        entry.inexact = true;
                    }
                }
                Change::Create | Change::Modify => {}
            }
        }
    }

    /// Whether the JVM would have `path` on disk: the model holds it, or a
    /// writer opened it.
    fn watch_file_exists(&self, path: &str) -> bool {
        self.io_files.contains_key(path) || self.watch.opened.contains(path)
    }

    /// A writer opened `path`: `truncate` for a writing open, otherwise an
    /// appending one or a wrapper over an open writer.
    pub(crate) fn watch_writer_opened(&mut self, path: &str, truncate: bool) {
        let existed = self.watch_file_exists(path);
        self.watch.opened.insert(path.to_owned());
        if truncate {
            self.watch.written.insert(path.to_owned(), 0);
            let change = if existed {
                Change::Modify
            } else {
                Change::Create
            };
            self.watch_change(path, change);
        } else if !existed {
            let text = self
                .io_writers
                .get(&format!("file:{path}"))
                .map_or(0, String::len);
            self.watch.written.insert(path.to_owned(), text);
            self.watch_change(path, Change::Create);
        }
    }

    /// A writer's text reached `path` (a flush or close): a write if there
    /// was new text.
    pub(crate) fn watch_writer_committed(&mut self, path: &str, length: usize) {
        let written = self.watch.written.get(path).copied().unwrap_or(0);
        self.watch.written.insert(path.to_owned(), length);
        if length > written {
            self.watch_change(path, Change::Modify);
        }
    }

    /// `Files.writeString`/`write`: a truncating open, then a write if there
    /// are bytes.
    pub(crate) fn watch_whole_write(&mut self, path: &str, empty: bool) {
        let change = if self.watch_file_exists(path) {
            Change::Modify
        } else {
            Change::Create
        };
        self.watch_change(path, change);
        if !empty {
            self.watch_change(path, Change::Modify);
        }
    }

    /// `createDirectories`/`File.mkdirs`: a create for each missing level,
    /// outermost first. Call before the model records `path`.
    pub(crate) fn watch_directories_created(&mut self, path: &str) {
        let path = path.trim_end_matches('/');
        let mut levels = Vec::new();
        let mut at = path.to_owned();
        while !at.is_empty() && at != "/" {
            if self.file_kind(&at) != file_attributes::FileKind::Missing {
                break;
            }
            levels.push(at.clone());
            at = split(&at).0;
        }
        for level in levels.iter().rev() {
            self.watch_change(level, Change::Create);
        }
    }

    /// `File.mkdir`/`createDirectory`/`createFile`/`createNewFile`: a create
    /// if the path was missing. Call before the model records it.
    pub(crate) fn watch_created(&mut self, path: &str) {
        if self.file_kind(path) == file_attributes::FileKind::Missing {
            self.watch_change(path, Change::Create);
        }
    }

    /// A delete of `path`: an event if it existed. Call before the model
    /// forgets it.
    pub(crate) fn watch_deleted(&mut self, path: &str) {
        if self.file_kind(path) != file_attributes::FileKind::Missing
            || self.watch.opened.contains(path)
        {
            self.watch_change(path, Change::Delete);
        }
    }

    /// `Files.move`/`copy` from `source` to `target`, after JDK 25's checks
    /// passed and before the model changes. A moved directory is counted:
    /// the model moves its entry, not what it holds.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn watch_move_or_copy(
        &mut self,
        source: &str,
        target: &str,
        moving: bool,
        atomic: bool,
        directory: bool,
        attributes: bool,
        empty: bool,
    ) {
        let target_existed = self.file_kind(target) != file_attributes::FileKind::Missing;
        if moving {
            if directory {
                self.watch_change(source, Change::Unmodelled);
                self.watch_change(target, Change::Unmodelled);
                return;
            }
            // A non-atomic `REPLACE_EXISTING` deletes the target, then renames.
            if !atomic && target_existed {
                self.watch_change(target, Change::Delete);
            }
            self.watch_change(source, Change::Delete);
            self.watch_change(target, Change::Create);
        } else {
            if target_existed {
                self.watch_change(target, Change::Delete);
            }
            self.watch_change(target, Change::Create);
            // The bytes, and `COPY_ATTRIBUTES`' owner, mode and times.
            if !empty || attributes {
                self.watch_change(target, Change::Modify);
            }
        }
    }

    /// Answer a watch-service call; `Ok(None)` for any other call.
    pub(super) fn invoke_watch_service(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(marker) = receiver.and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        // Every path the model names is on the default filesystem.
        if (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str())
            == (
                "java/nio/file/Path",
                "getFileSystem",
                "()Ljava/nio/file/FileSystem;",
            )
        {
            return Ok(Some(Value::Ref(Some("file-system/default".into()))));
        }
        if marker == "file-system/default"
            && (r.name.as_str(), r.descriptor.as_str())
                == ("newWatchService", "()Ljava/nio/file/WatchService;")
        {
            let service = self.watch.marker(SERVICE_PREFIX);
            self.watch
                .services
                .insert(service.clone(), Service::default());
            return Ok(Some(Value::Ref(Some(service))));
        }
        if r.name == "register"
            && matches!(
                r.owner.as_str(),
                "java/nio/file/Path" | "java/nio/file/Watchable"
            )
            && r.descriptor
                .starts_with("(Ljava/nio/file/WatchService;[Ljava/nio/file/WatchEvent$Kind;")
        {
            return self.watch_register(r, receiver.cloned().unwrap_or(Value::Ref(None)), args);
        }
        if marker.starts_with(SERVICE_PREFIX) && self.watch.services.contains_key(&marker) {
            return self.watch_service_call(r, &marker, args);
        }
        if marker.starts_with(KEY_PREFIX) && self.watch.keys.contains_key(&marker) {
            return self.watch_key_call(r, &marker);
        }
        if let Some(event) = self.watch.delivered.get(&marker).cloned() {
            return Ok(match (r.name.as_str(), r.descriptor.as_str()) {
                ("kind", "()Ljava/nio/file/WatchEvent$Kind;") => Some(Value::Ref(Some(format!(
                    "{KIND_PREFIX}{}",
                    event.kind.name()
                )))),
                ("context", "()Ljava/lang/Object;") => Some(
                    event
                        .context
                        .map_or(Value::Ref(None), |name| path_value(&name)),
                ),
                ("count", "()I") => {
                    if event.count == 1 && matches!(event.kind, Kind::Create | Kind::Delete) {
                        Some(Value::Int(1))
                    } else {
                        Some(self.host.unknown_operand_call(r)?)
                    }
                }
                _ => None,
            });
        }
        Ok(None)
    }

    /// `Path.register(service, kinds[, modifiers])`, in JDK 25's order of
    /// checks (`UnixPath.register`, `AbstractPoller.register`,
    /// `LinuxWatchService.implRegister`).
    fn watch_register(
        &mut self,
        r: &MethodRef,
        directory: Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let service = match StdHost::argument(r, args, 0)? {
            Value::Ref(None) => return Err(thrown("java/lang/NullPointerException", None)),
            Value::Ref(Some(service)) if self.watch.services.contains_key(service) => {
                service.clone()
            }
            // A `WatchService` of the jar's own, or one the model has no state for.
            _ => return self.host.unknown_operand_call(r).map(Some),
        };
        let kinds = match StdHost::argument(r, args, 1)? {
            Value::Ref(None) => return self.host.unknown_operand_call(r).map(Some),
            value => self.array(value)?.clone(),
        };
        let mut wanted = Vec::new();
        for kind in &kinds {
            let kind = match expect_ref(kind).map(String::as_str) {
                None => {
                    return Err(thrown(
                        "java/lang/NullPointerException",
                        Some("An element in event set is 'null'".into()),
                    ));
                }
                Some("watch-kind/ENTRY_CREATE") => Kind::Create,
                Some("watch-kind/ENTRY_DELETE") => Kind::Delete,
                Some("watch-kind/ENTRY_MODIFY") => Kind::Modify,
                Some("watch-kind/OVERFLOW") => continue,
                // A `WatchEvent.Kind` of the jar's own.
                Some(_) => return self.host.unknown_operand_call(r).map(Some),
            };
            if !wanted.contains(&kind) {
                wanted.push(kind);
            }
        }
        if wanted.is_empty() {
            return Err(thrown(
                "java/lang/IllegalArgumentException",
                Some("No events to register".into()),
            ));
        }
        if self.watch.services[&service].closed {
            return Err(thrown("java/nio/file/ClosedWatchServiceException", None));
        }
        if let Some(modifiers) = args.get(2)
            && !matches!(modifiers, Value::Ref(Some(_)) if self.array(modifiers)?.is_empty())
        {
            return self.host.unknown_operand_call(r).map(Some);
        }
        let path = path_text(&directory).to_owned();
        match self.file_kind(&path) {
            file_attributes::FileKind::Missing => {
                return Err(thrown("java/nio/file/NoSuchFileException", Some(path)));
            }
            file_attributes::FileKind::RegularFile => {
                return Err(thrown("java/nio/file/NotDirectoryException", Some(path)));
            }
            file_attributes::FileKind::Directory => {}
        }
        let directory_key = directory_name(&path);
        // Registering a watched directory again replaces its event set.
        if let Some((marker, key)) =
            self.watch.keys.iter_mut().find(|(_, key)| {
                key.valid && key.service == service && key.directory == directory_key
            })
        {
            key.kinds = wanted;
            return Ok(Some(Value::Ref(Some(marker.clone()))));
        }
        let marker = self.watch.marker(KEY_PREFIX);
        self.watch.keys.insert(
            marker.clone(),
            Key {
                service,
                directory: directory_key,
                watchable: directory,
                kinds: wanted,
                valid: true,
                signalled: false,
                events: Vec::new(),
                last_modify: BTreeMap::new(),
                inexact: false,
            },
        );
        Ok(Some(Value::Ref(Some(marker))))
    }

    /// `poll`, `poll(timeout, unit)`, `take` and `close`.
    fn watch_service_call(
        &mut self,
        r: &MethodRef,
        service: &str,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let blocking = match (r.name.as_str(), r.descriptor.as_str()) {
            ("close", "()V") => {
                if let Some(entry) = self.watch.services.get_mut(service) {
                    entry.closed = true;
                    entry.ready.clear();
                }
                for key in self.watch.keys.values_mut() {
                    if key.service == service {
                        key.valid = false;
                    }
                }
                return Ok(Some(Value::Void));
            }
            ("poll", "()Ljava/nio/file/WatchKey;") => false,
            ("poll", "(JLjava/util/concurrent/TimeUnit;)Ljava/nio/file/WatchKey;") => {
                if matches!(args.get(1), None | Some(Value::Ref(None))) {
                    return self.host.unknown_operand_call(r).map(Some);
                }
                false
            }
            ("take", "()Ljava/nio/file/WatchKey;") => true,
            _ => return Ok(None),
        };
        if self.watch.services[service].closed {
            return Err(thrown("java/nio/file/ClosedWatchServiceException", None));
        }
        let unsettled = self.watch.keys.iter().any(|(_, key)| {
            key.service == service
                && (key.inexact || self.watch.unflushed_in(&key.directory, &self.io_writers))
        });
        let next = self
            .watch
            .services
            .get_mut(service)
            .and_then(|entry| entry.ready.pop_front());
        match next {
            // `take` with nothing pending blocks for ever.
            None if blocking => self.host.unknown_operand_call(r).map(Some),
            next => {
                if unsettled {
                    self.host.unknown_operand_call(r)?;
                }
                Ok(Some(Value::Ref(next)))
            }
        }
    }

    /// `pollEvents`, `reset`, `cancel`, `isValid` and `watchable`.
    fn watch_key_call(
        &mut self,
        r: &MethodRef,
        marker: &str,
    ) -> Result<Option<Value>, NativeError> {
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("pollEvents", "()Ljava/util/List;") => {
                let key = &self.watch.keys[marker];
                let unsettled =
                    key.inexact || self.watch.unflushed_in(&key.directory, &self.io_writers);
                let events = self
                    .watch
                    .keys
                    .get_mut(marker)
                    .map(|key| {
                        key.last_modify.clear();
                        std::mem::take(&mut key.events)
                    })
                    .unwrap_or_default();
                let mut values = Vec::with_capacity(events.len());
                for event in events {
                    let event_marker = self.watch.marker(EVENT_PREFIX);
                    self.watch.delivered.insert(event_marker.clone(), event);
                    values.push(Value::Ref(Some(event_marker)));
                }
                if unsettled {
                    self.host.unknown_operand_call(r)?;
                }
                match self.host.empty_mutable_collection("list") {
                    Some(list) => {
                        self.host.replace_mutable_collection(&list, values);
                        list
                    }
                    None => collection_value("list", &values),
                }
            }
            ("reset", "()Z") => {
                let Some(key) = self.watch.keys.get_mut(marker) else {
                    return Ok(None);
                };
                if key.valid && key.signalled {
                    if key.events.is_empty() {
                        key.signalled = false;
                    } else {
                        let service = key.service.clone();
                        if let Some(entry) = self.watch.services.get_mut(&service) {
                            entry.ready.push_back(marker.to_owned());
                        }
                    }
                }
                Value::Int(i32::from(self.watch.keys[marker].valid))
            }
            ("cancel", "()V") => {
                if let Some(key) = self.watch.keys.get_mut(marker) {
                    key.valid = false;
                }
                Value::Void
            }
            ("isValid", "()Z") => Value::Int(i32::from(self.watch.keys[marker].valid)),
            ("watchable", "()Ljava/nio/file/Watchable;") => {
                self.watch.keys[marker].watchable.clone()
            }
            _ => return Ok(None),
        }))
    }
}
