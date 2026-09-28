#![allow(non_snake_case)]
// Compile the actual firmware modules, including the same stream renderer.
#[path = "../../../../apps/esp32/src/edge_chat.rs"]
mod edge_chat;
#[path = "../../../../apps/esp32/src/edge_session.rs"]
mod edge_session;
#[cfg(test)]
mod tests;

use operit_edge_transport::{
    linkTokenHash, tcp::TcpLinkChannel, EdgePairingAuthority, EdgePairingPersistentState,
    EdgePairingStore,
};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncBufReadExt, BufReader};

struct FileStore(PathBuf);
impl EdgePairingStore for FileStore {
    fn load(&self) -> Result<Option<EdgePairingPersistentState>, String> {
        match std::fs::read(&self.0) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn save(&self, state: &EdgePairingPersistentState) -> Result<(), String> {
        let bytes = serde_json::to_vec(state).map_err(|e| e.to_string())?;
        let temporary = self.0.with_extension("tmp");
        std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(temporary, &self.0).map_err(|e| e.to_string())
    }
}

fn emit(value: serde_json::Value) {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{value}").expect("editor IPC closed");
    stdout.flush().expect("editor IPC closed");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = std::env::var("OPERIT_SIM_TOKEN")?;
    if token.is_empty() {
        return Err("Simulator token is empty".into());
    }
    let statePath = PathBuf::from(std::env::var("OPERIT_SIM_STATE")?);
    let tokenHash = linkTokenHash(&token);
    let code = Arc::new(Mutex::new(String::new()));
    let error = Arc::new(Mutex::new(String::new()));
    let lastAction = Arc::new(Mutex::new(String::new()));
    let authority = Arc::new(EdgePairingAuthority::newWithStore(
        token,
        "esp32-edge-simulator",
        operit_link::LinkDeviceInfo {
            platform: "esp32".into(),
            model: "ESP32-2432S028".into(),
        },
        Arc::new(FileStore(statePath)),
        {
            let code = code.clone();
            move |value| {
                *code.lock().unwrap() = value;
            }
        },
    )?);
    // Match a normal LAN node: the editor IPC remains local, while the Edge
    // Link carrier is reachable by a Core on the same network by default.
    let address = std::env::var("OPERIT_SIM_BIND").unwrap_or_else(|_| "0.0.0.0:18765".into());
    let listener = TcpLinkChannel::bind(&address).await?;
    let address = listener.local_addr()?.to_string();
    // Advertise the raw TCP Edge listener separately from Core's HTTP service.
    // mDNS is discovery only; the token hash is not a substitute for pairing.
    let mdns = {
        let daemon = ServiceDaemon::new()?;
        let port = listener.local_addr()?.port();
        let pid = std::process::id();
        let serviceType = "_operit-edge._tcp.local.";
        let instance = format!("operit-edge-simulator-{pid}");
        let hostname = format!("operit-edge-simulator-{pid}.local.");
        let mut properties = HashMap::new();
        properties.insert("deviceId".to_string(), "esp32-edge-simulator".to_string());
        properties.insert("displayName".to_string(), "ESP32 Simulator".to_string());
        properties.insert("platform".to_string(), "esp32".to_string());
        properties.insert("model".to_string(), "ESP32-2432S028".to_string());
        properties.insert("tokenHash".to_string(), tokenHash);
        properties.insert("version".to_string(), "edge-1".to_string());
        let info = ServiceInfo::new(serviceType, &instance, &hostname, "", port, properties)?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_string();
        daemon.register(info)?;
        Some((daemon, fullname))
    };
    let sessionError = error.clone();
    let sessionAuthority = Arc::clone(&authority);
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(value) => value,
                Err(e) => {
                    *sessionError.lock().unwrap() = e.to_string();
                    break;
                }
            };
            // A reconnect can arrive before the previous TCP task has observed
            // its close. Do not reject it: the real ESP32 listener has no
            // single-connection gate, and the authenticated session is owned
            // by the channel task below.
            let authority = sessionAuthority.clone();
            let error = sessionError.clone();
            tokio::spawn(async move {
                *error.lock().unwrap() = String::new();
                if let Err(e) =
                    edge_session::handleChannel(authority, TcpLinkChannel::fromStream(stream)).await
                {
                    *error.lock().unwrap() = e;
                }
            });
        }
    });
    emit(serde_json::json!({"ready": true, "address": address, "discovery": "_operit-edge._tcp.local."}));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let request: serde_json::Value = serde_json::from_str(&line)?;
        let id = request["id"].clone();
        let paired = authority.hasPairings();
        let chat_state = edge_chat::snapshot();
        let chat_screen = edge_chat::screenText();
        let pairing_code = code.lock().unwrap().clone();
        let result = match request["command"].as_str() {
            Some("state") => Ok(serde_json::json!({"address": address,
                "deviceId": "esp32-edge-simulator", "paired": paired,
                "pairingCode": pairing_code, "error": *error.lock().unwrap(),
                "lastAction": *lastAction.lock().unwrap(),
                "chat": chat_state, "chatPreview": edge_chat::preview(), "chatScreen": chat_screen,
                "chatTask": edge_chat::taskStatus(),
                "chatSendResult": edge_chat::takeSendResult().map(|result| match result {
                    Ok(()) => serde_json::json!({"ok": true}),
                    Err(error) => serde_json::json!({"ok": false, "error": error}),
                })})),
            Some("action") => {
                let action = request["action"].as_str().unwrap_or("");
                *lastAction.lock().unwrap() = action.to_string();
                if action == "edge_unpair" {
                    if let Err(error) = authority.clearPairings() {
                        emit(serde_json::json!({"id":id, "error":error}));
                        continue;
                    }
                    edge_chat::clear();
                    *code.lock().unwrap() = String::new();
                } else if action == "edge_new" {
                    if let Err(error) = edge_chat::newChat() {
                        emit(serde_json::json!({"id":id, "error":error})); continue;
                    }
                } else if let Some(chatId) = action.strip_prefix("edge_select:") {
                    if let Err(error) = edge_chat::selectChat(chatId) {
                        emit(serde_json::json!({"id":id, "error":error})); continue;
                    }
                } else if action != "edge_pair" && action != "edge_chat" {
                    emit(serde_json::json!({"id":id, "error":"Unknown device action"})); continue;
                }
                Ok(serde_json::json!({"ok": true, "action": action}))
            }
            Some("send") => edge_chat::send(request["text"].as_str().unwrap_or("").into())
                .map(|_| serde_json::json!({"ok": true})),
            _ => Err("Unknown simulator command".into()),
        };
        match result {
            Ok(value) => emit(serde_json::json!({"id": id, "value": value})),
            Err(error) => emit(serde_json::json!({"id": id, "error": error})),
        }
    }
    if let Some((daemon, fullname)) = mdns {
        let _ = daemon.unregister(&fullname);
    }
    Ok(())
}
