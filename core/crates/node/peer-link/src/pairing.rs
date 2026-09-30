#![allow(non_snake_case)]

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use hmac::{Hmac, Mac};
use operit_link::{LinkDeviceInfo};
use crate::{LinkFrame, LinkFramePayload, LinkPairStartRequest, LinkPairStartResponse, LinkPairFinishRequest, LinkPairFinishResponse};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::transport::LinkChannel;

type HmacSha256 = Hmac<Sha256>;

pub const PAIRING_SERVICE_VERSION: i32 = 1;
pub const PAIRING_REVOKED: &str = "PAIRING_REVOKED";

/// A completed Peer pairing session that can authenticate future Link frames.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerSession {
    pub sessionId: String,
    pub deviceId: String,
    pub peerDeviceId: String,
    pub peerDeviceInfo: LinkDeviceInfo,
    pub pairingServiceVersion: i32,
    pub sessionSecret: Vec<u8>,
}

/// Small persisted state owned by a lightweight Peer device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairingPersistentState {
    /// The device's long-lived X25519 private key bytes.
    pub keySecret: Vec<u8>,
    /// Authenticated Core sessions accepted after a device restart.
    pub sessions: Vec<PeerSession>,
    /// Signed cancellation receipts contain no session secret. A disconnected
    /// Core can verify cancellation when it next reconnects, even after reboot.
    #[serde(default)]
    pub revokedSessions: BTreeMap<String, CancellationReceipt>,
}

/// Persist only the signed envelope, avoiding a second JSON implementation of
/// the entire PeerLink protocol on memory-constrained devices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationReceipt {
    sessionId: String,
    deviceId: String,
    signature: String,
    payloadBytes: Vec<u8>,
}

impl CancellationReceipt {
    fn frame(&self) -> LinkFrame {
        LinkFrame { messageId: self.sessionId.clone(), payload: LinkFramePayload::Authenticated {
            sessionId: self.sessionId.clone(), deviceId: self.deviceId.clone(),
            signature: self.signature.clone(), payloadBytes: self.payloadBytes.clone(),
        }}
    }
}

/// Storage boundary for device-specific NVS/flash implementations.
pub trait PairingStore: Send + Sync {
    fn load(&self) -> Result<Option<PairingPersistentState>, String>;
    fn save(&self, state: &PairingPersistentState) -> Result<(), String>;
}

/// Client-side state kept between pairing start and the user-entered code.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairStartState {
    pub pairingId: String,
    pub pairingServiceVersion: i32,
    pub clientDeviceId: String,
    pub clientDeviceInfo: LinkDeviceInfo,
    pub clientPublicKey: String,
    pub peerNodeId: String,
    pub peerDeviceInfo: LinkDeviceInfo,
    pub clientNonce: String,
    pub serverNonce: String,
    pub sharedSecret: Vec<u8>,
}

/// Returns the same token hash format used by Link Access pairing.
pub fn linkTokenHash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    BASE64.encode(hasher.finalize())
}

/// Signs one encoded Link payload with a completed pairing session.
pub fn signSession(sessionSecret: &[u8], payload: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(sessionSecret).expect("HMAC accepts any session secret length");
    mac.update(payload);
    BASE64.encode(mac.finalize().into_bytes())
}

/// Uses HMAC's constant-time verification rather than comparing encoded signatures.
pub fn verifySessionSignature(secret: &[u8], payload: &[u8], signature: &str) -> bool {
    let Ok(signature) = BASE64.decode(signature) else { return false; };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret) else { return false; };
    mac.update(payload);
    mac.verify_slice(&signature).is_ok()
}

fn publicKeyString(key: &PublicKey) -> String {
    BASE64.encode(key.as_bytes())
}

fn parsePublicKey(value: &str) -> Result<PublicKey, String> {
    let bytes = BASE64.decode(value).map_err(|error| error.to_string())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "X25519 public key must contain 32 bytes".to_string())?;
    Ok(PublicKey::from(bytes))
}

fn proof(sharedSecret: &[u8], clientNonce: &str, serverNonce: &str, role: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sharedSecret);
    hasher.update(clientNonce.as_bytes());
    hasher.update(serverNonce.as_bytes());
    hasher.update(role.as_bytes());
    BASE64.encode(hasher.finalize())
}

fn sessionSecret(sharedSecret: &[u8], clientNonce: &str, serverNonce: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(sharedSecret);
    hasher.update(clientNonce.as_bytes());
    hasher.update(serverNonce.as_bytes());
    hasher.update(b"session");
    hasher.finalize().to_vec()
}

fn pairingCode() -> String {
    let mut bytes = [0u8; 4];
    OsRng.fill_bytes(&mut bytes);
    format!("{:06}", u32::from_be_bytes(bytes) % 1_000_000)
}

#[derive(Clone)]
struct PendingPairing {
    clientDeviceId: String,
    clientDeviceInfo: LinkDeviceInfo,
    clientNonce: String,
    serverNonce: String,
    sharedSecret: Vec<u8>,
    pairingCode: String,
}

/// Owns the small pairing state required by an Peer device.
pub struct PairingAuthority {
    tokenHash: String,
    deviceId: String,
    deviceInfo: LinkDeviceInfo,
    keySecret: StaticSecret,
    pending: Mutex<BTreeMap<String, PendingPairing>>,
    sessions: Mutex<BTreeMap<String, PeerSession>>,
    revokedSessions: Mutex<BTreeMap<String, CancellationReceipt>>,
    store: Option<Arc<dyn PairingStore>>,
    onPairingCode: Arc<dyn Fn(String) + Send + Sync>,
    changes: tokio::sync::watch::Sender<u64>,
    commitSession: Option<Arc<dyn Fn(&PeerSession) -> Result<(), String> + Send + Sync>>,
}

impl PairingAuthority {
    /// Adapts a node's existing session repository without adding another trust state machine.
    pub fn withSessionCommit(mut self, commit: impl Fn(&PeerSession) -> Result<(), String> + Send + Sync + 'static) -> Self {
        self.commitSession = Some(Arc::new(commit)); self
    }
    pub fn publicKey(&self) -> String { publicKeyString(&PublicKey::from(&self.keySecret)) }
    pub fn subscribeChanges(&self) -> tokio::sync::watch::Receiver<u64> { self.changes.subscribe() }

    pub fn new(
        token: impl Into<String>,
        deviceId: impl Into<String>,
        deviceInfo: LinkDeviceInfo,
        onPairingCode: impl Fn(String) + Send + Sync + 'static,
    ) -> Self {
        let token = token.into();
        Self {
            tokenHash: linkTokenHash(&token),
            deviceId: deviceId.into(),
            deviceInfo,
            keySecret: StaticSecret::random_from_rng(OsRng),
            pending: Mutex::new(BTreeMap::new()),
            sessions: Mutex::new(BTreeMap::new()),
            revokedSessions: Mutex::new(BTreeMap::new()),
            store: None,
            onPairingCode: Arc::new(onPairingCode),
            changes: tokio::sync::watch::channel(0).0,
            commitSession: None,
        }
    }

    /// Creates an authority backed by device flash/NVS. The private key and
    /// completed sessions are restored before accepting clients.
    pub fn newWithStore(
        token: impl Into<String>,
        deviceId: impl Into<String>,
        deviceInfo: LinkDeviceInfo,
        store: Arc<dyn PairingStore>,
        onPairingCode: impl Fn(String) + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let token = token.into();
        let deviceId = deviceId.into();
        let (keySecret, sessions, revokedSessions) = match store.load()? {
            Some(state) => {
                let keyBytes: [u8; 32] = state
                    .keySecret
                    .try_into()
                    .map_err(|_| "persisted Peer X25519 key must contain 32 bytes".to_string())?;
                let sessions = state
                    .sessions
                    .into_iter()
                    .map(|session| {
                        if session.deviceId != deviceId {
                            return Err(format!(
                                "persisted Peer session {} belongs to a different device",
                                session.sessionId
                            ));
                        }
                        Ok((session.sessionId.clone(), session))
                    })
                    .collect::<Result<BTreeMap<_, _>, String>>()?;
                (StaticSecret::from(keyBytes), sessions, state.revokedSessions)
            }
            None => {
                let keySecret = StaticSecret::random_from_rng(OsRng);
                store.save(&PairingPersistentState {
                    keySecret: keySecret.to_bytes().to_vec(),
                    sessions: Vec::new(),
                    revokedSessions: BTreeMap::new(),
                })?;
                (keySecret, BTreeMap::new(), BTreeMap::new())
            }
        };
        Ok(Self {
            tokenHash: linkTokenHash(&token),
            deviceId,
            deviceInfo,
            keySecret,
            pending: Mutex::new(BTreeMap::new()),
            sessions: Mutex::new(sessions),
            revokedSessions: Mutex::new(revokedSessions),
            store: Some(store),
            onPairingCode: Arc::new(onPairingCode),
            changes: tokio::sync::watch::channel(0).0,
            commitSession: None,
        })
    }

    fn persistSessions(&self, sessions: &BTreeMap<String, PeerSession>, revokedSessions: &BTreeMap<String, CancellationReceipt>) -> Result<(), String> {
        let Some(store) = self.store.as_ref() else {
            return Ok(());
        };
        store.save(&PairingPersistentState {
            keySecret: self.keySecret.to_bytes().to_vec(),
            sessions: sessions.values().cloned().collect(),
            revokedSessions: revokedSessions.clone(),
        })
    }

    /// Reports whether this Peer has a completed pairing, including restored sessions.
    pub fn hasPairings(&self) -> bool {
        self.sessions.lock().map(|sessions| !sessions.is_empty()).unwrap_or(false)
    }

    /// Reports whether one active carrier is still authorized after a local reset.
    pub fn hasSession(&self, sessionId: &str) -> bool {
        self.sessions.lock().map(|sessions| sessions.contains_key(sessionId)).unwrap_or(false)
    }

    /// Completes the exact two-step Link pairing flow over a raw carrier.
    pub async fn pair(&self, channel: Arc<dyn LinkChannel>) -> Result<PeerSession, String> {
        let start = match channel
            .receive()
            .await?
            .ok_or_else(|| "Peer pairing channel closed".to_string())?
            .payload
        {
            LinkFramePayload::PairStart(request) => request,
            _ => return Err("expected Link pairing start".to_string()),
        };
        self.pairFromStart(channel, start).await
    }

    /// The same pairing transaction is used by HTTP, WS, TCP and serial admission.
    pub fn startPairing(&self, start: LinkPairStartRequest) -> Result<(LinkPairStartResponse, PairingPrompt), String> {
        if start.pairingServiceVersion != PAIRING_SERVICE_VERSION {
            return Err("unsupported Peer pairing service version".to_string());
        }
        if start.tokenHash != self.tokenHash {
            return Err("invalid Peer pairing token".to_string());
        }
        let clientPublic = parsePublicKey(&start.clientPublicKey)?;
        let serverPublic = PublicKey::from(&self.keySecret);
        let sharedSecret = self
            .keySecret
            .diffie_hellman(&clientPublic)
            .as_bytes()
            .to_vec();
        if sharedSecret.iter().all(|byte| *byte == 0) { return Err("Invalid client public key".into()); }
        let pairingId = Uuid::new_v4().to_string();
        let serverNonce = Uuid::new_v4().to_string();
        let code = pairingCode();
        {
            let mut pending = self.pending.lock().map_err(|error| error.to_string())?;
            pending.retain(|_, value| value.clientDeviceId != start.clientDeviceId);
            pending.insert(
                pairingId.clone(),
                PendingPairing {
                    clientDeviceId: start.clientDeviceId.clone(),
                    clientDeviceInfo: start.clientDeviceInfo.clone(),
                    clientNonce: start.clientNonce,
                    serverNonce: serverNonce.clone(),
                    sharedSecret,
                    pairingCode: code.clone(),
                },
            );
        }
        (self.onPairingCode)(code.clone());
        Ok((LinkPairStartResponse {
            pairingId: pairingId.clone(), pairingServiceVersion: PAIRING_SERVICE_VERSION,
            peerNodeId: self.deviceId.clone(), peerDeviceInfo: self.deviceInfo.clone(),
            peerPublicKey: publicKeyString(&serverPublic), serverNonce,
        }, PairingPrompt { pairingId, clientDeviceId: start.clientDeviceId,
            clientDeviceInfo: start.clientDeviceInfo, pairingCode: code }))
    }

    /// Completes pairing after the caller has already consumed PairStart.
    pub async fn pairFromStart(
        &self,
        channel: Arc<dyn LinkChannel>,
        start: LinkPairStartRequest,
    ) -> Result<PeerSession, String> {
        let (response, _) = self.startPairing(start)?;
        channel.send(LinkFrame { messageId: "pair-start-response".into(),
            payload: LinkFramePayload::PairStartResponse(response) }).await?;
        let finish = match channel
            .receive()
            .await?
            .ok_or_else(|| "Peer pairing channel closed before finish".to_string())?
            .payload
        {
            LinkFramePayload::PairFinish(request) => request,
            _ => return Err("expected Link pairing finish".to_string()),
        };
        self.pairFinishFromRequest(channel, finish).await
    }

    /// Completes a pending pairing from a PairFinish request.
    ///
    /// PairStart and PairFinish normally share one carrier, but the CLI is
    /// intentionally stateless between commands. Accepting PairFinish on a
    /// fresh carrier lets `pair-start` and `pair-finish` run in separate
    /// processes while retaining the same pending transaction on the Edge.
    pub async fn pairFinishFromRequest(
        &self,
        channel: Arc<dyn LinkChannel>,
        finish: LinkPairFinishRequest,
    ) -> Result<PeerSession, String> {
        let (session, coreProof) = self.acceptPairFinish(finish)?;
        channel.send(LinkFrame {
            messageId: "pair-finish-response".into(),
            payload: LinkFramePayload::PairFinishResponse(LinkPairFinishResponse {
                sessionId: session.sessionId.clone(),
                pairingServiceVersion: PAIRING_SERVICE_VERSION,
                coreProof,
            }),
        }).await?;
        Ok(session)
    }

    pub fn acceptPairFinish(&self, finish: LinkPairFinishRequest) -> Result<(PeerSession, String), String> {
        let mut pendingGuard = self
            .pending
            .lock()
            .map_err(|error| error.to_string())?;
        let pending = pendingGuard
            .get(&finish.pairingId).cloned()
            .ok_or_else(|| "Peer pairing transaction not found".to_string())?;
        let tokenAuthorized = finish.tokenHash.as_deref().is_some_and(|hash| hash == self.tokenHash);
        if !tokenAuthorized && pending.pairingCode != finish.pairingCode.trim() {
            return Err("invalid Peer pairing code".to_string());
        }
        let expectedProof = proof(
            &pending.sharedSecret,
            &pending.clientNonce,
            &pending.serverNonce,
            "client",
        );
        if expectedProof != finish.clientProof {
            return Err("invalid Peer client proof".to_string());
        }
        let session = PeerSession {
            sessionId: finish.pairingId.clone(),
            deviceId: self.deviceId.clone(),
            peerDeviceId: pending.clientDeviceId.clone(),
            peerDeviceInfo: pending.clientDeviceInfo.clone(),
            pairingServiceVersion: PAIRING_SERVICE_VERSION,
            sessionSecret: sessionSecret(
                &pending.sharedSecret,
                &pending.clientNonce,
                &pending.serverNonce,
            ),
        };
        {
            let mut sessions = self.sessions.lock().map_err(|error| error.to_string())?;
            let mut revoked = self.revokedSessions.lock().map_err(|error| error.to_string())?;
            let mut next = sessions.clone();
            let mut nextRevoked = revoked.clone();
            for old in sessions.values().filter(|old| old.peerDeviceId == session.peerDeviceId) {
                nextRevoked.insert(old.peerDeviceId.clone(), cancellationReceipt(old)?);
            }
            next.retain(|_, old| old.peerDeviceId != session.peerDeviceId);
            next.insert(session.sessionId.clone(), session.clone());
            if let Some(commit) = &self.commitSession { commit(&session)?; }
            self.persistSessions(&next, &nextRevoked)?;
            *sessions = next;
            *revoked = nextRevoked;
        }
        pendingGuard.remove(&finish.pairingId);
        drop(pendingGuard);
        // The one-time code is no longer valid once the session is persisted.
        self.changes.send_modify(|value| *value = value.wrapping_add(1));
        (self.onPairingCode)(String::new());
        Ok((session, proof(&pending.sharedSecret, &pending.clientNonce, &pending.serverNonce, "core")))
    }
    /// Removes all persisted pairing sessions and pending transactions.
    ///
    /// This does not erase the device token, Wi-Fi settings, or identity key.
    pub fn clearPairings(&self) -> Result<(), String> {
        let mut pending = self.pending.lock().map_err(|error| error.to_string())?;
        let mut sessions = self.sessions.lock().map_err(|error| error.to_string())?;
        let mut revoked = self.revokedSessions.lock().map_err(|error| error.to_string())?;
        let mut nextRevoked = revoked.clone();
        for session in sessions.values() {
            nextRevoked.insert(session.peerDeviceId.clone(), cancellationReceipt(session)?);
        }
        self.persistSessions(&BTreeMap::new(), &nextRevoked)?;
        *revoked = nextRevoked;
        pending.clear();
        sessions.clear();
        self.changes.send_modify(|value| *value = value.wrapping_add(1));
        (self.onPairingCode)(String::new());
        Ok(())
    }

    /// Only returns a previously authenticated cancellation, never an admission.
    pub fn cancellationFor(&self, frame: &LinkFrame) -> Option<LinkFrame> {
        let LinkFramePayload::Authenticated { sessionId, .. } = &frame.payload else { return None; };
        self.revokedSessions.lock().ok()?.values()
            .find(|receipt| &receipt.sessionId == sessionId).map(CancellationReceipt::frame)
    }

    /// Validates the first authenticated frame of a reconnecting Core and
    /// returns the stored session plus its decoded inner Link frame.
    pub fn authenticateFrame(&self, frame: &LinkFrame) -> Result<(PeerSession, LinkFrame), String> {
        let LinkFramePayload::Authenticated {
            sessionId,
            deviceId,
            signature,
            payloadBytes,
        } = &frame.payload
        else {
            return Err("expected authenticated Peer Link frame".to_string());
        };
        let session = self
            .sessions
            .lock()
            .map_err(|error| error.to_string())?
            .get(sessionId)
            .cloned()
            .ok_or_else(|| "Peer Link session is not known".to_string())?;
        if deviceId != &session.peerDeviceId {
            return Err("Peer Link device id mismatch".to_string());
        }
        if !verifySessionSignature(&session.sessionSecret, payloadBytes, signature) {
            return Err("Peer Link signature mismatch".to_string());
        }
        let inner = operit_link::decodeLink(payloadBytes).map_err(|error| error.to_string())?;
        Ok((session, inner))
    }
}

fn cancellationReceipt(session: &PeerSession) -> Result<CancellationReceipt, String> {
    let payloadBytes = operit_link::encodeLink(&LinkFrame {
        messageId: session.sessionId.clone(),
        payload: LinkFramePayload::Close {
            code: PAIRING_REVOKED.into(), message: "Peer pairing was cancelled".into(),
        },
    }).map_err(|error| error.to_string())?;
    Ok(CancellationReceipt {
            sessionId: session.sessionId.clone(), deviceId: session.deviceId.clone(),
            signature: signSession(&session.sessionSecret, &payloadBytes), payloadBytes,
    })
}

/// Owns the ephemeral key while the transport exchanges the initial request.
pub struct PairingClient {
    secret: StaticSecret,
    request: LinkPairStartRequest,
}
impl PairingClient {
    pub fn new(tokenHash: String, deviceId: String, info: LinkDeviceInfo, autoBootstrap: bool) -> Result<Self, String> {
        if deviceId.trim().is_empty() { return Err("Pairing client identity is empty".into()); }
        let secret = StaticSecret::random_from_rng(OsRng);
        let request = LinkPairStartRequest { pairingServiceVersion: PAIRING_SERVICE_VERSION,
            tokenHash, clientDeviceId: deviceId, clientDeviceInfo: info,
            clientPublicKey: publicKeyString(&PublicKey::from(&secret)),
            clientNonce: Uuid::new_v4().to_string(), autoBootstrap };
        Ok(Self { secret, request })
    }
    pub fn request(&self) -> LinkPairStartRequest { self.request.clone() }
    pub fn accept(self, response: LinkPairStartResponse) -> Result<PairStartState, String> {
        if response.pairingServiceVersion != PAIRING_SERVICE_VERSION || response.peerNodeId.trim().is_empty()
            || response.pairingId.is_empty() || response.serverNonce.is_empty() {
            return Err("Invalid pairing start response".into());
        }
        let public = parsePublicKey(&response.peerPublicKey)?;
        let sharedSecret = self.secret.diffie_hellman(&public).as_bytes().to_vec();
        if sharedSecret.iter().all(|byte| *byte == 0) { return Err("Invalid peer public key".into()); }
        Ok(PairStartState { pairingId: response.pairingId, pairingServiceVersion: response.pairingServiceVersion,
            clientDeviceId: self.request.clientDeviceId, clientDeviceInfo: self.request.clientDeviceInfo,
            clientPublicKey: self.request.clientPublicKey, peerNodeId: response.peerNodeId,
            peerDeviceInfo: response.peerDeviceInfo, clientNonce: self.request.clientNonce,
            serverNonce: response.serverNonce, sharedSecret })
    }
}
impl PairStartState {
    pub fn finishRequest(&self, code: String, tokenHash: Option<String>) -> LinkPairFinishRequest {
        LinkPairFinishRequest { pairingId: self.pairingId.clone(), pairingCode: code.trim().into(),
            clientProof: proof(&self.sharedSecret, &self.clientNonce, &self.serverNonce, "client"), tokenHash }
    }
    pub fn acceptFinish(&self, response: LinkPairFinishResponse) -> Result<PeerSession, String> {
        if response.sessionId != self.pairingId || response.pairingServiceVersion != self.pairingServiceVersion
            || response.coreProof != proof(&self.sharedSecret, &self.clientNonce, &self.serverNonce, "core") {
            return Err("Invalid pairing finish proof or identity".into());
        }
        Ok(PeerSession { sessionId: response.sessionId, deviceId: self.clientDeviceId.clone(),
            peerDeviceId: self.peerNodeId.clone(), peerDeviceInfo: self.peerDeviceInfo.clone(),
            pairingServiceVersion: response.pairingServiceVersion,
            sessionSecret: sessionSecret(&self.sharedSecret, &self.clientNonce, &self.serverNonce) })
    }
}

/// Frame-channel adapter for the shared pairing client; no transport-specific credentials.
pub async fn startPairAsClient(channel: Arc<dyn LinkChannel>, tokenHash: String, deviceId: String,
    deviceInfo: LinkDeviceInfo) -> Result<PairStartState, String> {
    let client = PairingClient::new(tokenHash, deviceId, deviceInfo, false)?;
    channel.send(LinkFrame { messageId: "pair-start".into(), payload: LinkFramePayload::PairStart(client.request()) }).await?;
    let response = channel.receive().await?.ok_or("Pairing carrier closed")?;
    match response.payload {
        LinkFramePayload::PairStartResponse(response) => client.accept(response),
        _ => Err("Expected pairing start response".into()),
    }
}
pub async fn finishPairAsClient(channel: Arc<dyn LinkChannel>, state: PairStartState, code: String) -> Result<PeerSession, String> {
    channel.send(LinkFrame { messageId: "pair-finish".into(), payload: LinkFramePayload::PairFinish(state.finishRequest(code, None)) }).await?;
    let response = channel.receive().await?.ok_or("Pairing carrier closed")?;
    match response.payload {
        LinkFramePayload::PairFinishResponse(response) => state.acceptFinish(response),
        _ => Err("Expected pairing finish response".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crate::LinkFrame;
    use tokio::sync::{mpsc, Mutex as AsyncMutex};

    struct MemoryChannel {
        sender: mpsc::UnboundedSender<LinkFrame>,
        receiver: AsyncMutex<mpsc::UnboundedReceiver<LinkFrame>>,
    }

    #[async_trait]
    impl LinkChannel for MemoryChannel {
        async fn send(&self, frame: LinkFrame) -> Result<(), String> {
            self.sender.send(frame).map_err(|error| error.to_string())
        }

        async fn receive(&self) -> Result<Option<LinkFrame>, String> {
            Ok(self.receiver.lock().await.recv().await)
        }

        async fn close(&self) {}
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn completesExistingPairingExchange() {
        let (clientSender, authorityReceiver) = mpsc::unbounded_channel();
        let (authoritySender, clientReceiver) = mpsc::unbounded_channel();
        let clientChannel = Arc::new(MemoryChannel {
            sender: clientSender,
            receiver: AsyncMutex::new(clientReceiver),
        });
        let authorityChannel = Arc::new(MemoryChannel {
            sender: authoritySender,
            receiver: AsyncMutex::new(authorityReceiver),
        });
        let displayedCode = Arc::new(Mutex::new(None));
        let displayedCodeForCallback = Arc::clone(&displayedCode);
        let authority = Arc::new(PairingAuthority::new(
            "edge-token",
            "edge-1",
            LinkDeviceInfo {
                platform: "esp32".to_string(),
                model: "test".to_string(),
            },
            move |code| {
                *displayedCodeForCallback.lock().unwrap() = Some(code);
            },
        ));
        let authorityTask = {
            let authority = Arc::clone(&authority);
            let authorityChannel = Arc::clone(&authorityChannel);
            tokio::spawn(async move { authority.pair(authorityChannel).await.unwrap() })
        };
        let start = startPairAsClient(
            clientChannel.clone(),
            linkTokenHash("edge-token"),
            "core-1".to_string(),
            LinkDeviceInfo {
                platform: "windows".to_string(),
                model: "test".to_string(),
            },
        )
        .await
        .unwrap();
        let code = displayedCode.lock().unwrap().clone().unwrap();
        let clientSession = finishPairAsClient(clientChannel, start, code)
            .await
            .unwrap();
        let authoritySession = authorityTask.await.unwrap();
        assert_eq!(clientSession.sessionId, authoritySession.sessionId);
        assert_eq!(clientSession.sessionSecret, authoritySession.sessionSecret);
    }

    /// Creates a bidirectional in-memory carrier for pairing lifecycle tests.
    fn pairingChannelPair() -> (Arc<MemoryChannel>, Arc<MemoryChannel>) {
        let (clientSender, authorityReceiver) = mpsc::unbounded_channel();
        let (authoritySender, clientReceiver) = mpsc::unbounded_channel();
        (
            Arc::new(MemoryChannel { sender: clientSender, receiver: AsyncMutex::new(clientReceiver) }),
            Arc::new(MemoryChannel { sender: authoritySender, receiver: AsyncMutex::new(authorityReceiver) }),
        )
    }

    #[derive(Default)]
    struct TestStore {
        state: Mutex<Option<PairingPersistentState>>,
        fail: std::sync::atomic::AtomicBool,
    }

    impl PairingStore for TestStore {
        fn load(&self) -> Result<Option<PairingPersistentState>, String> { Ok(self.state.lock().unwrap().clone()) }
        fn save(&self, state: &PairingPersistentState) -> Result<(), String> {
            if self.fail.load(std::sync::atomic::Ordering::Relaxed) { return Err("storage unavailable".into()); }
            *self.state.lock().unwrap() = Some(state.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn replacement_cancellation_and_storage_failure_preserve_identity() {
        let store = Arc::new(TestStore::default());
        let authority = PairingAuthority::newWithStore("token", "edge",
            LinkDeviceInfo { platform: "esp32".into(), model: "test".into() }, store.clone(), |_| {}).unwrap();
        let pair = |id: &str| {
            let pending = PendingPairing { clientDeviceId: "core".into(), clientDeviceInfo: LinkDeviceInfo::native(), clientNonce: "client".into(),
                serverNonce: "server".into(), sharedSecret: vec![7; 32], pairingCode: "123456".into() };
            let finish = LinkPairFinishRequest { tokenHash: None, pairingId: id.into(), pairingCode: pending.pairingCode.clone(),
                clientProof: proof(&pending.sharedSecret, &pending.clientNonce, &pending.serverNonce, "client") };
            authority.pending.lock().unwrap().insert(id.into(), pending);
            authority.acceptPairFinish(finish)
        };
        let (old, _) = pair("old").unwrap();
        store.fail.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(pair("failed").is_err());
        assert!(authority.hasSession(&old.sessionId));
        assert!(authority.clearPairings().is_err());
        assert!(authority.hasPairings());
        store.fail.store(false, std::sync::atomic::Ordering::Relaxed);
        let (replacement, _) = pair("new").unwrap();
        assert!(!authority.hasSession(&old.sessionId));
        assert!(authority.hasSession(&replacement.sessionId));
        assert_eq!(store.load().unwrap().unwrap().sessions.len(), 1);
        authority.clearPairings().unwrap();
        assert!(!authority.hasPairings());
        assert!(!authority.hasSession(&replacement.sessionId));
        let (client, server) = pairingChannelPair();
        let peerSession = PeerSession { deviceId: "core".into(), peerDeviceId: "edge".into(), ..replacement };
        let client = crate::AuthenticatedLinkChannel::new(client, peerSession);
        client.send(LinkFrame { messageId: "reconnect".into(), payload: LinkFramePayload::PeerFrame(operit_link::PeerFrame { messageId: "heartbeat-test".into(), payload: operit_link::PeerFramePayload::Heartbeat(operit_link::PeerHeartbeat::Probe { sequence: 1, sentAt: 0 }) }) }).await.unwrap();
        let frame = server.receive().await.unwrap().unwrap();
        assert!(authority.authenticateFrame(&frame).is_err());
        server.send(authority.cancellationFor(&frame).unwrap()).await.unwrap();
        assert!(matches!(client.receive().await.unwrap().unwrap().payload,
            LinkFramePayload::Close { code, .. } if code == PAIRING_REVOKED));
        pair("paired-again").unwrap();
        assert!(authority.hasPairings());
        assert_eq!(store.load().unwrap().unwrap().sessions.len(), 1);
    }

    /// Verifies a pending transaction survives closure of its initial carrier.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn completesPairingOnFreshCarrier() {
        let displayedCode = Arc::new(Mutex::new(None));
        let callbackCode = displayedCode.clone();
        let authority = Arc::new(PairingAuthority::new(
            "edge-token", "edge-1",
            LinkDeviceInfo { platform: "edge".to_owned(), model: "test".to_owned() },
            move |code| { *callbackCode.lock().unwrap() = Some(code); },
        ));
        let (client, server) = pairingChannelPair();
        let firstAuthority = authority.clone();
        let startTask = tokio::spawn(async move { firstAuthority.pair(server).await });
        let state = startPairAsClient(client.clone(), linkTokenHash("edge-token"), "core-1".to_owned(),
            LinkDeviceInfo { platform: "test".to_owned(), model: "test".to_owned() }).await.unwrap();
        let code = displayedCode.lock().unwrap().clone().unwrap();
        drop(client);
        assert!(startTask.await.unwrap().is_err());
        let (client, server) = pairingChannelPair();
        let finishTask = tokio::spawn(async move {
            let frame = server.receive().await.unwrap().unwrap();
            let LinkFramePayload::PairFinish(request) = frame.payload else {
                panic!("expected pairing completion on the fresh carrier");
            };
            authority.pairFinishFromRequest(server, request).await.unwrap()
        });
        let clientSession = finishPairAsClient(client, state, code).await.unwrap();
        let serverSession = finishTask.await.unwrap();
        assert_eq!(clientSession.sessionId, serverSession.sessionId);
        assert_eq!(clientSession.sessionSecret, serverSession.sessionSecret);
    }

}

#[derive(Clone, Debug)]
pub struct PairingPrompt {
    pub pairingId: String,
    pub clientDeviceId: String,
    pub clientDeviceInfo: LinkDeviceInfo,
    pub pairingCode: String,
}
