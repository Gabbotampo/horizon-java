//! The JDK's own class hierarchy, as Java 25 reports it: the data a "this object is not a
//! `wanted`" answer may rest on (#156, `host_ref.rs`).
//!
//! `jdk_supertypes_25.txt` is generated, never edited: `tests/fixtures/jdk_supertypes/
//! JdkSupertypes.java` run on OpenJDK 25 prints, for each class the runtime's JDK answers read,
//! whether it is final or an interface and its whole supertype set (`getSuperclass()` and
//! `getInterfaces()`, transitively), then every package of every system module. A JDK class's
//! supertypes can only come from those packages: the boot and platform loaders that define JDK
//! classes cannot see a class path, so a JDK class never extends or implements a jar's or a game's
//! type. That is a proof; "a JDK class is not an `org.w3c.dom.Element`" is not one (`IIOMetadataNode`
//! is).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const DATA: &str = include_str!("jdk_supertypes_25.txt");

/// One generated row.
pub(crate) struct JdkClass {
    pub(crate) is_final: bool,
    pub(crate) is_interface: bool,
    /// Every supertype, the class itself left out, `java/lang/Object` included (but for `Object`).
    pub(crate) supertypes: Vec<&'static str>,
    /// [`Self::supertypes`] without `java/lang/Object`: what `jdk_model::platform_supertypes`
    /// lists.
    pub(crate) below_object: Vec<&'static str>,
}

struct Table {
    classes: HashMap<&'static str, JdkClass>,
    packages: HashSet<&'static str>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut classes = HashMap::new();
        let mut packages = HashSet::new();
        for line in DATA.lines().filter(|line| !line.starts_with('#')) {
            let mut fields = line.split('\t');
            match (fields.next(), fields.next(), fields.next(), fields.next()) {
                (Some("class"), Some(name), Some(flags), supertypes) => {
                    let supertypes: Vec<&str> = supertypes
                        .unwrap_or_default()
                        .split(' ')
                        .filter(|name| !name.is_empty())
                        .collect();
                    let below_object = supertypes
                        .iter()
                        .copied()
                        .filter(|name| *name != "java/lang/Object")
                        .collect();
                    classes.insert(
                        name,
                        JdkClass {
                            is_final: flags == "final",
                            is_interface: flags == "interface",
                            supertypes,
                            below_object,
                        },
                    );
                }
                (Some("package"), Some(name), None, None) => {
                    packages.insert(name);
                }
                _ => panic!("malformed jdk_supertypes_25.txt line: {line:?}"),
            }
        }
        Table { classes, packages }
    })
}

/// The generated row of JDK class `name`.
pub(crate) fn class(name: &str) -> Option<&'static JdkClass> {
    table().classes.get(name)
}

/// Whether `name`'s package belongs to a system module (`java/util/List`, `org/w3c/dom/Node`,
/// `sun/nio/fs/UnixPath`), so a JDK class may have it as a supertype.
pub(crate) fn in_jdk_package(name: &str) -> bool {
    name.rsplit_once('/')
        .is_some_and(|(package, _)| table().packages.contains(package))
}

/// `JdkClass::below_object` of `name`, empty when it is not generated.
pub(crate) fn below_object(name: &str) -> &'static [&'static str] {
    class(name).map_or(&[], |row| row.below_object.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated rows are the oracle: every JDK model that answers `instanceof` for one of
    /// these classes says `true` exactly for its Java 25 supertypes, never for another class the
    /// table names (#163: `WeakHashMap` was `Cloneable`, `Hashtable` an `AbstractMap`,
    /// `CopyOnWriteArrayList` an `AbstractList`).
    #[test]
    fn jdk_models_answer_java_25_supertypes() {
        let names: Vec<&str> = table()
            .classes
            .iter()
            .flat_map(|(name, row)| std::iter::once(*name).chain(row.supertypes.iter().copied()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        for (class, row) in &table().classes {
            for wanted in &names {
                if *wanted == "java/lang/Object" || wanted == class {
                    continue;
                }
                let java = row.supertypes.contains(wanted);
                // `jdk_model`'s platform rows answer from the table; the immutable collections'
                // hand rows must never say a "yes" Java does not.
                let platform = crate::jdk_model::class_instance_of(class, wanted);
                assert!(!platform || java, "{class} is not a {wanted} on Java 25");
                let immutable = crate::java_immutable::supertypes(class).contains(wanted);
                assert!(!immutable || java, "{class} is not a {wanted} on Java 25");
                assert_eq!(
                    crate::host_ref::ClassHierarchy::is_instance(
                        &crate::host_ref::JdkHierarchy,
                        class,
                        wanted
                    ),
                    Some(java),
                    "{class} instanceof {wanted}"
                );
            }
        }
        // A JDK class may implement a JDK-module type outside `java/` (`IIOMetadataNode` is an
        // `org.w3c.dom.Element`): not provably "no" ...
        assert_eq!(
            crate::host_ref::ClassHierarchy::is_instance(
                &crate::host_ref::JdkHierarchy,
                "javax/imageio/metadata/IIOMetadataNode",
                "org/w3c/dom/Element"
            ),
            None
        );
        // ... but never a jar's or a game's.
        assert_eq!(
            crate::host_ref::ClassHierarchy::is_instance(
                &crate::host_ref::JdkHierarchy,
                "java/util/AbstractList",
                "net/minecraft/world/Container"
            ),
            Some(false)
        );
    }

    /// Rerun the generator when the JDK at hand is the one the table records (OpenJDK 25.0.5):
    /// its output must be the committed file, byte for byte.
    #[test]
    fn the_table_is_the_generators_output() {
        let java = std::env::var_os("JAVA_HOME")
            .map(|home| std::path::PathBuf::from(home).join("bin/java"))
            .unwrap_or_else(|| "java".into());
        let version = std::process::Command::new(&java)
            .args(["-XshowSettings:properties", "-version"])
            .output();
        let Ok(version) = version else {
            return;
        };
        let recorded = DATA.lines().next().unwrap_or_default();
        let here = String::from_utf8_lossy(&version.stderr);
        if !here.contains("java.version = 25.0.5") || !recorded.contains("Java 25.0.5") {
            eprintln!("skipped: the table records {recorded:?}, this JDK is not 25.0.5");
            return;
        }
        let source = [
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/jdk_supertypes/JdkSupertypes.java"),
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "../crates/horizon-native-java/tests/fixtures/jdk_supertypes/JdkSupertypes.java",
            ),
        ]
        .into_iter()
        .find(|path| path.is_file())
        .expect("the generator is in tests/fixtures/jdk_supertypes");
        let output = std::process::Command::new(&java)
            .arg(source)
            .output()
            .expect("java runs the generator");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), DATA);
    }
}
