//! 只按 transport 选择 Host 适配，不处理鉴权或配对。
mod bluetooth;
mod http;
mod inbox;
mod serial;
mod stream;
mod tcp;
mod websocket;

use crate::{PeerConnection, PeerEndpoint, PeerLink, PeerListener, PeerTransport};
use async_trait::async_trait;
use operit_host_api::HostManager::HostManager;
use std::sync::Arc;

#[derive(Default)]
pub struct HostPeerLink {
    webServers: http::ServerRegistry,
}

#[async_trait]
impl PeerLink for HostPeerLink {
    async fn connect(
        &self,
        host: Arc<HostManager>,
        source: PeerEndpoint,
        target: PeerEndpoint,
        transport: PeerTransport,
    ) -> Result<Arc<dyn PeerConnection>, String> {
        match transport {
            PeerTransport::Tcp => tcp::connect(&host, source, target).await,
            PeerTransport::Serial => serial::connect(&host, source, target).await,
            PeerTransport::Http => http::connect(&host, source, target).await,
            PeerTransport::WebSocket => websocket::connect(&host, source, target).await,
            PeerTransport::Bluetooth => bluetooth::connect(&host, source, target).await,
        }
    }
    async fn listen(
        &self,
        host: Arc<HostManager>,
        source: PeerEndpoint,
        transport: PeerTransport,
    ) -> Result<Arc<dyn PeerListener>, String> {
        match transport {
            PeerTransport::Tcp => tcp::listen(&host, source).await,
            PeerTransport::Serial => serial::listen(&host, source).await,
            PeerTransport::Http => http::listen(&self.webServers, &host, source).await,
            PeerTransport::WebSocket => websocket::listen(&self.webServers, &host, source).await,
            PeerTransport::Bluetooth => bluetooth::listen(&host, source).await,
        }
    }
}
