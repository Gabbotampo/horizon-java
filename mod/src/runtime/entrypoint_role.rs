//! Which entrypoints a jar runs for one execution role.
//!
//! A jar can carry metadata for more than one loader: "universal" builds ship a
//! `fabric.mod.json` beside a `META-INF/neoforge.mods.toml` and a `@Mod` class.
//! Each real loader reads only its own metadata. Fabric Loader never scans
//! classes for `@Mod`; it runs the `fabric.mod.json` entrypoints for the side it
//! is on (`main` everywhere, `client` or `server` for that side) and nothing
//! else. FML never reads `fabric.mod.json`.
//!
//! The native runtime treats a jar with Fabric metadata as a Fabric mod (the
//! same rule the game's compatibility report names the loader by), so it runs
//! only Fabric entrypoint kinds for it. Falling back to the NeoForge `@Mod`
//! class when the Fabric side declares nothing for this role would run code no
//! loader runs in that situation: zoomify declares only a `client` entrypoint,
//! and its `@Mod` constructor on a server reaches the client-only
//! `KeyMapping`. On a real Fabric server zoomify runs nothing, and so does the
//! native path.

use crate::runtime::{Entrypoint, NativeJar};

/// The entrypoint kinds Fabric Loader runs on a client, in the order it runs
/// them: `Hooks.startClient` invokes `main`, then `client`.
const FABRIC_CLIENT_KINDS: &[&str] = &["main", "client"];
/// The entrypoint kinds Fabric Loader runs on a dedicated server, in the order
/// it runs them: `Hooks.startServer` invokes `main`, then `server`.
const FABRIC_SERVER_KINDS: &[&str] = &["main", "server"];
/// A NeoForge (or legacy Forge) jar's only entrypoint kind: its `@Mod` classes,
/// constructed on both sides.
const NEOFORGE_KINDS: &[&str] = &["neoforge"];

impl NativeJar {
    /// Whether the jar carries Fabric metadata, which makes it a Fabric mod
    /// for native execution whatever other loader's metadata it also ships.
    pub fn has_fabric_metadata(&self) -> bool {
        self.fabric_id.is_some()
            || !self.fabric_entrypoints.is_empty()
            || self.resource_names().any(|name| name == "fabric.mod.json")
    }

    /// The entrypoint kinds this jar runs for a role, in priority order.
    ///
    /// A Fabric jar gets its role's Fabric kinds only; any other jar gets
    /// `neoforge`, its `@Mod` classes.
    pub fn role_entrypoint_kinds(&self, client: bool) -> &'static [&'static str] {
        match (self.has_fabric_metadata(), client) {
            (true, true) => FABRIC_CLIENT_KINDS,
            (true, false) => FABRIC_SERVER_KINDS,
            (false, _) => NEOFORGE_KINDS,
        }
    }

    /// Every entrypoint this jar runs for a role, in loader order, each paired
    /// with the kind that names it. Empty when the jar runs nothing in this
    /// role.
    ///
    /// Fabric runs both stages, not the first one that declares anything: a
    /// server jar with `main` and `server` entrypoints runs `main` first and
    /// `server` after it.
    pub fn role_entrypoints(&self, client: bool) -> Vec<(&'static str, Entrypoint)> {
        self.role_entrypoint_kinds(client)
            .iter()
            .flat_map(|kind| {
                self.discover_entrypoints(kind)
                    .into_iter()
                    .map(move |entry| (*kind, entry))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::NativeJar;
    use std::io::Write;

    /// A jar of the given resources and class files, read the way the game
    /// reads a mod (`NativeJar::from_path`), so `@Mod` discovery is the real one.
    fn jar(resources: &[(&str, &[u8])]) -> NativeJar {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "horizon-entrypoint-role-{}-{}.jar",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in resources {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        let jar = NativeJar::from_path(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        jar
    }

    /// A class file `demo/NeoMod` that carries NeoForge's `@Mod` annotation,
    /// which is what discovery keys a `@Mod` class on. A descriptor string in
    /// the pool alone is not an annotation (Kotlin's `Metadata` keeps one).
    fn neoforge_mod_class() -> Vec<u8> {
        fn utf8(out: &mut Vec<u8>, value: &str) {
            out.push(1);
            out.extend_from_slice(&(value.len() as u16).to_be_bytes());
            out.extend_from_slice(value.as_bytes());
        }
        // magic, version 52, six pool slots (1..=6).
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52, 0, 7];
        utf8(&mut out, "demo/NeoMod");
        out.extend_from_slice(&[7, 0, 1]);
        utf8(&mut out, "java/lang/Object");
        out.extend_from_slice(&[7, 0, 3]);
        utf8(&mut out, "Lnet/neoforged/fml/common/Mod;");
        utf8(&mut out, "RuntimeVisibleAnnotations");
        // public class, this #2, super #4, no interfaces, fields or methods,
        // one `@Mod` annotation with no members.
        out.extend_from_slice(&[0, 0x21, 0, 2, 0, 4, 0, 0, 0, 0, 0, 0, 0, 1]);
        out.extend_from_slice(&[0, 6, 0, 0, 0, 6, 0, 1, 0, 5, 0, 0]);
        out
    }

    /// A class file `demo/NeoClient` whose `@Mod(value = "demo", dist =
    /// [Dist.CLIENT])` is the shape javap shows for fzzy_config's client
    /// class. The `dist` element is an array of enum constants, not strings.
    fn client_only_neoforge_mod_class() -> Vec<u8> {
        fn u2(out: &mut Vec<u8>, value: u16) {
            out.extend_from_slice(&value.to_be_bytes());
        }
        fn utf8(out: &mut Vec<u8>, value: &str) {
            out.push(1);
            u2(out, value.len() as u16);
            out.extend_from_slice(value.as_bytes());
        }
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52];
        u2(&mut out, 12); // pool slots 1..=11
        utf8(&mut out, "demo/NeoClient"); // 1
        out.extend_from_slice(&[7, 0, 1]); // 2 Class
        utf8(&mut out, "java/lang/Object"); // 3
        out.extend_from_slice(&[7, 0, 3]); // 4 Class
        utf8(&mut out, "Lnet/neoforged/fml/common/Mod;"); // 5
        utf8(&mut out, "RuntimeVisibleAnnotations"); // 6
        utf8(&mut out, "value"); // 7
        utf8(&mut out, "demo"); // 8
        utf8(&mut out, "dist"); // 9
        utf8(&mut out, "Lnet/neoforged/api/distmarker/Dist;"); // 10
        utf8(&mut out, "CLIENT"); // 11
        // public class, this #2, super #4, no interfaces, fields or methods.
        out.extend_from_slice(&[0, 0x21, 0, 2, 0, 4, 0, 0, 0, 0, 0, 0]);
        // One RuntimeVisibleAnnotations attribute.
        u2(&mut out, 1);
        u2(&mut out, 6);
        let mut annotations = Vec::new();
        u2(&mut annotations, 1); // one annotation
        u2(&mut annotations, 5); // type
        u2(&mut annotations, 2); // two pairs
        u2(&mut annotations, 7); // value =
        annotations.extend_from_slice(&[b's', 0, 8]);
        u2(&mut annotations, 9); // dist =
        annotations.push(b'[');
        u2(&mut annotations, 1); // one enum element
        annotations.push(b'e');
        u2(&mut annotations, 10);
        u2(&mut annotations, 11);
        out.extend_from_slice(&(annotations.len() as u32).to_be_bytes());
        out.extend_from_slice(&annotations);
        out
    }

    const UNIVERSAL_FABRIC_JSON: &[u8] = br#"{"schemaVersion":1,"id":"demo","version":"1",
        "entrypoints":{"client":["demo.ClientInit"]}}"#;
    const NEOFORGE_TOML: &[u8] = br#"modLoader = "javafml"
loaderVersion = "[1,)"
license = "MIT"
[[mods]]
modId = "demo"
version = "1"
"#;

    #[test]
    fn a_universal_jar_runs_only_its_fabric_entrypoints() {
        // zoomify's shape: Fabric declares a client entrypoint only, the
        // NeoForge half has an `@Mod` class. Fabric Loader on a server runs
        // nothing; on a client it runs `client`. The `@Mod` class is never a
        // Fabric entrypoint.
        let class = neoforge_mod_class();
        let jar = jar(&[
            ("fabric.mod.json", UNIVERSAL_FABRIC_JSON),
            ("META-INF/neoforge.mods.toml", NEOFORGE_TOML),
            ("demo/NeoMod.class", &class),
        ]);
        assert!(
            !jar.discover_entrypoints("neoforge").is_empty(),
            "the fixture's @Mod class must be discoverable for this test to mean anything"
        );
        assert!(jar.role_entrypoints(false).is_empty());
        let entries = jar.role_entrypoints(true);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "client");
        assert_eq!(entries[0].1.class, "demo.ClientInit");
    }

    #[test]
    fn a_neoforge_jar_runs_its_mod_class_on_both_sides() {
        let class = neoforge_mod_class();
        let jar = jar(&[
            ("META-INF/neoforge.mods.toml", NEOFORGE_TOML),
            ("demo/NeoMod.class", &class),
        ]);
        for client in [false, true] {
            let entries = jar.role_entrypoints(client);
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].0, "neoforge");
            assert_eq!(entries[0].1.class, "demo.NeoMod");
        }
    }

    #[test]
    fn a_client_only_mod_class_never_reaches_the_server_role() {
        // FML constructs a `@Mod(dist = Dist.CLIENT)` class only on a client;
        // fzzy_config's client class carries this shape and must not run on a
        // server.
        let class = client_only_neoforge_mod_class();
        let jar = jar(&[
            ("META-INF/neoforge.mods.toml", NEOFORGE_TOML),
            ("demo/NeoClient.class", &class),
        ]);
        assert_eq!(
            jar.discover_entrypoints_on("neoforge", "SERVER").len(),
            0,
            "a CLIENT-only @Mod class must be filtered out of the server role"
        );
        let client = jar.discover_entrypoints_on("neoforge", "CLIENT");
        assert_eq!(client.len(), 1);
        assert_eq!(client[0].class, "demo.NeoClient");
    }

    #[test]
    fn a_fabric_server_runs_main_then_server() {
        // Fabric's Hooks.startServer invokes `main` and then `server`
        // (Hooks.startClient invokes `main` and then `client`); the native role
        // must run both stages in that order, not stop at the first.
        let jar = jar(&[(
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"demo","version":"1",
                "entrypoints":{"main":["demo.Main"],"server":["demo.Server"],"client":["demo.Client"]}}"#,
        )]);
        let server: Vec<_> = jar
            .role_entrypoints(false)
            .into_iter()
            .map(|(kind, entry)| (kind, entry.class))
            .collect();
        assert_eq!(
            server,
            [
                ("main", "demo.Main".to_owned()),
                ("server", "demo.Server".to_owned())
            ]
        );
        let client: Vec<_> = jar
            .role_entrypoints(true)
            .into_iter()
            .map(|(kind, entry)| (kind, entry.class))
            .collect();
        assert_eq!(
            client,
            [
                ("main", "demo.Main".to_owned()),
                ("client", "demo.Client".to_owned())
            ]
        );
        let main_only = self::jar(&[(
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"demo","version":"1","entrypoints":{"main":["demo.Main"]}}"#,
        )]);
        assert_eq!(main_only.role_entrypoints(false).len(), 1);
        assert_eq!(main_only.role_entrypoints(false)[0].0, "main");
        assert_eq!(main_only.role_entrypoints(true).len(), 1);
    }
}
