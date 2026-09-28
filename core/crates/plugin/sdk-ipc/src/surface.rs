use std::collections::{BTreeMap, BTreeSet};

use operit_link::CoreLinkError;

/// Defines the explicitly exported Core Link routes available to external SDK clients.
#[derive(Clone, Debug, Default)]
pub struct PluginSdkSurface {
    routes: BTreeMap<String, BTreeSet<String>>,
}

impl PluginSdkSurface {
    /// Creates an empty SDK route surface.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one call, watch, or push method to the exported route surface.
    pub fn expose(&mut self, object_id: impl Into<String>, method_name: impl Into<String>) {
        self.routes
            .entry(object_id.into())
            .or_default()
            .insert(method_name.into());
    }

    /// Verifies that one incoming Core Link route was explicitly exported.
    pub fn check(&self, object_id: &str, method_name: &str) -> Result<(), CoreLinkError> {
        if self
            .routes
            .get(object_id)
            .is_some_and(|methods| methods.contains(method_name))
        {
            return Ok(());
        }
        Err(CoreLinkError::new(
            "PLUGIN_SDK_ROUTE_NOT_EXPOSED",
            format!("Plugin SDK route is not exposed: object={object_id}, method={method_name}"),
        ))
    }
}
