//! The three DataFixerUpper codec nodes a NeoForge attachment serializer needs for carryon, run by
//! the runtime: `CompoundTag.CODEC`, `Codec#flatXmap(Function, Function)` and
//! `Codec#fieldOf(String)`, with `DataResult.success(Object)` and `DataResult.error(Supplier)` for
//! the two mod lambdas `flatXmap` takes.
//!
//! Everything else stays an opaque codec, as before: `RecordCodecBuilder`, the primitive codecs,
//! `xmap`, `optionalFieldOf`, `listOf`, `dispatch`... still answer the neutral `<type>#codec`
//! marker, and a serializer built on one is refused by name (`neoforge_attachments`). A
//! `DataResult` this module did not make is opaque too.
//!
//! # The behaviour, from the bytecode and a JVM oracle
//!
//! * `NeoForge`'s `AttachmentType$Builder$1.write` stores the map codec into the type's own child
//!   output (`TagValueOutput.store`: `codec.encoder().encodeStart(ops, value)`, merged into the
//!   child on success). An error is reported to the problem reporter and nothing is merged
//!   (@81–120), so the child the holder already created stays `{}`: the oracle's `encode/error`
//!   case saves `{"carryon:carry_on_data": {}}`.
//! * `read` is `input.read(codec).orElseThrow(..)`: a missing field, a field that is not a
//!   compound, or a decode lambda's error is `IllegalStateException("Unable to read attachment due
//!   to an internal codec error.")` (oracle `decode/missing-field`, `decode/int-field`,
//!   `decode/error`).
//! * `fieldOf(name)` writes `{name: <inner>}`; `CompoundTag.CODEC` passes a compound through and
//!   refuses any other tag.
//!
//! An encode or decode that made any unmodelled host call is [`Coded::Unmodelled`]: its bytes rest
//! on a neutral answer, so they never reach the store (`docs/attachments-design.md` §3.2).

use std::collections::BTreeMap;

use crate::java_nbt::{COMPOUND_TAG, Nbt, NbtCompound};
use crate::{Host, MethodRef, NativeError, Runtime, Value, expect_ref};

const CODEC: &str = "com/mojang/serialization/Codec";
const DATA_RESULT: &str = "com/mojang/serialization/DataResult";
/// The synthetic owner of a modelled `DataResult`'s hidden fields.
const RESULT_OWNER: &str = "horizon-java/data-result";
/// The marker `CompoundTag.CODEC` answers.
pub(crate) const COMPOUND_TAG_CODEC: &str = "nf-codec/compound-tag";
/// Codec nodes one jar may build. Codecs are built in class initialisers, so a jar that builds
/// more is building them in a loop: past this they stay opaque (counted).
const MAX_CODEC_NODES: usize = 4096;
/// `IAttachmentSerializer.read`'s message for every codec failure (`AttachmentType$Builder$1`'s
/// `buildException("read")`, checked against the oracle).
pub(crate) const READ_FAILED: &str = "Unable to read attachment due to an internal codec error.";

/// One modelled codec.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CodecNode {
    /// `CompoundTag.CODEC`.
    CompoundTag,
    /// `inner.flatXmap(to, from)`: decode runs `to` on the inner result, encode runs `from`
    /// first. Both answer a `DataResult`.
    FlatXmap {
        inner: String,
        to: Value,
        from: Value,
    },
    /// `inner.fieldOf(name)`, a `MapCodec` of one key.
    FieldOf { inner: String, name: String },
}

/// The modelled codecs of one jar, by marker.
#[derive(Clone, Debug, Default)]
pub(crate) struct Codecs {
    nodes: BTreeMap<String, CodecNode>,
}

impl Codecs {
    pub(crate) fn node(&self, marker: &str) -> Option<&CodecNode> {
        if marker == COMPOUND_TAG_CODEC {
            return Some(&CodecNode::CompoundTag);
        }
        self.nodes.get(marker)
    }

    /// Whether `marker` is a `MapCodec` this module models (a `fieldOf` node).
    pub(crate) fn is_map_codec(&self, marker: &str) -> bool {
        matches!(self.node(marker), Some(CodecNode::FieldOf { .. }))
    }

    fn add(&mut self, node: CodecNode) -> Option<String> {
        if self.nodes.len() >= MAX_CODEC_NODES {
            return None;
        }
        let marker = format!("nf-codec/{}", self.nodes.len());
        self.nodes.insert(marker.clone(), node);
        Some(marker)
    }

    /// The lambdas every node holds: roots of the jar's heap collection.
    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        for node in self.nodes.values() {
            if let CodecNode::FlatXmap { to, from, .. } = node {
                for value in [to, from] {
                    if let Value::Ref(Some(text)) = value {
                        out.push(text);
                    }
                }
            }
        }
    }
}

/// What running a modelled codec gave.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Coded<T> {
    Ok(T),
    /// A `DataResult.error`, a missing field, a wrong tag type, or a Java exception the codec's
    /// own lambdas threw: what NeoForge reports (encode) or throws as [`READ_FAILED`] (decode).
    Failed(String),
    /// Something outside the model: an opaque codec or `DataResult`, a value that is not a
    /// modelled tag, an unmodelled host call, an interpreter refusal. Never stored.
    Unmodelled(String),
}

fn result_key(marker: &str, name: &str) -> String {
    format!("{marker}|{RESULT_OWNER}#{name}")
}

impl<H: Host> Runtime<'_, H> {
    /// `CompoundTag.CODEC`'s `flatXmap` and `fieldOf`, `DataResult.success` and `DataResult.error`.
    /// `codecs` is the jar's codec table.
    pub(crate) fn invoke_attachment_codec(
        &mut self,
        codecs: &mut Codecs,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Option<Value> {
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (
                DATA_RESULT,
                "success",
                "(Ljava/lang/Object;)Lcom/mojang/serialization/DataResult;",
            ) => Some(self.data_result("success", args.first().cloned()?)),
            (
                DATA_RESULT,
                "error",
                "(Ljava/util/function/Supplier;)Lcom/mojang/serialization/DataResult;",
            ) => Some(self.data_result("error", args.first().cloned()?)),
            (
                CODEC,
                "flatXmap",
                "(Ljava/util/function/Function;Ljava/util/function/Function;)Lcom/mojang/serialization/Codec;",
            ) => {
                let inner = receiver.and_then(expect_ref)?;
                // A `MapCodec` has its own `flatXmap`, answering a `MapCodec`; this is `Codec`'s.
                if codecs.is_map_codec(inner) {
                    return None;
                }
                codecs.node(inner)?;
                let node = CodecNode::FlatXmap {
                    inner: inner.clone(),
                    to: args.first().cloned()?,
                    from: args.get(1).cloned()?,
                };
                codecs.add(node).map(|marker| Value::Ref(Some(marker)))
            }
            (CODEC, "fieldOf", "(Ljava/lang/String;)Lcom/mojang/serialization/MapCodec;") => {
                let inner = receiver.and_then(expect_ref)?;
                // `fieldOf` is a `Codec` method: a `MapCodec` has no `fieldOf`.
                if codecs.is_map_codec(inner) {
                    return None;
                }
                codecs.node(inner)?;
                let name = args.first().and_then(expect_ref)?.clone();
                let node = CodecNode::FieldOf {
                    inner: inner.clone(),
                    name,
                };
                codecs.add(node).map(|marker| Value::Ref(Some(marker)))
            }
            _ => None,
        }
    }

    /// A modelled `DataResult`: a heap object, so per-encode results are collected like any other
    /// object a jar allocates.
    fn data_result(&mut self, kind: &str, value: Value) -> Value {
        let object = self.new_object(DATA_RESULT);
        if let Some(marker) = expect_ref(&object).cloned() {
            self.fields
                .insert(result_key(&marker, "kind"), Value::Ref(Some(kind.into())));
            self.fields.insert(result_key(&marker, "v"), value);
        }
        object
    }

    /// `Ok(value)` for a modelled success, `Err(None)` for a modelled error, `Err(Some(why))` for
    /// anything else.
    fn read_data_result(&self, result: &Value) -> Result<Value, Option<String>> {
        let Some(marker) = expect_ref(result) else {
            return Err(Some("a codec lambda answered null".into()));
        };
        match self.fields.get(&result_key(marker, "kind")) {
            Some(Value::Ref(Some(kind))) if kind == "success" => Ok(self
                .fields
                .get(&result_key(marker, "v"))
                .cloned()
                .unwrap_or(Value::Ref(None))),
            Some(Value::Ref(Some(kind))) if kind == "error" => Err(None),
            _ => Err(Some(format!(
                "a codec lambda answered a DataResult this runtime does not model ({marker})"
            ))),
        }
    }

    /// Runs one mod lambda of a codec. A Java exception it throws is the codec failing; any other
    /// interpreter error is outside the model.
    fn codec_lambda(&mut self, lambda: &Value, argument: Value) -> Coded<Value> {
        match self.invoke_callback(lambda, std::slice::from_ref(&argument)) {
            Ok(Some(result)) => match self.read_data_result(&result) {
                Ok(value) => Coded::Ok(value),
                Err(None) => Coded::Failed("the codec answered DataResult.error".into()),
                Err(Some(why)) => Coded::Unmodelled(why),
            },
            Ok(None) => Coded::Unmodelled("a codec lambda the runtime cannot run".into()),
            Err(NativeError::UncaughtException { class, message, .. }) => Coded::Failed(format!(
                "{}{}",
                class.replace('/', "."),
                message.map(|m| format!(": {m}")).unwrap_or_default()
            )),
            Err(other) => Coded::Unmodelled(other.to_string()),
        }
    }

    fn encode_node(&mut self, marker: &str, value: &Value) -> Coded<Nbt> {
        match self.codec_node(marker) {
            Some(CodecNode::CompoundTag) => match expect_ref(value) {
                Some(tag) if self.nbt_kind(tag) == Some("compound") => match self.nbt_read(tag) {
                    Ok(tree) => Coded::Ok(tree),
                    Err(why) => Coded::Unmodelled(why),
                },
                // `CompoundTag.CODEC` encoding a null or a non-compound is a JVM exception
                // (a cast inside the codec); which one depends on the value, so it is not guessed.
                _ => Coded::Unmodelled(format!(
                    "CompoundTag.CODEC asked to encode {value:?}, not a modelled CompoundTag"
                )),
            },
            Some(CodecNode::FlatXmap { inner, from, .. }) => {
                match self.codec_lambda(&from, value.clone()) {
                    Coded::Ok(mapped) => self.encode_node(&inner, &mapped),
                    Coded::Failed(why) => Coded::Failed(why),
                    Coded::Unmodelled(why) => Coded::Unmodelled(why),
                }
            }
            Some(CodecNode::FieldOf { inner, name }) => match self.encode_node(&inner, value) {
                Coded::Ok(tag) => Coded::Ok(Nbt::Compound(NbtCompound::from([(name, tag)]))),
                other => other,
            },
            None => Coded::Unmodelled(format!("codec {marker} is not modelled")),
        }
    }

    fn decode_node(&mut self, marker: &str, tag: &Nbt) -> Coded<Value> {
        match self.codec_node(marker) {
            Some(CodecNode::CompoundTag) => match tag {
                Nbt::Compound(_) => Coded::Ok(self.nbt_alloc(tag)),
                _ => Coded::Failed("Not a compound tag".into()),
            },
            Some(CodecNode::FlatXmap { inner, to, .. }) => match self.decode_node(&inner, tag) {
                Coded::Ok(decoded) => self.codec_lambda(&to, decoded),
                other => other,
            },
            Some(CodecNode::FieldOf { inner, name }) => match tag {
                Nbt::Compound(map) => match map.get(&name) {
                    Some(field) => self.decode_node(&inner, field),
                    None => Coded::Failed(format!("No key {name} in MapLike")),
                },
                _ => Coded::Failed("Not a map".into()),
            },
            None => Coded::Unmodelled(format!("codec {marker} is not modelled")),
        }
    }

    /// Encodes `value` with the map codec `marker` into the fields NeoForge's `write` merges into
    /// the type's child, counting any unmodelled host call as [`Coded::Unmodelled`].
    pub(crate) fn encode_map_codec(&mut self, marker: &str, value: &Value) -> Coded<NbtCompound> {
        let before = self.attachment_unknown_total();
        let coded = self.encode_node(marker, value);
        if self.attachment_unknown_total() > before {
            return Coded::Unmodelled("the encode made an unmodelled host call".into());
        }
        match coded {
            Coded::Ok(Nbt::Compound(fields)) => Coded::Ok(fields),
            Coded::Ok(_) => Coded::Unmodelled("a map codec encoded to a non-map".into()),
            Coded::Failed(why) => Coded::Failed(why),
            Coded::Unmodelled(why) => Coded::Unmodelled(why),
        }
    }

    /// Decodes the type's saved child with the map codec `marker`.
    pub(crate) fn decode_map_codec(&mut self, marker: &str, child: &NbtCompound) -> Coded<Value> {
        let before = self.attachment_unknown_total();
        let coded = self.decode_node(marker, &Nbt::Compound(child.clone()));
        if self.attachment_unknown_total() > before {
            return Coded::Unmodelled("the decode made an unmodelled host call".into());
        }
        coded
    }
}

/// The `CompoundTag.CODEC` static, for the host's static-field chain.
pub(crate) fn codec_static(r: &MethodRef) -> Option<Value> {
    (r.owner == COMPOUND_TAG
        && r.name == "CODEC"
        && r.descriptor == "Lcom/mojang/serialization/Codec;")
        .then(|| Value::Ref(Some(COMPOUND_TAG_CODEC.into())))
}
