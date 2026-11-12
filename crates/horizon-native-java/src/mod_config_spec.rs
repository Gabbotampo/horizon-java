//! NeoForge's `ModConfigSpec.Builder`, the values it defines and the spec it
//! builds, as a mod's constructor (or a config class's `<clinit>`) reaches them.
//!
//! The runtime does not load NeoForge, so before this model every builder call
//! went to the host and answered the neutral value: `push` answered `null`
//! (appleskin and NaturesCompass then failed `define` on it), `configure`
//! answered a `null` `Pair` (curios and cherishedworlds failed `getRight()`),
//! and `build()` answered a `null` spec. The behaviour below is ported from
//! the bytecode of `neoforge-26.3.0.101-universal.jar`
//! (`ModConfigSpec$Builder`, `$BuilderContext`, `$ConfigValue`, `$ValueSpec`,
//! `$ListValueSpec`, `$Range`), not invented:
//!
//! * A builder keeps the current path, the pending context (comment lines,
//!   translation key, range, restart type) and how many values it defined.
//!   `push` appends to the path, moves a pending comment and translation onto
//!   the level and then runs `ensureEmpty`; every `define*` resets the context
//!   once its `ValueSpec` is made; `build` runs `ensureEmpty` and hands every
//!   value defined so far the new spec.
//! * The throws are NeoForge's, with its messages: `pop` past the path, a range
//!   whose `min.compareTo(max) > 0`, a null comment, a null default value,
//!   supplier, validator or element validator, and the four `ensureEmpty`
//!   checks. A failed `define` leaves its context pending, as NeoForge does.
//! * `ConfigValue.get()` (and `getAsBoolean`/`getAsInt`/... which call it)
//!   throws `NullPointerException("Cannot get config value before spec is
//!   built")` until the builder built, then `IllegalStateException("Cannot get
//!   config value before config is loaded.")`: NeoForge's `getRaw` checks the
//!   spec's `loadedConfig`, and nothing here loads a config file. It never
//!   answers the declared default. Once the spec has been handed to a host call
//!   the runtime does not model (a `registerConfig`, `acceptConfig`, a loader
//!   helper), the loader may have loaded it, so `get()` and `isLoaded()` are
//!   counted as unknown host calls instead of answered.
//! * `getDefault()` is the declared default: the constant, or the supplier run
//!   again on every call, as NeoForge's `defaultSupplier.get()` does.
//! * `defineInRange(..., Supplier, ...)` runs the supplier once at definition
//!   time (NeoForge builds its `" Default: "` comment from it).
//!
//! Left counted (the call goes to the host unchanged): `define(List, ValueSpec,
//! Supplier)`, the generic `defineInRange(..., Comparable..., Class)`,
//! `defineList(..., Range)`, every `defineEnum` overload but `(path, default)`
//! and a `defineEnum` with a null default, a null path or a path list the
//! runtime cannot read, a null supplier where NeoForge's own throw would carry
//! a helpful-NPE message, and the value and spec members not listed here
//! (`getPath`, `set`, `save`, `getSpec`, `clearCache`, `isEmpty`, ...).
//!
//! Comment text is not kept: nothing a mod reads back during construction
//! depends on it, and NeoForge's one comment check (a comment of only blank
//! lines) only logs a warning when `FMLEnvironment.isProduction()`, which is
//! what the loader model answers.
//!
//! State lives in the runtime's instance-field table under each marker, so it
//! survives into later callbacks with the rest of the jar's heap.

use super::*;

const SPEC: &str = "net/neoforged/neoforge/common/ModConfigSpec";
/// The prefix of a spec marker `build()` made (`Runtime::new_object`).
const SPEC_MARKER: &str = "net/neoforged/neoforge/common/ModConfigSpec#";
const BUILDER: &str = "net/neoforged/neoforge/common/ModConfigSpec$Builder";
const PAIR: &str = "org/apache/commons/lang3/tuple/Pair";
const IMMUTABLE_PAIR: &str = "org/apache/commons/lang3/tuple/ImmutablePair";

const PATH_STRING: &str = "(Ljava/lang/String;";
const PATH_LIST: &str = "(Ljava/util/List;";
const SUPPLIER: &str = "Ljava/util/function/Supplier;";
const PREDICATE: &str = "Ljava/util/function/Predicate;";

/// Field keys under a builder marker.
const B_PATH: &str = "mcs#path";
const B_COMMENTS: &str = "mcs#comments";
const B_TRANSLATION: &str = "mcs#translation";
const B_RANGE: &str = "mcs#range";
const B_RESTART: &str = "mcs#restart";
const B_DEFINED: &str = "mcs#defined";
const B_BUILT: &str = "mcs#built";
const B_SPEC: &str = "mcs#spec";
/// Field keys under a value marker.
const V_BUILDER: &str = "mcs#builder";
const V_INDEX: &str = "mcs#index";
const V_CONSTANT: &str = "mcs#constant";
const V_SUPPLIER: &str = "mcs#supplier";
/// Field keys under a spec marker.
const S_BUILDER: &str = "mcs#of";
const S_ESCAPED: &str = "mcs#escaped";
/// Field keys under a pair marker.
const P_LEFT: &str = "mcs#left";
const P_RIGHT: &str = "mcs#right";

fn is_value_class(owner: &str) -> bool {
    matches!(
        owner.strip_prefix("net/neoforged/neoforge/common/ModConfigSpec$"),
        Some(
            "ConfigValue" | "BooleanValue" | "IntValue" | "LongValue" | "DoubleValue" | "EnumValue"
        )
    )
}

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

fn npe(message: &str) -> NativeError {
    thrown("java/lang/NullPointerException", Some(message.into()))
}

fn illegal_state(message: &str) -> NativeError {
    thrown("java/lang/IllegalStateException", Some(message.into()))
}

fn illegal_argument(message: String) -> NativeError {
    thrown("java/lang/IllegalArgumentException", Some(message))
}

fn is_null(value: &Value) -> bool {
    matches!(value, Value::Ref(None))
}

/// `ModConfigSpec.split`: Guava's `Splitter.on(".")`, which keeps empty parts.
fn split_path(path: &str) -> Vec<String> {
    path.split('.').map(str::to_owned).collect()
}

/// `AbstractCollection.toString` of a list of strings.
fn list_text(path: &[String]) -> String {
    format!("[{}]", path.join(", "))
}

/// What a `define*` overload takes after its path, read from its descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `define(path, boolean)`.
    BoolConstant,
    /// `define(path, Supplier<Boolean>)`.
    BoolSupplier,
    /// `define(path, T)` / `define(path, T, Predicate)`.
    Constant { validator: bool },
    /// `define(path, Supplier, Predicate[, Class])`.
    Supplied,
    /// `defineInRange(path, x, min, max)` over `int`/`long`/`double`.
    RangeConstant(Number),
    /// `defineInRange(path, Supplier, min, max)` over `int`/`long`/`double`.
    RangeSupplied(Number),
    /// `defineInList(path, T | Supplier, Collection)`.
    InList { supplied: bool },
    /// `defineList`/`defineListAllowEmpty(path, List | Supplier, [Supplier,] Predicate)`.
    List {
        supplied: bool,
        element_supplier: bool,
    },
    /// `defineEnum(path, V)`.
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Number {
    Int,
    Long,
    Double,
}

fn define_shape(name: &str, descriptor: &str) -> Option<Shape> {
    let rest = descriptor
        .strip_prefix(PATH_STRING)
        .or_else(|| descriptor.strip_prefix(PATH_LIST))?;
    let (params, result) = rest.split_once(')')?;
    let boolean_result = result == "Lnet/neoforged/neoforge/common/ModConfigSpec$BooleanValue;";
    Some(match (name, params) {
        ("define", "Z") => Shape::BoolConstant,
        ("define", SUPPLIER) if boolean_result => Shape::BoolSupplier,
        ("define", "Ljava/lang/Object;") => Shape::Constant { validator: false },
        ("define", "Ljava/lang/Object;Ljava/util/function/Predicate;") => {
            Shape::Constant { validator: true }
        }
        (
            "define",
            "Ljava/util/function/Supplier;Ljava/util/function/Predicate;"
            | "Ljava/util/function/Supplier;Ljava/util/function/Predicate;Ljava/lang/Class;",
        ) => Shape::Supplied,
        ("defineInRange", "III") => Shape::RangeConstant(Number::Int),
        ("defineInRange", "JJJ") => Shape::RangeConstant(Number::Long),
        ("defineInRange", "DDD") => Shape::RangeConstant(Number::Double),
        ("defineInRange", "Ljava/util/function/Supplier;II") => Shape::RangeSupplied(Number::Int),
        ("defineInRange", "Ljava/util/function/Supplier;JJ") => Shape::RangeSupplied(Number::Long),
        ("defineInRange", "Ljava/util/function/Supplier;DD") => {
            Shape::RangeSupplied(Number::Double)
        }
        ("defineInList", "Ljava/lang/Object;Ljava/util/Collection;") => {
            Shape::InList { supplied: false }
        }
        ("defineInList", "Ljava/util/function/Supplier;Ljava/util/Collection;") => {
            Shape::InList { supplied: true }
        }
        ("defineList" | "defineListAllowEmpty", tail) => {
            let (supplied, after) = if let Some(after) = tail.strip_prefix("Ljava/util/List;") {
                (false, after)
            } else {
                (true, tail.strip_prefix(SUPPLIER)?)
            };
            let element_supplier = match after {
                PREDICATE => false,
                "Ljava/util/function/Supplier;Ljava/util/function/Predicate;" => true,
                _ => return None,
            };
            Shape::List {
                supplied,
                element_supplier,
            }
        }
        ("defineEnum", "Ljava/lang/Enum;") => Shape::Enum,
        _ => return None,
    })
}

impl<H: Host> Runtime<'_, H> {
    fn mcs_get(&self, marker: &str, key: &str) -> Option<&Value> {
        self.fields.get(&format!("{marker}|{key}"))
    }

    fn mcs_int(&self, marker: &str, key: &str) -> i32 {
        match self.mcs_get(marker, key) {
            Some(Value::Int(value)) => *value,
            _ => 0,
        }
    }

    fn mcs_set(&mut self, marker: &str, key: &str, value: Value) {
        self.fields.insert(format!("{marker}|{key}"), value);
    }

    fn mcs_path(&self, builder: &str) -> Vec<String> {
        self.mcs_get(builder, B_PATH)
            .and_then(expect_ref)
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default()
    }

    fn mcs_set_path(&mut self, builder: &str, path: &[String]) {
        let text = serde_json::to_string(path).unwrap_or_else(|_| "[]".into());
        self.mcs_set(builder, B_PATH, Value::Ref(Some(text)));
    }

    /// The builder marker a receiver names, when this model made it.
    fn mcs_builder(&self, receiver: Option<&Value>) -> Option<String> {
        let marker = receiver.and_then(expect_ref)?;
        self.mcs_get(marker, B_PATH).map(|_| marker.clone())
    }

    /// The path a `define*`/`push` argument names: `split` of a string, or the
    /// elements of a list. `None` for a null or unreadable argument, which
    /// stays a counted call.
    fn mcs_path_arg(&self, value: &Value) -> Option<Vec<String>> {
        if is_null(value) {
            return None;
        }
        runtime_collection_values(&*self.host, value).ok()
    }

    /// `BuilderContext.ensureEmpty`, in NeoForge's order.
    fn mcs_ensure_empty(&self, builder: &str) -> Result<(), NativeError> {
        if self.mcs_int(builder, B_COMMENTS) > 0 {
            return Err(illegal_state("Non-empty comment when empty expected"));
        }
        if self.mcs_int(builder, B_TRANSLATION) != 0 {
            return Err(illegal_state("Non-null translation key when null expected"));
        }
        if self.mcs_int(builder, B_RANGE) != 0 {
            return Err(illegal_state("Non-null range when null expected"));
        }
        if let Some(restart) = self.mcs_get(builder, B_RESTART).and_then(expect_ref) {
            return Err(illegal_state(&format!(
                "Dangling restart value set to {restart}"
            )));
        }
        Ok(())
    }

    /// A new `BuilderContext`: what every successful `define*` leaves behind.
    fn mcs_reset_context(&mut self, builder: &str) {
        self.mcs_set(builder, B_COMMENTS, Value::Int(0));
        self.mcs_set(builder, B_TRANSLATION, Value::Int(0));
        self.mcs_set(builder, B_RANGE, Value::Int(0));
        self.fields.remove(&format!("{builder}|{B_RESTART}"));
    }

    fn mcs_add_comments(&mut self, builder: &str, lines: usize) {
        let count = self
            .mcs_int(builder, B_COMMENTS)
            .saturating_add(lines as i32);
        self.mcs_set(builder, B_COMMENTS, Value::Int(count));
    }

    /// Answer a NeoForge config-spec call, or `None` to leave it to the rest of
    /// the dispatch (where an unmodelled one is counted).
    pub(super) fn invoke_mod_config_spec(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner == BUILDER {
            return self.mcs_builder_call(r, receiver, args);
        }
        if let Some(marker) = receiver.and_then(expect_ref)
            && self.mcs_get(marker, V_BUILDER).is_some()
            && (is_value_class(&r.owner)
                || matches!(
                    r.owner.as_str(),
                    "java/util/function/Supplier"
                        | "java/util/function/BooleanSupplier"
                        | "java/util/function/IntSupplier"
                        | "java/util/function/LongSupplier"
                        | "java/util/function/DoubleSupplier"
                ))
        {
            let marker = marker.clone();
            return self.mcs_value_call(r, &marker);
        }
        if let Some(marker) = receiver.and_then(expect_ref)
            && self.mcs_get(marker, P_LEFT).is_some()
            && matches!(
                r.owner.as_str(),
                PAIR | IMMUTABLE_PAIR | "java/util/Map$Entry"
            )
        {
            let key = match (r.name.as_str(), r.descriptor.as_str()) {
                ("getLeft" | "getKey", "()Ljava/lang/Object;") => P_LEFT,
                ("getRight" | "getValue", "()Ljava/lang/Object;") => P_RIGHT,
                _ => return Ok(None),
            };
            return Ok(self.mcs_get(marker, key).cloned());
        }
        if r.owner == SPEC
            && let Some(marker) = receiver.and_then(expect_ref)
            && self.mcs_get(marker, S_BUILDER).is_some()
        {
            let marker = marker.clone();
            if r.name == "isLoaded" && r.descriptor == "()Z" && !self.mcs_spec_escaped(&marker) {
                // `loadedConfig != null`, and nothing loaded this spec.
                return Ok(Some(Value::Int(0)));
            }
            // Any other spec member may hand the spec its loaded config
            // (`acceptConfig`) or otherwise act as the loader would.
            self.mcs_set(&marker, S_ESCAPED, Value::Int(1));
            return Ok(None);
        }
        self.mcs_note_escape(r, receiver, args);
        Ok(None)
    }

    fn mcs_spec_escaped(&self, spec: &str) -> bool {
        self.mcs_int(spec, S_ESCAPED) != 0
    }

    /// A spec handed to a host call this runtime does not model (outside the
    /// JDK and the jar's own classes) may have been registered and loaded.
    fn mcs_note_escape(&mut self, r: &MethodRef, receiver: Option<&Value>, args: &[Value]) {
        let escaped: Vec<String> = receiver
            .into_iter()
            .chain(args)
            .filter_map(expect_ref)
            .filter(|marker| marker.starts_with(SPEC_MARKER))
            .filter(|marker| self.mcs_get(marker, S_BUILDER).is_some())
            .cloned()
            .collect();
        if escaped.is_empty() || r.owner.starts_with("java/") || self.has_class(&r.owner) {
            return;
        }
        for spec in escaped {
            self.mcs_set(&spec, S_ESCAPED, Value::Int(1));
        }
    }

    fn mcs_builder_call(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.name == "<init>" && r.descriptor == "()V" {
            let Some(marker) = receiver.and_then(expect_ref).cloned() else {
                return Ok(None);
            };
            self.mcs_set_path(&marker, &[]);
            self.mcs_reset_context(&marker);
            self.mcs_set(&marker, B_DEFINED, Value::Int(0));
            self.mcs_set(&marker, B_BUILT, Value::Int(0));
            return Ok(Some(Value::Void));
        }
        let Some(builder) = self.mcs_builder(receiver) else {
            return Ok(None);
        };
        let this = Value::Ref(Some(builder.clone()));
        match (r.name.as_str(), r.descriptor.as_str()) {
            (
                "comment",
                "(Ljava/lang/String;)Lnet/neoforged/neoforge/common/ModConfigSpec$Builder;",
            ) => {
                if is_null(&args[0]) {
                    return Err(npe("Passed in null value for comment"));
                }
                self.mcs_add_comments(&builder, 1);
                Ok(Some(this))
            }
            (
                "comment",
                "([Ljava/lang/String;)Lnet/neoforged/neoforge/common/ModConfigSpec$Builder;",
            ) => {
                // A null array fails `arraylength` with a helpful-NPE message
                // this model does not reproduce: left counted.
                let Ok(lines) = self.array(&args[0]).cloned() else {
                    return Ok(None);
                };
                if let Some(index) = lines.iter().position(is_null) {
                    return Err(npe(&format!("Comment string at {index} is null.")));
                }
                self.mcs_add_comments(&builder, lines.len());
                Ok(Some(this))
            }
            (
                "translation",
                "(Ljava/lang/String;)Lnet/neoforged/neoforge/common/ModConfigSpec$Builder;",
            ) => {
                self.mcs_set(
                    &builder,
                    B_TRANSLATION,
                    Value::Int(i32::from(!is_null(&args[0]))),
                );
                Ok(Some(this))
            }
            (
                "worldRestart" | "gameRestart",
                "()Lnet/neoforged/neoforge/common/ModConfigSpec$Builder;",
            ) => {
                let restart = if r.name == "worldRestart" {
                    "WORLD"
                } else {
                    "GAME"
                };
                self.mcs_set(&builder, B_RESTART, Value::Ref(Some(restart.into())));
                Ok(Some(this))
            }
            (
                "push",
                "(Ljava/lang/String;)Lnet/neoforged/neoforge/common/ModConfigSpec$Builder;",
            )
            | ("push", "(Ljava/util/List;)Lnet/neoforged/neoforge/common/ModConfigSpec$Builder;") =>
            {
                let added = if r.descriptor.starts_with(PATH_STRING) {
                    match expect_ref(&args[0]) {
                        Some(text) => split_path(text),
                        None => return Ok(None),
                    }
                } else {
                    match self.mcs_path_arg(&args[0]) {
                        Some(path) => path,
                        None => return Ok(None),
                    }
                };
                let mut path = self.mcs_path(&builder);
                path.extend(added);
                self.mcs_set_path(&builder, &path);
                // The pending comment and translation become the level's own.
                self.mcs_set(&builder, B_COMMENTS, Value::Int(0));
                self.mcs_set(&builder, B_TRANSLATION, Value::Int(0));
                self.mcs_ensure_empty(&builder)?;
                Ok(Some(this))
            }
            ("pop", "()Lnet/neoforged/neoforge/common/ModConfigSpec$Builder;")
            | ("pop", "(I)Lnet/neoforged/neoforge/common/ModConfigSpec$Builder;") => {
                let count = match args.first() {
                    Some(Value::Int(count)) => *count,
                    _ => 1,
                };
                let mut path = self.mcs_path(&builder);
                if count > 0 && count as usize > path.len() {
                    return Err(illegal_argument(format!(
                        "Attempted to pop {count} elements when we only had: {}",
                        list_text(&path)
                    )));
                }
                let keep = path.len() - count.max(0) as usize;
                path.truncate(keep);
                self.mcs_set_path(&builder, &path);
                Ok(Some(this))
            }
            ("build", "()Lnet/neoforged/neoforge/common/ModConfigSpec;") => {
                self.mcs_build(&builder).map(Some)
            }
            (
                "configure",
                "(Ljava/util/function/Function;)Lorg/apache/commons/lang3/tuple/Pair;",
            ) => {
                let Some(left) = self.invoke_callback(&args[0], std::slice::from_ref(&this))?
                else {
                    return Ok(None);
                };
                let spec = self.mcs_build(&builder)?;
                let pair = self.new_object(IMMUTABLE_PAIR);
                let marker = expect_ref(&pair).cloned().unwrap_or_default();
                self.mcs_set(&marker, P_LEFT, left);
                self.mcs_set(&marker, P_RIGHT, spec);
                Ok(Some(pair))
            }
            (name, descriptor) => match define_shape(name, descriptor) {
                Some(shape) => self.mcs_define(&builder, r, shape, args),
                None => Ok(None),
            },
        }
    }

    fn mcs_build(&mut self, builder: &str) -> Result<Value, NativeError> {
        self.mcs_ensure_empty(builder)?;
        let spec = self.new_object(SPEC);
        let marker = expect_ref(&spec).cloned().unwrap_or_default();
        self.mcs_set(&marker, S_BUILDER, Value::Ref(Some(builder.to_owned())));
        // `values.forEach(value -> value.spec = spec)`: every value defined so
        // far now reads through this spec.
        let defined = self.mcs_int(builder, B_DEFINED);
        self.mcs_set(builder, B_BUILT, Value::Int(defined));
        self.mcs_set(builder, B_SPEC, spec.clone());
        Ok(spec)
    }

    /// One `define*` overload, in the order NeoForge's chain runs its checks.
    fn mcs_define(
        &mut self,
        builder: &str,
        r: &MethodRef,
        shape: Shape,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if args.is_empty() {
            return Ok(None);
        }
        // The path is only checked (a null or unreadable one stays counted):
        // comment text and the spec's own entry table are not kept.
        let readable = if r.descriptor.starts_with(PATH_STRING) {
            expect_ref(&args[0]).is_some()
        } else {
            self.mcs_path_arg(&args[0]).is_some()
        };
        if !readable {
            return Ok(None);
        }
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        // (default constant, default supplier, validator, element validator)
        let (constant, supplier, validator, element_validator): (
            Option<Value>,
            Option<Value>,
            Value,
            Option<Value>,
        ) = match shape {
            Shape::BoolConstant => {
                let value = matches!(arg(1), Value::Int(value) if value != 0);
                (
                    Some(Value::Ref(Some(format!("boolean:{value}")))),
                    None,
                    Value::Int(1),
                    None,
                )
            }
            Shape::BoolSupplier => (None, Some(arg(1)), Value::Int(1), None),
            Shape::Constant { validator } => {
                if is_null(&arg(1)) {
                    return Err(npe("Default value can not be null"));
                }
                let check = if validator { arg(2) } else { Value::Int(1) };
                (Some(arg(1)), None, check, None)
            }
            Shape::Supplied => (None, Some(arg(1)), arg(2), None),
            Shape::RangeConstant(number) | Shape::RangeSupplied(number) => {
                let supplied = matches!(shape, Shape::RangeSupplied(_));
                // `new Range(clazz, min, max)` runs first, in both overloads.
                let (min, max) = (arg(2), arg(3));
                let reversed = match number {
                    Number::Int | Number::Long => {
                        let low = boxed_long(&min).unwrap_or(0);
                        let high = boxed_long(&max).unwrap_or(0);
                        low > high
                    }
                    Number::Double => {
                        let low = boxed_number(&min).unwrap_or(0.0);
                        let high = boxed_number(&max).unwrap_or(0.0);
                        compare_double(low, high) > 0
                    }
                };
                if reversed {
                    return Err(illegal_argument("Range min must be less then max.".into()));
                }
                let constant = (!supplied).then(|| match number {
                    Number::Int => Value::Ref(Some(format!(
                        "integer:{}",
                        boxed_integer(&arg(1)).unwrap_or(0)
                    ))),
                    Number::Long => long_value(boxed_long(&arg(1)).unwrap_or(0)),
                    Number::Double => double_value(boxed_number(&arg(1)).unwrap_or(0.0)),
                });
                if supplied {
                    let supplier = arg(1);
                    // `supplier.get()` on null fails with a helpful-NPE message
                    // this model does not reproduce: left counted.
                    if is_null(&supplier) {
                        return Ok(None);
                    }
                    // `context.setRange(range)` comes before the supplier runs.
                    self.mcs_set(builder, B_RANGE, Value::Int(1));
                    if self.invoke_callback(&supplier, &[])?.is_none() {
                        return Ok(None);
                    }
                    (None, Some(supplier), Value::Int(1), None)
                } else {
                    self.mcs_set(builder, B_RANGE, Value::Int(1));
                    (constant, None, Value::Int(1), None)
                }
            }
            Shape::InList { supplied } => {
                // `acceptable::contains` null-checks its receiver first.
                if is_null(&arg(2)) {
                    return Err(thrown("java/lang/NullPointerException", None));
                }
                if supplied {
                    (None, Some(arg(1)), Value::Int(1), None)
                } else {
                    (Some(arg(1)), None, Value::Int(1), None)
                }
            }
            Shape::List {
                supplied,
                element_supplier,
            } => {
                let validator_index = if element_supplier { 3 } else { 2 };
                let element = arg(validator_index);
                if supplied {
                    (None, Some(arg(1)), Value::Int(1), Some(element))
                } else {
                    (Some(arg(1)), None, Value::Int(1), Some(element))
                }
            }
            Shape::Enum => {
                // `defaultValue.getDeclaringClass()` on null: a helpful-NPE
                // message this model does not reproduce, left counted.
                if is_null(&arg(1)) {
                    return Ok(None);
                }
                (Some(arg(1)), None, Value::Int(1), None)
            }
        };
        // `ValueSpec`'s constructor, then `ListValueSpec`'s.
        if let Some(supplier) = &supplier
            && is_null(supplier)
        {
            return Err(npe("Default supplier can not be null"));
        }
        if is_null(&validator) {
            return Err(npe("Validator can not be null"));
        }
        if let Some(element) = &element_validator
            && is_null(element)
        {
            return Err(npe("ElementValidator can not be null"));
        }
        // `define(List, ValueSpec, Supplier)`: a fresh context, and the value.
        self.mcs_reset_context(builder);
        let index = self.mcs_int(builder, B_DEFINED);
        self.mcs_set(builder, B_DEFINED, Value::Int(index.saturating_add(1)));
        let class = result_descriptor(&r.descriptor)
            .strip_prefix('L')
            .and_then(|name| name.strip_suffix(';'))
            .unwrap_or("net/neoforged/neoforge/common/ModConfigSpec$ConfigValue")
            .to_owned();
        let value = self.new_object(&class);
        let marker = expect_ref(&value).cloned().unwrap_or_default();
        self.mcs_set(&marker, V_BUILDER, Value::Ref(Some(builder.to_owned())));
        self.mcs_set(&marker, V_INDEX, Value::Int(index));
        if let Some(constant) = constant {
            self.mcs_set(&marker, V_CONSTANT, constant);
        }
        if let Some(supplier) = supplier {
            self.mcs_set(&marker, V_SUPPLIER, supplier);
        }
        Ok(Some(value))
    }

    /// A member of a value this model defined.
    fn mcs_value_call(&mut self, r: &MethodRef, value: &str) -> Result<Option<Value>, NativeError> {
        let builder = self
            .mcs_get(value, V_BUILDER)
            .and_then(expect_ref)
            .cloned()
            .unwrap_or_default();
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("getDefault", "()Ljava/lang/Object;") => {
                if let Some(constant) = self.mcs_get(value, V_CONSTANT) {
                    return Ok(Some(constant.clone()));
                }
                let Some(supplier) = self.mcs_get(value, V_SUPPLIER).cloned() else {
                    return Ok(None);
                };
                self.invoke_callback(&supplier, &[])
            }
            ("next", "()Lnet/neoforged/neoforge/common/ModConfigSpec$Builder;") => {
                Ok(Some(Value::Ref(Some(builder))))
            }
            ("get" | "getRaw", "()Ljava/lang/Object;")
            | ("getAsBoolean" | "isTrue" | "isFalse", "()Z")
            | ("getAsInt", "()I")
            | ("getAsLong", "()J")
            | ("getAsDouble", "()D") => {
                // `getRaw`: `checkNotNull(spec, ...)`, then
                // `checkState(spec.loadedConfig != null, ...)`.
                let index = self.mcs_int(value, V_INDEX);
                if index >= self.mcs_int(&builder, B_BUILT) {
                    return Err(npe("Cannot get config value before spec is built"));
                }
                let spec = self
                    .mcs_get(&builder, B_SPEC)
                    .and_then(expect_ref)
                    .cloned()
                    .unwrap_or_default();
                if self.mcs_spec_escaped(&spec) {
                    return Ok(None);
                }
                Err(illegal_state(
                    "Cannot get config value before config is loaded.",
                ))
            }
            _ => Ok(None),
        }
    }
}
