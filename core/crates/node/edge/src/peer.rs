//! Admits only direct, authenticated routes into this device's registered capabilities.
use std::sync::Arc;
use async_trait::async_trait;
use operit_link::{CoreCallRequest, CoreCallResponse, CoreEvent, CoreEventStream, CoreLinkError,
    CoreLinkPushSession, CorePushRequest, CoreWatchRequest, RoutedCoreRequest, RoutedCoreRequestKind};
use operit_peer_link::CoreNodeTransportClient;
use crate::EdgeNode;

pub struct DevicePeerEndpoint {
    pub node: Option<Arc<EdgeNode>>,
    pub spaceId: String,
    pub adjacentNodeId: String,
    pub localNodeId: String,
}
impl DevicePeerEndpoint {
    fn admitted<T>(&self, previous: &str, request: &RoutedCoreRequest<T>) -> Result<&EdgeNode, CoreLinkError> {
        if request.spaceId != self.spaceId || request.ttl == 0 || previous != self.adjacentNodeId
            || request.originNodeId != self.adjacentNodeId || request.targetNodeId != self.localNodeId
            || request.routeKind != RoutedCoreRequestKind::Target {
            return Err(CoreLinkError::new("CORE_ROUTE_DENIED", "Request does not match authenticated device admission"));
        }
        self.node.as_deref().ok_or_else(unsupported)
    }
}
fn unsupported() -> CoreLinkError { CoreLinkError::new("CORE_CAPABILITY_NOT_HOSTED", "Device does not host this operation") }
fn unrouted() -> CoreLinkError { CoreLinkError::new("CORE_ROUTE_REQUIRED", "Peer capability requests require an admitted route") }
#[async_trait]
impl CoreNodeTransportClient for DevicePeerEndpoint {
    async fn call(&self, request: CoreCallRequest) -> CoreCallResponse { CoreCallResponse::err(request.requestId, unrouted()) }
    async fn watchSnapshot(&self, _: CoreWatchRequest) -> Result<CoreEvent, CoreLinkError> { Err(unrouted()) }
    async fn watch(&self, _: CoreWatchRequest) -> Result<CoreEventStream, CoreLinkError> { Err(unrouted()) }
    async fn openPush(&self, _: CorePushRequest) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> { Err(unrouted()) }
    async fn routedCall(&self, previous: String, request: RoutedCoreRequest<CoreCallRequest>) -> CoreCallResponse {
        match self.admitted(&previous, &request) {
            Ok(node) => node.dispatchCall(request.payload),
            Err(error) => CoreCallResponse::err(request.payload.requestId, error),
        }
    }
    async fn routedWatchSnapshot(&self, previous: String, request: RoutedCoreRequest<CoreWatchRequest>) -> Result<CoreEvent, CoreLinkError> {
        self.admitted(&previous, &request)?.dispatchWatchSnapshot(request.payload)
    }
    async fn routedWatch(&self, previous: String, request: RoutedCoreRequest<CoreWatchRequest>) -> Result<CoreEventStream, CoreLinkError> {
        self.admitted(&previous, &request)?.dispatchWatch(request.payload)
    }
    async fn routedOpenPush(&self, previous: String, request: RoutedCoreRequest<CorePushRequest>) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
        self.admitted(&previous, &request)?;
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use operit_host_api::{DeviceDigitalOutputRequest, DeviceDigitalOutputState, DeviceIoHost, HostResult};
    use operit_host_api::HostManager::HostManager;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Board(AtomicUsize);
    impl DeviceIoHost for Board {
        fn setDigitalOutput(&self, request: DeviceDigitalOutputRequest) -> HostResult<DeviceDigitalOutputState> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(DeviceDigitalOutputState { pin: request.pin, level: request.level })
        }
        fn getDigitalOutput(&self, pin: u8) -> HostResult<DeviceDigitalOutputState> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(DeviceDigitalOutputState { pin, level: false })
        }
    }
    fn endpoint() -> (DevicePeerEndpoint, Arc<Board>) {
        let board = Arc::new(Board(AtomicUsize::new(0)));
        (DevicePeerEndpoint {
            node: Some(Arc::new(EdgeNode::fromHostManager(HostManager::new().withDeviceIoHost(board.clone())))),
            spaceId: "space".into(), adjacentNodeId: "peer".into(), localNodeId: "board".into(),
        }, board)
    }
    fn request() -> RoutedCoreRequest<CoreCallRequest> {
        RoutedCoreRequest {
            spaceId: "space".into(), originNodeId: "peer".into(), targetNodeId: "board".into(),
            ttl: 1, routeKind: RoutedCoreRequestKind::Target,
            payload: CoreCallRequest::new("write", operit_edge_contract::EDGE_DEVICE_IO_OBJECT_ID,
                "setDigitalOutput", operit_link::toCoreValue(DeviceDigitalOutputRequest { pin: 2, level: true }).unwrap()),
        }
    }
    #[tokio::test]
    async fn admitted_standard_call_reaches_host_without_transport_specific_dispatch() {
        let (endpoint, board) = endpoint();
        assert!(endpoint.routedCall("peer".into(), request()).await.result.is_ok());
        assert_eq!(board.0.load(Ordering::Relaxed), 1);
    }
    #[tokio::test]
    async fn invalid_routes_never_touch_board_host() {
        let (endpoint, board) = endpoint();
        for field in 0..6 {
            let mut request = request();
            let mut previous = "peer";
            match field {
                0 => request.spaceId = "other-space".into(),
                1 => request.originNodeId = "other-peer".into(),
                2 => request.targetNodeId = "other-board".into(),
                3 => request.ttl = 0,
                4 => request.routeKind = RoutedCoreRequestKind::SpaceBinding,
                _ => previous = "other-neighbor",
            }
            assert!(endpoint.routedCall(previous.into(), request).await.result.is_err());
        }
        assert!(endpoint.call(request().payload).await.result.is_err());
        assert_eq!(board.0.load(Ordering::Relaxed), 0);
    }
}
