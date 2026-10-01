//! HTTP server capability; implementations and sockets belong to platform Hosts.
use crate::{HostError, HostResult};
use async_trait::async_trait;
use bytes::Bytes;
use http::{Request, Response};
use http_body_util::combinators::UnsyncBoxBody;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
pub type ServerBody = UnsyncBoxBody<Bytes, HostError>;
pub type ServerRequest = Request<ServerBody>;
pub type ServerResponse = Response<ServerBody>;
pub type ServerFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
pub type HttpServerHandler =
    Arc<dyn Fn(ServerRequest) -> ServerFuture<ServerResponse> + Send + Sync>;
pub type WebSocketHandler = Box<dyn FnOnce(Box<dyn ServerWebSocket>) -> ServerFuture<()> + Send>;
#[derive(Clone, Debug)]
pub enum WebSocketMessage {
    Binary(Vec<u8>),
    Text(String),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<(u16, String)>),
}
#[async_trait]
pub trait ServerWebSocket: Send {
    async fn send(&mut self, message: WebSocketMessage) -> HostResult<()>;
    async fn recv(&mut self) -> Option<HostResult<WebSocketMessage>>;
}
/// A one-shot upgrade offered by the Host; the application decides whether to accept it.
#[derive(Clone)]
pub struct WebSocketUpgrade(
    pub Arc<Mutex<Option<Box<dyn FnOnce(WebSocketHandler) -> ServerResponse + Send>>>>,
);
impl WebSocketUpgrade {
    pub fn accept(self, handler: WebSocketHandler) -> HostResult<ServerResponse> {
        let upgrade = self
            .0
            .lock()
            .map_err(|e| HostError::new(e.to_string()))?
            .take()
            .ok_or_else(|| HostError::new("WebSocket upgrade already consumed"))?;
        Ok(upgrade(handler))
    }
}
#[async_trait]
pub trait HttpServerListener: Send + Sync {
    fn localAddress(&self) -> HostResult<std::net::SocketAddr>;
    async fn serve(&self, handler: HttpServerHandler, shutdown: ServerFuture<()>)
        -> HostResult<()>;
}
#[async_trait]
pub trait HttpServerHost: Send + Sync {
    async fn bind(&self, address: &str) -> HostResult<Arc<dyn HttpServerListener>>;
}
