# NeoForge InterModComms fixture

Two jars that talk over FML's `InterModComms`: `mod/imcsend/` (mod `sender`) and `mod/imcrecv/`
(mod `receiver`). The sender queues messages from its constructor, its `InterModEnqueueEvent`
listener, a deferred task and its `RegisterPayloadHandlersEvent` listener; its suppliers print when
they run. The receiver reads them with and without a method filter in `InterModProcessEvent`, calls
some suppliers twice, leaves one message unread until the payload step, and sends one back.
`neoforge_imc_tests.rs` compiles each jar with `javac` against compile-only stubs (`stubs/`, the
lifecycle fixture's `stubs/` and `../neoforge_bus/bus-stubs/`) and loads both on one IMC bus the way
the game's content pass does.

`oracle.jsonl` is the same two mods on the real loader-12.0.12 classes (`InterModComms`, `ModList`,
`ModLoadingContext`, `DeferredWorkQueue`, the lifecycle events) and NeoForge 26.3.0.101, on OpenJDK
25.0.5+8. `oracle/ImcOracle.java` builds the `ModList` with `ModList.of` and FML's package-private
`setLoadedMods` (`minecraft`, `sender`, `receiver`), and runs the loading steps as
`neoforge_lifecycle/oracle/LifecycleOracle.java` does, setting the active container around each
construction and post as `ModLoader.lambda$dispatchParallelTask$3` does. `== <step>` starts each step.

Regenerate (the lifecycle fixture's jars plus `com.mojang:logging` 1.7.12 and `slf4j-api` 2.0.17,
which `ModList.of` reaches through `CrashReportCallables`):

```sh
CP=$(ls libs/*.jar | tr '\n' :)
javac -d stubbed $(find ../neoforge_bus/bus-stubs ../neoforge_lifecycle/stubs stubs mod -name '*.java')
mkdir run && cp -r stubbed/imcsend stubbed/imcrecv run/
javac -d run -cp run:$CP oracle/ImcOracle.java
$JAVA25/bin/java -cp run:$CP ImcOracle 2>/dev/null > oracle.jsonl
```
