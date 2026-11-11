//! The `NullPointerException`s the JVM raises when an instruction needs an
//! object and finds `null` (JVMS §6.5: `invokevirtual`, `invokeinterface`,
//! `invokespecial`, `getfield`, `putfield`, `monitorenter`, `athrow`).
//!
//! The interpreter used to carry on instead: a call on `null` ran the jar's
//! body with `this == null` or fell into a host model, a field write on
//! `null` was dropped, and the run read `ran-clean` where Java stops. These
//! helpers build the real, catchable throwable so a jar's own
//! `catch (NullPointerException)` (or `catch (Exception)`) still runs, exactly
//! as on the JVM.
//!
//! The messages follow JDK 14+'s helpful NPE text (`Cannot invoke
//! "N.answer(int, String)"`, `Cannot read field "v"`), measured with OpenJDK
//! 27. One departure: the JVM appends a `because "<local1>" is null` clause
//! naming where the null came from, which needs a data-flow pass over the
//! method; the bounded runtime leaves that clause out.

use crate::runtime::{MethodRef, NativeError};

fn npe(message: String) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: Some(message),
        frame: None,
    }
}

/// `invokevirtual`/`invokeinterface`/`invokespecial` on a `null` receiver.
pub(crate) fn invoke_on_null(r: &MethodRef) -> NativeError {
    let parameters = crate::runtime::descriptor_arguments(&r.descriptor)
        .iter()
        .map(|parameter| java_type_name(parameter))
        .collect::<Vec<_>>()
        .join(", ");
    npe(format!(
        "Cannot invoke \"{}.{}({parameters})\"",
        java_class_name(&r.owner),
        r.name
    ))
}

/// `getfield` on `null`.
pub(crate) fn read_field_on_null(field: &MethodRef) -> NativeError {
    npe(format!("Cannot read field \"{}\"", field.name))
}

/// `putfield` on `null`.
pub(crate) fn assign_field_on_null(field: &MethodRef) -> NativeError {
    npe(format!("Cannot assign field \"{}\"", field.name))
}

/// `monitorenter` on `null` (`synchronized (null) {}`).
pub(crate) fn enter_monitor_on_null() -> NativeError {
    npe("Cannot enter synchronized block".into())
}

/// `athrow` of `null` (`throw null`).
pub(crate) fn throw_null() -> NativeError {
    npe("Cannot throw exception".into())
}

/// A binary class name as the helpful NPE message prints it: dotted, with
/// `java.lang.Object` and `java.lang.String` shortened (HotSpot's
/// `print_klass_name` shortens exactly those two).
fn java_class_name(internal: &str) -> String {
    match internal {
        "java/lang/Object" => "Object".into(),
        "java/lang/String" => "String".into(),
        other => other.replace('/', "."),
    }
}

/// A field descriptor as Java source names it (`I` → `int`, `[[Ljava/lang/Object;`
/// → `Object[][]`).
fn java_type_name(descriptor: &str) -> String {
    let dimensions = descriptor.bytes().take_while(|&b| b == b'[').count();
    let element = &descriptor[dimensions..];
    let base = match element {
        "Z" => "boolean".into(),
        "B" => "byte".into(),
        "C" => "char".into(),
        "S" => "short".into(),
        "I" => "int".into(),
        "J" => "long".into(),
        "F" => "float".into(),
        "D" => "double".into(),
        "V" => "void".into(),
        other => java_class_name(
            other
                .strip_prefix('L')
                .and_then(|name| name.strip_suffix(';'))
                .unwrap_or(other),
        ),
    };
    format!("{base}{}", "[]".repeat(dimensions))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(error: NativeError) -> String {
        match error {
            NativeError::UncaughtException { class, message, .. } => {
                assert_eq!(class, "java/lang/NullPointerException");
                message.unwrap()
            }
            other => panic!("not an NPE: {other:?}"),
        }
    }

    /// The method text of OpenJDK 27's helpful NPE for
    /// `n.answer(1, null, null, null)` with
    /// `int answer(int, String, int[], Object[][])` declared on class `N`:
    /// `Cannot invoke "N.answer(int, String, int[], Object[][])" because "<local1>" is null`.
    #[test]
    fn invoke_message_matches_the_jdk_method_text() {
        let error = invoke_on_null(&MethodRef {
            owner: "N".into(),
            name: "answer".into(),
            descriptor: "(ILjava/lang/String;[I[[Ljava/lang/Object;)I".into(),
        });
        assert_eq!(
            message(error),
            "Cannot invoke \"N.answer(int, String, int[], Object[][])\""
        );
        let error = invoke_on_null(&MethodRef {
            owner: "java/lang/Runnable".into(),
            name: "run".into(),
            descriptor: "()V".into(),
        });
        assert_eq!(message(error), "Cannot invoke \"java.lang.Runnable.run()\"");
    }
}
