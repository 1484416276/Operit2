use super::network_control_ui::{
    network_capabilities, network_device_id, network_device_label_by_id, network_device_labels,
    network_role_id, network_role_summary, new_network_control_id,
};
use super::*;
use crate::{
    create_cli_core_application, create_cli_core_application_configured,
    create_cli_core_application_without_space_sync,
};

use operit_link::{
    CoreCallRequest, CoreEvent, CoreEventKind, CoreEventStream, CoreLinkSharedClient,
    CoreStreamDescriptor, CoreValue, CoreWatchRequest, CORE_STREAM_TARGET,
};
use operit_model::PromptTurn::PromptTurn;
use operit_providers::chat::enhance::ConversationService::ConversationService;
use operit_providers::chat::EnhancedAIService::EnhancedAIService;
use operit_runtime::core::chat::ChatRuntimeSlot::ChatRuntimeSlot;
use operit_runtime::services::RuntimeHostInteractionService::{
    requestChatToolPermissionAsync, RuntimeHostInteractionToolPermissionTool,
    RuntimeHostInteractionToolPermissionToolParameter,
};
use operit_store::CoreNodeBindingStore::CoreNodeBindingStore;
use operit_store::NetworkControlStore::{NetworkControlIdentityAssignment, NetworkControlRole};
use operit_tools::tools::AIToolHandler::AIToolHandler;
use operit_tools::tools::ToolPermissionSystem::PermissionRequestResult;
use operit_tools::ToolExecutionManager::AITool;
use operit_util::MarkdownRenderStream::MarkdownStreamEvent;
use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;

pub(crate) async fn run_link_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("pair-start" | "pair-finish" | "pair-cancel" | "unpair" | "peers" | "listen") => {
            run_node_pairing_command(args).await
        }
        Some("token") if args.len() == 2 && args[1] == "show" => {
            let application = create_cli_core_application_without_space_sync("client").await?;
            let token = application.localPairingToken()?;
            if cli_json_mode() { emit_cli_json(serde_json::json!({"token": token})); }
            else { println!("{token}"); }
            Ok(())
        }
        Some("token") => Err("usage: operit2 cli link token show".into()),
        Some("discover") => run_link_discover_command(&args[1..]).await,
        Some("space") => run_link_space_command(&args[1..]).await,
        Some("control") => run_link_control_command(&args[1..]).await,
        Some("stream-probe") => run_link_stream_probe_command(&args[1..]).await,
        Some("edge-plugin") => run_link_edge_plugin_command(&args[1..]).await,
        _ => {
            print_link_usage();
            Ok(())
        }
    }
}

/// CLI 只解析输入和展示结果；配对直接调用应用注入的 RuntimePeerService。
async fn run_node_pairing_command(args: &[String]) -> Result<(), String> {
    let application = if args.first().map(String::as_str) == Some("listen") {
        create_cli_core_application_configured("server", configure_link_core).await?
    } else {
        create_cli_core_application_without_space_sync("client").await?
    };
    execute_node_pairing_command(application.nodeServices()?.peers().as_ref(), args).await
}

async fn execute_node_pairing_command(
    services: &dyn operit_node_runtime::RuntimePeerService::RuntimePeerService,
    args: &[String],
) -> Result<(), String> {
    use operit_node_runtime::NodeServices::{PeerEndpoint, PeerTransport as Transport};
    let transport = |value: &str| match value {
        "http" => Ok(Transport::Http),
        "ws" => Ok(Transport::WebSocket),
        "tcp" => Ok(Transport::Tcp),
        "serial" => Ok(Transport::Serial),
        "bluetooth" => Ok(Transport::Bluetooth),
        _ => Err("transport must be http, ws, tcp, serial or bluetooth".to_string()),
    };
    let value = match args {
        [command, node, address, mode] if command == "pair-start" => {
            let pending = services.startPairing(
                PeerEndpoint { nodeId: node.clone(), address: address.clone() },
                transport(mode)?,
                None,
            ).await.map_err(|e| e.to_string())?;
            serde_json::to_value(pending).map_err(|e| e.to_string())?
        }
        [command, node, address, mode, flag, token] if command == "pair-start" && flag == "--token" => {
            let pending = services.startPairing(PeerEndpoint { nodeId: node.clone(), address: address.clone() }, transport(mode)?, Some(token)).await.map_err(|e| e.to_string())?;
            serde_json::to_value(pending).map_err(|e| e.to_string())?
        }
        [command, id, code] if command == "pair-finish" => {
            serde_json::to_value(services.finishPairing(id, code).await.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?
        }
        [command, id] if command == "pair-cancel" => {
            services.cancelPairing(id).await.map_err(|e| e.to_string())?;
            serde_json::json!({"cancelled": id})
        }
        [command, node] if command == "unpair" => {
            services.removePairedPeer(node).await.map_err(|e| e.to_string())?;
            serde_json::json!({"removed": node})
        }
        [command] if command == "peers" => serde_json::to_value(services.pairedPeers().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?,
        [command, mode] if command == "listen" => {
            services.startListening(transport(mode)?).await.map_err(|e| e.to_string())?;
            tokio::signal::ctrl_c().await.map_err(|e| e.to_string())?;
            services.stop().await.map_err(|e| e.to_string())?;
            serde_json::json!({"stopped": true})
        }
        _ => return Err("link pair-start <node-id> <address> <transport> [--token <token>] | pair-finish <id> <code> | pair-cancel <id> | unpair <node-id> | peers | listen <transport>".into()),
    };
    if cli_json_mode() {
        emit_cli_json(value);
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
        );
    }
    Ok(())
}

async fn run_link_edge_plugin_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: operit2 cli link edge-plugin <device> <list|invoke <plugin-id> <action> [json-args]>";
    let device = args.first().ok_or(USAGE)?;
    let coreApplication = create_cli_core_application("client").await?;
    let topology = coreApplication.accessServices().deviceSpaceTopology()?;
    let deviceId = network_device_id(&topology, device)?;
    let client =
        operit_node_runtime::NodeClient::NodeClient::new(coreApplication.nodeRouter(), deviceId);
    let mut proxy = operit_proxy_edge::EdgeProxy::new(client);
    let json: serde_json::Value = match args.get(1).map(String::as_str) {
        Some("list") if args.len() == 2 => serde_json::to_value(
            proxy
                .plugins()
                .list()
                .await
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?,
        Some("invoke") if (4..=5).contains(&args.len()) => {
            let value: serde_json::Value =
                serde_json::from_str(args.get(4).map(String::as_str).unwrap_or("{}"))
                    .map_err(|error| error.to_string())?;
            let value = operit_link::toCoreValue(value).map_err(|error| error.to_string())?;
            let result = proxy
                .plugins()
                .invoke(&args[2], &args[3], value)
                .await
                .map_err(|error| error.to_string())?;
            operit_link::fromCoreValue(result).map_err(|error| error.to_string())?
        }
        _ => return Err(USAGE.into()),
    };
    if cli_json_mode() {
        emit_cli_json(json);
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&json).map_err(|error| error.to_string())?
        );
    }
    Ok(())
}

/// Configures the local client before the CLI Link server shares it through the Core tree.
fn configure_link_core(core: &mut operit_proxy_local::LocalCoreProxy) -> Result<(), String> {
    {
        let application = core.localApplicationMut();
        let enhanced_ai_service = EnhancedAIService::new(
            application.toolHandler.clone(),
            application.providerRuntimeContext.clone(),
        );
        let mut holder = application
            .chatRuntimeHolder
            .try_lock()
            .map_err(|_| "Chat runtime holder is busy".to_string())?;
        holder.getCore(ChatRuntimeSlot::MAIN).enhancedAiService = Some(enhanced_ai_service);
    }
    install_link_permission_requester(core);
    Ok(())
}

/// Installs the owner permission requester used by Link server tool calls.
pub(crate) fn install_link_permission_requester(core: &mut operit_proxy_local::LocalCoreProxy) {
    let handler = core.localApplicationMut().toolHandler.clone();
    handler
        .getToolPermissionSystem()
        .setAsyncPermissionRequester(move |tool, description, chatId| async move {
            let Some(chatId) = chatId else {
                return PermissionRequestResult::DENY;
            };
            let response = requestChatToolPermissionAsync(
                chatId,
                tool_to_permission_payload(&tool),
                description,
                Duration::from_secs(60),
            )
            .await
            .expect("permission request failed");
            match response.as_str() {
                "allow" => PermissionRequestResult::ALLOW,
                "allow_session" => PermissionRequestResult::ALLOW_SESSION,
                "deny" => PermissionRequestResult::DENY,
                other => panic!("unknown permission response result: {other}"),
            }
        });
}

fn tool_to_permission_payload(tool: &AITool) -> RuntimeHostInteractionToolPermissionTool {
    RuntimeHostInteractionToolPermissionTool {
        name: tool.name.clone(),
        parameters: tool
            .parameters
            .iter()
            .map(
                |parameter| RuntimeHostInteractionToolPermissionToolParameter {
                    name: parameter.name.clone(),
                    value: parameter.value.clone(),
                },
            )
            .collect(),
    }
}

/// 通过共享节点服务发现局域网设备；不要求手输 token，不直接调用旧发现或会话客户端。
async fn run_link_discover_command(args: &[String]) -> Result<(), String> {
    let mut timeout_ms = 2000_u64;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--timeout-ms" => {
                index += 1;
                timeout_ms = args
                    .get(index)
                    .ok_or_else(|| {
                        "usage: operit2 cli link discover [--timeout-ms <ms>]".to_string()
                    })?
                    .parse::<u64>()
                    .map_err(|error| error.to_string())?;
            }
            _ => {
                return Err("usage: operit2 cli link discover [--timeout-ms <ms>]".to_string());
            }
        }
        index += 1;
    }
    let coreApplication = create_cli_core_application_without_space_sync("client").await?;
    let peers = coreApplication.accessServices().discoverPeers(timeout_ms).await
        .map_err(|error| error.to_string())?;
    if cli_json_mode() {
        emit_cli_json(serde_json::json!({ "peers": peers }));
    } else {
        for peer in peers { println!("{} — {} ({})", peer.displayName, peer.address, peer.nodeId); }
    }
    Ok(())
}

/// Runs user-facing device-space inspection and membership commands.
async fn run_link_space_command(args: &[String]) -> Result<(), String> {
    let ownsSpaceMutation = matches!(
        args,
        [command, ..] if matches!(command.as_str(), "rename" | "disconnect" | "remove" | "join" | "leave")
    );
    let coreApplication = if ownsSpaceMutation {
        create_cli_core_application("client").await?
    } else {
        create_cli_core_application_without_space_sync("client").await?
    };
    let service = coreApplication.accessServices();
    match args.first().map(String::as_str) {
        None | Some("show") if args.len() <= 1 => {
            let space = service.deviceSpace()?;
            if cli_json_mode() {
                emit_cli_json(serde_json::to_value(&space).map_err(|error| error.to_string())?);
            } else {
                println!("Device space: {}", space.spaceName);
                println!("Devices: {}", space.members.len());
            }
            Ok(())
        }
        Some("rename") if args.len() == 2 => {
            let space = service.renameDeviceSpace(args[1].clone())?;
            if cli_json_mode() { emit_cli_json(serde_json::json!(space)); }
            else { println!("Device space renamed to {}", space.spaceName); }
            Ok(())
        }
        Some("status") if args.len() == 2 => {
            let topology = service.deviceSpaceTopology()?;
            let device_id = network_device_id(&topology, &args[1])?;
            let device_label = network_device_label_by_id(&topology, &device_id)?;
            let status = service.pairedDeviceStatus(device_id).await?;
            if cli_json_mode() { emit_cli_json(serde_json::json!({ "status": format!("{status:?}") })); }
            else { println!("{device_label}: {status:?}"); }
            Ok(())
        }
        Some("disconnect") if args.len() == 2 => {
            let topology = service.deviceSpaceTopology()?;
            let device_id = network_device_id(&topology, &args[1])?;
            let device_label = network_device_label_by_id(&topology, &device_id)?;
            service.disconnectDeviceSpaceConnection(device_id).await?;
            if cli_json_mode() { emit_cli_json(serde_json::json!({ "disconnected": true })); }
            else { println!("Disconnected device {device_label}"); }
            Ok(())
        }
        Some("remove") if args.len() == 2 => {
            let topology = service.deviceSpaceTopology()?;
            let device_id = network_device_id(&topology, &args[1])?;
            let device_label = network_device_label_by_id(&topology, &device_id)?;
            service.removeDeviceSpaceMember(device_id).await?;
            if cli_json_mode() { emit_cli_json(serde_json::json!({ "removed": true })); }
            else { println!("Removed device {device_label} from the Space"); }
            Ok(())
        }
        Some("join") if args.len() == 2 => {
            let space = service.joinPairedDeviceSpace(args[1].clone()).await?;
            if cli_json_mode() { emit_cli_json(serde_json::json!(space)); }
            else { println!("Joined device space {} ({} devices)", space.spaceName, space.members.len()); }
            Ok(())
        }
        Some("leave") if args.len() == 1 => {
            let space = service.leaveDeviceSpace()?;
            if cli_json_mode() { emit_cli_json(serde_json::json!(space)); }
            else { println!("Left device space; current space: {}", space.spaceName); }
            Ok(())
        }
        _ => Err("usage: operit2 cli link space <show|status <device-name>|rename <name>|join <node-id>|disconnect <device-name>|remove <device-name>|leave>".to_string()),
    }
}

/// Runs authoritative Space control policy commands through the shared runtime service.
async fn run_link_control_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str =
        "usage: operit2 cli link control <show|bootstrap|audit|identity|device|policy>";
    let mutatesPolicy = matches!(
        args.first().map(String::as_str),
        Some("bootstrap") | Some("identity") | Some("device") | Some("policy")
    );
    let coreApplication = if mutatesPolicy {
        create_cli_core_application("client").await?
    } else {
        create_cli_core_application_without_space_sync("client").await?
    };
    let service = coreApplication.accessServices();
    match args.first().map(String::as_str) {
        Some("show") if args.len() == 1 => {
            let state = service.deviceSpaceControl()?;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!(state));
            } else {
                println!(
                    "Network control: {} · {} identities · {} devices",
                    if state.initialized {
                        "ready"
                    } else {
                        "not initialized"
                    },
                    state.roles.len(),
                    service.deviceSpaceTopology()?.devices.len(),
                );
            }
            Ok(())
        }
        Some("bootstrap") if args.len() == 1 => {
            let state = service.bootstrapDeviceSpaceControl()?;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!(state));
            } else {
                println!("Network control initialized");
            }
            Ok(())
        }
        Some("audit") if args.len() == 1 => {
            let audit = service.deviceSpaceControlAudit()?;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!(audit));
            } else {
                for record in audit {
                    println!(
                        "{} · {}",
                        if record.accepted {
                            "accepted"
                        } else {
                            "rejected"
                        },
                        record.summary,
                    );
                }
            }
            Ok(())
        }
        Some("identity") => {
            run_link_control_identity_command(&service, &coreApplication, &args[1..])
        }
        Some("device") => run_link_control_device_command(&service, &args[1..]).await,
        Some("policy") => run_link_control_policy_command(&service, &args[1..]),
        _ => Err(USAGE.to_string()),
    }
}

/// Runs identity definition, assignment, and revocation commands.
fn run_link_control_identity_command(
    service: &operit_node_runtime::RuntimeRemoteLinkService::RuntimeRemoteLinkService,
    coreApplication: &operit_core_application::CoreApplication,
    args: &[String],
) -> Result<(), String> {
    const USAGE: &str = "usage: operit2 cli link control identity <list|define <name> <all|audit|relay|storage|execute|network|view|manage-identities|assign-identity|approve>...|set <device-name> <identity-name>|clear <device-name>>";
    match args {
        [command] if command == "list" => {
            let state = service.deviceSpaceControl()?;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!(state.roles));
            } else {
                for role in state.roles.values() {
                    println!("{}", network_role_summary(role));
                }
            }
            Ok(())
        }
        [command, displayName, capabilities @ ..]
            if command == "define" && !capabilities.is_empty() =>
        {
            let roleId = new_network_control_id("role");
            let capabilities = network_capabilities(capabilities)?;
            service.defineDeviceSpaceRole(NetworkControlRole {
                roleId: roleId.clone(),
                displayName: displayName.clone(),
                capabilities,
            })?;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!({
                    "roleId": roleId,
                    "name": displayName,
                    "defined": true
                }));
            } else {
                println!("Identity created: {displayName}");
            }
            Ok(())
        }
        [command, deviceName, roleName] if command == "set" => {
            let state = service.deviceSpaceControl()?;
            let topology = service.deviceSpaceTopology()?;
            let deviceId = network_device_id(&topology, deviceName)?;
            let roleId = network_role_id(&state, roleName)?;
            let roleLabel = state
                .roles
                .get(&roleId)
                .map(|role| role.displayName.clone())
                .ok_or_else(|| format!("network role does not exist: {roleName}"))?;
            service.setDeviceSpaceIdentity(NetworkControlIdentityAssignment {
                nodeId: deviceId.clone(),
                roleId: roleId.clone(),
            })?;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!({
                    "deviceId": deviceId,
                    "identityId": roleId,
                    "set": true
                }));
            } else {
                println!("Set identity \"{roleLabel}\" on \"{deviceName}\"");
            }
            Ok(())
        }
        [command, deviceName] if command == "clear" => {
            let topology = service.deviceSpaceTopology()?;
            let deviceId = network_device_id(&topology, deviceName)?;
            service.clearDeviceSpaceIdentity(deviceId.clone())?;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!({ "deviceId": deviceId, "cleared": true }));
            } else {
                println!("Cleared identity from \"{deviceName}\"");
            }
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    }
}

/// Runs member removal and connection prohibition commands.
async fn run_link_control_device_command(
    service: &operit_node_runtime::RuntimeRemoteLinkService::RuntimeRemoteLinkService,
    args: &[String],
) -> Result<(), String> {
    const USAGE: &str = "usage: operit2 cli link control device <list|admit <device-name>|remove <device-name>|disconnect <device-name>>";
    match args {
        [command] if command == "list" => {
            let topology = service.deviceSpaceTopology()?;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!(topology.devices));
            } else {
                for label in network_device_labels(&topology).values() {
                    println!("{label}");
                }
            }
            Ok(())
        }
        [command, deviceName] if matches!(command.as_str(), "admit" | "remove" | "disconnect") => {
            let topology = service.deviceSpaceTopology()?;
            let deviceId = network_device_id(&topology, deviceName)?;
            let deviceLabel = network_device_label_by_id(&topology, &deviceId)?;
            if command == "admit" {
                service.admitDeviceSpaceMember(deviceId)?;
            } else if command == "remove" {
                service.removeDeviceSpaceMember(deviceId).await?;
            } else {
                service.disconnectDeviceSpaceNode(deviceId).await?;
            }
            if cli_json_mode() {
                emit_cli_json(serde_json::json!({ command: true }));
            } else {
                println!("{} device \"{}\"", command, deviceLabel);
            }
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    }
}

/// Updates one explicitly named policy value.
fn run_link_control_policy_command(
    service: &operit_node_runtime::RuntimeRemoteLinkService::RuntimeRemoteLinkService,
    args: &[String],
) -> Result<(), String> {
    const USAGE: &str = "usage: operit2 cli link control policy <list|set <policy-name> <value>>";
    match args {
        [command] if command == "list" => {
            let policies = service.deviceSpaceControl()?.policies;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!(policies));
            } else {
                for (name, value) in policies {
                    println!("{name}: {value}");
                }
            }
            Ok(())
        }
        [command, policyId, value] if command == "set" => {
            service.updateDeviceSpacePolicy(policyId.clone(), value.clone())?;
            if cli_json_mode() {
                emit_cli_json(serde_json::json!({ "policy": policyId, "updated": true }));
            } else {
                println!("Policy updated: {policyId}");
            }
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    }
}

/// 通过节点身份验证实际路由的 StateFlow 和嵌入流，不读取底层 session。
async fn run_link_stream_probe_command(args: &[String]) -> Result<(), String> {
    let json_mode = cli_json_mode();
    let name = args
        .get(0)
        .ok_or_else(|| "usage: operit2 cli link stream-probe <node-id>".to_string())?;
    let coreApplication = create_cli_core_application_without_space_sync("client").await?;
    let peerNodeId = name.clone();
    let service = coreApplication.accessServices();
    let space = service.joinPairedDeviceSpace(name.clone()).await?;
    if !json_mode {
        println!(
            "Probe joined space {name} on {} ({} members)",
            peerNodeId,
            space.members.len()
        );
    }

    let chatId = format!("route-probe-{}", link_probe_unix_millis());
    CoreNodeBindingStore::new(coreApplication.nodeRuntime().runtimeStorageHost())?
        .create(&chatId, &peerNodeId)?;
    if !json_mode {
        println!(
            "Probe binding created for chat {chatId} -> {}",
            peerNodeId
        );
    }

    let target =
        operit_proxy_local::LocalCoreProxy::generatedTargetForSchema("chatRuntimeHolderMain")
            .ok_or_else(|| "generated object id missing: chatRuntimeHolderMain".to_string())?;
    let flowArgs = CoreValue::Map(BTreeMap::from([
        ("chatId".to_string(), CoreValue::String(chatId.clone())),
        (
            "streamText".to_string(),
            CoreValue::String("rslink-route-probe".to_string()),
        ),
    ]));
    let mut flowStream = coreApplication
        .localClient()
        .watch(CoreWatchRequest::new(
            format!("route-probe-flow-{chatId}"),
            target,
            "routeProbeChatMessagesFlow",
            flowArgs,
        ))
        .await
        .map_err(|error| error.to_string())?;
    let flowEvent = recv_link_probe_event(&mut flowStream, "route probe flow").await?;
    let messages: Vec<ChatMessage> =
        operit_link::fromCoreValue(flowEvent.value.clone()).map_err(|error| error.to_string())?;
    let contentStreamCount = messages
        .iter()
        .filter(|message| message.contentStream.is_some())
        .count();
    if !json_mode {
        println!(
            "Probe flow event {:?}: {} messages, {} content streams",
            flowEvent.kind,
            messages.len(),
            contentStreamCount
        );
    }
    if messages.is_empty() || contentStreamCount == 0 {
        return Err("probe flow did not expose a ChatMessage.contentStream".to_string());
    }

    let descriptor = find_core_stream_descriptor(&flowEvent.value)
        .ok_or_else(|| "probe flow did not contain a $coreStream descriptor".to_string())?;
    if !json_mode {
        println!(
            "Probe stream descriptor {} -> {}.{}",
            descriptor.streamId, descriptor.target, descriptor.propertyName
        );
    }
    if descriptor.target != CORE_STREAM_TARGET || descriptor.propertyName != "openCoreStream" {
        return Err("probe stream descriptor does not target the Core stream pool".to_string());
    }

    let mut embeddedStream = coreApplication
        .localClient()
        .watch(CoreWatchRequest::new(
            format!("route-probe-embedded-{chatId}"),
            CORE_STREAM_TARGET,
            "openCoreStream",
            descriptor.args.clone(),
        ))
        .await
        .map_err(|error| error.to_string())?;
    let mut changedCount = 0usize;
    let mut completedCount = 0usize;
    let mut chunkText = String::new();
    loop {
        let event =
            recv_link_probe_event(&mut embeddedStream, "route probe embedded stream").await?;
        let markdown: MarkdownStreamEvent =
            operit_link::fromCoreValue(event.value.clone()).map_err(|error| error.to_string())?;
        if !json_mode {
            println!(
                "Probe stream event {:?}: {} {}",
                event.kind,
                markdown.eventType,
                markdown.value.clone().unwrap_or_default()
            );
        }
        match event.kind {
            CoreEventKind::Changed => {
                changedCount += 1;
                if markdown.eventType == "chunk" {
                    if let Some(value) = markdown.value {
                        chunkText.push_str(&value);
                    }
                }
            }
            CoreEventKind::Completed => {
                completedCount += 1;
                break;
            }
            CoreEventKind::Snapshot | CoreEventKind::Delta => {}
        }
    }
    if changedCount == 0 || completedCount != 1 {
        return Err(format!(
            "probe embedded stream events invalid: changed={} completed={}",
            changedCount, completedCount
        ));
    }
    if chunkText != "rslink-route-probe / chunk-one / chunk-two" {
        return Err(format!("probe embedded stream chunks invalid: {chunkText}"));
    }
    if json_mode {
        emit_cli_json(serde_json::json!({
            "ok": true,
            "space": name,
            "remoteNode": peerNodeId,
            "chatId": chatId,
            "messages": messages.len(),
            "contentStreams": contentStreamCount,
            "changed": changedCount,
            "completed": completedCount,
            "text": chunkText,
        }));
    } else {
        println!("Probe succeeded: {changedCount} changes, {completedCount} completion, text: {chunkText}");
    }
    Ok(())
}

async fn recv_link_probe_event(
    stream: &mut CoreEventStream,
    label: &str,
) -> Result<CoreEvent, String> {
    match timeout(Duration::from_secs(3), stream.recv()).await {
        Ok(Some(event)) => Ok(event),
        Ok(None) => Err(format!("{label} closed before producing an event")),
        Err(_) => Err(format!("{label} did not produce an event in time")),
    }
}

/// Returns the current Unix epoch in milliseconds for unique probe keys.
fn link_probe_unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after UNIX_EPOCH")
        .as_millis() as i64
}

/// Finds the first embedded Core stream descriptor in a structured Link value.
fn find_core_stream_descriptor(value: &CoreValue) -> Option<CoreStreamDescriptor> {
    match value {
        CoreValue::List(values) => values.iter().find_map(find_core_stream_descriptor),
        CoreValue::Map(values) => {
            if let Some(CoreValue::Map(descriptor)) = values.get("$coreStream") {
                return operit_link::fromCoreValue(CoreValue::Map(descriptor.clone())).ok();
            }
            values.values().find_map(find_core_stream_descriptor)
        }
        _ => None,
    }
}

/// Prints Link command usage in the selected output format.
fn print_link_usage() {
    if cli_json_mode() {
        emit_cli_json(
            serde_json::json!({ "usage": "operit2 cli link <discover|token|pair-start|pair-finish|pair-cancel|unpair|peers|listen|space|control|stream-probe|edge-plugin>" }),
        );
        return;
    }
    println!("operit2 cli link discover [--timeout-ms <ms>]");
    println!("operit2 cli link token show  # explicitly reveal the local pairing token");
    println!("operit2 cli link pair-start <node-id> <address> <http|ws|tcp|serial|bluetooth> [--token <token>]");
    println!("operit2 cli link pair-finish <pairing-id> <code>");
    println!("operit2 cli link pair-cancel <pairing-id> | unpair <node-id> | peers");
    println!("operit2 cli link listen <http|ws|tcp|serial|bluetooth>");
    println!("operit2 cli link space <show|status <device-name>|rename <name>|join <node-id>|disconnect <device-name>|remove <device-name>|leave>");
    println!("operit2 cli link control <show|bootstrap|audit|identity|device|policy>");
    println!("  identity list|define <name> <all|audit|relay|storage|execute|network|view|manage-identities|assign-identity|approve>...|set <device-name> <identity-name>|clear <device-name>");
    println!("  device list|admit <device-name>|remove <device-name>|disconnect <device-name>");
    println!("operit2 cli link stream-probe <node-id>");
    println!("operit2 cli link edge-plugin <device> list");
    println!("operit2 cli link edge-plugin <device> invoke <plugin-id> <action> [json-args]");
}
