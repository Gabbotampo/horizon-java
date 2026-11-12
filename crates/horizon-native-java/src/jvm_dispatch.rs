//! Virtual dispatch in the JVM's order: the receiver's own body before any
//! host model.
//!
//! `invokevirtual`/`invokeinterface` select the method from the receiver's
//! runtime class (JVMS §5.4.6): the class itself, then its superclasses, then
//! the maximally specific superinterface default. The symbolic owner only
//! fixes the name and descriptor. The interpreter used to ask its host models
//! first and resolve the receiver's class only for the calls none of them
//! answered, so a model keyed by owner or member name answered a call the jar
//! had overridden. Two shapes this produced:
//!
//! * `Map.containsKey` on an object of a jar class that implements `Map`
//!   (a library's own map) went to the host's map model, which knows nothing
//!   of that object and answered from an empty row.
//! * `isEmpty()` on a jar subclass of `AbstractSet` that overrides `isEmpty`
//!   answered `size() == 0` from the `AbstractCollection` template instead of
//!   the override.
//!
//! [`Runtime::jvm_selected_jar_body`] runs before the model chain (after Mixin
//! anchors and invoke hooks, which replace the call site itself, and after the
//! null check). It answers only when the selected method is a jar or linked
//! class's body; a member the receiver inherits from a JDK or game class keeps
//! the existing chain, which models that class's contract.
//!
//! The superinterface step needs to know whether a JDK superclass declares the
//! member, since a superclass declaration beats any interface default. The
//! runtime holds no JDK class files, so [`JDK_SKELETONS`] lists the instance
//! methods the JDK's abstract collection skeletons declare, which are the JDK
//! superclasses library code extends most (fastutil's and Guava's collections
//! extend them). Any other JDK superclass is treated as unknown and keeps the
//! old answer.

use super::*;

/// `ACC_PRIVATE`, `ACC_PROTECTED`, `ACC_PUBLIC`.
const ACC_PRIVATE: u16 = 0x0002;
const ACC_PROTECTED: u16 = 0x0004;
const ACC_PUBLIC: u16 = 0x0001;

/// The non-static, non-private methods each JDK skeleton class declares
/// itself (not inherited), with its JDK superclass.
///
/// Read from JDK 25.0.5 by reflection (`getDeclaredMethods()`, static and
/// private ones dropped): the class's public API, not its code.
pub(crate) const JDK_SKELETONS: &[(&str, &str, &[&str])] = &[
    (
        "java/util/AbstractCollection",
        "java/lang/Object",
        &[
            "add(Ljava/lang/Object;)Z",
            "addAll(Ljava/util/Collection;)Z",
            "clear()V",
            "contains(Ljava/lang/Object;)Z",
            "containsAll(Ljava/util/Collection;)Z",
            "isEmpty()Z",
            "iterator()Ljava/util/Iterator;",
            "remove(Ljava/lang/Object;)Z",
            "removeAll(Ljava/util/Collection;)Z",
            "retainAll(Ljava/util/Collection;)Z",
            "size()I",
            "toArray()[Ljava/lang/Object;",
            "toArray([Ljava/lang/Object;)[Ljava/lang/Object;",
            "toString()Ljava/lang/String;",
        ],
    ),
    (
        "java/util/AbstractList",
        "java/util/AbstractCollection",
        &[
            "add(ILjava/lang/Object;)V",
            "add(Ljava/lang/Object;)Z",
            "addAll(ILjava/util/Collection;)Z",
            "clear()V",
            "equals(Ljava/lang/Object;)Z",
            "get(I)Ljava/lang/Object;",
            "hashCode()I",
            "indexOf(Ljava/lang/Object;)I",
            "iterator()Ljava/util/Iterator;",
            "lastIndexOf(Ljava/lang/Object;)I",
            "listIterator()Ljava/util/ListIterator;",
            "listIterator(I)Ljava/util/ListIterator;",
            "remove(I)Ljava/lang/Object;",
            "removeRange(II)V",
            "set(ILjava/lang/Object;)Ljava/lang/Object;",
            "subList(II)Ljava/util/List;",
        ],
    ),
    (
        "java/util/AbstractSequentialList",
        "java/util/AbstractList",
        &[
            "add(ILjava/lang/Object;)V",
            "addAll(ILjava/util/Collection;)Z",
            "get(I)Ljava/lang/Object;",
            "iterator()Ljava/util/Iterator;",
            "listIterator(I)Ljava/util/ListIterator;",
            "remove(I)Ljava/lang/Object;",
            "set(ILjava/lang/Object;)Ljava/lang/Object;",
        ],
    ),
    (
        "java/util/AbstractSet",
        "java/util/AbstractCollection",
        &[
            "equals(Ljava/lang/Object;)Z",
            "hashCode()I",
            "removeAll(Ljava/util/Collection;)Z",
        ],
    ),
    (
        "java/util/AbstractQueue",
        "java/util/AbstractCollection",
        &[
            "add(Ljava/lang/Object;)Z",
            "addAll(Ljava/util/Collection;)Z",
            "clear()V",
            "element()Ljava/lang/Object;",
            "remove()Ljava/lang/Object;",
        ],
    ),
    (
        "java/util/AbstractMap",
        "java/lang/Object",
        &[
            "clear()V",
            "clone()Ljava/lang/Object;",
            "containsKey(Ljava/lang/Object;)Z",
            "containsValue(Ljava/lang/Object;)Z",
            "entrySet()Ljava/util/Set;",
            "equals(Ljava/lang/Object;)Z",
            "get(Ljava/lang/Object;)Ljava/lang/Object;",
            "hashCode()I",
            "isEmpty()Z",
            "keySet()Ljava/util/Set;",
            "put(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            "putAll(Ljava/util/Map;)V",
            "remove(Ljava/lang/Object;)Ljava/lang/Object;",
            "size()I",
            "toString()Ljava/lang/String;",
            "values()Ljava/util/Collection;",
        ],
    ),
    (
        "java/util/AbstractMap$SimpleEntry",
        "java/lang/Object",
        &[
            "equals(Ljava/lang/Object;)Z",
            "getKey()Ljava/lang/Object;",
            "getValue()Ljava/lang/Object;",
            "hashCode()I",
            "setValue(Ljava/lang/Object;)Ljava/lang/Object;",
            "toString()Ljava/lang/String;",
        ],
    ),
    (
        "java/util/AbstractMap$SimpleImmutableEntry",
        "java/lang/Object",
        &[
            "equals(Ljava/lang/Object;)Z",
            "getKey()Ljava/lang/Object;",
            "getValue()Ljava/lang/Object;",
            "hashCode()I",
            "setValue(Ljava/lang/Object;)Ljava/lang/Object;",
            "toString()Ljava/lang/String;",
        ],
    ),
];

/// Whether the JDK class `class`, or a JDK superclass of it, declares the
/// instance method `name descriptor` itself, so that it is selected before
/// any superinterface default. `None` when the runtime does not know the
/// class's declarations.
///
/// `Object` and `Record` answer `false`: an interface may not declare a
/// default for one of `Object`'s public methods, so for those no default is
/// ever found, and for every other member the defaults are the next step.
/// `Enum`'s own methods are final (`name`, `ordinal`...).
pub(crate) fn jdk_superclass_declares(class: &str, name: &str, descriptor: &str) -> Option<bool> {
    let mut current = class;
    for _ in 0..JDK_SKELETONS.len() + 1 {
        match current {
            "java/lang/Object" | "java/lang/Record" => return Some(false),
            "java/lang/Enum" => return Some(ENUM_DECLARED_METHODS.contains(&name)),
            _ => {}
        }
        let (_, superclass, declared) = JDK_SKELETONS
            .iter()
            .find(|(skeleton, _, _)| *skeleton == current)?;
        if declared.iter().any(|member| {
            member
                .strip_prefix(name)
                .is_some_and(|rest| rest == descriptor)
        }) {
            return Some(true);
        }
        current = superclass;
    }
    None
}

/// Libraries the runtime answers with hand models (`guava_model.rs`,
/// `library_dispatch.rs`'s fastutil shapes, `attachment_codec.rs`), even when
/// a jar ships or links its own copy (the `MapMaker` fixture). Their objects
/// keep the models' answers: running a copy's bytecode for some calls while
/// the models answer the rest would mix two implementations in one object.
/// Interpreting a linked library wholly is a separate decision.
fn library_modelled(class: &str) -> bool {
    [
        "com/google/common/",
        "it/unimi/dsi/fastutil/",
        "com/mojang/serialization/",
        "com/mojang/datafixers/",
    ]
    .iter()
    .any(|root| class.starts_with(root))
}

/// The package part of an internal class name (`a/b/C` → `a/b`).
fn package_of(class: &str) -> &str {
    class.rsplit_once('/').map_or("", |(package, _)| package)
}

/// The class of a `class#n` object marker (an object `new` built), in any
/// package, or `None` for every other marker family.
fn constructed_object_class(marker: &str) -> Option<&str> {
    let (class, serial) = marker.split_once('#')?;
    (!class.is_empty()
        && !class.contains([':', ' ', '.'])
        && !serial.is_empty()
        && serial.bytes().all(|byte| byte.is_ascii_digit()))
    .then_some(class)
}

impl<H: Host> Runtime<'_, H> {
    /// The jar or linked class whose body an `invokevirtual`/`invokeinterface`
    /// of `r` on `receiver` selects, or `None` to leave the call to the model
    /// chain (and then to [`Self::dispatch_owner`]).
    ///
    /// The receiver must be an object this run constructed of a class the
    /// runtime holds. Selection follows JVMS §5.4.6: the first class on the
    /// receiver's superclass chain that declares a non-static, non-private
    /// method of that name and descriptor which can override the resolved one
    /// (a package-private resolved method is only overridden from its own
    /// package), else, when the chain leaves the jar at a JDK superclass known
    /// not to declare the member, a jar interface's default. A selected
    /// abstract method answers `None` (the JVM throws `AbstractMethodError`;
    /// the existing chain keeps its answer). A resolved method that is private
    /// or static is not dispatched virtually at all.
    pub(crate) fn jvm_selected_jar_body(
        &self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
    ) -> Option<String> {
        if !matches!(opcode, 0xb6 | 0xb9) || r.name.starts_with('<') {
            return None;
        }
        let class = constructed_object_class(expect_ref(receiver?)?)?;
        // A jar's own copy of a host-modelled class (an unrelocated Gson) is
        // answered by the host's model, as `new` and every other path treat it.
        if self.host.models_class(class) || library_modelled(class) {
            return None;
        }
        self.class(class)?;
        // The resolved method's access decides whether the call is virtual
        // and which overrides count.
        let resolved = self
            .member_owner(&r.owner, &r.name, &r.descriptor)
            .and_then(|(owner, jar)| jar.then_some(owner))
            .and_then(|owner| {
                let access = self.class(&owner)?.method(&r.name, &r.descriptor)?.access;
                Some((owner, access))
            });
        if let Some((_, access)) = &resolved
            && access & (ACC_PRIVATE | 0x0008) != 0
        {
            return None;
        }
        let package_private_in = resolved.as_ref().and_then(|(owner, access)| {
            (access & (ACC_PUBLIC | ACC_PROTECTED) == 0).then(|| package_of(owner).to_owned())
        });
        let mut current = class.to_owned();
        let mut chain = Vec::new();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(file) = self.class(&current) else {
                // The chain left the jar. A JDK superclass that declares the
                // member (or one the runtime cannot tell about) answers it
                // through the model chain; otherwise a jar default wins.
                if jdk_superclass_declares(&current, &r.name, &r.descriptor) != Some(false) {
                    return None;
                }
                return self.inherited_default_owner(&chain, &r.name, &r.descriptor);
            };
            if let Some(method) = file.method(&r.name, &r.descriptor)
                && !method.is_static()
                && method.access & ACC_PRIVATE == 0
                && package_private_in
                    .as_deref()
                    .is_none_or(|package| package == package_of(&current))
            {
                return method.code.is_some().then_some(current);
            }
            if file.super_name.is_empty() {
                return None;
            }
            chain.push(current);
            current = file.super_name.clone();
        }
        None
    }
}

impl<H: Host> Runtime<'_, H> {
    /// Whether `getfield r` on `receiver` reads a field the JVM gave its
    /// default value: an instance field a jar or linked class declares, on an
    /// object this run constructed of that class or a subclass, that nothing
    /// has written yet (JVMS §2.3, §2.4: every field starts at `0`, `false` or
    /// `null`).
    ///
    /// Such a read used to go to the host as if the field belonged to a
    /// platform class, which counted an unknown call for an ordinary
    /// `count++` on a fresh object. `r.owner` is the declaring class
    /// (`Runtime::field_owner` resolved it). A platform superclass's field
    /// is still the host's to answer.
    pub(crate) fn reads_unwritten_jar_field(&self, r: &MethodRef, receiver: &Value) -> bool {
        let Some(class) = expect_ref(receiver).and_then(|marker| constructed_object_class(marker))
        else {
            return false;
        };
        self.class(&r.owner).is_some_and(|file| {
            file.fields.iter().any(|field| {
                field.name == r.name && field.descriptor == r.descriptor && !field.is_static()
            })
        }) && self.class_is_instance(class, &r.owner)
    }
}
