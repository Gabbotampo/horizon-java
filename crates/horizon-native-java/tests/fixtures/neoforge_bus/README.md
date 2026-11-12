# NeoForge event-bus fixture

`mod/demo/` registers listeners on NeoForge's buses in every `IEventBus#addListener` overload; each
listener prints its name when it runs. `neoforge_bus_tests.rs` compiles it with `javac` against
compile-only stubs (`bus-stubs/`, with bus-8.0.5's signatures, and `events/`), puts only `demo/*` in
the jar, runs it in the runtime and fires the events through the game's dispatch.

`oracle.jsonl` is the same fixture on the real bus: `oracle/BusOracle.java` compiled against the real
jars instead of `bus-stubs/` (so the fixture's call sites are bus-8.0.5's own descriptors), on
OpenJDK 25.0.5+8 (the Java 26.3 runs mods on). Its game bus is NeoForge 26.3.0.101's own
`NeoForge.EVENT_BUS`; a mod bus is built as `FMLModContainer.<init>` @31–62 builds it. `events/`
stands in for NeoForge's event classes (same names, hierarchy, abstractness, `ICancellableEvent` and
`IModBusEvent`), which cannot load without the game.

Regenerate (jars from a NeoForge 26.3.0.101 install's `libraries/`: `net/neoforged/bus/8.0.5`,
`net/neoforged/fancymodloader/loader/12.0.12`, `net/neoforged/neoforge/26.3.0.101` (universal),
`net/jodah/typetools/0.6.3`, `org/apache/logging/log4j/log4j-api/2.26.0`, `org/ow2/asm/asm/9.11`,
`net/neoforged/mergetool/2.0.7` (api, for `Dist`)):

```sh
CP=bus-8.0.5.jar:loader-12.0.12.jar:neoforge-26.3.0.101-universal.jar:typetools-0.6.3.jar:log4j-api-2.26.0.jar:asm-9.11.jar:mergetool-2.0.7-api.jar
javac -d out -cp $CP $(find events mod oracle -name '*.java')
$JAVA25/bin/java -cp out:$CP BusOracle 2>/dev/null > oracle.jsonl
```

`@EventBusSubscriber` (`AutoSubscribers`, `SkippedSubscribers`, `InjectRefused`): the real
`AutomaticEventSubscriber.inject` needs a running loader (`FMLLoader.getCurrent()`), so the oracle's
`inject` follows its bytecode (`lambda$inject$4`: modid, side, `Class.forName`, the static and
parameter checks, `IModBusEvent` → mod bus) and registers each method with the real bus's
`register(Method)`. The messages of the static and parameter checks are FML's string templates
around the JDK's own `Method#toString`; the abstract-class refusal is the real bus's.
