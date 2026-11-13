//! `String.valueOf(Object)` of a model object: its modelled `toString`.
//!
//! javac (19 and later) lowers every non-`String` concat operand to
//! `String.valueOf(operand)`; older javac passes the object to the concat
//! site; `StringBuilder.append(Object)` is `String.valueOf` too. All three
//! printed the internal marker (`identifier/a:x`, `list/a`). [`owner`] names
//! the class whose modelled `toString` the host answers for a marker, so
//! `"" + x` is the text `x.toString()` gives. A plain string, a box, a builder
//! and a jar object keep their callers' existing paths.

use crate::runtime::{Host, MethodRef, Runtime, Value, expect_ref};

/// The owner whose `toString` answers `marker`, or `None` when no model owns
/// its text.
pub(crate) fn owner(marker: &str) -> Option<String> {
    // A plain string is its own text.
    if crate::runtime::value_class::is_plain_string(marker) {
        return None;
    }
    // The empty optionals carry a bare marker (`EMPTY_OPTIONAL*` in lib.rs).
    match marker {
        crate::runtime::EMPTY_OPTIONAL => return Some("java/util/Optional".to_owned()),
        crate::runtime::EMPTY_OPTIONAL_INT => return Some("java/util/OptionalInt".to_owned()),
        crate::runtime::EMPTY_OPTIONAL_LONG => return Some("java/util/OptionalLong".to_owned()),
        crate::runtime::EMPTY_OPTIONAL_DOUBLE => return Some("java/util/OptionalDouble".to_owned()),
        _ => {}
    }
    // Prefix families the host models before any class marker.
    let prefix = [
        ("identifier/", "net/minecraft/resources/Identifier"),
        ("uuid/", "java/util/UUID"),
        ("path:", "java/nio/file/Path"),
        ("duration/", "java/time/Duration"),
        ("tag-key/", "net/minecraft/tags/TagKey"),
        ("optional/", "java/util/Optional"),
        ("optional-int/", "java/util/OptionalInt"),
        ("optional-long/", "java/util/OptionalLong"),
        ("optional-double/", "java/util/OptionalDouble"),
        ("regex-pattern/", "java/util/regex/Pattern"),
        ("watch-kind/", "java/nio/file/WatchEvent$Kind"),
        ("charset/", "java/nio/charset/Charset"),
        ("coding-error-action/", "java/nio/charset/CodingErrorAction"),
        ("locale/", "java/util/Locale"),
        ("mod-origin/", "net/fabricmc/loader/api/metadata/ModOrigin"),
        ("unicode-block/", "java/lang/Character$UnicodeBlock"),
    ];
    for (prefix, owner) in prefix {
        if marker.starts_with(prefix) {
            return Some(owner.to_owned());
        }
    }
    // A platform enum constant; a jar enum may override `toString`.
    if let Some((class, _)) = marker
        .strip_prefix("enum/")
        .and_then(|rest| rest.rsplit_once('/'))
        && crate::runtime::host_models_class(class)
    {
        return Some("java/lang/Enum".to_owned());
    }
    // A collection marker: `list/a,b`, `set/...`, `map/k,v`, immutable forms
    // `list#i1/...`, and the bare empty kinds.
    let kind = marker.split_once('/').map_or(marker, |(kind, _)| kind);
    let kind = kind.split_once('#').map_or(kind, |(kind, _)| kind);
    match kind {
        "list" => return Some("java/util/List".to_owned()),
        "set" => return Some("java/util/Set".to_owned()),
        "map" => return Some("java/util/Map".to_owned()),
        _ => {}
    }
    // A class marker (`class#n`): only a class the host models has a body to
    // answer, so a jar object keeps the caller's text. A builder's text lives
    // in the runtime ([`Runtime::invoke_value_of_builder`]), not the host.
    if let Some((class, _)) = marker.split_once('#')
        && crate::runtime::host_models_class(class)
        && !matches!(class, "java/lang/StringBuilder" | "java/lang/StringBuffer")
    {
        // An unmodifiable collection prints through its interface's model.
        let interface = crate::runtime::java_immutable::supertypes(class).first();
        return Some(interface.copied().unwrap_or(class).to_owned());
    }
    None
}

/// The text `String.valueOf(value)` gives for a model object.
pub(crate) fn modelled<H: Host>(host: &mut H, value: &Value) -> Option<String> {
    let marker = expect_ref(value)?;
    modelled_marker(host, marker)
}

/// [`modelled`] for a marker that is already known to be a reference.
pub(crate) fn modelled_marker<H: Host>(host: &mut H, marker: &str) -> Option<String> {
    let owner = owner(marker)?;
    let value = Value::Ref(Some(marker.to_owned()));
    let to_string = MethodRef {
        owner,
        name: "toString".into(),
        descriptor: "()Ljava/lang/String;".into(),
    };
    match host.invoke_instance(&to_string, &value, &[]) {
        Ok(Value::Ref(Some(text))) if text != marker => Some(text),
        _ => None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `String.valueOf(Object)`/`Objects.toString(Object)` of a builder this
    /// runtime holds: its current text.
    ///
    /// javac compiles `"" + builder` to `String.valueOf(builder)` before the
    /// concat site, and the builder's text is runtime state the host cannot
    /// see, so the host would print its marker.
    pub(crate) fn invoke_value_of_builder(&self, r: &MethodRef, args: &[Value]) -> Option<Value> {
        if !matches!(
            (r.owner.as_str(), r.name.as_str()),
            ("java/lang/String", "valueOf") | ("java/util/Objects", "toString")
        ) || r.descriptor != "(Ljava/lang/Object;)Ljava/lang/String;"
        {
            return None;
        }
        let text = self.string_builders.get(expect_ref(args.first()?)?)?;
        Some(Value::Ref(Some(text.clone())))
    }
}
