//! 生产节点通信：唯一的配对/鉴权入口，所有 I/O 委托 PeerLink 和 Host。
//! 未鉴权只接受 hello/authorize；已鉴权 Call/Watch/Push 交给 Router。
use crate::{CoreNodeRouter::CoreNodeRouter, NodeServices::*, PeerStateStore::{PeerStateStore, StoredInbound, StoredOutbound, PAIRING_SERVICE_VERSION},
    RuntimePeerService::RuntimePeerService};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use operit_host_api::{HostManager::HostManager, TimeUtils::currentTimeMillis};
use operit_link::*;
use operit_peer_link::{HostPeerLink, PeerConnection, PeerLink, PeerListener, PeerMessage};
use operit_util::RuntimeStorageLayout::*;
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, sync::{Arc, Mutex, Weak}};
use tokio::sync::{broadcast, Mutex as AsyncMutex};
#[path = "peer/crypto.rs"] mod crypto;
use crypto::{Channel, error};
#[path = "peer/dispatch.rs"] mod dispatch;
const HANDSHAKE: &str = "$peer.pairing";

const PAIRING_LIFETIME_MS: i64 = 300_000;

#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    version: u32, id: String, clientDeviceId: String, peerNodeId: String,
    endpoint: String, transport: PeerTransport, info: LinkDeviceInfo,
    root: Vec<u8>, expires: i64, attempts: u8,
    #[serde(default)] confirmationCode: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Hello {
    version: u32, nodeId: String, expectedNodeId: String, info: LinkDeviceInfo,
    public: Vec<u8>, purpose: String, sessionId: String,
}
#[derive(Serialize, Deserialize)]
struct HelloReply { transcript: crypto::Transcript, info: LinkDeviceInfo, tokenRequired: bool }
#[derive(Serialize, Deserialize)]
struct Authorization { tokenProof: Vec<u8>, keyProof: Vec<u8> }
#[derive(Serialize, Deserialize)]
struct Authorized { proof: Vec<u8>, pairingId: String }

struct State {
    host: Arc<HostManager>, link: HostPeerLink, store: PeerStateStore,
    nodeId: String, info: LinkDeviceInfo, router: Weak<CoreNodeRouter>,
    listeners: AsyncMutex<BTreeMap<PeerTransport, Arc<dyn PeerListener>>>,
    connections: Mutex<BTreeMap<String, Vec<Weak<dyn PeerConnection>>>>,
    advertisements: Mutex<Vec<Box<dyn operit_host_api::ServiceDiscovery::DiscoveryAdvertisement>>>,
    slots: Arc<tokio::sync::Semaphore>,
    active: Mutex<BTreeSet<String>>, changes: broadcast::Sender<()>,
    /// 本节点的配对/撤销持久化操作串行化，不持锁执行网络 I/O。
    mutation: Mutex<()>,
}
#[derive(Clone)]
pub struct HostRuntimePeerService { state: Arc<State> }
impl HostRuntimePeerService {
    pub fn new(host: Arc<HostManager>, router: &Arc<CoreNodeRouter>, info: LinkDeviceInfo) -> Result<Arc<Self>, String> {
        let storage = host.runtimeStorageHost.clone().ok_or("Runtime storage Host is not installed")?;
        Ok(Arc::new(Self { state: Arc::new(State {
            host, link: HostPeerLink::default(), store: PeerStateStore::new(storage),
            nodeId: router.localNodeId(), info, router: Arc::downgrade(router),
            listeners: AsyncMutex::new(BTreeMap::new()), connections: Mutex::new(BTreeMap::new()),
            advertisements: Mutex::new(Vec::new()), slots: Arc::new(tokio::sync::Semaphore::new(64)),
            active: Mutex::new(BTreeSet::new()), changes: broadcast::channel(32).0,
            mutation: Mutex::new(()),
        }) }))
    }
    fn router(&self) -> Result<CoreNodeRouter, CoreLinkError> {
        self.state.router.upgrade().map(|r| (*r).clone()).ok_or_else(|| error("Node application has stopped"))
    }
    fn changed(&self) { let _ = self.state.changes.send(()); }
    fn track(&self, node: &str, raw: &Arc<dyn PeerConnection>) {
        let mut connections = self.state.connections.lock().unwrap();
        let list = connections.entry(node.into()).or_default();
        list.retain(|c| c.strong_count() != 0); list.push(Arc::downgrade(raw));
    }
    fn pending(&self, inbound: bool, id: &str) -> Result<Pending, CoreLinkError> {
        let path = if inbound { RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH } else { RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH };
        let p = self.pendingRecords(path)?.remove(id)
            .ok_or_else(|| error("Pairing transaction not found"))?;
        if p.expires <= currentTimeMillis() { self.state.store.deleteRecord(path, id).map_err(error)?; return Err(error("Pairing transaction expired")); }
        Ok(p)
    }
    fn inboundCredentials(&self) -> Result<BTreeMap<String, StoredInbound>, CoreLinkError> {
        self.state.store.records(RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH).map_err(error)
    }
    fn pendingRecords(&self, path: &str) -> Result<BTreeMap<String, Pending>, CoreLinkError> {
        let raw = self.state.store.records::<serde_json::Value>(path).map_err(error)?;
        raw.into_iter().filter(|(_, r)| r.get("version").and_then(|v| v.as_u64()) == Some(PAIRING_SERVICE_VERSION as u64))
            .map(|(id, r)| serde_json::from_value(r).map(|r| (id, r)).map_err(|e| error(e.to_string()))).collect()
    }
    fn outbound(&self, node: &str) -> Result<StoredOutbound, CoreLinkError> {
        self.state.store.records::<StoredOutbound>(RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH).map_err(error)?
            .into_values().find(|c| c.peerNodeId == node && c.pairingServiceVersion == PAIRING_SERVICE_VERSION)
            .ok_or_else(|| error("No current outbound authorization for node"))
    }
    async fn raw(&self, target: PeerEndpoint, transport: PeerTransport) -> Result<Arc<dyn PeerConnection>, CoreLinkError> {
        self.state.link.connect(self.state.host.clone(), PeerEndpoint { nodeId: self.state.nodeId.clone(), address: String::new() }, target, transport).await.map_err(error)
    }
    /// 主动端与接收端共用握手，无论 HTTP/WS/TCP/串口/蓝牙。
    async fn handshake(&self, raw: Arc<dyn PeerConnection>, purpose: &str, sessionId: &str,
        saved: Option<&[u8]>, token: Option<&str>,
    ) -> Result<(Arc<Channel>, String, LinkDeviceInfo, Vec<u8>, String), CoreLinkError> {
        let (private, public) = crypto::ephemeral()?;
        let hello = Hello { version: PAIRING_SERVICE_VERSION, nodeId: self.state.nodeId.clone(),
            expectedNodeId: raw.target().nodeId.clone(), info: self.state.info.clone(), public: public.clone(),
            purpose: purpose.into(), sessionId: sessionId.into() };
        let reply: HelloReply = rawCall(&raw, "hello", &hello).await?;
        let t = &reply.transcript;
        if t.version != PAIRING_SERVICE_VERSION || t.clientNodeId != self.state.nodeId || t.clientPublic != public
            || (!raw.target().nodeId.is_empty() && t.serverNodeId != raw.target().nodeId)
            || t.serverNodeId == self.state.nodeId || t.serverPublic.len() != 32 || t.challenge.len() != 32
            || (!sessionId.is_empty() && t.sessionId != sessionId) {
            return Err(error("Handshake identity/transcript mismatch"));
        }
        let context = crypto::transcript(t)?;
        let dh = crypto::agree(private, &t.serverPublic)?;
        let root = crypto::derive(&dh, &context, b"operit-pairing-root-v1")?;
        let key = match saved { Some(saved) => crypto::derive(&dh, saved, &context)?, None => root };
        let tokenProof = if purpose == "start" && reply.tokenRequired {
            crypto::proof(token.ok_or_else(|| error("Token required for non-LAN pairing"))?.as_bytes(), &context, b"token")
        } else { vec![] };
        let accepted: Authorized = rawCall(&raw, "authorize", &Authorization {
            tokenProof, keyProof: crypto::proof(&key, &context, b"client"),
        }).await?;
        crypto::verify(&key, &context, b"server", &accepted.proof)?;
        if accepted.pairingId != t.sessionId { return Err(error("Pairing correlation mismatch")); }
        let id = t.sessionId.clone(); let peerNodeId = t.serverNodeId.clone(); let info = reply.info;
        Ok((Channel::new(raw, &key, &context, true)?, id, info, root.to_vec(), peerNodeId))
    }
    async fn connectAuthorized(&self, node: &str) -> Result<Arc<Channel>, CoreLinkError> {
        let record = self.outbound(node)?;
        let transport = parseTransport(&record.transport)?;
        let root = BASE64.decode(&record.sessionSecret).map_err(|_| error("Invalid stored credential"))?;
        let raw = self.raw(PeerEndpoint { nodeId: node.into(), address: record.endpoint }, transport).await?;
        let result = self.handshake(raw.clone(), "session", &record.sessionId, Some(&root), None).await;
        let (channel, _, _, _, _) = match result { Ok(result) => result, Err(e) => { raw.close().await; return Err(e); } };
        self.track(node, &raw);
        if self.state.active.lock().unwrap().insert(node.into()) { self.changed(); }
        Ok(channel)
    }
    async fn readHandshake(&self, raw: &Arc<dyn PeerConnection>, method: &str) -> Result<CoreCallRequest, CoreLinkError> {
        let delay = self.state.host.hostRuntimeTaskSchedulerHost.as_ref().ok_or_else(|| error("Host scheduler missing"))?.waitForHostRuntimeDelay(15_000);
        tokio::select! { r = readCall(raw, method) => r, _ = delay => Err(error("Unauthenticated handshake deadline reached")) }
    }
    async fn serve(&self, raw: Arc<dyn PeerConnection>) -> Result<(), CoreLinkError> {
        let helloRequest = self.readHandshake(&raw, "hello").await?;
        let hello: Hello = fromCoreValue(helloRequest.args.clone()).map_err(|e| error(e.to_string()))?;
        if hello.version != PAIRING_SERVICE_VERSION || hello.nodeId.is_empty() || hello.nodeId == self.state.nodeId
            || (!hello.expectedNodeId.is_empty() && hello.expectedNodeId != self.state.nodeId)
            || !matches!(hello.purpose.as_str(), "start" | "finish" | "session") {
            return sendError(&raw, helloRequest.requestId, error("Invalid pairing identity/purpose")).await;
        }
        // 沿用持久化的 sessionId/sessionSecret；配对事务仍受过期时间和尝试上限保护。
        let saved = match hello.purpose.as_str() {
            "start" => None,
            "finish" => { let p = self.pending(true, &hello.sessionId)?;
                if p.clientDeviceId != hello.nodeId { return Err(error("Pairing identity mismatch")); }
                Some(p.root) },
            _ => {
                let records = self.inboundCredentials()?;
                let record = records.get(&hello.sessionId).ok_or_else(|| error("Inbound authorization not found"))?;
                if record.deviceId != hello.nodeId || record.pairingServiceVersion != PAIRING_SERVICE_VERSION { return Err(error("Inbound identity/version mismatch")); }
                Some(BASE64.decode(&record.sessionSecret).map_err(|_| error("Invalid stored credential"))?)
            },
        };
        let config = self.state.store.hostConfig().map_err(error)?.ok_or_else(|| error("Listener not configured"))?;
        // 免 token 必须同时开启本地发现，且来源是 Host 实际接入地址；未知来源 fail closed。
        let lan = config.discoveryEnabled && raw.remoteAddress().is_some_and(|a| isLocalAddress(a.ip()));
        let tokenRequired = hello.purpose == "start" && !lan;
        let (private, public) = crypto::ephemeral()?;
        let id = if hello.purpose == "start" { uuid::Uuid::new_v4().to_string() } else { hello.sessionId.clone() };
        let transcript = crypto::Transcript { version: PAIRING_SERVICE_VERSION, sessionId: id.clone(),
            clientNodeId: hello.nodeId.clone(), serverNodeId: self.state.nodeId.clone(),
            clientPublic: hello.public, serverPublic: public, challenge: crypto::random()?.to_vec() };
        let context = crypto::transcript(&transcript)?;
        let dh = crypto::agree(private, &transcript.clientPublic)?;
        let root = crypto::derive(&dh, &context, b"operit-pairing-root-v1")?;
        let key = match &saved { Some(saved) => crypto::derive(&dh, saved, &context)?, None => root };
        sendValue(&raw, helloRequest.requestId, HelloReply { transcript, info: self.state.info.clone(), tokenRequired }).await?;
        let request = self.readHandshake(&raw, "authorize").await?;
        let authorization: Authorization = fromCoreValue(request.args).map_err(|e| error(e.to_string()))?;
        let admission = (|| {
            if tokenRequired {
                if config.token.is_empty() { return Err(error("Non-LAN pairing token is not configured")); }
                crypto::verify(config.token.as_bytes(), &context, b"token", &authorization.tokenProof)?;
            }
            crypto::verify(&key, &context, b"client", &authorization.keyProof)
        })();
        if let Err(e) = admission { return sendError(&raw, request.requestId, e).await; }
        if hello.purpose == "start" {
            let _guard = self.state.mutation.lock().unwrap();
            let pending = self.pendingRecords(RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH)?;
            if pending.values().filter(|p| p.expires > currentTimeMillis()).count() >= 32 { return Err(error("Pending pairing capacity reached")); }
            self.state.store.putRecord(RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH, &id, &Pending {
                version: PAIRING_SERVICE_VERSION, id: id.clone(), clientDeviceId: hello.nodeId.clone(), peerNodeId: self.state.nodeId.clone(),
                endpoint: String::new(), transport: raw.transport(), info: hello.info.clone(),
                root: root.to_vec(), expires: currentTimeMillis() + PAIRING_LIFETIME_MS, attempts: 0,
                confirmationCode: Some(format!("{:06}{}", u32::from_be_bytes(crypto::random()?[..4].try_into().unwrap()) % 1_000_000, crypto::code(&root))),
            }).map_err(error)?;
            self.changed();
        }
        sendValue(&raw, request.requestId, Authorized { proof: crypto::proof(&key, &context, b"server"), pairingId: id.clone() }).await?;
        if hello.purpose == "start" { return Ok(()); }
        let channel = Channel::new(raw.clone(), &key, &context, false)?;
        self.track(&hello.nodeId, &raw);
        if hello.purpose == "finish" {
            let Some(PeerMessage::Request(CoreLinkRequest::Call(request))) = channel.receive().await? else { return Err(error("Encrypted confirmation Call required")); };
            if request.target != HANDSHAKE || request.methodName != "finish" { return Err(error("Confirmation required before business")); }
            let proof: Vec<u8> = fromCoreValue(request.args).map_err(|e| error(e.to_string()))?;
            let result = (|| {
                let _guard = self.state.mutation.lock().unwrap();
                let mut pending = self.pending(true, &id)?;
                if pending.attempts >= 5 { return Err(error("Confirmation attempt limit reached")); }
                pending.attempts += 1;
                self.state.store.putRecord(RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH, &id, &pending).map_err(error)?;
                let code = pending.confirmationCode.as_deref().ok_or_else(|| error("Receiver confirmation missing"))?;
                crypto::verify(&pending.root, id.as_bytes(), code.as_bytes(), &proof)?;
                let record = StoredInbound { deviceId: hello.nodeId.clone(), deviceInfo: hello.info.clone(),
                    pairingServiceVersion: PAIRING_SERVICE_VERSION, sessionSecret: BASE64.encode(&pending.root) };
                self.state.store.putRecord(RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH, &id, &record).map_err(error)?;
                self.state.store.deleteRecord(RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH, &id).map_err(error)?;
                self.changed(); Ok(CoreValue::Null)
            })();
            channel.send(PeerMessage::Response(CoreLinkResponse::Call(CoreCallResponse { requestId: request.requestId, result }))).await?;
            return Ok(());
        }
        dispatch::serve(self.clone(), channel, hello.nodeId, id).await
    }
}

async fn readCall(raw: &Arc<dyn PeerConnection>, method: &str) -> Result<CoreCallRequest, CoreLinkError> {
    match raw.receive().await.map_err(error)? {
        Some(PeerMessage::Request(CoreLinkRequest::Call(r))) if r.target == HANDSHAKE && r.methodName == method => Ok(r),
        _ => Err(error("Unauthenticated operation is not in the pairing whitelist")),
    }
}
async fn rawCall<T: Serialize, R: serde::de::DeserializeOwned>(raw: &Arc<dyn PeerConnection>, method: &str, args: &T) -> Result<R, CoreLinkError> {
    let id = CoreRequestId::new(uuid::Uuid::new_v4().to_string());
    raw.send(PeerMessage::Request(CoreLinkRequest::Call(CoreCallRequest::new(id.0.clone(), HANDSHAKE, method,
        toCoreValue(args).map_err(|e| error(e.to_string()))?)))).await.map_err(error)?;
    match raw.receive().await.map_err(error)? {
        Some(PeerMessage::Response(CoreLinkResponse::Call(r))) if r.requestId == id => fromCoreValue(r.result?).map_err(|e| error(e.to_string())),
        _ => Err(error("Pairing response correlation mismatch")),
    }
}
async fn sendValue(raw: &Arc<dyn PeerConnection>, id: CoreRequestId, value: impl Serialize) -> Result<(), CoreLinkError> {
    raw.send(PeerMessage::Response(CoreLinkResponse::Call(CoreCallResponse::ok(id, toCoreValue(value).map_err(|e| error(e.to_string()))?))))
        .await.map_err(error)
}
async fn sendError(raw: &Arc<dyn PeerConnection>, id: CoreRequestId, e: CoreLinkError) -> Result<(), CoreLinkError> {
    raw.send(PeerMessage::Response(CoreLinkResponse::Call(CoreCallResponse::err(id, e)))).await.map_err(error)
}
fn transportName(t: PeerTransport) -> &'static str { match t {
    PeerTransport::Http => "http", PeerTransport::WebSocket => "ws", PeerTransport::Tcp => "tcp",
    PeerTransport::Serial => "serial", PeerTransport::Bluetooth => "bluetooth",
} }
fn parseTransport(t: &str) -> Result<PeerTransport, CoreLinkError> { match t {
    "http" => Ok(PeerTransport::Http), "ws" => Ok(PeerTransport::WebSocket), "tcp" => Ok(PeerTransport::Tcp),
    "serial" => Ok(PeerTransport::Serial), "bluetooth" => Ok(PeerTransport::Bluetooth), _ => Err(error("Unknown transport")),
} }
fn isLocalAddress(address: std::net::IpAddr) -> bool { match address {
    std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
    std::net::IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
        || ip.to_ipv4_mapped().is_some_and(|ip| isLocalAddress(ip.into())),
} }

#[async_trait(?Send)]
impl RuntimePeerService for HostRuntimePeerService {
    async fn discoverPeers(&self, timeoutMs: u64) -> Result<Vec<DiscoveredPeer>, CoreLinkError> {
        let host = self.state.host.serviceDiscoveryHost.clone().ok_or_else(|| error("Discovery Host is not installed"))?;
        let scheduler = self.state.host.hostRuntimeTaskSchedulerHost.as_ref().ok_or_else(|| error("Host scheduler is not installed"))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        scheduler.scheduleHostRuntimeTask("peer-discovery", Box::new(move || {
            let result = host.discover("_operit-link._tcp.local.", timeoutMs); let _ = tx.send(result);
        })).map_err(|e| error(e.to_string()))?;
        let records = rx.await.map_err(|_| error("Discovery cancelled"))?.map_err(|e| error(e.to_string()))?;
        let mut peers = BTreeMap::new();
        for record in records {
            let Some(node) = record.properties.get("nodeId").filter(|n| *n != &self.state.nodeId) else { continue; };
            let modes = record.properties.get("transports").map(String::as_str).unwrap_or("tcp");
            for mode in modes.split(',') {
                for ip in &record.addresses {
                    let socket = std::net::SocketAddr::new(*ip, record.port).to_string();
                    let address = match mode { "http" => format!("http://{socket}/link"), "ws" => format!("ws://{socket}/link"), "tcp" => socket, _ => continue };
                    peers.insert((node.clone(), address.clone()), DiscoveredPeer { nodeId: node.clone(), address,
                        displayName: record.properties.get("displayName").cloned().unwrap_or_else(|| node.clone()) });
                }
            }
        }
        Ok(peers.into_values().collect())
    }
    async fn startPairing(&self, target: PeerEndpoint, transport: PeerTransport, token: Option<&str>) -> Result<PendingPairing, CoreLinkError> {
        let raw = self.raw(target.clone(), transport).await?;
        let result = self.handshake(raw.clone(), "start", "", None, token).await;
        raw.close().await;
        let (_, id, info, root, node) = result?;
        {
            let _guard = self.state.mutation.lock().unwrap();
            self.state.store.putRecord(RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH, &id, &Pending {
                version: PAIRING_SERVICE_VERSION, id: id.clone(), clientDeviceId: self.state.nodeId.clone(), peerNodeId: node.clone(), endpoint: target.address,
                transport, info: info.clone(), root, expires: currentTimeMillis() + PAIRING_LIFETIME_MS, attempts: 0, confirmationCode: None,
            }).map_err(error)?;
        }
        self.changed(); Ok(PendingPairing { pairingId: id, peerNodeId: node, displayName: info.displayName() })
    }
    async fn finishPairing(&self, id: &str, code: &str) -> Result<PairedPeer, CoreLinkError> {
        let pending = {
            let _guard = self.state.mutation.lock().unwrap();
            let mut p = self.pending(false, id)?;
            if p.attempts >= 5 { return Err(error("Confirmation attempt limit reached")); }
            p.attempts += 1;
            self.state.store.putRecord(RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH, id, &p).map_err(error)?;
            // 前六位只由接收端随机生成；后八位绑定交换 transcript，阻止 MITM 偷换身份。
            // 主动端不能自行计算完整验证码，匿名响应中也绝不包含它。
            if code.len() != 14 || !code.bytes().all(|c| c.is_ascii_digit()) || !code.ends_with(&crypto::code(&p.root)) {
                return Err(error("Confirmation code does not match the exchanged identities/keys"));
            }
            p
        };
        let raw = self.raw(PeerEndpoint { nodeId: pending.peerNodeId.clone(), address: pending.endpoint.clone() }, pending.transport).await?;
        let result = async {
            let (channel, _, _, _, _) = self.handshake(raw.clone(), "finish", id, Some(&pending.root), None).await?;
            let request = CoreCallRequest::new(uuid::Uuid::new_v4().to_string(), HANDSHAKE, "finish",
                toCoreValue(crypto::proof(&pending.root, id.as_bytes(), code.as_bytes())).map_err(|e| error(e.to_string()))?);
            match channel.exchange(CoreLinkRequest::Call(request.clone())).await? {
                CoreLinkResponse::Call(r) if r.requestId == request.requestId => { r.result?; },
                _ => return Err(error("Confirmation response mismatch")),
            }
            Ok::<_, CoreLinkError>(())
        }.await;
        raw.close().await; result?;
        {
            let _guard = self.state.mutation.lock().unwrap();
            self.pending(false, id)?; // 撤销/取消不能被正在完成的网络事务重新授予权限。
            self.state.store.putRecord(RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH, id, &StoredOutbound {
                endpoint: pending.endpoint, sessionId: id.into(), deviceId: self.state.nodeId.clone(), peerNodeId: pending.peerNodeId.clone(),
                peerDeviceInfo: pending.info.clone(), pairingServiceVersion: PAIRING_SERVICE_VERSION,
                sessionSecret: BASE64.encode(&pending.root), transport: transportName(pending.transport).into(),
            }).map_err(error)?;
            self.state.store.deleteRecord(RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH, id).map_err(error)?;
            self.state.active.lock().unwrap().insert(pending.peerNodeId.clone());
        }
        self.changed();
        Ok(PairedPeer { nodeId: pending.peerNodeId, displayName: pending.info.displayName(), inbound: false, outbound: true })
    }
    async fn cancelPairing(&self, id: &str) -> Result<(), CoreLinkError> {
        let _guard = self.state.mutation.lock().unwrap();
        self.state.store.deleteRecord(RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH, id).map_err(error)?;
        self.changed(); Ok(())
    }
    async fn startListening(&self, transports: &[PeerTransport]) -> Result<(), CoreLinkError> {
        let config = self.state.store.hostConfig().map_err(error)?.ok_or_else(|| error("Configure bindAddress/token/transports before listening"))?;
        let scheduler = self.state.host.hostRuntimeTaskSchedulerHost.clone().ok_or_else(|| error("Host scheduler is not installed"))?;
        let mut listeners = self.state.listeners.lock().await;
        for transport in transports {
            if listeners.contains_key(transport) { continue; }
            let listener = self.state.link.listen(self.state.host.clone(), PeerEndpoint {
                nodeId: self.state.nodeId.clone(), address: config.bindAddress.clone(),
            }, *transport).await.map_err(error)?;
            let service = self.clone(); let accepting = listener.clone(); let tasks = scheduler.clone();
            scheduler.scheduleHostRuntimeAsyncTask("peer-listen", Box::new(move || Box::pin(async move {
                while let Ok(Some(raw)) = accepting.accept().await {
                    let Ok(permit) = service.state.slots.clone().try_acquire_owned() else { raw.close().await; continue; };
                    service.track("", &raw);
                    let service = service.clone(); let connection = raw.clone();
                    if tasks.scheduleHostRuntimeAsyncTask("peer-connection", Box::new(move || Box::pin(async move {
                        let _permit = permit;
                        let result = service.serve(connection.clone()).await;
                        if let Err(e) = result { operit_util::AppLogger::AppLogger::w("RuntimePeerService", &e.to_string()); }
                        connection.close().await;
                    }))).is_err() { raw.close().await; }
                }
            }))).map_err(|e| error(e.to_string()))?;
            listeners.insert(*transport, listener);
        }
        if config.discoveryEnabled && self.state.advertisements.lock().unwrap().is_empty() {
            let host = self.state.host.serviceDiscoveryHost.as_ref().ok_or_else(|| error("Discovery enabled but Host not installed"))?;
            let socket: std::net::SocketAddr = config.bindAddress.parse().map_err(|_| error("Discovery requires a concrete network bindAddress"))?;
            let properties = [("nodeId".into(), self.state.nodeId.clone()), ("displayName".into(), self.state.info.displayName()),
                ("transports".into(), listeners.keys().map(|t| transportName(*t)).collect::<Vec<_>>().join(","))].into();
            let advertisement = host.advertise(operit_host_api::ServiceDiscovery::ServiceAdvertisement {
                serviceType: "_operit-link._tcp.local.".into(), instance: self.state.nodeId.clone(),
                hostname: format!("{}.local.", self.state.nodeId), port: socket.port(), properties,
            }).map_err(|e| error(e.to_string()))?;
            self.state.advertisements.lock().unwrap().push(advertisement);
        }
        self.changed(); Ok(())
    }
    async fn stop(&self) -> Result<(), CoreLinkError> {
        self.state.advertisements.lock().unwrap().clear();
        let listeners = std::mem::take(&mut *self.state.listeners.lock().await);
        for listener in listeners.into_values() { listener.close().await; }
        let connections = std::mem::take(&mut *self.state.connections.lock().unwrap());
        for raw in connections.into_values().flatten().filter_map(|v| v.upgrade()) { raw.close().await; }
        self.state.active.lock().unwrap().clear(); self.changed(); Ok(())
    }
    async fn call(&self, node: &str, request: RoutedCoreRequest<CoreCallRequest>) -> CoreCallResponse {
        let id = request.payload.requestId.clone();
        let result = async {
            let channel = self.connectAuthorized(node).await?;
            let wire = dispatch::routedCall(request)?;
            let result = channel.exchange(CoreLinkRequest::Call(wire)).await;
            channel.raw.close().await;
            match result? { CoreLinkResponse::Call(r) if r.requestId == id => r.result, _ => Err(error("Call response mismatch")) }
        }.await;
        if result.is_err() && self.state.active.lock().unwrap().remove(node) { self.changed(); }
        CoreCallResponse { requestId: id, result }
    }
    async fn watchSnapshot(&self, node: &str, request: RoutedCoreRequest<CoreWatchRequest>) -> Result<CoreEvent, CoreLinkError> {
        dispatch::watchSnapshot(self, node, request).await
    }
    async fn watch(&self, node: &str, request: RoutedCoreRequest<CoreWatchRequest>) -> Result<CoreEventStream, CoreLinkError> {
        dispatch::watch(self, node, request).await
    }
    async fn openPush(&self, node: &str, request: RoutedCoreRequest<CorePushRequest>) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
        dispatch::openPush(self, node, request).await
    }
    fn pairedPeers(&self) -> Result<Vec<PairedPeer>, CoreLinkError> { self.state.store.pairedPeers(&self.state.nodeId).map_err(error) }
    fn outboundPeerNodeIds(&self) -> Result<BTreeSet<String>, CoreLinkError> { Ok(self.pairedPeers()?.into_iter().filter(|p| p.outbound).map(|p| p.nodeId).collect()) }
    fn activePeerNodeIds(&self) -> Result<BTreeSet<String>, CoreLinkError> { Ok(self.state.active.lock().unwrap().clone()) }
    fn subscribePeerChanges(&self) -> broadcast::Receiver<()> { self.state.changes.subscribe() }
    fn pairingPrompts(&self) -> Result<Vec<PairingPrompt>, CoreLinkError> {
        Ok(self.pendingRecords(RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH)?.into_values()
            .filter(|p| p.expires > currentTimeMillis()).map(|p| PairingPrompt { pairingId: p.id,
                peerNodeId: p.clientDeviceId, displayName: p.info.displayName(), confirmationCode: p.confirmationCode.unwrap_or_default() }).collect())
    }
    async fn disconnectPeer(&self, node: &str) -> Result<(), CoreLinkError> {
        let connections = self.state.connections.lock().unwrap().remove(node).unwrap_or_default();
        for raw in connections.into_iter().filter_map(|v| v.upgrade()) { raw.close().await; }
        self.state.active.lock().unwrap().remove(node); self.changed(); Ok(())
    }
    async fn removePairedPeer(&self, node: &str) -> Result<(), CoreLinkError> {
        { let _guard = self.state.mutation.lock().unwrap(); self.state.store.removePairedPeer(node).map_err(error)?; }
        self.disconnectPeer(node).await
    }
}
