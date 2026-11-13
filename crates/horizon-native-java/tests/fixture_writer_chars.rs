//! `String.codePoints().forEach` (`string_code_points.rs`) and the writers'
//! `write(char[])`/`write(char[], int, int)` into the file model.
//!
//! `fixtures/writer_chars/Probe.java`, run in an empty directory, printed
//! `97,128512,98,|key = "value"\n#\n|3|uftruetruefalse|1255357,56832|Not a
//! valid Unicode code point: 0xFFFFFFFF` on JDK 25 (`jdk-25.0.5+8`): a
//! supplementary character is one code point, a `BufferedWriter` from
//! `Files.newBufferedWriter` and one over `OutputStreamWriter(Files
//! .newOutputStream(..))` write the units in range, and a surrogate pair
//! written as two units reads back as one character (two UTF-16 units); the
//! two `identity()`s answer their argument, each is one object, and they
//! differ; `Character.toChars` gives one unit, a surrogate pair, or throws.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/writer_chars/Probe.java");

#[test]
fn code_points_and_char_array_writes_answer_like_jdk_25() {
    let fixture = Fixture::build("writer-chars", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "97,128512,98,|key = \"value\"\n#\n|3|uftruetruefalse|1255357,56832|\
         Not a valid Unicode code point: 0xFFFFFFFF"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
