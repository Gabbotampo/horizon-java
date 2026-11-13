//! `new String(char[])`, `new String(char[], int, int)` and
//! `new String(int[], int, int)` (`string_constructors.rs`).
//!
//! `fixtures/string_chars/Probe.java` printed the string below on JDK 25
//! (`jdk-25.0.5+8`): the units are copied (a later write to the array leaves
//! the string alone), a surrogate pair stays one code point, and the range,
//! null and invalid-code-point checks throw JDK 25's exceptions.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/string_chars/Probe.java");

#[test]
fn char_and_code_point_constructors_build_the_string() {
    let fixture = Fixture::build("string-chars", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "key55357+56832!,6,true|ey,2,true|\
         StringIndexOutOfBoundsException:Range [4, 4 + 5) out of bounds for length 6|\
         StringIndexOutOfBoundsException:Range [-1, -1 + 2) out of bounds for length 6|\
         StringIndexOutOfBoundsException:Range [2, 2 + -1) out of bounds for length 6|\
         NullPointerException:Cannot read the array length because \"value\" is null|\
         NullPointerException:Cannot read the array length because \"value\" is null|\
         A55357+56832b,4,2|\
         StringIndexOutOfBoundsException:Range [2, 2 + 2) out of bounds for length 3|\
         IllegalArgumentException:-1|IllegalArgumentException:1114112|\
         NullPointerException:Cannot read the array length because \"codePoints\" is null"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

const LONE: &str = r#"package demo;

public final class Probe {
    public static String run() {
        return new String(new char[] {'a', '\uD800'}) + "|" + new String(new byte[] {104, 105});
    }
}
"#;

/// An unpaired surrogate (which the runtime's strings cannot hold) and the
/// `byte[]` constructors (Gabbotampo's #6) stay counted.
#[test]
fn unpaired_surrogates_and_byte_constructors_stay_counted() {
    let fixture = Fixture::build("string-chars-counted", &[("demo/Probe.java", LONE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture.text_on(&mut host, "run");
    let counted = host.unknown_host_calls();
    assert!(
        counted
            .iter()
            .any(|(call, _)| call == "java/lang/String#<init>([C)V"),
        "{counted:?}"
    );
    assert!(
        counted
            .iter()
            .any(|(call, _)| call == "java/lang/String#<init>([B)V"),
        "{counted:?}"
    );
}
