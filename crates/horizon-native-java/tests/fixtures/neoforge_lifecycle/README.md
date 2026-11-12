# NeoForge loading-stage fixture

`mod/life/` is a jar with two mods (`life`, `second`) that listen to every loading stage NeoForge
posts on a dedicated server; each listener and each piece of deferred work prints its name.
`neoforge_lifecycle_tests.rs` compiles it with `javac` against compile-only stubs (`stubs/`, plus
`../neoforge_bus/bus-stubs/` and that fixture's `RegisterCommandsEvent` stand-in), runs the `@Mod`
constructors, injects the `@EventBusSubscriber`, and runs every `LoadingStage` with its deferred work.

`oracle.jsonl` is the same mods on the real loader-12.0.12 and NeoForge 26.3.0.101 classes, on
OpenJDK 25.0.5+8 (the Java 26.3 runs mods on): real `ModContainer#acceptEvent`, real lifecycle event
classes and `DeferredWorkQueue`, a mod bus built as `FMLModContainer.<init>` @31–62 builds one.
The steps themselves (`CommonModLoader.begin`/`load`, `ModLoader.dispatchParallelEvent`,
`ModLoader.postEvent`) need a running `FMLLoader`, so `oracle/LifecycleOracle.java` runs that
sequence as their bytecode does; what each step runs, in which order, and when the deferred work
runs is the real classes' answer. `!post-failed <mod>` marks a post a listener ended by throwing,
`!work-failed` a step whose deferred work threw.

Regenerate (jars from a NeoForge 26.3.0.101 install's `libraries/`: bus 8.0.5, loader 12.0.12,
neoforge universal 26.3.0.101, typetools 0.6.3, log4j-api 2.26.0, asm 9.11, mergetool 2.0.7 api,
maven-artifact 3.10.0, guava 33.6.0-jre). The mod compiles against the stubs, whose descriptors are
the real ones (the real `PayloadRegistrar` needs Minecraft and Netty on the compile path), and runs
on the real jars:

```sh
CP=$(ls libs/*.jar | tr '\n' :)
javac -d stubbed $(find ../neoforge_bus/bus-stubs stubs mod -name '*.java') \
    ../neoforge_bus/events/net/neoforged/neoforge/event/RegisterCommandsEvent.java
mkdir -p run/net/neoforged/neoforge/event && cp -r stubbed/life run/ \
    && cp stubbed/net/neoforged/neoforge/event/RegisterCommandsEvent.class run/net/neoforged/neoforge/event/
javac -d run -cp run:$CP oracle/LifecycleOracle.java
$JAVA25/bin/java -cp run:$CP LifecycleOracle 2>/dev/null > oracle.jsonl
```
