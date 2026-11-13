//! Reading JSON text into Gson's tree: `JsonParser.parseString`/`parseReader`
//! (and the deprecated instance `parse`), and `Gson#fromJson` into a tree
//! class, as gson-2.14.0's `JsonReader` in `LENIENT` mode and
//! `JsonElementTypeAdapter.read` build it.
//!
//! A number becomes a `LazilyParsedNumber` holding the token's text; a
//! repeated name keeps its first position with the last value
//! (`JsonObject.add`); an empty document (whitespace and comments only) is
//! `JsonNull` for `JsonParser` and `null` for `Gson#fromJson`.
//!
//! Counted instead of answered: a document using the lenient forms this
//! parser does not reproduce (unquoted or single-quoted text, `;`/`=`
//! separators, implied `null`s, upper-case keywords, the `)]}'` prefix), and a
//! malformed document, including one nested past Gson's 255 levels (the
//! exception's message is the reader's position text, which is not
//! reproduced); a reader whose text the runtime does not hold; a JSON `null`,
//! or a value of the wrong kind, read into a tree class through `fromJson`
//! (the type adapter's message carries the reader's path); and any read from a
//! `Gson` whose options are themselves counted (#49).

use super::*;

const JSON_PARSER: &str = "com/google/gson/JsonParser";

/// Gson's `JsonReader` nesting limit: 255 nested arrays or objects parse,
/// the 256th throws (and so is counted here).
const MAX_TREE_DEPTH: usize = 255;

/// One parsed JSON value, before it becomes runtime objects.
enum Tree {
    Object(Vec<(String, Tree)>),
    Array(Vec<Tree>),
    Str(String),
    Num(String),
    Bool(bool),
    Null,
}

/// The outcome of reading a document with `JsonReader` in `LENIENT` mode.
enum Parsed {
    /// Only whitespace and comments: `Streams.parse` answers `JsonNull` and
    /// `Gson.fromJson` answers `null`.
    Empty,
    Value(Tree),
    /// A form this parser does not reproduce, or a malformed document.
    Unsupported,
}

/// A strict-JSON reader plus the lenient forms whose result is unambiguous:
/// `//`, `#` and `/* */` comments, unescaped control characters in strings,
/// and the `\'` escape. Anything else gives up rather than guess.
struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn parse(text: &'a str) -> Parsed {
        if text.len() > MAX_GSON_JSON_BYTES {
            return Parsed::Unsupported;
        }
        let mut parser = Parser {
            bytes: text.as_bytes(),
            text,
            pos: 0,
        };
        let parsed = (|| {
            parser.skip_space()?;
            if parser.pos == parser.bytes.len() {
                return Some(Parsed::Empty);
            }
            let value = parser.value(0)?;
            parser.skip_space()?;
            (parser.pos == parser.bytes.len()).then_some(Parsed::Value(value))
        })();
        parsed.unwrap_or(Parsed::Unsupported)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// `JsonReader.nextNonWhitespace`: the four whitespace characters and,
    /// in lenient mode, the three comment forms. A lone `/` gives up.
    fn skip_space(&mut self) -> Option<()> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r') => self.pos += 1,
                Some(b'#') => self.skip_line(),
                Some(b'/') => match self.bytes.get(self.pos + 1) {
                    Some(b'/') => self.skip_line(),
                    Some(b'*') => {
                        let rest = &self.text[self.pos + 2..];
                        let end = rest.find("*/")?;
                        self.pos += 2 + end + 2;
                    }
                    _ => return None,
                },
                _ => return Some(()),
            }
        }
    }

    fn skip_line(&mut self) {
        while let Some(byte) = self.peek() {
            self.pos += 1;
            if byte == b'\n' || byte == b'\r' {
                break;
            }
        }
    }

    /// After a number or keyword the next character must end the token, or
    /// the lenient reader reads the whole run as unquoted text.
    fn at_delimiter(&self) -> bool {
        matches!(
            self.peek(),
            None | Some(b' ' | b'\t' | b'\n' | b'\r' | b',' | b']' | b'}' | b'/' | b'#')
        )
    }

    fn value(&mut self, depth: usize) -> Option<Tree> {
        match self.peek()? {
            b'{' => {
                if depth >= MAX_TREE_DEPTH {
                    return None;
                }
                self.pos += 1;
                let mut members: Vec<(String, Tree)> = Vec::new();
                self.skip_space()?;
                if self.peek() == Some(b'}') {
                    self.pos += 1;
                    return Some(Tree::Object(members));
                }
                loop {
                    if self.peek() != Some(b'"') {
                        return None;
                    }
                    let name = self.string()?;
                    self.skip_space()?;
                    if self.peek() != Some(b':') {
                        return None;
                    }
                    self.pos += 1;
                    self.skip_space()?;
                    let value = self.value(depth + 1)?;
                    // `JsonObject.add` keeps a repeated name at its first
                    // position with the last value.
                    match members.iter_mut().find(|(existing, _)| *existing == name) {
                        Some(member) => member.1 = value,
                        None => members.push((name, value)),
                    }
                    self.skip_space()?;
                    match self.peek()? {
                        b',' => {
                            self.pos += 1;
                            self.skip_space()?;
                        }
                        b'}' => {
                            self.pos += 1;
                            return Some(Tree::Object(members));
                        }
                        _ => return None,
                    }
                }
            }
            b'[' => {
                if depth >= MAX_TREE_DEPTH {
                    return None;
                }
                self.pos += 1;
                let mut elements = Vec::new();
                self.skip_space()?;
                if self.peek() == Some(b']') {
                    self.pos += 1;
                    return Some(Tree::Array(elements));
                }
                loop {
                    elements.push(self.value(depth + 1)?);
                    self.skip_space()?;
                    match self.peek()? {
                        b',' => {
                            self.pos += 1;
                            self.skip_space()?;
                        }
                        b']' => {
                            self.pos += 1;
                            return Some(Tree::Array(elements));
                        }
                        _ => return None,
                    }
                }
            }
            b'"' => self.string().map(Tree::Str),
            b't' => self.keyword("true", Tree::Bool(true)),
            b'f' => self.keyword("false", Tree::Bool(false)),
            b'n' => self.keyword("null", Tree::Null),
            b'-' | b'0'..=b'9' => {
                let start = self.pos;
                while self.peek().is_some_and(|byte| {
                    matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                }) {
                    self.pos += 1;
                }
                let text = &self.text[start..self.pos];
                (json_number(text).is_some() && self.at_delimiter())
                    .then(|| Tree::Num(text.to_owned()))
            }
            _ => None,
        }
    }

    fn keyword(&mut self, word: &str, tree: Tree) -> Option<Tree> {
        if !self.text[self.pos..].starts_with(word) {
            return None;
        }
        self.pos += word.len();
        self.at_delimiter().then_some(tree)
    }

    /// A double-quoted string from its opening quote.
    fn string(&mut self) -> Option<String> {
        self.pos += 1;
        let mut units: Vec<u16> = Vec::new();
        loop {
            let ch = self.text[self.pos..].chars().next()?;
            self.pos += ch.len_utf8();
            match ch {
                '"' => break,
                '\\' => {
                    let escaped = self.text[self.pos..].chars().next()?;
                    self.pos += escaped.len_utf8();
                    let unit = match escaped {
                        'u' => {
                            let hex = self.text.get(self.pos..self.pos + 4)?;
                            if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                                return None;
                            }
                            self.pos += 4;
                            u16::from_str_radix(hex, 16).ok()?
                        }
                        't' => 0x09,
                        'b' => 0x08,
                        'n' => 0x0a,
                        'r' => 0x0d,
                        'f' => 0x0c,
                        '\'' | '"' | '\\' | '/' => escaped as u16,
                        _ => return None,
                    };
                    units.push(unit);
                }
                ch => {
                    let mut buffer = [0u16; 2];
                    units.extend_from_slice(ch.encode_utf16(&mut buffer));
                }
            }
        }
        // A lone surrogate has no Rust `String`: give up.
        String::from_utf16(&units).ok()
    }
}

/// What one `fromJson`/`parseReader` source holds.
enum Source {
    Text(String),
    /// A Java `null` string: `Gson.fromJson((String) null, ...)` is `null`.
    Null,
    /// A reader the runtime does not hold the text of.
    Unknown,
}

impl<H: Host> Runtime<'_, H> {
    /// Runtime objects for one parsed value, as `JsonElementTypeAdapter.read`
    /// builds them.
    fn materialize(&mut self, tree: Tree) -> Result<Value, NativeError> {
        Ok(match tree {
            Tree::Null => Value::Ref(Some(NULL_INSTANCE.to_owned())),
            Tree::Bool(value) => self.new_primitive(Prim::Bool(value)),
            Tree::Str(text) => self.new_primitive(Prim::Str(text)),
            Tree::Num(text) => {
                let number = self.new_lazy_number(text);
                self.new_primitive(Prim::Num(number))
            }
            Tree::Array(elements) => {
                let array = self.new_json_array()?;
                let Some(target) = self.elements_of(expect_string(&array)) else {
                    return Ok(array);
                };
                for element in elements {
                    let element = self.materialize(element)?;
                    self.list_add(&target, element)?;
                }
                array
            }
            Tree::Object(members) => {
                let object = self.new_json_object()?;
                let Some(target) = self.members_of(expect_string(&object)) else {
                    return Ok(object);
                };
                for (key, member) in members {
                    let member = self.materialize(member)?;
                    self.map_put(&target, &key, member)?;
                }
                object
            }
        })
    }

    /// The text a `Reader` argument holds, or a `String` argument's.
    fn source_text(&self, value: &Value, reader: bool) -> Source {
        match value {
            Value::Ref(None) => Source::Null,
            Value::Ref(Some(text)) if !reader => Source::Text(text.clone()),
            Value::Ref(Some(marker)) => self
                .string_readers
                .get(marker)
                .cloned()
                .map_or(Source::Unknown, Source::Text),
            _ => Source::Unknown,
        }
    }

    /// `JsonParser.parseString`/`parseReader` and the deprecated instance
    /// `parse` overloads.
    pub(super) fn invoke_json_parser(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let reader = match (r.name.as_str(), r.descriptor.as_str()) {
            ("<init>", "()V") => return Ok(Some(Value::Void)),
            ("parseString" | "parse", "(Ljava/lang/String;)Lcom/google/gson/JsonElement;") => false,
            ("parseReader" | "parse", "(Ljava/io/Reader;)Lcom/google/gson/JsonElement;") => true,
            _ => return self.count_tree_call(r, receiver, args).map(Some),
        };
        let source = self.source_text(args.first().unwrap_or(&Value::Ref(None)), reader);
        let Source::Text(text) = source else {
            // `new StringReader(null)` throws before the parse; an unknown
            // reader has no text to parse.
            return self.count_tree_call(r, receiver, args).map(Some);
        };
        match Parser::parse(&text) {
            Parsed::Empty => Ok(Some(Value::Ref(Some(NULL_INSTANCE.to_owned())))),
            Parsed::Value(tree) => self.materialize(tree).map(Some),
            Parsed::Unsupported => self.count_tree_call(r, receiver, args).map(Some),
        }
    }

    /// `Gson.fromJson` into a tree class. `toJson` of a tree is answered by
    /// the writer through `gson_to_json` ([`Self::gson_tree_json`]); every
    /// other `Gson` call keeps its own model.
    pub(super) fn invoke_gson_fromjson_tree(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.name != "fromJson" {
            return Ok(None);
        }
        let reader = match r.descriptor.as_str() {
            "(Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/Object;"
            | "(Ljava/lang/String;Ljava/lang/reflect/Type;)Ljava/lang/Object;" => false,
            "(Ljava/io/Reader;Ljava/lang/Class;)Ljava/lang/Object;"
            | "(Ljava/io/Reader;Ljava/lang/reflect/Type;)Ljava/lang/Object;" => true,
            _ => return Ok(None),
        };
        let Some(class) = args.get(1).and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        if !tree_class(&class) {
            return Ok(None);
        }
        let receiver_value = receiver.cloned().unwrap_or(Value::Ref(None));
        if self.gson_style_of(receiver).is_counted() {
            return self
                .count_tree_call(r, Some(&receiver_value), args)
                .map(Some);
        }
        let text = match self.source_text(args.first().unwrap_or(&Value::Ref(None)), reader) {
            Source::Text(text) => text,
            // `fromJson((String) null, ...)` is `null`.
            Source::Null if !reader => return Ok(Some(Value::Ref(None))),
            _ => {
                return self
                    .count_tree_call(r, Some(&receiver_value), args)
                    .map(Some);
            }
        };
        let tree = match Parser::parse(&text) {
            // An empty document is `null` (the reader's `EOFException` at
            // the first token).
            Parsed::Empty => return Ok(Some(Value::Ref(None))),
            Parsed::Value(tree) => tree,
            Parsed::Unsupported => {
                return self
                    .count_tree_call(r, Some(&receiver_value), args)
                    .map(Some);
            }
        };
        // The type hierarchy adapter checks the requested class; its
        // `JsonSyntaxException` message carries the reader's path, which is
        // not reproduced, and a JSON `null` for a tree class is not modelled.
        let fits = match (&tree, class.as_str()) {
            (Tree::Null, _) => false,
            (_, JSON_ELEMENT) => true,
            (Tree::Object(_), JSON_OBJECT) | (Tree::Array(_), JSON_ARRAY) => true,
            (Tree::Str(_) | Tree::Num(_) | Tree::Bool(_), JSON_PRIMITIVE) => true,
            _ => false,
        };
        if !fits {
            return self
                .count_tree_call(r, Some(&receiver_value), args)
                .map(Some);
        }
        self.materialize(tree).map(Some)
    }
}

/// Whether `r` is a `JsonParser` call.
pub(super) fn is_parser(r: &MethodRef) -> bool {
    r.owner == JSON_PARSER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parser_gives_up_on_forms_it_does_not_reproduce() {
        for text in [
            "{a:1}",
            "{'a':1}",
            "[1,]",
            "[,1]",
            "{\"a\":1,}",
            "{\"a\"=1}",
            "[1;2]",
            "TRUE",
            "01",
            "1.",
            "NaN",
            "\u{feff}{}",
            ")]}'\n{}",
            "{} {}",
            "/ {}",
            "/* open",
            "\"\\x\"",
            "\"\\ud800\"",
        ] {
            assert!(
                matches!(Parser::parse(text), Parsed::Unsupported),
                "{text:?} was answered"
            );
        }
        // gson-2.14.0 parses this to `JsonNull`.
        assert!(matches!(
            Parser::parse(" // only\n#also\n/* c */ "),
            Parsed::Empty
        ));
        // gson-2.14.0 parses 255 nested arrays and throws
        // `MalformedJsonException: Nesting limit 255 reached` at 256.
        let nested = format!("{}{}", "[".repeat(256), "]".repeat(256));
        assert!(matches!(Parser::parse(&nested), Parsed::Unsupported));
        let nested = format!("{}{}", "[".repeat(255), "]".repeat(255));
        assert!(matches!(Parser::parse(&nested), Parsed::Value(_)));
    }
}
