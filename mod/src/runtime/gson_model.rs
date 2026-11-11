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
//! Options are never silently dropped. Each one either **changes the writer**,
//! exactly as Gson 2.14 does, or marks the `Gson` so every `toJson`/`fromJson`
//! is a counted unknown (the host answers its neutral value) instead of text
//! that ignores the option.
//!
//! Applied, from Gson's bytecode (`GsonBuilder`, `FieldNamingPolicy`,
//! `Excluder`, `JsonWriter`):
//! * `setPrettyPrinting`/`setFormattingStyle`: `FormattingStyle`'s newline,
//!   indent and space-after-separators shape the document.
//! * `disableHtmlEscaping`: the HTML-unsafe set (`<`, `>`, `&`, `=`, `'`) is
//!   escaped unless this was called.
//! * `serializeNulls`: jar-object fields with a null value are written.
//! * `setFieldNamingPolicy`: every `FieldNamingPolicy` constant's transform
//!   (`separateCamelCase` + `upperCaseFirstLetter`) names the JSON keys, both
//!   when writing and when reading a jar object.
//! * `excludeFieldsWithModifiers`: `Excluder.withModifiers` **replaces** its
//!   default `STATIC|TRANSIENT` mask with the arguments' OR, so
//!   `excludeFieldsWithModifiers(0)` really does write static and transient
//!   fields — a Gson quirk the model reproduces (synthetic fields are always
//!   excluded, as `Excluder.excludeField` does). Static values come from the
//!   invocation's static-field table.
//! * `generateNonExecutableJson`: the `)]}'\n` prefix on the text.
//! * `serializeSpecialFloatingPointValues`: `NaN`/`Infinity`/`-Infinity`
//!   literals; without it Gson's `IllegalArgumentException` is raised.
//! * `setLongSerializationPolicy`: `STRING` writes a `long` as a quoted
//!   number, `DEFAULT` as a number.
//!
//! Recorded as counted, because applying them needs data this runtime does not
//! carry: `setFieldNamingStrategy` and the `registerType*Adapter` calls (their
//! `translateName`/`write` is arbitrary jar code over a `Field`/`JsonWriter`
//! this model does not build), `setExclusionStrategies` and the two `add*` forms
//! (the same, over `FieldAttributes`), `excludeFieldsWithoutExposeAnnotation`
//! (field annotations are not parsed), `enableComplexMapKeySerialization`
//! (non-primitive map keys), `setDateFormat` (a `Date` model), `setVersion`
//! (`@Since`/`@Until`), `setObjectToNumberStrategy`/`setNumberToNumberStrategy`
//! (number parsing) and `disableInnerClassSerialization` (Gson refuses an inner
//! class there; this writer would emit it).
//!
//! Accepted and named as inert for this runtime: `setLenient`/`setStrictness`
//! (the parser is lenient, the existing departure for reading jar resources;
//! writing does not depend on it), `disableJdkUnsafe` (nothing here instantiates
//! through `Unsafe`; `fromJson` already builds an object without running a
//! constructor) and `addReflectionAccessFilter` (filters Gson's reflection into
//! JDK classes, which this model never reflects into).
//!
//! A value the writer cannot reproduce faithfully (an opaque host marker) makes
//! the whole `toJson` a counted unknown, never invented text.

use super::*;

/// The synthetic field key a builder/Gson marker keeps its [`GsonStyle`]
/// under. It is not a jar field name, so no reflective read sees it.
pub(super) const STYLE_FIELD: &str = "#gsonStyle";
/// The marker prefix a `FormattingStyle` value carries.
const FORMATTING_PREFIX: &str = "formatting-style/";
/// The marker prefix the whole builder option set is stored under.
const STYLE_PREFIX: &str = "gson-style/";
const NAMING_OWNER: &str = "com/google/gson/FieldNamingPolicy";
const LONG_POLICY_OWNER: &str = "com/google/gson/LongSerializationPolicy";
/// Gson's `Excluder()` default: `Modifier.STATIC | Modifier.TRANSIENT`.
const DEFAULT_FIELD_MODIFIERS: u16 = 0x0088;
/// `ACC_SYNTHETIC`, which `Excluder.excludeField` always excludes.
const SYNTHETIC: u16 = 0x1000;
/// The prefix `generateNonExecutableJson()` writes.
const NON_EXECUTABLE_PREFIX: &str = ")]}'\n";

/// `FieldNamingPolicy`'s seven constants and their `translateName` transform.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Naming {
    #[default]
    Identity,
    UpperCamelCase,
    UpperCamelCaseWithSpaces,
    UpperCaseWithUnderscores,
    LowerCaseWithUnderscores,
    LowerCaseWithDashes,
    LowerCaseWithDots,
}

impl Naming {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "IDENTITY" => Self::Identity,
            "UPPER_CAMEL_CASE" => Self::UpperCamelCase,
            "UPPER_CAMEL_CASE_WITH_SPACES" => Self::UpperCamelCaseWithSpaces,
            "UPPER_CASE_WITH_UNDERSCORES" => Self::UpperCaseWithUnderscores,
            "LOWER_CASE_WITH_UNDERSCORES" => Self::LowerCaseWithUnderscores,
            "LOWER_CASE_WITH_DASHES" => Self::LowerCaseWithDashes,
            "LOWER_CASE_WITH_DOTS" => Self::LowerCaseWithDots,
            _ => return None,
        })
    }

    fn index(self) -> u8 {
        match self {
            Self::Identity => 0,
            Self::UpperCamelCase => 1,
            Self::UpperCamelCaseWithSpaces => 2,
            Self::UpperCaseWithUnderscores => 3,
            Self::LowerCaseWithUnderscores => 4,
            Self::LowerCaseWithDashes => 5,
            Self::LowerCaseWithDots => 6,
        }
    }

    fn from_index(index: u8) -> Self {
        match index {
            1 => Self::UpperCamelCase,
            2 => Self::UpperCamelCaseWithSpaces,
            3 => Self::UpperCaseWithUnderscores,
            4 => Self::LowerCaseWithUnderscores,
            5 => Self::LowerCaseWithDashes,
            6 => Self::LowerCaseWithDots,
            _ => Self::Identity,
        }
    }

    /// Gson's `translateName` for a field named `name`.
    fn transform(self, name: &str) -> String {
        match self {
            Self::Identity => name.to_owned(),
            Self::UpperCamelCase => upper_case_first_letter(name),
            Self::UpperCamelCaseWithSpaces => {
                upper_case_first_letter(&separate_camel_case(name, ' '))
            }
            Self::UpperCaseWithUnderscores => separate_camel_case(name, '_').to_uppercase(),
            Self::LowerCaseWithUnderscores => separate_camel_case(name, '_').to_lowercase(),
            Self::LowerCaseWithDashes => separate_camel_case(name, '-').to_lowercase(),
            Self::LowerCaseWithDots => separate_camel_case(name, '.').to_lowercase(),
        }
    }
}

/// `FieldNamingPolicy.separateCamelCase`: a separator before every uppercase
/// letter that is not the first character.
fn separate_camel_case(name: &str, separator: char) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if index > 0 && ch.is_uppercase() {
            out.push(separator);
        }
        out.push(ch);
    }
    out
}

/// `FieldNamingPolicy.upperCaseFirstLetter`: uppercase the first letter, or
/// answer the name unchanged when that letter is already uppercase.
fn upper_case_first_letter(name: &str) -> String {
    for (index, ch) in name.char_indices() {
        if !ch.is_alphabetic() {
            continue;
        }
        if ch.is_uppercase() {
            return name.to_owned();
        }
        let mut out = String::with_capacity(name.len());
        out.push_str(&name[..index]);
        out.extend(ch.to_uppercase());
        out.push_str(&name[index + ch.len_utf8()..]);
        return out;
    }
    name.to_owned()
}

/// `FormattingStyle`: the newline, indent and space-after-separators of the
/// document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Formatting {
    newline: String,
    indent: String,
    space_after_separators: bool,
}

impl Default for Formatting {
    /// Gson's `DEFAULT_FORMATTING_STYLE` is `FormattingStyle.COMPACT`.
    fn default() -> Self {
        Self {
            newline: String::new(),
            indent: String::new(),
            space_after_separators: false,
        }
    }
}

impl Formatting {
    /// `FormattingStyle.PRETTY` from gson-2.14.0's `<clinit>`: a newline, a
    /// two-space indent, spaces after the separators.
    fn pretty() -> Self {
        Self {
            newline: "\n".into(),
            indent: "  ".into(),
            space_after_separators: true,
        }
    }

    fn marker(&self) -> String {
        format!(
            "{FORMATTING_PREFIX}{}/{}/{}",
            hex(&self.newline),
            hex(&self.indent),
            u8::from(self.space_after_separators)
        )
    }

    fn from_marker(marker: &str) -> Option<Self> {
        let mut parts = marker.strip_prefix(FORMATTING_PREFIX)?.split('/');
        let newline = unhex(parts.next()?)?;
        let indent = unhex(parts.next()?)?;
        let space_after_separators = match parts.next()? {
            "1" => true,
            "0" => false,
            _ => return None,
        };
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            newline,
            indent,
            space_after_separators,
        })
    }
}

/// The document shape and writer options one `Gson` was created with.
#[derive(Clone)]
pub(super) struct GsonStyle {
    formatting: Formatting,
    html_safe: bool,
    serialize_nulls: bool,
    naming: Naming,
    /// `Excluder`'s field-modifier mask; `excludeFieldsWithModifiers` replaces
    /// it, exactly as `Excluder.withModifiers` does.
    field_modifiers: u16,
    non_executable: bool,
    special_floats: bool,
    long_strings: bool,
    /// An option that changes the output is set but cannot be applied here, so
    /// the writer must not produce text.
    counted: bool,
}

impl Default for GsonStyle {
    fn default() -> Self {
        Self {
            formatting: Formatting::default(),
            html_safe: true,
            serialize_nulls: false,
            naming: Naming::Identity,
            field_modifiers: DEFAULT_FIELD_MODIFIERS,
            non_executable: false,
            special_floats: false,
            long_strings: false,
            counted: false,
        }
    }
}

impl GsonStyle {
    /// The marker this style is stored and passed around as.
    fn marker(&self) -> String {
        format!(
            "{STYLE_PREFIX}newline={};indent={};space={};html={};nulls={};\
             naming={};mods={};nonexec={};special={};longs={};counted={}",
            hex(&self.formatting.newline),
            hex(&self.formatting.indent),
            u8::from(self.formatting.space_after_separators),
            u8::from(self.html_safe),
            u8::from(self.serialize_nulls),
            self.naming.index(),
            self.field_modifiers,
            u8::from(self.non_executable),
            u8::from(self.special_floats),
            u8::from(self.long_strings),
            u8::from(self.counted),
        )
    }

    fn from_marker(marker: &str) -> Option<Self> {
        let rest = marker.strip_prefix(STYLE_PREFIX)?;
        let mut style = Self::default();
        for entry in rest.split(';') {
            let (key, value) = entry.split_once('=')?;
            let flag = |value: &str| match value {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            };
            match key {
                "newline" => style.formatting.newline = unhex(value)?,
                "indent" => style.formatting.indent = unhex(value)?,
                "space" => style.formatting.space_after_separators = flag(value)?,
                "html" => style.html_safe = flag(value)?,
                "nulls" => style.serialize_nulls = flag(value)?,
                "naming" => style.naming = Naming::from_index(value.parse().ok()?),
                "mods" => style.field_modifiers = value.parse().ok()?,
                "nonexec" => style.non_executable = flag(value)?,
                "special" => style.special_floats = flag(value)?,
                "longs" => style.long_strings = flag(value)?,
                "counted" => style.counted = flag(value)?,
                _ => return None,
            }
        }
        Some(style)
    }

    /// Whether a `toJson`/`fromJson` on this `Gson` must be a counted unknown.
    pub(super) fn is_counted(&self) -> bool {
        self.counted
    }

    fn non_executable(&self) -> bool {
        self.non_executable
    }

    fn long_strings(&self) -> bool {
        self.long_strings
    }

    fn special_floats(&self) -> bool {
        self.special_floats
    }

    fn serialize_nulls(&self) -> bool {
        self.serialize_nulls
    }

    /// The JSON key a jar field with this name gets.
    pub(super) fn field_name(&self, name: &str) -> String {
        self.naming.transform(name)
    }

    /// Whether `Excluder.excludeField` keeps a field with these access flags:
    /// a synthetic field is always excluded, and the modifier mask (which
    /// `excludeFieldsWithModifiers` replaces) excludes the rest.
    pub(super) fn includes_field(&self, access: u16) -> bool {
        access & SYNTHETIC == 0 && access & self.field_modifiers == 0
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
        if self.formatting.newline.is_empty() {
            String::new()
        } else {
            format!(
                "{}{}",
                self.formatting.newline,
                self.formatting.indent.repeat(depth)
            )
        }
    }

    /// The text between a member's name and its value.
    fn colon(&self) -> &'static str {
        if self.formatting.space_after_separators {
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
    for pair in text.as_bytes().chunks(2) {
        let pair = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair, 16).ok()?);
    }
    String::from_utf8(bytes).ok()
}

/// The Gson constants a builder takes as arguments: `FormattingStyle`'s two
/// styles, `FieldNamingPolicy`'s seven policies and
/// `LongSerializationPolicy`'s two modes.
pub(super) fn static_field(r: &MethodRef) -> Option<Value> {
    match r.owner.as_str() {
        "com/google/gson/FormattingStyle" => {
            let style = match r.name.as_str() {
                "COMPACT" => Formatting::default(),
                "PRETTY" => Formatting::pretty(),
                _ => return None,
            };
            Some(Value::Ref(Some(style.marker())))
        }
        NAMING_OWNER => Naming::from_name(&r.name)
            .map(|_| Value::Ref(Some(format!("enum/{NAMING_OWNER}/{}", r.name)))),
        LONG_POLICY_OWNER => matches!(r.name.as_str(), "DEFAULT" | "STRING")
            .then(|| Value::Ref(Some(format!("enum/{LONG_POLICY_OWNER}/{}", r.name)))),
        _ => None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// Answer a `GsonBuilder`, `Gson` or `FormattingStyle` call, or `None`.
    pub(super) fn invoke_gson_builder(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let receiver = receiver
            .and_then(expect_ref)
            .map(|marker| marker.to_owned());
        match r.owner.as_str() {
            "com/google/gson/FormattingStyle" => {
                let Some(marker) = receiver else {
                    return Ok(None);
                };
                let Some(style) = Formatting::from_marker(&marker) else {
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
                            Formatting {
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
                            Formatting {
                                indent: indent.clone(),
                                ..style
                            }
                            .marker(),
                        ))
                    }
                    ("withSpaceAfterSeparators", "(Z)Lcom/google/gson/FormattingStyle;") => {
                        let space = matches!(args.first(), Some(Value::Int(flag)) if *flag != 0);
                        Value::Ref(Some(
                            Formatting {
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
            // where the real builder fails. Every option below records what it
            // changes; none is silently dropped.
            (
                "excludeFieldsWithModifiers"
                | "registerTypeAdapterFactory"
                | "registerTypeAdapter"
                | "registerTypeHierarchyAdapter"
                | "setExclusionStrategies"
                | "setFieldNamingStrategy"
                | "setLongSerializationPolicy"
                | "setFormattingStyle",
                _,
            ) => {
                if args.iter().any(|value| matches!(value, Value::Ref(None))) {
                    return Err(npe());
                }
                match r.name.as_str() {
                    "setFormattingStyle" => {
                        let Some(formatting) = args
                            .first()
                            .and_then(expect_ref)
                            .and_then(|style| Formatting::from_marker(style))
                        else {
                            return Err(npe());
                        };
                        let mut style = self.gson_style(&marker);
                        style.formatting = formatting;
                        self.set_gson_style(&marker, style);
                    }
                    "setLongSerializationPolicy" => {
                        let mut style = self.gson_style(&marker);
                        style.long_strings = args
                            .first()
                            .and_then(expect_ref)
                            .and_then(|policy| policy.rsplit_once('/').map(|(_, name)| name))
                            == Some("STRING");
                        self.set_gson_style(&marker, style);
                    }
                    "excludeFieldsWithModifiers" => {
                        let mut style = self.gson_style(&marker);
                        style.field_modifiers = self.field_modifier_mask(args)?;
                        self.set_gson_style(&marker, style);
                    }
                    // A registered adapter, naming strategy, exclusion
                    // strategy or hierarchy adapter is arbitrary jar code whose
                    // effect on the text this writer cannot reproduce.
                    _ => {
                        if r.name == "registerTypeAdapter" {
                            self.check_gson_type_adapter(args)?;
                        }
                        let mut style = self.gson_style(&marker);
                        style.counted = true;
                        self.set_gson_style(&marker, style);
                    }
                }
                answer()
            }
            ("setPrettyPrinting", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.formatting = Formatting::pretty();
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
            ("generateNonExecutableJson", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.non_executable = true;
                self.set_gson_style(&marker, style);
                answer()
            }
            ("serializeSpecialFloatingPointValues", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.special_floats = true;
                self.set_gson_style(&marker, style);
                answer()
            }
            (
                "setFieldNamingPolicy",
                "(Lcom/google/gson/FieldNamingPolicy;)Lcom/google/gson/GsonBuilder;",
            ) => {
                let mut style = self.gson_style(&marker);
                style.naming = args
                    .first()
                    .and_then(expect_ref)
                    .and_then(|policy| policy.rsplit_once('/').map(|(_, name)| name))
                    .and_then(Naming::from_name)
                    .unwrap_or_default();
                self.set_gson_style(&marker, style);
                answer()
            }
            // Output-changing options this model cannot apply: every
            // `toJson`/`fromJson` on the created `Gson` becomes a counted
            // unknown instead of text that ignores the option.
            (
                "excludeFieldsWithoutExposeAnnotation"
                | "enableComplexMapKeySerialization"
                | "setDateFormat"
                | "setVersion"
                | "setObjectToNumberStrategy"
                | "setNumberToNumberStrategy"
                | "disableInnerClassSerialization"
                | "addSerializationExclusionStrategy"
                | "addDeserializationExclusionStrategy"
                | "addReflectionAccessFilter",
                _,
            ) => {
                if r.name == "addReflectionAccessFilter"
                    && args.iter().any(|value| matches!(value, Value::Ref(None)))
                {
                    return Err(npe());
                }
                if matches!(
                    r.name.as_str(),
                    "addSerializationExclusionStrategy" | "addDeserializationExclusionStrategy"
                ) && args.iter().any(|value| matches!(value, Value::Ref(None)))
                {
                    return Err(npe());
                }
                // `addReflectionAccessFilter` filters Gson's reflection into
                // JDK classes, which this model never reflects into; it is
                // inert here, not counted.
                if r.name != "addReflectionAccessFilter" {
                    let mut style = self.gson_style(&marker);
                    style.counted = true;
                    self.set_gson_style(&marker, style);
                }
                answer()
            }
            // Inert for this runtime: the parser is lenient by its documented
            // departure (writing does not depend on strictness), nothing here
            // instantiates through `Unsafe`, and `fromJson` already builds an
            // object without running a constructor.
            ("setLenient" | "disableJdkUnsafe", _) => answer(),
            ("setStrictness", _) => {
                if args.iter().any(|value| matches!(value, Value::Ref(None))) {
                    return Err(npe());
                }
                answer()
            }
            _ => Ok(None),
        }
    }

    /// Gson's `registerTypeAdapter` argument checks: a non-null adapter that
    /// implements one of the four supported interfaces, and no override of the
    /// built-in `Object` adapter. The interface test is exact for a jar class;
    /// a lambda marker is accepted because it does not record its functional
    /// interface, and a value the runtime holds as text is reported as
    /// `java.lang.String`, the class Gson would name.
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
        if adapter.starts_with("@indy:") || adapter.starts_with("@fn:") {
            return Ok(());
        }
        let Some((class, _)) = adapter.split_once('#') else {
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

    /// The OR of an `excludeFieldsWithModifiers(int...)` array. Gson's
    /// `Excluder.withModifiers` **replaces** its `STATIC|TRANSIENT` default
    /// with this mask, so a zero mask really does include both.
    fn field_modifier_mask(&self, args: &[Value]) -> Result<u16, NativeError> {
        let Some(array) = args.first() else {
            return Ok(DEFAULT_FIELD_MODIFIERS);
        };
        let mut mask = 0u16;
        for value in self.array(array)? {
            let Value::Int(bits) = value else {
                return Err(NativeError::Metadata(
                    "excludeFieldsWithModifiers needs an int array".into(),
                ));
            };
            mask |= *bits as u16;
        }
        Ok(mask)
    }

    fn gson_style(&self, marker: &str) -> GsonStyle {
        self.fields
            .get(&format!("{marker}|{STYLE_FIELD}"))
            .and_then(expect_ref)
            .and_then(|marker| GsonStyle::from_marker(marker))
            .unwrap_or_default()
    }

    /// The style a builder or `Gson` receiver carries, or Gson's default.
    pub(super) fn gson_style_of(&self, receiver: Option<&Value>) -> GsonStyle {
        receiver
            .and_then(expect_ref)
            .map_or_else(GsonStyle::default, |marker| self.gson_style(marker))
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
            Value::Long(value) => Some(if style.long_strings() {
                format!("\"{value}\"")
            } else {
                value.to_string()
            }),
            Value::Float(value) => {
                if value.is_finite() {
                    Some(format!("{value}"))
                } else if style.special_floats() {
                    Some(if value.is_nan() {
                        "NaN".into()
                    } else if *value > 0.0 {
                        "Infinity".into()
                    } else {
                        "-Infinity".into()
                    })
                } else {
                    // Gson's `JsonWriter.value(double)` refusal.
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some(
                            "NaN is not a valid double value as per JSON specification. \
                             To override this behavior, use \
                             GsonBuilder.serializeSpecialFloatingPointValues() method."
                                .into(),
                        ),
                        frame: None,
                    });
                }
            }
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

    /// A jar object as Gson reflects it: its fields in declaration order, the
    /// `Excluder` modifier mask (which `excludeFieldsWithModifiers` replaces)
    /// and the naming policy, with `null` fields omitted unless
    /// `serializeNulls()` was set.
    fn gson_fields_text(
        &mut self,
        class: &str,
        marker: &str,
        style: &GsonStyle,
        depth: usize,
    ) -> Result<Option<String>, NativeError> {
        let mut members: Vec<(String, Value)> = Vec::new();
        for (declaring, name, descriptor, access) in self.gson_fields(class) {
            if !style.includes_field(access) {
                continue;
            }
            let field = MethodRef {
                owner: declaring.clone(),
                name: name.clone(),
                descriptor: descriptor.clone(),
            };
            let value = if access & 0x0008 != 0 {
                self.static_fields
                    .get(&Self::static_field_key(&field))
                    .cloned()
            } else {
                self.fields
                    .get(&format!("{marker}|{declaring}#{name}{descriptor}"))
                    .cloned()
            }
            .unwrap_or_else(|| Value::default_for(&descriptor));
            if !style.serialize_nulls() && matches!(value, Value::Ref(None)) {
                continue;
            }
            members.push((style.field_name(&name), value));
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
    /// Static fields are included: whether they are written is the `Excluder`
    /// mask's decision.
    pub(super) fn gson_fields(&self, class: &str) -> Vec<(String, String, String, u16)> {
        let mut fields: Vec<(String, String, String, u16)> = Vec::new();
        let mut current = class.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(file) = self.class(&current) else {
                break;
            };
            for field in &file.fields {
                if fields.iter().any(|(_, _, name, _)| name == &field.name) {
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

/// `java.lang.NullPointerException`, as Gson's `Objects.requireNonNull` raises
/// it for a null builder argument.
fn npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
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

/// The non-executable JSON prefix one `Gson` writes.
pub(super) fn non_executable_prefix(style: &GsonStyle) -> Option<&'static str> {
    style.non_executable().then_some(NON_EXECUTABLE_PREFIX)
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
