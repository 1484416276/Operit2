//! Board UART byte I/O. No pairing, Link message, codec or application dependency.
use async_trait::async_trait;
use operit_host_api::{HostError, HostResult, SerialPortConnection, SerialPortHost};
use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use std::sync::{Arc, Mutex as StdMutex, Weak, atomic::{AtomicBool, Ordering}};
use tokio::sync::Mutex;

// Nonblocking operations keep the async executor responsive. This small driver
// boundary also lets native tests exercise exactly the board connection lifecycle.
trait UartIo: Send + Sync {
    fn read(&self, bytes: &mut [u8]) -> HostResult<usize>;
    fn write(&self, bytes: &[u8]) -> HostResult<usize>;
    fn configure(&self, baud: u32) -> HostResult<()>;
}

/// Owns the UART driver independently of any protocol or admitted session.
/// One open connection leases the port; close/drop permits a fresh connection.
pub struct Esp32SerialPortHost {
    port: String,
    uart: Arc<dyn UartIo>,
    active: StdMutex<Weak<UartConnection>>,
}
impl Esp32SerialPortHost {
    #[cfg(target_os = "espidf")]
    pub fn new(port: impl Into<String>, uart: esp_idf_hal::uart::UartDriver<'static>) -> Self {
        Self { port: port.into(), uart: Arc::new(uart), active: StdMutex::new(Weak::new()) }
    }
}

#[cfg(target_os = "espidf")]
impl UartIo for esp_idf_hal::uart::UartDriver<'static> {
    fn read(&self, bytes: &mut [u8]) -> HostResult<usize> {
        self.read(bytes, 0).map_err(|error| HostError::new(error.to_string()))
    }
    fn write(&self, bytes: &[u8]) -> HostResult<usize> {
        self.write_nb(bytes).map_err(|error| HostError::new(error.to_string()))
    }
    fn configure(&self, baud: u32) -> HostResult<()> {
        self.change_baudrate(baud).map(|_| ()).map_err(|error| HostError::new(error.to_string()))
    }
}

#[async_trait]
impl SerialPortHost for Esp32SerialPortHost {
    async fn open(&self, port: &str, baud_rate: u32) -> HostResult<Arc<dyn SerialPortConnection>> {
        if port != self.port || baud_rate == 0 {
            return Err(HostError::new("Invalid board UART port or baud rate"));
        }
        let mut active = self.active.lock().map_err(|error| HostError::new(error.to_string()))?;
        if active.upgrade().is_some_and(|connection| !connection.closed.load(Ordering::Acquire)) {
            return Err(HostError::new("Board UART port is already open"));
        }
        self.uart.configure(baud_rate)?;
        let connection = Arc::new(UartConnection {
            uart: self.uart.clone(), reader: Mutex::new(()), writer: Mutex::new(()),
            io: StdMutex::new(()), closed: AtomicBool::new(false),
        });
        *active = Arc::downgrade(&connection);
        Ok(connection)
    }
}

struct UartConnection {
    uart: Arc<dyn UartIo>,
    reader: Mutex<()>,
    writer: Mutex<()>,
    // close serializes against short nonblocking driver calls. An old connection
    // can never touch the UART after a new connection has acquired the lease.
    io: StdMutex<()>,
    closed: AtomicBool,
}
#[async_trait]
impl SerialPortConnection for UartConnection {
    async fn read(&self) -> HostResult<Option<Vec<u8>>> {
        let _reader = self.reader.lock().await;
        let mut bytes = vec![0; 4096];
        loop {
            let count = {
                let _io = self.io.lock().map_err(|error| HostError::new(error.to_string()))?;
                if self.closed.load(Ordering::Acquire) { return Ok(None); }
                self.uart.read(&mut bytes)?
            };
            if count > 0 {
                bytes.truncate(count);
                return Ok(Some(bytes));
            }
            defaultHostRuntimeTaskSchedulerHost().waitForHostRuntimeDelay(2).await?;
        }
    }
    async fn write(&self, bytes: &[u8]) -> HostResult<()> {
        let _writer = self.writer.lock().await;
        let mut offset = 0;
        loop {
            let count = {
                let _io = self.io.lock().map_err(|error| HostError::new(error.to_string()))?;
                if self.closed.load(Ordering::Acquire) { return Err(HostError::new("Board UART connection is closed")); }
                if offset == bytes.len() { return Ok(()); }
                self.uart.write(&bytes[offset..])?
            };
            offset += count;
            if count == 0 { defaultHostRuntimeTaskSchedulerHost().waitForHostRuntimeDelay(2).await?; }
        }
    }
    async fn close(&self) {
        let _io = self.io.lock().unwrap_or_else(|error| error.into_inner());
        self.closed.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::future::{poll_fn, Future};
    use std::task::Poll;

    #[derive(Default)]
    struct Uart { input: StdMutex<VecDeque<u8>>, output: StdMutex<Vec<u8>> }
    impl UartIo for Uart {
        fn read(&self, bytes: &mut [u8]) -> HostResult<usize> {
            let mut input = self.input.lock().unwrap();
            let count = bytes.len().min(input.len());
            for byte in &mut bytes[..count] { *byte = input.pop_front().unwrap(); }
            Ok(count)
        }
        fn write(&self, bytes: &[u8]) -> HostResult<usize> {
            let count = bytes.len().min(3);
            self.output.lock().unwrap().extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn configure(&self, _: u32) -> HostResult<()> { Ok(()) }
    }
    fn host() -> (Esp32SerialPortHost, Arc<Uart>) {
        operit_host_api::HostManager::setDefaultHostRuntimeTaskSchedulerHost(Arc::new(
            operit_host_native_scheduler::NativeHostRuntimeTaskSchedulerHost,
        ));
        let uart = Arc::new(Uart::default());
        (Esp32SerialPortHost { port: "uart0".into(), uart: uart.clone(), active: StdMutex::new(Weak::new()) }, uart)
    }
    #[tokio::test]
    async fn host_transfers_opaque_bytes_and_releases_port() {
        let (host, uart) = host();
        assert!(host.open("missing", 115200).await.is_err());
        assert!(host.open("uart0", 0).await.is_err());
        let connection = host.open("uart0", 115200).await.unwrap();
        assert!(host.open("uart0", 115200).await.is_err());
        connection.write(b"not a Link frame").await.unwrap();
        assert_eq!(*uart.output.lock().unwrap(), b"not a Link frame");
        uart.input.lock().unwrap().extend(b"raw input");
        assert_eq!(connection.read().await.unwrap().unwrap(), b"raw input");
        connection.close().await;
        connection.close().await;
        assert!(connection.read().await.unwrap().is_none());
        assert!(connection.write(b"stale").await.is_err());
        let replacement = host.open("uart0", 115200).await.unwrap();
        connection.close().await;
        replacement.write(b"replacement").await.unwrap();
        drop(replacement);
        assert!(host.open("uart0", 115200).await.is_ok());
    }
    #[tokio::test]
    async fn cancelled_read_keeps_bytes_in_driver() {
        let (host, uart) = host();
        let connection = host.open("uart0", 115200).await.unwrap();
        let mut pending = Box::pin(connection.read());
        poll_fn(|cx| { assert!(pending.as_mut().poll(cx).is_pending()); Poll::Ready(()) }).await;
        drop(pending);
        uart.input.lock().unwrap().extend(b"kept");
        assert_eq!(connection.read().await.unwrap().unwrap(), b"kept");
    }
}
