//! `Field#get*`/`set*` run in the interpreter against the JDK's own answers.
//!
//! The `demo/` classes below were compiled with `javac --release 21` and run
//! with `java` (OpenJDK 27); each method's output is pasted verbatim into
//! `EXPECTED`. No expected value is derived from the runtime. The probe covers
//! every getter and typed setter on every primitive field and on reference
//! fields (widening, boxing, the conversion errors), `set(Object)` with each
//! box the value model keeps and with null, static fields (any receiver,
//! `ConstantValue` constants, class initialization), null and wrong receivers,
//! final fields (instance, static, a record's) and a private field read from
//! its own class.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const PROBE: &str = r##"package demo;

import java.lang.reflect.Field;

public final class Probe {
    static String failure(Throwable error) {
        return error.getClass().getSimpleName() + ":" + error.getMessage();
    }

    static final String[] PRIMITIVES = {"z", "b", "c", "s", "i", "j", "f", "d"};

    static String read(Field field, Object target, int getter) {
        try {
            switch (getter) {
                case 0: return String.valueOf(field.get(target));
                case 1: return String.valueOf(field.getBoolean(target));
                case 2: return String.valueOf(field.getByte(target));
                case 3: return String.valueOf(field.getChar(target));
                case 4: return String.valueOf(field.getShort(target));
                case 5: return String.valueOf(field.getInt(target));
                case 6: return String.valueOf(field.getLong(target));
                case 7: return String.valueOf(field.getFloat(target));
                default: return String.valueOf(field.getDouble(target));
            }
        } catch (Exception error) {
            return failure(error);
        }
    }

    static String write(Field field, Object target, int setter) {
        try {
            switch (setter) {
                case 0: field.setBoolean(target, true); break;
                case 1: field.setByte(target, (byte) -5); break;
                case 2: field.setChar(target, 'Z'); break;
                case 3: field.setShort(target, (short) -300); break;
                case 4: field.setInt(target, 70001); break;
                case 5: field.setLong(target, 5000000001L); break;
                case 6: field.setFloat(target, 1.25f); break;
                default: field.setDouble(target, 0.1); break;
            }
            return String.valueOf(field.get(target));
        } catch (Exception error) {
            return failure(error);
        }
    }

    static String put(Field field, Object target, Object value) {
        try {
            field.set(target, value);
            return String.valueOf(field.get(target));
        } catch (Exception error) {
            return failure(error);
        }
    }

    /** Every getter on every primitive field and on two reference fields. */
    public static String getters() throws Exception {
        Holder holder = new Holder();
        StringBuilder out = new StringBuilder();
        for (String name : new String[] {"z", "b", "c", "s", "i", "j", "f", "d", "boxed", "any"}) {
            Field field = Holder.class.getField(name);
            out.append(name).append(':');
            for (int getter = 0; getter < 9; getter++) {
                out.append(read(field, holder, getter)).append(getter < 8 ? "," : "\n");
            }
        }
        return out.toString();
    }

    /** Every typed setter on every primitive field (a fresh object per field). */
    public static String setters() throws Exception {
        StringBuilder out = new StringBuilder();
        for (String name : PRIMITIVES) {
            Field field = Holder.class.getField(name);
            out.append(name).append(':');
            for (int setter = 0; setter < 8; setter++) {
                out.append(write(field, new Holder(), setter)).append(setter < 7 ? "," : "\n");
            }
        }
        out.append("boxed:").append(write(Holder.class.getField("boxed"), new Holder(), 4));
        return out.toString();
    }

    /** `set(Object)` with each box whose class the runtime keeps, and null. */
    public static String boxes() throws Exception {
        Object[] values = {
            Boolean.valueOf(true), Byte.valueOf("5"), Short.valueOf((short) -7),
            Character.valueOf('q'), Integer.valueOf(42), Long.valueOf(9L), null
        };
        StringBuilder out = new StringBuilder();
        for (String name : new String[] {"z", "b", "c", "s", "i", "j", "f", "d", "boxed", "any"}) {
            Field field = Holder.class.getField(name);
            out.append(name).append(':');
            for (int index = 0; index < values.length; index++) {
                out.append(put(field, new Holder(), values[index]))
                    .append(index < values.length - 1 ? "," : "\n");
            }
        }
        Field link = Holder.class.getField("link");
        Holder other = new Holder();
        Holder target = new Holder();
        link.set(target, other);
        out.append("link:").append(link.get(target) == other)
            .append(',').append(put(link, new Holder(), new Other()));
        return out.toString();
    }

    /** A float and a double box into a double field: both widen. */
    public static String wideBoxes() throws Exception {
        Field d = Holder.class.getField("d");
        return put(d, new Holder(), Float.valueOf(0.1f)) + "," + put(d, new Holder(), Double.valueOf(0.1));
    }

    /** Static fields: any receiver, constants, class initialization. */
    public static String statics() throws Exception {
        Field si = Holder.class.getField("si");
        StringBuilder out = new StringBuilder();
        out.append(si.getInt(null)).append(',').append(si.getInt(new Other())).append(',');
        si.setInt("ignored", 13);
        out.append(Holder.si).append(',');
        si.set(null, Short.valueOf((short) 14));
        out.append(si.get(null)).append(',');
        out.append(Holder.class.getField("SFI").getInt(null)).append(',');
        out.append(Holder.class.getField("SFJ").getDouble(null)).append(',');
        out.append(Holder.class.getField("SFS").get(null)).append(',');
        Field value = Lazy.class.getField("value");
        out.append(value.getInt(null)).append(',').append(Lazy.log.toString());
        return out.toString();
    }

    /** Null and wrong receivers on an instance field. */
    public static String receivers() throws Exception {
        Field i = Holder.class.getField("i");
        StringBuilder out = new StringBuilder();
        out.append(read(i, null, 5)).append('|');
        out.append(read(i, new Other(), 5)).append('|');
        out.append(read(i, new Other(), 0)).append('|');
        out.append(read(i, new Other(), 1)).append('|');
        out.append(write(i, new Other(), 4)).append('|');
        out.append(write(i, null, 4)).append('|');
        out.append(put(i, new Other(), Integer.valueOf(1))).append('|');
        out.append(put(i, null, Integer.valueOf(1)));
        return out.toString();
    }

    /** Final fields without setAccessible. */
    public static String finals() throws Exception {
        Field fi = Holder.class.getField("fi");
        Field sfi = Holder.class.getField("SFI");
        StringBuilder out = new StringBuilder();
        out.append(write(fi, new Holder(), 4)).append('|');
        out.append(write(fi, new Holder(), 1)).append('|');
        out.append(write(fi, new Holder(), 5)).append('|');
        out.append(put(fi, new Holder(), Integer.valueOf(1))).append('|');
        out.append(put(fi, new Holder(), null)).append('|');
        out.append(put(fi, new Holder(), Long.valueOf(1))).append('|');
        out.append(write(fi, new Other(), 4)).append('|');
        out.append(write(sfi, null, 4)).append('|');
        out.append(put(sfi, null, Integer.valueOf(1))).append('|');
        out.append(fi.getInt(new Holder()));
        return out.toString();
    }

    /** Counted: setAccessible on a final instance field, then set. */
    public static String finalTouched() throws Exception {
        Field fi = Holder.class.getField("fi");
        fi.setAccessible(true);
        return write(fi, new Holder(), 4);
    }
}
"##;

const HOLDER: &str = r##"package demo;

public class Holder {
    public boolean z = true;
    public byte b = -7;
    public char c = 'A';
    public short s = -300;
    public int i = 70000;
    public long j = 5000000000L;
    public float f = 1.5f;
    public double d = 2.25;
    public Integer boxed = Integer.valueOf(9);
    public Object any;
    public Holder link;
    public final int fi = 3;
    public static int si = 11;
    public static final int SFI = 12;
    public static final long SFJ = 1234567890123L;
    public static final String SFS = "constant";
    private int priv = 4;
    protected int prot = 5;

    /** Same class: a private field needs no setAccessible. */
    public static String own() throws Exception {
        Holder h = new Holder();
        java.lang.reflect.Field priv = Holder.class.getDeclaredField("priv");
        priv.setInt(h, 40);
        return priv.getInt(h) + "," + priv.get(h) + "," + h.priv;
    }
}
"##;

const OTHER: &str = r##"package demo;

public class Other {
    public int i = 1;
}
"##;

const LAZY: &str = r##"package demo;

public class Lazy {
    public static StringBuilder log = new StringBuilder();
    public static int value = init();

    static int init() {
        log.append("init;");
        return 77;
    }
}
"##;

const POINT: &str = r##"package demo;

public record Point(int x, int y) {
    /** A record's fields are final: set throws even from the record itself. */
    public static String setOwn() throws Exception {
        java.lang.reflect.Field x = Point.class.getDeclaredField("x");
        Point p = new Point(1, 2);
        String out = x.getInt(p) + "," + x.get(p);
        try { x.setInt(p, 5); } catch (IllegalAccessException e) { out += "|" + Probe.failure(e); }
        return out;
    }
}
"##;

const OUTSIDE: &str = r##"package demo.sub;

import demo.Holder;

public final class Outside {
    /** A private field from another package without setAccessible. */
    public static String privateRead() throws Exception {
        return String.valueOf(Holder.class.getDeclaredField("priv").getInt(new Holder()));
    }
}
"##;

/// `java -cp <classes> Oracle <Class.method>...`, one block per method,
/// verbatim: `(class, method, output)`.
const EXPECTED: &[(&str, &str, &str)] = &[
    (
        "demo/Probe",
        "getters",
        r#"z:true,true,IllegalArgumentException:Attempt to get boolean field "demo.Holder.z" with illegal data type conversion to byte,IllegalArgumentException:Attempt to get boolean field "demo.Holder.z" with illegal data type conversion to char,IllegalArgumentException:Attempt to get boolean field "demo.Holder.z" with illegal data type conversion to short,IllegalArgumentException:Attempt to get boolean field "demo.Holder.z" with illegal data type conversion to int,IllegalArgumentException:Attempt to get boolean field "demo.Holder.z" with illegal data type conversion to long,IllegalArgumentException:Attempt to get boolean field "demo.Holder.z" with illegal data type conversion to float,IllegalArgumentException:Attempt to get boolean field "demo.Holder.z" with illegal data type conversion to double
b:-7,IllegalArgumentException:Attempt to get byte field "demo.Holder.b" with illegal data type conversion to boolean,-7,IllegalArgumentException:Attempt to get byte field "demo.Holder.b" with illegal data type conversion to char,-7,-7,-7,-7.0,-7.0
c:A,IllegalArgumentException:Attempt to get char field "demo.Holder.c" with illegal data type conversion to boolean,IllegalArgumentException:Attempt to get char field "demo.Holder.c" with illegal data type conversion to byte,A,IllegalArgumentException:Attempt to get char field "demo.Holder.c" with illegal data type conversion to short,65,65,65.0,65.0
s:-300,IllegalArgumentException:Attempt to get short field "demo.Holder.s" with illegal data type conversion to boolean,IllegalArgumentException:Attempt to get short field "demo.Holder.s" with illegal data type conversion to byte,IllegalArgumentException:Attempt to get short field "demo.Holder.s" with illegal data type conversion to char,-300,-300,-300,-300.0,-300.0
i:70000,IllegalArgumentException:Attempt to get int field "demo.Holder.i" with illegal data type conversion to boolean,IllegalArgumentException:Attempt to get int field "demo.Holder.i" with illegal data type conversion to byte,IllegalArgumentException:Attempt to get int field "demo.Holder.i" with illegal data type conversion to char,IllegalArgumentException:Attempt to get int field "demo.Holder.i" with illegal data type conversion to short,70000,70000,70000.0,70000.0
j:5000000000,IllegalArgumentException:Attempt to get long field "demo.Holder.j" with illegal data type conversion to boolean,IllegalArgumentException:Attempt to get long field "demo.Holder.j" with illegal data type conversion to byte,IllegalArgumentException:Attempt to get long field "demo.Holder.j" with illegal data type conversion to char,IllegalArgumentException:Attempt to get long field "demo.Holder.j" with illegal data type conversion to short,IllegalArgumentException:Attempt to get long field "demo.Holder.j" with illegal data type conversion to int,5000000000,5.0E9,5.0E9
f:1.5,IllegalArgumentException:Attempt to get float field "demo.Holder.f" with illegal data type conversion to boolean,IllegalArgumentException:Attempt to get float field "demo.Holder.f" with illegal data type conversion to byte,IllegalArgumentException:Attempt to get float field "demo.Holder.f" with illegal data type conversion to char,IllegalArgumentException:Attempt to get float field "demo.Holder.f" with illegal data type conversion to short,IllegalArgumentException:Attempt to get float field "demo.Holder.f" with illegal data type conversion to int,IllegalArgumentException:Attempt to get float field "demo.Holder.f" with illegal data type conversion to long,1.5,1.5
d:2.25,IllegalArgumentException:Attempt to get double field "demo.Holder.d" with illegal data type conversion to boolean,IllegalArgumentException:Attempt to get double field "demo.Holder.d" with illegal data type conversion to byte,IllegalArgumentException:Attempt to get double field "demo.Holder.d" with illegal data type conversion to char,IllegalArgumentException:Attempt to get double field "demo.Holder.d" with illegal data type conversion to short,IllegalArgumentException:Attempt to get double field "demo.Holder.d" with illegal data type conversion to int,IllegalArgumentException:Attempt to get double field "demo.Holder.d" with illegal data type conversion to long,IllegalArgumentException:Attempt to get double field "demo.Holder.d" with illegal data type conversion to float,2.25
boxed:9,IllegalArgumentException:Attempt to get java.lang.Integer field "demo.Holder.boxed" with illegal data type conversion to boolean,IllegalArgumentException:Attempt to get java.lang.Integer field "demo.Holder.boxed" with illegal data type conversion to byte,IllegalArgumentException:Attempt to get java.lang.Integer field "demo.Holder.boxed" with illegal data type conversion to char,IllegalArgumentException:Attempt to get java.lang.Integer field "demo.Holder.boxed" with illegal data type conversion to short,IllegalArgumentException:Attempt to get java.lang.Integer field "demo.Holder.boxed" with illegal data type conversion to int,IllegalArgumentException:Attempt to get java.lang.Integer field "demo.Holder.boxed" with illegal data type conversion to long,IllegalArgumentException:Attempt to get java.lang.Integer field "demo.Holder.boxed" with illegal data type conversion to float,IllegalArgumentException:Attempt to get java.lang.Integer field "demo.Holder.boxed" with illegal data type conversion to double
any:null,IllegalArgumentException:Attempt to get java.lang.Object field "demo.Holder.any" with illegal data type conversion to boolean,IllegalArgumentException:Attempt to get java.lang.Object field "demo.Holder.any" with illegal data type conversion to byte,IllegalArgumentException:Attempt to get java.lang.Object field "demo.Holder.any" with illegal data type conversion to char,IllegalArgumentException:Attempt to get java.lang.Object field "demo.Holder.any" with illegal data type conversion to short,IllegalArgumentException:Attempt to get java.lang.Object field "demo.Holder.any" with illegal data type conversion to int,IllegalArgumentException:Attempt to get java.lang.Object field "demo.Holder.any" with illegal data type conversion to long,IllegalArgumentException:Attempt to get java.lang.Object field "demo.Holder.any" with illegal data type conversion to float,IllegalArgumentException:Attempt to get java.lang.Object field "demo.Holder.any" with illegal data type conversion to double
"#,
    ),
    (
        "demo/Probe",
        "setters",
        r#"z:true,IllegalArgumentException:Can not set boolean field demo.Holder.z to (byte)-5,IllegalArgumentException:Can not set boolean field demo.Holder.z to (char)Z,IllegalArgumentException:Can not set boolean field demo.Holder.z to (short)-300,IllegalArgumentException:Can not set boolean field demo.Holder.z to (int)70001,IllegalArgumentException:Can not set boolean field demo.Holder.z to (long)5000000001,IllegalArgumentException:Can not set boolean field demo.Holder.z to (float)1.25,IllegalArgumentException:Can not set boolean field demo.Holder.z to (double)0.1
b:IllegalArgumentException:Can not set byte field demo.Holder.b to (boolean)true,-5,IllegalArgumentException:Can not set byte field demo.Holder.b to (char)Z,IllegalArgumentException:Can not set byte field demo.Holder.b to (short)-300,IllegalArgumentException:Can not set byte field demo.Holder.b to (int)70001,IllegalArgumentException:Can not set byte field demo.Holder.b to (long)5000000001,IllegalArgumentException:Can not set byte field demo.Holder.b to (float)1.25,IllegalArgumentException:Can not set byte field demo.Holder.b to (double)0.1
c:IllegalArgumentException:Can not set char field demo.Holder.c to (boolean)true,IllegalArgumentException:Can not set char field demo.Holder.c to (byte)-5,Z,IllegalArgumentException:Can not set char field demo.Holder.c to (short)-300,IllegalArgumentException:Can not set char field demo.Holder.c to (int)70001,IllegalArgumentException:Can not set char field demo.Holder.c to (long)5000000001,IllegalArgumentException:Can not set char field demo.Holder.c to (float)1.25,IllegalArgumentException:Can not set char field demo.Holder.c to (double)0.1
s:IllegalArgumentException:Can not set short field demo.Holder.s to (boolean)true,-5,IllegalArgumentException:Can not set short field demo.Holder.s to (char)Z,-300,IllegalArgumentException:Can not set short field demo.Holder.s to (int)70001,IllegalArgumentException:Can not set short field demo.Holder.s to (long)5000000001,IllegalArgumentException:Can not set short field demo.Holder.s to (float)1.25,IllegalArgumentException:Can not set short field demo.Holder.s to (double)0.1
i:IllegalArgumentException:Can not set int field demo.Holder.i to (boolean)true,-5,90,-300,70001,IllegalArgumentException:Can not set int field demo.Holder.i to (long)5000000001,IllegalArgumentException:Can not set int field demo.Holder.i to (float)1.25,IllegalArgumentException:Can not set int field demo.Holder.i to (double)0.1
j:IllegalArgumentException:Can not set long field demo.Holder.j to (boolean)true,-5,90,-300,70001,5000000001,IllegalArgumentException:Can not set long field demo.Holder.j to (float)1.25,IllegalArgumentException:Can not set long field demo.Holder.j to (double)0.1
f:IllegalArgumentException:Can not set float field demo.Holder.f to (boolean)true,-5.0,90.0,-300.0,70001.0,5.0E9,1.25,IllegalArgumentException:Can not set float field demo.Holder.f to (double)0.1
d:IllegalArgumentException:Can not set double field demo.Holder.d to (boolean)true,-5.0,90.0,-300.0,70001.0,5.000000001E9,1.25,0.1
boxed:IllegalArgumentException:Can not set java.lang.Integer field demo.Holder.boxed to (int)70001"#,
    ),
    (
        "demo/Probe",
        "boxes",
        r#"z:true,IllegalArgumentException:Can not set boolean field demo.Holder.z to java.lang.Byte,IllegalArgumentException:Can not set boolean field demo.Holder.z to java.lang.Short,IllegalArgumentException:Can not set boolean field demo.Holder.z to java.lang.Character,IllegalArgumentException:Can not set boolean field demo.Holder.z to java.lang.Integer,IllegalArgumentException:Can not set boolean field demo.Holder.z to java.lang.Long,IllegalArgumentException:Can not set boolean field demo.Holder.z to null value
b:IllegalArgumentException:Can not set byte field demo.Holder.b to java.lang.Boolean,5,IllegalArgumentException:Can not set byte field demo.Holder.b to java.lang.Short,IllegalArgumentException:Can not set byte field demo.Holder.b to java.lang.Character,IllegalArgumentException:Can not set byte field demo.Holder.b to java.lang.Integer,IllegalArgumentException:Can not set byte field demo.Holder.b to java.lang.Long,IllegalArgumentException:Can not set byte field demo.Holder.b to null value
c:IllegalArgumentException:Can not set char field demo.Holder.c to java.lang.Boolean,IllegalArgumentException:Can not set char field demo.Holder.c to java.lang.Byte,IllegalArgumentException:Can not set char field demo.Holder.c to java.lang.Short,q,IllegalArgumentException:Can not set char field demo.Holder.c to java.lang.Integer,IllegalArgumentException:Can not set char field demo.Holder.c to java.lang.Long,IllegalArgumentException:Can not set char field demo.Holder.c to null value
s:IllegalArgumentException:Can not set short field demo.Holder.s to java.lang.Boolean,5,-7,IllegalArgumentException:Can not set short field demo.Holder.s to java.lang.Character,IllegalArgumentException:Can not set short field demo.Holder.s to java.lang.Integer,IllegalArgumentException:Can not set short field demo.Holder.s to java.lang.Long,IllegalArgumentException:Can not set short field demo.Holder.s to null value
i:IllegalArgumentException:Can not set int field demo.Holder.i to java.lang.Boolean,5,-7,113,42,IllegalArgumentException:Can not set int field demo.Holder.i to java.lang.Long,IllegalArgumentException:Can not set int field demo.Holder.i to null value
j:IllegalArgumentException:Can not set long field demo.Holder.j to java.lang.Boolean,5,-7,113,42,9,IllegalArgumentException:Can not set long field demo.Holder.j to null value
f:IllegalArgumentException:Can not set float field demo.Holder.f to java.lang.Boolean,5.0,-7.0,113.0,42.0,9.0,IllegalArgumentException:Can not set float field demo.Holder.f to null value
d:IllegalArgumentException:Can not set double field demo.Holder.d to java.lang.Boolean,5.0,-7.0,113.0,42.0,9.0,IllegalArgumentException:Can not set double field demo.Holder.d to null value
boxed:IllegalArgumentException:Can not set java.lang.Integer field demo.Holder.boxed to java.lang.Boolean,IllegalArgumentException:Can not set java.lang.Integer field demo.Holder.boxed to java.lang.Byte,IllegalArgumentException:Can not set java.lang.Integer field demo.Holder.boxed to java.lang.Short,IllegalArgumentException:Can not set java.lang.Integer field demo.Holder.boxed to java.lang.Character,42,IllegalArgumentException:Can not set java.lang.Integer field demo.Holder.boxed to java.lang.Long,null
any:true,5,-7,q,42,9,null
link:true,IllegalArgumentException:Can not set demo.Holder field demo.Holder.link to demo.Other"#,
    ),
    ("demo/Probe", "wideBoxes", r#"0.10000000149011612,0.1"#),
    (
        "demo/Probe",
        "statics",
        r#"11,11,13,14,12,1.234567890123E12,constant,77,init;"#,
    ),
    (
        "demo/Probe",
        "receivers",
        r#"NullPointerException:Cannot invoke "Object.getClass()" because "obj" is null|IllegalArgumentException:Can not get int field demo.Holder.i on demo.Other|IllegalArgumentException:Can not get int field demo.Holder.i on demo.Other|IllegalArgumentException:Attempt to get int field "demo.Holder.i" with illegal data type conversion to boolean|IllegalArgumentException:Can not set int field demo.Holder.i on demo.Other|NullPointerException:Cannot invoke "Object.getClass()" because "obj" is null|IllegalArgumentException:Can not set int field demo.Holder.i to demo.Other|NullPointerException:Cannot invoke "Object.getClass()" because "obj" is null"#,
    ),
    (
        "demo/Probe",
        "finals",
        r#"IllegalAccessException:Can not set final int field demo.Holder.fi to (int)70001|IllegalAccessException:Can not set final int field demo.Holder.fi to (int)-5|IllegalArgumentException:Can not set final int field demo.Holder.fi to (long)5000000001|IllegalAccessException:Can not set final int field demo.Holder.fi to java.lang.Integer|IllegalAccessException:Can not set final int field demo.Holder.fi to null value|IllegalAccessException:Can not set final int field demo.Holder.fi to java.lang.Long|IllegalArgumentException:Can not set final int field demo.Holder.fi to demo.Other|IllegalAccessException:Can not set static final int field demo.Holder.SFI to (int)70001|IllegalAccessException:Can not set static final int field demo.Holder.SFI to java.lang.Integer|3"#,
    ),
    ("demo/Holder", "own", r#"40,40,40"#),
    (
        "demo/Point",
        "setOwn",
        r#"1,1|IllegalAccessException:Can not set final int field demo.Point.x to (int)5"#,
    ),
];

fn fixture(test: &str) -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/Holder.java", HOLDER),
            ("demo/Other.java", OTHER),
            ("demo/Lazy.java", LAZY),
            ("demo/Point.java", POINT),
            ("demo/sub/Outside.java", OUTSIDE),
        ],
    )
}

/// Run one static `String <class>.<method>()` and return its text and the
/// host's unknown calls.
fn run(
    fixture: &javac_fixture::Fixture,
    class: &str,
    method: &str,
) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = match fixture
        .jar
        .invoke_method_with_args(class, method, "()Ljava/lang/String;", None, &[], &mut host)
        .unwrap_or_else(|error| panic!("{method} failed: {error}"))
    {
        Value::Ref(Some(text)) => text,
        other => panic!("{method} did not return a string: {other:?}"),
    };
    (text, host.unknown_host_calls())
}

/// Every oracle block, answered with no unknown host call left.
#[test]
fn field_get_set_matches_the_jdk() {
    let fixture = fixture("field-get-set");
    for (class, method, text) in EXPECTED {
        let (answer, unknown) = run(&fixture, class, method);
        assert_eq!(answer, *text, "{class}.{method}");
        assert!(unknown.is_empty(), "{class}.{method} counted {unknown:?}");
    }
}

/// Counted, not answered: a private field read from another package is an
/// `IllegalAccessException` whose text names the modules when they are named
/// (NeoForge) and not otherwise (Fabric: `class demo.sub.Outside cannot access
/// a member of class demo.Holder with private access` on OpenJDK 27); the
/// runtime does not know which loader it runs under.
#[test]
fn failed_access_check_is_counted() {
    let fixture = fixture("field-get-set-access");
    let (_, unknown) = run(&fixture, "demo/sub/Outside", "privateRead");
    let names: Vec<&str> = unknown.iter().map(|(name, _)| name.as_str()).collect();
    assert!(
        names.contains(&"java/lang/reflect/Field#getInt(Ljava/lang/Object;)I"),
        "getInt not counted: {names:?}"
    );
}

/// Counted, not answered: after `setAccessible(true)` a final instance field
/// of a normal class is written through JDK 27's final-field mutation policy
/// (OpenJDK 27 with default flags writes it and prints a warning; with
/// `--illegal-final-field-mutation=deny` it throws).
#[test]
fn final_field_write_after_set_accessible_is_counted() {
    let fixture = fixture("field-get-set-final");
    let (_, unknown) = run(&fixture, "demo/Probe", "finalTouched");
    let names: Vec<&str> = unknown.iter().map(|(name, _)| name.as_str()).collect();
    assert!(
        names.contains(&"java/lang/reflect/Field#setInt(Ljava/lang/Object;I)V"),
        "setInt not counted: {names:?}"
    );
}
