//! `java.lang.reflect.Field#canAccess(Object)`, answered with the JDK's own
//! access rules where the runtime holds every class they read.
//!
//! Config loaders walk a config object's declared fields and keep the ones
//! `field.canAccess(instance)` allows (carryon's `ConfigLoader.buildCategory`
//! does exactly that). The call used to reach the host and answer `false`, so
//! every field was skipped and the mod built an empty config.
//!
//! Ported from the JDK 27 bytecode of `AccessibleObject.canAccess`,
//! `AccessibleObject.verifyAccess` and `jdk.internal.reflect.Reflection.verifyMemberAccess`:
//!
//! 1. The receiver check: an instance field needs a non-null object that is an
//!    instance of the declaring class (`IllegalArgumentException("null object
//!    for <field>")`, `"object is not an instance of <class>"`); a static field
//!    needs `null` (`"non-null object for <field>"`).
//! 2. `override` (`setAccessible(true)` on this `Field` object, as
//!    `reflect_override` tracks it) answers `true`.
//! 3. `verifyMemberAccess(caller, declaring, target, modifiers)`: the same class
//!    is allowed; then module access; a non-public declaring class needs the
//!    same package; a public member is allowed; a private one needs nestmates;
//!    a protected one a subclass caller (or the same package), with the JLS
//!    6.6.2 check that a protected *instance* member's object is the caller's
//!    subclass; a package-private one the same package.
//!
//! The caller is the class of the frame that executes `canAccess` (the JDK's
//! `Reflection.getCallerClass()`). The answer is decided only when it does not
//! rest on something the runtime cannot see; otherwise the call goes on to the
//! host and is counted, as before:
//!
//! * the field marker must carry its access flags and the declaring class must
//!   be one the runtime holds with its class-file bytes;
//! * the declaring class must be one of the running jar's own classes, and
//!   its module open to the caller under both loaders (`reflect_override`: a
//!   caller from the same jar, or any caller when the jar has no
//!   `module-info.class`), so the module check passes; the caller's class
//!   must be one the runtime holds. "Same package" is the package name (see
//!   `field_get_set`'s docs);
//! * the object's class must be decidable (`checkcast`'s rules);
//! * the `Field` object's `override` must be known (`reflect_override`): a
//!   field read back out of a collection is no particular copy, and its flag
//!   is known only when every copy of the field agrees;
//! * a private member's nest host must be readable (`NestHost` naming a class
//!   the runtime holds).

use super::*;

pub(super) const FIELD: &str = "java/lang/reflect/Field";

const ACC_PUBLIC: u16 = 0x0001;
const ACC_PRIVATE: u16 = 0x0002;
const ACC_PROTECTED: u16 = 0x0004;
const ACC_STATIC: u16 = 0x0008;

fn illegal_argument(message: String) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message),
        frame: None,
    }
}

pub(super) fn package_of(class: &str) -> &str {
    class.rsplit_once('/').map_or("", |(package, _)| package)
}

/// `Class.getTypeName()` of a field descriptor.
fn type_name(descriptor: &str) -> String {
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
        other => other
            .strip_prefix('L')
            .and_then(|name| name.strip_suffix(';'))
            .unwrap_or(other)
            .replace('/', "."),
    }
}

/// `Field.toString()`: `Modifier.toString(mods) + " "` (when `mods != 0`),
/// the type's and the declaring class's type names, and the field's name.
fn field_text(declaring: &str, name: &str, descriptor: &str, access: u16) -> String {
    // The JVM's recognized field modifiers (`getModifiers()`), printed in
    // `Modifier.toString`'s order.
    let modifiers = access & 0x50df;
    let words: Vec<&str> = [
        (0x0001, "public"),
        (0x0004, "protected"),
        (0x0002, "private"),
        (0x0008, "static"),
        (0x0010, "final"),
        (0x0080, "transient"),
        (0x0040, "volatile"),
    ]
    .into_iter()
    .filter(|(bit, _)| modifiers & bit != 0)
    .map(|(_, word)| word)
    .collect();
    let prefix = if modifiers == 0 {
        String::new()
    } else {
        format!("{} ", words.join(" "))
    };
    format!(
        "{prefix}{} {}.{name}",
        type_name(descriptor),
        declaring.replace('/', ".")
    )
}

/// The `NestHost` class a class file names, if it has the attribute.
fn nest_host_attribute(file: &ClassFile) -> Result<Option<String>, NativeError> {
    class_attribute_classes(file, "NestHost").map(|names| names.into_iter().next())
}

/// The classes a `NestHost` (one) or `NestMembers` (many) attribute lists.
fn class_attribute_classes(file: &ClassFile, wanted: &str) -> Result<Vec<String>, NativeError> {
    let mut reader = Reader::new(&file.raw);
    reader.skip(8)?;
    let pool = read_pool(&mut reader)?;
    reader.skip(6)?; // access, this, super
    let interfaces = reader.u2()? as usize;
    reader.skip(interfaces * 2)?;
    for _ in 0..2 {
        // fields, then methods
        let members = reader.u2()?;
        for _ in 0..members {
            reader.skip(6)?;
            skip_attributes(&mut reader, &pool)?;
        }
    }
    let attributes = reader.u2()?;
    for _ in 0..attributes {
        let name = utf8(&pool, reader.u2()?)?;
        let length = reader.u4()? as usize;
        if name != wanted {
            reader.skip(length)?;
            continue;
        }
        let count = if wanted == "NestHost" {
            1
        } else {
            reader.u2()?
        };
        return (0..count)
            .map(|_| class_name(&pool, reader.u2()?))
            .collect();
    }
    Ok(Vec::new())
}

impl<H: Host> Runtime<'_, H> {
    /// Answer `Field#canAccess(Object)` where the JDK's rules can be read off
    /// the classes the runtime holds, after `reflect_override`'s
    /// `setAccessible` family. `None` leaves the call to the rest of the
    /// dispatch (counted).
    pub(super) fn invoke_field_access(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
        caller: &str,
    ) -> Result<Option<Value>, NativeError> {
        if let Some(value) = self.invoke_reflect_override(r, receiver, args, caller)? {
            return Ok(Some(value));
        }
        if r.owner != FIELD && r.owner != "java/lang/reflect/AccessibleObject" {
            return Ok(None);
        }
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("canAccess", "(Ljava/lang/Object;)Z") => {
                let Some(marker) = receiver.and_then(expect_ref).cloned() else {
                    return Ok(None);
                };
                let object = args.first().cloned().unwrap_or(Value::Ref(None));
                self.field_can_access(&marker, &object, caller)
            }
            _ => self.invoke_field_get_set(r, receiver, args, caller),
        }
    }

    fn field_can_access(
        &self,
        marker: &str,
        object: &Value,
        caller: &str,
    ) -> Result<Option<Value>, NativeError> {
        let Some((declaring, member)) = reflect_override::canonical_field(marker)
            .strip_prefix("reflect-field/")
            .and_then(|rest| rest.split_once('#'))
        else {
            return Ok(None);
        };
        let (name, Some((descriptor, access))) = parse_field_marker(member) else {
            return Ok(None);
        };
        // A marker without its flags (a `getDeclaredField` miss) carries
        // none to check.
        if !member
            .split(':')
            .nth(2)
            .is_some_and(|flags| !flags.is_empty())
        {
            return Ok(None);
        }
        let Some(overridden) = self.field_override(marker) else {
            return Ok(None);
        };
        let Some(class_flags) = self.class(declaring).and_then(checkcast::class_access) else {
            return Ok(None);
        };
        let is_static = access & ACC_STATIC != 0;
        // 1. The receiver check.
        let target = if is_static {
            if !matches!(object, Value::Ref(None)) {
                return Err(illegal_argument(format!(
                    "non-null object for {}",
                    field_text(declaring, name, descriptor, access)
                )));
            }
            None
        } else {
            if matches!(object, Value::Ref(None)) {
                return Err(illegal_argument(format!(
                    "null object for {}",
                    field_text(declaring, name, descriptor, access)
                )));
            }
            match self.known_instance_of(object, declaring) {
                Some(true) => {}
                Some(false) => {
                    return Err(illegal_argument(format!(
                        "object is not an instance of {}",
                        declaring.replace('/', ".")
                    )));
                }
                None => return Ok(None),
            }
            // 2. `override`, before the object's class is ever read.
            if overridden {
                return Ok(Some(Value::Int(1)));
            }
            match self.known_object_class(object) {
                Some(class) => Some(class),
                None => return Ok(None),
            }
        };
        // 2. `override` (a static field).
        if overridden {
            return Ok(Some(Value::Int(1)));
        }
        // 3. `verifyAccess`, which starts with the same-class shortcut.
        let allowed = if caller == declaring {
            true
        } else {
            // The declaring jar's module is open to the caller
            // (`reflect_override`), and the caller's class is readable.
            if !self.module_open_to(declaring, caller) || self.class(caller).is_none() {
                return Ok(None);
            }
            let same_package = package_of(caller) == package_of(declaring);
            match self.member_access(
                caller,
                declaring,
                target.as_deref(),
                access,
                class_flags,
                same_package,
            )? {
                Some(allowed) => allowed,
                None => return Ok(None),
            }
        };
        Ok(Some(Value::Int(i32::from(allowed))))
    }

    /// `Reflection.verifyMemberAccess` past its same-class and module checks.
    pub(super) fn member_access(
        &self,
        caller: &str,
        declaring: &str,
        target: Option<&str>,
        access: u16,
        class_flags: u16,
        same_package: bool,
    ) -> Result<Option<bool>, NativeError> {
        if class_flags & ACC_PUBLIC == 0 && !same_package {
            return Ok(Some(false));
        }
        if access & ACC_PUBLIC != 0 {
            return Ok(Some(true));
        }
        let private = access & ACC_PRIVATE != 0;
        if private {
            match self.nestmates(caller, declaring)? {
                Some(true) => return Ok(Some(true)),
                Some(false) => {}
                None => return Ok(None),
            }
        }
        let protected = access & ACC_PROTECTED != 0;
        let mut success = protected && self.is_subclass_of(caller, declaring);
        if !success && !private && same_package {
            success = true;
        }
        if !success {
            return Ok(Some(false));
        }
        // JLS 6.6.2: a protected instance member through an object of a class
        // that is neither the caller nor its subclass.
        if let Some(target) = target
            && protected
            && target != caller
            && !same_package
            && !self.is_subclass_of(target, caller)
        {
            return Ok(Some(false));
        }
        Ok(Some(true))
    }

    /// `Reflection.isSubclassOf`: the superclass chain only, not interfaces.
    /// Leaving the classes the runtime holds ends the walk: a game or JDK
    /// class never extends a jar class.
    fn is_subclass_of(&self, class: &str, of: &str) -> bool {
        let mut current = class.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            if current == of {
                return true;
            }
            let Some(file) = self.class(&current) else {
                return false;
            };
            if file.super_name.is_empty() || file.super_name == current {
                return false;
            }
            current = file.super_name.clone();
        }
        false
    }

    /// `Class.isNestmateOf`: the two nest hosts are the same class. A class
    /// whose `NestHost` names a class that does not list it back (or sits in
    /// another package) is its own host, as the JVM's validation decides.
    /// `None` when a named host is not a class the runtime holds.
    fn nestmates(&self, left: &str, right: &str) -> Result<Option<bool>, NativeError> {
        let (Some(left), Some(right)) = (self.nest_host(left)?, self.nest_host(right)?) else {
            return Ok(None);
        };
        Ok(Some(left == right))
    }

    fn nest_host(&self, class: &str) -> Result<Option<String>, NativeError> {
        let Some(file) = self.class(class) else {
            return Ok(None);
        };
        let Some(host) = nest_host_attribute(file)? else {
            return Ok(Some(class.to_owned()));
        };
        let Some(host_file) = self.class(&host) else {
            return Ok(None);
        };
        let listed = class_attribute_classes(host_file, "NestMembers")?
            .iter()
            .any(|member| member == class);
        Ok(Some(if listed && package_of(&host) == package_of(class) {
            host
        } else {
            class.to_owned()
        }))
    }
}
