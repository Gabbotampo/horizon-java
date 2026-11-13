# P8 host objects fixture (#156)

`demo/HostEvents.java` probes the loader event objects the runtime keeps as host handles
(`host_events.rs`): NeoForge's buses, Fabric's array-backed events and their invokers, IMC messages
and suppliers, the future `enqueueWork` answers, and a cancellable event. `host_events_tests.rs`
compiles it with `javac` against the compile-only `stubs/`, ships only `demo/*`, and hands each probe
the runtime's handles.

The expected answers are `oracle/HostEventsOracle.java` passing the real objects on OpenJDK 25.0.5+8:
NeoForge 26.3.0.101's `NeoForge.EVENT_BUS`, a bus-8.0.5 mod bus, loader-12.0.12's `IMCMessage`
records and `DeferredWorkQueue#enqueueWork` futures, fabric-api-base 2.0.6's `EventFactory` (whose
`Event` needs Minecraft 26.3's `Identifier`), fabric-lifecycle-events-v1 4.1.9's
`ServerLifecycleEvents`. Regenerate:

```sh
CP=bus-8.0.5.jar:loader-12.0.12.jar:neoforge-26.3.0.101-universal.jar:typetools-0.6.3.jar:log4j-api-2.26.0.jar:asm-9.11.jar:mergetool-2.0.7-api.jar:fabric-api-base-2.0.6+fcdff87f5d.jar:fabric-lifecycle-events-v1-4.1.9+ffef5f675d.jar:minecraft-26.3-client.jar
javac --release 21 -d out -cp $CP demo/HostEvents.java oracle/HostEventsOracle.java
$JAVA25/bin/java -cp out:$CP HostEventsOracle
```
