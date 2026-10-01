#![allow(non_snake_case)]
//! Transport-only contract for carrying standard Link operations between runtimes.
//! No implementation, pairing, authorization, routing, persistence or global Host lookup.

use async_trait::async_trait;
use operit_host_api::HostManager::HostManager;
use operit_link::{CoreLinkRequest, CoreLinkResponse};
use std::sync::Arc;
use serde::{Serialize, Deserialize};

/// Selects a Host transport, not a different application protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PeerTransport {
    Http,
    WebSocket,
    Tcp,
    Serial,
    Bluetooth,
}

/// A runtime-supplied identity and transport address (URL, socket address or device address).
/// The node ID is addressing metadata, never proof of identity or authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerEndpoint {
    pub nodeId: String,
    pub address: String,
}

/// Local API union only: this does not define an additional serialized wire envelope.
/// Requests and responses retain the existing Call / Watch / Push protocol.
pub enum PeerMessage {
    Request(CoreLinkRequest),
    Response(CoreLinkResponse),
}

/// One bidirectional connection. Runtime consumes messages and decides how to dispatch them.
/// Implementations must preserve message order and partial input across receive cancellation.
#[async_trait]
pub trait PeerConnection: Send + Sync {
    fn source(&self) -> &PeerEndpoint;
    fn target(&self) -> &PeerEndpoint;
    fn transport(&self) -> PeerTransport;

    async fn send(&self, message: PeerMessage) -> Result<(), String>;
    /// None means the connection has ended, not that no message is currently available.
    async fn receive(&self) -> Result<Option<PeerMessage>, String>;
    /// Idempotent; wakes pending I/O. Dropping a connection must also release Host resources.
    async fn close(&self);
}

/// Accepts connections only; accepting never implies successful authentication or pairing.
#[async_trait]
pub trait PeerListener: Send + Sync {
    /// Accepted connections use source = local endpoint, target = remote endpoint.
    /// None means the listener has closed. Remote identity remains untrusted runtime input.
    async fn accept(&self) -> Result<Option<Arc<dyn PeerConnection>>, String>;
    /// Stops acceptance; already accepted connections have their own lifetime.
    /// Dropping a listener must also release its Host resources.
    async fn close(&self);
}

/// Future transport implementation boundary. These are declarations, not working backends.
/// All I/O must use the explicitly supplied Host; no platform-specific runtime is required here.
#[async_trait]
pub trait PeerLink: Send + Sync {
    async fn connect(
        &self,
        host: Arc<HostManager>,
        source: PeerEndpoint,
        target: PeerEndpoint,
        transport: PeerTransport,
    ) -> Result<Arc<dyn PeerConnection>, String>;

    /// Only the local endpoint is needed to listen; the remote endpoint arrives on accept.
    async fn listen(
        &self,
        host: Arc<HostManager>,
        source: PeerEndpoint,
        transport: PeerTransport,
    ) -> Result<Arc<dyn PeerListener>, String>;
}
