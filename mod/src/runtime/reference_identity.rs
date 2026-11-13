//! `if_acmpeq`/`if_acmpne` on boxes, strings and the JDK's shared empty
//! collections: Java's object identity, or a count.
//!
//! The interpreter holds a box as its value (`Integer` 1000 is a
//! `Value::Boxed`, or a bare `Value::Int` from lambda boxing) and a string as
//! its text, so two references compared with `==` used to be "the same
//! object" whenever their values were equal. Java answers from identity
//! instead. What JDK 25 does, read from its bytecode (`javap -c`, OpenJDK
//! 25.0.5) and checked by running it:
//!
//! * `Integer`/`Short`/`Long.valueOf` (and so autoboxing) answer a cached
//!   object for -128..=127, `Character.valueOf` for 0..=127, `Byte.valueOf`
//!   always, `Boolean.valueOf` the `TRUE`/`FALSE` singletons; outside its
//!   cache every call is a `new`. The `valueOf(String)` overloads go through
//!   the same caches. `Float`/`Double.valueOf` are always a `new`. (The
//!   `Integer` cache's upper bound can be raised with `-XX:AutoBoxCacheMax`;
//!   the game's launcher does not, so 127 holds.)
//! * `new X(..)` and string concatenation (`StringConcatHelper.simpleConcat`,
//!   `newStringOf`, `doConcat` all end in `new String`, JLS 15.18.1) always
//!   make a new object.
//! * `StringBuilder`/`StringBuffer.toString()` answer the `""` literal when
//!   empty and a `new String` otherwise.
//! * String literals (and compile-time constants, which javac folds into one
//!   literal) are interned: one object per text, which `intern()` answers too.
//!   But `intern()` of a text not yet in the pool puts *that* object in the
//!   pool, so a fresh string may later be the literal itself
//!   (`s.intern() == s`): a fresh string against a literal is unknown.
//! * `List.of()`, `Set.of()`, `Map.of()` and `Collections.emptyList()`,
//!   `emptySet()`, `emptyMap()` (and the `EMPTY_*` fields) answer one shared
//!   object each; the runtime builds a new marker per call.
//!
//! A value cannot say where it came from, so this module reads the method's
//! own bytecode: a dataflow pass over its basic blocks ([`analyse`]) finds,
//! for both operands of each `if_acmp`, the one instruction (or parameter)
//! that produced the reference, through locals, `dup`s and `checkcast`s, and
//! whether that instruction can run twice in one frame (it lies on a cycle of
//! the control flow). The pass runs once per method and is cached: per frame,
//! and across calls in a thread-local table checked against the code bytes.
//! [`decide`] then answers:
//!
//! * the same producer that runs at most once per frame: the same reference
//!   flowed to both operands, so `true`;
//! * two cached boxes of one class within the cache, two interned strings, or
//!   the same shared empty collection: `true`; different box classes or
//!   different shared collections: `false`;
//! * separate allocations (distinct producers, each a `new`, a concatenation,
//!   a non-empty builder text or a box outside its cache): `false`;
//! * anything else (a value read from a field, an array or a collection, a
//!   method's answer, a parameter, a fresh string against a literal, one
//!   producer on a loop): Java's answer depends on an identity the
//!   interpreter does not keep, so the comparison is counted as an unknown
//!   host call (`java/lang/Object#if_acmp` for boxes and collections,
//!   `java/lang/String#if_acmp` for strings) and keeps the old value-equality
//!   answer.
//!
//! Unequal boxes and unequal texts are different objects without any
//! analysis. Equal references that are neither statically `String`s nor
//! plain text (a jar object's `class#n` marker is one per allocation; a host
//! model's marker) keep the old value-equality answer.

use super::*;
use std::{collections::BTreeMap, rc::Rc};

/// Methods whose analysis the thread keeps, and the code bytes they may hold
/// together, before the table starts over.
const MAX_CACHED_METHODS: usize = 1024;
const MAX_CACHED_CODE_BYTES: usize = 16 << 20;
/// Work the dataflow may do for one method before it gives up, in slot
/// operations: every instruction stepped, and every local and stack slot
/// copied or merged at a block entry or an exception handler. A method whose
/// fixpoint costs more (one more merged local per pass over a long body) is
/// not analysed, and its comparisons are counted. This bounds the pass to a
/// few milliseconds per method, once per thread.
const MAX_WORK: usize = 4_000_000;

/// Charge `amount` to the method's budget; `None` once it is spent.
fn charge(done: &mut usize, amount: usize) -> Option<()> {
    *done += amount;
    (*done <= MAX_WORK).then_some(())
}

/// Cached analyses: key → (the code they were computed from, the answer).
type Cache = BTreeMap<String, (Rc<[u8]>, Option<Rc<Flow>>)>;

thread_local! {
    static FLOWS: RefCell<(Cache, usize)> = const { RefCell::new((BTreeMap::new(), 0)) };
}

/// One frame's analysis, looked up at its first comparison that needs it.
#[derive(Default)]
pub(crate) struct FrameIdentity {
    flow: Option<Option<Rc<Flow>>>,
}

/// Who produced a reference: a parameter slot or the instruction at a pc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Param(u16),
    At(u32),
}

/// One stack word or local of the abstract frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    /// `None`: several producers reach here, or a primitive.
    source: Option<Source>,
    /// What the bytecode says the reference is.
    ty: Ty,
}

const TOP: Slot = Slot {
    source: None,
    ty: Ty::Unknown,
};

impl Slot {
    fn merge(self, other: Slot) -> Slot {
        Slot {
            source: (self.source == other.source)
                .then_some(self.source)
                .flatten(),
            ty: if self.ty == other.ty {
                self.ty
            } else {
                Ty::Unknown
            },
        }
    }
}

/// A reference's static type, as far as "could it be a `String`" goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    /// Not known: a merge of two types, `null`, a primitive.
    Unknown,
    String,
    /// A type every `String` also is (`Object`, `CharSequence`,
    /// `Comparable`, `Serializable`, `Constable`, `ConstantDesc`).
    Maybe,
    /// Any other class: never a `String`.
    Other,
    /// An array (never a `String`) and what its elements are.
    Array(Elem),
}

/// An array's element type, classed like [`Ty`] (an array of arrays has
/// `Other` elements).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Elem {
    Unknown,
    String,
    Maybe,
    Other,
}

impl Elem {
    fn of(ty: Ty) -> Self {
        match ty {
            Ty::String => Self::String,
            Ty::Maybe => Self::Maybe,
            Ty::Unknown => Self::Unknown,
            Ty::Other | Ty::Array(_) => Self::Other,
        }
    }

    fn ty(self) -> Ty {
        match self {
            Self::Unknown => Ty::Unknown,
            Self::String => Ty::String,
            Self::Maybe => Ty::Maybe,
            Self::Other => Ty::Other,
        }
    }
}

/// The [`Ty`] of a field descriptor.
fn ty_of(descriptor: &str) -> Ty {
    match descriptor {
        "Ljava/lang/String;" => Ty::String,
        "Ljava/lang/Object;"
        | "Ljava/lang/CharSequence;"
        | "Ljava/lang/Comparable;"
        | "Ljava/io/Serializable;"
        | "Ljava/lang/constant/Constable;"
        | "Ljava/lang/constant/ConstantDesc;" => Ty::Maybe,
        _ => match descriptor.as_bytes().first() {
            Some(b'[') => Ty::Array(Elem::of(ty_of(&descriptor[1..]))),
            Some(b'L') => Ty::Other,
            _ => Ty::Unknown,
        },
    }
}

/// The [`Ty`] of a constant-pool class name (`java/lang/String`, `[I`).
fn ty_of_class(name: &str) -> Ty {
    if name.starts_with('[') {
        ty_of(name)
    } else {
        ty_of(&format!("L{name};"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    locals: Vec<Slot>,
    stack: Vec<Slot>,
}

impl State {
    /// Merge a frame's `locals` and `stack` in, in place; whether anything
    /// changed, `None` for stack heights a verified class never has.
    fn merge(&mut self, locals: &[Slot], stack: &[Slot]) -> Option<bool> {
        if self.stack.len() != stack.len() {
            return None;
        }
        let mut changed = false;
        if self.locals.len() < locals.len() {
            self.locals.resize(locals.len(), TOP);
            changed = true;
        }
        for (index, slot) in self.locals.iter_mut().enumerate() {
            let merged = slot.merge(locals.get(index).copied().unwrap_or(TOP));
            changed |= merged != *slot;
            *slot = merged;
        }
        for (slot, other) in self.stack.iter_mut().zip(stack) {
            let merged = slot.merge(*other);
            changed |= merged != *slot;
            *slot = merged;
        }
        Some(changed)
    }
}

/// What one producer makes, as far as identity goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// A `valueOf` (or `Boolean.TRUE`/`FALSE`) of this cached box class.
    Cached(&'static str),
    /// `Float`/`Double.valueOf`: a new box every time.
    Uncached(&'static str),
    /// `new`, concatenation: a new object.
    Fresh,
    /// `StringBuilder`/`StringBuffer.toString()`: the `""` literal when
    /// empty, a new string otherwise.
    BuilderText,
    /// A string literal or `intern()`: the pool's object for its text.
    Interned,
    /// One of the JDK's shared empty collections.
    Shared(&'static str),
    /// Anything else: identity unknown.
    Opaque,
}

/// One `if_acmp` operand: its producer, what it makes, and whether that
/// producer may run more than once per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Operand {
    source: Option<Source>,
    ty: Ty,
    kind: Kind,
    repeats: bool,
}

/// Every reachable `if_acmp` of one method: pc → (lhs, rhs).
#[derive(Debug, Default)]
struct Flow {
    compares: BTreeMap<u32, (Operand, Operand)>,
}

/// The answer for one comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Identity {
    Same,
    Different,
    Unknown,
}

const CACHED_BOXES: [&str; 6] = [
    "java/lang/Integer",
    "java/lang/Short",
    "java/lang/Long",
    "java/lang/Byte",
    "java/lang/Character",
    "java/lang/Boolean",
];
const UNCACHED_BOXES: [&str; 2] = ["java/lang/Float", "java/lang/Double"];

/// Which family a counted comparison is reported under.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Box,
    String,
    Collection,
}

impl<H: Host> Runtime<'_, H> {
    /// Whether `lhs` and `rhs` are the same object at the `if_acmp` at `at`.
    /// Counts the comparison when Java's answer depends on an identity the
    /// interpreter does not keep (and answers value equality then).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn same_reference(
        &mut self,
        frame: &mut FrameIdentity,
        file: &ClassFile,
        method: &str,
        desc: &str,
        is_static: bool,
        code: &[u8],
        at: usize,
        lhs: &Value,
        rhs: &Value,
    ) -> Result<bool, NativeError> {
        // The old answer, kept when the comparison is counted.
        let equal = lhs == rhs;
        // A host handle (`host_ref.rs`): different handles are different
        // objects and a canonical kind's equal handles one object; equal
        // handles of a by-value kind are one object only when the same
        // producer provably flows to both sides. Equal keys never answer
        // identity.
        if let (Value::Host(left), Value::Host(right)) = (lhs, rhs) {
            if left != right {
                return Ok(false);
            }
            if left.canonical() {
                return Ok(true);
            }
            let same = frame
                .flow
                .get_or_insert_with(|| flow(file, method, desc, is_static, code))
                .as_ref()
                .and_then(|flow| flow.compares.get(&(at as u32)).copied())
                .is_some_and(|(l, r)| l.source.is_some() && l.source == r.source && !l.repeats);
            if !same {
                self.host.unknown_operand_call(&MethodRef {
                    owner: "java/lang/Object".into(),
                    name: "if_acmp".into(),
                    descriptor: "(Ljava/lang/Object;Ljava/lang/Object;)Z".into(),
                })?;
            }
            return Ok(true);
        }
        let boxed = match (box_key(lhs), box_key(rhs)) {
            (Some(a), Some(b)) if a.same_box(&b) => Some(a),
            // Two boxes of different values or classes are different objects.
            (Some(_), Some(_)) => return Ok(false),
            _ => None,
        };
        let texts = match (lhs, rhs) {
            (Value::Ref(Some(a)), Value::Ref(Some(b))) => Some((a, b)),
            _ => None,
        };
        // A comparison only the producers can settle: two equal boxes, two
        // equal texts, or two collections that may be one shared object.
        let collections = texts.is_some_and(|(a, b)| a != b && maybe_collection(a, b));
        if boxed.is_none() && !texts.is_some_and(|(a, b)| a == b) && !collections {
            // Different values are different objects; null is null.
            return Ok(equal);
        }
        let operands = frame
            .flow
            .get_or_insert_with(|| flow(file, method, desc, is_static, code))
            .as_ref()
            .and_then(|flow| flow.compares.get(&(at as u32)).copied());
        let (family, identity) = if let Some(boxed) = boxed {
            let identity =
                operands.map_or(Identity::Unknown, |(l, r)| decide(Some(boxed), false, l, r));
            (Family::Box, identity)
        } else if collections {
            match operands {
                Some((l, r))
                    if matches!(l.kind, Kind::Shared(_)) || matches!(r.kind, Kind::Shared(_)) =>
                {
                    (Family::Collection, decide(None, false, l, r))
                }
                // Neither is a shared empty collection: two markers, two objects.
                _ => return Ok(false),
            }
        } else {
            let text = texts.map_or("", |(a, _)| a.as_str());
            let (left, right) = operands.map_or((Ty::Unknown, Ty::Unknown), |(l, r)| (l.ty, r.ty));
            // A reference the bytecode types as any class but `String` and
            // its supertypes is no `String`: a jar or host object, one marker
            // per object, compared as before (`System.out == System.out`,
            // `getEnvironmentType() == EnvType.SERVER`).
            if [left, right]
                .iter()
                .any(|ty| matches!(ty, Ty::Other | Ty::Array(_)))
            {
                return Ok(equal);
            }
            // Untyped (`Object`, an array element of unknown type): a jar
            // object's marker is one per allocation and a host model's marker
            // is no plain text; anything else may be a `String`.
            let typed = left == Ty::String || right == Ty::String;
            if !typed
                && (jar_text::object_marker_class(text).is_some()
                    || !value_class::is_plain_string(text))
            {
                return Ok(equal);
            }
            let identity = operands.map_or(Identity::Unknown, |(l, r)| {
                decide(None, text.is_empty(), l, r)
            });
            (Family::String, identity)
        };
        match identity {
            Identity::Same => Ok(true),
            Identity::Different => Ok(false),
            Identity::Unknown => {
                let owner = match family {
                    Family::String => "java/lang/String",
                    Family::Box | Family::Collection => "java/lang/Object",
                };
                self.host.unknown_operand_call(&MethodRef {
                    owner: owner.into(),
                    name: "if_acmp".into(),
                    descriptor: "(Ljava/lang/Object;Ljava/lang/Object;)Z".into(),
                })?;
                Ok(equal)
            }
        }
    }
}

/// Whether two different markers may both be a JDK shared empty collection
/// (the runtime builds one marker per `List.of()` call).
fn maybe_collection(a: &str, b: &str) -> bool {
    let collection = |marker: &str| {
        matches!(
            class_of_value(&Value::Ref(Some(marker.to_owned()))).as_deref(),
            Some(class) if class.starts_with("java/util/")
        ) || ["list", "set", "map"]
            .iter()
            .any(|kind| marker == *kind || marker.starts_with(&format!("{kind}/")))
    };
    collection(a) && collection(b)
}

/// A box as the interpreter holds it: a bare `Int`/`Long`/`Float` (lambda
/// boxing, `Long`/`Float`/`Double.valueOf`) or a [`Value::Boxed`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct BoxKey {
    /// The box class when the representation says it.
    class: Option<&'static str>,
    /// The integral value, or a float's `f64` bits.
    bits: i64,
    float: bool,
}

impl BoxKey {
    /// Whether the two may be one object: the same value (a float by its bits,
    /// so `0.0` and `-0.0` are two boxes) and no two different classes.
    fn same_box(&self, other: &BoxKey) -> bool {
        self.float == other.float
            && self.bits == other.bits
            && match (self.class, other.class) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }
}

fn box_key(value: &Value) -> Option<BoxKey> {
    Some(match value {
        Value::Int(v) => BoxKey {
            class: None,
            bits: i64::from(*v),
            float: false,
        },
        Value::Long(v) => BoxKey {
            class: Some("java/lang/Long"),
            bits: *v,
            float: false,
        },
        // A bare floating-point value is a `Double`; a `Float` is boxed.
        Value::Float(v) => BoxKey {
            class: Some("java/lang/Double"),
            bits: v.to_bits() as i64,
            float: true,
        },
        Value::Boxed(Boxed::Float(bits)) => BoxKey {
            class: Some("java/lang/Float"),
            bits: i64::from(*bits),
            float: true,
        },
        Value::Boxed(boxed) => BoxKey {
            class: Some(boxed.class()),
            bits: i64::from(boxed.int()),
            float: false,
        },
        _ => return None,
    })
}

/// Java's identity for two references holding the same value (`boxed` is
/// that value when it is a box; `empty` says the strings are `""`).
fn decide(boxed: Option<BoxKey>, empty: bool, left: Operand, right: Operand) -> Identity {
    if left.source.is_some() && left.source == right.source && !left.repeats {
        return Identity::Same;
    }
    // Two different instructions, or one that may have run twice.
    let distinct = left.source != right.source;
    let separate = |identity| {
        if distinct {
            identity
        } else {
            Identity::Unknown
        }
    };
    let text = |kind| match kind {
        Kind::BuilderText if empty => Kind::Interned,
        Kind::BuilderText => Kind::Fresh,
        other => other,
    };
    match (text(left.kind), text(right.kind)) {
        (Kind::Shared(a), Kind::Shared(b)) => {
            if a == b {
                Identity::Same
            } else {
                Identity::Different
            }
        }
        (Kind::Shared(_), Kind::Fresh) | (Kind::Fresh, Kind::Shared(_)) => Identity::Different,
        (Kind::Opaque | Kind::Shared(_), _) | (_, Kind::Opaque | Kind::Shared(_)) => {
            Identity::Unknown
        }
        (Kind::Interned, Kind::Interned) => Identity::Same,
        // `intern()` may have put the fresh string in the pool.
        (Kind::Interned, _) | (_, Kind::Interned) => Identity::Unknown,
        (Kind::Cached(a) | Kind::Uncached(a), Kind::Cached(b) | Kind::Uncached(b)) if a != b => {
            Identity::Different
        }
        (Kind::Cached(class), Kind::Cached(_))
            if boxed.is_some_and(|value| in_cache(class, value.bits)) =>
        {
            Identity::Same
        }
        // Each run of a fresh producer (or a box outside its cache) makes its
        // own object.
        _ => separate(Identity::Different),
    }
}

/// Whether `valueOf` of `class` answers its cached object for `value`.
fn in_cache(class: &str, value: i64) -> bool {
    match class {
        "java/lang/Byte" | "java/lang/Boolean" => true,
        "java/lang/Character" => (0..=127).contains(&value),
        "java/lang/Integer" | "java/lang/Short" | "java/lang/Long" => (-128..=127).contains(&value),
        _ => false,
    }
}

/// The method's comparisons: from the thread's table when the same code was
/// analysed before, or a fresh analysis. `None` when the pass gave up.
fn flow(
    file: &ClassFile,
    method: &str,
    desc: &str,
    is_static: bool,
    code: &[u8],
) -> Option<Rc<Flow>> {
    let key = format!("{}.{method}{desc}", file.name);
    let cached = FLOWS.with(|cache| {
        cache
            .borrow()
            .0
            .get(&key)
            .filter(|(bytes, _)| **bytes == *code)
            .map(|(_, flow)| flow.clone())
    });
    if let Some(flow) = cached {
        return flow;
    }
    let flow = analyse(file, method, desc, is_static, code).map(Rc::new);
    FLOWS.with(|cache| {
        let (table, bytes) = &mut *cache.borrow_mut();
        if table.len() >= MAX_CACHED_METHODS || *bytes + code.len() > MAX_CACHED_CODE_BYTES {
            table.clear();
            *bytes = 0;
        }
        *bytes += code.len();
        table.insert(key, (Rc::from(code), flow.clone()));
    });
    flow
}

/// The byte length of the instruction at `pc`.
fn instruction_length(code: &[u8], pc: usize) -> Option<usize> {
    let op = *code.get(pc)?;
    match op {
        0xaa | 0xab => {
            let base = pc + 1 + weaver::switch_padding(pc);
            let i4 = |at: usize| Some(i32::from_be_bytes(code.get(at..at + 4)?.try_into().ok()?));
            let entries = if op == 0xaa {
                usize::try_from(i4(base + 8)?.checked_sub(i4(base + 4)?)?.checked_add(1)?).ok()? * 4
            } else {
                usize::try_from(i4(base + 4)?).ok()? * 8
            };
            Some(base + 12 + entries - pc - if op == 0xab { 4 } else { 0 })
        }
        0xc4 => weaver::wide_instruction_size("", code, pc).ok(),
        _ => weaver::instruction_size(op),
    }
}

/// Run the dataflow over the method's basic blocks and describe both
/// operands of every reachable `if_acmp`.
fn analyse(
    file: &ClassFile,
    method: &str,
    desc: &str,
    is_static: bool,
    code: &[u8],
) -> Option<Flow> {
    let handlers = file.exception_handlers(method, desc).ok()?;
    // Instruction starts, and each one's block.
    let mut starts = Vec::new();
    let mut pc = 0;
    while pc < code.len() {
        starts.push(pc);
        pc += instruction_length(code, pc)?;
    }
    let mut leader = vec![false; code.len() + 1];
    leader[0] = true;
    for handler in &handlers {
        *leader.get_mut(handler.handler_pc as usize)? = true;
    }
    for &pc in &starts {
        let targets = successors(code, pc)?;
        let next = pc + instruction_length(code, pc)?;
        if targets != [next] {
            for target in targets {
                *leader.get_mut(target)? = true;
            }
            leader[next] = true;
        }
    }
    let blocks: Vec<usize> = starts.iter().copied().filter(|pc| leader[*pc]).collect();
    let mut block_of = vec![0usize; code.len()];
    let mut current = 0;
    for &pc in &starts {
        if leader[pc] {
            current = blocks.binary_search(&pc).ok()?;
        }
        block_of[pc] = current;
    }
    let ends: Vec<usize> = (0..blocks.len())
        .map(|b| blocks.get(b + 1).copied().unwrap_or(code.len()))
        .collect();

    let mut locals = Vec::new();
    if !is_static {
        locals.push(Slot {
            source: Some(Source::Param(0)),
            ty: Ty::Other,
        });
    }
    for argument in descriptor_arguments(desc) {
        locals.push(Slot {
            source: Some(Source::Param(locals.len() as u16)),
            ty: ty_of(&argument),
        });
        if argument == "J" || argument == "D" {
            locals.push(TOP);
        }
    }
    let mut entry: Vec<Option<State>> = vec![None; blocks.len()];
    entry[0] = Some(State {
        locals,
        stack: Vec::new(),
    });
    let mut queued = vec![false; blocks.len()];
    let mut work = vec![0usize];
    queued[0] = true;
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); blocks.len()];
    let mut compares = BTreeMap::new();
    let mut done = 0usize;
    let merge = |entry: &mut Vec<Option<State>>,
                 work: &mut Vec<usize>,
                 queued: &mut Vec<bool>,
                 target: usize,
                 locals: &[Slot],
                 stack: &[Slot]|
     -> Option<()> {
        let changed = match &mut entry[target] {
            Some(existing) => existing.merge(locals, stack)?,
            slot @ None => {
                *slot = Some(State {
                    locals: locals.to_vec(),
                    stack: stack.to_vec(),
                });
                true
            }
        };
        if changed && !queued[target] {
            queued[target] = true;
            work.push(target);
        }
        Some(())
    };
    while let Some(block) = work.pop() {
        queued[block] = false;
        let mut state = entry[block].clone()?;
        charge(&mut done, state.locals.len() + state.stack.len())?;
        let mut pc = blocks[block];
        loop {
            charge(&mut done, 1)?;
            // A handler starts with the locals of any instruction it covers.
            for handler in &handlers {
                if (handler.start_pc as usize..handler.end_pc as usize).contains(&pc) {
                    let target = block_of[handler.handler_pc as usize];
                    charge(&mut done, state.locals.len() + 1)?;
                    merge(
                        &mut entry,
                        &mut work,
                        &mut queued,
                        target,
                        &state.locals,
                        &[TOP],
                    )?;
                    if !edges[block].contains(&target) {
                        edges[block].push(target);
                    }
                }
            }
            if matches!(code[pc], 0xa5 | 0xa6) {
                let n = state.stack.len();
                let rhs = *state.stack.get(n.checked_sub(1)?)?;
                let lhs = *state.stack.get(n.checked_sub(2)?)?;
                compares.insert(pc as u32, (lhs, rhs));
            }
            step(file, code, pc, &mut state)?;
            let targets = successors(code, pc)?;
            let next = pc + instruction_length(code, pc)?;
            if next < ends[block] && targets == [next] {
                pc = next;
                continue;
            }
            for target in targets {
                let target = block_of[target];
                charge(&mut done, state.locals.len() + state.stack.len())?;
                merge(
                    &mut entry,
                    &mut work,
                    &mut queued,
                    target,
                    &state.locals,
                    &state.stack,
                )?;
                if !edges[block].contains(&target) {
                    edges[block].push(target);
                }
            }
            break;
        }
    }
    let cyclic = cyclic_blocks(&edges);
    let operand = |slot: Slot| Operand {
        source: slot.source,
        ty: slot.ty,
        kind: slot
            .source
            .map_or(Kind::Opaque, |source| kind(file, code, source)),
        repeats: match slot.source {
            Some(Source::At(pc)) => cyclic[block_of[pc as usize]],
            _ => false,
        },
    };
    Some(Flow {
        compares: compares
            .into_iter()
            .map(|(pc, (lhs, rhs))| (pc, (operand(lhs), operand(rhs))))
            .collect(),
    })
}

/// The blocks that lie on a cycle of the control flow (exception edges
/// included): Tarjan's strongly connected components, iteratively, in one
/// pass over the edges.
fn cyclic_blocks(edges: &[Vec<usize>]) -> Vec<bool> {
    let n = edges.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut cyclic = vec![false; n];
    let mut next = 0;
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        // (block, next edge to look at)
        let mut calls = vec![(root, 0usize)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (block, ref mut edge)) = calls.last_mut() {
            if let Some(&target) = edges[block].get(*edge) {
                *edge += 1;
                if target == block {
                    cyclic[block] = true;
                }
                if index[target] == usize::MAX {
                    index[target] = next;
                    low[target] = next;
                    next += 1;
                    stack.push(target);
                    on_stack[target] = true;
                    calls.push((target, 0));
                } else if on_stack[target] {
                    low[block] = low[block].min(index[target]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(parent, _)) = calls.last() {
                low[parent] = low[parent].min(low[block]);
            }
            if low[block] == index[block] {
                let mut members = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    members.push(member);
                    if member == block {
                        break;
                    }
                }
                if members.len() > 1 {
                    for member in members {
                        cyclic[member] = true;
                    }
                }
            }
        }
    }
    cyclic
}

/// What the producer at `source` makes.
fn kind(file: &ClassFile, code: &[u8], source: Source) -> Kind {
    let Source::At(pc) = source else {
        return Kind::Opaque;
    };
    let pc = pc as usize;
    let index = |wide: bool| -> Option<u16> {
        if wide {
            Some(u16::from_be_bytes([*code.get(pc + 1)?, *code.get(pc + 2)?]))
        } else {
            code.get(pc + 1).map(|&i| u16::from(i))
        }
    };
    match code.get(pc).copied() {
        Some(op @ (0x12 | 0x13)) => match index(op == 0x13).and_then(|i| file.pool.get(i as usize))
        {
            Some(Cp::String(_)) => Kind::Interned,
            _ => Kind::Opaque,
        },
        Some(0xbb) => Kind::Fresh,
        Some(0xba) => {
            let Some(i) = index(true) else {
                return Kind::Opaque;
            };
            match file.invoke_dynamic_name(i).ok().as_deref() {
                // `"" + b` of one `boolean` and nothing else is JDK 25's
                // `StringConcatFactory.unaryConcat` → `booleanStringifier`,
                // i.e. `String.valueOf(boolean)`: the `"true"`/`"false"`
                // literals.
                Some("makeConcatWithConstants" | "makeConcat") if unary_boolean(file, i) => {
                    Kind::Interned
                }
                // Every other concatenation is a new `String` (JLS 15.18.1;
                // JDK 25's `StringConcatHelper` copies even `"" + s`, and the
                // unary `char`/`int`/`String`/`Object` forms make one too).
                Some("makeConcatWithConstants" | "makeConcat") => Kind::Fresh,
                _ => Kind::Opaque,
            }
        }
        Some(0xb2) => match index(true).and_then(|i| file.field_ref(i).ok()) {
            Some(f)
                if f.owner == "java/lang/Boolean" && (f.name == "TRUE" || f.name == "FALSE") =>
            {
                Kind::Cached("java/lang/Boolean")
            }
            Some(f) if f.owner == "java/util/Collections" => match f.name.as_str() {
                "EMPTY_LIST" => Kind::Shared("Collections.EMPTY_LIST"),
                "EMPTY_SET" => Kind::Shared("Collections.EMPTY_SET"),
                "EMPTY_MAP" => Kind::Shared("Collections.EMPTY_MAP"),
                _ => Kind::Opaque,
            },
            _ => Kind::Opaque,
        },
        Some(op @ (0xb6 | 0xb8 | 0xb9)) => {
            let Some(m) = index(true).and_then(|i| file.method_ref(i).ok()) else {
                return Kind::Opaque;
            };
            let returns_own = return_desc(&m.descriptor)
                .strip_prefix('L')
                .and_then(|rest| rest.strip_suffix(';'))
                == Some(m.owner.as_str());
            let no_args = m.descriptor.starts_with("()");
            match (op, m.owner.as_str(), m.name.as_str()) {
                (0xb8, owner, "valueOf") if returns_own => {
                    if let Some(class) = CACHED_BOXES.iter().find(|c| **c == owner) {
                        Kind::Cached(class)
                    } else if let Some(class) = UNCACHED_BOXES.iter().find(|c| **c == owner) {
                        Kind::Uncached(class)
                    } else {
                        Kind::Opaque
                    }
                }
                // `List.of()` is `ImmutableCollections.EMPTY_LIST`, and
                // `Collections.emptyList()` is `Collections.EMPTY_LIST`.
                (0xb8, "java/util/List", "of") if no_args => {
                    Kind::Shared("ImmutableCollections.EMPTY_LIST")
                }
                (0xb8, "java/util/Set", "of") if no_args => {
                    Kind::Shared("ImmutableCollections.EMPTY_SET")
                }
                (0xb8, "java/util/Map", "of") if no_args => {
                    Kind::Shared("ImmutableCollections.EMPTY_MAP")
                }
                (0xb8, "java/util/Collections", "emptyList") => {
                    Kind::Shared("Collections.EMPTY_LIST")
                }
                (0xb8, "java/util/Collections", "emptySet") => {
                    Kind::Shared("Collections.EMPTY_SET")
                }
                (0xb8, "java/util/Collections", "emptyMap") => {
                    Kind::Shared("Collections.EMPTY_MAP")
                }
                (0xb6, "java/lang/String", "intern") => Kind::Interned,
                (0xb6, "java/lang/StringBuilder" | "java/lang/StringBuffer", "toString") => {
                    Kind::BuilderText
                }
                _ => Kind::Opaque,
            }
        }
        _ => Kind::Opaque,
    }
}

/// Words a field or return descriptor occupies on the operand stack.
fn words(descriptor: &str) -> usize {
    match descriptor.as_bytes().first() {
        Some(b'V') => 0,
        Some(b'J' | b'D') => 2,
        _ => 1,
    }
}

/// (words popped, words pushed) of every opcode whose effect needs no operand.
/// `None` for `jsr`/`ret`, which the pass does not follow.
fn fixed_effect(op: u8) -> Option<(usize, usize)> {
    Some(match op {
        0x00 | 0x84 | 0xa7 | 0xb1 | 0xc8 => (0, 0),
        0x01..=0x08 | 0x0b..=0x0d | 0x10 | 0x11 => (0, 1),
        0x09 | 0x0a | 0x0e | 0x0f | 0x14 => (0, 2),
        0x57 | 0x99..=0x9e | 0xaa | 0xab | 0xac | 0xae | 0xb0 | 0xbf | 0xc2 | 0xc3 => (1, 0),
        0xc6 | 0xc7 => (1, 0),
        0x58 | 0x9f..=0xa6 | 0xad | 0xaf => (2, 0),
        0x4f | 0x51 | 0x53..=0x56 => (3, 0),
        0x50 | 0x52 => (4, 0),
        0x74 | 0x76 | 0x86 | 0x8b | 0x91..=0x93 | 0xbc | 0xbd | 0xbe | 0xc1 => (1, 1),
        0x85 | 0x87 | 0x8c | 0x8d => (1, 2),
        0x2e | 0x30 | 0x32..=0x35 | 0x88 | 0x89 | 0x8e | 0x90 | 0x95 | 0x96 => (2, 1),
        0x2f | 0x31 | 0x75 | 0x77 | 0x8a | 0x8f => (2, 2),
        // int/float arithmetic, shifts and bitwise ops are the even opcodes
        0x60..=0x73 | 0x7e..=0x83 if op.is_multiple_of(2) => (2, 1),
        0x78 | 0x7a | 0x7c => (2, 1),
        0x79 | 0x7b | 0x7d => (3, 2),
        0x60..=0x73 | 0x7e..=0x83 => (4, 2),
        0x94 | 0x97 | 0x98 => (4, 1),
        _ => return None,
    })
}

/// Apply the instruction at `pc` to `state` (the JVM's operand stack in
/// words, so `dup2` and `pop2` need no types). `None` for `jsr`/`ret`.
fn step(file: &ClassFile, code: &[u8], pc: usize, state: &mut State) -> Option<()> {
    let op = *code.get(pc)?;
    let u1 = |at: usize| code.get(pc + at).copied();
    let u2 = |at: usize| Some(u16::from_be_bytes([u1(at)?, u1(at + 1)?]));
    let produced = Slot {
        source: Some(Source::At(pc as u32)),
        ty: Ty::Unknown,
    };
    let class_ty = |index: u16| class_name(&file.pool, index).ok().map(|c| ty_of_class(&c));
    let stack = &mut state.stack;
    match op {
        0x15..=0x19 => load(state, op - 0x15, usize::from(u1(1)?)),
        0x1a..=0x2d => load(state, (op - 0x1a) / 4, usize::from((op - 0x1a) % 4)),
        0x36..=0x3a => store(state, op - 0x36, usize::from(u1(1)?))?,
        0x3b..=0x4e => store(state, (op - 0x3b) / 4, usize::from((op - 0x3b) % 4))?,
        0xc4 => match u1(1)? {
            inner @ 0x15..=0x19 => load(state, inner - 0x15, usize::from(u2(2)?)),
            inner @ 0x36..=0x3a => store(state, inner - 0x36, usize::from(u2(2)?))?,
            0x84 => {}
            _ => return None,
        },
        // dup, dup_x1, dup_x2, dup2, dup2_x1, dup2_x2: copy the top n words
        // below the m words under them.
        0x59..=0x5e => {
            let (n, m) = [(1, 0), (1, 1), (1, 2), (2, 0), (2, 1), (2, 2)][usize::from(op - 0x59)];
            let len = stack.len();
            let at = len.checked_sub(n + m)?;
            let top = stack[len - n..].to_vec();
            stack.splice(at..at, top);
        }
        0x5f => {
            let len = stack.len();
            stack.swap(len.checked_sub(1)?, len.checked_sub(2)?);
        }
        0x12 | 0x13 => {
            let index = if op == 0x12 {
                u16::from(u1(1)?)
            } else {
                u2(1)?
            };
            let ty = match file.pool.get(index as usize) {
                Some(Cp::String(_)) => Ty::String,
                _ => Ty::Other,
            };
            stack.push(Slot { ty, ..produced });
        }
        0xb2..=0xb5 => {
            let field = file.field_ref(u2(1)?).ok()?;
            let size = words(&field.descriptor);
            pop(stack, [0, size, 1, 1 + size][usize::from(op - 0xb2)])?;
            if matches!(op, 0xb2 | 0xb4) {
                push_result(stack, &field.descriptor, produced);
            }
        }
        0xb6..=0xba => {
            let descriptor = if op == 0xba {
                file.invoke_dynamic_descriptor(u2(1)?).ok()?
            } else {
                file.method_ref(u2(1)?).ok()?.descriptor
            };
            let arguments: usize = descriptor_arguments(&descriptor)
                .iter()
                .map(|a| words(a))
                .sum();
            pop(
                stack,
                arguments + usize::from(matches!(op, 0xb6 | 0xb7 | 0xb9)),
            )?;
            push_result(stack, return_desc(&descriptor), produced);
        }
        0xbb => {
            let ty = match class_ty(u2(1)?)? {
                Ty::String => Ty::String,
                _ => Ty::Other,
            };
            stack.push(Slot { ty, ..produced });
        }
        // checkcast keeps the reference and narrows its type
        0xc0 => {
            let wanted = class_ty(u2(1)?)?;
            let top = stack.last_mut()?;
            if wanted != Ty::Maybe || top.ty == Ty::Unknown {
                top.ty = wanted;
            }
        }
        // aaload: the element type of the array under the index
        0x32 => {
            let len = stack.len();
            let ty = match stack.get(len.checked_sub(2)?)?.ty {
                Ty::Array(elem) => elem.ty(),
                _ => Ty::Unknown,
            };
            pop(stack, 2)?;
            stack.push(Slot { ty, ..produced });
        }
        0xbc => {
            pop(stack, 1)?;
            stack.push(Slot {
                ty: Ty::Array(Elem::Other),
                ..produced
            });
        }
        0xbd => {
            let ty = Ty::Array(Elem::of(class_ty(u2(1)?)?));
            pop(stack, 1)?;
            stack.push(Slot { ty, ..produced });
        }
        0xc5 => {
            let ty = class_ty(u2(1)?)?;
            pop(stack, usize::from(u1(3)?))?;
            stack.push(Slot { ty, ..produced });
        }
        _ => {
            let (popped, pushed) = fixed_effect(op)?;
            pop(stack, popped)?;
            match pushed {
                1 => stack.push(produced),
                n => stack.extend(std::iter::repeat_n(TOP, n)),
            }
        }
    }
    Some(())
}

/// Where control goes after the instruction at `pc` (weaver.rs decodes
/// sizes and branches).
fn successors(code: &[u8], pc: usize) -> Option<Vec<usize>> {
    let op = *code.get(pc)?;
    let i4 = |at: usize| {
        Some(i32::from_be_bytes(
            code.get(pc + at..pc + at + 4)?.try_into().ok()?,
        ))
    };
    let mut targets = Vec::new();
    if let Some((_, target)) = weaver::branch_target(op, code, pc).ok()? {
        targets.push(target);
    }
    if matches!(op, 0xaa | 0xab) {
        let base = 1 + weaver::switch_padding(pc);
        let jump = |offset: i32| pc.checked_add_signed(offset as isize);
        targets.push(jump(i4(base)?)?);
        let (count, stride) = if op == 0xaa {
            (i4(base + 8)?.checked_sub(i4(base + 4)?)?.checked_add(1)?, 4)
        } else {
            (i4(base + 4)?, 8)
        };
        for k in 0..usize::try_from(count).ok()? {
            targets.push(jump(i4(base + 12 + stride * k)?)?);
        }
    } else if !matches!(op, 0xa7 | 0xa8 | 0xa9 | 0xac..=0xb1 | 0xbf | 0xc8 | 0xc9) {
        targets.push(pc + instruction_length(code, pc)?);
    }
    targets
        .iter()
        .all(|target| *target < code.len())
        .then_some(targets)
}

/// Drop the top `n` words.
fn pop(stack: &mut Vec<Slot>, n: usize) -> Option<()> {
    stack.truncate(stack.len().checked_sub(n)?);
    Some(())
}

/// Push a field's or call's result: a reference keeps its producer.
fn push_result(stack: &mut Vec<Slot>, descriptor: &str, produced: Slot) {
    match words(descriptor) {
        0 => {}
        1 if descriptor.starts_with(['L', '[']) => stack.push(Slot {
            ty: ty_of(descriptor),
            ..produced
        }),
        n => stack.extend(std::iter::repeat_n(TOP, n)),
    }
}

/// `xload index`, `kind` 0..=4 for i, l, f, d, a.
fn load(state: &mut State, kind: u8, index: usize) {
    match kind {
        4 => {
            let slot = state.locals.get(index).copied().unwrap_or(TOP);
            state.stack.push(slot);
        }
        1 | 3 => state.stack.extend([TOP, TOP]),
        _ => state.stack.push(TOP),
    }
}

/// `xstore index`, `kind` 0..=4 for i, l, f, d, a.
fn store(state: &mut State, kind: u8, index: usize) -> Option<()> {
    let wide = matches!(kind, 1 | 3);
    let width = if wide { 2 } else { 1 };
    let len = state.stack.len().checked_sub(width)?;
    let value = if kind == 4 { state.stack[len] } else { TOP };
    state.stack.truncate(len);
    if state.locals.len() < index + width {
        state.locals.resize(index + width, TOP);
    }
    state.locals[index] = value;
    if wide {
        state.locals[index + 1] = TOP;
    }
    Some(())
}

/// Whether the `invokedynamic` at constant `index` concatenates exactly one
/// `boolean` and nothing else (`makeConcat`, or `makeConcatWithConstants`
/// whose recipe is a single argument slot).
fn unary_boolean(file: &ClassFile, index: u16) -> bool {
    if file.invoke_dynamic_descriptor(index).ok().as_deref() != Some("(Z)Ljava/lang/String;") {
        return false;
    }
    if file.invoke_dynamic_name(index).ok().as_deref() == Some("makeConcat") {
        return true;
    }
    let Some(Cp::InvokeDynamic { bootstrap, .. }) = file.pool.get(index as usize) else {
        return false;
    };
    let recipe = file
        .bootstrap_methods
        .get(*bootstrap as usize)
        .and_then(|method| method.arguments.first())
        .and_then(|argument| match file.pool.get(*argument as usize) {
            Some(Cp::String(text)) => match file.pool.get(*text as usize) {
                Some(Cp::Utf8(text)) => Some(text.as_str()),
                _ => None,
            },
            _ => None,
        });
    recipe == Some("\u{1}")
}
