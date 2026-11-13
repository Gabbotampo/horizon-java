//! `FabricLoader.getAllMods` and each container's own metadata
//! (`fabric_mods.rs`), on jars compiled with `javac`.
//!
//! The run: `alpha` is the running jar and bundles `gamma` (a nested jar);
//! `beta` is linked beside it. `alpha` declares a dependency on `absentdep`,
//! which nothing ships.
//!
//! Expected values:
//! * the metadata lines are what `fixtures/fabric_mods/demo/Describe.java`
//!   printed on JDK 25 (`jdk-25.0.5+8`) for the same three `fabric.mod.json`
//!   files, and for Fabric Loader 0.19.5's own and its bundled MixinExtras
//!   0.5.5's and `fabric-api-0.165.0+26.3`'s `fabric-api-base` module's,
//!   parsed by Fabric Loader 0.19.5's own `ModMetadataParser`;
//! * the order is Fabric Loader 0.19.5's: `ModResolver.findCompatibleSet`
//!   sorts the selected mods by `ModCandidateImpl::getId` (offset 841) and
//!   `getAllMods` is `Collections.unmodifiableList(mods)` in that order. The
//!   built-in containers are `java`, `minecraft`, `fabricloader`, `mixinextras`
//!   and the host's Fabric API ids.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fabric_mods");

const ORACLE: &str = "\
alpha: type=fabric version=1.2.0+mc26.3 name=Alpha Mod description=[Does things] environment=SERVER authors=Ann<->,Bob<b@x.example> contributors= homepage=https://h.example issues=- license=[MIT] provides=[alpha_alias] options=OBJECT(size=2,a=BOOLEAN(false),has_b=true,badges=absent) modmenu=OBJECT(size=1,a=absent,has_b=false,badges=ARRAY(size=1,0=STRING(library),5=Index 5 out of bounds for length 1)) note=STRING(hi) count=NUMBER#3 nothing=NULL#5 containsNothing=true containsMissing=false
beta: type=fabric version=0.1 name=beta description=[] environment=UNIVERSAL authors= contributors= homepage=- issues=- license=[] provides=[] options=absent modmenu=absent note=absent count=absent nothing=absent containsNothing=false containsMissing=false
fabric-api-base: type=fabric version=2.0.6+fcdff87f5d name=Fabric API Base description=[Contains the essentials for Fabric API modules.] environment=UNIVERSAL authors=FabricMC<-> contributors= homepage=https://fabricmc.net issues=https://github.com/FabricMC/fabric/issues license=[Apache-2.0] provides=[] options=absent modmenu=absent note=absent count=absent nothing=absent containsNothing=false containsMissing=false
fabricloader: type=fabric version=0.19.5 name=Fabric Loader description=[The base mod loader.] environment=UNIVERSAL authors=FabricMC<-> contributors= homepage=https://fabricmc.net issues=https://github.com/FabricMC/fabric-loader/issues license=[Apache-2.0] provides=[] options=absent modmenu=absent note=absent count=absent nothing=absent containsNothing=false containsMissing=false
gamma: type=fabric version=2.0-beta.1 name=gamma description=[] environment=UNIVERSAL authors= contributors=Cy<-> homepage=- issues=- license=[MIT, Apache-2.0] provides=[] options=absent modmenu=absent note=absent count=absent nothing=absent containsNothing=false containsMissing=false
mixinextras: type=fabric version=0.5.5 name=MixinExtras description=[Companion library to Mixin with lots of features to improve the compatibility and concision of your mixins!] environment=UNIVERSAL authors=LlamaLad7<-> contributors= homepage=https://github.com/LlamaLad7/MixinExtras issues=- license=[MIT] provides=[com_github_llamalad7_mixinextras] options=absent modmenu=OBJECT(size=1,a=absent,has_b=false,badges=ARRAY(size=1,0=STRING(library),5=Index 5 out of bounds for length 1)) note=absent count=absent nothing=absent containsNothing=false containsMissing=false
";

struct Run {
    root: PathBuf,
    alpha: NativeJar,
    beta: NativeJar,
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
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

fn zip_bytes(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(name.as_str(), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn build(test: &str) -> Run {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-fabric-mods-{test}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let fixture = Path::new(FIXTURE);
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let mut command = Command::new(javac);
    command
        .args(["--release", "21", "-d"])
        .arg(root.join("classes"));
    for source in files_under(&fixture.join("demo"))
        .into_iter()
        .chain(files_under(&fixture.join("stubs")))
    {
        command.arg(source);
    }
    assert!(
        command.status().unwrap().success(),
        "javac failed for {test}"
    );
    let json = |name: &str| fs::read(fixture.join(name)).unwrap();
    let gamma = zip_bytes(&[("fabric.mod.json".into(), json("gamma.json"))]);
    let mut alpha = vec![
        ("fabric.mod.json".to_owned(), json("alpha.json")),
        ("META-INF/jars/gamma.jar".to_owned(), gamma),
    ];
    let classes = root.join("classes");
    for class in files_under(&classes.join("demo")) {
        let entry = class.strip_prefix(&classes).unwrap().to_string_lossy();
        alpha.push((entry.replace('\\', "/"), fs::read(&class).unwrap()));
    }
    let alpha = NativeJar::from_bytes(&zip_bytes(&alpha)).unwrap();
    let beta = NativeJar::from_bytes(&zip_bytes(&[("fabric.mod.json".into(), json("beta.json"))]))
        .unwrap();
    Run { root, alpha, beta }
}

fn text(run: &Run, host: &mut StdHost, method: &str) -> String {
    match run
        .alpha
        .invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            host,
        )
        .unwrap_or_else(|error| panic!("{method} failed: {error}"))
    {
        Value::Ref(Some(text)) => text,
        other => panic!("{method} did not return a string: {other:?}"),
    }
}

fn host(run: &Run, game_version: Option<&str>) -> StdHost {
    let mut seed = StdHost::new().with_environment_type("SERVER");
    if let Some(version) = game_version {
        seed = seed.with_game_version(version);
    }
    let mut host = run.alpha.seed_host(seed);
    host.link_jar("beta", &run.beta).unwrap();
    host
}

#[test]
fn all_mods_lists_the_runs_containers_in_mod_id_order() {
    let run = build("ids");
    let mut host = host(&run, Some("26.3"));
    assert_eq!(
        text(&run, &mut host, "ids"),
        "alpha,beta,fabric-api,fabric-api-base,fabric-command-api-v2,\
         fabric-lifecycle-events-v1,fabric-resource-loader-v1,fabricloader,gamma,java,\
         minecraft,mixinextras"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn each_container_answers_from_its_own_fabric_mod_json() {
    let run = build("describe");
    let mut host = host(&run, Some("26.3"));
    assert_eq!(text(&run, &mut host, "describe"), ORACLE);
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn builtin_containers_answer_only_what_is_known() {
    let run = build("builtins");
    let mut host = host(&run, Some("26.3"));
    // Fabric Loader is the 0.19.5 the runtime reproduces; `java` is Java 25,
    // whose `java.vm.name` depends on the JDK vendor, so it is counted (and
    // answers `null`); `minecraft` is the seeded game version.
    assert_eq!(
        text(&run, &mut host, "builtins"),
        "loader-name=Fabric Loader;java=null/25/builtin/false;\
         minecraft=Minecraft/26.3/UNIVERSAL/-;"
    );
    assert_eq!(
        host.unknown_host_calls(),
        vec![(
            "net/fabricmc/loader/api/metadata/ModMetadata#getName()Ljava/lang/String;".to_owned(),
            1
        )]
    );
    // Without a seeded game version, `minecraft`'s version is unknown too:
    // counted, and its neutral `null` then fails the probe's
    // `getFriendlyString`, as an unanswered call does.
    let mut host = self::host(&run, None);
    assert!(
        run.alpha
            .invoke_method_with_args(
                "demo/Probe",
                "builtins",
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .is_err()
    );
    assert!(
        host.unknown_host_calls().iter().any(|(call, _)| call
            == "net/fabricmc/loader/api/metadata/ModMetadata#getVersion()Lnet/fabricmc/loader/api/Version;"),
        "{:?}",
        host.unknown_host_calls()
    );
}
