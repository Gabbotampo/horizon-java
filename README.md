# horizon-java

A bounded, JVM-free Java-mod runtime for Minecraft: Java Edition mod jars, extracted from
[Horizon](https://github.com/Solenopsisbot/horizon).

It is what is left when you strip Horizon's game code away: a Rust classfile interpreter, a
Fabric/NeoForge/legacy-Forge metadata reader, a Mixin planner and classfile weaver, and an optional
JNI reference runner that drives the real Fabric Loader / NeoForge FancyModLoader so measurements
have an oracle.

## What it is

- `horizon-native-java` — parses real classfiles and loader metadata, discovers entrypoints
  (`onInitialize`, `@Mod`, legacy `@Mod`), interprets a bounded bytecode subset with no JVM, and
  plans/weaves the Mixin subset it understands. Calls it cannot model cross an explicit `Host`
  boundary; unsupported opcodes report class, method and program counter instead of guessing.
- `horizon-jvm` — an optional development oracle. It starts a real JVM through JNI, runs Fabric
  Loader (Knot) or NeoForge FML against a generated façade, and reports what fails. It is never a
  silent fallback in Horizon, and nothing here requires a JVM to build or to run the native engine.

Both crates depend only on `serde_json`, `zip`, `jni` (and `libc` on Unix). No Horizon crate is
linked; the game supplies the `Host` implementation.

## What it is not

- **Not a full JVM.** It implements a bounded subset: no general `invokedynamic`, no arbitrary
  reflection, no dynamic class loading, no threads or monitors beyond the modelled shapes.
- **Not a drop-in Fabric/NeoForge loader.** It reads loader metadata and executes bounded
  entrypoints; a real loader's full lifecycle, remapping and every Mixin injector are out of scope.
- **Not a Minecraft distribution.** No game classes, no Mojang files, no third-party jars are in
  this repository. Anything that needs them reads them from a Horizon install or a corpus you
  provide.

## Measured status

These are the numbers in [docs/java-mod-compat-status.md](docs/java-mod-compat-status.md), measured
on 8 November 2026 at commit `8a3c8dc59` (one fresh dedicated server per jar). They describe how far
the runtime is from the two gates, not a claim that the corpus runs.

| Corpus | Jars | `native-ok` | `needs-loader` | `native-gap` |
|---|---:|---:|---:|---:|
| wide (`corpus-xl`) | 223 | 182 (82%) | 38 (17%) | 3 (1%) |
| mega (500 unique) | 500 | 416 (83%) | 73 (15%) | 11 (2%) |
| NeoForge 26.3 | 85 | 51 | 20 | 14 |

- **With declared dependencies** loaded beside each refused jar, `native-gap` is 3 and 6.
- **Phase 2 score** (game-hook tier on): 157 of 174 `needs-loader` rows (150 unique jars) run
  natively; 33 are loader-free, 124 still need the loader and 17 were refused (from 30 / 94 / 50
  on 6 November).
- **Mixins:** the one-jar runs weave no game-class mixins (4,891 and 10,099 plans skipped,
  overwhelmingly `target-outside-jar`); the opt-in hook tier binds plans to game hook sites instead.
  Game-class mixins remain the main phase-2 gap.

Phase 1 is not met yet: neither Fabric ledger has 0 `native-gap`, and the NeoForge ledger is
measured but not gated. See the status page for per-row detail and repro commands.

## Layout

```
crates/horizon-native-java/   the no-JVM interpreter, metadata reader, Mixin plan/weave
crates/horizon-jvm/           the optional JNI reference runner (Fabric Knot / NeoForge FML)
native-hooks.json             the hook-point census the native crate's tests pin against
tools/                        corpus drivers and the vendored `lab` helpers they use
tools/real_mod_fixtures/      committed ledgers, score history and the NeoForge corpus lock
tests/                        Python unit tests for the corpus tools
docs/                         design and measured status, carried over from Horizon
```

## Build and test

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 -m unittest discover -s tests
```

Rust tests that compile fixtures need `javac`/`jar`; tests that read a real mod corpus or a
Minecraft/loader install skip when the corresponding files or environment variables
(`HORIZON_JAVA_CORPUS_DIR`, `HORIZON_MINECRAFT_JAR`, `FABRIC_LOADER_JAR`, `NEOFORGE_SERVER_DIR`,
... ) are absent. No test downloads a jar.

## Corpus tools

The drivers were written against Horizon and are kept here so the measurements can be rerun. They
need a Horizon build with the Java-mod flag (`horizon-client --java-mod`), prepared resources, and
jars under `~/horizon-corpus`; they do not install either:

```sh
python3 tools/java_mod_server_ledger.py ~/horizon-corpus/corpus-xl \
  --binary <horizon>/target/fast/horizon-client \
  --native-binary <horizon>/target/fast/horizon-native-java \
  --resources <horizon>/.local/horizon-client/26.3 \
  --timeout 60 --json --output ledger.txt

python3 tools/hybrid_pack.py --native-score --docs docs/java-mod-compat-status.md
python3 tools/neoforge_corpus.py lock --limit 80
```

`tools/lab.py` is a deliberate two-function vendored subset (`cargo_target`, `game_env`), not
Horizon's full harness library.

## Game-side integration

The Horizon-side integration (the `horizon-client` mod host, its registries and its hook sites)
lives in the Horizon repository, not here, and is being redesigned against the Horizon mod API.
The docs in this repository still name those game paths; their header explains where they point.

## Licence

MIT. See [LICENSE](LICENSE).
