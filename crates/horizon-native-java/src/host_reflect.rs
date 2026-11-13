//! `java.lang.reflect.Field`, `Method` and `Constructor` as host handles
//! (#156 P10): their keys, and their `toString`/`hashCode`/`equals`/
//! `instanceof` rows in the kind table (`host_ref.rs`), written from the
//! JDK 25 sources and checked against a Java 25 run
//! (`tests/fixture_host_values_reflect.rs`).
//!
//! These used to be text (`reflect-field/…`, `reflect-method/…`,
//! `reflect-constructor/…`), so a jar string spelled like one was that member
//! to `Method.invoke`, `Field.get` and `Constructor.newInstance`. A handle is
//! not a `String`, so nothing a jar spells can be one.
//!
//! # Keys
//!
//! A key is a list of values in the one nested-key encoding (`value_marker`,
//! separators escaped, as `HostRef::of_values`):
//! `[class, name, descriptor|null, access|null, throws|null, flags]`, where
//! `flags` (bit 0: the declaring class is an interface) is always last and
//! always the runtime's own number. A `Field` copy appends `#<copy>` after it.
//! Every name and class a jar spells is an escaped string element, read back
//! as that string and nothing else: a lookup of `"COUNT:I:25#0"` names a
//! field called `COUNT:I:25#0`, never `COUNT`'s flags or a copy. Only the
//! runtime fills the descriptor, access, `throws` and copy, from the class
//! file a lookup found the member in.
//!
//! A key without access flags is a member the runtime could not read (a game
//! or JDK class, or a lookup it could not settle); its `toString`,
//! `hashCode`, `getModifiers` and `getDeclaringClass` are unknown and
//! counted, since its declaring class may be a superclass.
//!
//! # Identity
//!
//! The JDK hands out a fresh object on every lookup (`getMethod` twice:
//! `equals`, not `==`; Java 25). A `Field` carries a copy number (`#<n>`, the
//! object counter, `reflect_override.rs`), so each copy is its own key and
//! its kind is canonical when it has one: equal keys are one copy. A `Method`
//! or `Constructor` carries none, so two equal handles from two lookups are
//! counted at `==` (never canonical), as two lookups are two objects.

use crate::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, Nested};
use crate::jdk_model::java_string_hash;
use crate::{ClassFile, Host, NativeError, Runtime, Value, descriptor_arguments, return_desc};

pub(crate) const FIELD: &str = "java/lang/reflect/Field";
pub(crate) const METHOD: &str = "java/lang/reflect/Method";
pub(crate) const CONSTRUCTOR: &str = "java/lang/reflect/Constructor";

/// `ACC_INTERFACE` of a class's access flags.
const ACC_INTERFACE: u16 = 0x0200;

/// One reflected member, read from its handle's key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Member {
    pub(crate) class: String,
    /// The member's name; `<init>` for a constructor.
    pub(crate) name: String,
    /// A method's or constructor's descriptor, a field's type descriptor;
    /// `None` when the lookup named no exact member.
    pub(crate) descriptor: Option<String>,
    /// The class file's access flags, when the runtime read the member.
    pub(crate) access: Option<u16>,
    /// Whether the declaring class is an interface (methods only).
    pub(crate) interface: bool,
    /// The `Exceptions` attribute's classes joined by `,` (executables).
    pub(crate) throws: String,
}

impl Member {
    /// What `equals` compares: the declaring class, the name and, for a
    /// field, its type; for an executable its parameter and return types.
    fn identity(&self) -> (&str, &str, Option<&str>) {
        (&self.class, &self.name, self.descriptor.as_deref())
    }
}

/// What `Class.getField`/`getDeclaredField` finds, read off the classes the
/// runtime holds.
pub(crate) enum FieldLookup {
    /// The field, its declaring class, descriptor and access flags.
    Found(String, String, u16),
    /// Every class the search visits is held and none declares it: Java's
    /// `NoSuchFieldException`.
    Absent,
    /// The search reaches a class the runtime does not hold (a game or JDK
    /// class), which may declare it.
    Unknown,
}

/// How deep `getField` follows superclasses and superinterfaces before it
/// stops deciding.
const MAX_FIELD_SEARCH: usize = 64;

/// `NoSuchFieldException(name)`, as `getField`/`getDeclaredField` throw it.
pub(crate) fn no_such_field(name: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NoSuchFieldException".into(),
        message: Some(name.into()),
        frame: None,
    }
}

/// `NoSuchMethodException` as `getDeclaredMethod` throws it
/// (`Class.methodToString`): `<class>.<name>(<parameter class names>)`.
pub(crate) fn no_such_method(class: &str, name: &str, parameters: &[String]) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NoSuchMethodException".into(),
        message: Some(format!(
            "{}.{name}({})",
            class.replace('/', "."),
            parameters
                .iter()
                .map(|parameter| parameter.replace('/', "."))
                .collect::<Vec<_>>()
                .join(",")
        )),
        frame: None,
    }
}

/// The class file facts a lookup that found its member records in the key.
pub(crate) struct Declared {
    pub(crate) access: u16,
    pub(crate) interface: bool,
    pub(crate) throws: Vec<String>,
}

impl Declared {
    /// The facts of method `name` + `descriptor` of `file`, `None` when the
    /// class does not declare it.
    pub(crate) fn of(file: &ClassFile, name: &str, descriptor: &str) -> Option<Self> {
        let method = file
            .methods
            .iter()
            .find(|method| method.name == name && method.descriptor == descriptor)?;
        Some(Self {
            access: method.access,
            interface: crate::jdk_model::class_access_flags(file)
                .is_some_and(|access| access & ACC_INTERFACE != 0),
            throws: crate::neoforge_subscribe::method_exceptions(file, name, descriptor)
                .unwrap_or_default(),
        })
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `Class.getDeclaredField(name)` (`declared`) or `getField(name)` on
    /// `class`. `getField` searches as `Class.getField0` does: the class's
    /// own public fields, then each superinterface in order, then (for a
    /// class) its superclass, each recursively; a branch through a class the
    /// runtime does not hold makes the answer unknown (it may declare the
    /// field). `Object` declares no public field.
    pub(crate) fn field_lookup(&self, class: &str, name: &str, declared: bool) -> FieldLookup {
        self.field_search(class, name, declared, 0)
    }

    fn field_search(&self, class: &str, name: &str, declared: bool, depth: usize) -> FieldLookup {
        if !declared && class == "java/lang/Object" {
            return FieldLookup::Absent;
        }
        let Some(file) = self.class(class).filter(|_| depth < MAX_FIELD_SEARCH) else {
            return FieldLookup::Unknown;
        };
        if let Some(field) = file
            .fields
            .iter()
            .find(|field| field.name == name && (declared || field.access & 1 != 0))
        {
            return FieldLookup::Found(class.into(), field.descriptor.clone(), field.access);
        }
        if declared {
            return FieldLookup::Absent;
        }
        let interface = crate::jdk_model::class_access_flags(file)
            .is_some_and(|access| access & ACC_INTERFACE != 0);
        let mut parents = file.interface_names();
        if !interface && !file.super_name.is_empty() {
            parents.push(file.super_name.clone());
        }
        for parent in parents {
            match self.field_search(&parent, name, false, depth + 1) {
                FieldLookup::Absent => {}
                found_or_unknown => return found_or_unknown,
            }
        }
        FieldLookup::Absent
    }

    /// The `Method` a lookup hands out for `class.name descriptor`, with the
    /// class file's facts when the class declares exactly that method.
    pub(crate) fn reflected_method(
        &self,
        class: &str,
        name: &str,
        descriptor: Option<&str>,
    ) -> Value {
        let declared =
            descriptor.and_then(|descriptor| Declared::of(self.class(class)?, name, descriptor));
        method(class, name, descriptor, declared.as_ref())
    }

    /// The `Constructor` a lookup hands out for `class.<init> descriptor`.
    pub(crate) fn reflected_constructor(&self, class: &str, descriptor: Option<&str>) -> Value {
        let declared = descriptor
            .and_then(|descriptor| Declared::of(self.class(class)?, "<init>", descriptor));
        constructor(class, descriptor, declared.as_ref())
    }
}

/// The key's facts, in the one nested-key encoding; `flags` (the runtime's
/// own number) is always last, so no jar text ends a key.
fn facts(class: &str, name: &str, descriptor: Option<&str>, declared: Option<&Declared>) -> String {
    let text =
        |text: Option<&str>| text.map_or(Value::Ref(None), |text| Value::Ref(Some(text.into())));
    let throws = declared
        .filter(|declared| !declared.throws.is_empty())
        .map(|declared| declared.throws.join(","));
    [
        text(Some(class)),
        text(Some(name)),
        text(descriptor),
        declared.map_or(Value::Ref(None), |declared| {
            Value::Int(i32::from(declared.access))
        }),
        text(throws.as_deref()),
        Value::Int(i32::from(
            declared.is_some_and(|declared| declared.interface),
        )),
    ]
    .iter()
    .map(crate::value_marker)
    .collect::<Vec<_>>()
    .join(",")
}

/// A `Field` handle: `declared` is `(descriptor, access)` when the class's
/// field table named it, and `copy` the copy number a lookup gives it.
pub(crate) fn field(
    class: &str,
    name: &str,
    declared: Option<(&str, u16)>,
    copy: Option<u64>,
) -> Value {
    let facts_of = declared.map(|(_, access)| Declared {
        access,
        interface: false,
        throws: Vec::new(),
    });
    let mut key = facts(
        class,
        name,
        declared.map(|(descriptor, _)| descriptor),
        facts_of.as_ref(),
    );
    if let (Some(_), Some(copy)) = (declared, copy) {
        key.push_str(&format!("#{copy}"));
    }
    Value::Host(HostRef::new(HostKind::ReflectField, key))
}

/// A `Method` handle (`declared` only with a descriptor).
pub(crate) fn method(
    class: &str,
    name: &str,
    descriptor: Option<&str>,
    declared: Option<&Declared>,
) -> Value {
    let declared = declared.filter(|_| descriptor.is_some());
    Value::Host(HostRef::new(
        HostKind::ReflectMethod,
        facts(class, name, descriptor, declared),
    ))
}

/// A `Constructor` handle (`declared` only with a descriptor).
pub(crate) fn constructor(
    class: &str,
    descriptor: Option<&str>,
    declared: Option<&Declared>,
) -> Value {
    let declared = declared.filter(|_| descriptor.is_some());
    Value::Host(HostRef::new(
        HostKind::ReflectConstructor,
        facts(class, "<init>", descriptor, declared),
    ))
}

/// A key without its copy number, and the copy. The facts always end with
/// the runtime's `flags` element (`i<n>`), so any `#` a jar's name put in
/// them is followed by a `,` and never reads as a copy.
fn split_copy(key: &str) -> (&str, Option<&str>) {
    match key.rsplit_once('#') {
        Some((facts, copy))
            if !facts.is_empty()
                && !copy.is_empty()
                && copy.bytes().all(|b| b.is_ascii_digit()) =>
        {
            (facts, Some(copy))
        }
        _ => (key, None),
    }
}

/// The member a key's facts name.
fn parse(key: &str) -> Option<Member> {
    let (facts, _) = split_copy(key);
    let values = crate::split_capture_markers(facts)
        .ok()?
        .iter()
        .map(|marker| crate::parse_value_marker(marker).ok())
        .collect::<Option<Vec<_>>>()?;
    let text = |value: &Value| match value {
        Value::Ref(Some(text)) => Some(Some(text.clone())),
        Value::Ref(None) => Some(None),
        _ => None,
    };
    let [class, name, descriptor, access, throws, Value::Int(flags)] = values.as_slice() else {
        return None;
    };
    Some(Member {
        class: text(class)??,
        name: text(name)??,
        descriptor: text(descriptor)?,
        access: match access {
            Value::Int(access) => Some(u16::try_from(*access).ok()?),
            _ => None,
        },
        interface: flags & 1 != 0,
        throws: text(throws)?.unwrap_or_default(),
    })
}

fn handle_of(value: &Value, kind: HostKind) -> Option<&HostRef> {
    match value {
        Value::Host(handle) if handle.kind() == kind => Some(handle),
        _ => None,
    }
}

/// The member a `Field` handle names.
pub(crate) fn field_of(value: &Value) -> Option<Member> {
    parse(handle_of(value, HostKind::ReflectField)?.key())
}

/// The member a `Method` handle names.
pub(crate) fn method_of(value: &Value) -> Option<Member> {
    parse(handle_of(value, HostKind::ReflectMethod)?.key())
}

/// The member a `Constructor` handle names (`name` is `<init>`).
pub(crate) fn constructor_of(value: &Value) -> Option<Member> {
    parse(handle_of(value, HostKind::ReflectConstructor)?.key())
}

/// The member any of the three handles names.
pub(crate) fn member_of(value: &Value) -> Option<Member> {
    field_of(value)
        .or_else(|| method_of(value))
        .or_else(|| constructor_of(value))
}

/// The key every copy of a field shares: what the `override` table keys a
/// field by (`reflect_override.rs`), and what a copy-less key is as is.
pub(crate) fn field_copy_free(value: &Value) -> Option<&str> {
    Some(split_copy(handle_of(value, HostKind::ReflectField)?.key()).0)
}

/// The key of a reflection handle of any of the three kinds: the receiver
/// text of its entries in the runtime's field table (`reflect_override.rs`).
/// The key itself, not its encoding: a `Field` copy's key ends in its copy
/// number (`#<n>`), so the heap collector finds the copy's entries through
/// the copy's own key (`heap.rs`).
pub(crate) fn member_key(value: &Value) -> Option<&str> {
    match value {
        Value::Host(handle)
            if matches!(
                handle.kind(),
                HostKind::ReflectField | HostKind::ReflectMethod | HostKind::ReflectConstructor
            ) =>
        {
            Some(handle.key())
        }
        _ => None,
    }
}

/// `Class.getTypeName()` of a descriptor: `int`, `java.lang.String[]`,
/// `demo.Outer$Inner`.
pub(crate) fn type_name(descriptor: &str) -> String {
    if let Some(component) = descriptor.strip_prefix('[') {
        return format!("{}[]", type_name(component));
    }
    match descriptor {
        "Z" => "boolean".into(),
        "B" => "byte".into(),
        "C" => "char".into(),
        "S" => "short".into(),
        "I" => "int".into(),
        "J" => "long".into(),
        "F" => "float".into(),
        "D" => "double".into(),
        "V" => "void".into(),
        other => other
            .strip_prefix('L')
            .and_then(|name| name.strip_suffix(';'))
            .unwrap_or(other)
            .replace('/', "."),
    }
}

/// `Modifier.toString(mask)`: the words in its order.
fn modifier_words(mask: u16) -> String {
    [
        (0x0001, "public"),
        (0x0004, "protected"),
        (0x0002, "private"),
        (0x0400, "abstract"),
        (0x0008, "static"),
        (0x0010, "final"),
        (0x0080, "transient"),
        (0x0040, "volatile"),
        (0x0020, "synchronized"),
        (0x0100, "native"),
        (0x0800, "strictfp"),
    ]
    .into_iter()
    .filter(|(bit, _)| mask & bit != 0)
    .map(|(_, word)| word)
    .collect::<Vec<_>>()
    .join(" ")
}

/// `Field.toString()`: `Modifier.toString(getModifiers())` and a space unless
/// the modifiers are `0` (the JVM's recognized field flags, so a synthetic
/// field prints a lone space), the type's and the declaring class's type
/// names, and the name.
fn field_text(field: &Member) -> Option<String> {
    let modifiers = field.access? & 0x50df;
    let prefix = if modifiers == 0 {
        String::new()
    } else {
        format!("{} ", modifier_words(modifiers))
    };
    Some(format!(
        "{prefix}{} {}.{}",
        type_name(field.descriptor.as_deref()?),
        field.class.replace('/', "."),
        field.name
    ))
}

/// `Field.toString()` of a field the class file declares.
pub(crate) fn declared_field_text(
    class: &str,
    name: &str,
    descriptor: &str,
    access: u16,
) -> String {
    field_text(&Member {
        class: class.into(),
        name: name.into(),
        descriptor: Some(descriptor.into()),
        access: Some(access),
        interface: false,
        throws: String::new(),
    })
    .unwrap_or_default()
}

/// `Method.toString()` of a method the class file declares.
pub(crate) fn method_text(
    class: &str,
    name: &str,
    descriptor: &str,
    declared: &Declared,
) -> String {
    executable_text(
        &Member {
            class: class.into(),
            name: name.into(),
            descriptor: Some(descriptor.into()),
            access: Some(declared.access),
            interface: declared.interface,
            throws: declared.throws.join(","),
        },
        false,
    )
    .unwrap_or_default()
}

/// `Executable.sharedToString`: the modifiers under `mask` (with `default`
/// after the access word for an interface's default method), the header
/// (`Method`: return type, class and name; `Constructor`: class), the
/// parameter type names joined by `,`, and ` throws ` with the
/// `Exceptions` attribute's classes.
fn executable_text(member: &Member, constructor: bool) -> Option<String> {
    let access = member.access?;
    let descriptor = member.descriptor.as_deref()?;
    let (mask, default) = if constructor {
        (0x0007, false)
    } else {
        // `Method.isDefault`: public, neither abstract nor static, in an
        // interface.
        (
            0x0d3f,
            member.interface && access & (0x0400 | 0x0001 | 0x0008) == 0x0001,
        )
    };
    let modifiers = access & mask;
    let mut text = String::new();
    if !default {
        if modifiers != 0 {
            text.push_str(&modifier_words(modifiers));
            text.push(' ');
        }
    } else {
        if modifiers & 0x0007 != 0 {
            text.push_str(&modifier_words(modifiers & 0x0007));
            text.push(' ');
        }
        text.push_str("default ");
        if modifiers & !0x0007 != 0 {
            text.push_str(&modifier_words(modifiers & !0x0007));
            text.push(' ');
        }
    }
    let class = member.class.replace('/', ".");
    if constructor {
        text.push_str(&class);
    } else {
        text.push_str(&format!(
            "{} {class}.{}",
            type_name(return_desc(descriptor)),
            member.name
        ));
    }
    let parameters: Vec<String> = descriptor_arguments(descriptor)
        .iter()
        .map(|parameter| type_name(parameter))
        .collect();
    text.push_str(&format!("({})", parameters.join(",")));
    if !member.throws.is_empty() {
        text.push_str(" throws ");
        text.push_str(&member.throws.replace('/', "."));
    }
    Some(text)
}

/// `getDeclaringClass().getName().hashCode()`.
fn class_hash(member: &Member) -> i32 {
    java_string_hash(&member.class.replace('/', "."))
}

/// `Field.hashCode`/`Method.hashCode`: the declaring class's name's hash
/// `^` the name's, known only for a member the runtime read (an unread one's
/// declaring class may be a superclass).
fn member_hash(member: Option<Member>) -> Option<i32> {
    let member = member.filter(|member| member.access.is_some())?;
    Some(class_hash(&member) ^ java_string_hash(&member.name))
}

/// `equals` of two members of one kind: the same declaring class, name and
/// types is `true`; two read members that differ are `false`; anything else
/// rests on a lookup the runtime could not read (unknown).
fn member_equals(this: Option<Member>, other: Option<Member>) -> Option<bool> {
    let Some(other) = other else {
        // `obj instanceof Field/Method/Constructor` is false.
        return Some(false);
    };
    let this = this?;
    if this.identity() == other.identity() {
        return Some(true);
    }
    (this.access.is_some() && other.access.is_some()).then_some(false)
}

/// The JDK supertypes of the three classes (Java 25, `getSuperclass` and
/// `getInterfaces` closure). The classes are final, so a jar can add none.
fn instance_of(handle: &HostRef, wanted: &str, _: &dyn ClassHierarchy) -> Option<bool> {
    const MEMBER: &[&str] = &[
        "java/lang/Object",
        "java/lang/reflect/AccessibleObject",
        "java/lang/reflect/AnnotatedElement",
        "java/lang/reflect/Member",
    ];
    const EXECUTABLE: &[&str] = &[
        "java/lang/reflect/Executable",
        "java/lang/reflect/GenericDeclaration",
    ];
    Some(
        wanted == handle.class()
            || MEMBER.contains(&wanted)
            || (handle.kind() != HostKind::ReflectField && EXECUTABLE.contains(&wanted)),
    )
}

/// A `Field` copy is one object per key; a copy-less key (a lookup miss) is
/// not.
pub(crate) fn field_canonical(key: &str) -> bool {
    split_copy(key).1.is_some()
}

/// The same member's handle as `other`, read through the same parser.
fn other_of(other: &Value, kind: HostKind) -> Option<Member> {
    match kind {
        HostKind::ReflectField => field_of(other),
        HostKind::ReflectMethod => method_of(other),
        _ => constructor_of(other),
    }
}

fn this_of(handle: &HostRef) -> Option<Member> {
    parse(handle.key())
}

fn to_string(handle: &HostRef, _: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    let Some(member) = this_of(handle) else {
        return Ok(None);
    };
    Ok(match handle.kind() {
        HostKind::ReflectField => field_text(&member),
        HostKind::ReflectMethod => executable_text(&member, false),
        _ => executable_text(&member, true),
    })
}

fn hash_code(handle: &HostRef, _: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    Ok(match handle.kind() {
        // `Constructor.hashCode`: the declaring class's name's hash; a
        // constructor is never inherited, so the key's class is it.
        HostKind::ReflectConstructor => this_of(handle).map(|member| class_hash(&member)),
        _ => member_hash(this_of(handle)),
    })
}

fn equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(member_equals(
        this_of(handle),
        other_of(other, handle.kind()),
    ))
}

pub(crate) const MEMBER_METHODS: Methods = Methods {
    to_string,
    hash_code,
    equals,
    instance_of,
    // A member of one jar's classes means nothing in another jar's runtime.
    crosses_jars: |_, _| false,
    // `Field`, `Method` and `Constructor` are not `Comparable`.
    compare_to: None,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Keys read back into the member they were built from.
    #[test]
    fn keys_round_trip() {
        let declared = Declared {
            access: 0x0401,
            interface: true,
            throws: vec!["java/io/IOException".into(), "demo/E".into()],
        };
        let value = method(
            "demo/I",
            "name",
            Some("()Ljava/lang/String;"),
            Some(&declared),
        );
        let member = method_of(&value).unwrap();
        assert_eq!(member.descriptor.as_deref(), Some("()Ljava/lang/String;"));
        assert_eq!(member.access, Some(0x0401));
        assert!(member.interface);
        assert_eq!(member.throws, "java/io/IOException,demo/E");
        let member = constructor_of(&constructor("demo/C", Some("(I)V"), None)).unwrap();
        assert_eq!(
            (
                member.name.as_str(),
                member.descriptor.as_deref(),
                member.access
            ),
            ("<init>", Some("(I)V"), None)
        );
        let value = field("demo/C", "x", Some(("I", 9)), Some(12));
        assert_eq!(field_of(&value).unwrap().access, Some(9));
        assert!(field_canonical(member_key(&value).unwrap()));
        assert!(!field_canonical(field_copy_free(&value).unwrap()));
        // A string spelled like a key is no member.
        let text = Value::Ref(Some(member_key(&value).unwrap().to_owned()));
        assert_eq!(member_of(&text), None);
    }

    /// A deterministic generator (xorshift) of names over every character the
    /// key layers treat specially.
    struct Gen(u64);

    impl Gen {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn name(&mut self) -> String {
            const ALPHABET: &[&str] = &[
                ":", "#", "(", ")", ",", "\\", "/", ";", "I", "x", "0", "9", "25", "null", "i",
                "r", "l", "h", "rfld:", "COUNT", "é", "😀", "\\,", "#0", ":I:25",
            ];
            let length = self.next() % 8;
            (0..length)
                .map(|_| ALPHABET[(self.next() % ALPHABET.len() as u64) as usize])
                .collect()
        }
    }

    /// A lookup name containing any separator reads back as its own literal
    /// name with no facts, and never aliases another member: not a read
    /// member of the same class (`COUNT:I:25#0` is not `COUNT`'s copy 0),
    /// not a miss under another name, not a copy.
    #[test]
    fn spelled_names_round_trip_as_their_own_literal_names() {
        let real_field = field("demo/C", "COUNT", Some(("I", 25)), Some(0));
        let real_method = method(
            "demo/C",
            "target",
            Some("(I)I"),
            Some(&Declared {
                access: 9,
                interface: false,
                throws: Vec::new(),
            }),
        );
        let mut generator = Gen(0x2545_f491_4f6c_dd1d);
        let mut names: Vec<String> = [
            "COUNT:I:25#0",
            "COUNT:I:25",
            "COUNT#4",
            "target(I)I:9",
            "target(I)I",
            "x,i0",
            "x,i0#3",
            "rCOUNT",
            "",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        names.extend((0..3000).map(|_| generator.name()));
        let mut seen = std::collections::BTreeMap::new();
        for name in &names {
            for class in ["demo/C", name.as_str()] {
                let spelled = [
                    field(class, name, None, None),
                    method(class, name, None, None),
                    constructor(class, None, None),
                ];
                for value in &spelled {
                    let member = member_of(value).unwrap();
                    assert_eq!(member.class, class, "{name:?}");
                    assert_eq!((member.descriptor.as_ref(), member.access), (None, None));
                    assert!(!field_canonical(member_key(value).unwrap()), "{name:?}");
                    assert_ne!(member_key(value), member_key(&real_field), "{name:?}");
                    assert_ne!(member_key(value), member_key(&real_method), "{name:?}");
                }
                assert_eq!(field_of(&spelled[0]).unwrap().name, *name);
                assert_eq!(method_of(&spelled[1]).unwrap().name, *name);
                // Distinct (class, name) pairs have distinct keys.
                let key = member_key(&spelled[0]).unwrap().to_owned();
                if let Some(previous) = seen.insert(key, (class.to_owned(), name.clone())) {
                    assert_eq!(previous, (class.to_owned(), name.clone()));
                }
            }
            // A runtime-minted copy of a field whose real name is spelled so
            // keeps that name and its own copy.
            let copy = field("demo/C", name, Some(("I", 1)), Some(7));
            let member = field_of(&copy).unwrap();
            assert_eq!(
                (member.name.as_str(), member.access),
                (name.as_str(), Some(1))
            );
            assert!(field_canonical(member_key(&copy).unwrap()));
        }
    }
}
