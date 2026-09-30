#![allow(non_snake_case)]

use std::sync::Arc;

use esp_idf_hal::uart::UartDriver;
use esp_idf_svc::mdns::EspMdns;
use operit_peer_link::{PairingAuthority, PairingStore};
use operit_peer_link::transport::LinkChannel;
use operit_host_api::{HostError, HostResult, TcpHost};
use operit_link::LinkDeviceInfo;
use operit_peer_link::transport::tcp::TcpLinkChannel;
use tokio::runtime::Builder;

use crate::edge_serial::Esp32UartLinkChannel;
use crate::status::FirmwareStatus;

/// Runs the authenticated standard-Link listener on a dedicated lightweight
/// thread so network processing can progress independently of LVGL redraws.
pub struct Esp32EdgeLinkServer {
    _runtimeThread: Option<std::thread::JoinHandle<()>>,
    _mdns: Option<EspMdns>,
    authority: Arc<PairingAuthority>,
}

impl Esp32EdgeLinkServer {
    pub fn start(
        port: u16,
        tcpHost: Arc<dyn TcpHost>,
        token: String,
        status: Arc<FirmwareStatus>,
        store: Arc<dyn PairingStore>,
        edgeNode: Arc<operit_node_edge::EdgeNode>,
        uart: Option<UartDriver<'static>>,
    ) -> HostResult<Option<Self>> {
        if token.trim().is_empty() {
            log::warn!("Edge Link disabled: OPERIT_EDGE_TOKEN is not configured");
            return Ok(None);
        }
        let tokenHash = operit_peer_link::linkTokenHash(&token);
        crate::logRuntimeHealth("edge-runtime-ready");
        let authority = match PairingAuthority::newWithStore(
            token,
            "esp32-edge".to_string(),
            LinkDeviceInfo {
                platform: "esp32".to_string(),
                model: "ESP32-2432S028".to_string(),
            },
            store,
            {
                let status = Arc::clone(&status);
                move |code| {
                    status.setPairingCode(code.clone());
                    log::info!("Edge Link pairing code: {code}");
                }
            },
        ) {
            Ok(authority) => Arc::new(authority),
            Err(error) => {
                return Err(HostError::new(format!(
                    "Edge Link persistent store: {error}"
                )))
            }
        };
        crate::logRuntimeHealth("edge-authority-ready");
        let serialChannel = match uart {
            Some(uart) => match Esp32UartLinkChannel::new(uart) {
                Ok(channel) => {
                    log::info!("Edge Link listening on UART0 GPIO1/GPIO3 at 115200 baud");
                    Some(channel)
                }
                Err(error) => {
                    log::error!("Edge UART listener: {}", error.message);
                    None
                }
            },
            None => None,
        };
        let runtime = Builder::new_current_thread().enable_time().build()
            .map_err(|error| HostError::new(format!("Link runtime: {error}")))?;
        let listener = runtime.block_on(tcpHost.bind(&format!("0.0.0.0:{port}")))?;
        log::info!("Edge Link listening on TCP port {port}");
        crate::logRuntimeHealth("edge-before-mdns");
        let mdns = match EspMdns::take() {
            Ok(mut mdns) => {
                let txt = [
                    ("deviceId", "esp32-edge"),
                    ("displayName", "ESP32 Edge"),
                    ("platform", "esp32"),
                    ("model", "ESP32-2432S028"),
                    ("tokenHash", tokenHash.as_str()),
                    ("version", "edge-1"),
                ];
                // ESP-IDF requires a hostname before registering services.
                let registration = mdns.set_hostname("operit-edge-esp32").and_then(|()| {
                    mdns.add_service(
                        Some("operit-edge-esp32"),
                        "_operit-edge",
                        "_tcp",
                        port,
                        &txt,
                    )
                });
                match registration {
                    Ok(()) => {
                        log::info!("Edge mDNS discovery enabled (_operit-edge._tcp)");
                        Some(mdns)
                    }
                    Err(error) => {
                        log::warn!("Edge mDNS registration failed: {error}");
                        None
                    }
                }
            }
            Err(error) => {
                log::warn!("Edge mDNS initialization failed: {error}");
                None
            }
        };
        crate::logRuntimeHealth("edge-after-mdns");
        let workerAuthority = Arc::clone(&authority);
        let runtimeThread = std::thread::Builder::new()
            .name("operit-edge-link".to_string())
            // Keep the stack large enough for Link MessagePack/X25519 work,
            // while leaving the remaining internal heap for dynamic frame
            // decoding and pairing state.
            .stack_size(16 * 1024)
            .spawn(move || {
                runtime.block_on(async move {
                    crate::logRuntimeHealth("edge-worker-ready");
                    let mut serialStarted = false;
                    // One established session plus one pairing/reconnect candidate.
                    let mut tcpSessions = tokio::task::JoinSet::new();
                    loop {
                        if !serialStarted {
                            serialStarted = true;
                            if let Some(channel) = serialChannel.clone() {
                                let authority = Arc::clone(&workerAuthority);
                                let serialNode = Arc::clone(&edgeNode);
                                tokio::spawn(async move {
                                    loop {
                                        match crate::edge_session::handleChannelWithNode(Arc::clone(&authority), channel.clone(), Arc::clone(&serialNode)).await {
                                            Ok(()) => {}
                                            Err(error) => {
                                                log::warn!("Edge UART session: {error}");
                                                if channel.isClosed() {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                        }
                        while let Some(result) = tcpSessions.try_join_next() {
                            if let Err(error) = result {
                                log::warn!("Edge Link task: {error}");
                            }
                        }
                        match listener.accept().await {
                            Ok(connection) => {
                                // Sessions may have ended while accept was pending.
                                while let Some(result) = tcpSessions.try_join_next() {
                                    if let Err(error) = result {
                                        log::warn!("Edge Link task: {error}");
                                    }
                                }
                                if tcpSessions.len() >= 2 {
                                    log::warn!("Edge Link connection limit reached");
                                    connection.close().await;
                                    continue;
                                }
                                let channel = TcpLinkChannel::withReceiveLimit(connection, 48 * 1024);
                                let authority = Arc::clone(&workerAuthority);
                                let node = Arc::clone(&edgeNode);
                                tcpSessions.spawn(async move {
                                    if let Err(error) = crate::edge_session::handleChannelWithNode(
                                        authority, channel, node,
                                    ).await {
                                        log::warn!("Edge Link session: {error}");
                                    }
                                });
                            }
                            Err(error) => {
                                log::error!("Link listener stopped: {error}");
                                listener.close().await;
                                break;
                            }
                        }
                    }
                });
            })
            .map_err(|error| HostError::new(format!("Edge Link thread: {error}")))?;
        Ok(Some(Self {
            _runtimeThread: Some(runtimeThread),
            _mdns: mdns,
            authority,
        }))
    }

    pub fn hasPairings(&self) -> bool {
        self.authority.hasPairings()
    }

    /// Clears all persisted Edge pairings without changing Wi-Fi or the token.
    pub fn clearPairings(&self) -> Result<(), String> {
        self.authority.clearPairings()
    }

    /// The listener is continuously driven by the dedicated runtime thread.
    pub fn poll(&mut self) {}
}
