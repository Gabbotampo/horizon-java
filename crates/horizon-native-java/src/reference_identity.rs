//! `if_acmpeq`/`if_acmpne` on boxes and strings: Java's object identity, or a
//! count.
//!
//! The interpreter holds a box as its value (`Integer` 1000 is `Value::Int`)
//! and a string as its text, so two references compared with `==` used to be
//! "the same object" whenever their values were equal. Java answers from
//! identity instead (JDK 25, verified on OpenJDK 25.0.5):
//!
//! * `Integer`/`Short`/`Long` `valueOf` (and so autoboxing) returns a cached
//!   object for -128..=127, `Character.valueOf` for 0..=127, `Byte.valueOf`
//!   always, `Boolean.valueOf` the `TRUE`/`FALSE` singletons; outside its
//!   cache every call makes a new object, so `Integer a = 1000, b = 1000;
//!   a == b` is `false` while `a == a` is `true`. The `valueOf(String)`
//!   overloads go through the same caches. `Float`/`Double` are never cached.
//!   (The `Integer` cache's upper bound can be raised with
//!   `-XX:AutoBoxCacheMax`; the game's launcher does not, so 127 holds.)
//! * `new Integer(..)`, `new String(..)`, string concatenation and
//!   `StringBuilder`/`StringBuffer.toString()` always make a new object.
//! * String literals (and compile-time constants, which javac folds into one
//!   literal) are interned: one object per text, as is `intern()`'s answer.
//!
//! What a value cannot say is where it came from, so this module reads the
//! method's own bytecode: a small dataflow pass ([`analyse`]) finds, for both
//! operands of each `if_acmp`, the one instruction (or parameter) that produced
//! the reference, through locals, `dup`s and `checkcast`s. When two equal
//! values are compared, [`decide`] answers from those producers:
//!
//! * the same producer that runs at most once per frame (not inside a loop):
//!   the same reference flowed to both operands, so `true`;
//! * two cached boxes of the same type within the cache, or two interned
//!   strings: `true`; of different box types: `false`;
//! * two separate allocations (distinct producers, at least one of which
//!   always makes a new object or a box outside the cache): `false`;
//! * anything else (a box read from a field or a collection, a method's
//!   answer, a parameter, one producer inside a loop that may have run twice):
//!   Java's answer depends on an identity the interpreter does not keep, so
//!   the comparison is counted as an unknown host call
//!   (`java/lang/Object#if_acmp` for boxes, `java/lang/String#if_acmp` for
//!   strings) and keeps the old value-equality answer.
//!
//! Values that differ are different objects, so unequal operands (and null)
//! keep their exact answer without any analysis, and so do references whose
//! text is a jar object's `class#n` marker (one per allocation). Other equal
//! references that are not statically `String`s (a host model's markers) keep
//! the value-equality answer as before.

use super::*;
use std::{collections::BTreeMap, rc::Rc};

/// A method's analysis is cached per thread, keyed by its class, name and
/// descriptor and checked against the code it was computed from.
const MAX_CACHED_METHODS: usize = 1024;

thread_local! {
    static FLOWS: RefCell<BTreeMap<String, (usize, usize, Rc<Flow>)>> =
        const { RefCell::new(BTreeMap::new()) };
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
    /// Statically a `java.lang.String`.
    string: bool,
}

const TOP: Slot = Slot {
    source: None,
    string: false,
};

impl Slot {
    fn merge(self, other: Slot) -> Slot {
        Slot {
            source: (self.source == other.source)
                .then_some(self.source)
                .flatten(),
            string: self.string && other.string,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    locals: Vec<Slot>,
    stack: Vec<Slot>,
}

impl State {
    /// Merge `other` in; `Err` for stack heights a verified class never has.
    fn merge(&mut self, other: &State) -> Result<bool, ()> {
        if self.stack.len() != other.stack.len() {
            return Err(());
        }
        let before = self.clone();
        if self.locals.len() < other.locals.len() {
            self.locals.resize(other.locals.len(), TOP);
        }
        for (index, slot) in self.locals.iter_mut().enumerate() {
            *slot = slot.merge(other.locals.get(index).copied().unwrap_or(TOP));
        }
        for (slot, other) in self.stack.iter_mut().zip(&other.stack) {
            *slot = slot.merge(*other);
        }
        Ok(*self != before)
    }
}

/// What one producer makes, as far as identity goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// A `valueOf` (or `Boolean.TRUE`/`FALSE`) of this cached box class.
    Cached(&'static str),
    /// `Float`/`Double.valueOf`: a new box every time.
    Uncached(&'static str),
    /// `new`, concatenation, `StringBuilder.toString()`: a new object.
    Fresh,
    /// A string literal or `intern()`: the one object per text.
    Interned,
    /// Anything else: identity unknown.
    Opaque,
}

/// One `if_acmp` operand: its producer, what it makes, and whether that
/// producer may run more than once per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Operand {
    source: Option<Source>,
    string: bool,
    kind: Kind,
    repeats: bool,
}

/// Every `if_acmp` of one method: pc → (lhs, rhs).
#[derive(Debug, Default)]
struct Flow {
    compares: BTreeMap<u32, (Operand, Operand)>,
}

/// The answer for one comparison of two equal values.
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

impl<H: Host> Runtime<'_, H> {
    /// Whether `lhs` and `rhs` are the same object at the `if_acmp` at `at`.
    /// Counts the comparison when Java's answer depends on an identity the
    /// interpreter does not keep (and answers value equality then).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn same_reference(
        &mut self,
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
        let family = match (box_key(lhs), box_key(rhs)) {
            (Some(a), Some(b)) if a.same_box(&b) => "box",
            // Two boxes of different values or classes are different objects.
            (Some(_), Some(_)) => return Ok(false),
            _ => match (lhs, rhs) {
                (Value::Ref(Some(a)), Value::Ref(Some(b)))
                    if a == b && jar_text::object_marker_class(a).is_none() =>
                {
                    "string"
                }
                // Different values are different objects; null is null.
                _ => return Ok(equal),
            },
        };
        let identity = match flow(file, method, desc, is_static, code)
            .and_then(|flow| flow.compares.get(&(at as u32)).copied())
        {
            Some((left, right)) if family == "string" && !(left.string || right.string) => {
                // Not statically a `String`: a host model's marker, compared
                // as before.
                return Ok(equal);
            }
            Some((left, right)) => decide(box_key(lhs), left, right),
            None if family == "string" => return Ok(equal),
            None => Identity::Unknown,
        };
        match identity {
            Identity::Same => Ok(true),
            Identity::Different => Ok(false),
            Identity::Unknown => {
                let owner = if family == "box" {
                    "java/lang/Object"
                } else {
                    "java/lang/String"
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

/// A box as the interpreter holds it: a bare `Int`/`Long`/`Float` (lambda
/// boxing, `Long`/`Float`/`Double.valueOf`) or a typed marker
/// (`integer:1000`, `short:5`, `character:97`, `boolean:true`).
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
    let integral = |class, bits| BoxKey {
        class,
        bits,
        float: false,
    };
    let float = |class, value: f64| BoxKey {
        class,
        bits: value.to_bits() as i64,
        float: true,
    };
    Some(match value {
        Value::Int(v) => integral(None, i64::from(*v)),
        Value::Long(v) => integral(Some("java/lang/Long"), *v),
        Value::Float(v) => float(None, *v),
        Value::Ref(Some(text)) => {
            let (kind, number) = text.split_once(':')?;
            match kind {
                "integer" => integral(Some("java/lang/Integer"), number.parse().ok()?),
                "short" => integral(Some("java/lang/Short"), number.parse().ok()?),
                "byte" => integral(Some("java/lang/Byte"), number.parse().ok()?),
                "character" => integral(Some("java/lang/Character"), number.parse().ok()?),
                "long" => integral(Some("java/lang/Long"), number.parse().ok()?),
                "boolean" => integral(
                    Some("java/lang/Boolean"),
                    i64::from(number.parse::<bool>().ok()?),
                ),
                "double" => float(Some("java/lang/Double"), number.parse().ok()?),
                "float" => float(Some("java/lang/Float"), number.parse().ok()?),
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// Java's identity for two references holding the same value (`boxed` is that
/// value when it is a box, `None` for a string).
fn decide(boxed: Option<BoxKey>, left: Operand, right: Operand) -> Identity {
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
    match (left.kind, right.kind) {
        (Kind::Opaque, _) | (_, Kind::Opaque) => Identity::Unknown,
        (Kind::Interned, Kind::Interned) => Identity::Same,
        (Kind::Cached(a) | Kind::Uncached(a), Kind::Cached(b) | Kind::Uncached(b)) if a != b => {
            Identity::Different
        }
        (Kind::Cached(class), Kind::Cached(_))
            if boxed.is_some_and(|value| in_cache(class, value.bits)) =>
        {
            Identity::Same
        }
        // Each run of a fresh producer (or a box outside its cache) makes its
        // own object; an interned string is never one of them.
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

/// The method's comparisons, from the cache or a fresh analysis. `None` when
/// the bytecode is outside what the pass reads (`jsr`/`ret`, a malformed
/// frame).
fn flow(
    file: &ClassFile,
    method: &str,
    desc: &str,
    is_static: bool,
    code: &[u8],
) -> Option<Rc<Flow>> {
    let key = format!("{}.{method}{desc}", file.name);
    let identity = (code.as_ptr() as usize, code.len());
    if let Some(flow) = FLOWS.with(|cache| {
        cache
            .borrow()
            .get(&key)
            .filter(|(ptr, len, _)| (*ptr, *len) == identity)
            .map(|(_, _, flow)| Rc::clone(flow))
    }) {
        return Some(flow);
    }
    let flow = Rc::new(analyse(file, method, desc, is_static, code)?);
    FLOWS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= MAX_CACHED_METHODS {
            cache.clear();
        }
        cache.insert(key, (identity.0, identity.1, Rc::clone(&flow)));
    });
    Some(flow)
}

/// Run the dataflow over `code` and describe both operands of every
/// reachable `if_acmp`.
fn analyse(
    file: &ClassFile,
    method: &str,
    desc: &str,
    is_static: bool,
    code: &[u8],
) -> Option<Flow> {
    let handlers = file.exception_handlers(method, desc).ok()?;
    let mut locals = Vec::new();
    if !is_static {
        locals.push(Slot {
            source: Some(Source::Param(0)),
            string: false,
        });
    }
    for argument in descriptor_arguments(desc) {
        let slot = Slot {
            source: Some(Source::Param(locals.len() as u16)),
            string: argument == "Ljava/lang/String;",
        };
        locals.push(slot);
        if argument == "J" || argument == "D" {
            locals.push(TOP);
        }
    }
    let mut states: Vec<Option<State>> = vec![None; code.len()];
    *states.get_mut(0)? = Some(State {
        locals,
        stack: Vec::new(),
    });
    let mut work = vec![0usize];
    let mut compares = BTreeMap::new();
    let mut edges: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    while let Some(pc) = work.pop() {
        let state = states[pc].clone()?;
        // A handler starts with the locals of any instruction it covers.
        let mut successors: Vec<(usize, State)> = handlers
            .iter()
            .filter(|h| (h.start_pc as usize..h.end_pc as usize).contains(&pc))
            .map(|h| {
                (
                    h.handler_pc as usize,
                    State {
                        locals: state.locals.clone(),
                        stack: vec![TOP],
                    },
                )
            })
            .collect();
        let mut after = state.clone();
        let targets = step(file, code, pc, &mut after)?;
        if matches!(code[pc], 0xa5 | 0xa6) {
            let n = state.stack.len();
            let (lhs, rhs) = (
                *state.stack.get(n.checked_sub(2)?)?,
                *state.stack.get(n - 1)?,
            );
            compares.insert(pc as u32, (lhs, rhs));
        }
        successors.extend(targets.into_iter().map(|target| (target, after.clone())));
        edges.insert(pc, successors.iter().map(|(target, _)| *target).collect());
        for (target, incoming) in successors {
            let slot = states.get_mut(target)?;
            let changed = match slot {
                Some(existing) => existing.merge(&incoming).ok()?,
                None => {
                    *slot = Some(incoming);
                    true
                }
            };
            if changed {
                work.push(target);
            }
        }
    }
    let operand = |slot: Slot| Operand {
        source: slot.source,
        string: slot.string,
        kind: slot
            .source
            .map_or(Kind::Opaque, |source| kind(file, code, source)),
        repeats: match slot.source {
            Some(Source::At(pc)) => reaches_itself(&edges, pc as usize),
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

/// Whether the instruction at `pc` can run again after it ran (it lies on a
/// cycle of the control flow, exception edges included).
fn reaches_itself(edges: &BTreeMap<usize, Vec<usize>>, pc: usize) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    let mut work: Vec<usize> = edges.get(&pc).cloned().unwrap_or_default();
    while let Some(next) = work.pop() {
        if next == pc {
            return true;
        }
        if seen.insert(next) {
            work.extend(edges.get(&next).into_iter().flatten().copied());
        }
    }
    false
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
        Some(0xba) => match index(true).and_then(|i| file.invoke_dynamic_name(i).ok()) {
            // JLS 15.18.1: a non-constant concatenation is a new `String`
            // (JDK 25's `StringConcatFactory` copies even `"" + s`).
            Some(name) if name == "makeConcatWithConstants" || name == "makeConcat" => Kind::Fresh,
            _ => Kind::Opaque,
        },
        Some(0xb2) => match index(true).and_then(|i| file.field_ref(i).ok()) {
            Some(f)
                if f.owner == "java/lang/Boolean" && (f.name == "TRUE" || f.name == "FALSE") =>
            {
                Kind::Cached("java/lang/Boolean")
            }
            _ => Kind::Opaque,
        },
        Some(op @ (0xb6 | 0xb8)) => {
            let Some(m) = index(true).and_then(|i| file.method_ref(i).ok()) else {
                return Kind::Opaque;
            };
            let returns_own = return_desc(&m.descriptor)
                .strip_prefix('L')
                .and_then(|rest| rest.strip_suffix(';'))
                == Some(m.owner.as_str());
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
                (0xb6, "java/lang/String", "intern") => Kind::Interned,
                (0xb6, "java/lang/StringBuilder" | "java/lang/StringBuffer", "toString") => {
                    Kind::Fresh
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
/// words, so `dup2` and `pop2` need no types) and say where control goes.
fn step(file: &ClassFile, code: &[u8], pc: usize, state: &mut State) -> Option<Vec<usize>> {
    let op = *code.get(pc)?;
    let u1 = |at: usize| code.get(pc + at).copied();
    let u2 = |at: usize| Some(u16::from_be_bytes([u1(at)?, u1(at + 1)?]));
    let i4 = |at: usize| {
        Some(i32::from_be_bytes([
            u1(at)?,
            u1(at + 1)?,
            u1(at + 2)?,
            u1(at + 3)?,
        ]))
    };
    let produced = Slot {
        source: Some(Source::At(pc as u32)),
        string: false,
    };
    let string_class = |index: u16| {
        class_name(&file.pool, index)
            .ok()
            .is_some_and(|c| c == "java/lang/String")
    };
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
            let string = matches!(file.pool.get(index as usize), Some(Cp::String(_)));
            stack.push(Slot { string, ..produced });
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
        0xbb => stack.push(Slot {
            string: string_class(u2(1)?),
            ..produced
        }),
        // checkcast keeps the reference, and may learn it is a `String`
        0xc0 => stack.last_mut()?.string |= string_class(u2(1)?),
        0xc5 => {
            pop(stack, usize::from(u1(3)?))?;
            stack.push(produced);
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
    // Where control goes: weaver.rs decodes sizes and branches.
    let mut targets = Vec::new();
    if let Some((_, target)) = weaver::branch_target(op, code, pc).ok()? {
        targets.push(target);
    }
    if matches!(op, 0xaa | 0xab) {
        let base = 1 + weaver::switch_padding(pc);
        let jump = |offset: i32| pc.checked_add_signed(offset as isize);
        targets.push(jump(i4(base)?)?);
        let (count, first, stride) = if op == 0xaa {
            (
                i4(base + 8)?.checked_sub(i4(base + 4)?)?.checked_add(1)?,
                base + 12,
                4,
            )
        } else {
            (i4(base + 4)?, base + 12, 8)
        };
        for k in 0..usize::try_from(count).ok()? {
            targets.push(jump(i4(first + stride * k)?)?);
        }
    } else if !matches!(op, 0xa7 | 0xac..=0xb1 | 0xbf | 0xc8) {
        let size = match op {
            0xc4 => weaver::wide_instruction_size("", code, pc).ok()?,
            _ => weaver::instruction_size(op)?,
        };
        targets.push(pc + size);
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
            string: descriptor == "Ljava/lang/String;",
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
