#![allow(non_snake_case)]

use async_trait::async_trait;
use operit_link::{
    CoreCallRequest, CoreCallResponse, CoreEvent, CoreEventStream, CoreLinkError,
    CoreLinkClient, CoreLinkPushSession, CoreLinkSharedClient, CoreRouteRuntime, CorePushItem, CorePushRequest, CoreValue,
    CoreWatchRequest, LinkFrame, LinkFramePayload, PeerFrame, PeerFramePayload, PeerHeartbeat,
    PeerPushCloseRequest, PeerPushOpenRequest, PeerRequest, PeerResponse, PeerWatchCloseRequest,
    PeerWatchClosed, PeerWatchEvent, PeerWatchOpenRequest, RoutedCoreRequest,
    RoutedCoreRequestKind,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use uuid::Uuid;

pub mod auth;
pub mod pairing;
pub mod serial;
pub mod serial_codec;
pub mod tcp;

pub use auth::AuthenticatedLinkChannel;
pub use pairing::{
    finishPairAsClient, linkTokenHash, startPairAsClient, EdgePairStartState, EdgePairingAuthority,
    EdgePairingPersistentState, EdgePairingStore, EdgeSession, EDGE_PAIRING_SERVICE_VERSION,
};

/// Abstracts a bidirectional carrier while keeping all operations in Link types.
#[async_trait]
pub trait LinkChannel: Send + Sync {
    async fn send(&self, frame: LinkFrame) -> Result<(), String>;
    async fn receive(&self) -> Result<Option<LinkFrame>, String>;
    async fn close(&self);
}

struct EdgePeerState {
    carrier: EdgePeerFrameCarrier,
    pending: Mutex<BTreeMap<String, oneshot::Sender<Result<PeerResponse, CoreLinkError>>>>,
    watches: Mutex<BTreeMap<String, mpsc::UnboundedSender<CoreEvent>>>,
    requestHandler: Mutex<Option<Arc<dyn EdgePeerRequestHandler>>>,
    connected: AtomicBool,
}

/// Handles an inbound standard PeerLink request on an Edge. An Edge has no
/// built-in business capability; embeddings may use this only to relay an
/// explicitly supported request through another Space route.
#[async_trait]
pub trait EdgePeerRequestHandler: Send + Sync {
    async fn dispatchPeerRequest(&self, request: PeerRequest) -> PeerResponse;

    async fn dispatchPeerWatchEvent(&self, _event: PeerWatchEvent) {}

    async fn dispatchPeerWatchClosed(&self, _closed: PeerWatchClosed) {}
}

/// Frames a standard PeerLink message on an authenticated Edge Link carrier.
#[derive(Clone)]
pub struct EdgePeerFrameCarrier {
    channel: Arc<dyn LinkChannel>,
}

impl EdgePeerFrameCarrier {
    pub fn new(channel: Arc<dyn LinkChannel>) -> Self {
        Self { channel }
    }

    pub async fn sendPeerFrame(&self, frame: PeerFrame) -> Result<(), String> {
        let messageId = frame.messageId.clone();
        self.channel
            .send(LinkFrame {
                messageId,
                payload: LinkFramePayload::PeerFrame(frame),
            })
            .await
    }

    pub async fn receivePeerFrame(&self) -> Result<Option<PeerFrame>, String> {
        let Some(frame) = self.channel.receive().await? else {
            return Ok(None);
        };
        match frame.payload {
            LinkFramePayload::PeerFrame(frame) => Ok(Some(frame)),
            _ => Err("authenticated Edge carrier received a non-PeerLink frame".to_string()),
        }
    }

    pub async fn close(&self) {
        self.channel.close().await;
    }
}

/// Standard PeerLink endpoint carried by an authenticated Edge TCP or UART
/// channel. It shares the exact wire model used by CoreNode PeerLink, while
/// deliberately containing no HostManager or business-service dispatcher.
#[derive(Clone)]
pub struct EdgePeerLink {
    state: Arc<EdgePeerState>,
}

impl EdgePeerLink {
    /// Starts the standard PeerLink receive loop over an authenticated carrier.
    pub fn new(channel: Arc<dyn LinkChannel>) -> Self {
        let state = Arc::new(EdgePeerState {
            carrier: EdgePeerFrameCarrier::new(channel),
            pending: Mutex::new(BTreeMap::new()),
            watches: Mutex::new(BTreeMap::new()),
            requestHandler: Mutex::new(None),
            connected: AtomicBool::new(true),
        });
        let peer = Self { state };
        peer.startReceiver();
        peer
    }

    /// Installs the optional inbound relay capability. Normal Edge chat usage
    /// only sends requests to its adjacent full CoreNode and needs no handler.
    pub async fn installRequestHandler(&self, handler: Arc<dyn EdgePeerRequestHandler>) {
        *self.state.requestHandler.lock().await = Some(handler);
    }

    /// Forwards one incoming request to the adjacent full CoreNode. The
    /// caller owns the route decision; this method only preserves the exact
    /// standard PeerRequest/PeerResponse contract.
    pub async fn forwardPeerRequest(
        &self,
        request: PeerRequest,
    ) -> Result<PeerResponse, CoreLinkError> {
        self.request(request).await
    }

    /// Reports whether the authenticated PeerLink carrier remains active.
    pub fn isConnected(&self) -> bool {
        self.state.connected.load(Ordering::Acquire)
    }

    fn startReceiver(&self) {
        let peer = self.clone();
        tokio::spawn(async move {
            loop {
                let frame = match tokio::time::timeout(
                    std::time::Duration::from_secs(15), peer.state.carrier.receivePeerFrame(),
                ).await {
                    Ok(Ok(Some(frame))) => frame,
                    Ok(Ok(None)) => break,
                    Ok(Err(error)) => {
                        peer.failPending(error).await;
                        break;
                    }
                    Err(_) => {
                        peer.failPending("Space peer heartbeat expired".to_string()).await;
                        break;
                    }
                };
                peer.receivePeerFrame(frame).await;
            }
            peer.state.connected.store(false, Ordering::Release);
            peer.failPending("Edge PeerLink carrier closed".to_string()).await;
        });
    }

    async fn sendFrame(&self, frame: PeerFrame) -> Result<(), CoreLinkError> {
        if !self.isConnected() {
            return Err(CoreLinkError::new("PEER_LINK_CLOSED", "Edge PeerLink is closed"));
        }
        self.state
            .carrier
            .sendPeerFrame(frame)
            .await
            .map_err(|error| CoreLinkError::new("PEER_SEND_FAILED", error))
    }

    async fn request(&self, request: PeerRequest) -> Result<PeerResponse, CoreLinkError> {
        let messageId = format!("edge-peer-{}", Uuid::new_v4().simple());
        let (sender, receiver) = oneshot::channel();
        self.state.pending.lock().await.insert(messageId.clone(), sender);
        if let Err(error) = self
            .sendFrame(PeerFrame {
                messageId: messageId.clone(),
                payload: PeerFramePayload::Request(request),
            })
            .await
        {
            self.state.pending.lock().await.remove(&messageId);
            return Err(error);
        }
        receiver
            .await
            .map_err(|error| CoreLinkError::new("PEER_RESPONSE_CLOSED", error.to_string()))?
    }

    /// Executes a complete routed Core call through the adjacent Space peer.
    pub async fn routedCall(
        &self,
        request: RoutedCoreRequest<CoreCallRequest>,
    ) -> CoreCallResponse {
        let requestId = request.payload.requestId.clone();
        match self.request(PeerRequest::Call(request)).await {
            Ok(PeerResponse::Call(response)) => response,
            Ok(_) => CoreCallResponse::err(
                requestId,
                CoreLinkError::new("PEER_PROTOCOL_ERROR", "PeerLink returned the wrong response"),
            ),
            Err(error) => CoreCallResponse::err(requestId, error),
        }
    }

    /// Reads a complete routed watch snapshot through the adjacent Space peer.
    pub async fn routedWatchSnapshot(
        &self,
        request: RoutedCoreRequest<CoreWatchRequest>,
    ) -> Result<CoreEvent, CoreLinkError> {
        match self.request(PeerRequest::WatchSnapshot(request)).await? {
            PeerResponse::WatchSnapshot(result) => result,
            _ => Err(CoreLinkError::new(
                "PEER_PROTOCOL_ERROR",
                "PeerLink returned the wrong response",
            )),
        }
    }

    /// Opens a complete routed watch through the adjacent Space peer.
    pub async fn routedWatch(
        &self,
        request: RoutedCoreRequest<CoreWatchRequest>,
    ) -> Result<CoreEventStream, CoreLinkError> {
        let subscriptionId = format!("edge-peer-watch-{}", Uuid::new_v4().simple());
        let (sender, receiver) = mpsc::unbounded_channel();
        self.state
            .watches
            .lock()
            .await
            .insert(subscriptionId.clone(), sender);
        match self
            .request(PeerRequest::WatchOpen(PeerWatchOpenRequest {
                subscriptionId: subscriptionId.clone(),
                request,
            }))
            .await
        {
            Ok(PeerResponse::Operation(Ok(()))) => {}
            Ok(PeerResponse::Operation(Err(error))) => {
                self.state.watches.lock().await.remove(&subscriptionId);
                return Err(error);
            }
            Ok(_) => {
                self.state.watches.lock().await.remove(&subscriptionId);
                return Err(CoreLinkError::new(
                    "PEER_PROTOCOL_ERROR",
                    "PeerLink returned the wrong response",
                ));
            }
            Err(error) => {
                self.state.watches.lock().await.remove(&subscriptionId);
                return Err(error);
            }
        }
        let peer = self.clone();
        Ok(CoreEventStream::new(receiver).withOnClose(move || {
            tokio::spawn(async move {
                peer.state.watches.lock().await.remove(&subscriptionId);
                let _ = peer
                    .request(PeerRequest::WatchClose(PeerWatchCloseRequest { subscriptionId }))
                    .await;
            });
        }))
    }

    /// Opens a complete routed input stream through the adjacent Space peer.
    pub async fn routedOpenPush(
        &self,
        request: RoutedCoreRequest<CorePushRequest>,
    ) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
        let pushId = format!("edge-peer-push-{}", Uuid::new_v4().simple());
        match self
            .request(PeerRequest::PushOpen(PeerPushOpenRequest {
                pushId: pushId.clone(),
                request,
            }))
            .await?
        {
            PeerResponse::Operation(Ok(())) => Ok(Box::new(EdgePeerPushSession {
                peer: self.clone(),
                pushId,
                nextSequence: 0,
            })),
            PeerResponse::Operation(Err(error)) => Err(error),
            _ => Err(CoreLinkError::new(
                "PEER_PROTOCOL_ERROR",
                "PeerLink returned the wrong response",
            )),
        }
    }

    async fn receivePeerFrame(&self, frame: PeerFrame) {
        match frame.payload {
            PeerFramePayload::Response(response) => {
                if let Some(sender) = self.state.pending.lock().await.remove(&frame.messageId) {
                    let _ = sender.send(Ok(response));
                }
            }
            PeerFramePayload::WatchEvent(event) => {
                let completed = event.event.kind == operit_link::CoreEventKind::Completed;
                let sender = {
                    let mut watches = self.state.watches.lock().await;
                    if completed { watches.remove(&event.subscriptionId) }
                    else { watches.get(&event.subscriptionId).cloned() }
                };
                if let Some(sender) = sender {
                    if sender.send(event.event).is_err() {
                        self.state.watches.lock().await.remove(&event.subscriptionId);
                    }
                } else if let Some(handler) = self.state.requestHandler.lock().await.clone() {
                    handler.dispatchPeerWatchEvent(event).await;
                }
            }
            PeerFramePayload::WatchClosed(closed) => {
                self.state.watches.lock().await.remove(&closed.subscriptionId);
                if let Some(handler) = self.state.requestHandler.lock().await.clone() {
                    handler.dispatchPeerWatchClosed(closed).await;
                }
            }
            PeerFramePayload::Heartbeat(PeerHeartbeat::Probe { sequence, sentAt }) => {
                let _ = self
                    .sendFrame(PeerFrame {
                        messageId: format!("edge-peer-heartbeat-ack-{}", Uuid::new_v4().simple()),
                        payload: PeerFramePayload::Heartbeat(PeerHeartbeat::Ack { sequence, sentAt }),
                    })
                    .await;
            }
            PeerFramePayload::Heartbeat(PeerHeartbeat::Ack { .. }) => {}
            PeerFramePayload::Request(request) => {
                // Relay handling must run outside the carrier receive loop. The relay
                // sends another PeerRequest on this same channel and needs that loop
                // available to consume the corresponding response.
                let peer = self.clone();
                tokio::spawn(async move {
                    let handler = peer.state.requestHandler.lock().await.clone();
                    let response = match handler {
                        Some(handler) => handler.dispatchPeerRequest(request).await,
                        None => {
                            let error = CoreLinkError::new(
                                "EDGE_CAPABILITY_NOT_HOSTED",
                                "Edge nodes do not execute routed business capabilities",
                            );
                            match request {
                                PeerRequest::Call(request) => PeerResponse::Call(
                                    CoreCallResponse::err(request.payload.requestId, error),
                                ),
                                PeerRequest::WatchSnapshot(_) => PeerResponse::WatchSnapshot(Err(error)),
                                _ => PeerResponse::Operation(Err(error)),
                            }
                        }
                    };
                    let _ = peer
                        .sendFrame(PeerFrame {
                            messageId: frame.messageId,
                            payload: PeerFramePayload::Response(response),
                        })
                        .await;
                });
            }
        }
    }

    /// Delivers an already authenticated first frame during a reconnect.
    pub async fn receiveFrame(&self, frame: PeerFrame) {
        self.receivePeerFrame(frame).await;
    }

    async fn failPending(&self, message: String) {
        let error = CoreLinkError::new("PEER_LINK_CLOSED", message);
        for (_, sender) in std::mem::take(&mut *self.state.pending.lock().await) {
            let _ = sender.send(Err(error.clone()));
        }
        self.state.watches.lock().await.clear();
    }
}

struct EdgePeerPushSession {
    peer: EdgePeerLink,
    pushId: String,
    nextSequence: u64,
}

#[async_trait]
impl CoreLinkPushSession for EdgePeerPushSession {
    async fn send(&mut self, value: CoreValue) -> Result<(), CoreLinkError> {
        let sequence = self.nextSequence;
        match self
            .peer
            .request(PeerRequest::PushItem(CorePushItem {
                pushId: self.pushId.clone(),
                sequence,
                args: value,
            }))
            .await?
        {
            PeerResponse::Operation(Ok(())) => {
                self.nextSequence = self.nextSequence.saturating_add(1);
                Ok(())
            }
            PeerResponse::Operation(Err(error)) => Err(error),
            _ => Err(CoreLinkError::new(
                "PEER_PROTOCOL_ERROR",
                "PeerLink returned the wrong response",
            )),
        }
    }

    async fn close(self: Box<Self>) -> Result<(), CoreLinkError> {
        match self
            .peer
            .request(PeerRequest::PushClose(PeerPushCloseRequest {
                pushId: self.pushId,
            }))
            .await?
        {
            PeerResponse::Operation(result) => result,
            _ => Err(CoreLinkError::new(
                "PEER_PROTOCOL_ERROR",
                "PeerLink returned the wrong response",
            )),
        }
    }
}

/// Binds an Edge-originated client to one adjacent full CoreNode. Every
/// generated call and watch is wrapped in a standard Space route; no request
/// can fall through to an Edge-local HostManager service.
#[derive(Clone)]
pub struct EdgeSpaceRouteClient {
    peer: EdgePeerLink,
    spaceId: String,
    originNodeId: String,
    targetNodeId: String,
    ttl: u32,
    routeKind: RoutedCoreRequestKind,
}

impl EdgeSpaceRouteClient {
    pub fn new(
        peer: EdgePeerLink,
        spaceId: String,
        originNodeId: String,
        targetNodeId: String,
        ttl: u32,
    ) -> Self {
        Self {
            peer,
            spaceId,
            originNodeId,
            targetNodeId,
            ttl,
            routeKind: RoutedCoreRequestKind::SpaceRoute,
        }
    }

    /// Delegates Binding resolution to the paired Space router without keeping
    /// a business database on Edge.
    pub fn throughAdjacent(peer: EdgePeerLink, spaceId: String, originNodeId: String, adjacentNodeId: String, ttl: u32) -> Self {
        Self { peer, spaceId, originNodeId, targetNodeId: adjacentNodeId, ttl,
            routeKind: RoutedCoreRequestKind::SpaceBinding }
    }

    fn route<T>(&self, payload: T) -> RoutedCoreRequest<T> {
        RoutedCoreRequest {
            spaceId: self.spaceId.clone(),
            originNodeId: self.originNodeId.clone(),
            targetNodeId: self.targetNodeId.clone(),
            ttl: self.ttl,
            routeKind: self.routeKind,
            payload,
        }
    }

    /// Send-safe entry point for embedded UI tasks using the standard route.
    pub async fn callRouted(&self, request: CoreCallRequest) -> CoreCallResponse {
        self.peer.routedCall(self.route(request)).await
    }

    pub async fn watchRouted(&self, request: CoreWatchRequest) -> Result<CoreEventStream, CoreLinkError> {
        self.peer.routedWatch(self.route(request)).await
    }

    pub fn isConnected(&self) -> bool { self.peer.isConnected() }
}

#[async_trait(?Send)]
impl CoreLinkSharedClient for EdgeSpaceRouteClient {
    async fn call(&self, request: CoreCallRequest) -> CoreCallResponse {
        self.peer.routedCall(self.route(request)).await
    }

    async fn watchSnapshot(&self, request: CoreWatchRequest) -> Result<CoreEvent, CoreLinkError> {
        self.peer.routedWatchSnapshot(self.route(request)).await
    }

    async fn watch(&self, request: CoreWatchRequest) -> Result<CoreEventStream, CoreLinkError> {
        self.peer.routedWatch(self.route(request)).await
    }
}

#[async_trait(?Send)]
impl CoreLinkClient for EdgeSpaceRouteClient {
    async fn call(&mut self, request: CoreCallRequest) -> CoreCallResponse {
        CoreLinkSharedClient::call(self, request).await
    }

    async fn watchSnapshot(&mut self, request: CoreWatchRequest) -> Result<CoreEvent, CoreLinkError> {
        CoreLinkSharedClient::watchSnapshot(self, request).await
    }

    async fn watch(&mut self, request: CoreWatchRequest) -> Result<CoreEventStream, CoreLinkError> {
        CoreLinkSharedClient::watch(self, request).await
    }

    async fn openPush(&mut self, request: CorePushRequest) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
        self.peer.routedOpenPush(self.route(request)).await
    }
}

impl CoreRouteRuntime for EdgeSpaceRouteClient {
    fn shouldRoute(&self, _methodName: &str, _args: &CoreValue) -> Result<bool, CoreLinkError> {
        // Edge never owns a local business implementation, including when the
        // adjacent peer is unavailable. Failure must propagate through Link.
        Ok(true)
    }

    fn call(&self, request: CoreCallRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = CoreCallResponse>>> {
        let client = self.clone();
        Box::pin(async move { CoreLinkSharedClient::call(&client, request).await })
    }

    fn watch(&self, request: CoreWatchRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CoreEventStream, CoreLinkError>>>> {
        let client = self.clone();
        Box::pin(async move { CoreLinkSharedClient::watch(&client, request).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use operit_link::{CoreValue, LinkFrame};
    use tokio::sync::mpsc as async_mpsc;

    struct MemoryChannel {
        tx: async_mpsc::UnboundedSender<LinkFrame>,
        rx: Mutex<async_mpsc::UnboundedReceiver<LinkFrame>>,
    }

    #[async_trait]
    impl LinkChannel for MemoryChannel {
        async fn send(&self, frame: LinkFrame) -> Result<(), String> {
            self.tx.send(frame).map_err(|error| error.to_string())
        }

        async fn receive(&self) -> Result<Option<LinkFrame>, String> {
            Ok(self.rx.lock().await.recv().await)
        }

        async fn close(&self) {}
    }

    #[tokio::test]
    async fn routeRuntimeUsesSpacePeerFramesAndRejectsLocalExecution() {
        let (edgeTx, mut coreRx) = async_mpsc::unbounded_channel();
        let (coreTx, edgeRx) = async_mpsc::unbounded_channel();
        let peer = EdgePeerLink::new(Arc::new(MemoryChannel {
            tx: edgeTx,
            rx: Mutex::new(edgeRx),
        }));
        let client = EdgeSpaceRouteClient::new(peer, "space".into(), "edge".into(), "executor".into(), 8);
        assert!(client.shouldRoute("sendUserMessage", &CoreValue::emptyMap()).unwrap());
        let server = tokio::spawn(async move {
            let frame = coreRx.recv().await.unwrap();
            let LinkFramePayload::PeerFrame(frame) = frame.payload else { panic!("expected PeerFrame") };
            let PeerFramePayload::Request(PeerRequest::Call(request)) = frame.payload else { panic!("expected routed call") };
            assert_eq!(request.spaceId, "space");
            assert_eq!(request.targetNodeId, "executor");
            assert_eq!(request.routeKind, RoutedCoreRequestKind::SpaceRoute);
            assert_eq!(request.payload.methodName, "sendUserMessage");
            coreTx.send(LinkFrame {
                messageId: frame.messageId.clone(),
                payload: LinkFramePayload::PeerFrame(PeerFrame {
                    messageId: frame.messageId,
                    payload: PeerFramePayload::Response(PeerResponse::Call(CoreCallResponse::ok(
                        request.payload.requestId, CoreValue::Null,
                    ))),
                }),
            }).unwrap();
            let incoming = CoreCallRequest::new("inbound", "core/test", "getDigitalOutput", CoreValue::emptyMap());
            coreTx.send(LinkFrame {
                messageId: "inbound".into(),
                payload: LinkFramePayload::PeerFrame(PeerFrame {
                    messageId: "inbound".into(),
                    payload: PeerFramePayload::Request(PeerRequest::Call(RoutedCoreRequest {
                        spaceId: "space".into(), originNodeId: "edge".into(), targetNodeId: "edge".into(), ttl: 8,
                        routeKind: RoutedCoreRequestKind::SpaceRoute, payload: incoming,
                    })),
                }),
            }).unwrap();
            let frame = coreRx.recv().await.unwrap();
            let LinkFramePayload::PeerFrame(frame) = frame.payload else { panic!("expected PeerFrame") };
            let PeerFramePayload::Response(PeerResponse::Call(response)) = frame.payload else { panic!("must reject, not reflect request") };
            assert_eq!(response.requestId.0, "inbound");
            assert_eq!(response.result.unwrap_err().code, "EDGE_CAPABILITY_NOT_HOSTED");
        });
        let response = CoreRouteRuntime::call(&client, CoreCallRequest::new(
            "outbound", operit_link::CORE_INTERNAL_TARGET,
            "sendUserMessage", CoreValue::emptyMap(),
        )).await;
        assert!(response.result.is_ok());
        tokio::time::timeout(std::time::Duration::from_secs(2), server).await.unwrap().unwrap();
    }


}
