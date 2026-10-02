#![allow(non_snake_case)]
// Compile the actual firmware modules, including the same stream renderer.
#[path = "../../../../apps/esp32/src/edge_chat.rs"]
mod edge_chat;
#[path = "../../../../apps/esp32/src/edge_image.rs"]
mod edge_image;
use operit_node_runtime::NodeServices::{NodeServices, PeerTransport};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncBufReadExt, BufReader};

struct SimulatorStatusPlugin {
    address: String,
}

impl operit_node_edge::EdgePlugin for SimulatorStatusPlugin {
    fn manifest(&self) -> operit_node_edge::EdgePluginManifest {
        operit_node_edge::EdgePluginManifest {
            id: "device.status".into(),
            name: "Device status".into(),
            actions: vec!["read".into()],
        }
    }

    fn invoke(
        &self,
        action: &str,
        _args: operit_link::CoreValue,
    ) -> Result<operit_link::CoreValue, operit_node_edge::EdgeServiceError> {
        if action != "read" {
            return Err(operit_node_edge::EdgeServiceError::new(
                "unsupported status action",
            ));
        }
        Ok(operit_link::CoreValue::Map(
            std::collections::BTreeMap::from([
                (
                    "boardId".into(),
                    operit_link::CoreValue::String("ESP32-2432S028-SIM".into()),
                ),
                (
                    "expression".into(),
                    operit_link::CoreValue::String("online".into()),
                ),
                (
                    "ipv4".into(),
                    operit_link::CoreValue::String(self.address.clone()),
                ),
                (
                    "wifiSsid".into(),
                    operit_link::CoreValue::String("simulator".into()),
                ),
            ]),
        ))
    }
}

fn emit(value: serde_json::Value) {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{value}").expect("editor IPC closed");
    stdout.flush().expect("editor IPC closed");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(None).await
}

// 由启动装配注入核心；当前不提供虚假的网络或配对实现。
async fn run(nodeServices: Option<NodeServices>) -> Result<(), Box<dyn std::error::Error>> {
    let scheduler =
        Arc::new(operit_host_native_scheduler::LocalHostRuntimeTaskSchedulerHost::new()?);
    operit_host_api::HostManager::setDefaultHostRuntimeTaskSchedulerHost(scheduler.clone());
    let error = Arc::new(Mutex::new(String::new()));
    let lastAction = Arc::new(Mutex::new(String::new()));
    let address = std::env::var("OPERIT_SIM_BIND").unwrap_or_else(|_| "0.0.0.0:18765".into());
    let mut edgeNode = operit_node_edge::EdgeNode::fromHostManager(
        operit_host_api::HostManager::HostManager::default(),
    )
    .withPlugin(Arc::new(SimulatorStatusPlugin {
        address: address.clone(),
    }))
    .map_err(|error| error.message)?;
    if let Some(services) = nodeServices {
        edgeNode = edgeNode.withNodeServices(services);
    }
    emit(
        serde_json::json!({"ready": true, "peerServiceAvailable": edgeNode.nodeServices().is_ok()}),
    );
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let request: serde_json::Value = serde_json::from_str(&line)?;
        let id = request["id"].clone();
        let services = edgeNode.nodeServices();
        let paired = services
            .as_ref()
            .ok()
            .and_then(|s| s.peers().pairedPeers().ok())
            .is_some_and(|p| !p.is_empty());
        let chat_state = edge_chat::snapshot();
        let chat_screen = edge_chat::screenText();
        let pairing_code = services
            .as_ref()
            .ok()
            .and_then(|s| s.peers().pairingPrompts().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|p| format!("{}: {}", p.displayName, p.confirmationCode))
            .collect::<Vec<_>>()
            .join("\n");
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
                if action == "edge_pair" || action == "edge_unpair" {
                    let result = async {
                        let services = edgeNode.nodeServices().map_err(|error| error.message)?;
                        if action == "edge_pair" {
                            services.peers()
                                .startListening(&[PeerTransport::Tcp])
                                .await
                                .map_err(|e| e.to_string())?;
                        } else {
                            for peer in services.peers().pairedPeers().map_err(|e| e.to_string())? {
                                services.peers()
                                    .removePairedPeer(&peer.nodeId)
                                    .await
                                    .map_err(|e| e.to_string())?;
                            }
                            edge_chat::clear();
                        }
                        Ok::<_, String>(())
                    }
                    .await;
                    if let Err(error) = result {
                        emit(serde_json::json!({"id":id,"error":error}));
                        continue;
                    }
                } else if action == "edge_new" {
                    if let Err(error) = edge_chat::newChat() {
                        emit(serde_json::json!({"id":id, "error":error}));
                        continue;
                    }
                } else if let Some(chatId) = action.strip_prefix("edge_select:") {
                    if let Err(error) = edge_chat::selectChat(chatId) {
                        emit(serde_json::json!({"id":id, "error":error}));
                        continue;
                    }
                } else if action == "edge_image_cancel" {
                    edge_image::cancel();
                } else if let Some(input) = action.strip_prefix("edge_image:") {
                    if let Err(error) = edge_chat::openImage(input) {
                        emit(serde_json::json!({"id":id, "error":error}));
                        continue;
                    }
                } else if action != "edge_pair" && action != "edge_chat" {
                    emit(serde_json::json!({"id":id, "error":"Unknown device action"}));
                    continue;
                }
                Ok(serde_json::json!({"ok": true, "action": action}))
            }
            Some("image") => Ok(match edge_image::take() {
                Some(event) => match event.chunk {
                    Ok(chunk) => {
                        serde_json::json!({"request":event.request,"width":chunk.width,"height":chunk.height,
                        "offset":chunk.offset,"bytes":chunk.bytes})
                    }
                    Err(error) => serde_json::json!({"request":event.request,"error":error}),
                },
                None => serde_json::Value::Null,
            }),
            Some("sendImage") => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(request["bytes"].as_str().unwrap_or(""))
                    .map_err(|_| "Invalid image bytes")?;
                edge_chat::sendImage(bytes, request["mimeType"].as_str().unwrap_or("").into())
                    .map(|_| serde_json::json!({"accepted":true}))
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
    if let Ok(services) = edgeNode.nodeServices() {
        services.peers().stop().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
