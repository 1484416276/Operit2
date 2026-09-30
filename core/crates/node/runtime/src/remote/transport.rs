//! Authentication/session adapters for the shared peer transports.
use super::{signSession, PeerTransport, PairedRemoteSession, RemoteWsConnection, RemoteWsPayload, RemoteWsResponse};
use async_trait::async_trait;
use operit_peer_link::transport::{
    http::{self, HttpPeerSession},
    websocket::{self, WebSocketPeerSession},
};
use operit_peer_link::{CoreNodeTransportClient, PeerFrame, PeerLinkClient};
use operit_store::CoreSpaceStore::CoreSpaceStore;
use std::sync::Arc;

impl HttpPeerSession for PairedRemoteSession {
    fn localNodeId(&self) -> String {
        self.deviceId.clone()
    }
    fn peerNodeId(&self) -> String {
        self.peerNodeId.clone()
    }
    fn sessionId(&self) -> String {
        self.sessionId.clone()
    }
    fn baseUrl(&self) -> &str {
        &self.endpoint
    }
    fn sign(&self, bytes: &[u8]) -> String {
        signSession(&self.sessionSecret, bytes)
    }
    fn signedRemotePost(&self, path: &str, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
        PairedRemoteSession::signedRemotePost(self, path, bytes)
    }
}
struct AuthenticatedWebSocket {
    local: String,
    peer: String,
    socket: Arc<RemoteWsConnection>,
}
#[async_trait]
impl WebSocketPeerSession for AuthenticatedWebSocket {
    fn localNodeId(&self) -> String {
        self.local.clone()
    }
    fn peerNodeId(&self) -> String {
        self.peer.clone()
    }
    async fn openPeerChannel(&self) -> Result<String, String> {
        let channelId = format!("peer-channel-{}", uuid::Uuid::new_v4().simple());
        self.socket.sendPayload(RemoteWsPayload::PeerChannelOpen(
            operit_peer_link::PeerChannelOpenEnvelope { channelId },
        ))?;
        match self.socket.nextResponse().await? {
            RemoteWsResponse::PeerOpened(channelId) => Ok(channelId),
            RemoteWsResponse::Error(error) => Err(error.to_string()),
            _ => Err("unexpected WebSocket Peer Link open response".into()),
        }
    }
    async fn nextPeerFrame(&self) -> Result<Option<PeerFrame>, String> {
        loop {
            match self.socket.nextResponse().await? {
                RemoteWsResponse::PeerFrame(frame) => return Ok(Some(frame)),
                RemoteWsResponse::PeerClosed(_) => return Ok(None),
                RemoteWsResponse::Error(error) => return Err(error.to_string()),
                _ => {}
            }
        }
    }
    fn sendPeerFrame(&self, channelId: String, frame: PeerFrame) -> Result<(), String> {
        self.socket
            .sendPayload(RemoteWsPayload::PeerFrame { channelId, frame })
    }
    fn close(&self) {
        let _ = self.socket.close();
    }
}
pub async fn openOutboundPeerLink(
    session: PairedRemoteSession,
    core: Arc<dyn CoreNodeTransportClient>,
    topology: CoreSpaceStore,
) -> Result<PeerLinkClient, String> {
    match session.transport {
        PeerTransport::Http => {
            http::open(
                Arc::new(session),
                core,
                Some(Arc::new(super::topology::SpacePeerObserver(topology))),
            )
            .await
        }
        PeerTransport::WebSocket => {
            let socket = RemoteWsConnection::open(&session, "peer").await?;
            websocket::open(
                Arc::new(AuthenticatedWebSocket {
                    local: session.deviceId,
                    peer: session.peerNodeId,
                    socket,
                }),
                core,
                Some(Arc::new(super::topology::SpacePeerObserver(topology))),
            )
            .await
        }
        PeerTransport::Tcp | PeerTransport::Serial => Err(
            "TCP/serial peer session opening is not implemented".into(),
        ),
    }
}
