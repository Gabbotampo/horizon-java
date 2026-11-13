//! Host handles against a second review's Java 25 programs (#162).
//!
//! `fixtures/host_values/Rv*.java` are the reviewer's probes, unchanged; each
//! case is one `static String` method, and the expected strings are what
//! OpenJDK 25.0.5 printed for it (`javac --release 21`, two runs alike).
//! Each case is one of:
//!
//! * `Exact`: the runtime gives Java's answer and counts nothing;
//! * `Counted`: Java's answer needs something no run can reproduce (an
//!   identity hash, the order of a `Set.of`) or the runtime does not model
//!   yet, and the run says so by counting an unknown call;
//! * `Gap`: a named departure that is still silent, listed with its plan
//!   (not asserted, so it cannot pass for an answer).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;

/// What the runtime must do for one probe.
enum Outcome {
    Exact(&'static str),
    Counted,
    #[allow(dead_code)]
    Gap(&'static str),
}
use Outcome::*;

const CASES: &[(&str, &str, Outcome)] = &[
    (
        "Rv",
        "a1TreeMap",
        Exact(
            "a,b|3;b,\\c|4;hopt:ra|7;a,b2{Optional[a,b]=2, Optional[b,\\c]=1, Optional[hopt:ra]=3}",
        ),
    ),
    (
        "Rv",
        "a1LinkedHashMap",
        Exact(
            "a,b=Optional[v\\,w];c\\=x,y;=Optional.empty;|{Optional[a,b]=Optional[v\\,w], Optional[c\\]=x,y, Optional[]=Optional.empty}|Optional[v\\,w]truetrue",
        ),
    ),
    (
        "Rv",
        "a1Nested3",
        Exact(
            "a\\,b,c\\\\|a\\,b,c\\\\|truetrue|[Optional[Optional[Optional[a\\,b,c\\\\]]], [Optional[Optional[Optional[a\\,b,c\\\\]]]]]|-1348532728,-1348532728",
        ),
    ),
    (
        "Rv",
        "a1Every",
        Exact("bad=0|17,17|Optional[:]Optional[hopt:rx]Optional[]"),
    ),
    (
        "Rv",
        "a1Entry",
        Exact("k,v|w\\|[Optional[k,v]=Optional[w\\]]"),
    ),
    ("Rv", "a1Array", Counted),
    ("Rv", "a1Capture", Exact("a,b\\!|z,a,b\\|q,ra,b\\")),
    ("Rv", "a1Stream", Exact("[a,b, c\\]|a,b-c\\|a,b")),
    ("Rv", "a2Double", Counted),
    (
        "Rv",
        "a2LongInt",
        Exact(
            "OptionalLong[-9223372036854775808]OptionalLong[9223372036854775807]OptionalLong[0]OptionalInt[-2147483648]OptionalInt[0]OptionalInt[-1]|-2147483648,-2147483648,0,0,-2147483648,0|falsefalsefalse|4|falsefalsefalsetrue|-9223372036854775808-2147483648",
        ),
    ),
    ("Rv", "a2Streams", Counted),
    (
        "Rv",
        "a3Map",
        Exact("1,2,3,3|3535,3535|trueOptional[Rec[s=r, n=1]]"),
    ),
    (
        "Rv",
        "a3Text",
        Gap("F5 (#162): a held jar object prints its marker in a collection"),
    ),
    (
        "Rv",
        "a3Collections",
        Gap("F7 (#162): equal keys are not equals for held collections"),
    ),
    ("Rv", "a3Identity", Counted),
    (
        "Rv",
        "b5Arrays",
        Gap("F6 (#162): a cast to a handle class passes"),
    ),
    (
        "Rv",
        "b5Jar",
        Gap("F6 (#162): a cast to a handle class passes"),
    ),
    (
        "Rv",
        "b5Others",
        Gap("F6 (#162): a cast to a handle class passes"),
    ),
    (
        "Rv",
        "b5Boxes",
        Exact(
            "class java.lang.Integer cannot be cast to class java.util.Optional (java.lang.Integer and java.util.Optional are in module java.base of loader 'bootstrap')|class java.lang.Long cannot be cast to class java.util.OptionalInt (java.lang.Long and java.util.OptionalInt are in module java.base of loader 'bootstrap')|class java.lang.Character cannot be cast to class java.util.OptionalLong (java.lang.Character and java.util.OptionalLong are in module java.base of loader 'bootstrap')|class java.lang.Double cannot be cast to class java.util.OptionalDouble (java.lang.Double and java.util.OptionalDouble are in module java.base of loader 'bootstrap')|class java.lang.Float cannot be cast to class java.util.Optional (java.lang.Float and java.util.Optional are in module java.base of loader 'bootstrap')|class java.lang.Boolean cannot be cast to class java.util.Optional (java.lang.Boolean and java.util.Optional are in module java.base of loader 'bootstrap')",
        ),
    ),
    (
        "Rv",
        "b5Ifaces",
        Exact(
            "class java.util.Optional cannot be cast to class java.lang.Runnable (java.util.Optional and java.lang.Runnable are in module java.base of loader 'bootstrap')|class java.util.Optional cannot be cast to class java.util.function.Supplier (java.util.Optional and java.util.function.Supplier are in module java.base of loader 'bootstrap')|class java.util.Optional cannot be cast to class java.io.Serializable (java.util.Optional and java.io.Serializable are in module java.base of loader 'bootstrap')|class java.util.OptionalInt cannot be cast to class java.lang.Iterable (java.util.OptionalInt and java.lang.Iterable are in module java.base of loader 'bootstrap')|class java.util.Optional cannot be cast to class java.lang.CharSequence (java.util.Optional and java.lang.CharSequence are in module java.base of loader 'bootstrap')|class java.util.OptionalInt cannot be cast to class java.lang.Comparable (java.util.OptionalInt and java.lang.Comparable are in module java.base of loader 'bootstrap')|class java.util.OptionalInt cannot be cast to class java.lang.Number (java.util.OptionalInt and java.lang.Number are in module java.base of loader 'bootstrap')",
        ),
    ),
    (
        "Rv",
        "b5Null",
        Gap("F6 (#162): a cast to a handle class passes"),
    ),
    ("Rv", "b5Spelled", Counted),
    ("Rv", "b6IdentityMapOps", Counted),
    ("Rv", "b6IdentityMapSame", Exact("2,2,4")),
    ("Rv", "b6IdentitySet", Counted),
    ("Rv", "b6IdHash", Counted),
    ("Rv", "idDifferentKeys", Exact("falsetruefalsefalse")),
    ("Rv", "idThroughList", Counted),
    ("Rv", "idOfIntBox", Counted),
    (
        "Rv2",
        "t1",
        Gap("F5 (#162): a held jar object prints its marker in a collection"),
    ),
    (
        "Rv2",
        "t2",
        Gap("F5 (#162): a held jar object prints its marker in a collection"),
    ),
    (
        "Rv2",
        "t3",
        Gap("F5 (#162): a held jar object prints its marker in a collection"),
    ),
    (
        "Rv2",
        "t4",
        Gap("F5 (#162): a held jar object prints its marker in a collection"),
    ),
    (
        "Rv2",
        "t5",
        Gap("F5 (#162): a held jar object prints its marker in a collection"),
    ),
    (
        "Rv2",
        "t6",
        Gap("F5 (#162): a held jar object prints its marker in a collection"),
    ),
    (
        "Rv2",
        "t7",
        Gap("F5 (#162): a held jar object prints its marker in a collection"),
    ),
    ("Rv2", "t8", Exact("Optional[Optional[Node(a)]]")),
    ("Rv2", "t9", Counted),
    ("Rv2", "t10", Counted),
    ("Rv2", "t11", Counted),
    ("Rv2", "t12", Counted),
    ("Rv2", "t13", Exact("[Node(a)]")),
    ("Rv2", "h1", Exact("73")),
    ("Rv2", "h2", Exact("1025")),
    ("Rv2", "h3", Exact("true")),
    ("Rv2", "h4", Exact("7")),
    ("Rv2", "h5", Exact("true")),
    ("Rv2", "h6", Exact("true")),
    ("Rv2", "h7", Exact("31817")),
    ("Rv2", "h8", Exact("true")),
    (
        "Rv2",
        "h9",
        Gap("F7 (#162): equal keys are not equals for held collections"),
    ),
    ("Rv2", "h10", Exact("falsefalse")),
    ("Rv2", "i1", Counted),
    ("Rv2", "i2", Counted),
    ("Rv2", "i3", Counted),
    ("Rv2", "i4", Exact("1,2")),
    ("Rv2", "i5", Exact("1,2")),
    ("Rv2", "i6", Counted),
    ("Rv2", "i7", Counted),
    ("Rv2", "i8", Counted),
    (
        "Rv2",
        "c1",
        Gap("F6 (#162): a cast to a handle class passes"),
    ),
    (
        "Rv2",
        "c2",
        Gap("F6 (#162): a cast to a handle class passes"),
    ),
    (
        "Rv2",
        "c3",
        Gap("F6 (#162): a cast to a handle class passes"),
    ),
    ("Rv2", "c4", Exact("falsefalse")),
    (
        "Rv2",
        "c5",
        Exact(
            "class java.util.Optional cannot be cast to class java.lang.String (java.util.Optional and java.lang.String are in module java.base of loader 'bootstrap')",
        ),
    ),
    (
        "Rv2",
        "f1",
        Exact(
            "truefalse,truefalse,truefalse,truefalse,truefalse,truetruetruetruetruetruetruetruetruetruetruehopt:",
        ),
    ),
    ("Rv2", "f2", Exact("falsefalsefalsefalsefalse2hopt:String")),
    ("Rv3", "m1", Exact("7")),
    ("Rv3", "m2", Exact("7")),
    ("Rv3", "m3", Exact("7true")),
    ("Rv3", "m4", Exact("truetrue")),
    (
        "Rv3",
        "m5",
        Gap("F7 (#162): equal keys are not equals for held collections"),
    ),
    ("Rv3", "m6", Exact("[1]")),
    (
        "Rv3",
        "m7",
        Gap("F5 (#162): a held jar object prints its marker in a collection"),
    ),
    ("Rv3", "m8", Exact("7")),
    ("Rv3", "m9", Exact("truetrue")),
    (
        "Rv3",
        "e1",
        Gap("F7 (#162): equal keys are not equals for held collections"),
    ),
    (
        "Rv3",
        "e2",
        Gap("F7 (#162): equal keys are not equals for held collections"),
    ),
    (
        "Rv3",
        "e3",
        Gap("F7 (#162): equal keys are not equals for held collections"),
    ),
    ("Rv4", "n1", Exact("EXC java.lang.NullPointerException")),
    ("Rv4", "n2", Exact("EXC java.lang.NullPointerException")),
    ("Rv4", "n3", Exact("EXC java.lang.NullPointerException")),
    ("Rv4", "n4", Exact("EXC java.lang.NullPointerException")),
    ("Rv4", "n5", Exact("EXC java.lang.NullPointerException")),
    ("Rv4", "o1", Exact("42,true,true")),
    (
        "IdMap",
        "sameMembers",
        Exact("true11122true6788falsetrue1false80"),
    ),
    ("IdMap", "sameEmpty", Exact("2,2truefalse")),
    ("IdMap", "otherContainsKey", Counted),
    ("IdMap", "otherGet", Counted),
    ("IdMap", "otherGetOrDefault", Counted),
    ("IdMap", "otherPut", Counted),
    ("IdMap", "otherPutIfAbsent", Counted),
    ("IdMap", "otherRemove", Counted),
    ("IdMap", "otherRemoveValue", Counted),
    ("IdMap", "otherReplace", Counted),
    ("IdMap", "otherMerge", Counted),
    ("IdMap", "otherCompute", Counted),
    ("IdMap", "otherComputeIfPresent", Counted),
    ("IdMap", "otherComputeIfAbsent", Counted),
    ("IdMap", "otherContainsValue", Counted),
    ("IdMap", "otherKeySetContains", Counted),
    ("IdMap", "otherKeySetRemove", Counted),
    ("IdMap", "otherEntrySetContains", Counted),
    ("IdMap", "otherCopy", Counted),
    ("IdMap", "otherPutAll", Counted),
];

#[test]
fn host_values_answer_as_java_25_or_count() {
    let fixture = Fixture::build(
        "host-values-review",
        &[
            ("demo/Rv.java", include_str!("fixtures/host_values/Rv.java")),
            (
                "demo/Rv2.java",
                include_str!("fixtures/host_values/Rv2.java"),
            ),
            (
                "demo/Rv3.java",
                include_str!("fixtures/host_values/Rv3.java"),
            ),
            (
                "demo/Rv4.java",
                include_str!("fixtures/host_values/Rv4.java"),
            ),
            (
                "demo/IdMap.java",
                include_str!("fixtures/host_values/IdMap.java"),
            ),
        ],
    );
    let mut failures = Vec::new();
    for (class, method, outcome) in CASES {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            &format!("demo/{class}"),
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let unknown = host.unknown_host_calls();
        let ok = match outcome {
            Exact(expected) => {
                matches!(&answer, Ok(Value::Ref(Some(text))) if text == expected)
                    && unknown.is_empty()
            }
            Counted => !unknown.is_empty(),
            Gap(_) => true,
        };
        if !ok {
            let expected = match outcome {
                Exact(text) => format!("exact {text}"),
                Counted => "counted".to_owned(),
                Gap(_) => unreachable!(),
            };
            failures.push(format!(
                "{class}.{method}\n  want:    {expected}\n  native:  {answer:?}\n  unknown: {unknown:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases:\n{}",
        failures.len(),
        CASES.len(),
        failures.join("\n")
    );
}
