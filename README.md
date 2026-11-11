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

| Corpus | Jars | `ran-clean` | `ran-hollow` | `client-only-untested` | `library` | `needs-loader` | `native-gap` |
|---|---:|---:|---:|---:|---:|---:|---:|
| wide (`corpus-xl`) | 223 | 0 | 58 | 103 | 21 | 38 (17%) | 3 (1%) |
| mega (500 unique) | 500 | 0 | 116 | 262 | 38 | 73 (15%) | 11 (2%) |
| NeoForge 26.3 | 85 | 0 | 40 | 6 | 5 | 20 | 14 |

The four non-gap labels are what the retired `native-ok` meant: the server did not refuse the jar. Only
`ran-clean` says the server ran the entrypoint with no unmodelled host call; `ran-hollow` ran but leaned on
neutral answers (or nobody recorded the count), `client-only-untested` and `library` were skipped and say
nothing about whether the mod works. The committed ledgers predate the host-call count, so every `ran` row is
`ran-hollow` with `unknown_source=none` until the next measurement.

- **With declared dependencies** loaded beside each refused jar, `native-gap` is 3 and 6.
- **Phase 2 score** (game-hook tier on): 157 of 174 `needs-loader` rows (150 unique jars) run
  natively; 33 are loader-free, 124 still need the loader and 17 were refused (from 30 / 94 / 50
  on 6 November).
- **Mixins:** the one-jar runs weave no game-class mixins (4,891 and 10,099 plans skipped,
  overwhelmingly `target-outside-jar`); the opt-in hook tier binds plans to game hook sites instead.
  Game-class mixins remain the main phase-2 gap.

- **Entrypoint census** (`tools/unmodelled_calls.py`, the native probe over the same corpora, every server
  stage, the game's verdict), re-measured on 11 November 2026 with the interpreter-honesty fixes
  (#37, #38, #41): `ran-clean` **174 → 112** (wide 52 → 31, mega 86 → 59, NeoForge 36 → 22), 18 jars
  `ran-partial`, and 132 more refused, almost all on a `NullPointerException` the interpreter used to
  carry on through. Lower is correct: those jars were clean only because failures were answered
  silently. Detail in [docs/unmodelled-calls.md](docs/unmodelled-calls.md).

Phase 1 is not met yet: neither Fabric ledger has 0 `native-gap`, and the NeoForge ledger is
measured but not gated. See the status page for per-row detail and repro commands.

## Layout

```
crates/horizon-native-java/   the no-JVM interpreter, metadata reader, Mixin plan/weave
crates/horizon-jvm/           the optional JNI reference runner (Fabric Knot / NeoForge FML)
mod/                          the Horizon native mod that runs the runtime inside the game
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

## Mod

`mod/` is a Horizon **native mod** (`docs/mod-plan.md`): the first way this runtime runs inside the
game instead of in the probe. It implements the game's typed `Hooks` from the mod API on
[arismoko/horizon](https://github.com/arismoko/horizon) (crate `horizon-mod-api`, library
`mod_api`), pinned to commit `cf0113879ebee600bff86070466f75b2db90b40a` in `mod/horizon-mod.toml`.

What it does today:

- **M0, the shell.** `server_about_to_start` and `client_setup` log one line each, and the server
  start writes this mod's report, `config/horizon_java/java-runtime.json`, through the mod API's own
  config service (`Host::config_files`). With no jars to run it is `{"schema":1,"jars":[]}`.
- **M1, run jars.** Every `.jar` in `config/horizon_java/java-mods/` is read as bytes through the
  same service and run at `server_about_to_start` with its own `StdHost`. One log line per jar and
  one row in the report: the id, `ran-clean` / `ran-hollow` / `refused`, the host-call and
  unknown-host-call counts, and the refusal reason.
- **M2, game hooks.** Each jar's runtime (heap, statics and recorded listeners) stays alive for the
  server, and this mod's `server_about_to_start` / `server_starting` / `server_started` /
  `server_stopping` / `server_stopped` hooks fire the Fabric `ServerLifecycleEvents` and NeoForge
  `ServerStartingEvent`/`ServerStartedEvent`/`ServerStoppingEvent` listeners the entrypoints
  registered. Java commands from Fabric's `CommandRegistrationCallback` (NeoForge's
  `RegisterCommandsEvent`) become typed mod API commands; running one calls its Java
  `Command.run` and sends the reply through the mod API's command-source feedback. Literal,
  `word`/`text` and `integer` nodes only: anything else is refused by name and logged.

A native mod is one flat crate, so the runtime is copied into `mod/src/runtime/` by
`tools/vendor.py` (which rewrites `crate::` to `crate::runtime::` and records a SHA-256 per file in
`mod/vendor.lock`). The copy is committed, so a clean clone builds, and it carries the runtime commit
the lock records: `python3 tools/vendor.py --check-lock` always fails on a hand edit to it, while
`--check` (the mod is current with the runtime source) is required of a PR that touches `mod/` and is
run by CI only for those. A mod PR refreshes the copy with `python3 tools/vendor.py`.

```sh
python3 tools/vendor.py             # after changing crates/horizon-native-java
python3 tools/java_mod_fixture.py --out DIR                 # milestone 1's jar
python3 tools/java_mod_fixture.py --kind lifecycle --out DIR # SERVER_STARTED/STOPPING lines
python3 tools/java_mod_fixture.py --kind command --out DIR   # /javahello [name]
python3 tools/java_mod_fixture.py --kind hooks --out DIR     # the M2c event listeners
python3 tools/java_mod_fixture.py --kind m2d --out DIR       # AFTER_DEATH and a click's hand
```

The documented build is Horizon's launcher (`horizon-launcher native-build mod/ --server-output OUT
--trust-native-code`); it needs an installed, prepared Horizon version to reuse the game's files
from, so `docs/mod-plan.md` records both that and the equivalent staged build it was proved with,
the exact run commands, the game log lines, and the next milestones (game bindings over the mod
API's hooks, then registries, then mixins as hook rows).

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

The game side lives in the Horizon repository, not here: its registries, its hook sites and the mod
API `mod/` implements. `docs/mod-plan.md` says which parts of it the mod uses today and which are
next; the older docs in this repository still name Horizon's pre-mod-API paths, and their header
explains where those point.

## Licence

MIT. See [LICENSE](LICENSE).
