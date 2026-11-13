//! Gson's JSON tree (`JsonObject`, `JsonArray`, `JsonPrimitive`, `JsonNull`,
//! their conversions, equality, copies and text, and `Gson.toJson` of a tree, plus
//! `JsonParser` and `Gson.fromJson` into the tree classes), compiled from
//! real Java at test time.
//!
//! The probe below is compiled against signature-only copies of the
//! gson-2.14.0 API it calls (`STUBS`), and only `demo/Probe` goes into the
//! fixture jar, so every Gson call reaches the runtime's model. Compiled
//! against the real `gson-2.14.0.jar` instead, the probe's bytecode is the
//! same instruction for instruction (`javap -c -p`, constant-pool indices
//! aside), so the stubs do not change what the runtime is asked.
//!
//! Every expected value is the output of that same probe run on OpenJDK 27
//! with the real gson-2.14.0 jar (the version the 26.3 game, its datagen and
//! Fabric Loader ship), pasted verbatim. `counted` is the exception: there the
//! real jar's answer is quoted in the test and the runtime must count the
//! call instead of answering.

use horizon_native_java::{NativeError, NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

/// Signature-only copies of the gson-2.14.0 members the probe calls. The
/// bodies never run: the classes stay out of the fixture jar.
const STUBS: &[(&str, &str)] = &[
    (
        "com/google/gson/JsonElement.java",
        r#"package com.google.gson;

public abstract class JsonElement {
    public abstract JsonElement deepCopy();
    public boolean isJsonArray() { return false; }
    public boolean isJsonObject() { return false; }
    public boolean isJsonPrimitive() { return false; }
    public boolean isJsonNull() { return false; }
    public JsonObject getAsJsonObject() { return null; }
    public JsonArray getAsJsonArray() { return null; }
    public JsonPrimitive getAsJsonPrimitive() { return null; }
    public JsonNull getAsJsonNull() { return null; }
    public boolean getAsBoolean() { return false; }
    public Number getAsNumber() { return null; }
    public String getAsString() { return null; }
    public double getAsDouble() { return 0; }
    public float getAsFloat() { return 0; }
    public long getAsLong() { return 0; }
    public int getAsInt() { return 0; }
    public byte getAsByte() { return 0; }
    public char getAsCharacter() { return 0; }
    public short getAsShort() { return 0; }
    @Override public String toString() { return null; }
}
"#,
    ),
    (
        "com/google/gson/JsonObject.java",
        r#"package com.google.gson;

import java.util.Map;
import java.util.Set;

public final class JsonObject extends JsonElement {
    public JsonObject() {}
    @Override public JsonObject deepCopy() { return null; }
    public void add(String property, JsonElement value) {}
    public JsonElement remove(String property) { return null; }
    public void addProperty(String property, String value) {}
    public void addProperty(String property, Number value) {}
    public void addProperty(String property, Boolean value) {}
    public void addProperty(String property, Character value) {}
    public Set<Map.Entry<String, JsonElement>> entrySet() { return null; }
    public Set<String> keySet() { return null; }
    public int size() { return 0; }
    public boolean isEmpty() { return false; }
    public boolean has(String memberName) { return false; }
    public JsonElement get(String memberName) { return null; }
    public JsonPrimitive getAsJsonPrimitive(String memberName) { return null; }
    public JsonArray getAsJsonArray(String memberName) { return null; }
    public JsonObject getAsJsonObject(String memberName) { return null; }
    public Map<String, JsonElement> asMap() { return null; }
    @Override public boolean equals(Object o) { return false; }
    @Override public int hashCode() { return 0; }
}
"#,
    ),
    (
        "com/google/gson/JsonArray.java",
        r#"package com.google.gson;

import java.util.Iterator;

public final class JsonArray extends JsonElement implements Iterable<JsonElement> {
    public JsonArray() {}
    public JsonArray(int capacity) {}
    @Override public JsonArray deepCopy() { return null; }
    public void add(Boolean bool) {}
    public void add(Character character) {}
    public void add(Number number) {}
    public void add(String string) {}
    public void add(JsonElement element) {}
    public void addAll(JsonArray array) {}
    public JsonElement set(int index, JsonElement element) { return null; }
    public boolean remove(JsonElement element) { return false; }
    public JsonElement remove(int index) { return null; }
    public boolean contains(JsonElement element) { return false; }
    public int size() { return 0; }
    public boolean isEmpty() { return false; }
    @Override public Iterator<JsonElement> iterator() { return null; }
    public JsonElement get(int i) { return null; }
    @Override public boolean equals(Object o) { return false; }
    @Override public int hashCode() { return 0; }
}
"#,
    ),
    (
        "com/google/gson/JsonPrimitive.java",
        r#"package com.google.gson;

public final class JsonPrimitive extends JsonElement {
    public JsonPrimitive(Boolean bool) {}
    public JsonPrimitive(Number number) {}
    public JsonPrimitive(String string) {}
    public JsonPrimitive(Character c) {}
    @Override public JsonPrimitive deepCopy() { return null; }
    public boolean isBoolean() { return false; }
    public boolean isNumber() { return false; }
    public boolean isString() { return false; }
    @Override public boolean equals(Object o) { return false; }
    @Override public int hashCode() { return 0; }
}
"#,
    ),
    (
        "com/google/gson/JsonNull.java",
        r#"package com.google.gson;

public final class JsonNull extends JsonElement {
    public static final JsonNull INSTANCE = null;
    @Deprecated public JsonNull() {}
    @Override public JsonNull deepCopy() { return null; }
    @Override public boolean equals(Object o) { return false; }
    @Override public int hashCode() { return 0; }
}
"#,
    ),
    (
        "com/google/gson/JsonParser.java",
        r#"package com.google.gson;

import java.io.Reader;

public final class JsonParser {
    public static JsonElement parseString(String json) { return null; }
    public static JsonElement parseReader(Reader reader) { return null; }
}
"#,
    ),
    (
        "com/google/gson/Gson.java",
        r#"package com.google.gson;

import java.io.Reader;

public final class Gson {
    public Gson() {}
    public String toJson(Object src) { return null; }
    public String toJson(JsonElement jsonElement) { return null; }
    public <T> T fromJson(String json, Class<T> classOfT) { return null; }
    public <T> T fromJson(Reader json, Class<T> classOfT) { return null; }
}
"#,
    ),
    (
        "com/google/gson/GsonBuilder.java",
        r#"package com.google.gson;

public final class GsonBuilder {
    public GsonBuilder() {}
    public GsonBuilder serializeNulls() { return this; }
    public GsonBuilder setPrettyPrinting() { return this; }
    public GsonBuilder setLenient() { return this; }
    public GsonBuilder disableHtmlEscaping() { return this; }
    public Gson create() { return null; }
}
"#,
    ),
];

const PROBE: &str = r#"package demo;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.io.StringReader;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

public class Probe {
    static String fail(Throwable t) {
        return t.getClass().getName() + ": " + t.getMessage();
    }

    /** add/addProperty with every value kind, nulls, nesting, and toString. */
    public static String build() {
        JsonObject o = new JsonObject();
        o.addProperty("s", "a<b>&c='d'\"e\\f\n\t \u0001\u007fé");
        o.addProperty("i", Integer.valueOf(42));
        o.addProperty("l", Long.valueOf(-9000000000L));
        o.addProperty("d", Double.valueOf(2.5));
        o.addProperty("dd", Double.valueOf(1.0E20));
        o.addProperty("b", Boolean.TRUE);
        o.addProperty("c", Character.valueOf('x'));
        o.addProperty("ns", (String) null);
        o.addProperty("nn", (Number) null);
        o.addProperty("nb", (Boolean) null);
        o.add("ne", null);
        o.add("jn", JsonNull.INSTANCE);
        JsonArray a = new JsonArray();
        a.add("x");
        a.add(Integer.valueOf(1));
        a.add(Boolean.FALSE);
        a.add(Character.valueOf('y'));
        a.add((String) null);
        a.add((JsonElement) null);
        a.add(new JsonObject());
        a.add(new JsonArray());
        o.add("a", a);
        JsonObject inner = new JsonObject();
        inner.addProperty("k", "v");
        o.add("o", inner);
        return o.toString() + "\n" + a.size() + "|" + o.size() + "|" + o.get("a").getAsJsonArray().get(4);
    }

    /** Insertion order, replacement in place, removal, views. */
    public static String order() {
        JsonObject o = new JsonObject();
        o.addProperty("z", "1");
        o.addProperty("a", "2");
        o.addProperty("m", "3");
        o.addProperty("z", "4");
        JsonElement removed = o.remove("a");
        JsonElement missing = o.remove("nope");
        o.addProperty("a", "5");
        StringBuilder sb = new StringBuilder();
        for (String k : o.keySet()) sb.append(k).append(',');
        sb.append('|');
        for (Map.Entry<String, JsonElement> e : o.entrySet()) sb.append(e.getKey()).append('=').append(e.getValue().getAsString()).append(',');
        sb.append('|');
        for (Map.Entry<String, JsonElement> e : o.asMap().entrySet()) sb.append(e.getKey()).append(',');
        return sb + "|" + o + "|" + removed + "|" + missing + "|" + o.has("z") + o.has("q") + "|" + o.get("q")
            + "|" + o.size() + o.isEmpty() + new JsonObject().isEmpty() + "|" + o.asMap().size() + o.asMap().get("m");
    }

    /** The getAs conversions on each primitive kind. */
    public static String access() {
        JsonPrimitive num = new JsonPrimitive(Integer.valueOf(7));
        JsonPrimitive dbl = new JsonPrimitive(Double.valueOf(7.9));
        JsonPrimitive str = new JsonPrimitive("12");
        JsonPrimitive tru = new JsonPrimitive("TRUE");
        JsonPrimitive bool = new JsonPrimitive(Boolean.TRUE);
        JsonPrimitive ch = new JsonPrimitive(Character.valueOf('q'));
        JsonPrimitive lng = new JsonPrimitive(Long.valueOf(5000000000L));
        return num.getAsInt() + "," + num.getAsString() + "," + num.getAsDouble() + "," + num.getAsLong() + "," + num.getAsBoolean() + "," + num.getAsNumber()
            + "|" + dbl.getAsInt() + "," + dbl.getAsString() + "," + dbl.getAsFloat() + "," + dbl.getAsLong() + "," + dbl.getAsShort() + "," + dbl.getAsByte()
            + "|" + str.getAsInt() + "," + str.getAsLong() + "," + str.getAsDouble() + "," + str.getAsString() + "," + str.getAsNumber() + "," + str.getAsBoolean()
            + "|" + tru.getAsBoolean() + "," + bool.getAsBoolean() + "," + bool.getAsString()
            + "|" + ch.getAsString() + "," + ch.getAsCharacter() + "," + ch.isString()
            + "|" + lng.getAsInt() + "," + lng.getAsLong() + "," + lng.getAsString()
            + "|" + num.isNumber() + num.isString() + num.isBoolean() + bool.isBoolean() + str.isString();
    }

    /** The exceptions and their messages. */
    public static String errors() {
        List<String> out = new ArrayList<>();
        JsonObject o = new JsonObject();
        o.addProperty("k", Integer.valueOf(5));
        JsonArray two = new JsonArray();
        two.add("a");
        two.add("b");
        try { o.getAsInt(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { JsonNull.INSTANCE.getAsString(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { o.get("k").getAsJsonObject(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { o.getAsJsonArray(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { two.getAsJsonPrimitive(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { JsonNull.INSTANCE.getAsJsonObject(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { new JsonPrimitive("abc").getAsInt(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { new JsonPrimitive("99999999999").getAsInt(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { two.get(5); } catch (RuntimeException e) { out.add(fail(e)); }
        try { two.get(-1); } catch (RuntimeException e) { out.add(fail(e)); }
        try { two.getAsString(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { new JsonArray().getAsInt(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { new JsonPrimitive("").getAsCharacter(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { new JsonPrimitive(Boolean.TRUE).getAsNumber(); } catch (RuntimeException e) { out.add(fail(e)); }
        try { o.add(null, new JsonObject()); } catch (RuntimeException e) { out.add(fail(e)); }
        try { o.asMap().put("x", null); } catch (RuntimeException e) { out.add(fail(e)); }
        try { o.asMap().put(null, new JsonObject()); } catch (RuntimeException e) { out.add(fail(e)); }
        try { new JsonPrimitive((String) null); } catch (RuntimeException e) { out.add(fail(e)); }
        try { two.set(2, null); } catch (RuntimeException e) { out.add(fail(e)); }
        try { two.remove(3); } catch (RuntimeException e) { out.add(fail(e)); }
        try { new JsonArray(-1); } catch (RuntimeException e) { out.add(fail(e)); }
        JsonArray one = new JsonArray();
        one.add(Integer.valueOf(3));
        out.add(one.getAsInt() + "," + one.getAsString() + "," + one.getAsBoolean());
        out.add(String.valueOf(o.size()));
        return String.join("\n", out);
    }

    /** equals and hashCode, including the numeric rules. */
    public static String equality() {
        JsonPrimitive i1 = new JsonPrimitive(Integer.valueOf(1));
        JsonPrimitive l1 = new JsonPrimitive(Long.valueOf(1L));
        JsonPrimitive d1 = new JsonPrimitive(Double.valueOf(1.0));
        JsonPrimitive s1 = new JsonPrimitive("1");
        // `getAsNumber()` of a string primitive is a `LazilyParsedNumber`, the
        // box a parsed number has.
        JsonPrimitive p1 = new JsonPrimitive(new JsonPrimitive("1").getAsNumber());
        JsonPrimitive p10 = new JsonPrimitive(new JsonPrimitive("1.0").getAsNumber());
        JsonPrimitive nan1 = new JsonPrimitive(Double.valueOf(Double.NaN));
        JsonPrimitive nan2 = new JsonPrimitive(Double.valueOf(Double.NaN));
        JsonPrimitive t = new JsonPrimitive(Boolean.TRUE);
        JsonPrimitive ts = new JsonPrimitive("true");
        JsonObject a = new JsonObject();
        a.addProperty("x", Integer.valueOf(1));
        a.addProperty("y", "z");
        JsonObject b = new JsonObject();
        b.addProperty("y", "z");
        b.addProperty("x", Long.valueOf(1L));
        JsonArray arr1 = new JsonArray();
        arr1.add("p");
        arr1.add(Integer.valueOf(2));
        JsonArray arr2 = new JsonArray();
        arr2.add(Integer.valueOf(2));
        arr2.add("p");
        JsonArray arr3 = new JsonArray();
        arr3.add("p");
        arr3.add(Double.valueOf(2.0));
        return "" + i1.equals(l1) + i1.equals(d1) + i1.equals(s1) + i1.equals(p1) + p1.equals(p10) + nan1.equals(nan2) + t.equals(ts) + s1.equals(new JsonPrimitive("1"))
            + "|" + a.equals(b) + arr1.equals(arr2) + arr1.equals(arr3) + a.equals(arr1) + JsonNull.INSTANCE.equals(new JsonNull()) + i1.equals(null)
            + "|" + i1.hashCode() + "," + l1.hashCode() + "," + d1.hashCode() + "," + s1.hashCode() + "," + p1.hashCode() + "," + p10.hashCode() + "," + nan1.hashCode() + "," + t.hashCode()
            + "," + new JsonPrimitive(Long.valueOf(-5000000000L)).hashCode() + "," + new JsonPrimitive(Double.valueOf(-2.75)).hashCode()
            + "|" + a.hashCode() + "," + b.hashCode() + "," + arr1.hashCode() + "," + arr3.hashCode() + "," + new JsonObject().hashCode() + "," + new JsonArray().hashCode()
            + "|" + arr1.contains(new JsonPrimitive("p")) + arr1.contains(new JsonPrimitive(Double.valueOf(2.0))) + arr1.contains(new JsonPrimitive("q"))
            + "|" + arr1.remove(new JsonPrimitive(Long.valueOf(2L))) + arr1.remove(new JsonPrimitive("q")) + arr1;
    }

    /** deepCopy is independent; a primitive copies to itself. */
    public static String copies() {
        JsonObject o = new JsonObject();
        JsonArray a = new JsonArray();
        a.add("x");
        o.add("a", a);
        JsonPrimitive p = new JsonPrimitive("p");
        o.add("p", p);
        JsonObject copy = o.deepCopy();
        copy.get("a").getAsJsonArray().add("y");
        copy.addProperty("n", Integer.valueOf(1));
        JsonElement same = o.get("a");
        return o + "|" + copy + "|" + (copy.get("p") == p) + (copy.get("a") == same) + copy.equals(o) + o.deepCopy().equals(o)
            + (JsonNull.INSTANCE.deepCopy() == JsonNull.INSTANCE) + (p.deepCopy() == p);
    }

    /** JsonParser documents, numbers kept as parsed text, comments. */
    public static String parse() {
        String doc = "{\n  // a comment\n  \"n\": 5, \"neg\": -12, \"z\": -0, \"big\": 123456789012345678901234567890,\n"
            + "  \"dec\": 1.50, \"exp\": 1e3, \"dexp\": 2.5E-3, # hash comment\n  \"s\": \"\\u00e9\\/\\\"\\\\\\t\\ud83d\\ude00\", /* block */ \"t\": true, \"f\": false, \"nul\": null,\n"
            + "  \"arr\": [1, [2, []], {}], \"dup\": 1, \"dup\": 2\n}";
        JsonObject o = JsonParser.parseString(doc).getAsJsonObject();
        JsonElement n = o.get("n");
        return o.toString()
            + "|" + n.getAsInt() + "," + o.get("dec").getAsInt() + "," + o.get("exp").getAsInt() + "," + o.get("dec").getAsDouble() + "," + o.get("big").getAsDouble()
            + "," + o.get("exp").getAsLong() + "," + o.get("dexp").getAsFloat() + "," + o.get("z").getAsInt() + "," + o.get("neg").getAsLong() + "," + o.get("big").getAsString()
            + "|" + n.getAsNumber().getClass().getName() + "," + n.getAsJsonPrimitive().isNumber() + "," + o.get("s").getAsString().length() + "," + o.get("nul").isJsonNull()
            + "|" + JsonParser.parseString("") + "," + JsonParser.parseString(" \n ").isJsonNull() + "," + JsonParser.parseString("null").isJsonNull()
            + "," + JsonParser.parseString("[1,\"a\"]") + "," + JsonParser.parseString("\"top\"").getAsString() + "," + JsonParser.parseReader(new StringReader("{\"r\":1}"))
            + "|" + o.get("dup") + "," + o.keySet();
    }

    /** Gson#toJson over a tree with the builder's options. */
    public static String gsonWrite() {
        JsonObject o = new JsonObject();
        o.addProperty("h", "<a href='x'>&</a>");
        o.add("gone", JsonNull.INSTANCE);
        o.add("e", new JsonObject());
        o.add("ea", new JsonArray());
        JsonArray a = new JsonArray();
        a.add(Integer.valueOf(1));
        a.add(JsonNull.INSTANCE);
        JsonObject in = new JsonObject();
        in.addProperty("q", Double.valueOf(0.5));
        a.add(in);
        o.add("a", a);
        Gson plain = new Gson();
        Gson pretty = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
        Gson nulls = new GsonBuilder().serializeNulls().create();
        Object asObject = o;
        return plain.toJson(o) + "\n" + pretty.toJson(o) + "\n" + nulls.toJson(o) + "\n" + plain.toJson(asObject)
            + "\n" + plain.toJson(JsonNull.INSTANCE) + "\n" + pretty.toJson(new JsonPrimitive("x'y")) + "\n" + o;
    }

    /** Gson#fromJson into the tree classes. */
    public static String gsonRead() {
        Gson gson = new GsonBuilder().setLenient().create();
        JsonObject o = gson.fromJson("{\"a\": [1, 2.0, \"x\"], \"b\": {\"c\": null}} // tail comment", JsonObject.class);
        JsonObject fromReader = gson.fromJson(new StringReader("{\"r\": true}"), JsonObject.class);
        JsonObject empty = gson.fromJson("", JsonObject.class);
        JsonElement el = new Gson().fromJson("[3]", JsonElement.class);
        JsonArray arr = new Gson().fromJson("[4, 5]", JsonArray.class);
        return o + "|" + o.get("a").getAsJsonArray().get(1).getAsNumber().getClass().getSimpleName() + "|" + fromReader + "|" + empty + "|" + el + "|" + arr.size();
    }

    /** Iteration: for-each over an array, forEach with a lambda, entrySet. */
    public static String iterate() {
        JsonArray a = new JsonArray();
        a.add("x");
        a.add("y");
        a.add(Integer.valueOf(3));
        StringBuilder sb = new StringBuilder();
        for (JsonElement e : a) sb.append(e.getAsString()).append(';');
        List<String> seen = new ArrayList<>();
        a.forEach(e -> seen.add(e.toString()));
        a.set(0, new JsonPrimitive("w"));
        JsonElement old = a.set(1, null);
        JsonElement gone = a.remove(2);
        return sb + "|" + seen + "|" + a + "|" + old + "|" + gone + "|" + a.isEmpty() + a.size();
    }

    /** Type predicates and the JsonElement casts. */
    public static String kinds() {
        JsonElement o = new JsonObject();
        JsonElement a = new JsonArray();
        JsonElement p = new JsonPrimitive("s");
        JsonElement n = JsonNull.INSTANCE;
        return "" + o.isJsonObject() + o.isJsonArray() + o.isJsonPrimitive() + o.isJsonNull()
            + a.isJsonArray() + p.isJsonPrimitive() + n.isJsonNull() + n.isJsonPrimitive()
            + (o instanceof JsonObject) + (a instanceof JsonElement) + (p instanceof JsonArray) + (n instanceof JsonElement)
            + "|" + o.getAsJsonObject().size() + a.getAsJsonArray().size() + p.getAsJsonPrimitive().getAsString() + n.getAsJsonNull()
            + "|" + String.valueOf(o) + String.valueOf(n) + p;
    }

    /** Number toString per boxed type. */
    public static String numbers() {
        return new JsonPrimitive(Double.valueOf(0.1)) + "," + new JsonPrimitive(Float.valueOf(1.5f)) + "," + new JsonPrimitive(Double.valueOf(100.0))
            + "," + new JsonPrimitive(Double.valueOf(1.0E-5)) + "," + new JsonPrimitive(Double.valueOf(Double.NaN)) + "," + new JsonPrimitive(Double.valueOf(Double.NEGATIVE_INFINITY))
            + "," + new JsonPrimitive(Short.valueOf((short) -3)) + "," + new JsonPrimitive(Long.valueOf(Long.MIN_VALUE));
    }

    /** What the model counts instead of answering. */
    public static String counted() {
        String ambiguous = new JsonPrimitive(Float.valueOf(0.1f)).toString();
        int identity = JsonNull.INSTANCE.hashCode();
        JsonElement unquoted = JsonParser.parseString("{a:1}");
        return ambiguous + "|" + (identity == identity) + "|" + unquoted;
    }

    public static void main(String[] args) {
        String[] names = {"build", "order", "access", "errors", "equality", "copies", "parse", "gsonWrite", "gsonRead", "iterate", "kinds", "numbers", "counted"};
        for (String name : names) {
            try {
                String v = (String) Probe.class.getMethod(name).invoke(null);
                System.out.println("=== " + name);
                System.out.println(v);
            } catch (Exception e) {
                System.out.println("=== " + name + " FAILED " + e.getCause());
            }
        }
    }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

struct Fixture {
    root: PathBuf,
    jar: NativeJar,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn build(test: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-gson-tree-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        // The API stubs compile into their own directory, which is only the
        // probe's class path.
        let api = fixture.root.join("api");
        let mut javac_api = Command::new(javac());
        javac_api.args(["--release", "17", "-d"]).arg(&api);
        for (path, source) in STUBS {
            let file = fixture.root.join("api-src").join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            javac_api.arg(file);
        }
        assert!(
            javac_api.status().unwrap().success(),
            "javac failed for the gson API stubs of {test}"
        );
        let classes = fixture.root.join("classes");
        let probe = fixture.root.join("src/demo/Probe.java");
        fs::create_dir_all(probe.parent().unwrap()).unwrap();
        fs::write(&probe, PROBE).unwrap();
        let status = Command::new(javac())
            .args(["--release", "17", "-nowarn", "-cp"])
            .arg(&api)
            .arg("-d")
            .arg(&classes)
            .arg(&probe)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "javac failed for gson tree fixture {test}"
        );

        let jar_path = fixture.root.join("gson-tree.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"gson-tree-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
        )
        .unwrap();
        for class in files_under(&classes) {
            let entry = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();

        fixture.jar = NativeJar::from_path(&jar_path).unwrap();
        fixture
    }

    fn call(&self, method: &str, host: &mut StdHost) -> Result<Value, NativeError> {
        self.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            host,
        )
    }

    /// The probe method's text, and that the run counted nothing: an exact
    /// answer must not lean on an unknown call.
    fn exact(&self, method: &str) -> String {
        let mut host = StdHost::new();
        let text = match self.call(method, &mut host).unwrap() {
            Value::Ref(Some(text)) => text,
            other => panic!("{method} did not return a string: {other:?}"),
        };
        assert!(
            host.unknown_host_calls().is_empty(),
            "{method} counted unknown calls: {:?}",
            host.unknown_host_calls()
        );
        text
    }
}

/// Add/addProperty for every value kind, null → JsonNull, insertion order kept when a key is replaced, removal, the key/entry views and `asMap()`.
#[test]
fn objects_and_arrays_build_and_print_like_gson() {
    let fixture = Fixture::build("build");
    assert_eq!(
        fixture.exact("build"),
        "{\"s\":\"a<b>&c='d'\\\"e\\\\f\\n\\t\\u2028\\u0001\u{7f}\u{e9}\",\"i\":42,\"l\":-9000000000,\"d\":2.5,\"dd\":1.0E20,\"b\":true,\"c\":\"x\",\"ns\":null,\"nn\":null,\"nb\":null,\"ne\":null,\"jn\":null,\"a\":[\"x\",1,false,\"y\",null,null,{},[]],\"o\":{\"k\":\"v\"}}\n8|14|null"
    );
    assert_eq!(
        fixture.exact("order"),
        "z,m,a,|z=4,m=3,a=5,|z,m,a,|{\"z\":\"4\",\"m\":\"3\",\"a\":\"5\"}|\"2\"|null|truefalse|null|3falsetrue|3\"3\""
    );
}

/// The getAs conversions per primitive kind, and every exception with its message.
#[test]
fn conversions_and_their_exceptions_are_gsons() {
    let fixture = Fixture::build("access");
    assert_eq!(
        fixture.exact("access"),
        "7,7,7.0,7,false,7|7,7.9,7.9,7,7,7|12,12,12.0,12,12,false|true,true,true|q,q,true|705032704,5000000000,5000000000|truefalsefalsetruetrue"
    );
    assert_eq!(
        fixture.exact("errors"),
        "java.lang.UnsupportedOperationException: JsonObject\njava.lang.UnsupportedOperationException: JsonNull\njava.lang.IllegalStateException: Not a JSON Object: 5\njava.lang.IllegalStateException: Not a JSON Array: {\"k\":5}\njava.lang.IllegalStateException: Not a JSON Primitive: [\"a\",\"b\"]\njava.lang.IllegalStateException: Not a JSON Object: null\njava.lang.NumberFormatException: For input string: \"abc\"\njava.lang.NumberFormatException: For input string: \"99999999999\"\njava.lang.IndexOutOfBoundsException: Index 5 out of bounds for length 2\njava.lang.IndexOutOfBoundsException: Index -1 out of bounds for length 2\njava.lang.IllegalStateException: Array must have size 1, but has size 2\njava.lang.IllegalStateException: Array must have size 1, but has size 0\njava.lang.UnsupportedOperationException: String value is empty\njava.lang.UnsupportedOperationException: Primitive is neither a number nor a string\njava.lang.NullPointerException: key == null\njava.lang.NullPointerException: value == null\njava.lang.NullPointerException: key == null\njava.lang.NullPointerException: null\njava.lang.IndexOutOfBoundsException: Index 2 out of bounds for length 2\njava.lang.IndexOutOfBoundsException: Index 3 out of bounds for length 2\njava.lang.IllegalArgumentException: Illegal Capacity: -1\n3,3,false\n1"
    );
}

/// JsonPrimitive's numeric equality (a `LazilyParsedNumber` included), the map/list equality of objects and arrays, their hash codes, `contains`/`remove` by equality, `deepCopy`.
#[test]
fn equality_hashing_and_copies_follow_gsons_rules() {
    let fixture = Fixture::build("equality");
    assert_eq!(
        fixture.exact("equality"),
        "truetruefalsetruetruetruefalsetrue|truefalsetruefalsetruefalse|1,1,1072693248,49,1072693248,1072693248,2146959360,1231,705032702,-1073348608|124,124,4435,1073746257,0,1|truetruefalse|truefalse[\"p\"]"
    );
    assert_eq!(
        fixture.exact("copies"),
        "{\"a\":[\"x\"],\"p\":\"p\"}|{\"a\":[\"x\",\"y\"],\"p\":\"p\",\"n\":1}|truefalsefalsetruetruetrue"
    );
}

/// Gson#toJson of a tree: HTML escaping, pretty printing, JsonNull members dropped unless serializeNulls.
#[test]
fn gson_writes_trees_with_its_options() {
    let fixture = Fixture::build("gsonWrite");
    assert_eq!(
        fixture.exact("gsonWrite"),
        "{\"h\":\"\\u003ca href\\u003d\\u0027x\\u0027\\u003e\\u0026\\u003c/a\\u003e\",\"e\":{},\"ea\":[],\"a\":[1,null,{\"q\":0.5}]}\n{\n  \"h\": \"<a href='x'>&</a>\",\n  \"e\": {},\n  \"ea\": [],\n  \"a\": [\n    1,\n    null,\n    {\n      \"q\": 0.5\n    }\n  ]\n}\n{\"h\":\"\\u003ca href\\u003d\\u0027x\\u0027\\u003e\\u0026\\u003c/a\\u003e\",\"gone\":null,\"e\":{},\"ea\":[],\"a\":[1,null,{\"q\":0.5}]}\n{\"h\":\"\\u003ca href\\u003d\\u0027x\\u0027\\u003e\\u0026\\u003c/a\\u003e\",\"e\":{},\"ea\":[],\"a\":[1,null,{\"q\":0.5}]}\nnull\n\"x'y\"\n{\"h\":\"<a href='x'>&</a>\",\"gone\":null,\"e\":{},\"ea\":[],\"a\":[1,null,{\"q\":0.5}]}"
    );
}

/// IsJson*/instanceof, the JsonElement casts, String.valueOf, iteration and in-place edits, and number text per box.
#[test]
fn kinds_iteration_and_number_text_are_gsons() {
    let fixture = Fixture::build("kinds");
    assert_eq!(
        fixture.exact("kinds"),
        "truefalsefalsefalsetruetruetruefalsetruetruefalsetrue|00snull|{}null\"s\""
    );
    assert_eq!(
        fixture.exact("iterate"),
        "x;y;3;|[\"x\", \"y\", 3]|[\"w\",null]|\"y\"|3|false2"
    );
    assert_eq!(
        fixture.exact("numbers"),
        "0.1,1.5,100.0,1.0E-5,NaN,-Infinity,-3,-9223372036854775808"
    );
}

/// JsonParser with comments, escapes, parsed numbers kept as `LazilyParsedNumber` text and their conversions, duplicate names and empty documents.
#[test]
fn json_parser_reads_documents_and_keeps_number_text() {
    let fixture = Fixture::build("parse");
    assert_eq!(
        fixture.exact("parse"),
        "{\"n\":5,\"neg\":-12,\"z\":-0,\"big\":123456789012345678901234567890,\"dec\":1.50,\"exp\":1e3,\"dexp\":2.5E-3,\"s\":\"\u{e9}/\\\"\\\\\\t\u{1f600}\",\"t\":true,\"f\":false,\"nul\":null,\"arr\":[1,[2,[]],{}],\"dup\":2}|5,1,1000,1.5,1.2345678901234568E29,1000,0.0025,0,-12,123456789012345678901234567890|com.google.gson.internal.LazilyParsedNumber,true,7,true|null,true,true,[1,\"a\"],top,{\"r\":1}|2,[n, neg, z, big, dec, exp, dexp, s, t, f, nul, arr, dup]"
    );
}

/// Gson#fromJson into JsonObject, JsonElement and JsonArray, from a string and a reader, and an empty document.
#[test]
fn gson_reads_text_into_tree_classes() {
    let fixture = Fixture::build("gsonRead");
    assert_eq!(
        fixture.exact("gsonRead"),
        "{\"a\":[1,2.0,\"x\"],\"b\":{\"c\":null}}|LazilyParsedNumber|{\"r\":true}|null|[3]|2"
    );
}

/// What the model does not answer is counted, never approximated. On the
/// real jar the probe prints `0.1|true|{"a":1}`: `JsonNull.hashCode()` is an
/// identity hash, and `{a:1}` uses the lenient unquoted name the parser does
/// not reproduce. A boxed `0.1f` is a `Float` the runtime keeps apart from a
/// `Double` (`Boxed::Float`), so its text `0.1` is answered.
#[test]
fn what_the_model_cannot_answer_is_counted() {
    let fixture = Fixture::build("counted");
    let mut host = StdHost::new();
    let answer = fixture.call("counted", &mut host).unwrap();
    let counted = host
        .unknown_host_calls()
        .iter()
        .map(|(call, _)| call.clone())
        .collect::<Vec<_>>();
    assert!(
        matches!(&answer, Value::Ref(Some(text)) if text.starts_with("0.1|")),
        "a Float's JSON text is its own: {answer:?}"
    );
    for call in [
        "com/google/gson/JsonNull#hashCode()I",
        "com/google/gson/JsonParser#parseString(Ljava/lang/String;)Lcom/google/gson/JsonElement;",
    ] {
        assert!(
            counted.iter().any(|recorded| recorded == call),
            "{call} was not counted: {counted:?}"
        );
    }
}
