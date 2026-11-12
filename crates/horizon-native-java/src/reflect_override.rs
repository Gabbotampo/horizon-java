//! `AccessibleObject`'s `override` flag: `setAccessible`, `trySetAccessible`,
//! `isAccessible`, with each `Field` object keeping its own flag, and
//! `Field.equals`/`hashCode` across those objects.
//!
//! JDK 27 hands out a fresh `Field` on every `getDeclaredField`/`getField`
//! call, and one per element of every `getDeclaredFields`/`getFields` array
//! (`ReflectionFactory.copyField`). Each copy has its own `override`, so
//! `setAccessible(true)` on one leaves every other copy of the same field
//! checked. The copies are `equals` (same declaring class, name and type) and
//! share a `hashCode`, but are not `==`.
//!
//! Ported from the JDK 27 bytecode (`javap -c -p --module java.base`) of
//! `AccessibleObject.setAccessible(AccessibleObject[], boolean)`,
//! `setAccessible0`, `trySetAccessible`, `checkCanSetAccessible(Class, Class,
//! boolean)`, `isAccessible`, `canAccess`, `Field`/`Method`/`Constructor`
//! `.setAccessible(boolean)` and `checkCanSetAccessible(Class)`, and
//! `Field.equals`/`hashCode`:
//!
//! 1. `setAccessible(false)` never checks anything: it clears `override`.
//! 2. `setAccessible(true)` runs `checkCanSetAccessible(caller, declaring)`
//!    and then sets `override`. The check passes at once when the caller's
//!    module is the declaring class's module, when the caller is in
//!    `java.base`, or when the declaring module is unnamed. Otherwise it reads
//!    the declaring module's exports and opens, and throws
//!    `InaccessibleObjectException("Unable to make [field ]<member> accessible:
//!    <module> does not \"opens|exports <package>\" to <caller module>")`.
//! 3. `trySetAccessible()` answers `true` when `override` is already set,
//!    otherwise runs the same check without throwing and sets `override` on
//!    success.
//! 4. The static `setAccessible(array, true)` checks every element before it
//!    sets any.
//! 5. `isAccessible()` answers `override`; `canAccess(obj)` answers `true`
//!    once the receiver checks pass and `override` is set (`field_access`).
//!
//! **The module question.** The runtime does not know which loader runs it,
//! so the check is decided only for a member of the running jar's own classes
//! and only where both loaders pass it:
//!
//! * Fabric's Knot puts every mod class, the jar's own and the libraries it
//!   bundles, in its loader's unnamed module: the declaring module is unnamed
//!   and the check passes for any caller.
//! * FML 12 (`JarModuleInfo.from`) gives a jar without a root
//!   `module-info.class` an automatic module (`newAutomaticModule` plus its
//!   scanned packages), and `Module.isOpen` answers `true` for every package
//!   of an automatic module: the check passes for any caller. A jar with a
//!   descriptor is an explicit module, open only where it says so; then only
//!   a caller in the same jar (the same module) is decided.
//!
//! So: a running-jar member with a caller from the same jar always passes; a
//! caller from elsewhere (a config library the jar bundles) passes when the
//! jar has no `module-info.class`. Any other declaring class is counted: a
//! game class is in Knot's unnamed module on Fabric but in FML's `minecraft`
//! module on NeoForge, a linked jar's descriptor is not read, and a JDK
//! class's answer depends on the launch's `--add-opens` (OpenJDK 27 denies
//! `String.value` with `module java.base does not "opens java.lang" to
//! unnamed module @69b2283a`, a text that carries the module's identity
//! hash). The runtime never decides a denial, so it never builds an
//! `InaccessibleObjectException`.
//!
//! **Telling copies apart.** A `Field` the reflection lookups hand out is the
//! field's marker (`reflect-field/<class>#<name>:<descriptor>:<flags>`) with
//! a copy number appended (`#<n>`, from the object counter). Copies of one
//! field are therefore distinct values, and `if_acmpeq` tells them apart as
//! the JVM does. Collections compare and store their elements as text
//! (`value_marker`), so a copy enters a collection as the field's canonical
//! marker: lookups compare by `Field.equals`, as in Java, and a field read
//! back out of a collection is no particular copy. Its `override` is known
//! only when every copy of the field agrees (none set, or all set); otherwise
//! a call whose answer depends on it is counted. Each field also remembers
//! how many copies were handed out and how many have `override` set.

use super::*;

pub(super) const FIELD_PREFIX: &str = "reflect-field/";
const FIELD: &str = "java/lang/reflect/Field";
const METHOD: &str = "java/lang/reflect/Method";
const CONSTRUCTOR: &str = "java/lang/reflect/Constructor";
const EXECUTABLE: &str = "java/lang/reflect/Executable";
const ACCESSIBLE: &str = "java/lang/reflect/AccessibleObject";

/// `<copy>|OWN`: one `Field` copy's own `override` (`Int(0|1)`), or, for a
/// `Method`/`Constructor` marker, that some `setAccessible` reached it.
const OWN: &str = "reflect#override";
/// `<canonical>|COPIES`: how many copies of the field were handed out.
const COPIES: &str = "reflect#copies";
/// `<canonical>|OVERRIDDEN`: how many of those copies have `override` set.
const OVERRIDDEN: &str = "reflect#overridden";
/// `<canonical>|UNKNOWN`: an `override` change could not be attributed to a
/// copy (a counted `setAccessible`, or one on the canonical marker), so no
/// copy's flag is known any more.
const UNKNOWN: &str = "reflect#override-unknown";

/// Whether a jar entry is a module descriptor: the root one FML reads, or a
/// multi-release copy (kept conservatively, as if it counted).
pub(super) fn is_module_descriptor(name: &str) -> bool {
    name == "module-info.class"
        || name
            .strip_prefix("META-INF/versions/")
            .is_some_and(|rest| rest.ends_with("/module-info.class"))
}

/// Whether a host's resource table, seeded from the running jar, holds a
/// module descriptor; `None` when the table holds no loader metadata (the
/// host was not seeded from a jar, so absence says nothing).
pub(super) fn module_descriptor_in(resources: &BTreeMap<String, String>) -> Option<bool> {
    let seeded = [
        "fabric.mod.json",
        "META-INF/neoforge.mods.toml",
        "META-INF/mods.toml",
    ]
    .iter()
    .any(|name| resources.contains_key(*name));
    seeded.then(|| resources.keys().any(|name| is_module_descriptor(name)))
}

/// The copy number a reflection lookup appended to a field marker's member
/// part (`name:descriptor:flags#n`), stripped. A member without flags (a
/// lookup miss) never carries one.
pub(super) fn strip_field_copy(member: &str) -> &str {
    match member.rsplit_once('#') {
        Some((field, number))
            if !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
                && field.matches(':').count() == 2 =>
        {
            field
        }
        _ => member,
    }
}

/// A field marker without its copy number: the text every copy of the field
/// shares, and what `Field.equals` compares. Any other text is returned as is.
pub(super) fn canonical_field(marker: &str) -> &str {
    let Some(rest) = marker.strip_prefix(FIELD_PREFIX) else {
        return marker;
    };
    let Some((_, member)) = rest.split_once('#') else {
        return marker;
    };
    let stripped = strip_field_copy(member);
    &marker[..marker.len() - (member.len() - stripped.len())]
}

/// The class a `Field`, `Method` or `Constructor` marker is declared by, when
/// the marker names a member the declaring class has (a lookup miss carries
/// no descriptor or flags).
fn declaring_class(marker: &str) -> Option<&str> {
    let canonical = canonical_field(marker);
    if let Some(rest) = canonical.strip_prefix(FIELD_PREFIX) {
        let (class, member) = rest.split_once('#')?;
        return (member.matches(':').count() == 2).then_some(class);
    }
    if let Some(rest) = marker.strip_prefix("reflect-method/") {
        let (class, member) = rest.split_once('#')?;
        return member.contains('(').then_some(class);
    }
    let (class, descriptor) = marker
        .strip_prefix("reflect-constructor/")?
        .split_once('#')?;
    descriptor.starts_with('(').then_some(class)
}

impl<H: Host> Runtime<'_, H> {
    /// A new `Field` object for the field `canonical` names: the marker with a
    /// fresh copy number. Every lookup that returns a `Field` makes one.
    pub(super) fn field_copy(&mut self, canonical: String) -> Value {
        if !canonical.starts_with(FIELD_PREFIX) || declaring_class(&canonical).is_none() {
            // A lookup miss: nothing to tell apart.
            return Value::Ref(Some(canonical));
        }
        let copies = format!("{canonical}|{COPIES}");
        let count = self.counter(&copies);
        self.fields
            .insert(copies, Value::Int(count.saturating_add(1)));
        let marker = format!("{canonical}#{}", self.next_object);
        self.next_object = self.next_object.saturating_add(1);
        Value::Ref(Some(marker))
    }

    fn counter(&self, key: &str) -> i32 {
        match self.fields.get(key) {
            Some(Value::Int(count)) => *count,
            _ => 0,
        }
    }

    /// The `override` flag of the `Field` object `marker` is. `None` when it
    /// cannot be known: a canonical marker (read back from a collection)
    /// whose copies disagree, or a field whose flags lost track.
    pub(super) fn field_override(&self, marker: &str) -> Option<bool> {
        let canonical = canonical_field(marker);
        if self.fields.contains_key(&format!("{canonical}|{UNKNOWN}")) {
            return None;
        }
        if canonical != marker {
            return Some(self.counter(&format!("{marker}|{OWN}")) != 0);
        }
        let overridden = self.counter(&format!("{canonical}|{OVERRIDDEN}"));
        if overridden == 0 {
            return Some(false);
        }
        (overridden == self.counter(&format!("{canonical}|{COPIES}"))).then_some(true)
    }

    /// Set the `override` flag of `marker`. `false` when the object is not
    /// one copy and the copies do not already all hold `flag`: the change is
    /// then not attributable, and every copy's flag becomes unknown.
    fn set_override(&mut self, marker: &str, flag: bool) -> bool {
        let canonical = canonical_field(marker);
        if !canonical.starts_with(FIELD_PREFIX) {
            // `Method`/`Constructor` markers do not tell copies apart; the
            // mark makes `isAccessible` on them counted from now on.
            self.fields.insert(format!("{marker}|{OWN}"), Value::Int(1));
            return true;
        }
        if canonical == marker {
            if self.field_override(marker) == Some(flag) {
                return true;
            }
            self.forget_override(marker);
            return false;
        }
        let own = format!("{marker}|{OWN}");
        let was = self.counter(&own) != 0;
        self.fields.insert(own, Value::Int(i32::from(flag)));
        if was != flag {
            let overridden = format!("{canonical}|{OVERRIDDEN}");
            let count = self.counter(&overridden);
            let count = if flag {
                count.saturating_add(1)
            } else {
                count.saturating_sub(1).max(0)
            };
            self.fields.insert(overridden, Value::Int(count));
        }
        true
    }

    /// An `override` change this model could not decide (the call is
    /// counted): no copy of the field has a known flag any more.
    fn forget_override(&mut self, marker: &str) {
        let canonical = canonical_field(marker);
        if canonical.starts_with(FIELD_PREFIX) {
            self.fields
                .insert(format!("{canonical}|{UNKNOWN}"), Value::Int(1));
        } else {
            self.fields.insert(format!("{marker}|{OWN}"), Value::Int(1));
        }
    }

    /// Whether `declaring`'s module is open to `caller` under both loaders
    /// (module docs): a running-jar class, and a caller from the same jar or
    /// a jar without a module descriptor. Open implies exported, so the
    /// member-access check's module step passes too.
    pub(super) fn module_open_to(&self, declaring: &str, caller: &str) -> bool {
        self.classes.contains_key(declaring)
            && (self.classes.contains_key(caller)
                || self.host.jar_module_descriptor() == Some(false))
    }

    /// `checkCanSetAccessible(caller, declaring, …)`, decided only where it
    /// passes for a reason the runtime can see (module docs): `false` means
    /// "not decided", never "denied".
    fn can_set_accessible(&self, marker: &str, caller: &str) -> bool {
        declaring_class(marker).is_some_and(|declaring| self.module_open_to(declaring, caller))
    }

    /// Answer `setAccessible`, `trySetAccessible`, `isAccessible` and
    /// `Field.equals`/`hashCode`. `None` leaves the call to the rest of the
    /// dispatch (the host counts it).
    pub(super) fn invoke_reflect_override(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
        caller: &str,
    ) -> Result<Option<Value>, NativeError> {
        if !matches!(
            r.owner.as_str(),
            FIELD | METHOD | CONSTRUCTOR | EXECUTABLE | ACCESSIBLE
        ) {
            return Ok(None);
        }
        if (r.name.as_str(), r.descriptor.as_str())
            == ("setAccessible", "([Ljava/lang/reflect/AccessibleObject;Z)V")
        {
            return Ok(self.set_accessible_all(args, caller));
        }
        let Some(marker) = receiver.and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        if declaring_class(&marker).is_none() {
            return Ok(None);
        }
        Ok(match (r.name.as_str(), r.descriptor.as_str()) {
            ("setAccessible", "(Z)V") => {
                let flag = matches!(args.first(), Some(Value::Int(flag)) if *flag != 0);
                if flag && !self.can_set_accessible(&marker, caller) {
                    // Counted: the real call may have set `override`.
                    self.forget_override(&marker);
                    return Ok(None);
                }
                self.set_override(&marker, flag).then_some(Value::Void)
            }
            ("trySetAccessible", "()Z") => {
                if self.field_override(&marker) == Some(true) && marker.starts_with(FIELD_PREFIX) {
                    return Ok(Some(Value::Int(1)));
                }
                if !self.can_set_accessible(&marker, caller) {
                    self.forget_override(&marker);
                    return Ok(None);
                }
                self.set_override(&marker, true).then_some(Value::Int(1))
            }
            ("isAccessible", "()Z") => {
                if marker.starts_with(FIELD_PREFIX) {
                    self.field_override(&marker)
                        .map(|flag| Value::Int(i32::from(flag)))
                } else {
                    // A `Method`/`Constructor` nothing made accessible.
                    (!self.fields.contains_key(&format!("{marker}|{OWN}"))).then_some(Value::Int(0))
                }
            }
            ("equals", "(Ljava/lang/Object;)Z") if r.owner == FIELD => {
                let other = args.first().and_then(expect_ref);
                let same = other.is_some_and(|other| {
                    other.starts_with(FIELD_PREFIX)
                        && canonical_field(other) == canonical_field(&marker)
                });
                Some(Value::Int(i32::from(same)))
            }
            ("hashCode", "()I") if r.owner == FIELD => {
                // `getDeclaringClass().getName().hashCode() ^ getName().hashCode()`.
                let rest = &canonical_field(&marker)[FIELD_PREFIX.len()..];
                let (class, member) = rest.split_once('#').unwrap_or((rest, ""));
                let name = member.split(':').next().unwrap_or(member);
                Some(Value::Int(
                    jdk_model::java_string_hash(&class.replace('/', "."))
                        ^ jdk_model::java_string_hash(name),
                ))
            }
            _ => None,
        })
    }

    /// The static `AccessibleObject.setAccessible(array, flag)`: with `true`,
    /// every element is checked before any is set.
    fn set_accessible_all(&mut self, args: &[Value], caller: &str) -> Option<Value> {
        let flag = matches!(args.get(1), Some(Value::Int(flag)) if *flag != 0);
        let items = args.first().and_then(|array| self.array(array).ok())?;
        let count = items.len();
        // A `null` element is a `NullPointerException` in the check loop.
        let markers: Vec<String> = items.iter().filter_map(expect_ref).cloned().collect();
        let decided = markers.len() == count
            && markers.iter().all(|marker| {
                declaring_class(marker).is_some()
                    && (!flag || self.can_set_accessible(marker, caller))
            });
        if !decided {
            for marker in &markers {
                self.forget_override(marker);
            }
            return None;
        }
        let mut all = true;
        for marker in &markers {
            all &= self.set_override(marker, flag);
        }
        all.then_some(Value::Void)
    }
}
