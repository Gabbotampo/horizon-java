//! Java's regular-expression syntax, parsed into a tree.
//!
//! The parser accepts a subset of `java.util.regex.Pattern` whose meaning the
//! matcher reproduces exactly, and answers [`Unsupported`] for everything
//! else, including every pattern Java rejects: the runtime never invents a
//! `PatternSyntaxException`, and never accepts text Java would reject. The
//! subset is listed in the module docs of [`super`].
//!
//! Flags are resolved while parsing, the way `Pattern` does it: each literal,
//! class and anchor records the case-insensitive / dotall / multiline state
//! in force where it was written.

use super::Unsupported;

/// `Pattern.CASE_INSENSITIVE`.
pub(crate) const CASE_INSENSITIVE: i32 = 0x02;
/// `Pattern.MULTILINE`.
pub(crate) const MULTILINE: i32 = 0x08;
/// `Pattern.LITERAL`.
pub(crate) const LITERAL: i32 = 0x10;
/// `Pattern.DOTALL`.
pub(crate) const DOTALL: i32 = 0x20;
/// The flags this module models; any other bit is counted.
pub(crate) const MODELLED_FLAGS: i32 = CASE_INSENSITIVE | MULTILINE | LITERAL | DOTALL;

/// Repetition counts above this are counted rather than modelled.
const MAX_COUNT: u32 = 100_000;
/// Nesting deeper than this is counted (Java's own limit is its stack).
const MAX_DEPTH: usize = 64;

/// One character test.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Class {
    /// A literal; `ci` folds ASCII letters (Java's default case folding).
    Char(char, bool),
    /// `a-z`; `ci` also accepts an ASCII character whose other case is in range.
    Range(char, char, bool),
    /// `.`: everything but a line terminator, or everything with DOTALL.
    Dot {
        dotall: bool,
    },
    Digit,
    Word,
    Space,
    HSpace,
    VSpace,
    Posix(Posix),
    /// A Unicode general category, decided only for ASCII characters.
    Category(Category),
    Not(Box<Class>),
    Union(Vec<Class>),
    And(Box<Class>, Box<Class>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Posix {
    Lower,
    Upper,
    Ascii,
    Alpha,
    Digit,
    Alnum,
    Punct,
    Graph,
    Print,
    Blank,
    Cntrl,
    XDigit,
    Space,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Category {
    Letter,
    Upper,
    Lower,
    Number,
    DecimalDigit,
}

/// A zero-width test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Assert {
    /// `^` without MULTILINE, and `\A`.
    Begin,
    /// `\z`.
    End,
    /// `$` (`multiline` per flag) and `\Z` (never multiline).
    Dollar { multiline: bool },
    /// `^` with MULTILINE.
    Caret,
    /// `\b` (`negate` false) and `\B`.
    Bound { negate: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Greed {
    Greedy,
    Lazy,
    Possessive,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Ast {
    Empty,
    /// One character matched by a class (literals included).
    One(Class),
    Concat(Vec<Ast>),
    Alt(Vec<Ast>),
    /// A group; `Some(n)` captures as group `n`.
    Group(Option<usize>, Box<Ast>),
    Repeat {
        node: Box<Ast>,
        min: u32,
        /// `None` is unbounded.
        max: Option<u32>,
        greed: Greed,
    },
    Look {
        behind: bool,
        negate: bool,
        node: Box<Ast>,
    },
    Atomic(Box<Ast>),
    Backref(usize),
    Assert(Assert),
}

/// A parsed pattern.
#[derive(Debug, Clone)]
pub(crate) struct Parsed {
    pub(crate) ast: Ast,
    /// Capturing groups, not counting group 0.
    pub(crate) groups: usize,
    /// Named groups, in definition order.
    pub(crate) names: Vec<(String, usize)>,
}

fn unsupported<T>(reason: impl Into<String>) -> Result<T, Unsupported> {
    Err(Unsupported(reason.into()))
}

/// Parse `pattern` compiled with `flags` (`Pattern.compile(pattern, flags)`).
pub(crate) fn parse(pattern: &str, flags: i32) -> Result<Parsed, Unsupported> {
    if flags & !MODELLED_FLAGS != 0 {
        return unsupported(format!("flags 0x{flags:x}"));
    }
    let chars: Vec<char> = pattern.chars().collect();
    if chars.iter().any(|c| *c as u32 > 0xFFFF) {
        return unsupported("a supplementary character in the pattern");
    }
    if flags & LITERAL != 0 {
        let ci = flags & CASE_INSENSITIVE != 0;
        let ast = Ast::Concat(
            chars
                .into_iter()
                .map(|c| Ast::One(Class::Char(c, ci)))
                .collect(),
        );
        return Ok(Parsed {
            ast,
            groups: 0,
            names: Vec::new(),
        });
    }
    let mut parser = Parser {
        p: chars,
        at: 0,
        flags,
        groups: 0,
        closed: Vec::new(),
        names: Vec::new(),
    };
    let ast = parser.alternation(0)?;
    if parser.at != parser.p.len() {
        // An unmatched `)`: Java's "Unmatched closing ')'".
        return unsupported("an unmatched ')'");
    }
    Ok(Parsed {
        ast,
        groups: parser.groups,
        names: parser.names,
    })
}

struct Parser {
    p: Vec<char>,
    at: usize,
    flags: i32,
    groups: usize,
    /// `closed[n - 1]`: whether group `n` has been closed so far.
    closed: Vec<bool>,
    names: Vec<(String, usize)>,
}

/// One parsed atom and whether a quantifier may follow it.
struct Atom {
    ast: Ast,
    quantifiable: bool,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.p.get(self.at).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.p.get(self.at + offset).copied()
    }

    fn next(&mut self) -> Result<char, Unsupported> {
        let c = self
            .peek()
            .ok_or_else(|| Unsupported("a pattern that ends early".into()))?;
        self.at += 1;
        Ok(c)
    }

    fn ci(&self) -> bool {
        self.flags & CASE_INSENSITIVE != 0
    }

    fn alternation(&mut self, depth: usize) -> Result<Ast, Unsupported> {
        if depth > MAX_DEPTH {
            return unsupported("groups nested too deeply");
        }
        let mut alternatives = vec![self.sequence(depth)?];
        while self.peek() == Some('|') {
            self.at += 1;
            alternatives.push(self.sequence(depth)?);
        }
        Ok(if alternatives.len() == 1 {
            alternatives.pop().unwrap_or(Ast::Empty)
        } else {
            Ast::Alt(alternatives)
        })
    }

    fn sequence(&mut self, depth: usize) -> Result<Ast, Unsupported> {
        let mut nodes = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            // `\Q...\E`: every character is a literal; a quantifier after the
            // `\E` binds to the last one, as Java's quoting rewrite leaves it.
            if c == '\\' && self.peek_at(1) == Some('Q') {
                self.at += 2;
                let mut quoted = Vec::new();
                while let Some(q) = self.peek() {
                    if q == '\\' && self.peek_at(1) == Some('E') {
                        self.at += 2;
                        break;
                    }
                    self.at += 1;
                    quoted.push(q);
                }
                let Some(last) = quoted.pop() else {
                    if self.quantifier_follows() {
                        return unsupported("a quantifier after an empty \\Q\\E");
                    }
                    continue;
                };
                let ci = self.ci();
                nodes.extend(quoted.into_iter().map(|q| Ast::One(Class::Char(q, ci))));
                let atom = Atom {
                    ast: Ast::One(Class::Char(last, ci)),
                    quantifiable: true,
                };
                nodes.push(self.quantified(atom)?);
                continue;
            }
            let Some(atom) = self.atom(depth)? else {
                // A flag group `(?i)` changes state and adds no node.
                if self.quantifier_follows() {
                    return unsupported("a quantifier after a flag group");
                }
                continue;
            };
            nodes.push(self.quantified(atom)?);
        }
        Ok(match nodes.len() {
            0 => Ast::Empty,
            1 => nodes.pop().unwrap_or(Ast::Empty),
            _ => Ast::Concat(nodes),
        })
    }

    fn quantifier_follows(&self) -> bool {
        matches!(self.peek(), Some('*' | '+' | '?' | '{'))
    }

    /// One atom, or `None` for a flag-only group.
    fn atom(&mut self, depth: usize) -> Result<Option<Atom>, Unsupported> {
        let c = self.next()?;
        let ci = self.ci();
        let one = |class| {
            Ok(Some(Atom {
                ast: Ast::One(class),
                quantifiable: true,
            }))
        };
        let zero_width = |ast| {
            Ok(Some(Atom {
                ast,
                quantifiable: false,
            }))
        };
        match c {
            '.' => one(Class::Dot {
                dotall: self.flags & DOTALL != 0,
            }),
            '[' => one(self.class(depth)?),
            '(' => self.group(depth),
            '^' => zero_width(Ast::Assert(if self.flags & MULTILINE != 0 {
                Assert::Caret
            } else {
                Assert::Begin
            })),
            '$' => zero_width(Ast::Assert(Assert::Dollar {
                multiline: self.flags & MULTILINE != 0,
            })),
            '\\' => self.escape_atom(),
            // Java: "Dangling meta character" and "Illegal repetition".
            '*' | '+' | '?' | '{' => unsupported(format!("a dangling '{c}'")),
            c => one(Class::Char(c, ci)),
        }
    }

    /// The atom after a `\` outside a class.
    fn escape_atom(&mut self) -> Result<Option<Atom>, Unsupported> {
        let c = self.next()?;
        let zero_width = |assert| {
            Ok(Some(Atom {
                ast: Ast::Assert(assert),
                quantifiable: false,
            }))
        };
        match c {
            'A' => zero_width(Assert::Begin),
            'z' => zero_width(Assert::End),
            'Z' => zero_width(Assert::Dollar { multiline: false }),
            'b' if self.peek() == Some('{') => unsupported("\\b{g}"),
            'b' => zero_width(Assert::Bound { negate: false }),
            'B' => zero_width(Assert::Bound { negate: true }),
            'R' => {
                // `\u000D\u000A|[\u000A\u000B\u000C\u000D\u0085  ]`,
                // as the Javadoc defines it.
                let crlf = Ast::Concat(vec![
                    Ast::One(Class::Char('\r', false)),
                    Ast::One(Class::Char('\n', false)),
                ]);
                let single = Ast::One(Class::Union(
                    [
                        '\n', '\u{0B}', '\u{0C}', '\r', '\u{85}', '\u{2028}', '\u{2029}',
                    ]
                    .into_iter()
                    .map(|c| Class::Char(c, false))
                    .collect(),
                ));
                Ok(Some(Atom {
                    ast: Ast::Group(None, Box::new(Ast::Alt(vec![crlf, single]))),
                    quantifiable: false,
                }))
            }
            '1'..='9' => {
                let group = self.backref_number(c)?;
                Ok(Some(Atom {
                    ast: self.backref(group)?,
                    quantifiable: false,
                }))
            }
            'k' => {
                if self.next()? != '<' {
                    return unsupported("\\k without '<'");
                }
                let name = self.group_name()?;
                let Some(group) = self
                    .names
                    .iter()
                    .find(|(known, _)| *known == name)
                    .map(|(_, group)| *group)
                else {
                    return unsupported(format!("\\k<{name}> before its group"));
                };
                Ok(Some(Atom {
                    ast: self.backref(group)?,
                    quantifiable: false,
                }))
            }
            _ => {
                self.at -= 1;
                let class = self.escape_class()?;
                Ok(Some(Atom {
                    ast: Ast::One(class),
                    quantifiable: true,
                }))
            }
        }
    }

    /// Java's back-reference number: the first digit always counts, each
    /// further digit only while the number names a group opened so far.
    fn backref_number(&mut self, first: char) -> Result<usize, Unsupported> {
        let mut number = first as usize - '0' as usize;
        while let Some(d) = self.peek().filter(char::is_ascii_digit) {
            let longer = number * 10 + (d as usize - '0' as usize);
            if longer > self.groups {
                break;
            }
            number = longer;
            self.at += 1;
        }
        Ok(number)
    }

    fn backref(&self, group: usize) -> Result<Ast, Unsupported> {
        if self.ci() {
            return unsupported("a case-insensitive back reference");
        }
        if !self
            .closed
            .get(group.wrapping_sub(1))
            .copied()
            .unwrap_or(false)
        {
            return unsupported(format!("a back reference to open or missing group {group}"));
        }
        Ok(Ast::Backref(group))
    }

    /// The class an escape names (`\d`, `\t`, `\.`, `\x41`...), in or out of
    /// a bracket class. The cursor is on the character after `\`.
    fn escape_class(&mut self) -> Result<Class, Unsupported> {
        let ci = self.ci();
        let c = self.next()?;
        let literal = |c: char| Ok(Class::Char(c, ci));
        match c {
            'd' => Ok(Class::Digit),
            'D' => Ok(Class::Not(Box::new(Class::Digit))),
            'w' => Ok(Class::Word),
            'W' => Ok(Class::Not(Box::new(Class::Word))),
            's' => Ok(Class::Space),
            'S' => Ok(Class::Not(Box::new(Class::Space))),
            'h' => Ok(Class::HSpace),
            'H' => Ok(Class::Not(Box::new(Class::HSpace))),
            'v' => Ok(Class::VSpace),
            'V' => Ok(Class::Not(Box::new(Class::VSpace))),
            'p' | 'P' => {
                let class = self.property()?;
                Ok(if c == 'P' {
                    Class::Not(Box::new(class))
                } else {
                    class
                })
            }
            't' => literal('\t'),
            'n' => literal('\n'),
            'r' => literal('\r'),
            'f' => literal('\u{0C}'),
            'a' => literal('\u{07}'),
            'e' => literal('\u{1B}'),
            '0' => {
                let value = self.octal()?;
                literal(value)
            }
            'x' => {
                let value = self.hex_escape()?;
                literal(value)
            }
            'u' => {
                let mut value = 0u32;
                for _ in 0..4 {
                    let digit = self.next()?.to_digit(16);
                    let Some(digit) = digit else {
                        return unsupported("an illegal \\u escape");
                    };
                    value = value * 16 + digit;
                }
                if (0xD800..=0xDFFF).contains(&value) {
                    return unsupported("a surrogate \\u escape");
                }
                literal(char::from_u32(value).ok_or_else(|| Unsupported("\\u".into()))?)
            }
            'c' => {
                let control = self.next()? as u32 ^ 64;
                literal(char::from_u32(control).ok_or_else(|| Unsupported("\\c".into()))?)
            }
            c if c.is_ascii_alphanumeric() => unsupported(format!("the escape \\{c}")),
            // Java allows escaping any other character as itself.
            c => literal(c),
        }
    }

    /// `\0n`, `\0nn`, `\0mnn` (m ≤ 3), after the `0`.
    fn octal(&mut self) -> Result<char, Unsupported> {
        let digit = |c: Option<char>| c.filter(|c| ('0'..='7').contains(c)).map(|c| c as u32 - 48);
        let Some(n) = digit(self.peek()) else {
            return unsupported("an illegal octal escape");
        };
        self.at += 1;
        let Some(m) = digit(self.peek()) else {
            return Ok(char::from_u32(n).unwrap_or('\0'));
        };
        self.at += 1;
        if let Some(o) = digit(self.peek())
            && n <= 3
        {
            self.at += 1;
            return Ok(char::from_u32(n * 64 + m * 8 + o).unwrap_or('\0'));
        }
        Ok(char::from_u32(n * 8 + m).unwrap_or('\0'))
    }

    /// `\xhh` or `\x{h...}`, after the `x`.
    fn hex_escape(&mut self) -> Result<char, Unsupported> {
        let mut value = 0u32;
        if self.peek() == Some('{') {
            self.at += 1;
            let mut digits = 0;
            while let Some(digit) = self.peek().and_then(|c| c.to_digit(16)) {
                self.at += 1;
                digits += 1;
                value = value.saturating_mul(16).saturating_add(digit);
            }
            if digits == 0 || self.next()? != '}' {
                return unsupported("an illegal \\x{...} escape");
            }
        } else {
            for _ in 0..2 {
                let Some(digit) = self.next()?.to_digit(16) else {
                    return unsupported("an illegal \\x escape");
                };
                value = value * 16 + digit;
            }
        }
        if value > 0xFFFF || (0xD800..=0xDFFF).contains(&value) {
            return unsupported("a supplementary or surrogate \\x escape");
        }
        char::from_u32(value).ok_or_else(|| Unsupported("\\x".into()))
    }

    /// `\p{Name}` or `\pL`, after the `p`.
    fn property(&mut self) -> Result<Class, Unsupported> {
        let name: String = if self.peek() == Some('{') {
            self.at += 1;
            let mut name = String::new();
            loop {
                match self.next()? {
                    '}' => break,
                    c => name.push(c),
                }
            }
            name
        } else {
            self.next()?.to_string()
        };
        let posix = match name.as_str() {
            "Lower" => Some(Posix::Lower),
            "Upper" => Some(Posix::Upper),
            "ASCII" => Some(Posix::Ascii),
            "Alpha" => Some(Posix::Alpha),
            "Digit" => Some(Posix::Digit),
            "Alnum" => Some(Posix::Alnum),
            "Punct" => Some(Posix::Punct),
            "Graph" => Some(Posix::Graph),
            "Print" => Some(Posix::Print),
            "Blank" => Some(Posix::Blank),
            "Cntrl" => Some(Posix::Cntrl),
            "XDigit" => Some(Posix::XDigit),
            "Space" => Some(Posix::Space),
            _ => None,
        };
        if let Some(posix) = posix {
            // Case-insensitive `Lower`/`Upper` follow rules this module does
            // not model.
            if self.ci() && matches!(posix, Posix::Lower | Posix::Upper) {
                return unsupported(format!("case-insensitive \\p{{{name}}}"));
            }
            return Ok(Class::Posix(posix));
        }
        let category = match name.as_str() {
            "L" | "IsL" => Category::Letter,
            "Lu" | "IsLu" => Category::Upper,
            "Ll" | "IsLl" => Category::Lower,
            "N" | "IsN" => Category::Number,
            "Nd" | "IsNd" => Category::DecimalDigit,
            _ => return unsupported(format!("the property \\p{{{name}}}")),
        };
        if self.ci() {
            return unsupported(format!("case-insensitive \\p{{{name}}}"));
        }
        Ok(Class::Category(category))
    }

    /// A bracket class; the cursor is after `[`.
    fn class(&mut self, depth: usize) -> Result<Class, Unsupported> {
        if depth > MAX_DEPTH {
            return unsupported("classes nested too deeply");
        }
        let negated = self.peek() == Some('^');
        if negated {
            self.at += 1;
        }
        let mut left: Vec<Class> = Vec::new();
        // A `]` first in a class is a literal (`[]a]`, `[^]]`).
        if self.peek() == Some(']') {
            if self.peek_at(1) == Some('-') {
                return unsupported("a range starting at a leading ']'");
            }
            self.at += 1;
            left.push(Class::Char(']', self.ci()));
        }
        let mut intersection: Option<Class> = None;
        loop {
            match self.next()? {
                ']' => break,
                '[' => left.push(self.class(depth + 1)?),
                '&' if self.peek() == Some('&') => {
                    self.at += 1;
                    if left.is_empty() || matches!(self.peek(), Some('&' | ']')) {
                        return unsupported("an '&&' with an empty side");
                    }
                    let side = Class::Union(std::mem::take(&mut left));
                    intersection = Some(match intersection.take() {
                        Some(previous) => Class::And(Box::new(previous), Box::new(side)),
                        None => side,
                    });
                }
                c => {
                    self.at -= 1;
                    let item = self.class_item(c)?;
                    left.push(item);
                }
            }
        }
        let union = Class::Union(left);
        let class = match intersection {
            Some(previous) => {
                if matches!(&union, Class::Union(items) if items.is_empty()) {
                    return unsupported("an '&&' with an empty side");
                }
                Class::And(Box::new(previous), Box::new(union))
            }
            None => union,
        };
        Ok(if negated {
            Class::Not(Box::new(class))
        } else {
            class
        })
    }

    /// One item inside a class: a literal, a range, or an escape. The cursor
    /// is on its first character, `c`.
    fn class_item(&mut self, c: char) -> Result<Class, Unsupported> {
        let ci = self.ci();
        let start = if c == '\\' {
            self.at += 1;
            if matches!(
                self.peek(),
                Some('Q' | 'E' | 'b' | 'B' | 'R' | 'k' | 'A' | 'z' | 'Z' | 'G' | 'X' | 'N')
            ) || self.peek().is_some_and(|c| c.is_ascii_digit() && c != '0')
            {
                return unsupported("this escape inside a class");
            }
            match self.escape_class()? {
                Class::Char(c, _) => c,
                other => {
                    // `[\w-/]`: a `-` after a predefined class is the next
                    // item, a literal.
                    if self.peek() == Some('-') && matches!(self.peek_at(1), Some('-' | '[' | '&'))
                    {
                        return unsupported("a '-' after a predefined class");
                    }
                    return Ok(other);
                }
            }
        } else {
            self.at += 1;
            c
        };
        if self.peek() != Some('-') {
            return Ok(Class::Char(start, ci));
        }
        match self.peek_at(1) {
            // `[a-]` and `[a-[b]]`: the `-` is the next item.
            Some(']') | Some('[') => Ok(Class::Char(start, ci)),
            Some('&') => unsupported("a range ending in '&'"),
            None => unsupported("an unclosed class"),
            Some(end) => {
                self.at += 1;
                let end = if end == '\\' {
                    self.at += 1;
                    if !matches!(self.peek(), Some(c) if !c.is_ascii_alphanumeric() || matches!(c, 't' | 'n' | 'r' | 'f' | 'a' | 'e' | 'x' | 'u' | '0' | 'c'))
                    {
                        return unsupported("a range ending in a class escape");
                    }
                    match self.escape_class()? {
                        Class::Char(end, _) => end,
                        _ => return unsupported("a range ending in a class escape"),
                    }
                } else {
                    self.at += 1;
                    end
                };
                if end < start {
                    return unsupported("an illegal character range");
                }
                Ok(Class::Range(start, end, ci))
            }
        }
    }

    /// A group; the cursor is after `(`.
    fn group(&mut self, depth: usize) -> Result<Option<Atom>, Unsupported> {
        if self.peek() != Some('?') {
            self.groups += 1;
            let index = self.groups;
            self.closed.push(false);
            let body = self.scoped_body(depth)?;
            self.closed[index - 1] = true;
            return Ok(Some(Atom {
                ast: Ast::Group(Some(index), Box::new(body)),
                quantifiable: true,
            }));
        }
        self.at += 1;
        let look = |behind, negate, body: Ast| {
            Ok(Some(Atom {
                ast: Ast::Look {
                    behind,
                    negate,
                    node: Box::new(body),
                },
                quantifiable: false,
            }))
        };
        match self.next()? {
            ':' => {
                let body = self.scoped_body(depth)?;
                Ok(Some(Atom {
                    ast: Ast::Group(None, Box::new(body)),
                    quantifiable: true,
                }))
            }
            '=' => look(false, false, self.scoped_body(depth)?),
            '!' => look(false, true, self.scoped_body(depth)?),
            '>' => {
                let body = self.scoped_body(depth)?;
                Ok(Some(Atom {
                    ast: Ast::Atomic(Box::new(body)),
                    quantifiable: false,
                }))
            }
            '<' => match self.peek() {
                Some('=') => {
                    self.at += 1;
                    look(true, false, self.scoped_body(depth)?)
                }
                Some('!') => {
                    self.at += 1;
                    look(true, true, self.scoped_body(depth)?)
                }
                _ => {
                    let name = self.group_name()?;
                    if self.names.iter().any(|(known, _)| *known == name) {
                        return unsupported(format!("a repeated group name <{name}>"));
                    }
                    self.groups += 1;
                    let index = self.groups;
                    self.closed.push(false);
                    self.names.push((name, index));
                    let body = self.scoped_body(depth)?;
                    self.closed[index - 1] = true;
                    Ok(Some(Atom {
                        ast: Ast::Group(Some(index), Box::new(body)),
                        quantifiable: true,
                    }))
                }
            },
            _ => {
                self.at -= 1;
                self.flag_group(depth)
            }
        }
    }

    /// The body of a group up to its `)`, with the flags restored after it.
    fn scoped_body(&mut self, depth: usize) -> Result<Ast, Unsupported> {
        let saved = self.flags;
        let body = self.alternation(depth + 1)?;
        if self.peek() != Some(')') {
            return unsupported("an unclosed group");
        }
        self.at += 1;
        self.flags = saved;
        Ok(body)
    }

    /// `(?idms-idms)` or `(?idms-idms:X)`; the cursor is on the first flag.
    fn flag_group(&mut self, depth: usize) -> Result<Option<Atom>, Unsupported> {
        let mut on = true;
        let mut flags = self.flags;
        loop {
            let c = self.next()?;
            let bit = match c {
                'i' => CASE_INSENSITIVE,
                'm' => MULTILINE,
                's' => DOTALL,
                '-' if on => {
                    on = false;
                    continue;
                }
                ')' => {
                    self.flags = flags;
                    return Ok(None);
                }
                ':' => {
                    let saved = self.flags;
                    self.flags = flags;
                    let body = self.scoped_body(depth)?;
                    self.flags = saved;
                    return Ok(Some(Atom {
                        ast: Ast::Group(None, Box::new(body)),
                        quantifiable: true,
                    }));
                }
                c => return unsupported(format!("the inline flag or group type '{c}'")),
            };
            if on {
                flags |= bit;
            } else {
                flags &= !bit;
            }
        }
    }

    /// A group name after `<`, through its `>`: an ASCII letter, then ASCII
    /// letters and digits.
    fn group_name(&mut self) -> Result<String, Unsupported> {
        let mut name = String::new();
        loop {
            let c = self.next()?;
            match c {
                '>' if !name.is_empty() => return Ok(name),
                c if c.is_ascii_alphabetic() || (c.is_ascii_digit() && !name.is_empty()) => {
                    name.push(c)
                }
                _ => return unsupported("an illegal group name"),
            }
        }
    }

    /// The quantifier after `atom`, if any.
    fn quantified(&mut self, atom: Atom) -> Result<Ast, Unsupported> {
        let (min, max) = match self.peek() {
            Some('*') => {
                self.at += 1;
                (0, None)
            }
            Some('+') => {
                self.at += 1;
                (1, None)
            }
            Some('?') => {
                self.at += 1;
                (0, Some(1))
            }
            Some('{') => {
                if !self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
                    return unsupported("an illegal repetition");
                }
                self.at += 1;
                let min = self.count()?;
                let max = if self.peek() == Some(',') {
                    self.at += 1;
                    if self.peek() == Some('}') {
                        None
                    } else {
                        Some(self.count()?)
                    }
                } else {
                    Some(min)
                };
                if self.next()? != '}' || max.is_some_and(|max| max < min) {
                    return unsupported("an illegal counted repetition");
                }
                (min, max)
            }
            _ => return Ok(atom.ast),
        };
        if !atom.quantifiable {
            return unsupported("a quantifier on a zero-width or compound atom");
        }
        let greed = match self.peek() {
            Some('?') => {
                self.at += 1;
                Greed::Lazy
            }
            Some('+') => {
                self.at += 1;
                Greed::Possessive
            }
            _ => Greed::Greedy,
        };
        if self.quantifier_follows() {
            return unsupported("a quantifier after a quantifier");
        }
        Ok(Ast::Repeat {
            node: Box::new(atom.ast),
            min,
            max,
            greed,
        })
    }

    fn count(&mut self) -> Result<u32, Unsupported> {
        let mut value: u32 = 0;
        let mut digits = 0;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            self.at += 1;
            digits += 1;
            value = value.saturating_mul(10).saturating_add(d);
        }
        if digits == 0 || value > MAX_COUNT {
            return unsupported("a repetition count outside the modelled range");
        }
        Ok(value)
    }
}
