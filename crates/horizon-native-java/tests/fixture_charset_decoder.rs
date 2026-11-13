//! `Charset.newDecoder()` for UTF-8 and its error actions (`charset_decoder.rs`).
//!
//! `fixtures/charset_decoder/Probe.java`, run from a jar holding
//! `settings.toml` (`answer = 42\n`), printed
//! `REPORT,REPORT,65533,1.0|true,IGNORE,REPLACE|iae:Null action,iae:Empty replacement,iae:Replacement too long,ok,?|answer = 42`
//! on JDK 25 (`jdk-25.0.5+8`): a new decoder's defaults, the builders
//! returning `this`, their argument checks, and a strict decoder reading the
//! resource through `InputStreamReader`, as night-config's parser does.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/charset_decoder/Probe.java");

#[test]
fn utf8_decoders_answer_like_jdk_25() {
    let fixture = Fixture::build_with_lib(
        "charset-decoder",
        &[("demo/Probe.java", PROBE)],
        &[("settings.toml", b"answer = 42\n")],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "REPORT,REPORT,65533,1.0|true,IGNORE,REPLACE|iae:Null action,iae:Empty replacement,\
         iae:Replacement too long,ok,?|answer = 42"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
