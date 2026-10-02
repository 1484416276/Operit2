//! 字节流传输共用的消息边界处理；不含 socket、配对或路由。
use crate::{PeerConnection, PeerEndpoint, PeerMessage, PeerTransport};
use async_trait::async_trait;
use operit_link::{decodeLink, encodeLink};
use std::sync::Arc;
use tokio::sync::Mutex;

pub(super) const MAX_PEER_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[async_trait]
pub(super) trait ByteConnection: Send + Sync {
    fn remoteAddress(&self) -> Option<std::net::SocketAddr> { None }
    async fn write(&self, bytes: &[u8]) -> Result<(), String>;
    async fn read(&self) -> Result<Option<Vec<u8>>, String>;
    async fn close(&self);
}

pub(super) struct FramedPeerConnection {
    source: PeerEndpoint,
    target: PeerEndpoint,
    transport: PeerTransport,
    io: Arc<dyn ByteConnection>,
    writeLock: Mutex<()>,
    readLock: Mutex<()>,
    pending: Mutex<Vec<u8>>,
}
impl FramedPeerConnection {
    pub(super) fn new(
        source: PeerEndpoint,
        target: PeerEndpoint,
        transport: PeerTransport,
        io: Arc<dyn ByteConnection>,
    ) -> Arc<Self> {
        Arc::new(Self {
            source,
            target,
            transport,
            io,
            writeLock: Mutex::new(()),
            readLock: Mutex::new(()),
            pending: Mutex::new(Vec::new()),
        })
    }
    async fn readFrame(&self) -> Result<Option<Vec<u8>>, String> {
        let _guard = self.readLock.lock().await;
        let mut pending = self.pending.lock().await;
        loop {
            if pending.len() >= 4 {
                let length = u32::from_be_bytes(pending[..4].try_into().unwrap()) as usize;
                if length == 0 || length > MAX_PEER_MESSAGE_BYTES {
                    return Err(format!("invalid peer message length: {length}"));
                }
                if pending.len() >= length + 4 {
                    let frame = pending[4..length + 4].to_vec();
                    pending.drain(..length + 4);
                    return Ok(Some(frame));
                }
            }
            match self.io.read().await? {
                Some(bytes) if !bytes.is_empty() => {
                    if pending
                        .len()
                        .checked_add(bytes.len())
                        .filter(|n| *n <= MAX_PEER_MESSAGE_BYTES + 4)
                        .is_none()
                    {
                        return Err("peer receive buffer exceeded limit".into());
                    }
                    pending.extend_from_slice(&bytes);
                }
                Some(_) => continue,
                None if pending.is_empty() => return Ok(None),
                None => return Err("peer stream ended inside a message".into()),
            }
        }
    }
}
#[async_trait]
impl PeerConnection for FramedPeerConnection {
    fn source(&self) -> &PeerEndpoint {
        &self.source
    }
    fn target(&self) -> &PeerEndpoint {
        &self.target
    }
    fn transport(&self) -> PeerTransport {
        self.transport
    }
    fn remoteAddress(&self) -> Option<std::net::SocketAddr> { self.io.remoteAddress() }
    async fn send(&self, message: PeerMessage) -> Result<(), String> {
        let payload = encodeLink(message).map_err(|e| e.to_string())?;
        if payload.is_empty() || payload.len() > MAX_PEER_MESSAGE_BYTES {
            return Err("peer message exceeds limit".into());
        }
        let _guard = self.writeLock.lock().await;
        let mut frame = Vec::with_capacity(payload.len() + 4);
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);
        self.io.write(&frame).await
    }
    async fn receive(&self) -> Result<Option<PeerMessage>, String> {
        self.readFrame()
            .await?
            .map(|bytes| decodeLink(&bytes).map_err(|e| e.to_string()))
            .transpose()
    }
    async fn close(&self) {
        self.io.close().await;
    }
}
