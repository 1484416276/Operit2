#![allow(non_snake_case)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use operit_board_esp32::ESP32_2432S028_BOARD_ID;

/// Snapshot published by the firmware status endpoint.
#[derive(Clone, Debug)]
pub struct FirmwareStatusSnapshot {
    pub boardId: String,
    pub expression: String,
    pub ipv4: String,
    pub wifiSsid: String,
    pub pairingCode: String,
}

/// Shared firmware status consumed by the setup and status endpoints.
pub struct FirmwareStatus {
    boardId: String,
    revision: AtomicU32,
    expression: Mutex<String>,
    ipv4: Mutex<String>,
    wifiSsid: Mutex<String>,
    pairingCode: Mutex<String>,
}

impl FirmwareStatus {
    /// Creates firmware status with no network address yet.
    pub fn new(expression: impl Into<String>) -> Self {
        Self {
            boardId: ESP32_2432S028_BOARD_ID.to_string(),
            revision: AtomicU32::new(0),
            expression: Mutex::new(expression.into()),
            ipv4: Mutex::new(String::new()),
            wifiSsid: Mutex::new(String::new()),
            pairingCode: Mutex::new(String::new()),
        }
    }

    /// Records the current robot face expression.
    pub fn setExpression(&self, expression: impl Into<String>) {
        if let Ok(mut current) = self.expression.lock() {
            *current = expression.into();
            self.revision.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Records the current station IPv4 address.
    pub fn setIpv4(&self, ipv4: impl Into<String>) {
        if let Ok(mut current) = self.ipv4.lock() {
            *current = ipv4.into();
            self.revision.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Records the associated Wi-Fi SSID without storing the password.
    pub fn setWifiSsid(&self, ssid: impl Into<String>) {
        if let Ok(mut current) = self.wifiSsid.lock() {
            *current = ssid.into();
            self.revision.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Publishes the one-time Edge pairing code while a pairing is pending.
    pub fn setPairingCode(&self, code: impl Into<String>) {
        if let Ok(mut current) = self.pairingCode.lock() {
            *current = code.into();
            self.revision.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Returns a monotonic version for consumers that only need to repaint on change.
    pub fn revision(&self) -> u32 {
        self.revision.load(Ordering::Relaxed)
    }

    /// Returns a copy of the values shown on the firmware home page.
    pub fn snapshot(&self) -> FirmwareStatusSnapshot {
        FirmwareStatusSnapshot {
            boardId: self.boardId.clone(),
            expression: self
                .expression
                .lock()
                .map(|value| value.clone())
                .unwrap_or_default(),
            ipv4: self
                .ipv4
                .lock()
                .map(|value| value.clone())
                .unwrap_or_default(),
            wifiSsid: self
                .wifiSsid
                .lock()
                .map(|value| value.clone())
                .unwrap_or_default(),
            pairingCode: self
                .pairingCode
                .lock()
                .map(|value| value.clone())
                .unwrap_or_default(),
        }
    }
}

/// Renders firmware status as JSON for machine clients.
pub fn renderStatusJson(snapshot: &FirmwareStatusSnapshot) -> String {
    format!(
        "{{\"boardId\":\"{}\",\"expression\":\"{}\",\"wifiSsid\":\"{}\",\"ipv4\":\"{}\",\"pairingCode\":\"{}\"}}",
        jsonEscape(&snapshot.boardId),
        jsonEscape(&snapshot.expression),
        jsonEscape(&snapshot.wifiSsid),
        jsonEscape(&snapshot.ipv4),
        jsonEscape(&snapshot.pairingCode)
    )
}

/// Escapes a JSON string value.
fn jsonEscape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
