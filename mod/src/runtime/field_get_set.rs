//! `java.lang.reflect.Field` reads and writes (`get`, `getBoolean` ...
//! `getDouble`, `set`, `setBoolean` ... `setDouble`) on fields the running jar
//! declares, answered exactly as JDK 27 answers them.
//!
//! Config loaders read the fields `Field#canAccess` allowed through the typed
//! getters (carryon's `ConfigLoader` calls `getBoolean`, `getDouble` and
//! `getInt`). Those calls reached the host, were counted and answered the
//! type's default.
//!
//! Ported from the JDK 27 bytecode of `java.lang.reflect.Field` (`get*`/`set*`,
//! `checkAccess`, `setFinal`), `jdk.internal.reflect.ReflectionFactory.newFieldAccessor`,
//! `MethodHandleAccessorFactory.newFieldAccessor`, `FieldAccessorImpl` and the
//! `MethodHandle*FieldAccessorImpl` classes:
//!
//! 1. Without `override`, `checkAccess(caller, obj)`: an instance field reads
//!    `obj.getClass()` first (a `null` object is a `NullPointerException`),
//!    then `verifyAccess` (the rules `field_access` ports for `canAccess`).
//! 2. The accessor is made, which initializes the declaring class.
//!    It is read-only when the field is final and either `override` is off or
//!    the field is trusted final (static, a record's, a hidden class's).
//! 3. A typed getter on a primitive field widens (`getLong` on an `int` field);
//!    anything else is `IllegalArgumentException("Attempt to get <type> field
//!    \"<class>.<name>\" with illegal data type conversion to <getter>")`,
//!    thrown before the receiver is looked at. A reference field has no typed
//!    getter. `get` boxes a primitive with `valueOf`.
//! 4. A typed setter that widens to the field's type converts the value and
//!    calls the field's own setter: a read-only field checks the receiver and
//!    throws `IllegalAccessException("Can not set final <type> field ... to
//!    (<type>)<value>")`, otherwise the receiver must be an instance of the
//!    declaring class (`"Can not set <type> field ... on <class>"`). A setter
//!    that does not widen throws `"Can not set ... to (<setter>)<value>"` first.
//! 5. `set(Object)` checks the receiver (`"... to <class>"`), then read-only,
//!    then null (`"... to null value"`), then unboxes the boxes that widen to
//!    the field's type; a reference field takes any value of its type.
//! 6. A static field ignores the receiver entirely.
//!
//! Decided only when nothing the answer rests on is invisible. Every other
//! call on a `Field` the reflection lookups handed out is counted at the host,
//! `get` and `set` included: none is answered without the JDK's checks.
//!
//! * the `Field` handle carries its flags and the declaring class is one of the
//!   running jar's own classes (fields of game, JDK or linked-jar classes are
//!   never modelled here);
//! * the `Field` object's own `override` is known (`reflect_override`);
//! * without `override`, the access check passes, the declaring jar's module
//!   is open to the caller under both loaders (`reflect_override`'s module
//!   docs: a caller from the same jar, or any caller when the jar has no
//!   `module-info.class`), and the caller's class is one the runtime holds. A
//!   failed check is an `IllegalAccessException` whose text names the
//!   caller's and the declaring class's modules when they are named (NeoForge)
//!   and not when they are not (Fabric); the runtime does not know which
//!   loader runs it, so that is counted. "Same package" is the package name:
//!   the same name in two jars is one runtime package under Knot's single
//!   loader, and cannot happen under FML, which forbids split packages;
//! * the receiver's class and the stored value's class are known: a jar
//!   object, a typed box, a plain string (the rule `instanceof String` uses,
//!   `value_class`), or a JDK object `jdk_supertypes` lists. A store whose
//!   answer does not depend on the value's class (any object into a writable
//!   `Object` field, a `Float`/`Double` box into a writable `double` field, a
//!   runtime list into a `List` field) is decided without it;
//! * with `override`, no final instance field of a normal class is written.
//!   OpenJDK 25, the Java 26.3 runs mods on, writes it silently; from JDK 26
//!   the write goes through JEP 500's final-field mutation policy, which the
//!   launch flags decide. Counting it stays correct under either.

use super::field_access::{FIELD, package_of};
use super::*;

const ACC_STATIC: u16 = 0x0008;
const ACC_FINAL: u16 = 0x0010;

/// A primitive field or value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prim {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
}

/// Each primitive's descriptor, `Class.getName()`, `get<Type>`/`set<Type>`
/// suffix and box class.
const PRIMS: [(Prim, &str, &str, &str, &str); 8] = [
    (
        Prim::Boolean,
        "Z",
        "boolean",
        "Boolean",
        "java/lang/Boolean",
    ),
    (Prim::Byte, "B", "byte", "Byte", "java/lang/Byte"),
    (Prim::Char, "C", "char", "Char", "java/lang/Character"),
    (Prim::Short, "S", "short", "Short", "java/lang/Short"),
    (Prim::Int, "I", "int", "Int", "java/lang/Integer"),
    (Prim::Long, "J", "long", "Long", "java/lang/Long"),
    (Prim::Float, "F", "float", "Float", "java/lang/Float"),
    (Prim::Double, "D", "double", "Double", "java/lang/Double"),
];

impl Prim {
    /// `PRIMS` lists the variants in declaration order.
    fn row(self) -> (Prim, &'static str, &'static str, &'static str, &'static str) {
        PRIMS[self as usize]
    }

    fn of_descriptor(descriptor: &str) -> Option<Self> {
        PRIMS
            .iter()
            .find(|row| row.1 == descriptor)
            .map(|row| row.0)
    }

    /// The type a `get<Type>`/`set<Type>` method name ends with.
    fn of_method_suffix(suffix: &str) -> Option<Self> {
        PRIMS.iter().find(|row| row.3 == suffix).map(|row| row.0)
    }

    fn descriptor(self) -> &'static str {
        self.row().1
    }

    fn name(self) -> &'static str {
        self.row().2
    }

    fn box_class(self) -> &'static str {
        self.row().4
    }

    /// JLS 5.1.2 widening (plus identity), the conversions the accessors allow.
    fn widens_to(self, to: Self) -> bool {
        use Prim::*;
        self == to
            || matches!(
                (self, to),
                (Byte, Short | Int | Long | Float | Double)
                    | (Short | Char, Int | Long | Float | Double)
                    | (Int, Long | Float | Double)
                    | (Long, Float | Double)
                    | (Float, Double)
            )
    }
}

/// A primitive value of a known type, in the interpreter's value model.
fn primitive(value: &Value, of: Prim) -> Option<Value> {
    match (of, value) {
        (Prim::Boolean, Value::Int(v)) => Some(Value::Int(i32::from(*v != 0))),
        (Prim::Byte, Value::Int(v)) => Some(Value::Int(i32::from(*v as i8))),
        (Prim::Char, Value::Int(v)) => Some(Value::Int(i32::from(*v as u16))),
        (Prim::Short, Value::Int(v)) => Some(Value::Int(i32::from(*v as i16))),
        (Prim::Int, Value::Int(v)) => Some(Value::Int(*v)),
        (Prim::Long, Value::Long(v)) => Some(Value::Long(*v)),
        (Prim::Long, Value::Int(v)) => Some(Value::Long(i64::from(*v))),
        (Prim::Float, Value::Float(v)) => Some(Value::Float(float32(*v))),
        (Prim::Double, Value::Float(v)) => Some(Value::Float(*v)),
        _ => None,
    }
}

/// Widen a primitive `value` to `to`, as `i2l`/`i2f`/`l2f`/`i2d`/`l2d` do.
/// An int-like value widening to another int-like type, and a `float` to a
/// `double`, keep their value (a `float` is stored already rounded).
fn widen(value: &Value, to: Prim) -> Value {
    match (to, value) {
        (Prim::Long, Value::Int(v)) => Value::Long(i64::from(*v)),
        (Prim::Float, Value::Int(v)) => Value::Float(f64::from(*v as f32)),
        (Prim::Float, Value::Long(v)) => Value::Float(f64::from(*v as f32)),
        (Prim::Double, Value::Int(v)) => Value::Float(f64::from(*v)),
        (Prim::Double, Value::Long(v)) => Value::Float(*v as f64),
        _ => value.clone(),
    }
}

/// `<Type>.toString(v)`, the text the accessors' messages print a value with.
fn value_text(value: &Value, of: Prim) -> String {
    match (of, value) {
        (Prim::Boolean, Value::Int(v)) => (*v != 0).to_string(),
        (Prim::Char, Value::Int(v)) => String::from_utf16_lossy(&[*v as u16]),
        (Prim::Float, Value::Float(v)) => format_java_float32(*v),
        (Prim::Double, Value::Float(v)) => format_java_float(*v),
        (_, Value::Int(v)) => v.to_string(),
        (_, Value::Long(v)) => v.to_string(),
        _ => String::new(),
    }
}

/// `valueOf` of a primitive, as the interpreter's own boxes look.
fn boxed(value: &Value, of: Prim) -> Value {
    match (of, value) {
        (Prim::Boolean, Value::Int(v)) => Value::Boxed(Boxed::Boolean(*v != 0)),
        (Prim::Byte, Value::Int(v)) => Value::Boxed(Boxed::Byte(*v as i8)),
        (Prim::Char, Value::Int(v)) => Value::Boxed(Boxed::Character(*v as u16)),
        (Prim::Short, Value::Int(v)) => Value::Boxed(Boxed::Short(*v as i16)),
        (Prim::Int, Value::Int(v)) => Value::Boxed(Boxed::Integer(*v)),
        (Prim::Float, Value::Float(v)) => Value::Boxed(Boxed::float(*v)),
        _ => value.clone(),
    }
}

/// The primitive inside a box whose class the value model keeps. A bare
/// `Value::Int` may be any of the int-like boxes and a bare `Value::Float` is
/// a `Double`, so neither is read here.
fn unbox(value: &Value) -> Option<(Prim, Value)> {
    let boxed = match value {
        Value::Long(v) => return Some((Prim::Long, Value::Long(*v))),
        Value::Boxed(boxed) => *boxed,
        _ => return None,
    };
    if let Boxed::Float(_) = boxed {
        return Some((Prim::Float, boxed.primitive()));
    }
    let of = match boxed {
        Boxed::Boolean(_) => Prim::Boolean,
        Boxed::Byte(_) => Prim::Byte,
        Boxed::Short(_) => Prim::Short,
        Boxed::Character(_) => Prim::Char,
        Boxed::Integer(_) => Prim::Int,
        Boxed::Float(_) => Prim::Float,
    };
    Some((of, Value::Int(boxed.int())))
}

/// `Class.getName()` of a field descriptor.
fn class_get_name(descriptor: &str) -> String {
    if let Some(of) = Prim::of_descriptor(descriptor) {
        return of.name().into();
    }
    if descriptor.starts_with('[') {
        return descriptor.replace('/', ".");
    }
    descriptor
        .strip_prefix('L')
        .and_then(|name| name.strip_suffix(';'))
        .unwrap_or(descriptor)
        .replace('/', ".")
}

/// The class of an object `new` made of a JDK class `jdk_supertypes` lists
/// (`java/util/ArrayList#3`).
fn jdk_object_class(marker: &str) -> Option<&str> {
    let (class, number) = marker.split_once('#')?;
    (!number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && jdk_supertypes::is_listed(class))
    .then_some(class)
}

fn thrown(class: &str, message: String) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: Some(message),
        frame: None,
    }
}

const ILLEGAL_ARGUMENT: &str = "java/lang/IllegalArgumentException";
const ILLEGAL_ACCESS: &str = "java/lang/IllegalAccessException";

/// One reflective read or write.
#[derive(Clone, Copy)]
enum Op {
    /// `get` (`None`) or `get<Type>`.
    Get(Option<Prim>),
    /// `set` (`None`) or `set<Type>`.
    Set(Option<Prim>),
}

impl Op {
    fn parse(name: &str, descriptor: &str) -> Option<Self> {
        match (name, descriptor) {
            ("get", "(Ljava/lang/Object;)Ljava/lang/Object;") => return Some(Self::Get(None)),
            ("set", "(Ljava/lang/Object;Ljava/lang/Object;)V") => return Some(Self::Set(None)),
            _ => {}
        }
        if let Some(of) = name.strip_prefix("get").and_then(Prim::of_method_suffix) {
            let wanted = format!("(Ljava/lang/Object;){}", of.descriptor());
            return (descriptor == wanted).then_some(Self::Get(Some(of)));
        }
        let of = name.strip_prefix("set").and_then(Prim::of_method_suffix)?;
        let wanted = format!("(Ljava/lang/Object;{})V", of.descriptor());
        (descriptor == wanted).then_some(Self::Set(Some(of)))
    }
}

/// The field one marker names, with what the messages print.
struct Target<'m> {
    declaring: &'m str,
    name: &'m str,
    descriptor: &'m str,
    access: u16,
}

impl Target<'_> {
    fn is_static(&self) -> bool {
        self.access & ACC_STATIC != 0
    }

    fn is_final(&self) -> bool {
        self.access & ACC_FINAL != 0
    }

    /// `FieldAccessorImpl.getQualifiedFieldName()`.
    fn qualified(&self) -> String {
        format!("{}.{}", self.declaring.replace('/', "."), self.name)
    }

    /// `" static"`/`" final"` then the type and the qualified name, the middle
    /// of every "Can not get/set" message.
    fn described(&self) -> String {
        format!(
            "{}{} {} field {}",
            if self.is_static() { " static" } else { "" },
            if self.is_final() { " final" } else { "" },
            class_get_name(self.descriptor),
            self.qualified()
        )
    }

    /// `getSetMessage(type, value)`: `"... to (<type>)<value>"`, `"... to
    /// <type>"`, or `"... to null value"`.
    fn set_message(&self, tail: &str) -> String {
        format!("Can not set{} to {tail}", self.described())
    }

    /// `MethodHandleFieldAccessorImpl.getMessage`: a receiver the handle's
    /// cast refused.
    fn on_message(&self, getter: bool, receiver: &str) -> String {
        format!(
            "Can not {}{} on {receiver}",
            if getter { "get" } else { "set" },
            self.described()
        )
    }

    fn get_conversion(&self, to: &str) -> NativeError {
        thrown(
            ILLEGAL_ARGUMENT,
            format!(
                "Attempt to get {} field \"{}\" with illegal data type conversion to {to}",
                class_get_name(self.descriptor),
                self.qualified()
            ),
        )
    }

    fn static_key(&self) -> String {
        format!("{}#{}{}", self.declaring, self.name, self.descriptor)
    }
}

/// What the receiver check of the accessor found.
enum Receiver {
    /// A static field, or an instance of the declaring class.
    Fits,
    /// An object of this (dotted) class, not an instance of the declaring one.
    Wrong(String),
}

/// `Class.isAssignableFrom` for a reference field whose type is a JDK box,
/// `Number` or `Object`, from a box of known class. `None` for any other type.
fn box_assignable(field_class: &str, box_class: &str) -> Option<bool> {
    match field_class {
        "java/lang/Object" => Some(true),
        "java/lang/Number" => Some(!matches!(
            box_class,
            "java/lang/Boolean" | "java/lang/Character"
        )),
        "java/lang/Boolean"
        | "java/lang/Byte"
        | "java/lang/Character"
        | "java/lang/Short"
        | "java/lang/Integer"
        | "java/lang/Long"
        | "java/lang/Float"
        | "java/lang/Double" => Some(field_class == box_class),
        _ => None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// Answer one `Field#get*`/`set*` call on a `Field` the reflection
    /// lookups handed out where JDK 27's answer can be read off the classes
    /// the runtime holds, and count every other one at the host. `None` only
    /// for a receiver that is no such `Field` (the rest of the dispatch).
    pub(super) fn invoke_field_get_set(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
        caller: &str,
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != FIELD {
            return Ok(None);
        }
        let Some(op) = Op::parse(&r.name, &r.descriptor) else {
            return Ok(None);
        };
        let Some(receiver) = receiver.filter(|receiver| host_reflect::field_of(receiver).is_some())
        else {
            return Ok(None);
        };
        if let Some(value) = self.decide_field_get_set(op, receiver, args, caller)? {
            return Ok(Some(value));
        }
        // Undecided: counted, never answered without the JDK's checks.
        self.host.invoke_instance(r, receiver, args).map(Some)
    }

    /// [`Self::invoke_field_get_set`]'s answer, or `None` when it rests on
    /// something the runtime cannot see (module docs).
    fn decide_field_get_set(
        &mut self,
        op: Op,
        receiver: &Value,
        args: &[Value],
        caller: &str,
    ) -> Result<Option<Value>, NativeError> {
        // Only a `Field` `getDeclaredField(s)`/`getField(s)` built from the
        // declaring class's own field table carries its flags.
        let Some(host_reflect::Member {
            class: declaring,
            name,
            descriptor: Some(descriptor),
            access: Some(access),
            ..
        }) = host_reflect::field_of(receiver)
        else {
            return Ok(None);
        };
        if !self.classes.contains_key(declaring) {
            return Ok(None);
        }
        let field = Target {
            declaring,
            name,
            descriptor,
            access,
        };
        // This `Field` object's own `override` (`reflect_override`).
        let Some(overridden) = self.field_override(receiver) else {
            return Ok(None);
        };
        let object = args.first().cloned().unwrap_or(Value::Ref(None));

        // 1. `checkAccess`, without `override`.
        let target_class = if field.is_static() || overridden {
            None
        } else if matches!(object, Value::Ref(None)) {
            return Err(thrown(
                "java/lang/NullPointerException",
                "Cannot invoke \"Object.getClass()\" because \"obj\" is null".into(),
            ));
        } else {
            match self.object_class_name(&object) {
                Some(class) => Some(class),
                None => return Ok(None),
            }
        };
        if !overridden && caller != declaring {
            // The member rules read the caller's superclasses and nest.
            if !self.module_open_to(declaring, caller) || self.class(caller).is_none() {
                return Ok(None);
            }
            let Some(class_flags) = self.class(declaring).and_then(checkcast::class_access) else {
                return Ok(None);
            };
            let same_package = package_of(caller) == package_of(declaring);
            // The JLS 6.6.2 check reads the object's class only for a jar
            // class; a JDK object is never the caller's subclass.
            let target = target_class
                .as_deref()
                .filter(|class| self.classes.contains_key(*class));
            match self.member_access(
                caller,
                declaring,
                target,
                access,
                class_flags,
                same_package,
            )? {
                Some(true) => {}
                // `IllegalAccessException`, whose text depends on modules.
                Some(false) | None => return Ok(None),
            }
        }
        // With `override`, a final instance field of a normal class is written
        // through the final-field mutation policy (JEP 500): the JVM's flags.
        if overridden
            && matches!(op, Op::Set(_))
            && field.is_final()
            && !field.is_static()
            && !self.is_record(declaring)
        {
            return Ok(None);
        }

        // 2. The accessor: making it initializes the declaring class.
        self.ensure_initialized(declaring)?;
        if overridden && !field.is_static() && matches!(object, Value::Ref(None)) {
            return self.null_object_with_override(&field, op, args);
        }
        match op {
            Op::Get(getter) => self.reflect_get(&field, &object, getter),
            Op::Set(None) => {
                let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
                self.reflect_set_object(&field, &object, &value)
            }
            Op::Set(Some(setter)) => {
                let Some(value) = args.get(1).and_then(|value| primitive(value, setter)) else {
                    return Ok(None);
                };
                self.reflect_set_primitive(&field, &object, setter, &value)
            }
        }
    }

    /// An instance field reached with a `null` object after
    /// `setAccessible(true)`: `checkAccess`, which reads `obj.getClass()`, is
    /// skipped, so the accessor sees the null. OpenJDK 27: a conversion the
    /// accessor does not have still throws first; otherwise a getter or a
    /// typed setter throws the method handle's `NullPointerException`, which
    /// has no message, and `set(Object)` throws from `ensureObj`
    /// (`Cannot invoke "Object.getClass()" because "o" is null`).
    fn null_object_with_override(
        &self,
        field: &Target<'_>,
        op: Op,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let of = Prim::of_descriptor(field.descriptor);
        let npe = |message: Option<&str>| NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: message.map(str::to_owned),
            frame: None,
        };
        match op {
            Op::Get(Some(getter)) if !of.is_some_and(|of| of.widens_to(getter)) => {
                Err(field.get_conversion(getter.name()))
            }
            Op::Get(_) => Err(npe(None)),
            Op::Set(None) => Err(npe(Some(
                "Cannot invoke \"Object.getClass()\" because \"o\" is null",
            ))),
            Op::Set(Some(setter)) => {
                let Some(value) = args.get(1).and_then(|value| primitive(value, setter)) else {
                    return Ok(None);
                };
                if !of.is_some_and(|of| setter.widens_to(of)) {
                    return Err(thrown(
                        ILLEGAL_ARGUMENT,
                        field.set_message(&format!(
                            "({}){}",
                            setter.name(),
                            value_text(&value, setter)
                        )),
                    ));
                }
                // A read-only (record) field runs `ensureObj` before its
                // final-field exception; not observed on the JDK here.
                if field.is_final() {
                    return Ok(None);
                }
                Err(npe(None))
            }
        }
    }

    fn reflect_get(
        &self,
        field: &Target<'_>,
        object: &Value,
        getter: Option<Prim>,
    ) -> Result<Option<Value>, NativeError> {
        let of = Prim::of_descriptor(field.descriptor);
        // A conversion the accessor does not have throws before the handle
        // ever sees the receiver.
        if let Some(getter) = getter
            && !of.is_some_and(|of| of.widens_to(getter))
        {
            return Err(field.get_conversion(getter.name()));
        }
        match self.receiver_fits(field, object) {
            Some(Receiver::Fits) => {}
            Some(Receiver::Wrong(class)) => {
                return Err(thrown(ILLEGAL_ARGUMENT, field.on_message(true, &class)));
            }
            None => return Ok(None),
        }
        let Some(stored) = self.stored(field, object)? else {
            return Ok(None);
        };
        Ok(Some(match (of, getter) {
            (Some(_), Some(getter)) => widen(&stored, getter),
            (Some(of), None) => boxed(&stored, of),
            (None, _) => stored,
        }))
    }

    fn reflect_set_primitive(
        &mut self,
        field: &Target<'_>,
        object: &Value,
        setter: Prim,
        value: &Value,
    ) -> Result<Option<Value>, NativeError> {
        let Some(of) = Prim::of_descriptor(field.descriptor).filter(|of| setter.widens_to(*of))
        else {
            return Err(thrown(
                ILLEGAL_ARGUMENT,
                field.set_message(&format!("({}){}", setter.name(), value_text(value, setter))),
            ));
        };
        // `set<Narrower>` widens and calls the field's own setter.
        let value = widen(value, of);
        if field.is_final() {
            // Read-only: `ensureObj`, then the final-field exception.
            match self.receiver_fits(field, object) {
                Some(Receiver::Fits) => {}
                Some(Receiver::Wrong(class)) => {
                    return Err(thrown(ILLEGAL_ARGUMENT, field.set_message(&class)));
                }
                None => return Ok(None),
            }
            return Err(thrown(
                ILLEGAL_ACCESS,
                field.set_message(&format!("({}){}", of.name(), value_text(&value, of))),
            ));
        }
        match self.receiver_fits(field, object) {
            Some(Receiver::Fits) => {}
            Some(Receiver::Wrong(class)) => {
                return Err(thrown(ILLEGAL_ARGUMENT, field.on_message(false, &class)));
            }
            None => return Ok(None),
        }
        self.store(field, object, value);
        Ok(Some(Value::Void))
    }

    fn reflect_set_object(
        &mut self,
        field: &Target<'_>,
        object: &Value,
        value: &Value,
    ) -> Result<Option<Value>, NativeError> {
        // `ensureObj` comes first on every accessor.
        match self.receiver_fits(field, object) {
            Some(Receiver::Fits) => {}
            Some(Receiver::Wrong(class)) => {
                return Err(thrown(ILLEGAL_ARGUMENT, field.set_message(&class)));
            }
            None => return Ok(None),
        }
        let value_class = match value {
            Value::Ref(None) => None,
            other => match self.object_class_name(other) {
                Some(class) => Some(class),
                // The value's class is unknown, which only the messages and
                // the type checks read. A writable `Object` field takes any
                // object, and a writable `double` field unboxes a `Float` and
                // a `Double` box (which share `Value::Float`) to the same
                // value, so neither store depends on it.
                None if !field.is_final() && self.stores_without_class(field, other) => {
                    self.store(field, object, other.clone());
                    return Ok(Some(Value::Void));
                }
                None => return Ok(None),
            },
        };
        if field.is_final() {
            return Err(thrown(
                ILLEGAL_ACCESS,
                field.set_message(value_class.as_deref().unwrap_or("null value")),
            ));
        }
        let stored = if let Some(of) = Prim::of_descriptor(field.descriptor) {
            let Some(class) = value_class else {
                return Err(thrown(ILLEGAL_ARGUMENT, field.set_message("null value")));
            };
            match unbox(value) {
                Some((from, unboxed)) if from.widens_to(of) => widen(&unboxed, of),
                Some(_) => return Err(thrown(ILLEGAL_ARGUMENT, field.set_message(&class))),
                None => return Ok(None),
            }
        } else {
            let field_class = field
                .descriptor
                .strip_prefix('L')
                .and_then(|name| name.strip_suffix(';'));
            match (field_class, &value_class) {
                (_, None) => Value::Ref(None),
                (Some(field_class), Some(class)) => {
                    match self.reference_assignable(value, field_class) {
                        Some(true) => value.clone(),
                        Some(false) => {
                            return Err(thrown(ILLEGAL_ARGUMENT, field.set_message(class)));
                        }
                        None => return Ok(None),
                    }
                }
                // An array field.
                (None, Some(_)) => return Ok(None),
            }
        };
        self.store(field, object, stored);
        Ok(Some(Value::Void))
    }

    /// Whether a writable `field` takes `value` whatever the value's class:
    /// an `Object` field takes any object; a `double` field unboxes a `Float`
    /// and a `Double` box (which share `Value::Float`) to the same value; a
    /// field of an interface every runtime list, set or map marker implements
    /// (`List`, `Collection`...) takes one.
    fn stores_without_class(&self, field: &Target<'_>, value: &Value) -> bool {
        if matches!(value, Value::Void) {
            return false;
        }
        if field.descriptor == "D" {
            return matches!(value, Value::Float(_));
        }
        let Some(field_class) = field
            .descriptor
            .strip_prefix('L')
            .and_then(|name| name.strip_suffix(';'))
        else {
            return false;
        };
        field_class == "java/lang/Object"
            || expect_ref(value)
                .is_some_and(|marker| jdk_supertypes::collection_marker_is(marker, field_class))
    }

    /// Whether `value` (a box of known class, a plain string, a JDK object
    /// `jdk_supertypes` lists or a jar object) can be stored in a field of
    /// class `field_class`. `None` where that is not decidable.
    fn reference_assignable(&self, value: &Value, field_class: &str) -> Option<bool> {
        if field_class == "java/lang/Object" {
            return Some(true);
        }
        if let Some(marker) = expect_ref(value) {
            // `String` is final: a string is an instance of its own
            // supertypes and nothing else.
            if value_class::is_plain_string(marker) {
                return Some(value_class::string_instance_of(marker, field_class));
            }
            if let Some(class) = jdk_object_class(marker) {
                return jdk_supertypes::jdk_instance_of(class, field_class);
            }
        }
        if let Some((of, _)) = unbox(value) {
            return box_assignable(field_class, of.box_class())
                .or_else(|| jdk_supertypes::jdk_instance_of(of.box_class(), field_class));
        }
        // A jar object into a jar type.
        if self.classes.contains_key(field_class) {
            return self.known_instance_of(value, field_class);
        }
        // The box classes are final, so no jar object is one. Any other JDK
        // type (`Number`, an interface) a jar class may extend or implement.
        PRIMS
            .iter()
            .any(|row| row.4 == field_class)
            .then_some(false)
    }

    /// The dotted `getClass().getName()` of a jar object, a typed box, a
    /// plain string (by the rule `instanceof String` uses) or a JDK object
    /// `jdk_supertypes` lists.
    fn object_class_name(&self, value: &Value) -> Option<String> {
        if let Some((of, _)) = unbox(value) {
            return Some(of.box_class().replace('/', "."));
        }
        if let Some(class) = self.known_object_class(value) {
            return Some(class.replace('/', "."));
        }
        let marker = expect_ref(value)?;
        if let Some(class) = jdk_object_class(marker) {
            return Some(class.replace('/', "."));
        }
        value_class::is_plain_string(marker).then(|| "java.lang.String".to_owned())
    }

    /// The accessor's receiver check: none for a static field; otherwise an
    /// instance of the declaring class.
    fn receiver_fits(&self, field: &Target<'_>, object: &Value) -> Option<Receiver> {
        if field.is_static() {
            return Some(Receiver::Fits);
        }
        match self.known_instance_of(object, field.declaring)? {
            true => Some(Receiver::Fits),
            false => self.object_class_name(object).map(Receiver::Wrong),
        }
    }

    /// The field's current value, as its type's value; `None` when the slot
    /// holds something that is not one.
    fn stored(&self, field: &Target<'_>, object: &Value) -> Result<Option<Value>, NativeError> {
        let held = if field.is_static() {
            match self.static_fields.get(&field.static_key()) {
                Some(value) => Some(value.clone()),
                // Preparation set a `ConstantValue` field before `<clinit>`.
                None => self.constant_value_of(field)?,
            }
        } else {
            let Some(object) = expect_ref(object) else {
                return Ok(None);
            };
            self.fields
                .get(&format!("{object}|{}", field.static_key()))
                .cloned()
        };
        let value = held.unwrap_or_else(|| Value::default_for(field.descriptor));
        Ok(match Prim::of_descriptor(field.descriptor) {
            Some(of) => primitive(&value, of),
            // A reference field hands back the object in its slot, whatever
            // its class: a box the value model keeps as `Long`/`Float`/`Int`
            // is still the object `set` stored.
            None => match value {
                Value::Void => None,
                _ => Some(value),
            },
        })
    }

    fn store(&mut self, field: &Target<'_>, object: &Value, value: Value) {
        if field.is_static() {
            self.static_fields.insert(field.static_key(), value);
        } else if let Some(object) = expect_ref(object) {
            self.fields
                .insert(format!("{object}|{}", field.static_key()), value);
        }
    }

    /// `Class.isRecord()`, as `jdk_model` reads it: a final direct subclass of
    /// `java.lang.Record`.
    fn is_record(&self, class: &str) -> bool {
        self.class(class).is_some_and(|file| {
            file.super_name == "java/lang/Record"
                && checkcast::class_access(file).is_some_and(|flags| flags & ACC_FINAL != 0)
        })
    }

    /// A static field's `ConstantValue` attribute (JVMS 4.7.2), read from the
    /// declaring class's bytes.
    fn constant_value_of(&self, field: &Target<'_>) -> Result<Option<Value>, NativeError> {
        let Some(file) = self.class(field.declaring) else {
            return Ok(None);
        };
        if file.raw.is_empty() {
            return Ok(None);
        }
        let mut reader = Reader::new(&file.raw);
        reader.skip(8)?;
        let pool = read_pool(&mut reader)?;
        reader.skip(6)?; // access, this, super
        let interfaces = reader.u2()? as usize;
        reader.skip(interfaces * 2)?;
        for _ in 0..reader.u2()? {
            reader.skip(2)?; // access
            let name = utf8(&pool, reader.u2()?)?;
            let descriptor = utf8(&pool, reader.u2()?)?;
            let matches = name == field.name && descriptor == field.descriptor;
            for _ in 0..reader.u2()? {
                let attribute = utf8(&pool, reader.u2()?)?;
                let length = reader.u4()? as usize;
                if matches && attribute == "ConstantValue" {
                    let index = reader.u2()?;
                    return Ok(Some(match pool.get(index as usize) {
                        Some(Cp::Double(_)) => constant_wide_value(&pool, index)?,
                        _ => constant_value(&pool, index)?,
                    }));
                }
                reader.skip(length)?;
            }
            if matches {
                return Ok(None);
            }
        }
        Ok(None)
    }
}
