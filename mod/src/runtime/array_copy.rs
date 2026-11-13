//! `System.arraycopy`, as JDK 25's HotSpot runs it.
//!
//! The call was not modelled at all: it was counted, and nothing was copied.
//! night-config's `CharsWrapper.Builder` grows its buffer with
//! `System.arraycopy(old, 0, grown, offset, length)`, so on a Fabric run that
//! runs Forge Config API Port's own classes (#124) every key the TOML parser
//! read came out as `'d\0\0\0...'` and every config failed to parse.
//!
//! HotSpot's checks, in its order (`TypeArrayKlass::copy_array`,
//! `ObjArrayKlass::copy_array`; messages checked on JDK 25.0.5):
//!
//! 1. a null array: `NullPointerException` with no message;
//! 2. a source that is not an array: `ArrayStoreException("arraycopy: source
//!    type <class> is not an array")`;
//! 3. arrays of different kinds: `ArrayStoreException("arraycopy: type
//!    mismatch: can not copy char[] into int[]")`, where a reference array
//!    reads `object array[]`;
//! 4. a negative source index, destination index or length, in that order:
//!    `ArrayIndexOutOfBoundsException("arraycopy: source index -1 out of
//!    bounds for char[4]")`, `"... destination index ..."`, `"arraycopy:
//!    length -1 is negative"`;
//! 5. a range past the end, the source's first: `"arraycopy: last source
//!    index 5 out of bounds for char[4]"`, `"... last destination index ..."`.
//!
//! Then the elements are copied as if through a temporary array, so an
//! overlapping copy within one array is exact in either direction.
//!
//! Only arrays whose component the runtime recorded (`newarray`, `anewarray`,
//! and now `Arrays.copyOf`/`copyOfRange`, which keep the original's class) are
//! copied. A reference copy runs when the
//! destination's component is the source's or `Object`: any other pair needs
//! JDK's per-element store check (`element type mismatch`), and is counted.
//! So is an array with no recorded component, and a destination that is not
//! an array.

use super::*;

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

const PRIMITIVES: &[&str] = &[
    "boolean", "char", "float", "double", "byte", "short", "int", "long",
];

/// How HotSpot names an array kind in its messages: `char`, or `object array`.
fn kind_name(component: &str) -> &str {
    if PRIMITIVES.contains(&component) {
        component
    } else {
        "object array"
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `System.arraycopy`; `Ok(None)` for any other call.
    pub(super) fn invoke_array_copy(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str())
            != (
                "java/lang/System",
                "arraycopy",
                "(Ljava/lang/Object;ILjava/lang/Object;II)V",
            )
        {
            return Ok(None);
        }
        let (Some(src), Some(dst)) = (args.first(), args.get(2)) else {
            return Ok(None);
        };
        let (Value::Ref(Some(src_marker)), Value::Ref(Some(dst_marker))) = (src, dst) else {
            return Err(thrown("java/lang/NullPointerException", None));
        };
        let int = |index: usize| expect_int(r, StdHost::argument(r, args, index)?);
        let (src_pos, dst_pos, length) = (int(1)?, int(3)?, int(4)?);
        if !src_marker.starts_with("array/") {
            // A string is the one non-array value whose class is certain.
            if value_class::is_plain_string(src_marker) {
                return Err(thrown(
                    "java/lang/ArrayStoreException",
                    Some("arraycopy: source type java.lang.String is not an array".into()),
                ));
            }
            return self.host.unknown_operand_call(r).map(Some);
        }
        let (Some(src_component), Some(dst_component)) = (
            self.array_component(src_marker),
            self.array_component(dst_marker),
        ) else {
            return self.host.unknown_operand_call(r).map(Some);
        };
        let src_primitive = PRIMITIVES.contains(&src_component.as_str());
        let dst_primitive = PRIMITIVES.contains(&dst_component.as_str());
        if src_primitive != dst_primitive || (src_primitive && src_component != dst_component) {
            return Err(thrown(
                "java/lang/ArrayStoreException",
                Some(format!(
                    "arraycopy: type mismatch: can not copy {}[] into {}[]",
                    kind_name(&src_component),
                    kind_name(&dst_component)
                )),
            ));
        }
        if !src_primitive && dst_component != src_component && dst_component != "java/lang/Object" {
            // JDK checks each element's class against the destination's.
            return self.host.unknown_operand_call(r).map(Some);
        }
        let src_values = self.array(src)?.clone();
        let dst_len = self.array(dst)?.len();
        let src_len = src_values.len();
        let src_name = kind_name(&src_component).to_owned();
        let dst_name = kind_name(&dst_component).to_owned();
        let oob =
            |message: String| thrown("java/lang/ArrayIndexOutOfBoundsException", Some(message));
        if src_pos < 0 {
            return Err(oob(format!(
                "arraycopy: source index {src_pos} out of bounds for {src_name}[{src_len}]"
            )));
        }
        if dst_pos < 0 {
            return Err(oob(format!(
                "arraycopy: destination index {dst_pos} out of bounds for {dst_name}[{dst_len}]"
            )));
        }
        if length < 0 {
            return Err(oob(format!("arraycopy: length {length} is negative")));
        }
        let (src_pos, dst_pos, length) = (src_pos as usize, dst_pos as usize, length as usize);
        if src_pos + length > src_len {
            return Err(oob(format!(
                "arraycopy: last source index {} out of bounds for {src_name}[{src_len}]",
                src_pos + length
            )));
        }
        if dst_pos + length > dst_len {
            return Err(oob(format!(
                "arraycopy: last destination index {} out of bounds for {dst_name}[{dst_len}]",
                dst_pos + length
            )));
        }
        let slice = src_values[src_pos..src_pos + length].to_vec();
        let Some(target) = self.arrays.get_mut(dst_marker) else {
            return self.host.unknown_operand_call(r).map(Some);
        };
        target[dst_pos..dst_pos + length].clone_from_slice(&slice);
        Ok(Some(Value::Void))
    }
}
