# NeoForge attachment fixtures

* `oracle.jsonl` is the output of a JVM oracle run against the real 26.3 jars
  (`minecraft-server-patched-26.3.0.101.jar`, `neoforge-26.3.0.101-universal.jar`, DFU 10.0.21) on
  OpenJDK 27, one JSON line per case. Only the output is committed. The oracle built a carryon-shaped
  type (`AttachmentType.builder(..).serialize(CompoundTag.CODEC.flatXmap(..).fieldOf(..)).build()`), took
  NeoForge's own serializer from it by reflection, and recorded:
  * `encode/*`: `serializer.write(value, root.child("carryon:carry_on_data"))` on a
    `TagValueOutput.createWithContext(collector, RegistryAccess.EMPTY)`, exactly as
    `AttachmentHolder#serializeAttachments` calls it, and `root.buildResult()`;
  * `decode/*`: `serializer.read(null, TagValueInput.create(collector, RegistryAccess.EMPTY, child))`;
  * `compound/*`: the `CompoundTag` methods the slice models, on every tag type;
  * `builder/checks`: the builder's exceptions and messages;
  * `deferred/key`, `registry/key`: the `ResourceKey` texts in the unbound-holder message.
  * `holder/calls` could not run: `AttachmentHolder`'s class initialiser needs FML
    (`ExceptionInInitializerError`), so the holder-call tests take their expectations from the
    bytecode of `AttachmentHolder` and `IAttachmentHolder`, with offsets in each test's doc.

  Tags are `{"t": <type>, "v": <value>}`; floats and doubles are `{"t": .., "bits": <raw IEEE bits>}`.
  The one identity hash in `encode/error`'s report text is replaced by `<identity hash>`.
* `stubs/` are compile-only stand-ins for the game, NeoForge and DFU signatures the fixture mod calls.
  They are never put in the fixture jar: every call to them reaches the runtime's host, as a real jar's
  calls do.
* `mod/` is the fixture mod: a carryon-shaped attachment (`Data`, its whole state one `CompoundTag`) and
  one type per case the tests need.
