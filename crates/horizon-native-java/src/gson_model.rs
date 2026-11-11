//! `com.google.gson`'s builder, formatting style and JSON writer.
//!
//! A config bootstrap builds its `Gson` through the fluent builder:
//! `new GsonBuilder().setPrettyPrinting().registerTypeAdapter(..).create()`.
//! Every one of those calls returns the same builder in Gson, so the chain is
//! the object the constructor made; only `create()` makes the `Gson`. The
//! builder's options live beside its marker in the interpreter's field table
//! (the same place a `TypeToken` keeps its captured type), and `create()`
//! copies them onto the `Gson` marker.
//!
//! The writer reproduces Gson's own text: `FormattingStyle`'s newline, indent
//! and space-after-separators shape the document, HTML-unsafe characters are
//! escaped unless `disableHtmlEscaping()` was called, and a jar object is
//! written from its instance fields (declaration order, static, transient and
//! synthetic fields skipped, `null` fields omitted unless `serializeNulls()`).
//! A value with no honest JSON shape answers `None`, and the caller counts the
//! `toJson` call instead of writing made-up text.
//!
//! Deliberate departures from Gson, named in the PR that added this file:
//! * a field naming policy, exclusion strategy, `@Expose` filter, modifiers
//!   exclusion or `setVersion` is accepted and has no effect on the field set
//!   the writer emits (the field table keeps no annotations and no per-field
//!   policy is read);
//! * `registerTypeAdapter`'s "does not implement any supported type adapter"
//!   `IllegalArgumentException` is only raised for a jar class the runtime can
//!   resolve; a lambda or a host marker is accepted, because the marker does
//!   not record which functional interface it implements.

use super::*;

/// The synthetic field key a builder/Gson marker keeps its [`GsonStyle`]
/// under. It is not a jar field name, so no reflective read sees it.
pub(super) const STYLE_FIELD: &str = "#gsonStyle";
/// The marker prefix a [`FormattingStyle`] value carries.
const STYLE_PREFIX: &str = "formatting-style/";

/// Gson's document shape: `FormattingStyle` plus the escaping and null flags a
/// builder set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GsonStyle {
    newline: String,
    indent: String,
    space_after_separators: bool,
    html_safe: bool,
    serialize_nulls: bool,
}

impl Default for GsonStyle {
    /// Gson's `DEFAULT_FORMATTING_STYLE` is `FormattingStyle.COMPACT`; HTML
    /// escaping defaults on and null fields are omitted.
    fn default() -> Self {
        Self {
            newline: String::new(),
            indent: String::new(),
            space_after_separators: false,
            html_safe: true,
            serialize_nulls: false,
        }
    }
}

impl GsonStyle {
    /// `FormattingStyle.PRETTY`: a newline, a two-space indent and a space
    /// after `:` and `,` (Gson 2.14, `FormattingStyle.<clinit>`; the getters
    /// answer these values on a real JVM).
    fn pretty() -> Self {
        Self {
            newline: "\n".into(),
            indent: "  ".into(),
            space_after_separators: true,
            ..Self::default()
        }
    }

    /// The marker this style is stored and passed around as.
    fn marker(&self) -> String {
        format!(
            "{STYLE_PREFIX}{}/{}/{}{}{}",
            hex(&self.newline),
            hex(&self.indent),
            u8::from(self.space_after_separators),
            u8::from(self.html_safe),
            u8::from(self.serialize_nulls),
        )
    }

    fn from_marker(marker: &str) -> Option<Self> {
        let rest = marker.strip_prefix(STYLE_PREFIX)?;
        let mut parts = rest.split('/');
        let newline = unhex(parts.next()?)?;
        let indent = unhex(parts.next()?)?;
        let flags = parts.next()?;
        if parts.next().is_some() || flags.len() != 3 {
            return None;
        }
        let mut digits = flags.bytes().map(|byte| byte as char);
        let space_after_separators = digits.next() == Some('1');
        let html_safe = digits.next() == Some('1');
        let serialize_nulls = digits.next() == Some('1');
        Some(Self {
            newline,
            indent,
            space_after_separators,
            html_safe,
            serialize_nulls,
        })
    }

    /// Gson's `JsonWriter.string` escaping: the JSON control characters, plus
    /// the HTML-unsafe set (`<`, `>`, `&`, `=`, `'`) while HTML escaping is on.
    fn escape(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{08}' => out.push_str("\\b"),
                '\u{0c}' => out.push_str("\\f"),
                '<' | '>' | '&' | '=' | '\'' if self.html_safe => {
                    out.push_str(&format!("\\u{:04x}", ch as u32));
                }
                ch if (ch as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", ch as u32));
                }
                ch => out.push(ch),
            }
        }
        out
    }

    /// The whitespace before one element or member at `depth`, and `""` in
    /// compact style.
    fn line(&self, depth: usize) -> String {
        if self.newline.is_empty() {
            String::new()
        } else {
            format!("{}{}", self.newline, self.indent.repeat(depth))
        }
    }

    /// The text between a member's name and its value.
    fn colon(&self) -> &'static str {
        if self.space_after_separators {
            ": "
        } else {
            ":"
        }
    }
}

fn hex(text: &str) -> String {
    text.bytes().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<String> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    let raw = text.as_bytes();
    for pair in raw.chunks(2) {
        let pair = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair, 16).ok()?);
    }
    String::from_utf8(bytes).ok()
}

/// `FormattingStyle`'s two constants, as a real JVM answers them.
pub(super) fn static_field(r: &MethodRef) -> Option<Value> {
    if r.owner != "com/google/gson/FormattingStyle" {
        return None;
    }
    let style = match r.name.as_str() {
        "COMPACT" => GsonStyle::default(),
        "PRETTY" => GsonStyle::pretty(),
        _ => return None,
    };
    Some(Value::Ref(Some(style.marker())))
}

impl<H: Host> Runtime<'_, H> {
    /// Answer a `GsonBuilder` or `FormattingStyle` call, or `None`.
    pub(super) fn invoke_gson_builder(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let receiver = receiver.and_then(expect_ref).map(|marker| marker.to_owned());
        match r.owner.as_str() {
            "com/google/gson/FormattingStyle" => {
                let Some(marker) = receiver else {
                    return Ok(None);
                };
                let Some(style) = GsonStyle::from_marker(&marker) else {
                    return Ok(None);
                };
                Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
                    ("getNewline", "()Ljava/lang/String;") => {
                        Value::Ref(Some(style.newline.clone()))
                    }
                    ("getIndent", "()Ljava/lang/String;") => Value::Ref(Some(style.indent.clone())),
                    ("usesSpaceAfterSeparators", "()Z") => {
                        Value::Int(i32::from(style.space_after_separators))
                    }
                    ("withNewline", "(Ljava/lang/String;)Lcom/google/gson/FormattingStyle;") => {
                        let Some(newline) = args.first().and_then(expect_ref) else {
                            return Err(npe());
                        };
                        Value::Ref(Some(
                            GsonStyle {
                                newline: newline.clone(),
                                ..style
                            }
                            .marker(),
                        ))
                    }
                    ("withIndent", "(Ljava/lang/String;)Lcom/google/gson/FormattingStyle;") => {
                        let Some(indent) = args.first().and_then(expect_ref) else {
                            return Err(npe());
                        };
                        Value::Ref(Some(
                            GsonStyle {
                                indent: indent.clone(),
                                ..style
                            }
                            .marker(),
                        ))
                    }
                    ("withSpaceAfterSeparators", "(Z)Lcom/google/gson/FormattingStyle;") => {
                        let space = matches!(args.first(), Some(Value::Int(flag)) if *flag != 0);
                        Value::Ref(Some(
                            GsonStyle {
                                space_after_separators: space,
                                ..style
                            }
                            .marker(),
                        ))
                    }
                    _ => return Ok(None),
                }))
            }
            "com/google/gson/GsonBuilder" => self.invoke_gson_builder_call(r, receiver, args),
            _ => Ok(None),
        }
    }

    fn invoke_gson_builder_call(
        &mut self,
        r: &MethodRef,
        receiver: Option<String>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(marker) = receiver else {
            return Ok(None);
        };
        // Every fluent method answers the receiver, so the chain stays on the
        // builder the constructor made, exactly as Gson's `return this` does.
        let answer = || Ok(Some(Value::Ref(Some(marker.clone()))));
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("<init>", "()V") => {
                self.set_gson_style(&marker, GsonStyle::default());
                Ok(Some(Value::Void))
            }
            ("create", "()Lcom/google/gson/Gson;") => {
                let style = self.gson_style(&marker);
                let object = self.new_object("com/google/gson/Gson");
                let created = expect_ref(&object).cloned().unwrap_or_default();
                self.set_gson_style(&created, style);
                Ok(Some(object))
            }
            // Gson's own null checks, so a bootstrap that passes null fails
            // where the real builder fails.
            (
                "excludeFieldsWithModifiers"
                | "registerTypeAdapterFactory"
                | "registerTypeAdapter"
                | "registerTypeHierarchyAdapter"
                | "setExclusionStrategies"
                | "addSerializationExclusionStrategy"
                | "addDeserializationExclusionStrategy"
                | "setFieldNamingStrategy"
                | "setLongSerializationPolicy"
                | "setObjectToNumberStrategy"
                | "setNumberToNumberStrategy"
                | "setFormattingStyle"
                | "setStrictness"
                | "addReflectionAccessFilter",
                _,
            ) => {
                if args.iter().any(|value| matches!(value, Value::Ref(None))) {
                    return Err(npe());
                }
                if r.name == "registerTypeAdapter" {
                    self.check_gson_type_adapter(args)?;
                }
                if r.name == "setFormattingStyle" {
                    let Some(style) = args
                        .first()
                        .and_then(expect_ref)
                        .and_then(|style| GsonStyle::from_marker(style))
                    else {
                        return Err(npe());
                    };
                    self.set_gson_style(&marker, style);
                }
                answer()
            }
            ("setPrettyPrinting", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                let pretty = GsonStyle::pretty();
                style.newline = pretty.newline;
                style.indent = pretty.indent;
                style.space_after_separators = pretty.space_after_separators;
                self.set_gson_style(&marker, style);
                answer()
            }
            ("disableHtmlEscaping", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.html_safe = false;
                self.set_gson_style(&marker, style);
                answer()
            }
            ("serializeNulls", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.serialize_nulls = true;
                self.set_gson_style(&marker, style);
                answer()
            }
            // A builder option that does not change the text of the value
            // shapes the bounded writer emits (naming and exclusion policies,
            // strictness, version, date formats, extra adapters) still answers
            // the builder; the departures are named in the module doc.
            (
                "setVersion"
                | "generateNonExecutableJson"
                | "excludeFieldsWithoutExposeAnnotation"
                | "enableComplexMapKeySerialization"
                | "disableInnerClassSerialization"
                | "setFieldNamingPolicy"
                | "setLenient"
                | "setDateFormat"
                | "serializeSpecialFloatingPointValues"
                | "disableJdkUnsafe",
                _,
            ) => answer(),
            _ => Ok(None),
        }
    }

    /// Gson's `registerTypeAdapter` argument checks: a non-null adapter that
    /// implements one of the four supported interfaces, and no override of the
    /// built-in `Object` adapter. The interface test is exact for a jar class;
    /// a lambda or host marker cannot be resolved here and is accepted (named
    /// departure).
    fn check_gson_type_adapter(&mut self, args: &[Value]) -> Result<(), NativeError> {
        if let Some(class) = args.first().and_then(expect_ref)
            && class == "java/lang/Object"
        {
            return Err(NativeError::UncaughtException {
                class: "java/lang/IllegalArgumentException".into(),
                message: Some("Cannot override built-in adapter for class java.lang.Object".into()),
                frame: None,
            });
        }
        let Some(adapter) = args.get(1).and_then(expect_ref) else {
            return Err(npe());
        };
        // A lambda (a `JsonSerializer`/`JsonDeserializer`/`TypeAdapter`
        // implementation by construction) is accepted; every other value must
        // be a class this runtime can resolve and walk.
        if adapter.starts_with("@indy:") || adapter.starts_with("@fn:") {
            return Ok(());
        }
        let Some((class, _)) = adapter.split_once('#') else {
            // The runtime holds a Java `String` and a text marker the same
            // way; `String` is the class Gson names for such a value.
            return Err(not_a_supported_adapter("java/lang/String"));
        };
        if self.class(class).is_none() {
            return Err(not_a_supported_adapter(class));
        }
        let supported = [
            "com/google/gson/JsonSerializer",
            "com/google/gson/JsonDeserializer",
            "com/google/gson/InstanceCreator",
            "com/google/gson/TypeAdapter",
        ];
        if supported
            .iter()
            .any(|wanted| self.class_is_instance(class, wanted))
        {
            return Ok(());
        }
        Err(not_a_supported_adapter(class))
    }

    /// The style a builder or `Gson` receiver carries, or Gson's default.
    pub(super) fn gson_style_of(&self, receiver: Option<&Value>) -> GsonStyle {
        receiver
            .and_then(expect_ref)
            .map_or_else(GsonStyle::default, |marker| self.gson_style(marker))
    }

    fn gson_style(&self, marker: &str) -> GsonStyle {
        self.fields
            .get(&format!("{marker}|{STYLE_FIELD}"))
            .and_then(expect_ref)
            .and_then(|marker| GsonStyle::from_marker(marker))
            .unwrap_or_default()
    }

    pub(super) fn set_gson_style(&mut self, marker: &str, style: GsonStyle) {
        self.fields.insert(
            format!("{marker}|{STYLE_FIELD}"),
            Value::Ref(Some(style.marker())),
        );
    }

    /// Gson's `toJson` text for a modelled value, or `None` when the value has
    /// no honest JSON shape (an object the runtime does not own and cannot
    /// reflect over). The caller counts the call instead of writing fake text.
    pub(super) fn gson_to_json(
        &mut self,
        value: &Value,
        style: &GsonStyle,
        depth: usize,
    ) -> Result<Option<String>, NativeError> {
        let text = match value {
            Value::Ref(None) | Value::Void => Some("null".to_owned()),
            Value::Int(value) => Some(value.to_string()),
            Value::Long(value) => Some(value.to_string()),
            Value::Float(value) => Some(if value.is_finite() {
                format!("{value}")
            } else {
                "null".into()
            }),
            Value::Ref(Some(marker)) => {
                // A jar enum constant serializes as its name.
                if let Some(name) = marker
                    .strip_prefix("enum/")
                    .and_then(|rest| rest.rsplit_once('/').map(|(_, name)| name))
                {
                    Some(format!("\"{}\"", style.escape(name)))
                } else if let Some(text) = jdk_model::boxed_text(marker, "Ljava/lang/Object;") {
                    // A boxed number or boolean is a JSON number or literal;
                    // a `Character` is a one-character string.
                    if marker.starts_with("character:") {
                        Some(format!("\"{}\"", style.escape(&text)))
                    } else {
                        Some(text)
                    }
                } else if let Some(kind) = gson_marker_kind(self.host, value) {
                    let values = runtime_collection_values(self.host, value)?;
                    if kind == "map" {
                        self.gson_object_text(&values, style, depth)?
                    } else {
                        self.gson_array_text(&values, style, depth)?
                    }
                } else if let Some((class, id)) = marker.split_once('#')
                    && id.bytes().all(|byte| byte.is_ascii_digit())
                    && self.class(class).is_some()
                {
                    self.gson_fields_text(class, marker, style, depth)?
                } else {
                    // A plain `String` value (and a marker the runtime only
                    // models as text) is a JSON string.
                    Some(format!("\"{}\"", style.escape(marker)))
                }
            }
        };
        Ok(text)
    }

    fn gson_array_text(
        &mut self,
        values: &[String],
        style: &GsonStyle,
        depth: usize,
    ) -> Result<Option<String>, NativeError> {
        if values.is_empty() {
            return Ok(Some("[]".into()));
        }
        let mut out = String::from("[");
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&style.line(depth + 1));
            let Some(text) = self.gson_to_json(&parse_value_marker(value)?, style, depth + 1)?
            else {
                return Ok(None);
            };
            out.push_str(&text);
        }
        out.push_str(&style.line(depth));
        out.push(']');
        Ok(Some(out))
    }

    fn gson_object_text(
        &mut self,
        values: &[String],
        style: &GsonStyle,
        depth: usize,
    ) -> Result<Option<String>, NativeError> {
        if !values.len().is_multiple_of(2) {
            return Err(NativeError::Metadata(
                "native map marker has an odd value count".into(),
            ));
        }
        if values.is_empty() {
            return Ok(Some("{}".into()));
        }
        let mut out = String::from("{");
        let mut first = true;
        for pair in values.chunks(2) {
            let key = parse_value_marker(&pair[0])?;
            let value = parse_value_marker(&pair[1])?;
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&style.line(depth + 1));
            out.push('"');
            out.push_str(&style.escape(&value_text(&key)));
            out.push('"');
            out.push_str(style.colon());
            let Some(text) = self.gson_to_json(&value, style, depth + 1)? else {
                return Ok(None);
            };
            out.push_str(&text);
        }
        out.push_str(&style.line(depth));
        out.push('}');
        Ok(Some(out))
    }

    /// A jar object as Gson reflects it: its instance fields in declaration
    /// order, `static`, `transient` and synthetic fields skipped, `null` fields
    /// omitted unless `serializeNulls()` was set.
    fn gson_fields_text(
        &mut self,
        class: &str,
        marker: &str,
        style: &GsonStyle,
        depth: usize,
    ) -> Result<Option<String>, NativeError> {
        let mut members: Vec<(String, Value)> = Vec::new();
        for (declaring, name, descriptor, access) in self.instance_fields_with_access(class) {
            // `transient` (0x80) and synthetic (0x1000) fields are not data.
            if access & (0x0080 | 0x1000) != 0 {
                continue;
            }
            let key = format!("{marker}|{declaring}#{name}{descriptor}");
            let value = self
                .fields
                .get(&key)
                .cloned()
                .unwrap_or_else(|| Value::default_for(&descriptor));
            if !style.serialize_nulls && matches!(value, Value::Ref(None)) {
                continue;
            }
            members.push((name, value));
        }
        if members.is_empty() {
            return Ok(Some("{}".into()));
        }
        let mut out = String::from("{");
        for (index, (name, value)) in members.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&style.line(depth + 1));
            out.push('"');
            out.push_str(&style.escape(name));
            out.push('"');
            out.push_str(style.colon());
            let Some(text) = self.gson_to_json(value, style, depth + 1)? else {
                return Ok(None);
            };
            out.push_str(&text);
        }
        out.push_str(&style.line(depth));
        out.push('}');
        Ok(Some(out))
    }

    /// Every declared field of `class` and its superclasses with its access
    /// flags, the class's own fields first, exactly Gson's reflection order.
    fn instance_fields_with_access(&self, class: &str) -> Vec<(String, String, String, u16)> {
        let mut fields: Vec<(String, String, String, u16)> = Vec::new();
        let mut current = class.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(file) = self.class(&current) else {
                break;
            };
            for field in &file.fields {
                if field.is_static() || fields.iter().any(|(_, _, name, _)| name == &field.name) {
                    continue;
                }
                fields.push((
                    current.clone(),
                    field.name.clone(),
                    field.descriptor.clone(),
                    field.access,
                ));
            }
            if file.super_name.is_empty() || file.super_name == current {
                break;
            }
            current = file.super_name.clone();
        }
        fields
    }
}

/// Gson's `registerTypeAdapter` refusal for an object that implements none of
/// its adapter interfaces.
fn not_a_supported_adapter(class: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(format!(
            "Class {} does not implement any supported type adapter class or interface",
            class.replace('/', ".")
        )),
        frame: None,
    }
}

/// `java.lang.NullPointerException`, as Gson's `Objects.requireNonNull` raises
/// it for a null builder argument.
fn npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

/// Whether one marker is a collection this host can read, and its shape.
///
/// A neutral marker names its kind directly; a `new`-backed marker is a class
/// name whose list/set/map family decides how its pairs or elements serialize.
fn gson_marker_kind<H: Host>(host: &H, value: &Value) -> Option<&'static str> {
    let marker = expect_ref(value)?;
    if host.mutable_collection_values(value).is_some() {
        let class = marker
            .split_once('#')
            .map_or(marker.as_str(), |(class, _)| class);
        return if is_map_owner(class) {
            Some("map")
        } else {
            Some("list")
        };
    }
    match gson_collection_kind(marker) {
        Some("map") => Some("map"),
        Some(_) => Some("list"),
        None => None,
    }
}

fn gson_collection_kind(marker: &str) -> Option<&str> {
    let kind = marker.split_once('/').map_or(marker, |(kind, _)| kind);
    let kind = kind.split_once('#').map_or(kind, |(kind, _)| kind);
    matches!(kind, "list" | "set" | "map").then_some(kind)
}
