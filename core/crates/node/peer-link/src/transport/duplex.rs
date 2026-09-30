use crate::{connection::*, observer::PeerLinkObserver};
use async_trait::async_trait;
use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Authenticated full-duplex I/O, without node or store dependencies.
#[async_trait]
pub trait DuplexPeerSession: Send + Sync {
    fn localNodeId(&self) -> String;
    fn peerNodeId(&self) -> String;
    async fn openPeerChannel(&self) -> Result<String, String>;
    async fn nextPeerFrame(&self) -> Result<Option<PeerFrame>, String>;
    async fn sendPeerFrame(&self, channelId: String, frame: PeerFrame) -> Result<(), String>;
    fn close(&self);
}
struct PendingPeerSession(Option<Arc<dyn DuplexPeerSession>>);
impl Drop for PendingPeerSession {
    fn drop(&mut self) {
        if let Some(connection) = self.0.take() {
            connection.close();
        }
    }
}
/// Opens and registers the full-duplex outbound Peer Link.
#[allow(non_snake_case)]
pub async fn open(
    session: Arc<dyn DuplexPeerSession>,
    core: Arc<dyn CoreNodeTransportClient>,
    topologyStore: Option<Arc<dyn PeerLinkObserver>>,
) -> Result<PeerLinkClient, String> {
    let localNodeId = session.localNodeId();
    let peerNodeId = session.peerNodeId();
    let mut pending = PendingPeerSession(Some(session.clone()));
    let channelId = session.openPeerChannel().await?;
    let sender = Arc::new(DuplexPeerSender {
        connection: session.clone(),
        channelId: channelId.clone(),
        closed: AtomicBool::new(false),
    });
    let connection = PeerConnection::new(
        localNodeId.clone(),
        peerNodeId.clone(),
        channelId,
        sender.clone(),
        core,
        topologyStore,
    );
    let frameDispatchConnection = connection.clone();
    let frameConnection = session.clone();
    defaultHostRuntimeTaskSchedulerHost()
        .scheduleHostRuntimeAsyncTask(
            "peer-duplex-ordered-receive",
            Box::new(move || {
                Box::pin(async move {
                    loop {
                        let response = match frameConnection.nextPeerFrame().await {
                            Ok(value) => value,
                            Err(error) => {
                                frameDispatchConnection.close(error);
                                return;
                            }
                        };
                        match response {
                            Some(frame) => {
                                if let Err(error) =
                                    frameDispatchConnection.receiveFrame(frame).await
                                {
                                    frameDispatchConnection
                                        .close(format!("Peer Link frame dispatch failed: {error}"));
                                    return;
                                }
                            }
                            None => {
                                frameDispatchConnection
                                    .close("Peer carrier closed".to_string());
                                return;
                            }
                        }
                    }
                })
            }),
        )
        .map_err(|error| {
            sender.close();
            error.to_string()
        })?;
    let client = registerPeerLink(connection)?;
    pending.0.take();
    Ok(client)
}

/// Sends Peer frames directly through one authenticated full-duplex carrier.
struct DuplexPeerSender {
    connection: Arc<dyn DuplexPeerSession>,
    channelId: String,
    closed: AtomicBool,
}

#[async_trait]
impl PeerFrameSender for DuplexPeerSender {
    /// Sends one ordered Peer frame without creating a per-frame HTTP request.
    async fn send(&self, frame: PeerFrame) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("Outbound Peer carrier is closed".to_string());
        }
        self.connection.sendPeerFrame(self.channelId.clone(), frame).await
    }

    /// Closes the carrier exactly once.
    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.connection.close();
    }
}
