//! WebSocket adapter for the shared full-duplex PeerLink lifecycle.
use super::duplex::{self, DuplexPeerSession};
use crate::{CoreNodeTransportClient, PeerFrame, PeerLinkClient, observer::PeerLinkObserver};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait WebSocketPeerSession: Send + Sync {
    fn localNodeId(&self) -> String;
    fn peerNodeId(&self) -> String;
    async fn openPeerChannel(&self) -> Result<String, String>;
    async fn nextPeerFrame(&self) -> Result<Option<PeerFrame>, String>;
    fn sendPeerFrame(&self, channelId: String, frame: PeerFrame) -> Result<(), String>;
    fn close(&self);
}
struct WebSocketSession(Arc<dyn WebSocketPeerSession>);
#[async_trait]
impl DuplexPeerSession for WebSocketSession {
    fn localNodeId(&self) -> String { self.0.localNodeId() }
    fn peerNodeId(&self) -> String { self.0.peerNodeId() }
    async fn openPeerChannel(&self) -> Result<String, String> { self.0.openPeerChannel().await }
    async fn nextPeerFrame(&self) -> Result<Option<PeerFrame>, String> { self.0.nextPeerFrame().await }
    async fn sendPeerFrame(&self, channelId: String, frame: PeerFrame) -> Result<(), String> {
        self.0.sendPeerFrame(channelId, frame)
    }
    fn close(&self) { self.0.close(); }
}
pub async fn open(
    session: Arc<dyn WebSocketPeerSession>,
    core: Arc<dyn CoreNodeTransportClient>,
    observer: Option<Arc<dyn PeerLinkObserver>>,
) -> Result<PeerLinkClient, String> {
    duplex::open(Arc::new(WebSocketSession(session)), core, observer).await
}
