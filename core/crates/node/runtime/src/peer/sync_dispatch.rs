//! Peer synchronization adapter to the existing generated application/service dispatch.
use super::*;

impl CoreNodeRouter {
    pub(super) fn validatePeerSyncRoute<T>(
        &self,
        previousNodeId: &str,
        request: &RoutedCoreRequest<T>,
    ) -> Result<bool, CoreLinkError> {
        // Synchronization is a same-Space operation, unlike pre-admission
        // join calls. A member may arrive over its admitted return channel.
        let peers = self.nodeServices().map_err(CoreLinkError::internal)?.peers().pairedPeers()?;
        if !peers.iter().any(|p| p.nodeId == previousNodeId && (p.inbound || p.outbound))
            || self.spaceChannelScope(previousNodeId)?.is_none() {
            return Err(CoreLinkError::new("PEER_NOT_AUTHORIZED", "Synchronization requires an authenticated admitted Space member"));
        }
        if request.routeKind != RoutedCoreRequestKind::Target {
            return Err(CoreLinkError::new(
                "PEER_SYNC_INVALID_ROUTE",
                "Synchronization requires a target route",
            ));
        }
        // Includes Space identity, membership and revocation checks for every hop.
        self.validateIncomingRoute(previousNodeId, request)
    }

    pub(super) async fn dispatchPeerSyncCall(
        &self,
        method: PeerSyncMethod,
        mut request: CoreCallRequest,
    ) -> CoreCallResponse {
        if method == PeerSyncMethod::DeviceSpace {
            let service =
                RuntimeRemoteLinkService::newWithRouter((*self.localCore).clone(), self.clone());
            let result = service
                .deviceSpace()
                .map_err(CoreLinkError::internal)
                .and_then(|space| {
                    operit_link::toCoreValue(space)
                        .map_err(|error| CoreLinkError::internal(error.to_string()))
                });
            return CoreCallResponse {
                requestId: request.requestId,
                result,
            };
        }
        let Some(target) = self.targetForSchema("application") else {
            return CoreCallResponse::err(
                request.requestId,
                CoreLinkError::internal("Application schema missing"),
            );
        };
        request.target = target.into();
        self.executeLocalCall(request).await
    }

    pub(super) fn dispatchPeerSyncPush(
        &self,
        mut request: CorePushRequest,
    ) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
        let target = self
            .targetForSchema("services.syncBlobTransferManager")
            .ok_or_else(|| CoreLinkError::internal("Sync blob schema missing"))?;
        request.target = target.into();
        self.localCore.openPush(request)
    }
}
