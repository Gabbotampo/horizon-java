//! A linked jar's resources, found by `Class.getResourceAsStream` and
//! `getResource` the way a JVM class path finds them.
//!
//! A JVM's class loader looks a resource up across every jar on the class
//! path, the running jar first and then the others in order, and the first jar
//! that holds the name answers. The host used to answer only the running jar's
//! own resources, so a linked library reading its bundled defaults got `null`:
//! Forge Config API Port's `ForgeConfigApiPortConfig.loadFrom` does
//! `Objects.requireNonNull(getClass().getResourceAsStream("/forgeconfigapiport.toml"))`
//! and failed with an NPE in every Fabric jar that links it.
//!
//! The linked table keeps each linked jar's resources (the bytes `NativeJar`
//! already bounds: 4 MiB a resource, 32 MiB a jar) under the first jar that
//! names them, as it keeps classes. The running jar's own resources still
//! answer first.

use super::*;

impl LinkedClassTable {
    /// Keep `jar`'s resources whose names no earlier jar in the table took.
    pub(crate) fn link_resources(&mut self, jar: &NativeJar) {
        for (name, bytes) in &jar.resource_bytes {
            if name.ends_with(".class") {
                continue;
            }
            self.resources
                .entry(name.clone())
                .or_insert_with(|| (format!("resource-bytes/{name}"), bytes.clone()));
        }
    }

    /// The marker and bytes of a resource a linked jar holds.
    pub(crate) fn resource(&self, name: &str) -> Option<(&str, &[u8])> {
        self.resources
            .get(name)
            .map(|(marker, bytes)| (marker.as_str(), bytes.as_slice()))
    }
}
