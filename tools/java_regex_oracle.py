#!/usr/bin/env python3
"""Ask a real JDK what `java.util.regex` answers, for the native regex model.

    java_regex_oracle.py --cases cases.json --out answers.json
    java_regex_oracle.py --fuzz N [--seed S] --out answers.json
    java_regex_oracle.py --targeted --out answers.json
    java_regex_oracle.py --emit-rust answers.json [--sample N] > oracle_cases.rs
    java_regex_oracle.py --table oracle_cases.rs [--patterns corpus.json]

The native runtime reimplements Java's regex engine
(`crates/horizon-native-java/src/java_regex/`). Its tests compare against
answers recorded from OpenJDK 27 by this script: a small Java program compiles
each pattern and reports `matches`, `lookingAt`, every `find` (all group
bounds), `replaceAll`/`replaceFirst` (or the exception they throw),
`Pattern.split` and `String.split` at several limits.

`--table` regenerates the committed table: the call shapes the census jars
make (`CORE_CASES`), a sample of real patterns (`--patterns`, a JSON list
extracted from the corpus jars' constant pools) on `TABLE_INPUTS`, and a
sample of fuzz cases.

`--fuzz` generates random patterns from a grammar over the supported syntax
(plus mutations that make some of them invalid) and random inputs over a small
alphabet. `--emit-rust` turns an answer file into the Rust literal table the
unit tests read. The ignored test `java_regex::tests::oracle_file` diffs the
native engine against any answer file (`HORIZON_REGEX_ORACLE=path`).

The JDK is `$JAVA_HOME` or `/usr/lib/jvm/java-27-openjdk`.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
import tempfile
from pathlib import Path

JAVA_SOURCE = r"""
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.regex.*;

public final class RegexOracle {
    static String q(String s) {
        if (s == null) return "null";
        StringBuilder b = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '"' || c == '\\') b.append('\\').append(c);
            else if (c < 0x20 || c > 0x7e) b.append(String.format("\\u%04x", (int) c));
            else b.append(c);
        }
        return b.append('"').toString();
    }

    static String thrown(Throwable t) {
        return q("!" + t.getClass().getName() + ": " + t.getMessage());
    }

    /** Lines: pattern, flags, then inputs and replacements, all \\u-escaped. */
    static String unescape(String s) {
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '\\' && s.charAt(i + 1) == 'u') {
                b.append((char) Integer.parseInt(s.substring(i + 2, i + 6), 16));
                i += 5;
            } else b.append(c);
        }
        return b.toString();
    }

    static String splits(Pattern p, String input) {
        StringBuilder b = new StringBuilder("[");
        int[] limits = {0, -1, 1, 2, 3};
        for (int k = 0; k < limits.length; k++) {
            if (k > 0) b.append(',');
            try {
                String[] parts = p.split(input, limits[k]);
                b.append('[');
                for (int i = 0; i < parts.length; i++) b.append(i > 0 ? "," : "").append(q(parts[i]));
                b.append(']');
            } catch (Throwable t) { b.append(thrown(t)); }
        }
        return b.append(']').toString();
    }

    static String stringSplits(String regex, String input) {
        StringBuilder b = new StringBuilder("[");
        int[] limits = {0, 2};
        for (int k = 0; k < limits.length; k++) {
            if (k > 0) b.append(',');
            try {
                String[] parts = input.split(regex, limits[k]);
                b.append('[');
                for (int i = 0; i < parts.length; i++) b.append(i > 0 ? "," : "").append(q(parts[i]));
                b.append(']');
            } catch (Throwable t) { b.append(thrown(t)); }
        }
        return b.append(']').toString();
    }

    public static void main(String[] args) throws Exception {
        List<String> lines = Files.readAllLines(Path.of(args[0]), StandardCharsets.UTF_8);
        StringBuilder out = new StringBuilder("[\n");
        int i = 0;
        boolean firstCase = true;
        while (i < lines.size()) {
            String pattern = unescape(lines.get(i++));
            int flags = Integer.parseInt(lines.get(i++));
            int inputCount = Integer.parseInt(lines.get(i++));
            List<String> inputs = new ArrayList<>();
            for (int k = 0; k < inputCount; k++) inputs.add(unescape(lines.get(i++)));
            int replacementCount = Integer.parseInt(lines.get(i++));
            List<String> replacements = new ArrayList<>();
            for (int k = 0; k < replacementCount; k++) replacements.add(unescape(lines.get(i++)));
            if (!firstCase) out.append(",\n");
            firstCase = false;
            out.append("{\"pattern\":").append(q(pattern)).append(",\"flags\":").append(flags);
            Pattern p;
            try {
                p = Pattern.compile(pattern, flags);
            } catch (Throwable t) {
                out.append(",\"compile\":").append(thrown(t)).append('}');
                continue;
            }
            out.append(",\"groups\":").append(p.matcher("").groupCount());
            out.append(",\"inputs\":[");
            for (int k = 0; k < inputs.size(); k++) {
                String input = inputs.get(k);
                if (k > 0) out.append(',');
                out.append("{\"input\":").append(q(input));
                try {
                    out.append(",\"matches\":").append(p.matcher(input).matches());
                    out.append(",\"lookingAt\":").append(p.matcher(input).lookingAt());
                    Matcher m = p.matcher(input);
                    out.append(",\"find\":[");
                    int found = 0;
                    while (found < 40 && m.find()) {
                        if (found++ > 0) out.append(',');
                        out.append('[');
                        for (int g = 0; g <= m.groupCount(); g++) {
                            if (g > 0) out.append(',');
                            out.append(m.start(g)).append(',').append(m.end(g));
                        }
                        out.append(']');
                    }
                    out.append(']');
                } catch (Throwable t) {
                    out.append(",\"error\":").append(thrown(t)).append('}');
                    continue;
                }
                out.append(",\"replace\":[");
                for (int r = 0; r < replacements.size(); r++) {
                    if (r > 0) out.append(',');
                    String all, first;
                    try { all = q(p.matcher(input).replaceAll(replacements.get(r))); } catch (Throwable t) { all = thrown(t); }
                    try { first = q(p.matcher(input).replaceFirst(replacements.get(r))); } catch (Throwable t) { first = thrown(t); }
                    out.append('[').append(q(replacements.get(r))).append(',').append(all).append(',').append(first).append(']');
                }
                out.append("],\"split\":").append(splits(p, input));
                if (flags == 0) out.append(",\"stringSplit\":").append(stringSplits(pattern, input));
                out.append('}');
            }
            out.append("]}");
        }
        out.append("\n]\n");
        System.out.print(out);
    }
}
"""

# The inputs every corpus pattern is tried on: config lines, ids, versions,
# paths and mixed text, the shapes the jars' patterns are written for.
CORPUS_INPUTS = [
    "",
    "a",
    "minecraft:stone",
    "Hello World",
    "  padded  text  ",
    "line1\r\nline2\nline3\r",
    "key = value # comment",
    "1.21.10-beta+build.7",
    "foo_bar-baz.qux/Path\\to",
    "CamelCaseString",
    "[tag] Some, list; of: things|x",
    "x=1,y=-2.5,z=3e4",
    '"quoted \\"value\\"" = "other"',
    "§aColored §ltext",
    "0x1F 077 #fff",
    "a\tb\u000bc\u000cd",
    "été naïve  nbsp",
]

# The calls the census jars make while they initialise, on the text they pass.
CORE_CASES = [
    {
        # Cupboard's `CupboardConfig#splitLongDesc`: break a long description
        # after each sentence or comma, keeping the punctuation.
        "pattern": "((?<!latest)(?<!e\\.g)\\.(?!g\\.)|,)\\h*",
        "inputs": [
            "Whether to log errors, e.g. missing files. Default: true",
            "Uses the latest. version, then e.g. g. this",
            "no punctuation here",
        ],
        "replacements": ["$1\n"],
    },
    {
        # maxhenkel's `CommentedProperties#removeNewLines`.
        "pattern": "\\r?\\n",
        "inputs": ["one\ntwo\r\nthree\r", "\n\nlead", "trail\n\n", ""],
        "replacements": [" "],
    },
    {"pattern": "\\R", "inputs": ["{\r\n  a\n\u2028b\r}", "\r\r\n"], "replacements": ["|"]},
    {"pattern": '^"desc[^"]{0,15}"\\s*:.*', "inputs": ['"desc_mode" : "x"', '"description_too_long_here" : 1']},
    {"pattern": "[a-z0-9_.-]*", "inputs": ["supermartijn642corelib", "Bad Id", ""]},
    # Shapes the independent review of #81 found answered wrongly: back
    # references into `GroupCurly`/zero-width groups and `&&` operands.
    {"pattern": "(?:(\\w))*\\1", "inputs": ["ab", "a"], "replacements": ["X"]},
    {"pattern": "(?:(a)b)*\\1", "inputs": ["abb"], "replacements": ["X"]},
    {"pattern": "((a)(b))*\\2", "inputs": ["ab"], "replacements": ["X"]},
    {"pattern": "((?=a))*\\1", "inputs": ["a", "ab"], "replacements": ["<$0>"]},
    {"pattern": "[a&&[b]&x]", "inputs": ["1a2", "a", "&"], "replacements": ["-"]},
    {"pattern": "[ab&&[b]&c]", "inputs": ["abc&"], "replacements": ["-"]},
    {"pattern": "[a&&[a]b&&b]", "inputs": ["a", "b"], "replacements": ["-"]},
    {"pattern": "(?<name>[a-z]+)-(\\d+)", "inputs": ["item-12 and block-7"], "replacements": ["${name}:$2", "${nope}", "${1x}", "$3", "${}", "${name"]},
]

# The inputs each sampled corpus pattern is tried on in the committed table.
TABLE_INPUTS = [
    "",
    "minecraft:stone",
    "  padded  text  ",
    "line1\r\nline2\nline3\r",
    "1.21.10-beta+build.7",
    "key = \"value\", x=1;y=-2.5",
]

REPLACEMENTS = ["", "-", "$0", "<$1>", "\\$", "$", "\\", "${name}", "$2$1", "$9", "x$12y"]


def java_home() -> Path:
    return Path(os.environ.get("JAVA_HOME", "/usr/lib/jvm/java-27-openjdk"))


def escape_line(text: str) -> str:
    """Every UTF-16 unit outside printable ASCII, and `\\`, as `\\uXXXX`."""
    data = text.encode("utf-16-be")
    out = []
    for index in range(0, len(data), 2):
        code = int.from_bytes(data[index : index + 2], "big")
        if 0x20 <= code < 0x7F and code != 0x5C:
            out.append(chr(code))
        else:
            out.append(f"\\u{code:04x}")
    return "".join(out)


def run_oracle(cases: list[dict]) -> list[dict]:
    with tempfile.TemporaryDirectory(prefix="horizon-regex-oracle-") as tmp:
        root = Path(tmp)
        (root / "RegexOracle.java").write_text(JAVA_SOURCE, encoding="utf-8")
        subprocess.run(
            [str(java_home() / "bin/javac"), "-d", str(root), str(root / "RegexOracle.java")],
            check=True,
        )
        lines = []
        for case in cases:
            lines.append(escape_line(case["pattern"]))
            lines.append(str(case.get("flags", 0)))
            lines.append(str(len(case["inputs"])))
            lines.extend(escape_line(text) for text in case["inputs"])
            lines.append(str(len(case["replacements"])))
            lines.extend(escape_line(text) for text in case["replacements"])
        (root / "cases.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
        result = subprocess.run(
            [str(java_home() / "bin/java"), "-Xss64m", "-cp", str(root), "RegexOracle", str(root / "cases.txt")],
            check=True,
            capture_output=True,
            text=True,
        )
        return json.loads(result.stdout)


# ---- fuzzing ----------------------------------------------------------------

FUZZ_ALPHABET = "abcAB\n\r _1-"


class Fuzzer:
    def __init__(self, seed: int) -> None:
        self.rng = random.Random(seed)
        self.groups = 0

    def literal(self) -> str:
        return self.rng.choice(["a", "b", "c", "A", "\\n", "\\r", " ", "_", "1", "-", "\\.", "\\-"])

    def klass(self) -> str:
        r = self.rng
        items = []
        for _ in range(r.randint(1, 3)):
            items.append(
                r.choice(
                    ["a", "b", "c", "a-c", "A-Z", "\\d", "\\s", "\\w", "_", "-", "\\n", "1", "[ab]", "[^a]", "\\w-_", "]"]
                )
            )
        body = "".join(items)
        if r.random() < 0.15:
            body += "&&" + r.choice(["[^b]", "b", "a-z", "[ab]"])
        return "[" + ("^" if r.random() < 0.3 else "") + body + "]"

    def atom(self, depth: int) -> str:
        r = self.rng
        roll = r.random()
        if depth > 2 or roll < 0.35:
            return self.literal()
        if roll < 0.45:
            return r.choice([".", "\\d", "\\w", "\\s", "\\W", "\\S", "\\h", "\\v"])
        if roll < 0.55:
            return self.klass()
        if roll < 0.62:
            return r.choice(["^", "$", "\\b", "\\B", "\\A", "\\z", "\\Z"])
        if roll < 0.66 and self.groups:
            return "\\" + str(r.randint(1, self.groups))
        if roll < 0.70:
            return r.choice(["(?i)", "(?m)", "(?s)", "(?-i)"])
        kind = r.choice(["(", "(", "(", "(?:", "(?:", "(?=", "(?!", "(?<=", "(?<!", "(?>", "(?i:", "(?<n" ])
        if kind == "(?<n":
            self.groups += 1
            kind = f"(?<g{self.groups}>"
        elif kind == "(":
            self.groups += 1
        if kind in ("(?<=", "(?<!"):
            inner = "|".join(self.literal() + (self.literal() if r.random() < 0.5 else "") for _ in range(r.randint(1, 2)))
            return kind + inner + ")"
        return kind + self.alternation(depth + 1) + ")"

    def quantifier(self) -> str:
        r = self.rng
        q = r.choice(["*", "+", "?", "{2}", "{1,}", "{0,2}", "{1,3}"])
        return q + r.choice(["", "", "", "?", "+"])

    def sequence(self, depth: int) -> str:
        parts = []
        for _ in range(self.rng.randint(0, 4)):
            atom = self.atom(depth)
            if self.rng.random() < 0.35 and not atom.startswith(("(?=", "(?!", "(?<=", "(?<!", "(?i)", "(?m)", "(?s)", "(?-")):
                atom += self.quantifier()
            parts.append(atom)
        return "".join(parts)

    def alternation(self, depth: int) -> str:
        return "|".join(self.sequence(depth) for _ in range(self.rng.choice([1, 1, 1, 2, 3])))

    def pattern(self) -> str:
        self.groups = 0
        text = self.alternation(0)
        if self.rng.random() < 0.08 and text:
            # A mutation: most of these make the pattern invalid.
            index = self.rng.randrange(len(text))
            text = text[:index] + self.rng.choice(["(", ")", "[", "]", "{", "*", "\\", "?", ""]) + text[index + 1 :]
        return text

    def input(self) -> str:
        return "".join(self.rng.choice(FUZZ_ALPHABET) for _ in range(self.rng.randint(0, 14)))


def fuzz_cases(count: int, seed: int) -> list[dict]:
    fuzzer = Fuzzer(seed)
    cases = []
    for _ in range(count):
        flags = fuzzer.rng.choice([0, 0, 0, 0, 2, 8, 32, 16])
        cases.append(
            {
                "pattern": fuzzer.pattern(),
                "flags": flags,
                "inputs": [fuzzer.input() for _ in range(4)],
                "replacements": ["<$0>", "$1", "\\$x"],
            }
        )
    return cases


def targeted_cases() -> list[dict]:
    """Every `&&` operand shape and back references into repeated groups:
    the two areas the independent review of #81 found answered wrongly."""
    parts = ["a", "ab", "a-c", "[b]", "[^a]", "[ab][c]", "&", "-", "\\w", "\\^", "[a]b", "b[a]", "[b]&x", "[a-z]", "x"]
    class_inputs = ["a", "b", "c", "x", "&", "-", "^", "[", "1a2&b-c^x"]
    cases = []
    for left in parts:
        for right in parts:
            for tail in ["", "&&b", "&&[b]", "&b", "&&[^c]x"]:
                for negate in ["", "^"]:
                    cases.append({"pattern": f"[{negate}{left}&&{right}{tail}]", "inputs": class_inputs, "replacements": ["-"]})
    atoms = ["(\\w)", "(a)", "(a|b)", "(ab?)", "((a)(b))", "(?:(a)b)", "(?:x(a))", "(?:(\\w)(\\w))", "(a*)", "(?:(a)|b)", "((?=a))", "(b?)"]
    ref_inputs = ["", "a", "ab", "aa", "abb", "aab", "xaa", "abab", "ba", "bab", "aaab"]
    for atom in atoms:
        for quantifier in ["*", "+", "?", "{2}", "{0,2}", "*?", "+?", "*+"]:
            for ref in ["\\1", "\\2", "\\3", "a\\1", "\\1b"]:
                cases.append({"pattern": atom + quantifier + ref, "inputs": ref_inputs, "replacements": ["X", "<$0>"]})
    return cases


# ---- Rust literals ----------------------------------------------------------


def rust_str(text: str) -> str:
    out = ['"']
    for ch in text:
        code = ord(ch)
        if ch == '"':
            out.append('\\"')
        elif ch == "\\":
            out.append("\\\\")
        elif 0x20 <= code < 0x7F:
            out.append(ch)
        else:
            out.append(f"\\u{{{code:x}}}")
    out.append('"')
    return "".join(out)


def emit_rust(answers: list[dict], sample: int | None, seed: int) -> str:
    if sample is not None and len(answers) > sample:
        answers = random.Random(seed).sample(answers, sample)
    out = [
        "// @generated by tools/java_regex_oracle.py --emit-rust from OpenJDK 27's answers.",
        "// Do not edit: regenerate (see that script's docstring).",
        "",
        "use super::{Case, Input, err, ok};",
        "",
        "// `err` is unused when no recorded split threw.",
        "const _: fn(&'static str) -> Result<&'static [&'static str], &'static str> = err;",
        "",
        "pub(super) const CASES: &[Case] = &[",
    ]
    for case in answers:
        out.append("    Case {")
        out.append(f"        pattern: {rust_str(case['pattern'])},")
        out.append(f"        flags: {case['flags']},")
        if "compile" in case:
            out.append(f"        compile_error: Some({rust_str(case['compile'])}),")
            out.append("        groups: 0,")
            out.append("        inputs: &[],")
            out.append("    },")
            continue
        out.append("        compile_error: None,")
        out.append(f"        groups: {case['groups']},")
        out.append("        inputs: &[")
        for item in case["inputs"]:
            if "error" in item:
                continue
            out.append("            Input {")
            out.append(f"                input: {rust_str(item['input'])},")
            out.append(f"                matches: {str(item['matches']).lower()},")
            out.append(f"                looking_at: {str(item['lookingAt']).lower()},")
            finds = ", ".join("&[" + ", ".join(str(v) for v in found) + "]" for found in item["find"])
            out.append(f"                find: &[{finds}],")
            replaces = ", ".join(
                "(" + ", ".join(rust_str(v) for v in triple) + ")" for triple in item["replace"]
            )
            out.append(f"                replace: &[{replaces}],")

            def parts(value: object) -> str:
                if isinstance(value, str):
                    return f"err({rust_str(value)})"
                return "ok(&[" + ", ".join(rust_str(v) for v in value) + "])"

            out.append("                split: &[" + ", ".join(parts(v) for v in item["split"]) + "],")
            string_split = item.get("stringSplit")
            if string_split is None:
                out.append("                string_split: None,")
            else:
                out.append(
                    "                string_split: Some(&[" + ", ".join(parts(v) for v in string_split) + "]),"
                )
            out.append("            },")
        out.append("        ],")
        out.append("    },")
    out.append("];")
    return "\n".join(out) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cases", type=Path, help="JSON list of {pattern, flags, inputs?, replacements?}")
    parser.add_argument("--patterns", type=Path, help="JSON list of pattern strings, tried on the corpus inputs")
    parser.add_argument("--fuzz", type=int, help="generate this many random cases")
    parser.add_argument("--targeted", action="store_true", help="add the class and back-reference grid")
    parser.add_argument("--seed", type=int, default=27)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--emit-rust", type=Path, metavar="ANSWERS")
    parser.add_argument("--sample", type=int)
    parser.add_argument("--table", type=Path, metavar="OUT_RS")
    args = parser.parse_args()

    if args.table:
        rng = random.Random(args.seed)
        cases = [dict(case) for case in CORE_CASES]
        if args.patterns:
            patterns = json.loads(args.patterns.read_text(encoding="utf-8"))
            for pattern in rng.sample(patterns, min(80, len(patterns))):
                cases.append(
                    {"pattern": pattern, "flags": 0, "inputs": TABLE_INPUTS, "replacements": ["-", "<$0>", "$1"]}
                )
        cases.extend(fuzz_cases(120, args.seed))
        for case in cases:
            case.setdefault("flags", 0)
            case.setdefault("inputs", TABLE_INPUTS)
            case.setdefault("replacements", ["-"])
        args.table.write_text(emit_rust(run_oracle(cases), None, args.seed), encoding="utf-8")
        return 0

    if args.emit_rust:
        answers = json.loads(args.emit_rust.read_text(encoding="utf-8"))
        sys.stdout.write(emit_rust(answers, args.sample, args.seed))
        return 0

    cases: list[dict] = []
    if args.cases:
        cases.extend(json.loads(args.cases.read_text(encoding="utf-8")))
    if args.patterns:
        for pattern in json.loads(args.patterns.read_text(encoding="utf-8")):
            cases.append({"pattern": pattern, "flags": 0, "inputs": CORPUS_INPUTS, "replacements": REPLACEMENTS})
    if args.fuzz:
        cases.extend(fuzz_cases(args.fuzz, args.seed))
    if args.targeted:
        cases.extend(targeted_cases())
    for case in cases:
        case.setdefault("inputs", CORPUS_INPUTS)
        case.setdefault("replacements", REPLACEMENTS)
    answers = run_oracle(cases)
    text = json.dumps(answers, ensure_ascii=False, indent=0)
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
