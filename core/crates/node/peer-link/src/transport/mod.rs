//! 只按 transport 选择 Host 适配，不处理鉴权或配对。
mod bluetooth;
mod http;
mod inbox;
mod serial;
mod stream;
mod tcp;
mod websocket;

use crate::{PeerConnection, PeerEndpoint, PeerLink, PeerListener, PeerListenerCapabilities, PeerTransport};
use async_trait::async_trait;
use operit_host_api::HostManager::HostManager;
use std::sync::Arc;

#[derive(Default)]
pub struct HostPeerLink {
    webServers: http::ServerRegistry,
}

impl HostPeerLink {
    /// Derives each inbound transport from its installed Host operation contract.
    pub fn listenerCapabilities(host: &HostManager) -> PeerListenerCapabilities {
        let mut transports = Vec::new();
        if let Some(server) = &host.httpServerHost {
            transports.push(PeerTransport::Http);
            if server.supportsWebSocketUpgrade() {
                transports.push(PeerTransport::WebSocket);
            }
        }
        if host.tcpHost.is_some() {
            transports.push(PeerTransport::Tcp);
        }
        // Serial ports expose outbound streams, not a peer accept/listen contract.
        if host.bluetoothHost.as_ref().is_some_and(|host| host.supportsClassicListening()) {
            transports.push(PeerTransport::Bluetooth);
        }
        PeerListenerCapabilities { transports, discoveryAdvertisement: host.serviceDiscoveryHost.as_ref().is_some_and(|host| host.supportsAdvertisement()) }
    }
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
    /// Rejects unsupported inbound transports before acquiring any Host resource.
    async fn listen(
        &self,
        host: Arc<HostManager>,
        source: PeerEndpoint,
        transport: PeerTransport,
    ) -> Result<Arc<dyn PeerListener>, String> {
        if !Self::listenerCapabilities(&host).transports.contains(&transport) {
            return Err(format!("Host does not support {transport:?} peer listeners"));
        }
        match transport {
            PeerTransport::Tcp => tcp::listen(&host, source).await,
            PeerTransport::Serial => serial::listen(&host, source).await,
            PeerTransport::Http => http::listen(&self.webServers, &host, source).await,
            PeerTransport::WebSocket => websocket::listen(&self.webServers, &host, source).await,
            PeerTransport::Bluetooth => bluetooth::listen(&host, source).await,
        }
    }
}
