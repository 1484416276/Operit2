//! 已鉴权 Link 会话：路由元数据留在标准 args 中，执行仍使用既有 CoreLinkSession。
use super::*;

pub(super) fn routedCall(request: RoutedCoreRequest<CoreCallRequest>) -> Result<CoreCallRequest, CoreLinkError> {
    let mut wire = request.payload.clone();
    wire.args = toCoreValue(request).map_err(|e| error(e.to_string()))?; Ok(wire)
}
fn routedWatch(request: RoutedCoreRequest<CoreWatchRequest>) -> Result<CoreWatchRequest, CoreLinkError> {
    let mut wire = request.payload.clone(); wire.args = toCoreValue(request).map_err(|e| error(e.to_string()))?; Ok(wire)
}
fn routedPush(request: RoutedCoreRequest<CorePushRequest>) -> Result<CorePushRequest, CoreLinkError> {
    let mut wire = request.payload.clone(); wire.args = toCoreValue(request).map_err(|e| error(e.to_string()))?; Ok(wire)
}
struct RoutedClient { router: CoreNodeRouter, peer: String }
#[async_trait(?Send)]
impl CoreLinkClient for RoutedClient {
    async fn call(&mut self, request: CoreCallRequest) -> CoreCallResponse {
        let id = request.requestId.clone();
        let routed = fromCoreValue::<RoutedCoreRequest<CoreCallRequest>>(request.args).map_err(|e| error(e.to_string()));
        match routed {
            Ok(r) if r.payload.requestId == id => self.router.routedCall(self.peer.clone(), r).await,
            Ok(_) => CoreCallResponse::err(id, error("Routed correlation mismatch")),
            Err(e) => CoreCallResponse::err(id, e),
        }
    }
    async fn watchSnapshot(&mut self, request: CoreWatchRequest) -> Result<CoreEvent, CoreLinkError> {
        let r: RoutedCoreRequest<CoreWatchRequest> = fromCoreValue(request.args).map_err(|e| error(e.to_string()))?;
        if r.payload.requestId != request.requestId { return Err(error("Routed correlation mismatch")); }
        self.router.routedWatchSnapshot(self.peer.clone(), r).await
    }
    async fn watch(&mut self, request: CoreWatchRequest) -> Result<CoreEventStream, CoreLinkError> {
        let r: RoutedCoreRequest<CoreWatchRequest> = fromCoreValue(request.args).map_err(|e| error(e.to_string()))?;
        if r.payload.requestId != request.requestId { return Err(error("Routed correlation mismatch")); }
        self.router.routedWatch(self.peer.clone(), r).await
    }
    async fn openPush(&mut self, request: CorePushRequest) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
        let r: RoutedCoreRequest<CorePushRequest> = fromCoreValue(request.args).map_err(|e| error(e.to_string()))?;
        if r.payload.requestId != request.requestId { return Err(error("Routed correlation mismatch")); }
        self.router.routedOpenPush(self.peer.clone(), r).await
    }
}
pub(super) async fn serve(service: HostRuntimePeerService, channel: Arc<Channel>, peer: String, sessionId: String, spaceChannel: bool) -> Result<(), CoreLinkError> {
    let mut session = CoreLinkSession::new(RoutedClient { router: service.router()?, peer: peer.clone() }, 32);
    loop {
        enum Incoming { Message(Option<PeerMessage>), Event(Option<CoreLinkResponse>) }
        let incoming = if session.hasWatches() {
            tokio::select! {
                message = channel.receive() => Incoming::Message(message?),
                event = session.nextWatchEvent() => Incoming::Event(event),
            }
        } else { Incoming::Message(channel.receive().await?) };
        // 每次业务入口重新确认入站授权。撤销不能被存活中的旧连接绕过。
        let valid = if spaceChannel {
            service.spaceInbound(&sessionId, &peer).is_ok()
        } else {
            service.inboundCredentials()?.get(&sessionId)
                .is_some_and(|c| c.deviceId == peer && c.pairingServiceVersion == PAIRING_SERVICE_VERSION)
        };
        if !valid { return Err(error("Inbound authorization revoked")); }
        match incoming {
            Incoming::Message(Some(PeerMessage::Request(request))) => {
                let response = match request {
                    CoreLinkRequest::Call(request) if request.target == space_channel::TARGET => {
                        let result = if spaceChannel { Err(error("Return channels cannot issue pairing-scoped offers")) }
                            else { service.acceptSpaceChannel(&peer, &sessionId, &channel.raw, &request) };
                        CoreLinkResponse::Call(CoreCallResponse { requestId: request.requestId, result })
                    }
                    request => session.dispatch(request).await,
                };
                channel.send(PeerMessage::Response(response)).await?;
            },
            Incoming::Event(Some(event)) => channel.send(PeerMessage::Response(event)).await?,
            Incoming::Message(None) => break,
            _ => return Err(error("Unexpected inbound Link response")),
        }
    }
    Ok(())
}
pub(super) async fn watchSnapshot(service: &HostRuntimePeerService, node: &str, request: RoutedCoreRequest<CoreWatchRequest>) -> Result<CoreEvent, CoreLinkError> {
    let channel = service.connectAuthorized(node).await?;
    let id = request.payload.requestId.clone();
    let result = channel.exchange(CoreLinkRequest::Watch(CoreLinkWatchRequest::Snapshot(routedWatch(request)?))).await;
    channel.raw.close().await;
    match result? {
        CoreLinkResponse::Watch { requestId, result } if requestId == id => match result? {
            CoreLinkWatchResponse::Snapshot(event) => Ok(event), _ => Err(error("Watch snapshot response mismatch")),
        }, _ => Err(error("Watch correlation mismatch")),
    }
}
pub(super) async fn watch(service: &HostRuntimePeerService, node: &str, request: RoutedCoreRequest<CoreWatchRequest>) -> Result<CoreEventStream, CoreLinkError> {
    let channel = service.connectAuthorized(node).await?;
    let id = request.payload.requestId.clone();
    let opened = channel.exchange(CoreLinkRequest::Watch(CoreLinkWatchRequest::Open(routedWatch(request)?))).await?;
    match opened {
        CoreLinkResponse::Watch { requestId, result } if requestId == id => match result? {
            CoreLinkWatchResponse::Opened => {}, _ => { channel.raw.close().await; return Err(error("Watch open response mismatch")); },
        }, _ => { channel.raw.close().await; return Err(error("Watch correlation mismatch")); },
    }
    let (tx, stream) = CoreEventStream::channel();
    let (stop, mut stopped) = tokio::sync::oneshot::channel::<()>();
    let scheduler = service.state.host.hostRuntimeTaskSchedulerHost.as_ref().ok_or_else(|| error("Host scheduler missing"))?;
    scheduler.scheduleHostRuntimeAsyncTask("peer-watch", Box::new(move || Box::pin(async move {
        loop {
            let message = tokio::select! { _ = &mut stopped => break, message = channel.receive() => message };
            match message {
                Ok(Some(PeerMessage::Response(CoreLinkResponse::Watch { requestId, result: Ok(CoreLinkWatchResponse::Event(event)) })))
                    if requestId == id => { if tx.send(event).is_err() { break; } },
                _ => break,
            }
        }
        channel.raw.close().await;
    }))).map_err(|e| error(e.to_string()))?;
    Ok(stream.withOnClose(move || { let _ = stop.send(()); }))
}
struct Push { channel: Arc<Channel>, id: String, next: u64 }
#[async_trait]
impl CoreLinkPushSession for Push {
    async fn send(&mut self, args: CoreValue) -> Result<(), CoreLinkError> {
        let sequence = self.next;
        self.next = sequence.checked_add(1).ok_or_else(|| error("Push sequence exhausted"))?;
        match self.channel.exchange(CoreLinkRequest::Push(CoreLinkPushRequestMessage::Item(CorePushItem {
            pushId: self.id.clone(), sequence, args,
        }))).await? {
            CoreLinkResponse::Push { pushId, result } if pushId == self.id => match result? {
                CoreLinkPushResponse::ItemAccepted { sequence: accepted } if accepted == sequence => Ok(()),
                _ => Err(error("Push sequence response mismatch")),
            }, _ => Err(error("Push correlation mismatch")),
        }
    }
    async fn close(self: Box<Self>) -> Result<(), CoreLinkError> {
        let result = self.channel.exchange(CoreLinkRequest::Push(CoreLinkPushRequestMessage::Close { pushId: self.id.clone() })).await;
        self.channel.raw.close().await;
        match result? {
            CoreLinkResponse::Push { pushId, result } if pushId == self.id => match result? {
                CoreLinkPushResponse::Closed => Ok(()), _ => Err(error("Push close response mismatch")),
            }, _ => Err(error("Push correlation mismatch")),
        }
    }
}
pub(super) async fn openPush(service: &HostRuntimePeerService, node: &str, request: RoutedCoreRequest<CorePushRequest>) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
    let channel = service.connectAuthorized(node).await?;
    let id = request.payload.requestId.0.clone();
    match channel.exchange(CoreLinkRequest::Push(CoreLinkPushRequestMessage::Open(routedPush(request)?))).await? {
        CoreLinkResponse::Push { pushId, result } if pushId == id => match result? {
            CoreLinkPushResponse::Opened => Ok(Box::new(Push { channel, id, next: 0 })),
            _ => { channel.raw.close().await; Err(error("Push open response mismatch")) },
        }, _ => { channel.raw.close().await; Err(error("Push correlation mismatch")) },
    }
}
