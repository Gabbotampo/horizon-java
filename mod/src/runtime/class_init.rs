//! JVM class initialization (JVMS §5.5) for the classes a jar (or a library
//! linked beside it) declares.
//!
//! The rules the interpreter keeps, each checked against a real JVM:
//!
//! * A class is initialized once, before its first active use. A same-thread
//!   re-entry while its `<clinit>` runs (a static initializer that calls back
//!   into its own class through another one) sees the partially initialized
//!   class, as the JVM does: fields not assigned yet read their defaults.
//! * Before a class's own `<clinit>`, its superclass is initialized, then each
//!   superinterface that declares a default (non-abstract, non-static) method.
//!   An interface's superinterfaces are not initialized with it.
//! * A `<clinit>` that throws marks the class erroneous. The first access fails
//!   with `ExceptionInInitializerError` wrapping the throw, or the throw itself
//!   when it already is an `Error`. Every later access fails with
//!   `NoClassDefFoundError: Could not initialize class <binary name>`, never
//!   with a half-initialized class. A superclass that fails makes the subclass
//!   erroneous too, with the superclass's failure propagated unchanged.
//!
//! The interpreter can also refuse a `<clinit>` (an unmodelled call under a
//! strict host, the execution budget). That is not a JVM event, so it is not
//! turned into a Java exception a jar could catch: the class is marked
//! erroneous with the refusal, and every later access is refused again with the
//! same cause instead of running on the static fields the refused initializer
//! left half-written. Earlier the class stayed marked initialized, which is
//! what jamlib's `Event#listeners` hit in a strict game-hook firing.
//!
//! * JVMS §5.5 step 6: before the superclass and `<clinit>`, every static
//!   field with a `ConstantValue` attribute (JVMS §4.7.2) takes its constant.
//!   javac inlines such constants at their use sites, but a `getstatic` from
//!   another compiler (or from a class compiled against an older, non-constant
//!   version of the field) reads the field itself, and must see the constant
//!   rather than the type's default. OpenJDK 27 (HotSpot) applies the
//!   attribute to every static field, final or not, and stores an `int`
//!   constant into a narrower field as `i2b`/`i2s`/`i2c` would, and into a
//!   `boolean` as its lowest bit (a hand-made class with 300, 70000, -1 and 2
//!   in `byte`, `short`, `char` and `boolean` fields reads 44, 4464, 65535 and
//!   0). A constant whose pool entry does not fit the field's type is a
//!   format error the JVM refuses at load; it is left unassigned here.
//!
//! The state lives in the runtime's initialized-class set, which the host
//! persists between entrypoints and callbacks. An erroneous class is recorded
//! there under a key no internal class name can collide with (it starts with a
//! NUL), so the existing persistence carries it unchanged.

use super::*;

/// Prefix of an erroneous-class record in the initialized-class set:
/// `\0erroneous\0<internal name>\0<cause>`.
const ERRONEOUS_PREFIX: &str = "\u{0}erroneous\u{0}";
/// The cause tag of a class whose `<clinit>` threw a Java exception.
const JAVA_FAILURE: &str = "java";
/// The cause tag prefix of a class whose `<clinit>` the interpreter refused.
const REFUSED_FAILURE: &str = "refused:";
/// The longest refusal text an erroneous record keeps, in chars.
const MAX_REFUSAL_CHARS: usize = 512;
/// Superinterfaces one class initialization walks at most.
const MAX_INITIALIZED_SUPERINTERFACES: usize = 256;

/// `(name, descriptor, value)` of every static field of a class file that has
/// a `ConstantValue` attribute whose constant fits the field's type.
fn static_constant_values(raw: &[u8]) -> Result<Vec<(String, String, Value)>, NativeError> {
    let mut reader = Reader::new(raw);
    reader.skip(8)?;
    let pool = read_pool(&mut reader)?;
    reader.skip(6)?; // access, this, super
    let interfaces = reader.u2()? as usize;
    reader.skip(interfaces * 2)?;
    let mut constants = Vec::new();
    for _ in 0..reader.u2()? {
        let access = reader.u2()?;
        let name = utf8(&pool, reader.u2()?)?;
        let descriptor = utf8(&pool, reader.u2()?)?;
        for _ in 0..reader.u2()? {
            let attribute = utf8(&pool, reader.u2()?)?;
            let length = reader.u4()? as usize;
            if access & 0x0008 == 0 || attribute != "ConstantValue" || length != 2 {
                reader.skip(length)?;
                continue;
            }
            let index = reader.u2()?;
            if let Some(value) = constant_for_field(&pool, index, &descriptor) {
                constants.push((name.clone(), descriptor.clone(), value));
            }
        }
    }
    Ok(constants)
}

/// The value a `ConstantValue` pool entry gives a field of `descriptor`
/// (JVMS §4.7.2's table), narrowed as HotSpot stores it; `None` when the
/// entry's kind does not match the type.
fn constant_for_field(pool: &[Cp], index: u16, descriptor: &str) -> Option<Value> {
    let int = match pool.get(index as usize) {
        Some(Cp::Integer(value)) => Some(*value),
        _ => None,
    };
    Some(match descriptor {
        "I" => Value::Int(int?),
        "S" => Value::Int(i32::from(int? as i16)),
        "C" => Value::Int(i32::from(int? as u16)),
        "B" => Value::Int(i32::from(int? as i8)),
        "Z" => Value::Int(int? & 1),
        "J" => match pool.get(index as usize)? {
            Cp::Long(value) => Value::Long(*value),
            _ => return None,
        },
        "F" => match pool.get(index as usize)? {
            Cp::Float(value) => Value::Float(f64::from(*value)),
            _ => return None,
        },
        "D" => match pool.get(index as usize)? {
            Cp::Double(value) => Value::Float(*value),
            _ => return None,
        },
        "Ljava/lang/String;" => match pool.get(index as usize)? {
            Cp::String(_) => constant_value(pool, index).ok()?,
            _ => return None,
        },
        _ => return None,
    })
}

/// The key prefix every record for `class` starts with.
fn erroneous_class_prefix(class: &str) -> String {
    format!("{ERRONEOUS_PREFIX}{class}\u{0}")
}

/// The binary name the JVM's messages use: `a.b.Outer$Inner`.
fn binary_name(class: &str) -> String {
    class.replace('/', ".")
}

/// Whether a class-file access word names an interface.
fn is_interface(file: &ClassFile) -> bool {
    jdk_model::class_access_flags(file).is_some_and(|flags| flags & 0x0200 != 0)
}

/// Whether an interface declares a default method: one with a body that is
/// neither static nor its initializer.
fn declares_default_method(file: &ClassFile) -> bool {
    file.methods
        .iter()
        .any(|method| method.code.is_some() && !method.is_static() && method.name != "<clinit>")
}

impl<H: Host> Runtime<'_, H> {
    /// Initialize a jar-owned class before its first active use, following
    /// the rules in this module's documentation. A class neither the jar nor a
    /// linked library declares is the host's, and has nothing to run here.
    pub(super) fn ensure_initialized(&mut self, class: &str) -> Result<(), NativeError> {
        if !self.has_class(class) {
            return Ok(());
        }
        if let Some(cause) = self.erroneous_cause(class) {
            return Err(erroneous_access(class, &cause));
        }
        // Done, or in progress on this (the only) thread: either way the JVM
        // returns at once.
        if !self.initialized_classes.insert(class.to_owned()) {
            return Ok(());
        }
        // Step 6: the `ConstantValue` statics, before anything else runs.
        self.assign_constant_values(class)?;
        let supertypes = self.initialize_supertypes(class);
        let outcome = match supertypes {
            // A supertype's failure is this class's failure, propagated as is.
            Err(error) => Err(error),
            Ok(()) => self
                .run_class_initializer(class)
                .map_err(|error| match error {
                    NativeError::UncaughtException {
                        class: thrown,
                        message,
                        frame,
                    } => wrap_initializer_throw(self.classes, &self.linked, thrown, message, frame),
                    other => other,
                }),
        };
        if let Err(error) = &outcome {
            self.initialized_classes.remove(class);
            let cause = match error {
                NativeError::UncaughtException { .. } => JAVA_FAILURE.to_owned(),
                refusal => {
                    let text: String = refusal
                        .to_string()
                        .chars()
                        .take(MAX_REFUSAL_CHARS)
                        .collect();
                    format!("{REFUSED_FAILURE}{text}")
                }
            };
            self.initialized_classes
                .insert(format!("{}{cause}", erroneous_class_prefix(class)));
        }
        outcome
    }

    /// JVMS §5.5 step 6 (module docs): each static field's `ConstantValue`,
    /// read from the class's bytes. A value already there (a host that seeded
    /// the field) is kept.
    fn assign_constant_values(&mut self, class: &str) -> Result<(), NativeError> {
        let Some(file) = self.class(class) else {
            return Ok(());
        };
        if file.raw.is_empty() {
            return Ok(());
        }
        for (name, descriptor, value) in static_constant_values(&file.raw)? {
            self.static_fields
                .entry(format!("{class}#{name}{descriptor}"))
                .or_insert(value);
        }
        Ok(())
    }

    /// The recorded cause of an erroneous class, or `None`.
    fn erroneous_cause(&self, class: &str) -> Option<String> {
        let prefix = erroneous_class_prefix(class);
        self.initialized_classes
            .range(prefix.clone()..)
            .next()
            .and_then(|key| key.strip_prefix(&prefix))
            .map(str::to_owned)
    }

    /// JVMS §5.5 step 7: the superclass, then the superinterfaces that declare
    /// a default method, in the order of the recursive enumeration of the
    /// class's `interfaces` table. Nothing for an interface.
    fn initialize_supertypes(&mut self, class: &str) -> Result<(), NativeError> {
        let Some(file) = self.class(class) else {
            return Ok(());
        };
        if is_interface(file) {
            return Ok(());
        }
        let super_name = file.super_name.clone();
        let mut pending: Vec<String> = file.interface_names().into_iter().rev().collect();
        if !super_name.is_empty() && super_name != class {
            self.ensure_initialized(&super_name)?;
        }
        let mut seen = std::collections::BTreeSet::new();
        while let Some(interface) = pending.pop() {
            if seen.len() >= MAX_INITIALIZED_SUPERINTERFACES {
                break;
            }
            if !seen.insert(interface.clone()) {
                continue;
            }
            let Some(file) = self.class(&interface) else {
                continue;
            };
            let defaults = declares_default_method(file);
            pending.extend(file.interface_names().into_iter().rev());
            if defaults {
                self.ensure_initialized(&interface)?;
            }
        }
        Ok(())
    }

    /// Run a class's own `<clinit>`, when it has one.
    fn run_class_initializer(&mut self, class: &str) -> Result<(), NativeError> {
        if self
            .class(class)
            .and_then(|file| file.method("<clinit>", "()V"))
            .is_some()
        {
            self.invoke(class, "<clinit>", "()V", None, &[])?;
        }
        Ok(())
    }
}

/// What the first access sees when a `<clinit>` throws: the throw itself when
/// it is an `Error`, otherwise `ExceptionInInitializerError` (whose message is
/// null). The wrapped cause stays in the diagnostic's frame text, as the
/// JVM's `Caused by:` line does.
fn wrap_initializer_throw(
    classes: &BTreeMap<String, ClassFile>,
    linked: &LinkedClassTable,
    thrown: String,
    message: Option<String>,
    frame: Option<String>,
) -> NativeError {
    if exception_is_subtype(classes, linked, &thrown, "java/lang/Error") {
        return NativeError::UncaughtException {
            class: thrown,
            message,
            frame,
        };
    }
    let mut cause = binary_name(&thrown);
    if let Some(message) = &message {
        cause.push_str(": ");
        cause.push_str(message);
    }
    let frame = match frame {
        Some(frame) => format!("{frame}; caused by {cause}"),
        None => format!("caused by {cause}"),
    };
    NativeError::UncaughtException {
        class: "java/lang/ExceptionInInitializerError".into(),
        message: None,
        frame: Some(frame),
    }
}

/// What a later access to an erroneous class sees.
fn erroneous_access(class: &str, cause: &str) -> NativeError {
    match cause.strip_prefix(REFUSED_FAILURE) {
        Some(refusal) => NativeError::Metadata(format!(
            "class {} failed to initialize earlier: {refusal}",
            binary_name(class)
        )),
        None => NativeError::UncaughtException {
            class: "java/lang/NoClassDefFoundError".into(),
            message: Some(format!("Could not initialize class {}", binary_name(class))),
            frame: None,
        },
    }
}
