//! `Files.readAttributes(path, BasicFileAttributes.class)`, `Files.isDirectory`
//! / `isRegularFile` and `File.isDirectory` / `isFile` over the file model
//! (`file_attributes.rs`).
//!
//! `fixtures/read_attributes/Probe.java`'s `run`, run in an empty directory on
//! JDK 25 (`jdk-25.0.5+8`, elysium), printed the string `run` asserts.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/read_attributes/Probe.java");

#[test]
fn file_kinds_and_read_attributes_answer_like_jdk_25() {
    let fixture = Fixture::build("read-attributes", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "false,true,false,false|false,true,false,false|true,false,false,false|\
         true,false,false,false|false/true,false/true,true/false,true/false,false/false|\
         true,false,false,true|NoSuchFileException:missing.toml|\
         NoSuchFileException:d/missing/x|NullPointerException:null|\
         UnsupportedOperationException:null|\
         NullPointerException:Cannot invoke \"java.nio.file.Path.getFileSystem()\" because \"path\" is null|\
         NullPointerException:Cannot invoke \"java.nio.file.Path.getFileSystem()\" because \"path\" is null|\
         false|false/false,NoSuchFileException:f|false/false,true/false|\
         true,false,false,false,true/false|1"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

/// A file's size (the model keeps text, not bytes or a charset), its times and
/// its inode are not the model's to know: each is counted, never answered.
#[test]
fn sizes_times_and_file_keys_are_counted() {
    let fixture = Fixture::build("read-attributes-counted", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "counted"), "true");
    let calls = host.unknown_host_calls();
    for member in [
        "java/nio/file/attribute/BasicFileAttributes#size()J",
        "java/nio/file/attribute/BasicFileAttributes#lastModifiedTime()Ljava/nio/file/attribute/FileTime;",
        "java/nio/file/attribute/BasicFileAttributes#fileKey()Ljava/lang/Object;",
    ] {
        assert!(
            calls
                .iter()
                .any(|(call, count)| call == member && *count == 1),
            "{member} not counted once: {calls:?}"
        );
    }
    assert_eq!(host.unknown_host_call_count(), 3, "{calls:?}");
}
