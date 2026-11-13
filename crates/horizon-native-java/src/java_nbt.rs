//! Vanilla NBT (`net.minecraft.nbt`) as far as NeoForge attachments need it: a pure tree
//! ([`Nbt`]) the mod converts to and from the game's own `Compound`, and the `CompoundTag`
//! object a jar builds and changes, modelled on the jar's heap.
//!
//! # Why heap objects
//!
//! A Java `CompoundTag` is a mutable object with identity: carryon keeps one inside its
//! attachment value and its setters `putInt` into it in place, and `getCompoundOrEmpty` hands back
//! the live child, so a change through the child shows in the parent. A by-value tree would lose
//! that. So a compound here is an ordinary heap object (`net/minecraft/nbt/CompoundTag#N`, made by
//! [`Runtime::new_object`]) whose entries are hidden instance fields on the synthetic owner
//! [`MODEL_OWNER`]. The resident heap (`heap.rs`) then keeps, persists and collects tags exactly as
//! it does every other object a jar builds, with its bounds, and nothing here needs a table of its
//! own.
//!
//! * a compound has `kind = "compound"` and one field `e:<hex key>` per entry, holding the entry's
//!   tag marker (the key is hex so a `|` or `#` in it can never confuse the heap's field keys);
//! * a list has `kind = "list"`, `len`, and `i:<n>` per element;
//! * every other tag is an immutable object with `kind` and `v`, as vanilla's are (a primitive
//!   tag's `copy()` answers itself). Floats and doubles keep their exact bits.
//!
//! # What is modelled
//!
//! The `CompoundTag` methods carryon's attachment reaches, each read from the 26.3 server jar's
//! bytecode (`javap -c -p net.minecraft.nbt.CompoundTag`) and checked against a JVM oracle
//! (`tests/fixtures/attachments/oracle.jsonl`): `<init>()`, `copy()`, `contains(String)`,
//! `get(String)`, `put(String, Tag)`, `putString`, `putBoolean`, `putInt`, `getStringOr`,
//! `getBooleanOr` (which is `getByteOr(..) != 0`, so a numeric tag of any type is narrowed the way
//! its `byteValue()` does), `getIntOr` and `getCompoundOrEmpty`. Every other `CompoundTag` or `Tag`
//! method, and any call on a tag this module did not build, falls through to the host and is
//! counted as unmodelled, exactly as before.

use std::collections::BTreeMap;

use crate::{Host, MethodRef, NativeError, Runtime, Value, expect_ref};

/// The synthetic class every hidden NBT field is declared on. No jar can declare it (`-` is not a
/// legal binary-name character), so a jar's own fields never collide with these.
pub(crate) const MODEL_OWNER: &str = "horizon-java/nbt";
/// `CompoundTag`'s internal name.
pub(crate) const COMPOUND_TAG: &str = "net/minecraft/nbt/CompoundTag";
const TAG: &str = "net/minecraft/nbt/Tag";
/// NBT's own nesting limit (`NbtAccounter`'s 512): a deeper tree is refused rather than walked.
pub const MAX_DEPTH: usize = 512;

/// One NBT value, as the game's `horizon_core::nbt::Tag` holds it (without `End`). Floats and
/// doubles are their raw IEEE bits, so a NaN payload survives a round trip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Nbt {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    ByteArray(Vec<u8>),
    String(String),
    List(Vec<Nbt>),
    Compound(NbtCompound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

/// A compound's entries. Vanilla keeps a `HashMap`, so order carries no meaning; a sorted map
/// makes comparisons and the bridge deterministic.
pub type NbtCompound = BTreeMap<String, Nbt>;

impl Nbt {
    /// The vanilla class a tag of this kind is.
    fn class(&self) -> &'static str {
        match self {
            Self::Byte(_) => "net/minecraft/nbt/ByteTag",
            Self::Short(_) => "net/minecraft/nbt/ShortTag",
            Self::Int(_) => "net/minecraft/nbt/IntTag",
            Self::Long(_) => "net/minecraft/nbt/LongTag",
            Self::Float(_) => "net/minecraft/nbt/FloatTag",
            Self::Double(_) => "net/minecraft/nbt/DoubleTag",
            Self::ByteArray(_) => "net/minecraft/nbt/ByteArrayTag",
            Self::String(_) => "net/minecraft/nbt/StringTag",
            Self::List(_) => "net/minecraft/nbt/ListTag",
            Self::Compound(_) => COMPOUND_TAG,
            Self::IntArray(_) => "net/minecraft/nbt/IntArrayTag",
            Self::LongArray(_) => "net/minecraft/nbt/LongArrayTag",
        }
    }

    /// The `kind` field's text.
    fn kind(&self) -> &'static str {
        match self {
            Self::Byte(_) => "byte",
            Self::Short(_) => "short",
            Self::Int(_) => "int",
            Self::Long(_) => "long",
            Self::Float(_) => "float",
            Self::Double(_) => "double",
            Self::ByteArray(_) => "byte[]",
            Self::String(_) => "string",
            Self::List(_) => "list",
            Self::Compound(_) => "compound",
            Self::IntArray(_) => "int[]",
            Self::LongArray(_) => "long[]",
        }
    }

    /// `NumericTag.intValue()` for a numeric tag, `None` for any other (`getIntOr`'s
    /// `instanceof NumericTag`). Bytecode: `ByteTag`/`ShortTag`/`IntTag` answer the value,
    /// `LongTag` `(int)(value & -1L)`, `FloatTag`/`DoubleTag` `Mth.floor`, which is
    /// `(int) Math.floor(v)`: a saturating cast with NaN → 0, as Rust's `as` is.
    fn int_value(&self) -> Option<i32> {
        Some(match self {
            Self::Byte(v) => i32::from(*v),
            Self::Short(v) => i32::from(*v),
            Self::Int(v) => *v,
            Self::Long(v) => *v as i32,
            Self::Float(bits) => f64::from(f32::from_bits(*bits)).floor() as i32,
            Self::Double(bits) => f64::from_bits(*bits).floor() as i32,
            _ => return None,
        })
    }

    /// `NumericTag.byteValue()`: `ByteTag` answers its value, every other numeric tag
    /// `(byte)(intValue-ish & 255)` (`LongTag` masks the long first, which is the same low byte).
    fn byte_value(&self) -> Option<i8> {
        match self {
            Self::Byte(v) => Some(*v),
            other => other.int_value().map(|v| (v & 255) as u8 as i8),
        }
    }
}

/// The hidden field key `name` of `marker`, in the heap's `receiver|owner#name descriptor` form.
fn field_key(marker: &str, name: &str) -> String {
    format!("{marker}|{MODEL_OWNER}#{name}")
}

fn entry_name(key: &str) -> String {
    let mut name = String::with_capacity(2 + key.len() * 2);
    name.push_str("e:");
    for byte in key.bytes() {
        name.push_str(&format!("{byte:02x}"));
    }
    name
}

fn entry_key(name: &str) -> Option<String> {
    let hex = name.strip_prefix("e:")?;
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
}

fn text(value: &str) -> Value {
    Value::Ref(Some(value.to_owned()))
}

fn join<T: std::fmt::Display>(values: &[T]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn split<T: std::str::FromStr>(csv: &str) -> Option<Vec<T>> {
    if csv.is_empty() {
        return Some(Vec::new());
    }
    csv.split(',').map(|part| part.parse().ok()).collect()
}

fn illegal_argument(message: String) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message),
        frame: None,
    }
}

impl<H: Host> Runtime<'_, H> {
    fn nbt_field(&self, marker: &str, name: &str) -> Option<&Value> {
        self.fields.get(&field_key(marker, name))
    }

    fn set_nbt_field(&mut self, marker: &str, name: &str, value: Value) {
        self.fields.insert(field_key(marker, name), value);
    }

    /// The `kind` of a tag this module built, `None` for anything else.
    pub(crate) fn nbt_kind(&self, marker: &str) -> Option<&str> {
        match self.nbt_field(marker, "kind") {
            Some(Value::Ref(Some(kind))) => Some(kind.as_str()),
            _ => None,
        }
    }

    /// A new, empty modelled `CompoundTag`.
    pub(crate) fn nbt_new_compound(&mut self) -> Value {
        let value = self.new_object(COMPOUND_TAG);
        if let Some(marker) = expect_ref(&value).cloned() {
            self.set_nbt_field(&marker, "kind", text("compound"));
        }
        value
    }

    /// The entries of a modelled compound, by key, as tag markers.
    fn nbt_entries(&self, marker: &str) -> Vec<(String, String)> {
        let prefix = field_key(marker, "e:");
        self.fields
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .filter_map(|(key, value)| {
                let name = key.strip_prefix(&field_key(marker, ""))?;
                Some((entry_key(name)?, expect_ref(value)?.clone()))
            })
            .collect()
    }

    fn nbt_entry(&self, marker: &str, key: &str) -> Option<String> {
        expect_ref(self.nbt_field(marker, &entry_name(key))?).cloned()
    }

    /// Builds heap objects for `tag` and answers its marker.
    pub(crate) fn nbt_alloc(&mut self, tag: &Nbt) -> Value {
        if let Nbt::Compound(entries) = tag {
            let compound = self.nbt_new_compound();
            let marker = expect_ref(&compound).cloned().unwrap_or_default();
            for (key, value) in entries {
                let child = self.nbt_alloc(value);
                self.set_nbt_field(&marker, &entry_name(key), child);
            }
            return compound;
        }
        let object = self.new_object(tag.class());
        let marker = expect_ref(&object).cloned().unwrap_or_default();
        self.set_nbt_field(&marker, "kind", text(tag.kind()));
        let value = match tag {
            Nbt::Byte(v) => Value::Int(i32::from(*v)),
            Nbt::Short(v) => Value::Int(i32::from(*v)),
            Nbt::Int(v) => Value::Int(*v),
            Nbt::Long(v) => Value::Long(*v),
            Nbt::Float(bits) => Value::Int(*bits as i32),
            Nbt::Double(bits) => Value::Long(*bits as i64),
            Nbt::String(v) => text(v),
            Nbt::ByteArray(v) => text(&join(&v.iter().map(|b| *b as i8).collect::<Vec<_>>())),
            Nbt::IntArray(v) => text(&join(v)),
            Nbt::LongArray(v) => text(&join(v)),
            Nbt::List(elements) => {
                self.set_nbt_field(&marker, "len", Value::Int(elements.len() as i32));
                for (index, element) in elements.iter().enumerate() {
                    let child = self.nbt_alloc(element);
                    self.set_nbt_field(&marker, &format!("i:{index}"), child);
                }
                return object;
            }
            Nbt::Compound(_) => unreachable!("handled above"),
        };
        self.set_nbt_field(&marker, "v", value);
        object
    }

    /// The tree a modelled tag holds now. `Err` names why it cannot be read: a marker this module
    /// did not build, or a tree deeper than NBT allows (a compound put inside itself).
    pub(crate) fn nbt_read(&self, marker: &str) -> Result<Nbt, String> {
        self.nbt_read_at(marker, 0)
    }

    fn nbt_read_at(&self, marker: &str, depth: usize) -> Result<Nbt, String> {
        if depth > MAX_DEPTH {
            return Err(format!("NBT nested deeper than {MAX_DEPTH}"));
        }
        let kind = self
            .nbt_kind(marker)
            .ok_or_else(|| format!("{marker} is not a modelled tag"))?
            .to_owned();
        let bad = || format!("{marker}: malformed {kind} tag");
        let value = self.nbt_field(marker, "v");
        let int = || match value {
            Some(Value::Int(v)) => Ok(*v),
            _ => Err(bad()),
        };
        let long = || match value {
            Some(Value::Long(v)) => Ok(*v),
            _ => Err(bad()),
        };
        let csv = || match value {
            Some(Value::Ref(Some(v))) => Ok(v.as_str()),
            _ => Err(bad()),
        };
        Ok(match kind.as_str() {
            "compound" => {
                let mut entries = NbtCompound::new();
                for (key, child) in self.nbt_entries(marker) {
                    entries.insert(key, self.nbt_read_at(&child, depth + 1)?);
                }
                Nbt::Compound(entries)
            }
            "list" => {
                let len = match self.nbt_field(marker, "len") {
                    Some(Value::Int(len)) => *len as usize,
                    _ => return Err(bad()),
                };
                let mut elements = Vec::with_capacity(len);
                for index in 0..len {
                    let child = self
                        .nbt_field(marker, &format!("i:{index}"))
                        .and_then(expect_ref)
                        .ok_or_else(bad)?;
                    elements.push(self.nbt_read_at(child, depth + 1)?);
                }
                Nbt::List(elements)
            }
            "byte" => Nbt::Byte(int()? as i8),
            "short" => Nbt::Short(int()? as i16),
            "int" => Nbt::Int(int()?),
            "long" => Nbt::Long(long()?),
            "float" => Nbt::Float(int()? as u32),
            "double" => Nbt::Double(long()? as u64),
            "string" => Nbt::String(csv()?.to_owned()),
            "byte[]" => Nbt::ByteArray(
                split::<i8>(csv()?)
                    .ok_or_else(bad)?
                    .into_iter()
                    .map(|b| b as u8)
                    .collect(),
            ),
            "int[]" => Nbt::IntArray(split(csv()?).ok_or_else(bad)?),
            "long[]" => Nbt::LongArray(split(csv()?).ok_or_else(bad)?),
            _ => return Err(bad()),
        })
    }

    /// `Tag.copy()`: compounds and lists are copied deeply, every other tag is immutable and
    /// answers itself (`ByteTag.copy` and friends `areturn this`). Array tags are mutable in
    /// vanilla but no array method is modelled here, so sharing them is unobservable.
    fn nbt_copy(&mut self, marker: &str, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(format!("NBT nested deeper than {MAX_DEPTH}"));
        }
        match self.nbt_kind(marker) {
            Some("compound") => {
                let copy = self.nbt_new_compound();
                let target = expect_ref(&copy).cloned().unwrap_or_default();
                for (key, child) in self.nbt_entries(marker) {
                    let child = self.nbt_copy(&child, depth + 1)?;
                    self.set_nbt_field(&target, &entry_name(&key), child);
                }
                Ok(copy)
            }
            Some("list") => {
                let tree = self.nbt_read_at(marker, depth)?;
                Ok(self.nbt_alloc(&tree))
            }
            Some(_) => Ok(text(marker)),
            None => Err(format!("{marker} is not a modelled tag")),
        }
    }

    /// The tree of the tag `marker` names, if it is a numeric tag this module built.
    fn nbt_scalar(&self, marker: &str) -> Option<Nbt> {
        match self.nbt_kind(marker)? {
            "compound" | "list" => None,
            _ => self.nbt_read(marker).ok(),
        }
    }

    /// The modelled `CompoundTag` surface; `None` leaves the call to the host (counted).
    pub(crate) fn invoke_compound_tag(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != COMPOUND_TAG && r.owner != TAG {
            return Ok(None);
        }
        let Some(marker) = receiver.and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        if r.owner == COMPOUND_TAG && r.name == "<init>" && r.descriptor == "()V" {
            // `new CompoundTag()`: the fresh marker gets its kind, nothing else.
            if !marker.starts_with(&format!("{COMPOUND_TAG}#")) || self.nbt_kind(&marker).is_some()
            {
                return Ok(None);
            }
            self.set_nbt_field(&marker, "kind", text("compound"));
            return Ok(Some(Value::Void));
        }
        if r.name == "copy" && self.nbt_kind(&marker).is_some() {
            return match self.nbt_copy(&marker, 0) {
                Ok(copy) => Ok(Some(copy)),
                Err(_) => Ok(None),
            };
        }
        if r.owner != COMPOUND_TAG || self.nbt_kind(&marker) != Some("compound") {
            return Ok(None);
        }
        // Every modelled method takes a non-null `String` key first. A null key is a valid
        // `HashMap` key in Java but no NBT writer can save it, so it is left unmodelled.
        let Some(key) = args.first().and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        let get = |runtime: &Self| runtime.nbt_entry(&marker, &key);
        let value = match (r.name.as_str(), r.descriptor.as_str()) {
            ("contains", "(Ljava/lang/String;)Z") => Value::Int(i32::from(get(self).is_some())),
            ("get", "(Ljava/lang/String;)Lnet/minecraft/nbt/Tag;") => Value::Ref(get(self)),
            ("put", "(Ljava/lang/String;Lnet/minecraft/nbt/Tag;)Lnet/minecraft/nbt/Tag;") => {
                let tag = args.get(1).cloned().unwrap_or(Value::Ref(None));
                // @0-17: a null tag is an IllegalArgumentException naming the key.
                let Some(tag) = expect_ref(&tag).cloned() else {
                    return Err(illegal_argument(format!(
                        "Invalid null NBT value with key {key}"
                    )));
                };
                if self.nbt_kind(&tag).is_none() {
                    // A tag this module did not build (an unmodelled call's answer): its contents
                    // are unknown, so the put stays the host's, and counted.
                    return Ok(None);
                }
                let previous = get(self);
                self.set_nbt_field(&marker, &entry_name(&key), text(&tag));
                Value::Ref(previous)
            }
            ("putString", "(Ljava/lang/String;Ljava/lang/String;)V") => {
                // `StringTag.valueOf(null)` calls `isEmpty()` on the null: an NPE.
                let Some(value) = args.get(1).and_then(expect_ref).cloned() else {
                    return Err(crate::null_checks::invoke_on_null(&MethodRef {
                        owner: "java/lang/String".into(),
                        name: "isEmpty".into(),
                        descriptor: "()Z".into(),
                    }));
                };
                let tag = self.nbt_alloc(&Nbt::String(value));
                self.set_nbt_field(&marker, &entry_name(&key), tag);
                Value::Void
            }
            ("putBoolean", "(Ljava/lang/String;Z)V") => {
                let flag = matches!(args.get(1), Some(Value::Int(v)) if *v != 0);
                let tag = self.nbt_alloc(&Nbt::Byte(i8::from(flag)));
                self.set_nbt_field(&marker, &entry_name(&key), tag);
                Value::Void
            }
            ("putInt", "(Ljava/lang/String;I)V") => {
                let Some(Value::Int(v)) = args.get(1) else {
                    return Ok(None);
                };
                let tag = self.nbt_alloc(&Nbt::Int(*v));
                self.set_nbt_field(&marker, &entry_name(&key), tag);
                Value::Void
            }
            ("getStringOr", "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;") => {
                match get(self).and_then(|tag| self.nbt_scalar(&tag)) {
                    Some(Nbt::String(text)) => Value::Ref(Some(text)),
                    _ => args.get(1).cloned().unwrap_or(Value::Ref(None)),
                }
            }
            ("getBooleanOr", "(Ljava/lang/String;Z)Z") => {
                let fallback = matches!(args.get(1), Some(Value::Int(v)) if *v != 0);
                let byte = get(self)
                    .and_then(|tag| self.nbt_scalar(&tag))
                    .and_then(|tag| tag.byte_value())
                    .unwrap_or(i8::from(fallback));
                Value::Int(i32::from(byte != 0))
            }
            ("getIntOr", "(Ljava/lang/String;I)I") => {
                let Some(Value::Int(fallback)) = args.get(1) else {
                    return Ok(None);
                };
                Value::Int(
                    get(self)
                        .and_then(|tag| self.nbt_scalar(&tag))
                        .and_then(|tag| tag.int_value())
                        .unwrap_or(*fallback),
                )
            }
            ("getCompoundOrEmpty", "(Ljava/lang/String;)Lnet/minecraft/nbt/CompoundTag;") => {
                // `getCompound(key).orElseGet(CompoundTag::new)`: the live child, or a new empty
                // compound that is not put into this one.
                match get(self).filter(|tag| self.nbt_kind(tag) == Some("compound")) {
                    Some(child) => text(&child),
                    None => self.nbt_new_compound(),
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}
