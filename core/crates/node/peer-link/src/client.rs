//! Space-scoped client projection over the canonical PeerLink engine.
use async_trait::async_trait;
use operit_link::{CoreCallRequest, CoreCallResponse, CoreEvent, CoreEventStream, CoreLinkError,
    CoreLinkClient, CoreLinkSharedClient, CoreRouteRuntime, CoreValue, CoreWatchRequest,
    RoutedCoreRequest, RoutedCoreRequestKind};
use crate::PeerLinkClient;
use operit_link::{CorePushRequest, CoreLinkPushSession};
/// Binds a routed client to one authenticated adjacent node. Every
/// generated call and watch is wrapped in a standard Space route; no request
/// can fall through to an unrelated local service.
#[derive(Clone)]
pub struct PeerRouteClient {
    peer: PeerLinkClient,
    spaceId: String,
    originNodeId: String,
    targetNodeId: String,
    ttl: u32,
    routeKind: RoutedCoreRequestKind,
}

impl PeerRouteClient {
    pub fn new(
        peer: PeerLinkClient,
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
    /// a local business database.
    pub fn throughAdjacent(peer: PeerLinkClient, spaceId: String, originNodeId: String, adjacentNodeId: String, ttl: u32) -> Self {
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

    pub fn isConnected(&self) -> bool { !self.peer.isClosed() }
}

#[async_trait(?Send)]
impl CoreLinkSharedClient for PeerRouteClient {
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
impl CoreLinkClient for PeerRouteClient {
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

impl CoreRouteRuntime for PeerRouteClient {
    fn shouldRoute(&self, _methodName: &str, _args: &CoreValue) -> Result<bool, CoreLinkError> {
        // A remote client never falls back to a local implementation when the
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

