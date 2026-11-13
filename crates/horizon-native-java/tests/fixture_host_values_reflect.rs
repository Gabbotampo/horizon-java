//! `Field`, `Method` and `Constructor` are typed handles (#156 P10).
//!
//! They used to be text a jar `String` could spell (`reflect-method/…`,
//! `reflect-field/…`, `reflect-constructor/…`), and `toString` printed that
//! text. `fixtures/host_values/Refl.java` checks that a string spelled like
//! the old markers (or a handle's encoding) stays a `String`, that the real
//! objects print, hash, compare and cast as Java's do, and every identity
//! path. Each expected string is what its `main` printed on OpenJDK 25.0.5
//! (`javac --release 21`, two runs alike). Each case is `Exact` (Java's
//! answer, nothing counted) or `Counted` (Java's answer rests on an identity
//! the run cannot see, and the run counts it).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;

enum Outcome {
    Exact(&'static str),
    Counted,
}
use Outcome::*;

const CASES: &[(&str, Outcome)] = &[
    // Strings stay Strings: no member `instanceof`, `equals` both ways,
    // concat; the handles' encodings also `instanceof String`, `hashCode`, a
    // `HashMap` key and `(Method) "hrmth:…"`, which throws Java's exception.
    // The old `word/rest` markers skip those (`is_plain_string` still reads
    // them as non-strings until #156 P17, as P0's fixture notes), and no
    // text with a `#` asks `getClass` (`class#n` is P12's family).
    (
        "spelled",
        Exact(concat!(
            "falsefalsefalsefalsefalsefalsefalsefalsetrue|reflect-method/demo/Refl#target(I)I,",
            "falsefalsefalsefalsefalsefalsefalsefalsetrue|reflect-field/demo/Refl#COUNT:I:25,",
            "falsefalsefalsefalsefalsefalsefalsefalsetrue|reflect-field/demo/Refl#COUNT:I:25#3,",
            "falsefalsefalsefalsefalsefalsefalsefalsetrue|reflect-constructor/demo/Refl#()V,",
            "falsefalsefalsefalsefalsefalsefalsefalsetrue|hrmth:demo/Refl#target(I)I:9,true-1103501518,",
            "falsefalsefalsefalsefalsefalsefalsefalsetrue|hrfld:demo/Refl#COUNT:I:25#3,true-586347693,",
            "falsefalsefalsefalsefalsefalsefalsefalsetrue|hrctr:demo/Refl#()V:1,true122162635,",
            "5smf|class java.lang.String cannot be cast to class java.lang.reflect.Method ",
            "(java.lang.String and java.lang.reflect.Method are in module java.base of loader ",
            "'bootstrap')|oktarget"
        )),
    ),
    // `toString` and `hashCode` of every modifier shape (abstract, default
    // and static interface methods, varargs, arrays, inner classes, an enum
    // constant, `throws` clauses).
    (
        "text",
        Exact(concat!(
            "public static int demo.Refl.target(int) throws java.io.IOException #-406041067\n",
            "static synchronized java.lang.String demo.Refl.join(int,java.lang.String[],java.util.List) #746840750\n",
            "private final void demo.Refl.hidden() #-1680039378\n",
            "public abstract java.lang.String demo.Refl$Api.name() #48449105\n",
            "public default int demo.Refl$Api.size() #48618715\n",
            "public static void demo.Refl$Api.util() #48680152\n",
            "public static final int demo.Refl.COUNT #795305835\n",
            "private transient volatile java.lang.String[] demo.Refl.names #713681100\n",
            "protected static long[][] demo.Refl.grid #746763458\n",
            "int demo.Refl.plain #720299598\n",
            "public static final demo.Refl$Mode demo.Refl$Mode.ON #1463600956\n",
            "public demo.Refl() #749879172\n",
            "protected demo.Refl(int,java.lang.String[]) throws java.io.IOException,java.lang.IllegalStateException #749879172\n",
            "public demo.Refl$Inner(demo.Refl[]) #-1876664042\n",
            "public static int demo.Refl.target(int) throws java.io.IOException|public static final int demo.Refl.COUNT"
        )),
    ),
    // Copies are `equals` and share a hash; each is its class and its
    // supertypes; the members still work.
    (
        "real",
        Exact(
            "truetruetruefalsefalsefalsefalse|truetrue|java.lang.reflect.Method,java.lang.reflect.Field,java.lang.reflect.Constructor|truetruetruetruetruefalsefalsetruefalse|429targetCOUNT20",
        ),
    ),
    // Collections find a copy by `Field`/`Method.equals`; a copy read back
    // out of a list is the same object.
    ("collections", Exact("true1212truetruetrue|truetrue")),
    // The same handle on both sides: one object, also through a list (its
    // `setAccessible` reaches the copy, not another lookup's).
    ("sameHandle", Exact("truetruetruetruetruefalse")),
    // Two `Field` copies carry two copy numbers: decided, never `==`.
    ("fieldCopies", Exact("falsetrue2")),
    // Two `Method`/`Constructor` lookups carry equal keys (no copy number):
    // Java says two objects (`false`, size 2); counted.
    ("methodCopies", Counted),
    ("constructorCopies", Counted),
];

#[test]
fn reflection_handles_answer_as_java_25_or_count() {
    let fixture = Fixture::build(
        "host-values-reflect",
        &[(
            "demo/Refl.java",
            include_str!("fixtures/host_values/Refl.java"),
        )],
    );
    let mut failures = Vec::new();
    for (method, outcome) in CASES {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Refl",
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
        };
        if !ok {
            let expected = match outcome {
                Exact(text) => format!("exact {text}"),
                Counted => "counted".to_owned(),
            };
            failures.push(format!(
                "{method}\n  want:    {expected}\n  native:  {answer:?}\n  unknown: {unknown:?}"
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
