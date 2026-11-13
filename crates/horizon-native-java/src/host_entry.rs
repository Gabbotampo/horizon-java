//! `toString()` of the host's map entries, as JDK 25 prints them.
//!
//! A `HashMap`/`LinkedHashMap` node, a `TreeMap` entry and `Map.entry(..)`'s
//! `KeyValueHolder` are value-encoded `map-entry/k,v` markers in the host.
//! Their `toString()` printed the host's internal encoding of the key and
//! value. Each of those JDK classes answers `key + "=" + value`, which is
//! `String.valueOf(key) + "=" + String.valueOf(value)`: a jar key or value
//! prints through its own `toString` (#70), a box or string as Java does.

use super::*;

impl<H: Host> Runtime<'_, H> {
    /// `entry.toString()` for a host map entry; `None` for any other value.
    pub(crate) fn host_entry_text(
        &mut self,
        value: &Value,
        class: &str,
        method: &str,
    ) -> Result<Option<Value>, NativeError> {
        if !expect_ref(value).is_some_and(|marker| marker.starts_with("map-entry/")) {
            return Ok(None);
        }
        let (key, entry_value) = map_entry_parts(value)?;
        let key = self.value_of_text(&key, "Ljava/lang/Object;", class, method)?;
        let entry_value = self.value_of_text(&entry_value, "Ljava/lang/Object;", class, method)?;
        Ok(Some(Value::Ref(Some(format!("{key}={entry_value}")))))
    }
}
