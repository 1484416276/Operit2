//! Physical/protocol carriers, independent of device roles.
use async_trait::async_trait;
use std::sync::Arc;
use crate::LinkFrame;
#[async_trait]
pub trait LinkChannel: Send + Sync {
    async fn send(&self, frame: LinkFrame) -> Result<(), String>;
    async fn receive(&self) -> Result<Option<LinkFrame>, String>;
    async fn close(&self);
}
pub mod http;
#[cfg(feature = "serial")]
pub mod serial;
pub mod serial_codec;
pub mod websocket;
pub mod duplex;
pub mod channel;

/// Opens one Peer carrier from the user-facing endpoint string.
///
/// TCP remains the default (`192.168.1.20:8765`). USB/UART endpoints use
/// `serial://COM27` or `serial://COM27?baud=115200` and share the exact same
/// Link pairing and authenticated frame layer.
#[cfg(all(feature = "tcp", feature = "serial"))]
pub async fn connectChannel(endpoint: &str) -> Result<Arc<dyn LinkChannel>, String> {
    if let Some(serialEndpoint) = endpoint.strip_prefix("serial://") {
        let (port, query) = serialEndpoint
            .split_once('?')
            .unwrap_or((serialEndpoint, ""));
        if port.trim().is_empty() {
            return Err("Peer serial endpoint must contain a port name".to_string());
        }
        let baudRate = query
            .split('&')
            .find_map(|part| part.strip_prefix("baud="))
            .map(|value| {
                value
                    .parse::<u32>()
                    .map_err(|error| format!("invalid Peer serial baud rate: {error}"))
            })
            .transpose()?
            .unwrap_or(115_200);
        let host = operit_host_api::HostManager::defaultSerialPortHost()
            .map_err(|error| error.to_string())?;
        let channel = serial::SerialLinkChannel::open(
            host.as_ref(),
            port,
            baudRate,
        )
        .await?;
        return Ok(channel);
    }
    Err("The obsolete TCP LinkFrame protocol has been removed".into())
}

/// Closes an opened carrier when a handshake fails or is cancelled.
pub struct ChannelGuard(pub Option<Arc<dyn LinkChannel>>);
impl Drop for ChannelGuard {
    fn drop(&mut self) {
        if let Some(channel) = self.0.take() {
            let _ = operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost()
                .scheduleHostRuntimeAsyncTask("peer-handshake-close", Box::new(move || {
                    Box::pin(async move { channel.close().await })
                }));
        }
    }
}
