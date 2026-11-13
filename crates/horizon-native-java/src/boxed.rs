//! A boxed `Boolean`, `Byte`, `Short`, `Character` or `Integer`: its own
//! [`Value::Boxed`] variant.
//!
//! These boxes used to be `Value::Ref` text shaped `integer:5`, `short:5`,
//! `byte:5`, `character:97` and `boolean:true`. A Java `String` with the same
//! text was then the same value: `"" + "integer:5"` printed `5`, the literal
//! `"boolean:true"` was a `Boolean` to `getClass()` and `instanceof`, and two
//! different literals (`"integer:5"`, `"integer:05"`) were one number. No
//! choice of text can stay out of a Java string's reach (a `String` holds any
//! UTF-16 code units), so the class travels in the variant instead.
//!
//! A `Float` is boxed here too (its `f32` bits), so it is never the same
//! value as a `Double`, which stays a `Value::Float`, as a `Long` stays a
//! `Value::Long`. An `Integer` built by `valueOf` is `Boxed::Integer`; one
//! built by lambda boxing or `IntStream.boxed()` is a bare `Value::Int`.
//! [`canonical`] makes the two one value wherever they are compared, hashed
//! or used as a key.

/// The box class and its primitive value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Boxed {
    Boolean(bool),
    Byte(i8),
    Short(i16),
    /// A UTF-16 code unit: a lone surrogate is a valid `Character`.
    Character(u16),
    Integer(i32),
    /// A `Float`: its `f32` bits.
    Float(u32),
}

impl Boxed {
    /// The box class's internal name.
    pub fn class(self) -> &'static str {
        match self {
            Self::Boolean(_) => "java/lang/Boolean",
            Self::Byte(_) => "java/lang/Byte",
            Self::Short(_) => "java/lang/Short",
            Self::Character(_) => "java/lang/Character",
            Self::Integer(_) => "java/lang/Integer",
            Self::Float(_) => "java/lang/Float",
        }
    }

    /// A boxed `Float` of `value` (rounded to `f32`).
    pub fn float(value: f64) -> Self {
        Self::Float((value as f32).to_bits())
    }

    /// The primitive the box holds, as the interpreter holds that primitive:
    /// an `int` (a `boolean` 0 or 1, a `char` its code unit) or a `float`.
    pub fn primitive(self) -> crate::Value {
        match self {
            Self::Float(bits) => crate::Value::Float(f64::from(f32::from_bits(bits))),
            other => crate::Value::Int(other.int()),
        }
    }

    /// The primitive as the interpreter's `int` (a `boolean` is 0 or 1, a
    /// `char` its unsigned code unit, a `float` truncated as `f2i` does).
    pub fn int(self) -> i32 {
        match self {
            Self::Float(bits) => f32::from_bits(bits) as i32,
            Self::Boolean(value) => i32::from(value),
            Self::Byte(value) => i32::from(value),
            Self::Short(value) => i32::from(value),
            Self::Character(value) => i32::from(value),
            Self::Integer(value) => value,
        }
    }

    /// Java's `toString()` of the box.
    pub fn text(self) -> String {
        match self {
            Self::Boolean(value) => value.to_string(),
            Self::Character(unit) => String::from_utf16_lossy(&[unit]),
            Self::Float(bits) => crate::format_java_float32(f64::from(f32::from_bits(bits))),
            other => other.int().to_string(),
        }
    }

    /// `valueOf` of the box class named `class` for the `int` the interpreter
    /// holds, narrowed as the JVM narrows the primitive (`i2b`, `i2s`, `i2c`).
    pub fn of(class: &str, value: i32) -> Option<Self> {
        Some(match class {
            "java/lang/Boolean" => Self::Boolean(value != 0),
            "java/lang/Byte" => Self::Byte(value as i8),
            "java/lang/Short" => Self::Short(value as i16),
            "java/lang/Character" => Self::Character(value as u16),
            "java/lang/Integer" => Self::Integer(value),
            _ => return None,
        })
    }

    /// The capture-marker form (`value_marker`): `B` + a class letter + the
    /// value. No other value encodes with a leading `B`.
    pub(crate) fn marker(self) -> String {
        let letter = match self {
            Self::Boolean(_) => 'Z',
            Self::Byte(_) => 'B',
            Self::Short(_) => 'S',
            Self::Character(_) => 'C',
            Self::Integer(_) => 'I',
            Self::Float(bits) => return format!("BF{bits}"),
        };
        format!("B{letter}{}", self.int())
    }

    /// Read [`Self::marker`] back.
    pub(crate) fn parse_marker(marker: &str) -> Option<Self> {
        let rest = marker.strip_prefix('B')?;
        let (letter, value) = rest.split_at_checked(1)?;
        let class = match letter {
            "Z" => "java/lang/Boolean",
            "B" => "java/lang/Byte",
            "S" => "java/lang/Short",
            "C" => "java/lang/Character",
            "I" => "java/lang/Integer",
            "F" => return value.parse().ok().map(Self::Float),
            _ => return None,
        };
        Self::of(class, value.parse().ok()?)
    }
}

/// One value for an `Integer` however it was boxed: `Boxed::Integer(v)` is
/// `Value::Int(v)` (`Integer.valueOf(1).equals(i)` for an `i` from
/// `IntStream.boxed()`, and the same key in a `HashMap`). Every other value
/// is itself.
pub(crate) fn canonical(value: &crate::Value) -> std::borrow::Cow<'_, crate::Value> {
    match value {
        crate::Value::Boxed(Boxed::Integer(v)) => std::borrow::Cow::Owned(crate::Value::Int(*v)),
        other => std::borrow::Cow::Borrowed(other),
    }
}
