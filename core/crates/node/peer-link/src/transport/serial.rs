//! 串口适配；端口 I/O 由传入的 Host 提供。
use super::stream::{ByteConnection, FramedPeerConnection};
use crate::{PeerConnection, PeerEndpoint, PeerListener, PeerTransport};
use async_trait::async_trait;
use operit_host_api::HostManager::HostManager;
use std::sync::Arc;

struct SerialBytes(Arc<dyn operit_host_api::SerialPort::SerialPortConnection>);
#[async_trait]
impl ByteConnection for SerialBytes {
    async fn write(&self, bytes: &[u8]) -> Result<(), String> {
        self.0.write(bytes).await.map_err(|e| e.to_string())
    }
    async fn read(&self) -> Result<Option<Vec<u8>>, String> {
        self.0.read().await.map_err(|e| e.to_string())
    }
    async fn close(&self) {
        self.0.close().await;
    }
}

pub(super) async fn connect(
    host: &HostManager,
    source: PeerEndpoint,
    target: PeerEndpoint,
) -> Result<Arc<dyn PeerConnection>, String> {
    let provider = host
        .serialPortHost
        .as_ref()
        .ok_or("Serial Host is not installed")?;
    let connection = provider
        .open(&target.address, 115200)
        .await
        .map_err(|e| e.to_string())?;
    Ok(FramedPeerConnection::new(
        source,
        target,
        PeerTransport::Serial,
        Arc::new(SerialBytes(connection)),
    ))
}
pub(super) async fn listen(
    _host: &HostManager,
    _source: PeerEndpoint,
) -> Result<Arc<dyn PeerListener>, String> {
    Err("Serial transport is connect-only; the Host owns port discovery".into())
}
