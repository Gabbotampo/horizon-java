//! `net.minecraft.Optionull` and `Class.getEnclosingClass`, run on a probe
//! compiled with `javac`.
//!
//! Every expected string below is what the same `demo.Probe` printed on
//! JDK 25 (`jdk-25.0.5+8`) against the real 26.3 `server-game.jar` (whose
//! `Optionull` ran), recorded once: the stub here only lets `javac` compile
//! the probe, and is never packed. The `IncompatibleClassChangeError` text
//! is JDK 25's for a member class whose outer class was recompiled without
//! it (`Pair` v2 beside `Pair$Member` from v1).

#[path = "support/scratch.rs"]
mod scratch;

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{NativeJar, StdHost, Value};
use javac_fixture::Fixture;
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;

const OPTIONULL_STUB: &str = r#"package net.minecraft;

import java.util.Collection;
import java.util.function.Function;
import java.util.function.Supplier;

// Compiled against, never packed: every call reaches the runtime's model.
public class Optionull {
    public static <T> T orElse(T t, T defaultValue) { throw new AssertionError(); }
    public static <T, R> R map(T t, Function<T, R> map) { throw new AssertionError(); }
    public static <T, R> R mapOrDefault(T t, Function<T, R> map, R defaultValue) { throw new AssertionError(); }
    public static <T, R> R mapOrElse(T t, Function<T, R> map, Supplier<R> elseSupplier) { throw new AssertionError(); }
    public static <T> T first(Collection<T> collection) { throw new AssertionError(); }
    public static <T> T firstOrDefault(Collection<T> collection, T defaultValue) { throw new AssertionError(); }
    public static <T> T firstOrElse(Collection<T> collection, Supplier<T> elseSupplier) { throw new AssertionError(); }
    public static <T> boolean isNullOrEmpty(T[] t) { throw new AssertionError(); }
    public static boolean isNullOrEmpty(boolean[] t) { throw new AssertionError(); }
    public static boolean isNullOrEmpty(byte[] t) { throw new AssertionError(); }
    public static boolean isNullOrEmpty(char[] t) { throw new AssertionError(); }
    public static boolean isNullOrEmpty(short[] t) { throw new AssertionError(); }
    public static boolean isNullOrEmpty(int[] t) { throw new AssertionError(); }
    public static boolean isNullOrEmpty(long[] t) { throw new AssertionError(); }
    public static boolean isNullOrEmpty(float[] t) { throw new AssertionError(); }
    public static boolean isNullOrEmpty(double[] t) { throw new AssertionError(); }
}
"#;

const PROBE: &str = r#"package demo;

import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.function.Function;
import java.util.function.Supplier;
import net.minecraft.Optionull;

public final class Probe {
    static final class Upper implements Function<String, String> {
        public String apply(String value) { return value.toUpperCase(); }
    }

    static final class Fallback implements Supplier<String> {
        int calls;
        public String get() { calls++; return "fallback" + calls; }
    }

    public static class Outer {
        public static class Inner {
            public static class Deepest {}
        }
    }

    public interface Shape {}

    static final Object STATIC_ANON = new Object() {};
    static final Supplier<Object> FROM_LAMBDA = () -> {
        class InLambda {}
        return new InLambda();
    };

    public static String map() {
        StringBuilder out = new StringBuilder();
        out.append(Optionull.map("abc", String::length)).append('|');
        out.append(Optionull.<String, Integer>map(null, String::length)).append('|');
        out.append(Optionull.mapOrDefault("abc", value -> value + "!", "d")).append('|');
        out.append(Optionull.mapOrDefault((String) null, value -> value + "!", "d")).append('|');
        Fallback fallback = new Fallback();
        out.append(Optionull.mapOrElse("x", new Upper(), fallback)).append('|');
        out.append(fallback.calls).append('|');
        out.append(Optionull.mapOrElse((String) null, new Upper(), fallback)).append('|');
        out.append(Optionull.mapOrElse((String) null, new Upper(), fallback)).append('|');
        out.append(Optionull.mapOrDefault("abc", value -> null, "d")).append('|');
        // A null function is never touched when the value is null.
        out.append((Object) Optionull.map((String) null, null)).append('|');
        out.append((Object) Optionull.mapOrDefault((String) null, null, "kept"));
        return out.toString();
    }

    public static String orElse() {
        StringBuilder out = new StringBuilder();
        out.append(Optionull.orElse("a", "b")).append(Optionull.orElse(null, "b")).append('|');
        out.append(Optionull.orElse("a", null)).append('|');
        try {
            out.append(Optionull.<String>orElse(null, null));
        } catch (NullPointerException error) {
            out.append("npe:").append(error.getMessage());
        }
        return out.toString();
    }

    public static String first() {
        StringBuilder out = new StringBuilder();
        List<String> list = new ArrayList<>();
        list.add("p");
        list.add("q");
        LinkedHashSet<String> set = new LinkedHashSet<>();
        set.add("s1");
        set.add("s2");
        out.append(Optionull.first(list)).append(Optionull.first(new ArrayList<String>())).append('|');
        out.append(Optionull.first(set)).append('|');
        out.append(Optionull.firstOrDefault(list, "d")).append(Optionull.firstOrDefault(new ArrayList<String>(), "d")).append('|');
        Fallback fallback = new Fallback();
        out.append(Optionull.firstOrElse(list, fallback)).append(fallback.calls).append('|');
        out.append(Optionull.firstOrElse(new ArrayList<String>(), fallback)).append(fallback.calls).append('|');
        out.append(Optionull.firstOrElse(new ArrayList<String>(), () -> "lambda"));
        return out.toString();
    }

    public static String nulls() {
        StringBuilder out = new StringBuilder();
        try {
            Optionull.map("x", null);
            out.append("ran");
        } catch (NullPointerException error) {
            out.append(error.getMessage());
        }
        out.append('|');
        try {
            Optionull.mapOrElse(null, value -> value, null);
            out.append("ran");
        } catch (NullPointerException error) {
            out.append(error.getMessage());
        }
        out.append('|');
        try {
            Optionull.first(null);
            out.append("ran");
        } catch (NullPointerException error) {
            out.append(error.getMessage());
        }
        out.append('|');
        try {
            Optionull.firstOrElse(new ArrayList<String>(), null);
            out.append("ran");
        } catch (NullPointerException error) {
            out.append(error.getMessage());
        }
        return out.toString();
    }

    public static String empty() {
        StringBuilder out = new StringBuilder();
        out.append(Optionull.isNullOrEmpty((Object[]) null)).append(',');
        out.append(Optionull.isNullOrEmpty(new String[0])).append(',');
        out.append(Optionull.isNullOrEmpty(new String[] {"a"})).append(',');
        out.append(Optionull.isNullOrEmpty((int[]) null)).append(',');
        out.append(Optionull.isNullOrEmpty(new int[0])).append(',');
        out.append(Optionull.isNullOrEmpty(new int[3])).append(',');
        out.append(Optionull.isNullOrEmpty(new boolean[1])).append(',');
        out.append(Optionull.isNullOrEmpty(new byte[0])).append(',');
        out.append(Optionull.isNullOrEmpty(new char[2])).append(',');
        out.append(Optionull.isNullOrEmpty(new short[0])).append(',');
        out.append(Optionull.isNullOrEmpty(new long[1])).append(',');
        out.append(Optionull.isNullOrEmpty(new float[0])).append(',');
        out.append(Optionull.isNullOrEmpty((double[]) null)).append(',');
        out.append(Optionull.isNullOrEmpty(new double[1]));
        return out.toString();
    }

    static String name(Class<?> type) {
        return type == null ? "null" : type.getName();
    }

    public static String enclosing() {
        class Local {}
        Object anonymous = new Object() {};
        Shape shape = new Shape() {};
        return name(Probe.class.getEnclosingClass()) + "|"
            + name(Upper.class.getEnclosingClass()) + "|"
            + name(Outer.Inner.class.getEnclosingClass()) + "|"
            + name(Outer.Inner.Deepest.class.getEnclosingClass()) + "|"
            + name(Shape.class.getEnclosingClass()) + "|"
            + name(Local.class.getEnclosingClass()) + "|"
            + name(anonymous.getClass().getEnclosingClass()) + "|"
            + name(shape.getClass().getEnclosingClass()) + "|"
            + name(STATIC_ANON.getClass().getEnclosingClass()) + "|"
            + name(FROM_LAMBDA.get().getClass().getEnclosingClass());
    }

    /// A JDK class has no class file here: the host counts the call.
    public static String hostClass() {
        return name(java.util.Map.Entry.class.getEnclosingClass());
    }
}
"#;

fn fixture(test: &str) -> Fixture {
    Fixture::build(
        test,
        &[
            ("net/minecraft/Optionull.java", OPTIONULL_STUB),
            ("demo/Probe.java", PROBE),
        ],
    )
}

#[test]
fn optionull_map_members_run_the_function_or_the_fallback() {
    let fixture = fixture("optionull-map");
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "map"),
        "3|null|abc!|d|X|0|fallback1|fallback2|null|null|kept"
    );
    assert_eq!(fixture.text_on(&mut host, "orElse"), "ab|a|npe:defaultObj");
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn optionull_first_members_read_the_iteration_order() {
    let fixture = fixture("optionull-first");
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "first"),
        "pnull|s1|pd|p0|fallback11|lambda"
    );
    assert_eq!(
        fixture.text_on(&mut host, "empty"),
        "true,true,false,true,true,false,false,true,false,true,false,true,true,false"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn optionull_null_parameters_throw_the_jvms_helpful_npe() {
    let fixture = fixture("optionull-nulls");
    assert_eq!(
        fixture.text("nulls"),
        "Cannot invoke \"java.util.function.Function.apply(Object)\" because \"map\" is null|\
         Cannot invoke \"java.util.function.Supplier.get()\" because \"elseSupplier\" is null|\
         Cannot invoke \"java.util.Collection.iterator()\" because \"collection\" is null|\
         Cannot invoke \"java.util.function.Supplier.get()\" because \"elseSupplier\" is null"
    );
}

#[test]
fn enclosing_class_reads_enclosing_method_then_inner_classes() {
    let fixture = fixture("enclosing-class");
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "enclosing"),
        "null|demo.Probe|demo.Probe$Outer|demo.Probe$Outer$Inner|demo.Probe|demo.Probe|\
         demo.Probe|demo.Probe|demo.Probe|demo.Probe"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn enclosing_class_of_a_class_the_runtime_does_not_hold_is_counted() {
    let fixture = fixture("enclosing-host");
    let mut host = fixture.jar.seed_host(StdHost::new());
    // `java.util.Map` on a JVM; the runtime has no class file to read.
    fixture.text_on(&mut host, "hostClass");
    assert_eq!(
        host.unknown_host_calls(),
        vec![(
            "java/lang/Class#getEnclosingClass()Ljava/lang/Class;".to_owned(),
            1
        )]
    );
}

/// `javac` one source into `out`, returning the class files it wrote.
fn compile(root: &std::path::Path, name: &str, source: &str) -> PathBuf {
    let src = root.join(name).join("demo/Pair.java");
    fs::create_dir_all(src.parent().unwrap()).unwrap();
    fs::write(&src, source).unwrap();
    let out = root.join(name).join("classes");
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .args(["--release", "21", "-d"])
        .arg(&out)
        .arg(&src)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed for {name}");
    out.join("demo")
}

#[test]
fn enclosing_class_throws_when_the_outer_class_disagrees() {
    let root = scratch::Scratch::new("enclosing-disagree");
    let v1 = compile(
        &root,
        "v1",
        "package demo; public class Pair { public static class Member {} \
         public static String enclosing() { try { return String.valueOf(Member.class.getEnclosingClass()); } \
         catch (IncompatibleClassChangeError error) { return error.getMessage(); } } }",
    );
    // `Pair` recompiled without its member: its `InnerClasses` no longer
    // lists `Pair$Member`, while `Pair$Member` still names `Pair`.
    let v2 = compile(
        &root,
        "v2",
        "package demo; public class Pair { \
         public static String enclosing() { try { return String.valueOf(\
         Class.forName(\"demo.Pair$Member\").getEnclosingClass()); } \
         catch (IncompatibleClassChangeError error) { return error.getMessage(); } \
         catch (ClassNotFoundException error) { return \"missing\"; } } }",
    );
    let jar_path = root.join("pair.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    zip.start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(br#"{"id":"pair","version":"1.0","entrypoints":{"main":"demo.Pair"}}"#)
        .unwrap();
    for (entry, file) in [
        ("demo/Pair.class", v2.join("Pair.class")),
        ("demo/Pair$Member.class", v1.join("Pair$Member.class")),
    ] {
        zip.start_file(entry, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(file).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    let answer = jar
        .invoke_method_with_args(
            "demo/Pair",
            "enclosing",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        answer,
        Value::Ref(Some(
            "demo.Pair and demo.Pair$Member disagree on InnerClasses attribute".into()
        ))
    );
}
