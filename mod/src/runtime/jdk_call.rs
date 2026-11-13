//! The calls a JDK method body makes, run exactly as a call instruction in a
//! jar frame runs them.
//!
//! The JDK's collection skeletons (`jdk_skeletons/`) are template methods:
//! `AbstractCollection.toString` calls `this.iterator()`, then `hasNext()`
//! and `next()` on whatever that returned, then `String.valueOf` on each
//! element. Those receivers can be anything a jar call site can see: the
//! jar's own objects, the host's collections and iterators, strings, boxes,
//! lambdas, game objects. The interpreter's answer for each of them is the
//! dispatch chain of its `invoke*` instructions, so that is what a skeleton
//! body must reach too, or its callbacks would answer differently from the
//! same call written in a jar.
//!
//! [`Runtime::call_virtual`] and [`Runtime::call_static`] therefore run the
//! call through a **call-site frame**: a tiny synthetic class whose one static
//! method loads its arguments, executes the single `invokevirtual`,
//! `invokeinterface`, `invokestatic` or `instanceof` instruction and returns.
//! Its frame runs in `invoke_frame` like any jar frame, so the call sees the
//! same Mixin-free dispatch, honesty counting and host models as one in a jar.
//! Two cheaper paths come first and give the same answer: the receiver's own
//! jar body ([`Runtime::jvm_selected_jar_body`], the first step of the chain)
//! and a skeleton body ([`Runtime::invoke_jdk_skeleton`], the next one).
//!
//! The synthetic classes are built once per distinct call
//! (`horizon/jdk/CallSite$<n>`), shared by every runtime on every thread, and
//! never registered as a class anyone can name: only `invoke_frame` looks
//! them up ([`site`]). The frame is counted against the jar-frame bound, as
//! the JDK body's own frame would be on a JVM, and it pushes no `StackWalker`
//! frame (the skeleton body already pushed the JDK frame it stands for).

use super::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Name prefix of every call-site class.
const SITE_PREFIX: &str = "horizon/jdk/CallSite$";
/// The call-site method's name.
const SITE_METHOD: &str = "call";
/// Distinct call sites one process builds at most. The skeleton bodies make a
/// fixed set of calls (a few dozen), so this only bounds a bug.
const MAX_SITES: usize = 4096;

/// The instruction a call site executes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Site {
    /// `invokevirtual owner.name descriptor`.
    Virtual,
    /// `invokeinterface owner.name descriptor`.
    Interface,
    /// `invokestatic owner.name descriptor`.
    Static,
    /// `instanceof owner`.
    InstanceOf,
}

#[derive(Default)]
struct Sites {
    /// `kind|owner|name|descriptor` → the site's class name.
    by_call: HashMap<String, String>,
    /// Class name → the parsed site class and its method's descriptor.
    classes: HashMap<String, (Arc<ClassFile>, String)>,
}

fn sites() -> &'static Mutex<Sites> {
    static SITES: OnceLock<Mutex<Sites>> = OnceLock::new();
    SITES.get_or_init(|| Mutex::new(Sites::default()))
}

/// The call-site class `class`, for `invoke_frame`. `None` for every other
/// name.
pub(crate) fn site(class: &str) -> Option<Arc<ClassFile>> {
    if !class.starts_with(SITE_PREFIX) {
        return None;
    }
    let sites = sites().lock().ok()?;
    sites.classes.get(class).map(|(file, _)| Arc::clone(file))
}

/// The site class for one call, built on first use: its name and its
/// method's descriptor.
fn site_for(
    kind: Site,
    owner: &str,
    name: &str,
    descriptor: &str,
) -> Result<(String, String), NativeError> {
    let key = format!("{kind:?}|{owner}|{name}|{descriptor}");
    let mut sites = sites()
        .lock()
        .map_err(|_| NativeError::Metadata("call-site table is poisoned".into()))?;
    if let Some(class) = sites.by_call.get(&key) {
        let method = sites.classes[class].1.clone();
        return Ok((class.clone(), method));
    }
    if sites.by_call.len() >= MAX_SITES {
        return Err(NativeError::Metadata(format!(
            "more than {MAX_SITES} JDK call sites"
        )));
    }
    let class = format!("{SITE_PREFIX}{}", sites.by_call.len());
    let (bytes, method) = build_site(&class, kind, owner, name, descriptor)?;
    let file = ClassFile::parse(&bytes)?;
    sites.by_call.insert(key, class.clone());
    sites
        .classes
        .insert(class.clone(), (Arc::new(file), method.clone()));
    Ok((class, method))
}

/// A constant pool under construction.
#[derive(Default)]
struct Pool {
    bytes: Vec<u8>,
    count: u16,
}

impl Pool {
    fn push(&mut self, entry: &[u8]) -> u16 {
        self.bytes.extend_from_slice(entry);
        self.count += 1;
        self.count
    }

    fn utf8(&mut self, text: &str) -> u16 {
        let mut entry = vec![1];
        entry.extend_from_slice(&(text.len() as u16).to_be_bytes());
        entry.extend_from_slice(text.as_bytes());
        self.push(&entry)
    }

    fn class(&mut self, name: &str) -> u16 {
        let name = self.utf8(name);
        let mut entry = vec![7];
        entry.extend_from_slice(&name.to_be_bytes());
        self.push(&entry)
    }

    fn member(&mut self, tag: u8, owner: &str, name: &str, descriptor: &str) -> u16 {
        let class = self.class(owner);
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        let mut nat = vec![12];
        nat.extend_from_slice(&name.to_be_bytes());
        nat.extend_from_slice(&descriptor.to_be_bytes());
        let nat = self.push(&nat);
        let mut entry = vec![tag];
        entry.extend_from_slice(&class.to_be_bytes());
        entry.extend_from_slice(&nat.to_be_bytes());
        self.push(&entry)
    }
}

/// The load opcode and slot width of one descriptor parameter.
fn load_of(parameter: &str) -> (u8, u8) {
    match parameter {
        "J" => (0x16, 2),
        "D" => (0x18, 2),
        "F" => (0x17, 1),
        "I" | "Z" | "B" | "C" | "S" => (0x15, 1),
        _ => (0x19, 1),
    }
}

/// The return opcode for a return descriptor.
fn return_of(descriptor: &str) -> u8 {
    match descriptor {
        "V" => 0xb1,
        "J" => 0xad,
        "D" => 0xaf,
        "F" => 0xae,
        "I" | "Z" | "B" | "C" | "S" => 0xac,
        _ => 0xb0,
    }
}

/// The class file of one call site: `public static <ret> call(<receiver>,
/// <params>)` whose body is the one instruction. Version 52 (Java 8), so no
/// `StackMapTable` is needed for a body without branches.
fn build_site(
    class: &str,
    kind: Site,
    owner: &str,
    name: &str,
    descriptor: &str,
) -> Result<(Vec<u8>, String), NativeError> {
    if !(class.is_ascii() && owner.is_ascii() && name.is_ascii() && descriptor.is_ascii()) {
        return Err(NativeError::Metadata(format!(
            "JDK call site {owner}.{name}{descriptor} is not ASCII"
        )));
    }
    let mut pool = Pool::default();
    let this_class = pool.class(class);
    let super_class = pool.class("java/lang/Object");
    let (parameters, returns) = match kind {
        Site::InstanceOf => (vec!["Ljava/lang/Object;".to_owned()], "Z".to_owned()),
        Site::Static => (
            descriptor_arguments(descriptor),
            return_desc(descriptor).into(),
        ),
        Site::Virtual | Site::Interface => {
            let mut parameters = vec!["Ljava/lang/Object;".to_owned()];
            parameters.extend(descriptor_arguments(descriptor));
            (parameters, return_desc(descriptor).into())
        }
    };
    let method_descriptor = format!("({}){returns}", parameters.concat());
    let mut code = Vec::new();
    let mut slot: u16 = 0;
    for parameter in &parameters {
        let (load, width) = load_of(parameter);
        code.push(load);
        code.push(slot as u8);
        slot += u16::from(width);
    }
    if slot > 255 {
        return Err(NativeError::Metadata(format!(
            "JDK call site {owner}.{name}{descriptor} has too many arguments"
        )));
    }
    match kind {
        Site::Virtual => {
            let index = pool.member(10, owner, name, descriptor);
            code.push(0xb6);
            code.extend_from_slice(&index.to_be_bytes());
        }
        Site::Static => {
            let index = pool.member(10, owner, name, descriptor);
            code.push(0xb8);
            code.extend_from_slice(&index.to_be_bytes());
        }
        Site::Interface => {
            let index = pool.member(11, owner, name, descriptor);
            code.push(0xb9);
            code.extend_from_slice(&index.to_be_bytes());
            code.push(slot as u8);
            code.push(0);
        }
        Site::InstanceOf => {
            let index = pool.class(owner);
            code.push(0xc1);
            code.extend_from_slice(&index.to_be_bytes());
        }
    }
    code.push(if kind == Site::InstanceOf {
        0xac
    } else {
        return_of(&returns)
    });
    let method_name = pool.utf8(SITE_METHOD);
    let method_descriptor_index = pool.utf8(&method_descriptor);
    let code_name = pool.utf8("Code");

    let mut out = Vec::new();
    out.extend_from_slice(&0xCAFE_BABE_u32.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&52u16.to_be_bytes());
    out.extend_from_slice(&(pool.count + 1).to_be_bytes());
    out.extend_from_slice(&pool.bytes);
    // ACC_PUBLIC | ACC_FINAL | ACC_SUPER | ACC_SYNTHETIC.
    out.extend_from_slice(&0x1031u16.to_be_bytes());
    out.extend_from_slice(&this_class.to_be_bytes());
    out.extend_from_slice(&super_class.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // interfaces
    out.extend_from_slice(&0u16.to_be_bytes()); // fields
    out.extend_from_slice(&1u16.to_be_bytes()); // methods
    // ACC_PUBLIC | ACC_STATIC | ACC_SYNTHETIC.
    out.extend_from_slice(&0x1009u16.to_be_bytes());
    out.extend_from_slice(&method_name.to_be_bytes());
    out.extend_from_slice(&method_descriptor_index.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // attributes
    out.extend_from_slice(&code_name.to_be_bytes());
    let attribute_length = 2 + 2 + 4 + code.len() + 2 + 2;
    out.extend_from_slice(&(attribute_length as u32).to_be_bytes());
    out.extend_from_slice(&(slot + 2).to_be_bytes()); // max_stack
    out.extend_from_slice(&slot.max(1).to_be_bytes()); // max_locals
    out.extend_from_slice(&(code.len() as u32).to_be_bytes());
    out.extend_from_slice(&code);
    out.extend_from_slice(&0u16.to_be_bytes()); // exception table
    out.extend_from_slice(&0u16.to_be_bytes()); // code attributes
    out.extend_from_slice(&0u16.to_be_bytes()); // class attributes
    Ok((out, method_descriptor))
}

/// The error a JDK body's call returns when the host counted the call and
/// answered a neutral `null` the body would dereference. The skeleton frame
/// turns it into the body's own neutral answer, so the run stays counted
/// (`ran-hollow`) without an NPE Java would never throw.
pub(crate) const ABANDONED: &str = "JDK body stopped at a counted host answer";

/// A Java exception of `class` with `message`, as a JDK body throws it.
pub(crate) fn throw(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.to_owned(),
        message,
        frame: None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `receiver.name(args)` as the instruction `kind` (`Virtual` or
    /// `Interface`) naming `owner.name descriptor`, dispatched exactly as a jar
    /// call site would dispatch it.
    pub(crate) fn call_virtual(
        &mut self,
        kind: Site,
        owner: &str,
        name: &str,
        descriptor: &str,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let r = MethodRef {
            owner: owner.to_owned(),
            name: name.to_owned(),
            descriptor: descriptor.to_owned(),
        };
        if *receiver == Value::Ref(None) {
            return Err(null_checks::invoke_on_null(&r));
        }
        let opcode = if kind == Site::Interface { 0xb9 } else { 0xb6 };
        if let Some(body) = self.jvm_selected_jar_body(&r, opcode, Some(receiver))? {
            return self.invoke(&body, name, descriptor, Some(receiver.clone()), args);
        }
        if let Some(value) = self.invoke_jdk_skeleton(&r, opcode, Some(receiver), args)? {
            return Ok(value);
        }
        let mut values = Vec::with_capacity(args.len() + 1);
        values.push(receiver.clone());
        values.extend_from_slice(args);
        let counted = self.host.counted_unknown_calls();
        let value = self.run_site(kind, &r, &values)?;
        // A counted, neutral `null` where the JDK body goes on to use the
        // answer (an iterator, a view): the body stops rather than throw an
        // NPE Java never would into the jar.
        if value == Value::Ref(None)
            && self.host.counted_unknown_calls() > counted
            && return_desc(descriptor).starts_with("Ljava/util/")
        {
            return Err(NativeError::Metadata(ABANDONED.into()));
        }
        Ok(value)
    }

    /// `owner.name(args)` as `invokestatic`, through a call-site frame.
    pub(crate) fn call_static(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let r = MethodRef {
            owner: owner.to_owned(),
            name: name.to_owned(),
            descriptor: descriptor.to_owned(),
        };
        self.run_site(Site::Static, &r, args)
    }

    /// `value instanceof class`, answered by the `instanceof` instruction
    /// (counted when no model can stand behind a `false`, as in a jar).
    pub(crate) fn java_instance_of(
        &mut self,
        value: &Value,
        class: &str,
    ) -> Result<bool, NativeError> {
        if *value == Value::Ref(None) {
            return Ok(false);
        }
        let r = MethodRef {
            owner: class.to_owned(),
            name: String::new(),
            descriptor: String::new(),
        };
        let answer = self.run_site(Site::InstanceOf, &r, std::slice::from_ref(value))?;
        Ok(matches!(answer, Value::Int(flag) if flag != 0))
    }

    /// Run one call-site frame.
    fn run_site(
        &mut self,
        kind: Site,
        r: &MethodRef,
        values: &[Value],
    ) -> Result<Value, NativeError> {
        let (class, descriptor) = site_for(kind, &r.owner, &r.name, &r.descriptor)?;
        if !frame_bound::admits_frame(self.depth) {
            return Err(NativeError::CallDepthExceeded {
                class: r.owner.clone(),
                method: r.name.clone(),
                limit: self.depth,
            });
        }
        if self.budget.expired() {
            return Err(self.budget_exceeded(&r.owner, &r.name));
        }
        self.depth += 1;
        let result = self.invoke_frame(&class, SITE_METHOD, &descriptor, None, values);
        self.depth -= 1;
        // A throw that started in the call itself names no jar frame: the
        // site is not one, and the JDK body has no frame label here.
        result.map_err(|error| match error {
            NativeError::UncaughtException {
                class,
                message,
                frame: Some(frame),
            } if frame.starts_with(SITE_PREFIX) => NativeError::UncaughtException {
                class,
                message,
                frame: None,
            },
            other => other,
        })
    }
}
