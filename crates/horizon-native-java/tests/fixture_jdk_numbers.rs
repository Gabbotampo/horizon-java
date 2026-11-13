//! `Integer`/`Long`/`Short`/`Byte` string parsing and `Character.isDigit`/
//! `isLetter` answer what the JDK answers: no trimming, Unicode decimal
//! digits, `NumberFormatException` with the JDK's messages (never a panic for
//! a bad radix), and general categories rather than Rust's wider properties.

/// The expected strings, one `method=text` line per `Probe` method: the
/// output of `java` running the same `Probe`, pasted verbatim. OpenJDK 27 and
/// OpenJDK 17 print the same lines, so Java 25 (Minecraft 26.3's runtime) does
/// too.
const EXPECTED: &str = "\
ints=NFE:For input string: \" 5\"|5|NFE:For input string: \"-\"|NFE:For input string: \"\"|NFE:Cannot parse null string|NFE:For input string: \"2147483648\"|-2147483648|NFE:For input string: \" 5\"|-12\n\
radixes=NFE:radix 40 greater than Character.MAX_RADIX|NFE:radix 1 less than Character.MIN_RADIX|255|255|35|NFE:For input string: \"g\" under radix 16|NFE:For input string: \"\" under radix 16|5|NFE:radix 37 greater than Character.MAX_RADIX\n\
unicodeDigits=34|12|10|NFE:For input string: \"½\"|12\n\
longs=9223372036854775807|NFE:For input string: \" 1\"|NFE:radix 37 greater than Character.MAX_RADIX|-255|NFE:For input string: \"9223372036854775808\"|NFE:Cannot parse null string|NFE:For input string: \"1L\"\n\
shortsAndBytes=NFE:Value out of range. Value:\"40000\" Radix:10|32767|NFE:For input string: \" 1\"|NFE:radix 40 greater than Character.MAX_RADIX|NFE:Value out of range. Value:\"128\" Radix:10|-128|NFE:For input string: \" 1\"|-32768\n\
characters=false|true|true|false|true|true|false|false|true|false|true|true\n\
";

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

const PROBE: &str = r##"package demo;

import java.util.function.Supplier;

public final class Probe {
    static String t(Supplier<Object> parse) {
        try {
            return String.valueOf(parse.get());
        } catch (NumberFormatException e) {
            return "NFE:" + e.getMessage();
        } catch (Throwable e) {
            return e.getClass().getSimpleName();
        }
    }

    public static String ints() {
        return t(() -> Integer.parseInt(" 5")) + "|" + t(() -> Integer.parseInt("+5")) + "|"
            + t(() -> Integer.parseInt("-")) + "|" + t(() -> Integer.parseInt("")) + "|"
            + t(() -> Integer.parseInt(null)) + "|" + t(() -> Integer.parseInt("2147483648")) + "|"
            + t(() -> Integer.parseInt("-2147483648")) + "|" + t(() -> Integer.valueOf(" 5")) + "|"
            + t(() -> Integer.valueOf("-0012"));
    }

    public static String radixes() {
        return t(() -> Integer.parseInt("1", 40)) + "|" + t(() -> Integer.parseInt("1", 1)) + "|"
            + t(() -> Integer.parseInt("ff", 16)) + "|" + t(() -> Integer.parseInt("FF", 16)) + "|"
            + t(() -> Integer.parseInt("z", 36)) + "|" + t(() -> Integer.parseInt("g", 16)) + "|"
            + t(() -> Integer.parseInt("", 16)) + "|" + t(() -> Integer.valueOf("101", 2)) + "|"
            + t(() -> Integer.valueOf("7", 37));
    }

    public static String unicodeDigits() {
        return t(() -> Integer.parseInt("٣٤")) + "|" + t(() -> Integer.parseInt("１２"))
            + "|" + t(() -> Integer.parseInt("Ａ", 16)) + "|" + t(() -> Integer.parseInt("½"))
            + "|" + t(() -> Long.parseLong("१२"));
    }

    public static String longs() {
        return t(() -> Long.parseLong("9223372036854775807")) + "|" + t(() -> Long.parseLong(" 1"))
            + "|" + t(() -> Long.parseLong("1", 37)) + "|" + t(() -> Long.parseLong("-ff", 16))
            + "|" + t(() -> Long.valueOf("9223372036854775808")) + "|" + t(() -> Long.parseLong(null))
            + "|" + t(() -> Long.parseLong("1L"));
    }

    public static String shortsAndBytes() {
        return t(() -> Short.parseShort("40000")) + "|" + t(() -> Short.parseShort("7fff", 16)) + "|"
            + t(() -> Short.valueOf(" 1")) + "|" + t(() -> Short.valueOf("1", 40)) + "|"
            + t(() -> Byte.parseByte("128")) + "|" + t(() -> Byte.valueOf("-128")) + "|"
            + t(() -> Byte.parseByte(" 1")) + "|" + t(() -> Short.parseShort("-8000", 16));
    }

    public static String characters() {
        return Character.isDigit('½') + "|" + Character.isDigit('٣') + "|"
            + Character.isDigit(0x1D7CE) + "|" + Character.isDigit('²') + "|"
            + Character.isLetter('ǅ') + "|" + Character.isLetter('ʰ') + "|"
            + Character.isLetter('Ⅰ') + "|" + Character.isLetter('ͅ') + "|"
            + Character.isLetter('中') + "|" + Character.isLetter('_') + "|"
            + Character.isLetter(0x1D400) + "|" + Character.isDigit('7');
    }
}
"##;

fn check(methods: &[&str]) {
    let fixture = javac_fixture::Fixture::build("numbers", &[("demo/Probe.java", PROBE)]);
    for method in methods {
        let expected = EXPECTED
            .lines()
            .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
            .unwrap_or_else(|| panic!("no expected line for {method}"));
        assert_eq!(fixture.text(method), expected, "{method}");
    }
}

#[test]
fn integer_parsing_does_not_trim_and_throws_the_jdk_exception() {
    check(&["ints", "longs", "shortsAndBytes"]);
}

#[test]
fn a_radix_outside_2_to_36_throws_instead_of_aborting() {
    // `Integer.parseInt("1", 40)` used to panic in `from_str_radix`.
    check(&["radixes"]);
}

#[test]
fn parsing_reads_unicode_decimal_digits_and_fullwidth_letters() {
    check(&["unicodeDigits"]);
}

#[test]
fn character_categories_are_nd_and_l_only() {
    // `isDigit('½')` (No) and `isLetter('Ⅰ')` (Nl) are false on a JVM.
    check(&["characters"]);
}
