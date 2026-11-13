//! `System.arraycopy` (`array_copy.rs`).
//!
//! `fixtures/array_copy/Probe.java` printed the string below on JDK 25
//! (`jdk-25.0.5+8`): copies into an offset, overlapping copies in both
//! directions within one array, `int[]` and `String[]` → `Object[]`, then
//! HotSpot's exceptions in its check order, and a copy into an
//! `Arrays.copyOf` copy (a `char[]` too).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/array_copy/Probe.java");

#[test]
fn arraycopy_answers_like_hotspot() {
    let fixture = Fixture::build("array-copy", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "abc00|11234,23455|230|nullxy|\
         ArrayIndexOutOfBoundsException:arraycopy: length -1 is negative|\
         ArrayIndexOutOfBoundsException:arraycopy: source index -1 out of bounds for char[4]|\
         ArrayIndexOutOfBoundsException:arraycopy: destination index -1 out of bounds for char[6]|\
         ArrayIndexOutOfBoundsException:arraycopy: last source index 5 out of bounds for char[4]|\
         ArrayIndexOutOfBoundsException:arraycopy: last destination index 7 out of bounds for char[6]|\
         ArrayIndexOutOfBoundsException:arraycopy: last destination index 4 out of bounds for object array[3]|\
         NullPointerException:null|NullPointerException:null|\
         ArrayStoreException:arraycopy: type mismatch: can not copy char[] into int[]|\
         ArrayStoreException:arraycopy: source type java.lang.String is not an array|\
         ArrayStoreException:arraycopy: type mismatch: can not copy char[] into object array[]|ok|\
         ababcd6"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
