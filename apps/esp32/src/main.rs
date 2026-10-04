#![allow(non_snake_case)]

mod config;
mod edge_chat;
mod edge_image;
#[cfg(target_os = "espidf")]
mod edge_plugin;
#[cfg(target_os = "espidf")]
mod lvgl;
#[cfg(target_os = "espidf")]
mod settings;
mod status;
#[cfg(target_os = "espidf")]
mod ui_deploy;

#[cfg(target_os = "espidf")]
mod wifi;

/// Starts the ESP32-2432S028 Operit Edge firmware.
#[cfg(target_os = "espidf")]
fn main() {
    if let Err(error) = runFirmware(None) {
        log::error!("operit-esp32 failed: {}", error.message);
        panic!("operit-esp32 failed: {}", error.message);
    }
}

/// Stops non-ESP-IDF targets from launching the firmware binary.
#[cfg(not(target_os = "espidf"))]
fn main() {
    eprintln!("operit-esp32 requires the xtensa-esp32-espidf target");
    std::process::exit(1);
}

#[cfg(target_os = "espidf")]
fn runFirmware(
    nodeServices: Option<operit_node_runtime::NodeServices::NodeServices>,
) -> operit_host_api::HostResult<()> {
    use std::sync::Arc;

    use crate::config::Esp32FirmwareConfig;
    use crate::lvgl::{updateStatus, Esp32Lvgl};
    use crate::settings::{Esp32SettingsStore, Esp32SetupServer};
    use crate::status::FirmwareStatus;
    use crate::wifi::Esp32Wifi;
    use esp_idf_hal::delay::FreeRtos;
    use esp_idf_hal::peripherals::Peripherals;
    use esp_idf_svc::nvs::EspDefaultNvsPartition;
    use operit_board_esp32::{Esp32Board, INITIAL_EXPRESSION, LED_GREEN_PIN, LED_RED_PIN};
    use operit_host_api::{HostError, RobotFaceHost};

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!(
        "operit-esp32 stability diagnostics v1; reset_reason={}",
        unsafe { esp_idf_svc::sys::esp_reset_reason() }
    );
    logRuntimeHealth("boot");

    let peripherals = Peripherals::take().map_err(|error| HostError::new(error.to_string()))?;
    let modem = peripherals.modem;
    let nvsPartition =
        EspDefaultNvsPartition::take().map_err(|error| HostError::new(format!("nvs: {error}")))?;
    let settingsStore = Esp32SettingsStore::new(nvsPartition.clone())?;
    let settings = settingsStore.load()?;
    let config = Esp32FirmwareConfig::fromSettings(&settings);
    let mut board = Esp32Board::new(
        peripherals.spi2,
        peripherals.pins.gpio2,
        peripherals.pins.gpio12,
        peripherals.pins.gpio13,
        peripherals.pins.gpio14,
        peripherals.pins.gpio15,
        peripherals.pins.gpio21,
        peripherals.pins.gpio4,
        peripherals.pins.gpio16,
        peripherals.pins.gpio17,
        peripherals.spi3,
        peripherals.pins.gpio25,
        peripherals.pins.gpio32,
        peripherals.pins.gpio39,
        peripherals.pins.gpio33,
        peripherals.pins.gpio36,
    )?;
    let faceHost = board.robotFaceHost();
    board.activateLvgl();
    // The physical TFT is the only display sink in the firmware. Release the
    // optional diagnostic framebuffer so pairing and chat retain heap headroom.
    board.screenMirror().disablePixelMirror();
    let mut lvgl = Esp32Lvgl::new(&board)?;
    let scheduler =
        Arc::new(operit_host_native_scheduler::LocalHostRuntimeTaskSchedulerHost::new()?);
    operit_host_api::HostManager::setDefaultHostRuntimeTaskSchedulerHost(scheduler.clone());
    let hostManager = board
        .installIntoHostManager()
        .withHostRuntimeTaskSchedulerHost(scheduler);
    let status = Arc::new(FirmwareStatus::new(INITIAL_EXPRESSION));
    let mut edgeNode = operit_node_edge::EdgeNode::fromHostManager(hostManager.clone())
        .withPlugin(Arc::new(edge_plugin::DeviceStatusPlugin::new(Arc::clone(
            &status,
        ))))
        .map_err(|error| HostError::new(error.message))?;
    if let Some(services) = nodeServices {
        edgeNode = edgeNode.withNodeServices(services);
    }
    let edgeNode = Arc::new(edgeNode);
    // 这里只驱动外层异步方法，不在固件中实现传输或配对状态机。
    let nodeExecutor = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|error| HostError::new(error.to_string()))?;
    let pairingState = || -> Result<(bool, String), String> {
        let services = edgeNode.nodeServices().map_err(|error| error.message)?;
        let paired = !services.peers()
            .pairedPeers()
            .map_err(|error| error.to_string())?
            .is_empty();
        let codes = services.peers()
            .pairingPrompts()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|prompt| format!("{}: {}", prompt.displayName, prompt.confirmationCode))
            .collect::<Vec<_>>()
            .join("\n");
        Ok((paired, codes))
    };
    let setExpression = |expression: &str| -> operit_host_api::HostResult<()> {
        let state = faceHost.setExpression(operit_host_api::RobotFaceExpressionRequest {
            expression: expression.to_string(),
        })?;
        status.setExpression(state.expression);
        Ok(())
    };
    let deviceIo = hostManager
        .deviceIoHost
        .as_ref()
        .ok_or_else(|| HostError::new("Board digital I/O is unavailable"))?;
    setExpression("booting")?;
    // Show the launcher even if the configured network is unavailable.
    lvgl.pump(1);
    let (_wifi, wifiMode) = Esp32Wifi::connectOrSetup(modem, &config, nvsPartition.clone())?;
    logRuntimeHealth("wifi-ready");
    let _sntp = if wifiMode == crate::wifi::Esp32WifiMode::Station {
        Some(Esp32Wifi::startTimeSync()?)
    } else {
        None
    };
    logRuntimeHealth("sntp-ready");
    let _setupServer = match wifiMode {
        crate::wifi::Esp32WifiMode::Station => {
            let ip = _wifi.ipv4()?;
            status.setWifiSsid(config.wifiSsid.clone());
            status.setIpv4(ip.to_string());
            setExpression("online")?;
            deviceIo.setDigitalOutput(operit_host_api::DeviceDigitalOutputRequest {
                pin: LED_GREEN_PIN,
                level: true,
            })?;
            log::info!("operit-esp32 online at http://{ip}/");
            None
        }
        crate::wifi::Esp32WifiMode::SetupAccessPoint => {
            status.setWifiSsid("Operit-ESP32-Setup");
            status.setIpv4("192.168.4.1");
            setExpression("error")?;
            deviceIo.setDigitalOutput(operit_host_api::DeviceDigitalOutputRequest {
                pin: LED_RED_PIN,
                level: true,
            })?;
            log::info!("operit-esp32 setup AP ready: Operit-ESP32-Setup / http://192.168.4.1/");
            Some(Esp32SetupServer::start(
                Arc::clone(&status),
                Arc::clone(&settingsStore),
                config.httpPort,
            )?)
        }
    };
    logRuntimeHealth("network-start-complete");

    // The listener being enabled is not the same as having a live Space
    // route. The display must start offline until Core has admitted the Edge.
    let paired = match pairingState() {
        Ok((paired, codes)) => {
            status.setPairingCode(codes);
            paired
        }
        Err(_) => false,
    };
    updateStatus(&mut lvgl, &status, crate::edge_chat::isConnected(), paired);
    lvgl.pump(1);
    let mut lastExpression = status.snapshot().expression;
    let mut lastStatusRevision = status.revision();
    let mut lastEdgeReady = crate::edge_chat::isConnected();
    let mut lastPaired = paired;
    logRuntimeHealth("ready");
    let mut nextHealth = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut lastChatRevision = u32::MAX;
    let mut lastChatConnected = false;
    loop {
        let point = match board.pollTouch() {
            Ok(point) => point,
            Err(error) => {
                log::warn!("operit-esp32 touch: {}", error.message);
                None
            }
        };
        // The shared LVGL runtime owns drawer gestures; do not intercept Back here.
        lvgl.setTouch(point.map(|sample| (sample.x, sample.y)));
        lvgl.pump(20);
        for action in lvgl.drainActions() {
            match action.as_str() {
                "face_online" => {
                    setExpression("online")?;
                }
                "face_neutral" => {
                    setExpression("neutral")?;
                }
                "run_node" | "edge_search" | "edge_pair" => {
                    let result = nodeExecutor.block_on(async {
                        let services = edgeNode.nodeServices().map_err(|error| error.message)?;
                        services.peers()
                            .startListening(&[operit_node_runtime::NodeServices::PeerTransport::Tcp])
                            .await
                            .map_err(|error| error.to_string())
                    });
                    match result {
                        Ok(()) => {
                            setExpression("listening")?;
                        }
                        Err(error) => lvgl.actionError(&error),
                    }
                }
                "edge_unpair" => {
                    let result = nodeExecutor.block_on(async {
                        let services = edgeNode.nodeServices().map_err(|error| error.message)?;
                        for peer in services.peers().pairedPeers().map_err(|error| error.to_string())? {
                            services.peers()
                                .removePairedPeer(&peer.nodeId)
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                        Ok::<_, String>(())
                    });
                    match result {
                        Ok(()) => {
                            crate::edge_chat::clear();
                            status.setPairingCode("");
                            setExpression("neutral")?;
                        }
                        Err(error) => lvgl.actionError(&error),
                    }
                }
                "edge_chat" => {}
                "edge_new" => {
                    if let Err(error) = crate::edge_chat::newChat() {
                        lvgl.actionError(&error);
                    }
                }
                action if action.starts_with("edge_select:") => {
                    if let Err(error) = crate::edge_chat::selectChat(&action[12..]) {
                        lvgl.actionError(&error);
                    }
                }
                "edge_send" => {
                    let draft = lvgl.chatDraft();
                    if let Err(error) = crate::edge_chat::send(draft) {
                        log::warn!("operit-esp32 chat send: {error}");
                        lvgl.chatSendResult(Err(error));
                    }
                }
                "edge_image_cancel" => crate::edge_image::cancel(),
                action if action.starts_with("edge_image:") => {
                    if let Err(error) = crate::edge_chat::openImage(&action[11..]) {
                        lvgl.imageError(&error);
                    }
                }
                _ => log::debug!("operit-esp32 LVGL action: {action}"),
            }
        }
        if let Some(result) = crate::edge_chat::takeSendResult() {
            lvgl.chatSendResult(result);
        }
        if let Some(event) = crate::edge_image::take() {
            lvgl.imageEvent(event);
        }
        if let Ok(face) = faceHost.getExpression() {
            if face.expression != lastExpression {
                lastExpression = face.expression.clone();
                status.setExpression(face.expression);
            }
        }
        // Poll first so a newly accepted Core carrier is reflected on the
        // display in the same loop iteration.
        let edgeReady = crate::edge_chat::isConnected();
        let paired = match pairingState() {
            Ok((paired, codes)) => {
                status.setPairingCode(codes);
                paired
            }
            Err(_) => {
                status.setPairingCode("");
                false
            }
        };
        let statusRevision = status.revision();
        if statusRevision != lastStatusRevision
            || edgeReady != lastEdgeReady
            || paired != lastPaired
        {
            updateStatus(&mut lvgl, &status, edgeReady, paired);
            lastStatusRevision = statusRevision;
            lastEdgeReady = edgeReady;
            lastPaired = paired;
        }
        let now = std::time::Instant::now();
        let chatRevision = crate::edge_chat::revision();
        let chatConnected = crate::edge_chat::isConnected();
        if chatRevision != lastChatRevision || chatConnected != lastChatConnected {
            let chatState = crate::edge_chat::snapshot();
            lvgl.setChatState(&chatState);
            lvgl.setChatScreen(&crate::edge_chat::screenTextFromSnapshot(&chatState));
            lvgl.setChatTask(&crate::edge_chat::taskStatus());
            lastChatRevision = chatRevision;
            lastChatConnected = chatConnected;
        }
        if now >= nextHealth {
            logRuntimeHealth("running");
            nextHealth = std::time::Instant::now() + std::time::Duration::from_secs(30);
        }
        FreeRtos::delay_ms(1);
    }
}

/// Fixed-size diagnostics: no history buffer or framebuffer copies.
#[cfg(target_os = "espidf")]
pub(crate) fn logRuntimeHealth(stage: &str) {
    use esp_idf_svc::sys;
    unsafe {
        log::info!(
            "health {stage}: heap_free={} heap_min={} largest_8bit={} main_stack_free={}",
            sys::esp_get_free_heap_size(),
            sys::esp_get_minimum_free_heap_size(),
            sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_8BIT),
            sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()),
        );
    }
}
