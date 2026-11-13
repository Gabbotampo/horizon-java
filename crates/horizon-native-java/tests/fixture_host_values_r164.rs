//! The review of #164 (`fixtures/host_values/R164.java`, the reviewer's
//! 117 cases, unchanged): paths, files, `FMLPaths` and the watch service's
//! objects against Java 25.
//!
//! Every `Exact` string is what OpenJDK 25.0.5 printed (elysium, `javac
//! --release 21`, two runs identical, `FMLPaths` from NeoForge loader
//! 12.0.12). `Counted` cases need what no run reproduces (an identity hash,
//! the machine's `sun.jnu.encoding`, two objects with equal keys) or a model
//! the runtime does not have yet, and count it. `Gap` cases are silent
//! differences that predate #164, each with its plan; they are not asserted.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;

enum Outcome {
    Exact(&'static str),
    Counted,
    #[allow(dead_code)]
    Gap(&'static str),
}
use Outcome::*;

const CASES: &[(&str, Outcome)] = &[
    ("fAbs", Counted),
    ("fCastFml", Counted),
    (
        "fCastKey",
        Gap(
            "pre-existing: a String checkcast to a JDK interface passes (String -> interface casts; #156 P17)",
        ),
    ),
    (
        "fCastKind",
        Gap(
            "pre-existing: a String checkcast to a JDK interface passes (String -> interface casts; #156 P17)",
        ),
    ),
    (
        "fCastPath",
        Gap(
            "pre-existing: a String checkcast to a JDK interface passes (String -> interface casts; #156 P17)",
        ),
    ),
    (
        "fCastRealToString",
        Exact(
            r#"EXC java.lang.ClassCastException:class sun.nio.fs.UnixPath cannot be cast to class java.lang.String (sun.nio.fs.UnixPath and java.lang.String are in module java.base of loader 'bootstrap')"#,
        ),
    ),
    ("fClassCast", Exact(r#"falsefalsetrue"#)),
    ("fClasses", Exact(r#"java.io.Filetruetruefalse"#)),
    (
        "fCollections",
        Exact(
            r#"[path:/x, /x, hpath:/y, watch-kind/OVERFLOW]|{path:/k=/v, /k2=path:/v2}|StringString|012|truefalse"#,
        ),
    ),
    ("fDot", Counted),
    ("fDotDot", Counted),
    ("fDouble", Counted),
    ("fEmpty", Counted),
    ("fEqHash", Counted),
    (
        "fFile",
        Exact(
            r#"java.lang.String,true,false,true,-1352516893,file:/a/b,file:/a/b,9,s,[file:/a/b],file:/a/b,file:/a/b,file:/a/b"#,
        ),
    ),
    ("fFml", Counted),
    ("fFmlLong", Counted),
    (
        "fHFml",
        Exact(
            r#"java.lang.String,true,false,true,-260702284,hfmlp:GAMEDIR,hfmlp:GAMEDIR,13,s,[hfmlp:GAMEDIR],hfmlp:GAMEDIR,hfmlp:GAMEDIR,hfmlp:GAMEDIR"#,
        ),
    ),
    (
        "fHPath",
        Exact(
            r#"java.lang.String,true,false,true,72157238,hpath:/x,hpath:/x,8,s,[hpath:/x],hpath:/x,hpath:/x,hpath:/x"#,
        ),
    ),
    (
        "fHWkey",
        Exact(
            r#"java.lang.String,true,false,true,1319915047,hwkey:1,hwkey:1,7,s,[hwkey:1],hwkey:1,hwkey:1,hwkey:1"#,
        ),
    ),
    (
        "fHWkind",
        Exact(
            r#"java.lang.String,true,false,true,1438343808,hwkind:ENTRY_CREATE,hwkind:ENTRY_CREATE,19,s,[hwkind:ENTRY_CREATE],hwkind:ENTRY_CREATE,hwkind:ENTRY_CREATE,hwkind:ENTRY_CREATE"#,
        ),
    ),
    ("fLong", Counted),
    ("fNonAscii", Counted),
    ("fParentChild", Exact(r#"/a|a|a|/b|a/b|a/b|/x|y|/c|/|/"#)),
    (
        "fPath",
        Exact(
            r#"java.lang.String,true,false,true,-1250755840,path:/horizon/game,path:/horizon/game,18,s,[path:/horizon/game],path:/horizon/game,path:/horizon/game,path:/horizon/game"#,
        ),
    ),
    (
        "fPathApis",
        Exact(
            r#"path:/horizon/config|false|path:/horizon/config/x|config|path:/horizon|falsefalse|/horizon/path:/x|path:/a/b|path:/a/b|false"#,
        ),
    ),
    (
        "fPathEmpty",
        Exact(
            r#"java.lang.String,true,false,true,106438837,path:,path:,5,s,[path:],path:,path:,path:"#,
        ),
    ),
    (
        "fPathRel",
        Exact(
            r#"java.lang.String,true,false,true,-1043888073,path:config,path:config,11,s,[path:config],path:config,path:config,path:config"#,
        ),
    ),
    ("fRel", Counted),
    ("fRoot", Counted),
    (
        "fRoundTrip",
        Exact(r#"/a/b,true,true,/a/b,x/y,true,,,truesun.nio.fs.UnixPath"#),
    ),
    ("fSwitch", Exact(r#"SPSK"#)),
    ("fTextOps", Counted),
    ("fVsReal", Counted),
    ("fWatchEvt", Counted),
    ("fWatchKey", Counted),
    ("fWatchKind", Counted),
    ("fWatchSvc", Counted),
    ("fmlEnum", Counted),
    ("fmlEnumMap", Exact(r#"{MODSDIR=1}"#)),
    ("fmlHashSet", Exact(r#"2true"#)),
    ("fmlJij", Counted),
    ("fmlSwitch", Counted),
    ("iFileSame", Exact(r#"truetrue"#)),
    ("iFiles", Exact(r#"false"#)),
    ("iIdMap", Counted),
    ("iKinds", Counted),
    ("iNormalizeSame", Counted),
    ("iSameLocal", Exact(r#"truetrue12true"#)),
    ("iThroughList", Counted),
    ("iToAbsSame", Counted),
    ("iTwoPaths", Counted),
    ("iTwoPathsNe", Counted),
    ("pAbsOne", Counted),
    (
        "pAbsParent",
        Gap("pre-existing: Path.getParent of a root child answers null (Java `/`)"),
    ),
    ("pAbsTrailing", Counted),
    ("pCompare", Exact(r#"-1,-2,0,1"#)),
    ("pDot", Counted),
    ("pDotDot", Counted),
    ("pDoubleRoot", Counted),
    ("pDoubleSlash", Counted),
    ("pEmpty", Counted),
    ("pEmptyFileName", Exact(r#"[]"#)),
    ("pEquals", Exact(r#"truefalsefalsetruefalsefalsetrue"#)),
    (
        "pFileNameParent",
        Gap("pre-existing: Path.getParent/getFileName at the root"),
    ),
    ("pHashCollections", Exact(r#"2,true,12,4015,a"#)),
    ("pIterator", Counted),
    ("pLong", Counted),
    ("pNames", Counted),
    ("pNonAscii", Counted),
    (
        "pNormalize",
        Gap("pre-existing: Path.normalize returns its input (no `.`/`..` folding)"),
    ),
    ("pNormalizeClean", Exact(r#"/a/b|true"#)),
    ("pOfMore", Exact(r#"a/b|a|a|/a|a/b|a/b/c||/a|/a/b/c"#)),
    ("pRelDot", Counted),
    ("pRelativize", Counted),
    ("pResolve", Exact(r#"a|/b|/b|b|a/b|a/b/c|a|/z"#)),
    ("pResolveDots", Exact(r#"a/..|a/./b"#)),
    ("pRoot", Counted),
    (
        "pRootFileName",
        Gap("pre-existing: Path.getFileName of `/` answers the empty path (Java null)"),
    ),
    ("pStartsEnds", Counted),
    (
        "pTextOps",
        Exact(
            r#"/horizon/game|/horizon/game|/horizon/game|/horizon/game|Optional[/horizon/game]|[/horizon/game, a]|{/horizon/game=1}|13"#,
        ),
    ),
    ("pToAbsAbs", Exact(r#"/a/b"#)),
    (
        "pToString",
        Exact(
            r#"[]0,[.]46,[..]1472,[/]47,[/]47,[a/b]94772,[/a/b]1494949,[./a]45760,[a/./b]91027475,[a/../b]-1473118015,[/x/y/z]1457910654,"#,
        ),
    ),
    ("wFlow", Counted),
    ("wHash", Counted),
    (
        "wKindsText",
        Exact(
            r#"ENTRY_CREATEENTRY_CREATEENTRY_CREATE[ENTRY_CREATE],ENTRY_DELETEENTRY_DELETEENTRY_DELETE[ENTRY_DELETE],ENTRY_MODIFYENTRY_MODIFYENTRY_MODIFY[ENTRY_MODIFY],OVERFLOWOVERFLOWOVERFLOW[OVERFLOW],4"#,
        ),
    ),
    ("wObjText", Counted),
    (
        "xCastKindToPath",
        Exact(
            r#"EXC java.lang.ClassCastException:class java.nio.file.StandardWatchEventKinds$StdWatchEventKind cannot be cast to class java.nio.file.Path (java.nio.file.StandardWatchEventKinds$StdWatchEventKind and java.nio.file.Path are in module java.base of loader 'bootstrap')"#,
        ),
    ),
    (
        "xCastMsgOptional",
        Exact(
            r#"EXC java.lang.ClassCastException:class java.util.Optional cannot be cast to class java.lang.String (java.util.Optional and java.lang.String are in module java.base of loader 'bootstrap')"#,
        ),
    ),
    (
        "xCastPathToFile",
        Exact(
            r#"EXC java.lang.ClassCastException:class sun.nio.fs.UnixPath cannot be cast to class java.io.File (sun.nio.fs.UnixPath and java.io.File are in module java.base of loader 'bootstrap')"#,
        ),
    ),
    (
        "xCastPlain",
        Gap(
            "pre-existing: a String checkcast to a JDK interface passes (String -> interface casts)",
        ),
    ),
    (
        "xCastRunnable",
        Gap(
            "pre-existing: a String checkcast to a JDK interface passes (String -> interface casts)",
        ),
    ),
    (
        "xFileFromPath",
        Exact(r#"/horizon/config|config|/horizon|-400671308|/horizon/config/x.json|true"#),
    ),
    ("xFileList", Counted),
    ("xFileNullChild", Counted),
    ("xFileNullParent", Counted),
    ("xFileStrParentNonNorm", Exact(r#"a/b|/|/"#)),
    ("xSwitchFml", Exact(r#"F"#)),
    ("xSwitchKind", Exact(r#"KENTRY_DELETE"#)),
    ("xSwitchOptional", Exact(r#"OOS"#)),
    ("xSwitchPath", Exact(r#"Px"#)),
    (
        "xWatchFlow",
        Exact(
            r#"falsefalsetruefalsefalse|truetrue|truer164-watch2|true1ENTRY_CREATE1a.txttruefalse|true0|falsefalse|closed"#,
        ),
    ),
    ("xWatchIdMap", Exact(r#"23truefalse"#)),
    ("xWatchInList", Exact(r#"truetruetruetrue"#)),
    ("yArrayStore", Counted),
    ("yArrayStoreKind", Exact(r#"ENTRY_CREATE"#)),
    ("yClassCast", Exact(r#"/x/x|true"#)),
    ("yCollSort", Exact(r#"[/y, /z]"#)),
    ("yComparing", Exact(r#"[/y, /z]"#)),
    ("yInstanceofPattern", Exact(r#"PyKENTRY_MODIFYC"#)),
    ("yListSortNull", Exact(r#"[/y, /z]"#)),
    ("yMaps", Exact(r#"12true2true"#)),
    ("yRecordSwitch", Exact(r#"CI"#)),
    (
        "ySetOfDup",
        Exact(r#"EXC java.lang.IllegalArgumentException:duplicate element: a"#),
    ),
    ("ySorted", Exact(r#"[a, a/c, b]"#)),
    ("yStreamMap", Exact(r#"[b, d]"#)),
    ("yTreeMap", Exact(r#"a"#)),
];

const FMLPATHS_STUB: &str = r#"
package net.neoforged.fml.loading;
public enum FMLPaths {
  GAMEDIR, JIJ_CACHEDIR, MODSDIR, CONFIGDIR, FMLCONFIG;
  public java.nio.file.Path get() { return null; }
}
"#;

#[test]
fn review_cases_answer_as_java_25_or_count() {
    let fixture = Fixture::build(
        "host-values-r164",
        &[
            (
                "demo/R164.java",
                include_str!("fixtures/host_values/R164.java"),
            ),
            ("net/neoforged/fml/loading/FMLPaths.java", FMLPATHS_STUB),
        ],
    );
    let mut failures = Vec::new();
    for (method, outcome) in CASES {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/R164",
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
            failures.push(format!(
                "{method}\n  native:  {answer:?}\n  unknown: {unknown:?}"
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
