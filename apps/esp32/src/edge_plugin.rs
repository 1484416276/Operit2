//! Firmware-owned native plugin actions exposed over authenticated PeerLink.
use std::sync::Arc;

use operit_link::CoreValue;
use operit_node_edge::{EdgePlugin, EdgePluginManifest, EdgeServiceError};

use crate::status::FirmwareStatus;

pub struct DeviceStatusPlugin {
    status: Arc<FirmwareStatus>,
}

impl DeviceStatusPlugin {
    pub fn new(status: Arc<FirmwareStatus>) -> Self {
        Self { status }
    }
}

impl EdgePlugin for DeviceStatusPlugin {
    fn manifest(&self) -> EdgePluginManifest {
        EdgePluginManifest {
            id: "device.status".into(),
            name: "Device status".into(),
            actions: vec!["read".into()],
        }
    }

    fn invoke(&self, action: &str, _args: CoreValue) -> Result<CoreValue, EdgeServiceError> {
        if action != "read" {
            return Err(EdgeServiceError::new("unsupported status action"));
        }
        let status = self.status.snapshot();
        Ok(CoreValue::Map(std::collections::BTreeMap::from([
            ("boardId".into(), CoreValue::String(status.boardId)),
            ("expression".into(), CoreValue::String(status.expression)),
            ("ipv4".into(), CoreValue::String(status.ipv4)),
            ("wifiSsid".into(), CoreValue::String(status.wifiSsid)),
        ])))
    }
}
