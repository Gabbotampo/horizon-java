//! `Class.forName` initializes the class it loads, as JDK 25 does.
//!
//! `fixtures/forname/Probe.java` printed
//! `after-lazy;eager;after-eager;eiie;ncdfe;lazy;` on JDK 25
//! (`jdk-25.0.5+8`): `forName(name, false, loader)` leaves the class
//! uninitialized, `forName(name)` runs its static initializer once, a failing
//! initializer surfaces as `ExceptionInInitializerError` and the next attempt
//! as `NoClassDefFoundError`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/forname/Probe.java");

#[test]
fn for_name_initializes_unless_told_not_to() {
    let fixture = Fixture::build("forname-init", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "run",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    assert_eq!(
        answer,
        Value::Ref(Some("after-lazy;eager;after-eager;eiie;ncdfe;lazy;".into()))
    );
}
