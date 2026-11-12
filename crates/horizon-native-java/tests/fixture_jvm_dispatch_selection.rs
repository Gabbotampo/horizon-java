//! JVMS method selection details for `jvm_dispatch.rs`, from an independent
//! review's programs: transitive overriding of a package-private method
//! (JVMS §5.4.5), the maximally specific superinterface default (§5.4.3.3),
//! and the errors §5.4.6 leaves for conflicting or abstract-only defaults.
//!
//! The conflict and re-abstraction cases need separate compilation (javac
//! rejects them in one compile): `lib/` is compiled once with empty
//! interfaces next to the probe, then again with the defaults added, and
//! the second build's classes are what the probe jar ships, as a library
//! upgrade would.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same classes
//! (`javac --release 21` in the same two stages, `java -cp`), pasted
//! verbatim, exception messages included.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;
use std::{fs, path::PathBuf, process::Command};

const PROBE: &str = r#"package demo;

import lib.IX;
import lib.IY;
import lib.IZ;
import lib.IW;

public final class Probe {
    interface IA { default String who() { return "IA"; } }
    interface IB extends IA { @Override default String who() { return "IB"; } }
    interface IC extends IA { }
    interface ID extends IC, IB { }

    static final class Diamond implements IA, IB { }
    static class BaseB implements IB { }
    static final class SubA extends BaseB implements IA { }
    static final class Deep implements IA, ID { }

    /// `IY` gains a default after this class was compiled: two unrelated
    /// defaults (JVMS 5.4.6 step 3 selects neither).
    static final class Amb implements IX, IY { }
    /// `IZ` gains an abstract `who()` unrelated to `IX`'s default.
    static final class Half implements IX, IZ { }
    /// `IW` re-abstracts `IX.who()` after this class was compiled.
    static final class Re implements IW { }

    public static String diamonds() {
        Diamond d = new Diamond();
        SubA s = new SubA();
        Deep deep = new Deep();
        return ((IA) d).who() + "|" + d.who() + "|" + ((IA) s).who() + "|" + ((IA) deep).who();
    }

    public static String transitive() {
        return demo.p1.A.callTag(new demo.p2.C()) + "|" + demo.p1.A.callTag(new demo.p2.D());
    }

    static String attempt(IX x) {
        try {
            return x.who();
        } catch (Throwable error) {
            return error.getClass().getName() + ": " + error.getMessage();
        }
    }

    public static String conflicting() { return attempt(new Amb()); }
    public static String halfAbstract() { return attempt(new Half()); }
    public static String reabstracted() { return attempt(new Re()); }
}
"#;

const A: &str = r#"package demo.p1;

public class A {
    String tag() { return "A"; }
    public static String callTag(A a) { return a.tag(); }
}
"#;

const B: &str = r#"package demo.p1;

/// Overrides the package-private `A.tag` from its own package and widens it.
public class B extends A {
    @Override public String tag() { return "B"; }
}
"#;

const C: &str = r#"package demo.p2;

/// Overrides `B.tag`, so transitively `A.tag` (JVMS 5.4.5).
public class C extends demo.p1.B {
    @Override public String tag() { return "C"; }
}
"#;

const D: &str = r#"package demo.p2;

/// Does not override `A.tag`: package-private in another package, and no
/// intermediate class widens it.
public class D extends demo.p1.A {
    String tag() { return "D"; }
}
"#;

/// `lib/` as the probe was compiled against it.
const IX: &str = r#"package lib;

public interface IX { default String who() { return "IX"; } }
"#;
const IY_BEFORE: &str = r#"package lib;

public interface IY { }
"#;
const IZ_BEFORE: &str = r#"package lib;

public interface IZ { }
"#;
const IW_BEFORE: &str = r#"package lib;

public interface IW extends IX { }
"#;

/// `lib/` as the jar ships it.
const IY_AFTER: &str = r#"package lib;

public interface IY { default String who() { return "IY"; } }
"#;
const IZ_AFTER: &str = r#"package lib;

public interface IZ { String who(); }
"#;
const IW_AFTER: &str = r#"package lib;

public interface IW extends IX { @Override String who(); }
"#;

/// Compile the shipped `lib/` interfaces on their own and read their class
/// files back.
fn shipped_lib(test: &str) -> Vec<(String, Vec<u8>)> {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-{test}-lib-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let mut command = Command::new(javac);
    command
        .args(["--release", "21", "-d"])
        .arg(root.join("out"));
    for (name, source) in [
        ("IX", IX),
        ("IY", IY_AFTER),
        ("IZ", IZ_AFTER),
        ("IW", IW_AFTER),
    ] {
        let file = root.join("src/lib").join(format!("{name}.java"));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        command.arg(file);
    }
    assert!(
        command.status().unwrap().success(),
        "javac failed for {test}"
    );
    let classes = ["IX", "IY", "IZ", "IW"]
        .into_iter()
        .map(|name| {
            let bytes = fs::read(root.join("out/lib").join(format!("{name}.class"))).unwrap();
            (format!("lib/{name}.class"), bytes)
        })
        .collect();
    let _ = fs::remove_dir_all(&root);
    classes
}

fn fixture(test: &str) -> Fixture {
    let shipped = shipped_lib(test);
    let extra = shipped
        .iter()
        .map(|(entry, bytes)| (entry.as_str(), bytes.as_slice()))
        .collect::<Vec<_>>();
    Fixture::build_with_lib(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/p1/A.java", A),
            ("demo/p1/B.java", B),
            ("demo/p2/C.java", C),
            ("demo/p2/D.java", D),
            ("lib/IX.java", IX),
            ("lib/IY.java", IY_BEFORE),
            ("lib/IZ.java", IZ_BEFORE),
            ("lib/IW.java", IW_BEFORE),
        ],
        &extra,
    )
}

fn exact(test: &str, method: &str) -> String {
    let fixture = fixture(test);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, method);
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{method}: {:?}",
        host.unknown_host_calls()
    );
    text
}

/// `IB extends IA` overrides `who()`: the more specific default wins however
/// the class lists its interfaces (`IA` used to win as the first found).
#[test]
fn the_maximally_specific_default_is_selected() {
    assert_eq!(exact("selection-diamonds", "diamonds"), "IB|IB|IB|IB");
}

/// `B` (same package as `A`) widens the package-private `A.tag` to public,
/// so `C` in another package overrides it; `D` without the widening does not.
#[test]
fn a_widened_package_private_method_is_overridden_transitively() {
    assert_eq!(exact("selection-transitive", "transitive"), "C|A");
}

#[test]
fn two_unrelated_defaults_throw_incompatible_class_change() {
    assert_eq!(
        exact("selection-conflict", "conflicting"),
        "java.lang.IncompatibleClassChangeError: Conflicting default methods: lib/IX.who lib/IY.who"
    );
}

/// One default and one unrelated abstract declaration: the default is the
/// only non-abstract maximally specific method, so it runs.
#[test]
fn a_default_beside_an_unrelated_abstract_method_runs() {
    assert_eq!(exact("selection-half", "halfAbstract"), "IX");
}

#[test]
fn a_re_abstracted_default_throws_abstract_method_error() {
    assert_eq!(
        exact("selection-abstract", "reabstracted"),
        "java.lang.AbstractMethodError: Method demo/Probe$Re.who()Ljava/lang/String; is abstract"
    );
}
