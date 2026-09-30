#![allow(non_snake_case)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use esp_idf_hal::delay;
use esp_idf_hal::uart::UartDriver;
use operit_peer_link::transport::serial_codec::{encodeSerialFrame, SerialFrameDecoder};
use operit_peer_link::transport::LinkChannel;
use operit_host_api::{HostError, HostResult};
use operit_peer_link::LinkFrame;
use tokio::sync::{mpsc, Mutex as AsyncMutex};

/// UART0 carrier for the USB-UART bridge used by the ESP32 board.
///
/// UART0 is also the ESP-IDF console. TX logs are therefore allowed on the
/// wire, while RX only accepts the framed EdgeLink protocol. The magic,
/// length, and CRC decoder ignores console bytes on the host side and keeps
/// this carrier usable without a second USB-UART adapter.
pub struct Esp32UartLinkChannel {
    uart: Arc<UartDriver<'static>>,
    writeLock: Mutex<()>,
    receiver: AsyncMutex<mpsc::Receiver<LinkFrame>>,
    closed: Arc<AtomicBool>,
    _reader: JoinHandle<()>,
}

impl Esp32UartLinkChannel {
    /// Starts a UART reader and returns the Link carrier used by the server.
    pub fn new(uart: UartDriver<'static>) -> HostResult<Arc<Self>> {
        let uart = Arc::new(uart);
        let closed = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel(4);
        let readerUart = Arc::clone(&uart);
        let readerClosed = Arc::clone(&closed);
        let reader = std::thread::Builder::new()
            .name("operit-edge-uart-reader".to_string())
            .spawn(move || {
                let mut decoder = SerialFrameDecoder::withMaxFrameBytes(48 * 1024);
                let mut bytes = vec![0u8; 1024];
                loop {
                    match readerUart.read(&mut bytes, delay::BLOCK) {
                        Ok(0) => continue,
                        Ok(count) => {
                            let mut input = &bytes[..count];
                            loop {
                                match decoder.push(input) {
                                    Ok(Some(frame)) => {
                                        if sender.blocking_send(frame).is_err() {
                                            readerClosed.store(true, Ordering::Release);
                                            return;
                                        }
                                    }
                                    Ok(None) => break,
                                    Err(error) => {
                                        log::warn!("ESP32 Edge UART frame: {error}");
                                        break;
                                    }
                                }
                                input = &[];
                            }
                        }
                        Err(error) => {
                            log::warn!("ESP32 Edge UART read: {error}");
                            readerClosed.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
            })
            .map_err(|error| HostError::new(format!("Edge UART reader thread: {error}")))?;
        Ok(Arc::new(Self {
            uart,
            writeLock: Mutex::new(()),
            receiver: AsyncMutex::new(receiver),
            closed,
            _reader: reader,
        }))
    }

    pub fn isClosed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

#[async_trait::async_trait]
impl LinkChannel for Esp32UartLinkChannel {
    async fn send(&self, frame: LinkFrame) -> Result<(), String> {
        let encoded = encodeSerialFrame(&frame)?;
        let _writeGuard = self
            .writeLock
            .lock()
            .map_err(|error| format!("ESP32 Edge UART write lock: {error}"))?;
        let mut offset = 0usize;
        while offset < encoded.len() {
            let count = self
                .uart
                .write(&encoded[offset..])
                .map_err(|error| format!("ESP32 Edge UART write: {error}"))?;
            if count == 0 {
                return Err("ESP32 Edge UART wrote zero bytes".to_string());
            }
            offset += count;
        }
        self.uart
            .wait_tx_done(delay::BLOCK)
            .map_err(|error| format!("ESP32 Edge UART transmit: {error}"))
    }

    async fn receive(&self) -> Result<Option<LinkFrame>, String> {
        Ok(self.receiver.lock().await.recv().await)
    }

    async fn close(&self) {
        // UART0 remains owned by the firmware for later reconnects.
    }
}
