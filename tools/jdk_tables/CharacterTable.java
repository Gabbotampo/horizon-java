// Generates crates/horizon-native-java/src/jdk_character_table.rs from the JDK
// that runs it:
//
//     /usr/lib/jvm/java-27-openjdk/bin/java tools/jdk_tables/CharacterTable.java \
//         > crates/horizon-native-java/src/jdk_character_table.rs
//
// Every value in the table is what this JDK's own `java.lang.Character`
// answers for that code point; nothing is derived from the Unicode files. The
// properties that the JDK's documentation defines from another one (isLetter
// from getType, digit(cp, radix) from digit(cp, 36), the `char` overloads from
// the `int` ones) are checked here for every code point, so the runtime may
// derive them, and the generator stops if one does not hold.
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public class CharacterTable {
    // Bit order of `flags`; jdk_character.rs reads them with the same names.
    static final String[] FLAGS = {
        "LOWER", "UPPER", "ALPHABETIC", "IDEOGRAPHIC", "WHITESPACE",
        "JAVA_ID_START", "JAVA_ID_PART", "UNICODE_ID_START", "UNICODE_ID_PART",
        "ID_IGNORABLE", "MIRRORED", "EMOJI", "EMOJI_PRESENTATION", "EMOJI_MODIFIER",
        "EMOJI_MODIFIER_BASE", "EMOJI_COMPONENT", "EXTENDED_PICTOGRAPHIC",
    };

    static int flags(int cp) {
        boolean[] f = {
            Character.isLowerCase(cp), Character.isUpperCase(cp), Character.isAlphabetic(cp),
            Character.isIdeographic(cp), Character.isWhitespace(cp),
            Character.isJavaIdentifierStart(cp), Character.isJavaIdentifierPart(cp),
            Character.isUnicodeIdentifierStart(cp), Character.isUnicodeIdentifierPart(cp),
            Character.isIdentifierIgnorable(cp), Character.isMirrored(cp),
            Character.isEmoji(cp), Character.isEmojiPresentation(cp),
            Character.isEmojiModifier(cp), Character.isEmojiModifierBase(cp),
            Character.isEmojiComponent(cp), Character.isExtendedPictographic(cp),
        };
        int bits = 0;
        for (int i = 0; i < f.length; i++) if (f[i]) bits |= 1 << i;
        return bits;
    }

    // One row: type, flags, directionality, the three case deltas, numeric
    // value, digit in radix 36. Case mappings are stored as deltas so a run of
    // code points with one shared offset (A-Z) stays one run.
    static String row(int cp) {
        return Character.getType(cp) + ", " + flags(cp) + ", " + Character.getDirectionality(cp)
            + ", " + (Character.toUpperCase(cp) - cp) + ", " + (Character.toLowerCase(cp) - cp)
            + ", " + (Character.toTitleCase(cp) - cp) + ", " + Character.getNumericValue(cp)
            + ", " + Character.digit(cp, 36);
    }

    static void check(boolean ok, String what, int cp) {
        if (!ok) throw new IllegalStateException(what + " does not hold at U+" + Integer.toHexString(cp));
    }

    static void checkDerived(int cp) {
        int type = Character.getType(cp);
        check(Character.isLetter(cp) == (type >= 1 && type <= 5), "isLetter", cp);
        check(Character.isDigit(cp) == (type == Character.DECIMAL_DIGIT_NUMBER), "isDigit", cp);
        check(Character.isLetterOrDigit(cp) == (Character.isLetter(cp) || Character.isDigit(cp)),
            "isLetterOrDigit", cp);
        check(Character.isTitleCase(cp) == (type == Character.TITLECASE_LETTER), "isTitleCase", cp);
        check(Character.isSpaceChar(cp) == (type == Character.SPACE_SEPARATOR
            || type == Character.LINE_SEPARATOR || type == Character.PARAGRAPH_SEPARATOR),
            "isSpaceChar", cp);
        check(Character.isDefined(cp) == (type != Character.UNASSIGNED), "isDefined", cp);
        int d36 = Character.digit(cp, 36);
        for (int radix = -1; radix <= 40; radix++) {
            int expected = radix < Character.MIN_RADIX || radix > Character.MAX_RADIX
                ? -1 : (d36 >= 0 && d36 < radix ? d36 : -1);
            check(Character.digit(cp, radix) == expected, "digit radix " + radix, cp);
        }
        if (cp <= 0xFFFF) {
            char c = (char) cp;
            check(Character.isLowerCase(c) == Character.isLowerCase(cp), "isLowerCase(char)", cp);
            check(Character.isUpperCase(c) == Character.isUpperCase(cp), "isUpperCase(char)", cp);
            check(Character.isWhitespace(c) == Character.isWhitespace(cp), "isWhitespace(char)", cp);
            check(Character.isJavaIdentifierStart(c) == Character.isJavaIdentifierStart(cp), "jis(char)", cp);
            check(Character.isJavaIdentifierPart(c) == Character.isJavaIdentifierPart(cp), "jip(char)", cp);
            check(Character.isUnicodeIdentifierStart(c) == Character.isUnicodeIdentifierStart(cp), "uis(char)", cp);
            check(Character.isUnicodeIdentifierPart(c) == Character.isUnicodeIdentifierPart(cp), "uip(char)", cp);
            check(Character.isIdentifierIgnorable(c) == Character.isIdentifierIgnorable(cp), "iig(char)", cp);
            check(Character.isMirrored(c) == Character.isMirrored(cp), "isMirrored(char)", cp);
            check(Character.getType(c) == type, "getType(char)", cp);
            check(Character.getDirectionality(c) == Character.getDirectionality(cp), "dir(char)", cp);
            check(Character.toUpperCase(c) == (char) Character.toUpperCase(cp), "toUpperCase(char)", cp);
            check(Character.toLowerCase(c) == (char) Character.toLowerCase(cp), "toLowerCase(char)", cp);
            check(Character.toTitleCase(c) == (char) Character.toTitleCase(cp), "toTitleCase(char)", cp);
            check(Character.getNumericValue(c) == Character.getNumericValue(cp), "numeric(char)", cp);
            check(Character.digit(c, 36) == d36, "digit(char)", cp);
        }
    }

    public static void main(String[] args) {
        Map<String, Integer> rows = new LinkedHashMap<>();
        List<int[]> runs = new ArrayList<>();
        String previous = null;
        for (int cp = 0; cp <= Character.MAX_CODE_POINT; cp++) {
            checkDerived(cp);
            String row = row(cp);
            if (row.equals(previous)) continue;
            previous = row;
            Integer index = rows.get(row);
            if (index == null) {
                index = rows.size();
                rows.put(row, index);
            }
            runs.add(new int[] {cp, index});
        }
        StringBuilder out = new StringBuilder();
        out.append("//! `java.lang.Character` properties of every code point, as ")
            .append(System.getProperty("java.vm.name")).append(' ')
            .append(System.getProperty("java.version")).append(" answers them.\n//!\n")
            .append("//! Generated by `tools/jdk_tables/CharacterTable.java`; do not edit.\n")
            .append("//! ").append(runs.size()).append(" runs over ").append(rows.size())
            .append(" distinct rows.\n\n");
        for (int i = 0; i < FLAGS.length; i++) {
            out.append("pub(crate) const ").append(FLAGS[i]).append(": u32 = 1 << ").append(i).append(";\n");
        }
        out.append("\n/// `(type, flags, directionality, upper delta, lower delta, title delta,\n")
            .append("/// numeric value, digit in radix 36)`.\n")
            .append("pub(crate) type Row = (u8, u32, i8, i32, i32, i32, i32, i8);\n\n");
        out.append("#[rustfmt::skip]\npub(crate) const ROWS: &[Row] = &[\n");
        for (String row : rows.keySet()) out.append("    (").append(row).append("),\n");
        out.append("];\n\n");
        out.append("/// `(first code point, row)`: each run lasts until the next one starts.\n");
        out.append("#[rustfmt::skip]\npub(crate) const RUNS: &[(u32, u16)] = &[\n");
        int column = 0;
        for (int[] run : runs) {
            if (column == 0) out.append("   ");
            out.append(" (0x").append(Integer.toHexString(run[0]).toUpperCase()).append(", ").append(run[1]).append("),");
            if (++column == 6) { out.append('\n'); column = 0; }
        }
        if (column != 0) out.append('\n');
        out.append("];\n");
        // `Character.UnicodeBlock.of(cp)`: the block's name (its `toString`,
        // the constant's field name), or "" where `of` answers null.
        out.append("\n/// `(first code point, Character.UnicodeBlock.of name)`; \"\" is null.\n");
        out.append("#[rustfmt::skip]\npub(crate) const BLOCK_RUNS: &[(u32, &str)] = &[\n");
        String previousBlock = null;
        for (int cp = 0; cp <= Character.MAX_CODE_POINT; cp++) {
            Character.UnicodeBlock block = Character.UnicodeBlock.of(cp);
            String name = block == null ? "" : block.toString();
            if (block != null && Character.UnicodeBlock.forName(name) != block)
                throw new IllegalStateException("block name " + name + " does not name its block");
            if (name.equals(previousBlock)) continue;
            previousBlock = name;
            out.append("    (0x").append(Integer.toHexString(cp).toUpperCase()).append(", \"").append(name).append("\"),\n");
        }
        out.append("];\n");
        System.out.print(out);
    }
}
