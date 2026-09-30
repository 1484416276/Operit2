use operit_peer_link::{AuthenticatedLinkChannel, PairingAuthority, PeerRouteClient};
use operit_peer_link::transport::LinkChannel;
use std::sync::Arc;

/// Handles the first frame and then serves one authenticated EdgeLink session.
pub async fn handleChannel(
    authority: Arc<PairingAuthority>,
    channel: Arc<dyn LinkChannel>,
) -> Result<(), String> {
    let result = handleSession(authority, channel.clone(), None).await;
    channel.close().await;
    result
}

pub async fn handleChannelWithNode(
    authority: Arc<PairingAuthority>,
    channel: Arc<dyn LinkChannel>,
    node: Arc<operit_node_edge::EdgeNode>,
) -> Result<(), String> {
    let result = handleSession(authority, channel.clone(), Some(node)).await;
    channel.close().await;
    result
}

async fn handleSession(
    authority: Arc<PairingAuthority>,
    channel: Arc<dyn LinkChannel>,
    node: Option<Arc<operit_node_edge::EdgeNode>>,
) -> Result<(), String> {
    let first = operit_peer_link::timing::withHostTimeout(10_000, channel.receive()).await?
        .ok_or_else(|| "Edge Link carrier closed".to_string())?;
    match &first.payload {
        operit_peer_link::LinkFramePayload::PairStart(request) => {
            log::info!("Edge pairing: PairStart decoded");
            let session = operit_peer_link::timing::withHostTimeout(180_000, authority.pairFromStart(channel.clone(), request.clone())).await?;
            log::info!("Edge pairing: PairFinish accepted");
            #[cfg(target_os = "espidf")]
            crate::logRuntimeHealth("edge-paired");
            serveAuthenticatedSession(authority, channel, session, node).await
        }
        operit_peer_link::LinkFramePayload::PairFinish(request) => {
            // The CLI deliberately runs pair-start and pair-finish as separate
            // processes. Therefore PairFinish may be the first frame on a new
            // carrier; the authority keeps the pending transaction by ID.
            let session = authority
                .pairFinishFromRequest(channel.clone(), request.clone())
                .await?;
            serveAuthenticatedSession(authority, channel, session, node).await
        }
        operit_peer_link::LinkFramePayload::Authenticated { .. } => {
            if let Some(receipt) = authority.cancellationFor(&first) {
                channel.send(receipt).await?;
                return Ok(());
            }
            let (session, inner) = authority.authenticateFrame(&first)?;
            let peerId = session.peerDeviceId.clone();
            let sessionId = session.sessionId.clone();
            let deviceId = session.deviceId.clone();
            let authenticated = AuthenticatedLinkChannel::new(channel, session);
            installSpaceRoute(authority, authenticated, inner, &peerId, &sessionId, &deviceId, node).await?;
            Ok(())
        }
        _ => Err("Edge Link connection did not start with pairing or authentication".to_string()),
    }
}

async fn serveAuthenticatedSession(
    authority: Arc<PairingAuthority>,
    channel: Arc<dyn LinkChannel>,
    session: operit_peer_link::PeerSession,
    node: Option<Arc<operit_node_edge::EdgeNode>>,
) -> Result<(), String> {
    let peerId = session.peerDeviceId.clone();
    let sessionId = session.sessionId.clone();
    let deviceId = session.deviceId.clone();
    let authenticated = AuthenticatedLinkChannel::new(channel, session);
    let context = operit_peer_link::timing::withHostTimeout(30_000, authenticated.receive()).await?
        .ok_or_else(|| "Space admission context was not received".to_string())?;
    installSpaceRoute(authority, authenticated, context, &peerId, &sessionId, &deviceId, node).await
}

async fn installSpaceRoute(
    authority: Arc<PairingAuthority>,
    channel: Arc<dyn LinkChannel>,
    frame: operit_peer_link::LinkFrame,
    peerId: &str,
    sessionId: &str,
    deviceId: &str,
    node: Option<Arc<operit_node_edge::EdgeNode>>,
) -> Result<(), String> {
    if !authority.hasSession(sessionId) {
        channel.send(operit_peer_link::LinkFrame {
            messageId: sessionId.into(),
            payload: operit_peer_link::LinkFramePayload::Close {
                code: operit_peer_link::pairing::PAIRING_REVOKED.into(),
                message: "Edge pairing was cancelled".into(),
            },
        }).await?;
        channel.close().await;
        return Err("Pairing was cancelled during admission".into());
    }
    let operit_peer_link::LinkFramePayload::SpaceContext {
        spaceId,
        adjacentNodeId,
        ttl,
    } = frame.payload
    else {
        return Err("Edge session did not begin with authenticated Space admission".to_string());
    };
    if spaceId.trim().is_empty() || adjacentNodeId != peerId || ttl == 0
    {
        return Err("Invalid authenticated Space route context".to_string());
    }
    // Heartbeat belongs to PeerConnection. Do not require a second,
    // device-specific probe before registering the authenticated peer.
    let mut changes = authority.subscribeChanges();
    let attached = operit_peer_link::attachPeerLinkCarrier(
        deviceId.into(), peerId.into(), format!("session-{sessionId}"),
        Arc::new(operit_peer_link::transport::channel::ChannelPeerCarrier(channel.clone())),
        Arc::new(operit_node_edge::peer::DevicePeerEndpoint {
            node, spaceId: spaceId.clone(), adjacentNodeId: peerId.into(), localNodeId: deviceId.into(),
        }), None,
    )?;
    let client = PeerRouteClient::throughAdjacent(attached.client(), spaceId, deviceId.into(), adjacentNodeId, ttl);
    operit_link::installCoreRouteRuntime(Arc::new(client.clone()));
    let bootstrap = crate::edge_chat::reconnect(client, format!("edge-chat-{deviceId}"));
    let result = async {
        loop {
            if !authority.hasSession(sessionId) {
                let _ = channel.send(operit_peer_link::LinkFrame { messageId: sessionId.into(),
                    payload: operit_peer_link::LinkFramePayload::Close {
                        code: operit_peer_link::pairing::PAIRING_REVOKED.into(), message: "Pairing was revoked".into(),
                    },
                }).await;
                return Ok(());
            }
            tokio::select! {
                _ = attached.waitClosed() => return Ok(()),
                changed = changes.changed() => { changed.map_err(|e| e.to_string())?; }
                frame = channel.receive() => {
                    let Some(frame) = frame? else { return Ok(()); };
                    match frame.payload {
                        operit_peer_link::LinkFramePayload::PeerFrame(frame) => attached.receiveFrame(frame).await?,
                        operit_peer_link::LinkFramePayload::Close { .. } => return Ok(()),
                        _ => return Err("Expected authenticated PeerFrame".into()),
                    }
                }
            }
        }
    }.await;
    bootstrap.abort();
    attached.close("Authenticated carrier ended".into());
    channel.close().await;
    result
}
