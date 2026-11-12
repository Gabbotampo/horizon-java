//! `StandardOpenOption`s handed to `Files`' writers, and closing the stream
//! `Files.newOutputStream` opens, ported from JDK 25.
//!
//! Sound Physics Remastered, Simple Voice Chat and the other `CommentedProperties`
//! configs save with `Files.newOutputStream(path, CREATE, SYNC, TRUNCATE_EXISTING)`,
//! wrap the stream in a `PrintWriter`, and close the stream. The constants were
//! counted unknowns read as `null`, the writers ignored their options, and the
//! stream's `close` was counted too.
//!
//! The option sets are checked the way JDK 25 checks them (`src.zip` of Temurin
//! 25.0.5+8): `FileSystemProvider.newOutputStream` refuses `READ` with
//! `IllegalArgumentException("READ not allowed")` as it copies the array in
//! order, then adds `WRITE`; `UnixChannelFactory.Flags.toFlags` throws a bare
//! `NullPointerException` on a `null` (a `HashSet` iterates `null` first) and
//! `newFileChannel` refuses `APPEND` with `TRUNCATE_EXISTING`. `Files.newBufferedWriter`,
//! `write` and `writeString` all open through `newOutputStream`.
//!
//! The file model (`invoke_io` in `lib.rs`) creates or truncates the file it
//! opens. That is exactly what an option set holding `CREATE` and
//! `TRUNCATE_EXISTING` (with any of `WRITE`, `SYNC`, `DSYNC`, `SPARSE`, which have
//! nothing to show in memory) or no options at all does, whether the file
//! exists or not, so only those sets reach it. Every other set (`APPEND`,
//! `CREATE_NEW`, no `CREATE`, `DELETE_ON_CLOSE`, a `LinkOption`) depends on
//! whether the file exists or does something the model does not keep, and is
//! counted instead of being answered as a truncating write.
//!
//! `close`/`flush` on a stream `Files.newOutputStream` opened commits what its
//! wrappers wrote to the file, as the model's writers do. A `FileOutputStream`
//! the jar constructs itself keeps its counted `close`: `Properties.store`
//! writes nothing in the host's model, and committing that stream would turn
//! the missing text into an empty file.

use super::*;

const FILES: &str = "java/nio/file/Files";
const OPEN_OPTION: &str = "java/nio/file/StandardOpenOption";
const OPTIONS_PARAMETER: &str = "[Ljava/nio/file/OpenOption;)";
/// The synthetic field marking a stream `Files.newOutputStream` opened.
const NIO_STREAM: &str = "#nioOutputStream";

/// `StandardOpenOption`'s constants, in JDK 25's declaration order.
pub(super) const STANDARD_OPEN_OPTIONS: &[&str] = &[
    "READ",
    "WRITE",
    "APPEND",
    "TRUNCATE_EXISTING",
    "CREATE",
    "CREATE_NEW",
    "DELETE_ON_CLOSE",
    "SPARSE",
    "SYNC",
    "DSYNC",
];

/// What JDK 25 makes of a writer's option array.
enum Verdict {
    /// The model's create-or-truncate open is exactly the JDK's.
    CreateOrTruncate,
    /// The JDK throws before it opens anything.
    Throws(NativeError),
    /// The outcome depends on state the model does not keep: count the call.
    Unmodelled,
}

/// Answer a `Files` writer whose options this model checks, or a stream's
/// `close`/`flush`; `None` leaves the call to the rest of the dispatch.
pub(super) fn invoke<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
    class: &str,
    method: &str,
) -> Result<Option<Value>, NativeError> {
    match r.owner.as_str() {
        FILES => invoke_files(rt, r, receiver, args, class, method),
        "java/io/OutputStream" | "java/io/FileOutputStream" => invoke_stream(rt, r, receiver),
        OPEN_OPTION => Ok(value_of(r, args)?),
        _ => Ok(None),
    }
}

/// `StandardOpenOption.valueOf(String)`, javac's synthetic static, as
/// `Enum.valueOf` answers it.
fn value_of(r: &MethodRef, args: &[Value]) -> Result<Option<Value>, NativeError> {
    if (r.name.as_str(), r.descriptor.as_str())
        != (
            "valueOf",
            "(Ljava/lang/String;)Ljava/nio/file/StandardOpenOption;",
        )
    {
        return Ok(None);
    }
    let Some(Value::Ref(name)) = args.first() else {
        return Ok(None);
    };
    let Some(name) = name else {
        return Err(thrown(
            "java/lang/NullPointerException",
            Some("Name is null"),
        ));
    };
    match STANDARD_OPEN_OPTIONS
        .iter()
        .find(|constant| *constant == name)
    {
        Some(constant) => Ok(Some(Value::Ref(Some(format!(
            "enum/{OPEN_OPTION}/{constant}"
        ))))),
        None => Err(thrown(
            "java/lang/IllegalArgumentException",
            Some(&format!(
                "No enum constant java.nio.file.StandardOpenOption.{name}"
            )),
        )),
    }
}

fn invoke_files<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
    class: &str,
    method: &str,
) -> Result<Option<Value>, NativeError> {
    let writer = matches!(
        r.name.as_str(),
        "newOutputStream" | "newBufferedWriter" | "write" | "writeString"
    );
    if !writer
        || !r.descriptor.starts_with("(Ljava/nio/file/Path;")
        || !r.descriptor.contains(OPTIONS_PARAMETER)
    {
        return Ok(None);
    }
    let Some((options, leading)) = args.split_last() else {
        return Ok(None);
    };
    // A `null` path, text, charset or array is checked before (or instead of)
    // the options; the order of those checks is not this model's to answer.
    if leading
        .iter()
        .chain([options])
        .any(|value| *value == Value::Ref(None))
    {
        return Ok(Some(rt.host.unknown_operand_call(r)?));
    }
    let Some(elements) = expect_ref(options).and_then(|marker| rt.arrays.get(marker)) else {
        return Ok(Some(rt.host.unknown_operand_call(r)?));
    };
    match check(elements) {
        Verdict::Throws(error) => Err(error),
        Verdict::Unmodelled => Ok(Some(rt.host.unknown_operand_call(r)?)),
        Verdict::CreateOrTruncate => {
            let opened = rt.invoke_io(r, receiver, args, class, method)?;
            if r.name == "newOutputStream"
                && let Some(marker) = opened.as_ref().and_then(expect_ref)
            {
                rt.fields
                    .insert(format!("{marker}|{NIO_STREAM}"), Value::Int(1));
            }
            Ok(opened)
        }
    }
}

/// Check one option array the way `FileSystemProvider.newOutputStream` and
/// `UnixChannelFactory` do in JDK 25.
fn check(elements: &[Value]) -> Verdict {
    // An empty array is the default `CREATE, TRUNCATE_EXISTING, WRITE`.
    if elements.is_empty() {
        return Verdict::CreateOrTruncate;
    }
    let mut names = Vec::with_capacity(elements.len());
    let mut has_null = false;
    let mut has_other = false;
    for element in elements {
        match element {
            Value::Ref(None) => has_null = true,
            Value::Ref(Some(marker)) => match option_name(marker) {
                // `newOutputStream` throws on `READ` while it copies the
                // array, before any later element is looked at.
                Some("READ") => {
                    return Verdict::Throws(thrown(
                        "java/lang/IllegalArgumentException",
                        Some("READ not allowed"),
                    ));
                }
                Some(name) => names.push(name),
                None => has_other = true,
            },
            _ => has_other = true,
        }
    }
    // `toFlags` iterates a `HashSet`, which yields `null` first.
    if has_null {
        return Verdict::Throws(thrown("java/lang/NullPointerException", None));
    }
    if has_other {
        return Verdict::Unmodelled;
    }
    let has = |name: &str| names.contains(&name);
    if has("APPEND") && has("TRUNCATE_EXISTING") {
        return Verdict::Throws(thrown(
            "java/lang/IllegalArgumentException",
            Some("APPEND + TRUNCATE_EXISTING not allowed"),
        ));
    }
    let silent = |name: &&str| matches!(*name, "WRITE" | "SYNC" | "DSYNC" | "SPARSE");
    if has("CREATE")
        && has("TRUNCATE_EXISTING")
        && names
            .iter()
            .all(|name| silent(name) || matches!(*name, "CREATE" | "TRUNCATE_EXISTING"))
    {
        return Verdict::CreateOrTruncate;
    }
    Verdict::Unmodelled
}

/// The constant a `StandardOpenOption` marker names.
fn option_name(marker: &str) -> Option<&'static str> {
    let name = marker.strip_prefix("enum/")?.strip_prefix(OPEN_OPTION)?;
    let name = name.strip_prefix('/')?;
    STANDARD_OPEN_OPTIONS
        .iter()
        .copied()
        .find(|constant| *constant == name)
}

fn invoke_stream<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
) -> Result<Option<Value>, NativeError> {
    if !matches!(
        (r.name.as_str(), r.descriptor.as_str()),
        ("close" | "flush", "()V")
    ) {
        return Ok(None);
    }
    let Some(marker) = receiver.and_then(expect_ref).cloned() else {
        return Ok(None);
    };
    if !rt.fields.contains_key(&format!("{marker}|{NIO_STREAM}")) {
        return Ok(None);
    }
    rt.io_commit(&marker);
    Ok(Some(Value::Void))
}

fn thrown(class: &str, message: Option<&str>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: message.map(str::to_owned),
        frame: None,
    }
}
