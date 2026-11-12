//! `java.util.regex`, exactly, for the patterns it can decide.
//!
//! Mods split config lines (`split("\\r?\\n")`), rewrite descriptions
//! (`replaceFirst`), validate ids (`matches("[a-z0-9_.-]*")`) and compile
//! `Pattern`s while they initialise. The runtime takes no regex dependency,
//! and no Rust regex crate has Java's semantics, so this module implements
//! Java's matcher itself: a backtracking search over the same node graph
//! `Pattern` builds, with Java's precedence, group bookkeeping, anchors and
//! replacement rules, read from OpenJDK 27's bytecode and pinned by oracle
//! tests generated on that JDK (`tests.rs`).
//!
//! **The subset.** Literals and escapes (`\t \n \r \f \a \e \0oo \xhh \x{h}
//! \uhhhh \cX`, any escaped punctuation), `\Q...\E`, `.`, bracket classes with
//! ranges, negation, nesting and `&&`, `\d \D \w \W \s \S \h \H \v \V`, the
//! POSIX `\p{Alpha}` family, `\p{L} \p{Lu} \p{Ll} \p{N} \p{Nd}` (decided for
//! ASCII input only), `^ $ \A \z \Z \b \B \R`, capturing, named and
//! non-capturing groups, alternation, greedy/lazy/possessive quantifiers on
//! characters and groups, lookahead, bounded lookbehind, atomic groups, back
//! references (`\1`, `\k<name>`), and the flags `CASE_INSENSITIVE` (ASCII
//! folding, Java's default), `MULTILINE`, `DOTALL` and `LITERAL`, inline as
//! `(?ims-ims)` / `(?ims:...)` too. Input and pattern must be in the Basic
//! Multilingual Plane, so a Java `char` index is a Rust `char` index.
//!
//! **Everything else is counted** ([`Unsupported`]): other flags and
//! properties, Unicode case folding, a pattern Java rejects (the runtime does
//! not reproduce `PatternSyntaxException`'s text), a quantifier on a zero-width
//! or compound atom, a group value whose bookkeeping in Java depends on an
//! edge this module does not follow (see [`engine::Program::tainted`]), and a
//! search past its step budget or deeper than Java's own stack is sure to
//! allow (see [`engine::Limits::JAVA`]). The caller turns that into an
//! unknown host call, never a guessed answer.

// Only `matches` is wired into the runtime here; `replaceAll`/`replaceFirst`,
// `split` and the quoting helpers are answered by the stacked
// `regex_calls.rs` PR. The oracle tests already exercise all of them.
#![allow(dead_code)]

mod engine;
mod syntax;
#[cfg(test)]
mod tests;

use engine::{Limits, Matcher, Program};

/// Why a call is outside the modelled subset; the caller counts it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Unsupported(pub(crate) String);

/// A Java exception the modelled call throws, as `(class, message)`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Thrown {
    pub(crate) class: &'static str,
    pub(crate) message: String,
}

fn illegal_argument(message: impl Into<String>) -> Thrown {
    Thrown {
        class: "java/lang/IllegalArgumentException",
        message: message.into(),
    }
}

type R<T> = Result<T, Unsupported>;

/// A compiled pattern (`Pattern.compile(pattern, flags)`).
#[derive(Debug, Clone)]
pub(crate) struct Regex {
    program: Program,
}

/// Text as Java `char`s: a BMP-only string's `char`s are its UTF-16 units.
fn units(text: &str) -> R<Vec<char>> {
    let chars: Vec<char> = text.chars().collect();
    if chars.iter().any(|c| *c as u32 > 0xFFFF) {
        return Err(Unsupported("a supplementary character in the input".into()));
    }
    Ok(chars)
}

fn text(chars: &[char]) -> String {
    chars.iter().collect()
}

/// A finished match: group bounds in `char`s.
#[derive(Debug, Clone)]
pub(crate) struct Match {
    groups: Vec<usize>,
    tainted: Vec<bool>,
}

impl Match {
    fn from(matcher: &Matcher<'_>, regex: &Regex) -> Self {
        Self {
            groups: matcher.groups.clone(),
            tainted: regex.program.tainted.clone(),
        }
    }

    pub(crate) fn start(&self) -> usize {
        self.groups[0]
    }

    pub(crate) fn end(&self) -> usize {
        self.groups[1]
    }

    /// Group `n`'s bounds (`None` when it did not participate). Counted
    /// for a group whose value this module does not promise.
    pub(crate) fn group(&self, n: usize) -> R<Option<(usize, usize)>> {
        if self.tainted.get(n).copied().unwrap_or(false) {
            return Err(Unsupported(format!(
                "group {n}, whose value Java keeps by rules this module does not follow"
            )));
        }
        let (start, end) = (self.groups[2 * n], self.groups[2 * n + 1]);
        Ok((start != engine::UNSET && end != engine::UNSET).then_some((start, end)))
    }
}

impl Regex {
    pub(crate) fn compile(pattern: &str, flags: i32) -> R<Self> {
        let parsed = syntax::parse(pattern, flags)?;
        Ok(Self {
            program: engine::compile(parsed)?,
        })
    }

    /// `Matcher.groupCount()`.
    pub(crate) fn group_count(&self) -> usize {
        self.program.groups
    }

    /// The group a name refers to.
    pub(crate) fn named_group(&self, name: &str) -> Option<usize> {
        self.program
            .names
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, group)| *group)
    }

    /// `Matcher.matches()` (`whole`) or `lookingAt()` on `input`.
    pub(crate) fn match_start(&self, input: &str, whole: bool) -> R<Option<Match>> {
        let chars = units(input)?;
        let mut matcher = Matcher::new(&self.program, &chars, Limits::JAVA);
        Ok(matcher
            .match_at_start(whole)?
            .then(|| Match::from(&matcher, self)))
    }

    /// `Matcher.find()` from a fresh matcher, repeated: every match in order,
    /// with Java's rule that a search after an empty match starts one `char`
    /// later. Stops after `limit` matches when one is given.
    pub(crate) fn find_all(&self, chars: &[char], limit: Option<usize>) -> R<Vec<Match>> {
        let mut matcher = Matcher::new(&self.program, chars, Limits::JAVA);
        let mut found = Vec::new();
        let (mut first, mut last) = (usize::MAX, 0usize);
        loop {
            if limit.is_some_and(|limit| found.len() >= limit) {
                return Ok(found);
            }
            let mut from = last;
            if from == first {
                from += 1;
            }
            if from > chars.len() || !matcher.search(from)? {
                return Ok(found);
            }
            let found_match = Match::from(&matcher, self);
            (first, last) = (found_match.start(), found_match.end());
            found.push(found_match);
        }
    }

    /// `Matcher.replaceAll` (`all`) or `replaceFirst` on `input`.
    pub(crate) fn replace(
        &self,
        input: &str,
        replacement: &str,
        all: bool,
    ) -> R<Result<String, Thrown>> {
        let chars = units(input)?;
        let replacement = units(replacement)?;
        let matches = self.find_all(&chars, (!all).then_some(1))?;
        if matches.is_empty() {
            // The replacement is never parsed when nothing matched.
            return Ok(Ok(input.to_owned()));
        }
        let mut out = String::with_capacity(input.len());
        let mut append_position = 0;
        for found in &matches {
            out.push_str(&text(&chars[append_position..found.start()]));
            if let Err(thrown) = self.expand(&chars, found, &replacement, &mut out)? {
                return Ok(Err(thrown));
            }
            append_position = found.end();
        }
        out.push_str(&text(&chars[append_position..]));
        Ok(Ok(out))
    }

    /// `Matcher.appendExpandedReplacement`: `$n`, `${name}` and `\x` in a
    /// replacement string, with Java's exceptions.
    fn expand(
        &self,
        chars: &[char],
        found: &Match,
        replacement: &[char],
        out: &mut String,
    ) -> R<Result<(), Thrown>> {
        let mut cursor = 0;
        while cursor < replacement.len() {
            let c = replacement[cursor];
            if c == '\\' {
                cursor += 1;
                let Some(escaped) = replacement.get(cursor) else {
                    return Ok(Err(illegal_argument("character to be escaped is missing")));
                };
                out.push(*escaped);
                cursor += 1;
                continue;
            }
            if c != '$' {
                out.push(c);
                cursor += 1;
                continue;
            }
            cursor += 1;
            let Some(&after) = replacement.get(cursor) else {
                return Ok(Err(illegal_argument(
                    "Illegal group reference: group index is missing",
                )));
            };
            let group = if after == '{' {
                cursor += 1;
                let begin = cursor;
                let mut last = after;
                while let Some(&n) = replacement.get(cursor) {
                    last = n;
                    if !n.is_ascii_alphanumeric() {
                        break;
                    }
                    cursor += 1;
                }
                if begin == cursor {
                    return Ok(Err(illegal_argument(
                        "named capturing group has 0 length name",
                    )));
                }
                if last != '}' {
                    return Ok(Err(illegal_argument(
                        "named capturing group is missing trailing '}'",
                    )));
                }
                let name = text(&replacement[begin..cursor]);
                if name.starts_with(|c: char| c.is_ascii_digit()) {
                    return Ok(Err(illegal_argument(format!(
                        "capturing group name {{{name}}} starts with digit character"
                    ))));
                }
                let Some(group) = self.named_group(&name) else {
                    return Ok(Err(illegal_argument(format!(
                        "No group with name {{{name}}}"
                    ))));
                };
                cursor += 1;
                group
            } else {
                let Some(first) = after.to_digit(10) else {
                    return Ok(Err(illegal_argument("Illegal group reference")));
                };
                let mut group = first as usize;
                cursor += 1;
                while let Some(digit) = replacement.get(cursor).and_then(|d| d.to_digit(10)) {
                    let longer = group * 10 + digit as usize;
                    if self.group_count() < longer {
                        break;
                    }
                    group = longer;
                    cursor += 1;
                }
                group
            };
            if group > self.group_count() {
                return Ok(Err(Thrown {
                    class: "java/lang/IndexOutOfBoundsException",
                    message: format!("No group {group}"),
                }));
            }
            if let Some((start, end)) = found.group(group)? {
                out.push_str(&text(&chars[start..end]));
            }
        }
        Ok(Ok(()))
    }

    /// `Pattern.split(input, limit)`.
    pub(crate) fn split(&self, input: &str, limit: i32) -> R<Vec<String>> {
        let chars = units(input)?;
        let matches = self.find_all(&chars, None)?;
        Ok(split_at(
            &chars,
            matches.iter().map(|m| (m.start(), m.end())),
            limit,
        ))
    }
}

/// Java's `Pattern.split` over the matches' bounds.
fn split_at(
    chars: &[char],
    matches: impl Iterator<Item = (usize, usize)>,
    limit: i32,
) -> Vec<String> {
    let limited = limit > 0;
    let mut index = 0;
    let mut match_count: i64 = 0;
    let mut parts = Vec::new();
    for (start, end) in matches {
        if !limited || match_count < i64::from(limit) - 1 {
            // A zero-width match at the very beginning never produces an
            // empty leading substring.
            if index == 0 && start == 0 && start == end {
                continue;
            }
            parts.push(text(&chars[index..start]));
            index = end;
            match_count += 1;
        } else if match_count == i64::from(limit) - 1 {
            parts.push(text(&chars[index..]));
            index = end;
            match_count += 1;
        }
    }
    if index == 0 {
        return vec![text(chars)];
    }
    if !limited || match_count < i64::from(limit) {
        parts.push(text(&chars[index..]));
    }
    if limit == 0 {
        while parts.last().is_some_and(String::is_empty) {
            parts.pop();
        }
    }
    parts
}

/// `String.matches(regex)` / `Pattern.matches(regex, input)`.
pub(crate) fn matches(pattern: &str, input: &str) -> R<bool> {
    Ok(Regex::compile(pattern, 0)?
        .match_start(input, true)?
        .is_some())
}

/// `String.replaceAll` (`all`) / `replaceFirst`.
pub(crate) fn replace(
    input: &str,
    pattern: &str,
    replacement: &str,
    all: bool,
) -> R<Result<String, Thrown>> {
    Regex::compile(pattern, 0)?.replace(input, replacement, all)
}

/// `String.split(regex, limit)`, fast path included: a one-character
/// literal (or an escaped non-alphanumeric) splits without compiling, with
/// the same result `Pattern.split` gives.
pub(crate) fn string_split(input: &str, pattern: &str, limit: i32) -> R<Vec<String>> {
    Regex::compile(pattern, 0)?.split(input, limit)
}

/// `Pattern.quote(text)`.
pub(crate) fn quote(text: &str) -> String {
    if !text.contains("\\E") {
        return format!("\\Q{text}\\E");
    }
    let mut out = String::from("\\Q");
    let mut rest = text;
    while let Some(index) = rest.find("\\E") {
        out.push_str(&rest[..index]);
        out.push_str("\\E\\\\E\\Q");
        rest = &rest[index + 2..];
    }
    out.push_str(rest);
    out.push_str("\\E");
    out
}

/// `Matcher.quoteReplacement(text)`.
pub(crate) fn quote_replacement(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\\' || c == '$' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
