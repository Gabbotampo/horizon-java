//! `java.lang.Math` on `double` and `float`, as the JDK specifies it.
//!
//! `floor`, `ceil`, `rint`, `sqrt`, `abs` and `round` are exact in Java, and
//! IEEE 754 gives Rust the same answers, signed zeros and NaN included;
//! `round`'s tie and saturation rules are spelled out below. The functions
//! answered here were compared with JDK 25 on x86-64 and agree bit for bit.
//!
//! `pow` is left unmodelled on purpose. Java only requires it within one ulp,
//! and the platform `pow` is not HotSpot's algorithm: it differs from the JDK
//! in the last bit on some inputs, and between platform libms. An answer off
//! by one ulp would turn a counted unknown into a false `ran-clean`, so `pow`
//! stays counted until it is a port of HotSpot's own `dpow`.

use crate::runtime::{MethodRef, NativeError, Value, double_value, expect_float, float_value};

/// `Math.round(double)`: the closest `long`, ties toward positive infinity.
/// NaN is 0 and an out-of-range value saturates, which is what Rust's `as`
/// does. `value - floor` is exact, so 0.49999999999999994 rounds to 0, as the
/// JDK has since Java 7 (`floor(value + 0.5)` would answer 1).
fn round_double(value: f64) -> i64 {
    let floor = value.floor();
    let rounded = if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    rounded as i64
}

/// `Math.round(float)`, the same rule on `f32` and `int`.
fn round_float(value: f32) -> i32 {
    let floor = value.floor();
    let rounded = if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    rounded as i32
}

/// Answer a `Math` call on `double`/`float`, or `None` for any other call.
pub(crate) fn static_call(r: &MethodRef, args: &[Value]) -> Option<Result<Value, NativeError>> {
    if r.owner != "java/lang/Math" {
        return None;
    }
    let argument = |index: usize| -> Result<f64, NativeError> {
        let value = args.get(index).ok_or_else(|| {
            NativeError::Metadata(format!("{}#{} missing argument {index}", r.owner, r.name))
        })?;
        expect_float(r, value)
    };
    let unary = |op: fn(f64) -> f64| argument(0).map(|value| double_value(op(value)));
    Some(match (r.name.as_str(), r.descriptor.as_str()) {
        ("floor", "(D)D") => unary(f64::floor),
        ("ceil", "(D)D") => unary(f64::ceil),
        ("rint", "(D)D") => unary(f64::round_ties_even),
        ("sqrt", "(D)D") => unary(f64::sqrt),
        ("abs", "(D)D") => unary(f64::abs),
        ("abs", "(F)F") => argument(0).map(|value| float_value(value.abs())),
        ("round", "(D)J") => argument(0).map(|value| Value::Long(round_double(value))),
        ("round", "(F)I") => argument(0).map(|value| Value::Int(round_float(value as f32))),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, descriptor: &str, args: &[Value]) -> Value {
        let r = MethodRef {
            owner: "java/lang/Math".into(),
            name: name.into(),
            descriptor: descriptor.into(),
        };
        static_call(&r, args).unwrap().unwrap()
    }

    fn double(name: &str, args: &[f64]) -> f64 {
        let args: Vec<Value> = args.iter().map(|value| Value::Float(*value)).collect();
        match call(name, "(D)D", &args) {
            Value::Float(value) => value,
            other => panic!("{name} answered {other:?}"),
        }
    }

    /// Exact functions keep Java's signed zeros and NaN.
    #[test]
    fn exact_functions_follow_java() {
        assert_eq!(double("floor", &[-1.5]), -2.0);
        assert!(double("floor", &[-0.0]).is_sign_negative());
        assert!(double("ceil", &[-0.5]).is_sign_negative());
        assert_eq!(double("ceil", &[1.25]), 2.0);
        assert_eq!(double("rint", &[2.5]), 2.0);
        assert_eq!(double("rint", &[3.5]), 4.0);
        assert_eq!(double("sqrt", &[2.25]), 1.5);
        assert!(double("sqrt", &[-1.0]).is_nan());
        assert!(double("abs", &[-0.0]).is_sign_positive());
        assert_eq!(
            call("abs", "(F)F", &[Value::Float(-1.5)]),
            Value::Float(1.5)
        );
    }

    /// `round` ties toward positive infinity, maps NaN to 0 and saturates.
    #[test]
    fn round_follows_java() {
        let round = |value: f64| call("round", "(D)J", &[Value::Float(value)]);
        assert_eq!(round(2.5), Value::Long(3));
        assert_eq!(round(-2.5), Value::Long(-2));
        assert_eq!(round(0.49999999999999994), Value::Long(0));
        assert_eq!(round(f64::NAN), Value::Long(0));
        assert_eq!(round(1e300), Value::Long(i64::MAX));
        assert_eq!(round(f64::NEG_INFINITY), Value::Long(i64::MIN));
        let round = |value: f64| call("round", "(F)I", &[Value::Float(value)]);
        assert_eq!(round(-0.5), Value::Int(0));
        assert_eq!(round(1.5), Value::Int(2));
        assert_eq!(round(3e10), Value::Int(i32::MAX));
    }

    /// The standard host routes a `Math` call here.
    #[test]
    fn standard_host_answers_math_on_doubles() {
        use crate::runtime::{Host, StdHost};
        let r = MethodRef {
            owner: "java/lang/Math".into(),
            name: "floor".into(),
            descriptor: "(D)D".into(),
        };
        assert_eq!(
            StdHost::new().invoke(&r, &[Value::Float(-0.5)]).unwrap(),
            Value::Float(-1.0)
        );
    }

    /// Another owner, or a descriptor this module does not answer, is left
    /// to the host.
    #[test]
    fn other_calls_are_not_answered() {
        let r = MethodRef {
            owner: "java/lang/StrictMath".into(),
            name: "floor".into(),
            descriptor: "(D)D".into(),
        };
        assert!(static_call(&r, &[Value::Float(2.5)]).is_none());
        let r = MethodRef {
            owner: "java/lang/Math".into(),
            name: "sin".into(),
            descriptor: "(D)D".into(),
        };
        assert!(static_call(&r, &[Value::Float(0.0)]).is_none());
        let r = MethodRef {
            owner: "java/lang/Math".into(),
            name: "pow".into(),
            descriptor: "(DD)D".into(),
        };
        assert!(static_call(&r, &[Value::Float(2.0), Value::Float(10.0)]).is_none());
    }
}
