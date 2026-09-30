// Frame-channel adapter for the shared Peer pairing transaction.
use crate::remote::{LinkAccessStore, LinkDeviceInfo, PairedPeerSessionRecord,
    PendingOutboundPairingRecord, PeerTransport};
use operit_peer_link::transport::{connectChannel, ChannelGuard};
use crate::RuntimeRemoteLinkService::RuntimeRemotePairStartResult;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use operit_peer_link::{finishPairAsClient, startPairAsClient};

#[derive(Clone)]
pub(crate) struct PeerChannelPairing {
    linkAccessStore: LinkAccessStore,
}

impl PeerChannelPairing {
    pub(crate) fn new(linkAccessStore: LinkAccessStore) -> Self {
        Self { linkAccessStore }
    }

    /// Starts the same Peer pairing transaction used by HTTP and WebSocket.
        #[allow(non_snake_case)]
    pub async fn startChannelPairing(
        &self,
        endpoint: String,
        tokenHash: String,
        clientDeviceInfo: LinkDeviceInfo,
    ) -> Result<RuntimeRemotePairStartResult, String> {
        if endpoint.trim().is_empty() {
            return Err("peer endpoint must not be empty".to_string());
        }
        if tokenHash.trim().is_empty() {
            return Err("peer token hash must not be empty".to_string());
        }
        let identity = self
            .linkAccessStore
            .initializeIdentity(clientDeviceInfo)?;
        let channel = connectChannel(&endpoint).await?;
        let _channelGuard = ChannelGuard(Some(channel.clone()));
        let state = startPairAsClient(
            channel.clone(),
            tokenHash,
            identity.deviceId,
            identity.deviceInfo,
        )
        .await?;
        let result = RuntimeRemotePairStartResult {
            coreUserName: String::new(),
            pairingId: state.pairingId.clone(),
            pairingServiceVersion: state.pairingServiceVersion,
            peerNodeId: state.peerNodeId.clone(),
            peerDeviceInfo: state.peerDeviceInfo.clone(),
        };
        self.linkAccessStore.savePendingOutboundPairing(
            state.pairingId.clone(),
            PendingOutboundPairingRecord {
                endpoint: endpoint.clone(),
                transport: PeerTransport::forEndpoint(&endpoint)?,
                state,
            },
        )?;
        channel.close().await;
        Ok(result)
    }

    /// Completes the same trust-only pairing transaction used by HTTP and WebSocket.
        #[allow(non_snake_case)]
    pub async fn finishChannelPairing(
        &self,
        pairingId: String,
        pairingCode: String,
        name: String,
    ) -> Result<PairedPeerSessionRecord, String> {
        if pairingId.trim().is_empty()
            || pairingCode.trim().is_empty()
            || name.trim().is_empty()
        {
            return Err("peer pairing id, code, and session name are required".to_string());
        }
        let stored = self
            .linkAccessStore
            .pendingOutboundPairings()?
            .get(&pairingId)
            .cloned()
            .ok_or_else(|| format!("pending peer pairing does not exist: {pairingId}"))?;
        let state = stored.state;
        let lock = crate::remote::connections::peerLock(&state.clientDeviceId, &state.peerNodeId);
        let _lifecycle = lock.lock().await;
        if !self
            .linkAccessStore
            .pendingOutboundPairings()?
            .contains_key(&pairingId)
        {
            return Err("pairing transaction has already completed".into());
        }
        if self
            .linkAccessStore
            .outboundSessions()?
            .get(&name)
            .is_some_and(|old| old.peerNodeId != state.peerNodeId)
        {
            return Err(format!("peer session name belongs to another device: {name}"));
        }
        let channel = connectChannel(&stored.endpoint).await?;
        let _channelGuard = ChannelGuard(Some(channel.clone()));
        let session = finishPairAsClient(channel.clone(), state, pairingCode).await?;
        let record = PairedPeerSessionRecord {
            endpoint: stored.endpoint,
            transport: stored.transport,
            sessionId: session.sessionId.clone(),
            deviceId: session.deviceId.clone(),
            peerNodeId: session.peerDeviceId.clone(),
            peerDeviceInfo: session.peerDeviceInfo.clone(),
            pairingServiceVersion: session.pairingServiceVersion,
            sessionSecret: BASE64.encode(&session.sessionSecret),
        };
        self.linkAccessStore
            .saveOutboundSession(name, record.clone())?;
        self.linkAccessStore
            .removePendingOutboundPairing(&pairingId)?;
        channel.close().await;
        Ok(record)
    }
}