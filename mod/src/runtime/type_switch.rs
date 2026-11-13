//! `SwitchBootstraps.typeSwitch`, the call site of a pattern-matching
//! `switch` (`switch (value) { case Enum e -> ...; case Boolean b -> ... }`).
//!
//! The site answers the index of the first label at or after the restart
//! index that the value matches, `-1` for `null`, and the label count when
//! none matches, which the following `tableswitch` turns into the `default`
//! arm. Labels are the bootstrap's static arguments: a class (an `instanceof`
//! test), a `String` or an `Integer` (an equality test). A site with any other
//! label (an `EnumDesc` constant) is left on the interpreter's generic path.
//! ShulkerBoxTooltip builds its config readers through one such switch.

use super::*;

/// One `typeSwitch` label.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SwitchLabel {
    Class(String),
    String(String),
    Int(i32),
}

impl ClassFile {
    /// The labels of an `invokedynamic` site bootstrapped by
    /// `SwitchBootstraps.typeSwitch`, or `None` for any other site.
    pub(crate) fn type_switch_labels(&self, index: u16) -> Option<Vec<SwitchLabel>> {
        let bootstrap = match self.pool.get(index as usize) {
            Some(Cp::InvokeDynamic { bootstrap, .. }) => {
                self.bootstrap_methods.get(*bootstrap as usize)?
            }
            _ => return None,
        };
        let handle = match self.pool.get(bootstrap.method_ref as usize) {
            Some(Cp::MethodHandle { reference_index }) => *reference_index,
            _ => return None,
        };
        let target = self.method_ref(handle).ok()?;
        if target.owner != "java/lang/runtime/SwitchBootstraps" || target.name != "typeSwitch" {
            return None;
        }
        bootstrap
            .arguments
            .iter()
            .map(|argument| match self.pool.get(*argument as usize)? {
                Cp::Class(_) => class_name(&self.pool, *argument)
                    .ok()
                    .map(SwitchLabel::Class),
                Cp::String(text) => match self.pool.get(*text as usize)? {
                    Cp::Utf8(text) => Some(SwitchLabel::String(text.clone())),
                    _ => None,
                },
                Cp::Integer(value) => Some(SwitchLabel::Int(*value)),
                _ => None,
            })
            .collect()
    }
}

impl<H: Host> Runtime<'_, H> {
    /// The index a `typeSwitch` site answers for `value`, starting at
    /// `restart`.
    pub(crate) fn type_switch_index(
        &mut self,
        labels: &[SwitchLabel],
        value: &Value,
        restart: i32,
    ) -> Result<i32, NativeError> {
        if *value == Value::Ref(None) {
            return Ok(-1);
        }
        let start = usize::try_from(restart).unwrap_or(0);
        for (index, label) in labels.iter().enumerate().skip(start) {
            let matched = match label {
                // A handle answers by its kind's row; one that cannot tell is
                // counted, as the `instanceof` opcode counts it.
                SwitchLabel::Class(class) => match value {
                    Value::Host(handle) => match handle.instance_of(class, &*self) {
                        Some(known) => known,
                        None => {
                            let check = MethodRef {
                                owner: class.clone(),
                                name: "<instanceof>".into(),
                                descriptor: "(Ljava/lang/Object;)Z".into(),
                            };
                            matches!(
                                self.host.invoke(&check, std::slice::from_ref(value))?,
                                Value::Int(flag) if flag != 0
                            )
                        }
                    },
                    _ => self.switch_instance_of(value, class),
                },
                SwitchLabel::String(text) => expect_ref(value).is_some_and(|marker| marker == text),
                SwitchLabel::Int(expected) => boxed_int(value) == Some(*expected),
            };
            if matched {
                return Ok(index as i32);
            }
        }
        Ok(labels.len() as i32)
    }

    /// `value instanceof class` for a switch label: jar objects through their
    /// hierarchy, boxed and string values through their JDK class, and game
    /// enum constants as `Enum`s.
    fn switch_instance_of(&self, value: &Value, class: &str) -> bool {
        if class == "java/lang/Object" {
            return true;
        }
        if let Some(own) = class_of_value(value)
            && (own == class || self.class_is_instance(&own, class))
        {
            return true;
        }
        let Some(marker) = expect_ref(value) else {
            return false;
        };
        if class == "java/lang/Enum" && marker.starts_with("enum/") {
            return true;
        }
        // A string is a bare marker: no object, boxed, collection, array,
        // callback, enum or game prefix claims it.
        if class == "java/lang/String" || class == "java/lang/CharSequence" {
            let claimed = class_of_value(value).is_some()
                || marker.contains('#')
                || ["array/", "@", "enum/", "game/", "registry/", "identifier/"]
                    .iter()
                    .any(|prefix| marker.starts_with(prefix));
            return !claimed;
        }
        self.object_is_instance_of(marker, class)
    }
}

/// An `int` a switch value carries, boxed or not.
fn boxed_int(value: &Value) -> Option<i32> {
    match value {
        Value::Int(value) | Value::Boxed(crate::runtime::Boxed::Integer(value)) => Some(*value),
        _ => None,
    }
}
