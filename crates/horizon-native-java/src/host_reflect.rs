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
//! A key names the member and, when the lookup found it in a class the
//! runtime holds, what `toString` prints about it (its access flags, whether
//! the declaring class is an interface, its `throws` clause). A key without
//! them is a member the runtime could not read (a game or JDK class, or a
//! lookup that named no exact member); its `toString` and `hashCode` are
//! unknown and counted, since the declaring class may be a superclass.
//!
//! * `Field`: `<class>#<name>[:<descriptor>:<access>][#<copy>]`;
//! * `Method`: `<class>#<name>[<descriptor>[:<access>[i][:<throws>]]]`, `i`
//!   when the declaring class is an interface, `<throws>` the `Exceptions`
//!   attribute's classes joined by `,`;
//! * `Constructor`: `<class>#[<descriptor>[:<access>[:<throws>]]]`.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Member<'a> {
    pub(crate) class: &'a str,
    /// The member's name; `<init>` for a constructor.
    pub(crate) name: &'a str,
    /// A method's or constructor's descriptor, a field's type descriptor;
    /// `None` when the lookup named no exact member.
    pub(crate) descriptor: Option<&'a str>,
    /// The class file's access flags, when the runtime read the member.
    pub(crate) access: Option<u16>,
    /// Whether the declaring class is an interface (methods only).
    pub(crate) interface: bool,
    /// The `Exceptions` attribute's classes joined by `,` (executables).
    pub(crate) throws: &'a str,
}

impl Member<'_> {
    /// `<class>#<name><descriptor>`: what `equals` compares (declaring class,
    /// name and, for a field, its type; for an executable its parameter and
    /// return types).
    fn identity(&self) -> (&str, &str, Option<&str>) {
        (self.class, self.name, self.descriptor)
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

/// A `Field` handle: `declared` is `(descriptor, access)` when the class's
/// field table named it, and `copy` the copy number a lookup gives it.
pub(crate) fn field(
    class: &str,
    name: &str,
    declared: Option<(&str, u16)>,
    copy: Option<u64>,
) -> Value {
    let mut key = format!("{class}#{name}");
    if let Some((descriptor, access)) = declared {
        key.push_str(&format!(":{descriptor}:{access}"));
        if let Some(copy) = copy {
            key.push_str(&format!("#{copy}"));
        }
    }
    Value::Host(HostRef::new(HostKind::ReflectField, key))
}

fn executable_key(head: String, declared: Option<&Declared>, interface_flag: bool) -> String {
    let mut key = head;
    if let Some(declared) = declared {
        key.push_str(&format!(":{}", declared.access));
        if interface_flag && declared.interface {
            key.push('i');
        }
        if !declared.throws.is_empty() {
            key.push(':');
            key.push_str(&declared.throws.join(","));
        }
    }
    key
}

/// A `Method` handle (`declared` only with a descriptor).
pub(crate) fn method(
    class: &str,
    name: &str,
    descriptor: Option<&str>,
    declared: Option<&Declared>,
) -> Value {
    let head = format!("{class}#{name}{}", descriptor.unwrap_or_default());
    let declared = declared.filter(|_| descriptor.is_some());
    Value::Host(HostRef::new(
        HostKind::ReflectMethod,
        executable_key(head, declared, true),
    ))
}

/// A `Constructor` handle (`declared` only with a descriptor).
pub(crate) fn constructor(
    class: &str,
    descriptor: Option<&str>,
    declared: Option<&Declared>,
) -> Value {
    let head = format!("{class}#{}", descriptor.unwrap_or_default());
    let declared = declared.filter(|_| descriptor.is_some());
    Value::Host(HostRef::new(
        HostKind::ReflectConstructor,
        executable_key(head, declared, false),
    ))
}

/// The copy number a lookup appended to a field key's member part
/// (`name:descriptor:flags#n`): `(member, copy)`.
fn split_field_copy(member: &str) -> (&str, Option<&str>) {
    match member.rsplit_once('#') {
        Some((field, number))
            if !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
                && field.matches(':').count() == 2 =>
        {
            (field, Some(number))
        }
        _ => (member, None),
    }
}

fn parse_field(key: &str) -> Option<Member<'_>> {
    let (class, member) = key.split_once('#')?;
    let (member, _) = split_field_copy(member);
    let mut parts = member.split(':');
    let name = parts.next().unwrap_or(member);
    let descriptor = parts.next();
    let access = parts.next().and_then(|access| access.parse().ok());
    Some(Member {
        class,
        name,
        descriptor,
        access,
        interface: false,
        throws: "",
    })
}

/// `<descriptor>[:<access>[i][:<throws>]]` after an executable's name.
fn parse_executable<'a>(class: &'a str, name: &'a str, rest: &'a str) -> Member<'a> {
    let mut parts = rest.splitn(3, ':');
    let descriptor = parts.next().filter(|descriptor| !descriptor.is_empty());
    let flags = parts.next();
    let interface = flags.is_some_and(|flags| flags.ends_with('i'));
    let access = flags.and_then(|flags| flags.trim_end_matches('i').parse().ok());
    Member {
        class,
        name,
        descriptor,
        access,
        interface,
        throws: parts.next().unwrap_or_default(),
    }
}

fn parse_method(key: &str) -> Option<Member<'_>> {
    let (class, member) = key.split_once('#')?;
    Some(match member.find('(') {
        Some(at) => parse_executable(class, &member[..at], &member[at..]),
        None => parse_executable(class, member, ""),
    })
}

fn parse_constructor(key: &str) -> Option<Member<'_>> {
    let (class, rest) = key.split_once('#')?;
    Some(parse_executable(class, "<init>", rest))
}

fn handle_of(value: &Value, kind: HostKind) -> Option<&HostRef> {
    match value {
        Value::Host(handle) if handle.kind() == kind => Some(handle),
        _ => None,
    }
}

/// The member a `Field` handle names.
pub(crate) fn field_of(value: &Value) -> Option<Member<'_>> {
    parse_field(handle_of(value, HostKind::ReflectField)?.key())
}

/// The member a `Method` handle names.
pub(crate) fn method_of(value: &Value) -> Option<Member<'_>> {
    parse_method(handle_of(value, HostKind::ReflectMethod)?.key())
}

/// The member a `Constructor` handle names (`name` is `<init>`).
pub(crate) fn constructor_of(value: &Value) -> Option<Member<'_>> {
    parse_constructor(handle_of(value, HostKind::ReflectConstructor)?.key())
}

/// The member any of the three handles names.
pub(crate) fn member_of(value: &Value) -> Option<Member<'_>> {
    field_of(value)
        .or_else(|| method_of(value))
        .or_else(|| constructor_of(value))
}

/// The key every copy of a field shares: what the `override` table keys a
/// field by (`reflect_override.rs`), and what a copy-less key is as is.
pub(crate) fn field_copy_free(value: &Value) -> Option<&str> {
    let key = handle_of(value, HostKind::ReflectField)?.key();
    let (class, member) = key.split_once('#')?;
    let (stripped, _) = split_field_copy(member);
    Some(&key[..class.len() + 1 + stripped.len()])
}

/// The key of a reflection handle of any of the three kinds: the receiver
/// text of its entries in the runtime's field table (`reflect_override.rs`).
/// The key itself, not its encoding, so the heap collector finds a `Field`
/// copy's entries through the copy's own key (`heap.rs`).
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
fn field_text(field: &Member<'_>) -> Option<String> {
    let modifiers = field.access? & 0x50df;
    let prefix = if modifiers == 0 {
        String::new()
    } else {
        format!("{} ", modifier_words(modifiers))
    };
    Some(format!(
        "{prefix}{} {}.{}",
        type_name(field.descriptor?),
        field.class.replace('/', "."),
        field.name
    ))
}

/// `Executable.sharedToString`: the modifiers under `mask` (with `default`
/// after the access word for an interface's default method), the header
/// (`Method`: return type, class and name; `Constructor`: class), the
/// parameter type names joined by `,`, and ` throws ` with the
/// `Exceptions` attribute's classes.
fn executable_text(member: &Member<'_>, constructor: bool) -> Option<String> {
    let access = member.access?;
    let descriptor = member.descriptor?;
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
fn class_hash(member: &Member<'_>) -> i32 {
    java_string_hash(&member.class.replace('/', "."))
}

/// `Field.hashCode`/`Method.hashCode`: the declaring class's name's hash
/// `^` the name's, known only for a member the runtime read (an unread one's
/// declaring class may be a superclass).
fn member_hash(member: Option<Member<'_>>) -> Option<i32> {
    let member = member.filter(|member| member.access.is_some())?;
    Some(class_hash(&member) ^ java_string_hash(member.name))
}

/// `equals` of two members of one kind: the same declaring class, name and
/// types is `true`; two read members that differ are `false`; anything else
/// rests on a lookup the runtime could not read (unknown).
fn member_equals(this: Option<Member<'_>>, other: Option<Member<'_>>) -> Option<bool> {
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
    key.split_once('#')
        .is_some_and(|(_, member)| split_field_copy(member).1.is_some())
}

/// The same member's handle as `other`, read through the same parser.
fn other_of(other: &Value, kind: HostKind) -> Option<Member<'_>> {
    match kind {
        HostKind::ReflectField => field_of(other),
        HostKind::ReflectMethod => method_of(other),
        _ => constructor_of(other),
    }
}

fn this_of(handle: &HostRef) -> Option<Member<'_>> {
    match handle.kind() {
        HostKind::ReflectField => parse_field(handle.key()),
        HostKind::ReflectMethod => parse_method(handle.key()),
        _ => parse_constructor(handle.key()),
    }
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
        assert_eq!(member.descriptor, Some("()Ljava/lang/String;"));
        assert_eq!(member.access, Some(0x0401));
        assert!(member.interface);
        assert_eq!(member.throws, "java/io/IOException,demo/E");
        let value = constructor("demo/C", Some("(I)V"), None);
        let member = constructor_of(&value).unwrap();
        assert_eq!(
            (member.name, member.descriptor, member.access),
            ("<init>", Some("(I)V"), None)
        );
        let value = field("demo/C", "x", Some(("I", 9)), Some(12));
        assert_eq!(field_copy_free(&value), Some("demo/C#x:I:9"));
        assert_eq!(field_of(&value).unwrap().access, Some(9));
        assert!(field_canonical(member_key(&value).unwrap()));
        assert!(!field_canonical("demo/C#x"));
        // A string spelled like a key is no member.
        assert_eq!(member_of(&Value::Ref(Some("demo/C#x:I:9#12".into()))), None);
    }
}
