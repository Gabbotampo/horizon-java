//! The matcher: Java's node graph, matched the way `java.util.regex` does it.
//!
//! The parsed tree is compiled into nodes that mirror `Pattern`'s own
//! (`GroupHead`/`GroupTail`, `Branch`, `Curly`, `Prolog`/`Loop`, `Pos`/`Neg`,
//! `Behind`/`NotBehind`, `BackRef`, the anchors). Each node matches at a
//! position and calls its continuation, so the search explores alternatives
//! in exactly Java's order and leaves the groups exactly where Java's would
//! be. Behaviour was read from OpenJDK 27's bytecode and is pinned by the
//! oracle tests in `tests.rs`.
//!
//! Where an answer cannot be decided (a Unicode property on a non-ASCII
//! character, `\b` next to one, a search that outgrows its budget), matching
//! stops with [`Unsupported`] and the caller counts the call.

use super::Unsupported;
use super::syntax::{Assert, Ast, Category, Class, Greed, Parsed, Posix};

/// Node visits one operation (a whole `replaceAll`, say) may spend.
const MAX_STEPS: usize = 2_000_000;
/// How deep a search may recurse, in nodes, and how much stack it may use.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub(crate) depth: usize,
    pub(crate) stack: usize,
}

impl Limits {
    /// Where an answer stops being Java's.
    ///
    /// Java's matcher recurses one frame (or a few) per node, as this one
    /// does, and throws `StackOverflowError` when the thread's stack runs
    /// out: from about 450 characters of `(?:(a)|b)*` under `-Xint` on a
    /// 512 KB thread, about 1,000–1,500 on a cold 1 MB thread, and several
    /// thousand once C2 has compiled the nodes. Past the shallowest of those
    /// there is no single Java answer, so a search that goes deeper is
    /// counted. `DEPTH` is set well below the node depth this engine reaches
    /// on the shortest input JDK 27 overflows on in every measured setting
    /// (see `deep_searches_are_counted_where_java_can_overflow`).
    pub(crate) const JAVA: Limits = Limits {
        depth: MAX_DEPTH,
        stack: MAX_STACK,
    };
}

/// See [`Limits::JAVA`]. JDK 27 overflowed from 2,678 of this engine's
/// nodes at the shallowest (`(?:a|b)*?c`, `-Xint`, 512 KB thread); this is
/// 2.7 times below that, so a shape whose Java frames are heavier than the
/// measured ones still stays clear of Java's own limit.
pub(crate) const MAX_DEPTH: usize = 1_000;
/// A safety net under [`MAX_DEPTH`]: stack bytes a search may use before it
/// is counted, whatever its depth. An unoptimized build used at most
/// ~1.13 KB per node on the measured shapes (an optimized one ~250 B), so
/// `MAX_DEPTH` nodes fit inside it and a 2 MB thread keeps ~750 KB for its
/// callers; the net only binds for a shape with larger frames than those.
const MAX_STACK: usize = 1_280 << 10;

type R<T> = Result<T, Unsupported>;

/// Where a matched path ends.
#[derive(Debug, Clone, Copy, PartialEq)]
enum End {
    /// The pattern's end: `Matcher.matches` also requires the input's end.
    Main,
    /// The end of an atomic group or a lookahead body: record the position.
    Sub,
    /// The end of a lookbehind body: it must end where the lookbehind is.
    Behind,
}

#[derive(Debug, Clone)]
enum Node {
    End(End),
    One {
        class: Class,
        next: usize,
    },
    /// A single-character repetition (Java's `Curly` over a `CharProperty`).
    Curly {
        class: Class,
        min: u32,
        max: Option<u32>,
        greed: Greed,
        next: usize,
    },
    GroupHead {
        local: usize,
        next: usize,
    },
    GroupTail {
        local: usize,
        group: Option<usize>,
        next: usize,
    },
    /// Alternatives in order; `None` is an empty alternative.
    Branch {
        alternatives: Vec<Option<usize>>,
        next: usize,
    },
    /// Java's `Prolog`: the first iteration of a group repetition.
    Prolog {
        looping: usize,
    },
    /// Java's `Loop` (greedy) and `LazyLoop`; `begin` is the body's
    /// `GroupHead` local, the start of the current iteration.
    Loop {
        body: usize,
        count: usize,
        begin: usize,
        min: u32,
        max: Option<u32>,
        lazy: bool,
        next: usize,
    },
    /// Java's possessive `Curly` over a group: each iteration is atomic.
    Possessive {
        body: usize,
        min: u32,
        max: Option<u32>,
        next: usize,
    },
    Look {
        body: usize,
        negate: bool,
        /// Lookbehind: the body's minimum and maximum lengths.
        behind: Option<(usize, usize)>,
        next: usize,
    },
    Atomic {
        body: usize,
        next: usize,
    },
    Backref {
        group: usize,
        next: usize,
    },
    Assert {
        kind: Assert,
        next: usize,
    },
}

/// A compiled pattern.
#[derive(Debug, Clone)]
pub(crate) struct Program {
    nodes: Vec<Node>,
    start: usize,
    locals: usize,
    /// Capturing groups, not counting group 0.
    pub(crate) groups: usize,
    pub(crate) names: Vec<(String, usize)>,
    /// `tainted[n]`: group `n`'s value can differ from Java's in an edge this
    /// module does not model (it is set inside a lookaround, an atomic or
    /// possessive group, an iteration that may match nothing, or a group
    /// nested in a repeated group). Reading it is counted; the match itself
    /// is exact.
    pub(crate) tainted: Vec<bool>,
}

struct Compiler<'t> {
    nodes: Vec<Node>,
    locals: usize,
    tainted: &'t [bool],
}

/// Whether `ast` can only ever match nothing, whatever it sees: anchors,
/// lookarounds and empty groups. Java repeats such a group with rules this
/// module does not model.
fn zero_width_only(ast: &Ast) -> bool {
    match ast {
        Ast::Empty | Ast::Assert(_) | Ast::Look { .. } => true,
        Ast::One(_) | Ast::Backref(_) => false,
        Ast::Concat(items) | Ast::Alt(items) => items.iter().all(zero_width_only),
        Ast::Group(_, node) | Ast::Atomic(node) => zero_width_only(node),
        Ast::Repeat { node, max, .. } => *max == Some(0) || zero_width_only(node),
    }
}

/// Whether `ast` can match without consuming a character.
fn nullable(ast: &Ast) -> bool {
    match ast {
        Ast::Empty | Ast::Assert(_) | Ast::Look { .. } | Ast::Backref(_) => true,
        Ast::One(_) => false,
        Ast::Concat(items) => items.iter().all(nullable),
        Ast::Alt(items) => items.iter().any(nullable),
        Ast::Group(_, node) | Ast::Atomic(node) => nullable(node),
        Ast::Repeat { node, min, .. } => *min == 0 || nullable(node),
    }
}

/// Mark every group inside `ast` as tainted.
fn taint_all(ast: &Ast, tainted: &mut [bool]) {
    match ast {
        Ast::Group(group, node) => {
            if let Some(group) = group {
                tainted[*group] = true;
            }
            taint_all(node, tainted);
        }
        Ast::Concat(items) | Ast::Alt(items) => {
            items.iter().for_each(|item| taint_all(item, tainted));
        }
        Ast::Repeat { node, .. } | Ast::Look { node, .. } | Ast::Atomic(node) => {
            taint_all(node, tainted)
        }
        Ast::Empty | Ast::One(_) | Ast::Backref(_) | Ast::Assert(_) => {}
    }
}

/// Find the groups whose values this module does not promise.
fn taint(ast: &Ast, tainted: &mut [bool]) {
    match ast {
        Ast::Look { node, .. } | Ast::Atomic(node) => taint_all(node, tainted),
        Ast::Repeat { node, greed, .. } => {
            if *greed == Greed::Possessive || nullable(node) {
                taint_all(node, tainted);
            } else if let Ast::Group(_, body) = &**node {
                // Java repeats a fixed-length group with `GroupCurly`, which
                // restores the repeated group itself when it gives back an
                // iteration but leaves the groups nested in it as that
                // iteration set them. The repeated group is exact; the
                // nested ones are not promised.
                taint_all(body, tainted);
            }
        }
        Ast::Group(_, node) => taint(node, tainted),
        Ast::Concat(items) | Ast::Alt(items) => {
            items.iter().for_each(|item| taint(item, tainted));
        }
        Ast::Empty | Ast::One(_) | Ast::Backref(_) | Ast::Assert(_) => {}
    }
}

/// A lookbehind body's length bounds, or `None` where Java either rejects the
/// body ("does not have an obvious maximum length") or would compute them
/// with rules this module does not reproduce.
fn behind_bounds(ast: &Ast) -> Option<(usize, usize)> {
    match ast {
        Ast::Empty | Ast::Assert(_) => Some((0, 0)),
        Ast::One(_) => Some((1, 1)),
        Ast::Concat(items) => items.iter().try_fold((0usize, 0usize), |(lo, hi), item| {
            let (a, b) = behind_bounds(item)?;
            Some((lo + a, hi.checked_add(b)?))
        }),
        Ast::Alt(items) => {
            let mut bounds: Option<(usize, usize)> = None;
            for item in items {
                let (a, b) = behind_bounds(item)?;
                bounds = Some(match bounds {
                    Some((lo, hi)) => (lo.min(a), hi.max(b)),
                    None => (a, b),
                });
            }
            bounds
        }
        Ast::Group(_, node) => behind_bounds(node),
        // A single character repeated a bounded number of times (Java's
        // `Curly`); repeated groups compile to `Loop`, which Java rejects.
        Ast::Repeat {
            node,
            min,
            max: Some(max),
            greed: Greed::Greedy | Greed::Lazy,
        } if matches!(**node, Ast::One(_)) && *max <= 1_000 => Some((*min as usize, *max as usize)),
        Ast::Repeat { .. } | Ast::Look { .. } | Ast::Atomic(_) | Ast::Backref(_) => None,
    }
}

impl Compiler<'_> {
    fn push(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn local(&mut self) -> usize {
        self.locals += 1;
        self.locals - 1
    }

    /// Compile `ast` so that a match continues at `next`; answers the entry.
    fn compile(&mut self, ast: &Ast, next: usize) -> R<usize> {
        Ok(match ast {
            Ast::Empty => next,
            Ast::One(class) => self.push(Node::One {
                class: class.clone(),
                next,
            }),
            Ast::Concat(items) => {
                let mut entry = next;
                for item in items.iter().rev() {
                    entry = self.compile(item, entry)?;
                }
                entry
            }
            Ast::Alt(items) => {
                let mut alternatives = Vec::with_capacity(items.len());
                for item in items {
                    alternatives.push(if *item == Ast::Empty {
                        None
                    } else {
                        Some(self.compile(item, next)?)
                    });
                }
                self.push(Node::Branch { alternatives, next })
            }
            Ast::Group(group, body) => {
                let (head, _) = self.group(*group, body, next)?;
                head
            }
            Ast::Repeat {
                node,
                min,
                max,
                greed,
            } => self.repeat(node, *min, *max, *greed, next)?,
            Ast::Look {
                behind,
                negate,
                node,
            } => {
                let (body, bounds) = if *behind {
                    let Some(bounds) = behind_bounds(node) else {
                        return Err(Unsupported(
                            "a lookbehind without a modelled maximum length".into(),
                        ));
                    };
                    let end = self.push(Node::End(End::Behind));
                    (self.compile(node, end)?, Some(bounds))
                } else {
                    let end = self.push(Node::End(End::Sub));
                    (self.compile(node, end)?, None)
                };
                self.push(Node::Look {
                    body,
                    negate: *negate,
                    behind: bounds,
                    next,
                })
            }
            Ast::Atomic(node) => {
                let end = self.push(Node::End(End::Sub));
                let body = self.compile(node, end)?;
                self.push(Node::Atomic { body, next })
            }
            Ast::Backref(group) => {
                // A back reference reads the group while matching, so a value
                // this module does not promise would change the match itself.
                if self.tainted.get(*group).copied().unwrap_or(true) {
                    return Err(Unsupported(format!(
                        "a back reference to group {group}, whose value Java keeps by rules this module does not follow"
                    )));
                }
                self.push(Node::Backref {
                    group: *group,
                    next,
                })
            }
            Ast::Assert(kind) => self.push(Node::Assert { kind: *kind, next }),
        })
    }

    /// `GroupHead body GroupTail`, continuing at `next`; answers the head and
    /// its local.
    fn group(&mut self, group: Option<usize>, body: &Ast, next: usize) -> R<(usize, usize)> {
        let local = self.local();
        let tail = self.push(Node::GroupTail { local, group, next });
        let body = self.compile(body, tail)?;
        Ok((self.push(Node::GroupHead { local, next: body }), local))
    }

    fn repeat(
        &mut self,
        node: &Ast,
        min: u32,
        max: Option<u32>,
        greed: Greed,
        next: usize,
    ) -> R<usize> {
        if let Ast::One(class) = node {
            return Ok(self.push(Node::Curly {
                class: class.clone(),
                min,
                max,
                greed,
                next,
            }));
        }
        let Ast::Group(group, body) = node else {
            return Err(Unsupported("a quantifier on a compound atom".into()));
        };
        if zero_width_only(body) {
            // A body that only ever matches nothing (`()*`, `(\b)+`,
            // `((?=a))*`) takes a `GroupCurly` path in Java that this module
            // does not model.
            return Err(Unsupported("a quantified zero-width group".into()));
        }
        if greed == Greed::Possessive {
            let end = self.push(Node::End(End::Sub));
            let (body, _) = self.group(*group, body, end)?;
            return Ok(self.push(Node::Possessive {
                body,
                min,
                max,
                next,
            }));
        }
        if max == Some(1) && min <= 1 {
            // `X?` on a group is a `Branch` (greedy) or its mirror (lazy);
            // `X{1}` is the group itself.
            let (head, _) = self.group(*group, body, next)?;
            if min == 1 {
                return Ok(head);
            }
            let alternatives = match greed {
                Greed::Lazy => vec![None, Some(head)],
                _ => vec![Some(head), None],
            };
            return Ok(self.push(Node::Branch { alternatives, next }));
        }
        // `Prolog -> Loop`, with the body's tail continuing into the loop.
        let count = self.local();
        let looping = self.push(Node::End(End::Main)); // placeholder, replaced below
        let (body, begin) = self.group(*group, body, looping)?;
        self.nodes[looping] = Node::Loop {
            body,
            count,
            begin,
            min,
            max,
            lazy: greed == Greed::Lazy,
            next,
        };
        Ok(self.push(Node::Prolog { looping }))
    }
}

/// Compile a parsed pattern.
pub(crate) fn compile(parsed: Parsed) -> R<Program> {
    let mut tainted = vec![false; parsed.groups + 1];
    taint(&parsed.ast, &mut tainted);
    let mut compiler = Compiler {
        nodes: Vec::new(),
        locals: 0,
        tainted: &tainted,
    };
    let end = compiler.push(Node::End(End::Main));
    let start = compiler.compile(&parsed.ast, end)?;
    Ok(Program {
        nodes: compiler.nodes,
        start,
        locals: compiler.locals,
        groups: parsed.groups,
        names: parsed.names,
        tainted,
    })
}

// ---- character tests ------------------------------------------------------

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// ASCII-only case folding, Java's default (no `UNICODE_CASE`).
fn ascii_lower(c: char) -> char {
    c.to_ascii_lowercase()
}

fn posix(kind: Posix, c: char) -> bool {
    let punct = c.is_ascii_punctuation();
    match kind {
        Posix::Lower => c.is_ascii_lowercase(),
        Posix::Upper => c.is_ascii_uppercase(),
        Posix::Ascii => c.is_ascii(),
        Posix::Alpha => c.is_ascii_alphabetic(),
        Posix::Digit => c.is_ascii_digit(),
        Posix::Alnum => c.is_ascii_alphanumeric(),
        Posix::Punct => punct,
        Posix::Graph => c.is_ascii_alphanumeric() || punct,
        Posix::Print => c.is_ascii_alphanumeric() || punct || c == ' ',
        Posix::Blank => c == ' ' || c == '\t',
        Posix::Cntrl => (c as u32) < 0x20 || c == '\u{7F}',
        Posix::XDigit => c.is_ascii_hexdigit(),
        Posix::Space => matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r'),
    }
}

/// Whether `class` accepts `c`; an error where the answer needs Unicode
/// tables the runtime does not carry.
fn test(class: &Class, c: char) -> R<bool> {
    Ok(match class {
        Class::Char(expected, ci) => {
            *expected == c || (*ci && ascii_lower(*expected) == ascii_lower(c))
        }
        Class::Range(lo, hi, ci) => {
            (*lo..=*hi).contains(&c)
                || (*ci
                    && c.is_ascii()
                    && ((*lo..=*hi).contains(&c.to_ascii_uppercase())
                        || (*lo..=*hi).contains(&c.to_ascii_lowercase())))
        }
        Class::Dot { dotall } => *dotall || !is_line_terminator(c),
        Class::Digit => c.is_ascii_digit(),
        Class::Word => c.is_ascii_alphanumeric() || c == '_',
        Class::Space => matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r'),
        Class::HSpace => matches!(
            c,
            ' ' | '\t' | '\u{A0}' | '\u{1680}' | '\u{180E}' | '\u{2000}'
                ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
        ),
        Class::VSpace => matches!(
            c,
            '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ),
        Class::Posix(kind) => posix(*kind, c),
        Class::Category(category) => {
            if !c.is_ascii() {
                return Err(Unsupported(
                    "a Unicode property on a non-ASCII character".into(),
                ));
            }
            match category {
                Category::Letter => c.is_ascii_alphabetic(),
                Category::Upper => c.is_ascii_uppercase(),
                Category::Lower => c.is_ascii_lowercase(),
                Category::Number | Category::DecimalDigit => c.is_ascii_digit(),
            }
        }
        Class::Not(inner) => !test(inner, c)?,
        Class::Union(items) => {
            for item in items {
                if test(item, c)? {
                    return Ok(true);
                }
            }
            false
        }
        Class::And(left, right) => test(left, c)? && test(right, c)?,
    })
}

// ---- matching -------------------------------------------------------------

/// One search's mutable state: Java's `Matcher` fields that nodes touch.
pub(crate) struct Matcher<'a> {
    program: &'a Program,
    text: &'a [char],
    /// `groups[2n]`, `groups[2n + 1]`: group `n`'s bounds, `usize::MAX` unset.
    pub(crate) groups: Vec<usize>,
    locals: Vec<usize>,
    /// Where the last `End::Sub` was reached.
    last: usize,
    /// Where an enclosing lookbehind's body must end.
    behind_to: usize,
    /// Whether `End::Main` requires the end of the input (`matches()`).
    anchor_end: bool,
    steps: usize,
    limits: Limits,
    /// Current and deepest recursion, in nodes.
    depth: usize,
    pub(crate) peak_depth: usize,
    /// The stack address the search started at, and the lowest one reached.
    stack_base: usize,
    pub(crate) stack_low: usize,
}

/// The address of a fresh local: where the stack is now.
#[inline(always)]
fn stack_position() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

pub(crate) const UNSET: usize = usize::MAX;

impl<'a> Matcher<'a> {
    /// A matcher bounded by `limits` (stacks grow down on every supported
    /// target).
    pub(crate) fn new(program: &'a Program, text: &'a [char], limits: Limits) -> Self {
        let stack_base = stack_position();
        Self {
            program,
            text,
            groups: vec![UNSET; (program.groups + 1) * 2],
            locals: vec![UNSET; program.locals],
            last: 0,
            behind_to: 0,
            anchor_end: false,
            steps: 0,
            limits,
            depth: 0,
            peak_depth: 0,
            stack_base,
            stack_low: stack_base,
        }
    }

    /// Stack bytes the deepest recursion so far used.
    #[cfg(test)]
    pub(crate) fn stack_base_minus_low(&self) -> usize {
        self.stack_base - self.stack_low
    }

    /// Java's `Matcher.search(from)`: the leftmost match at or after `from`.
    /// Sets `groups` and answers whether one was found.
    pub(crate) fn search(&mut self, from: usize) -> R<bool> {
        self.reset_groups();
        self.anchor_end = false;
        for start in from..=self.text.len() {
            if self.at(self.program.start, start)? {
                self.groups[0] = start;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Java's `Matcher.match(0, anchor)`: `matches()` (`anchor_end`) or
    /// `lookingAt()`.
    pub(crate) fn match_at_start(&mut self, anchor_end: bool) -> R<bool> {
        self.reset_groups();
        self.anchor_end = anchor_end;
        if self.at(self.program.start, 0)? {
            self.groups[0] = 0;
            return Ok(true);
        }
        Ok(false)
    }

    fn reset_groups(&mut self) {
        self.groups.iter_mut().for_each(|value| *value = UNSET);
    }

    /// Spends a step and checks the limits; `false` stops the search, and
    /// [`Matcher::refusal`] says why.
    #[inline(never)]
    fn enter(&mut self) -> bool {
        self.steps += 1;
        let here = stack_position();
        self.stack_low = self.stack_low.min(here);
        self.steps <= MAX_STEPS
            && self.depth < self.limits.depth
            && self.stack_base.saturating_sub(here) <= self.limits.stack
    }

    #[cold]
    #[inline(never)]
    fn refusal(&self) -> Unsupported {
        Unsupported(if self.steps > MAX_STEPS {
            "a search over its step budget".into()
        } else if self.depth >= self.limits.depth {
            "a search nested deeper than Java's stack is sure to allow".into()
        } else {
            "a search past its stack budget".into()
        })
    }

    fn spend(&mut self) -> R<()> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(Unsupported("a search over its step budget".into()));
        }
        Ok(())
    }

    /// Match node `id` at `i`. A search deeper than its limits is counted
    /// (see [`Limits::JAVA`]); the depth is restored however the call returns.
    ///
    /// Each node kind lives in its own method so that, in an unoptimized
    /// build, a level of recursion carries only the frame of the node it is
    /// in: one `match` holding every kind's locals made each level ~2.8 KB,
    /// too much for [`MAX_DEPTH`] levels on a small thread.
    fn at(&mut self, id: usize, i: usize) -> R<bool> {
        if !self.enter() {
            return Err(self.refusal());
        }
        self.depth += 1;
        self.peak_depth = self.peak_depth.max(self.depth);
        let result = match &self.program.nodes[id] {
            Node::End(end) => Ok(self.end(*end, i)),
            Node::One { class, next } => self.one(class, *next, i),
            Node::Curly {
                class,
                min,
                max,
                greed,
                next,
            } => self.curly(class, *min, *max, *greed, *next, i),
            Node::GroupHead { local, next } => self.group_head(*local, *next, i),
            Node::GroupTail { local, group, next } => self.group_tail(*local, *group, *next, i),
            Node::Branch { alternatives, next } => self.branch(alternatives, *next, i),
            Node::Prolog { looping } => self.loop_init(*looping, i),
            Node::Loop { .. } => self.loop_match(id, i),
            Node::Possessive {
                body,
                min,
                max,
                next,
            } => self.possessive(*body, *min, *max, *next, i),
            Node::Look {
                body,
                negate,
                behind,
                next,
            } => self.look(*body, *negate, *behind, *next, i),
            Node::Atomic { body, next } => self.atomic(*body, *next, i),
            Node::Backref { group, next } => self.backref(*group, *next, i),
            Node::Assert { kind, next } => self.assert_then(*kind, *next, i),
        };
        self.depth -= 1;
        result
    }

    #[inline(never)]
    fn end(&mut self, end: End, i: usize) -> bool {
        match end {
            End::Main => {
                if self.anchor_end && i != self.text.len() {
                    return false;
                }
                self.last = i;
                self.groups[1] = i;
                true
            }
            End::Sub => {
                self.last = i;
                true
            }
            End::Behind => i == self.behind_to,
        }
    }

    #[inline(never)]
    fn one(&mut self, class: &Class, next: usize, i: usize) -> R<bool> {
        match self.text.get(i) {
            Some(c) if test(class, *c)? => self.at(next, i + 1),
            _ => Ok(false),
        }
    }

    #[inline(never)]
    fn group_head(&mut self, local: usize, next: usize, i: usize) -> R<bool> {
        let saved = self.locals[local];
        self.locals[local] = i;
        let result = self.at(next, i);
        self.locals[local] = saved;
        result
    }

    #[inline(never)]
    fn group_tail(&mut self, local: usize, group: Option<usize>, next: usize, i: usize) -> R<bool> {
        let start = self.locals[local];
        let Some(group) = group else {
            return self.at(next, i);
        };
        let (saved_start, saved_end) = (self.groups[2 * group], self.groups[2 * group + 1]);
        self.groups[2 * group] = start;
        self.groups[2 * group + 1] = i;
        if self.at(next, i)? {
            return Ok(true);
        }
        self.groups[2 * group] = saved_start;
        self.groups[2 * group + 1] = saved_end;
        Ok(false)
    }

    #[inline(never)]
    fn branch(&mut self, alternatives: &[Option<usize>], next: usize, i: usize) -> R<bool> {
        for alternative in alternatives {
            if self.at(alternative.unwrap_or(next), i)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[inline(never)]
    fn possessive(
        &mut self,
        body: usize,
        min: u32,
        max: Option<u32>,
        next: usize,
        i: usize,
    ) -> R<bool> {
        let mut i = i;
        let mut count = 0;
        while count < min {
            if !self.at(body, i)? {
                return Ok(false);
            }
            i = self.last;
            count += 1;
        }
        while max.is_none_or(|max| count < max) {
            if !self.at(body, i)? || self.last == i {
                break;
            }
            i = self.last;
            count += 1;
        }
        self.at(next, i)
    }

    #[inline(never)]
    fn look(
        &mut self,
        body: usize,
        negate: bool,
        behind: Option<(usize, usize)>,
        next: usize,
        i: usize,
    ) -> R<bool> {
        let matched = match behind {
            None => self.at(body, i)?,
            Some((min, max)) => {
                let saved = self.behind_to;
                self.behind_to = i;
                let lowest = i.saturating_sub(max);
                let mut matched = false;
                if let Some(mut j) = i.checked_sub(min) {
                    loop {
                        if self.at(body, j)? {
                            matched = true;
                            break;
                        }
                        if j == lowest {
                            break;
                        }
                        j -= 1;
                    }
                }
                self.behind_to = saved;
                matched
            }
        };
        if matched == negate {
            return Ok(false);
        }
        self.at(next, i)
    }

    #[inline(never)]
    fn atomic(&mut self, body: usize, next: usize, i: usize) -> R<bool> {
        if !self.at(body, i)? {
            return Ok(false);
        }
        let end = self.last;
        self.at(next, end)
    }

    #[inline(never)]
    fn backref(&mut self, group: usize, next: usize, i: usize) -> R<bool> {
        let text = self.text;
        let (start, end) = (self.groups[2 * group], self.groups[2 * group + 1]);
        if start == UNSET || end == UNSET {
            return Ok(false);
        }
        let length = end - start;
        if i + length > text.len() || text[start..end] != text[i..i + length] {
            return Ok(false);
        }
        self.at(next, i + length)
    }

    #[inline(never)]
    fn assert_then(&mut self, kind: Assert, next: usize, i: usize) -> R<bool> {
        if self.assert(kind, i)? {
            self.at(next, i)
        } else {
            Ok(false)
        }
    }

    fn assert(&self, kind: Assert, i: usize) -> R<bool> {
        let text = self.text;
        let end = text.len();
        Ok(match kind {
            Assert::Begin => i == 0,
            Assert::End => i == end,
            Assert::Caret => {
                if i == end {
                    false
                } else if i == 0 {
                    true
                } else {
                    let before = text[i - 1];
                    is_line_terminator(before) && !(before == '\r' && text[i] == '\n')
                }
            }
            Assert::Dollar { multiline } => {
                if !multiline {
                    if i + 2 < end {
                        return Ok(false);
                    }
                    if i + 2 == end && !(text[i] == '\r' && text[i + 1] == '\n') {
                        return Ok(false);
                    }
                }
                if i < end {
                    let c = text[i];
                    if c == '\n' {
                        // No match between `\r` and `\n`.
                        !(i > 0 && text[i - 1] == '\r')
                    } else {
                        is_line_terminator(c)
                    }
                } else {
                    true
                }
            }
            Assert::Bound { negate } => {
                // Java also counts a non-spacing mark after a word character
                // as part of the word; that needs Unicode tables, so a
                // non-ASCII neighbour is counted.
                let word = |c: char| -> R<bool> {
                    if !c.is_ascii() {
                        return Err(Unsupported("\\b next to a non-ASCII character".into()));
                    }
                    Ok(c.is_ascii_alphanumeric() || c == '_')
                };
                let left = i > 0 && word(text[i - 1])?;
                let right = i < end && word(text[i])?;
                (left != right) != negate
            }
        })
    }

    /// Java's `Curly` over one character class.
    fn curly(
        &mut self,
        class: &Class,
        min: u32,
        max: Option<u32>,
        greed: Greed,
        next: usize,
        i: usize,
    ) -> R<bool> {
        let text = self.text;
        let fits = |count: usize| max.is_none_or(|max| count < max as usize);
        let mut count = 0usize;
        let mut at = i;
        while count < min as usize {
            match text.get(at) {
                Some(c) if test(class, *c)? => {}
                _ => return Ok(false),
            }
            at += 1;
            count += 1;
        }
        match greed {
            Greed::Lazy => loop {
                if self.at(next, at)? {
                    return Ok(true);
                }
                if !fits(count) {
                    return Ok(false);
                }
                match text.get(at) {
                    Some(c) if test(class, *c)? => {}
                    _ => return Ok(false),
                }
                self.spend()?;
                at += 1;
                count += 1;
            },
            Greed::Greedy | Greed::Possessive => {
                let floor = at;
                while fits(count) {
                    match text.get(at) {
                        Some(c) if test(class, *c)? => {}
                        _ => break,
                    }
                    self.spend()?;
                    at += 1;
                    count += 1;
                }
                if greed == Greed::Possessive {
                    return self.at(next, at);
                }
                loop {
                    if self.at(next, at)? {
                        return Ok(true);
                    }
                    if at == floor {
                        return Ok(false);
                    }
                    at -= 1;
                }
            }
        }
    }

    /// Java's `Loop.matchInit` (greedy) and `LazyLoop.matchInit`.
    fn loop_init(&mut self, id: usize, i: usize) -> R<bool> {
        let Node::Loop {
            body,
            count,
            min,
            max,
            lazy,
            next,
            ..
        } = &self.program.nodes[id]
        else {
            return Err(Unsupported("a malformed loop".into()));
        };
        let (body, count, min, max, lazy, next) = (*body, *count, *min, *max, *lazy, *next);
        let saved = self.locals[count];
        let result = if min > 0 {
            self.locals[count] = 1;
            self.at(body, i)
        } else if lazy {
            if self.at(next, i)? {
                Ok(true)
            } else if max.is_none_or(|max| max > 0) {
                self.locals[count] = 1;
                self.at(body, i)
            } else {
                Ok(false)
            }
        } else if max.is_none_or(|max| max > 0) {
            self.locals[count] = 1;
            if self.at(body, i)? {
                Ok(true)
            } else {
                self.at(next, i)
            }
        } else {
            self.at(next, i)
        };
        self.locals[count] = saved;
        result
    }

    /// Java's `Loop.match` / `LazyLoop.match`: after one iteration of the
    /// body. An iteration that consumed nothing leaves the loop.
    fn loop_match(&mut self, id: usize, i: usize) -> R<bool> {
        let Node::Loop {
            body,
            count,
            begin,
            min,
            max,
            lazy,
            next,
        } = &self.program.nodes[id]
        else {
            return Err(Unsupported("a malformed loop".into()));
        };
        let (body, count_local, begin, min, max, lazy, next) =
            (*body, *count, *begin, *min, *max, *lazy, *next);
        let begun = self.locals[begin];
        if begun != UNSET && i <= begun {
            return self.at(next, i);
        }
        let count = self.locals[count_local];
        let below_max = max.is_none_or(|max| (count as u64) < max as u64);
        if (count as u64) < min as u64 {
            self.locals[count_local] = count + 1;
            let matched = self.at(body, i)?;
            if !matched {
                self.locals[count_local] = count;
            }
            return Ok(matched);
        }
        if lazy {
            if self.at(next, i)? {
                return Ok(true);
            }
            if !below_max {
                return Ok(false);
            }
            self.locals[count_local] = count + 1;
            let matched = self.at(body, i)?;
            if !matched {
                self.locals[count_local] = count;
            }
            return Ok(matched);
        }
        if below_max {
            self.locals[count_local] = count + 1;
            if self.at(body, i)? {
                return Ok(true);
            }
            self.locals[count_local] = count;
        }
        self.at(next, i)
    }
}
