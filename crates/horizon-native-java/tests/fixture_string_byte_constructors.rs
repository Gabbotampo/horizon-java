//! `new String()`, `new String(String)` and the `byte[]` constructors
//! (`string_constructors.rs`).
//!
//! `fixtures/string_bytes/Probe.java` printed the string below on JDK 25
//! (OpenJDK 25, `java -cp out demo.Probe`): UTF-8 by default, ISO-8859-1
//! and US-ASCII by `Charset` or by name, the range check, and the null checks
//! in JDK 25's order (the charset before the array).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/string_bytes/Probe.java");

#[test]
fn byte_constructors_decode_like_jdk_25() {
    let fixture = Fixture::build("string-bytes", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "caf+233|af+233|c+233|c+65533|caf|c+233|\
         StringIndexOutOfBoundsException:Range [3, 3 + 5) out of bounds for length 5|\
         NullPointerException:Cannot read the array length because \"bytes\" is null|\
         NullPointerException:Cannot read the array length because \"bytes\" is null|\
         NullPointerException:null|NullPointerException:null|NullPointerException:null|\
         NullPointerException:Cannot read field \"value\" because \"original\" is null||copy"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

const COUNTED: &str = r#"package demo;

public final class Probe {
    public static String run() throws Exception {
        byte[] bytes = {104, 105};
        String out = new String(bytes, "UTF-16");
        try {
            out += new String(bytes, "default");
        } catch (java.io.UnsupportedEncodingException e) {
            out += "!";
        }
        return out;
    }
}
"#;

/// A charset the host does not decode exactly (UTF-16), and a name JDK 25
/// does not know (`default`, which throws `UnsupportedEncodingException`
/// there), are counted, not answered.
#[test]
fn other_charsets_stay_counted() {
    let fixture = Fixture::build("string-bytes-counted", &[("demo/Probe.java", COUNTED)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture.text_on(&mut host, "run");
    let counted = host.unknown_host_calls();
    assert!(
        counted.iter().any(|(call, count)| call
            == "java/lang/String#<init>([BLjava/lang/String;)V"
            && *count == 2),
        "{counted:?}"
    );
}
