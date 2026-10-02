// Real facades, independent stores and authenticated CoreNodeRouter envelopes.
use crate::RuntimeRemoteLinkService::{RuntimeRemoteLinkService, SpaceJoinStatus};

fn approvalService(node: &str) -> (CoreNodeRouter, RuntimeRemoteLinkService) {
    let router = testCoreNodeRouterWithoutBinding(node);
    router.spaceStore.writeLocalDeviceProfile(node.into(), "test".into(), "test".into(), "1".into()).unwrap();
    router.networkControlStore.initializeCurrentSpace().unwrap();
    let service = RuntimeRemoteLinkService::newWithRouter((*router.localCore).clone(), router.clone());
    (router, service)
}
#[tokio::test]
async fn independent_space_join_is_pending_then_only_explicit_approval_admits() {
    let _guard = routeTestGlobalLock().lock().await;
    installTestRuntimeScheduler();
    let (macRouter, mac) = approvalService("approval-mac");
    let (iosRouter, ios) = approvalService("approval-ios");
    let _link = installTestPeer(&macRouter, iosRouter.localNodeId(), TestCoreNodeRouterEndpoint::new(iosRouter.clone())).unwrap();
    let beforeMac = mac.deviceSpace().unwrap();
    let beforeIos = ios.deviceSpace().unwrap();
    let request = mac.requestDeviceSpaceJoin(iosRouter.localNodeId()).await.unwrap();
    assert_eq!(request.status, SpaceJoinStatus::Pending);
    assert_eq!(request.reviewerDeviceId.as_deref(), Some("approval-ios"));
    assert_eq!(request.reviewerHops, Some(1));
    assert_eq!(mac.deviceSpace().unwrap(), beforeMac);
    assert_eq!(ios.deviceSpace().unwrap(), beforeIos);
    assert!(!iosRouter.networkControlStore.currentState().unwrap().memberNodeIds.contains("approval-mac"));
    assert!(mac.incomingDeviceSpaceJoins().await.unwrap().is_empty());
    let incoming = ios.incomingDeviceSpaceJoins().await.unwrap();
    assert_eq!(incoming.len(), 1);
    assert!(incoming[0].canApprove);
    // Restarting the facade neither loses the request nor generates a new one.
    let restoredMac = RuntimeRemoteLinkService::newWithRouter((*macRouter.localCore).clone(), macRouter.clone());
    assert_eq!(restoredMac.requestDeviceSpaceJoin("approval-ios".into()).await.unwrap().requestId, request.requestId);
    let approved = ios.decideDeviceSpaceJoin(request.requestId.clone(), request.assignmentVersion, true).await.unwrap();
    assert_eq!(approved.status, SpaceJoinStatus::Approved);
    // A duplicate decision returns the original outcome, without a second admission.
    let revision = ios.deviceSpace().unwrap().spaceRevision;
    ios.decideDeviceSpaceJoin(request.requestId.clone(), request.assignmentVersion, true).await.unwrap();
    assert_eq!(ios.deviceSpace().unwrap().spaceRevision, revision);
    let joined = restoredMac.refreshDeviceSpaceJoin(request.requestId).await.unwrap();
    assert_eq!(joined.status, SpaceJoinStatus::Joined);
    assert_eq!(mac.deviceSpace().unwrap(), ios.deviceSpace().unwrap());
    assert!(!macRouter.networkControlStore.nodeHasCapability("approval-mac", "network.members.join", None).unwrap());
    assert!(ios.incomingDeviceSpaceJoins().await.unwrap().is_empty());
}
#[tokio::test]
async fn rejection_cancel_and_stale_assignment_do_not_admit_or_change_applicant_space() {
    let _guard = routeTestGlobalLock().lock().await;
    installTestRuntimeScheduler();
    let (macRouter, mac) = approvalService("reject-mac");
    let (iosRouter, ios) = approvalService("reject-ios");
    let _link = installTestPeer(&macRouter, iosRouter.localNodeId(), TestCoreNodeRouterEndpoint::new(iosRouter.clone())).unwrap();
    let before = mac.deviceSpace().unwrap();
    let request = mac.requestDeviceSpaceJoin("reject-ios".into()).await.unwrap();
    ios.incomingDeviceSpaceJoins().await.unwrap();
    assert!(ios.decideDeviceSpaceJoin(request.requestId.clone(), request.assignmentVersion + 1, true).await.is_err());
    assert_eq!(ios.decideDeviceSpaceJoin(request.requestId.clone(), request.assignmentVersion, false).await.unwrap().status, SpaceJoinStatus::Rejected);
    assert_eq!(mac.refreshDeviceSpaceJoin(request.requestId).await.unwrap().status, SpaceJoinStatus::Rejected);
    assert_eq!(mac.deviceSpace().unwrap(), before);
    assert!(!iosRouter.networkControlStore.currentState().unwrap().memberNodeIds.contains("reject-mac"));
    let retry = mac.requestDeviceSpaceJoin("reject-ios".into()).await.unwrap();
    assert_eq!(mac.cancelDeviceSpaceJoin(retry.requestId.clone()).await.unwrap().status, SpaceJoinStatus::Cancelled);
    assert!(ios.incomingDeviceSpaceJoins().await.unwrap().is_empty());
    assert_eq!(mac.deviceSpace().unwrap(), before);
    assert!(!iosRouter.spaceStore.contains("reject-mac".into()).unwrap());
}

fn controlOperationsForSpace(router: &CoreNodeRouter, spaceId: &str) -> Vec<operit_store::SyncOperationStore::SyncOperation> {
    router.networkControlStore.currentSpaceOperations().unwrap().into_iter()
        .filter(|operation| operation.payload["spaceId"].as_str() == Some(spaceId)).collect()
}

#[tokio::test]
async fn leaving_joined_space_initializes_admin_and_rejoining_requires_approval() {
    let _guard = routeTestGlobalLock().lock().await;
    installTestRuntimeScheduler();
    let (macRouter, mac) = approvalService("leave-mac");
    let (iosRouter, ios) = approvalService("leave-ios");
    let _link = installTestPeer(&macRouter, iosRouter.localNodeId(), TestCoreNodeRouterEndpoint::new(iosRouter.clone())).unwrap();
    let first = mac.requestDeviceSpaceJoin("leave-ios".into()).await.unwrap();
    ios.incomingDeviceSpaceJoins().await.unwrap();
    ios.decideDeviceSpaceJoin(first.requestId.clone(), first.assignmentVersion, true).await.unwrap();
    mac.refreshDeviceSpaceJoin(first.requestId).await.unwrap();
    let joined = mac.deviceSpace().unwrap();
    // A normal admitted member remains unable to approve in the joined Space.
    assert!(mac.incomingDeviceSpaceJoins().await.unwrap().is_empty());
    assert!(!macRouter.networkControlStore.nodeHasCapability("leave-mac", "network.approval", None).unwrap());
    let macSpace = mac.leaveDeviceSpace().unwrap();
    let iosSpace = ios.leaveDeviceSpace().unwrap();
    assert_ne!(macSpace.spaceId, joined.spaceId);
    assert_ne!(iosSpace.spaceId, joined.spaceId);
    assert_ne!(macSpace.spaceId, iosSpace.spaceId);
    for (router, space) in [(&macRouter, &macSpace), (&iosRouter, &iosSpace)] {
        assert_eq!(space.members, vec![router.localNodeId()]);
        let state = router.networkControlStore.currentState().unwrap();
        assert!(state.initialized);
        assert_eq!(state.deviceIdentityIds.get(&router.localNodeId()).map(String::as_str), Some("admin"));
        assert!(router.networkControlStore.nodeHasCapability(&router.localNodeId(), "network.members.join", None).unwrap());
        assert!(router.networkControlStore.nodeHasCapability(&router.localNodeId(), "network.approval", None).unwrap());
        assert_eq!(controlOperationsForSpace(router, &space.spaceId).len(), 1);
    }
    // Existing pairing survives leaving, but membership still needs approval.
    let second = mac.requestDeviceSpaceJoin("leave-ios".into()).await.unwrap();
    assert_eq!(second.status, SpaceJoinStatus::Pending);
    assert_eq!(second.reviewerDeviceId.as_deref(), Some("leave-ios"));
    let incoming = ios.incomingDeviceSpaceJoins().await.unwrap();
    assert_eq!(incoming.len(), 1);
    assert!(incoming[0].canApprove);
    assert_eq!(ios.incomingDeviceSpaceJoins().await.unwrap(), incoming);
    assert_eq!(controlOperationsForSpace(&iosRouter, &iosSpace.spaceId).len(), 1);
    assert_eq!(mac.deviceSpace().unwrap(), macSpace);
    assert!(!iosRouter.networkControlStore.currentState().unwrap().memberNodeIds.contains("leave-mac"));
    ios.decideDeviceSpaceJoin(second.requestId.clone(), second.assignmentVersion, true).await.unwrap();
    assert_eq!(mac.refreshDeviceSpaceJoin(second.requestId).await.unwrap().status, SpaceJoinStatus::Joined);
    assert_eq!(mac.deviceSpace().unwrap(), ios.deviceSpace().unwrap());
    assert!(!macRouter.networkControlStore.nodeHasCapability("leave-mac", "network.approval", None).unwrap());
}

#[tokio::test]
async fn repeated_leave_initializes_each_new_space_once_without_changing_old_policy() {
    let _guard = routeTestGlobalLock().lock().await;
    installTestRuntimeScheduler();
    let (router, service) = approvalService("repeat-leave");
    let old = service.deviceSpace().unwrap();
    let oldOperations = controlOperationsForSpace(&router, &old.spaceId);
    let first = service.leaveDeviceSpace().unwrap();
    assert_ne!(first.spaceId, old.spaceId);
    let firstOperations = controlOperationsForSpace(&router, &first.spaceId);
    assert_eq!(firstOperations.len(), 1);
    let second = service.leaveDeviceSpace().unwrap();
    assert_ne!(second.spaceId, first.spaceId);
    assert_eq!(controlOperationsForSpace(&router, &second.spaceId).len(), 1);
    assert!(router.networkControlStore.nodeHasCapability("repeat-leave", "network.approval", None).unwrap());
    // Inspect only the isolated test store: all older Space policy operations survive.
    assert_eq!(controlOperationsForSpace(&router, &first.spaceId), firstOperations);
    assert_eq!(controlOperationsForSpace(&router, &old.spaceId), oldOperations);
    assert_eq!(service.deviceSpace().unwrap(), second);
}

/// Single real TCP pairing: independent peers have no return authority, while
/// admitted members exchange scoped return credentials without a second pairing.
/// Reconnect works; current revocation and local management boundaries still hold.
#[tokio::test]
async fn real_tcp_single_pairing_admission_enables_scoped_return_channel() {
    use crate::HostRuntimePeerService::HostRuntimePeerService;
    use crate::PeerStateStore::{PeerHostConfig, PeerHostPortMode, PeerStateStore};
    use operit_link::protocol::LinkDeviceInfo;
    use operit_host_api::HostManager::HostManager;
    use operit_host_native_common::NativeTcpHost;
    let _guard = routeTestGlobalLock().lock().await;
    installTestRuntimeScheduler();
    let (macRouter, _) = approvalService("tcp-mac");
    let (iosRouter, _) = approvalService("tcp-ios");
    let macRouter = Arc::new(macRouter);
    let iosRouter = Arc::new(iosRouter);
    let makeHost = |router: &CoreNodeRouter| Arc::new(HostManager {
        runtimeStorageHost: Some(router.localCore.runtimeStorageHost()),
        tcpHost: Some(Arc::new(NativeTcpHost)),
        hostRuntimeTaskSchedulerHost: Some(defaultHostRuntimeTaskSchedulerHost()),
        ..HostManager::default()
    });
    let macPeer = HostRuntimePeerService::new(makeHost(&macRouter), &macRouter, LinkDeviceInfo { platform: "mac".into(), model: "test".into() }).unwrap();
    let iosPeer = HostRuntimePeerService::new(makeHost(&iosRouter), &iosRouter, LinkDeviceInfo { platform: "ios".into(), model: "test".into() }).unwrap();
    macRouter.installNodeServices(NodeServices::new(macPeer.clone())).unwrap();
    iosRouter.installNodeServices(NodeServices::new(iosPeer.clone())).unwrap();
    let mac = RuntimeRemoteLinkService::newWithRouter((*macRouter.localCore).clone(), (*macRouter).clone());
    let ios = RuntimeRemoteLinkService::newWithRouter((*iosRouter.localCore).clone(), (*iosRouter).clone());
    // Both devices expose a listener; only Mac performs the six-digit pairing.
    for (router, peer) in [(&macRouter, &macPeer), (&iosRouter, &iosPeer)] {
        PeerStateStore::new(router.localCore.runtimeStorageHost()).saveHostConfig(&PeerHostConfig {
            bindAddress: "127.0.0.1:0".into(), token: "test-token".into(), transports: vec![PeerTransport::Tcp],
            discoveryEnabled: false, portMode: PeerHostPortMode::Automatic, updatedAt: 1,
        }).unwrap();
        peer.startListening(&[PeerTransport::Tcp]).await.unwrap();
    }
    let address = PeerStateStore::new(iosRouter.localCore.runtimeStorageHost()).hostConfig().unwrap().unwrap().bindAddress;
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        let pairing = macPeer.startPairing(PeerEndpoint { nodeId: "tcp-ios".into(), address }, PeerTransport::Tcp, Some("test-token")).await.unwrap();
        let prompts = iosPeer.pairingPrompts().unwrap();
        let code = &prompts.iter().find(|p| p.pairingId == pairing.pairingId).unwrap().confirmationCode;
        assert_eq!(code.len(), 6);
        macPeer.finishPairing(&pairing.pairingId, code).await.unwrap();
        assert!(mac.pairedDeviceOnline("tcp-ios".into()).unwrap());
        assert!(ios.pairedDeviceOnline("tcp-mac".into()).unwrap());
        assert!(iosPeer.pairingPrompts().unwrap().is_empty());
        let makeRequest = |origin: &str, target: &str, payload| RoutedCoreRequest {
            spaceId: "independent-space".into(), originNodeId: origin.into(), targetNodeId: target.into(), ttl: 0,
            routeKind: RoutedCoreRequestKind::Target, payload,
        };
        // iOS cannot actively dial Mac without an outbound grant. Still online.
        let response = iosPeer.call("tcp-mac", makeRequest("tcp-ios", "tcp-mac", CoreCallRequest::new("reverse", NODE_SPACE_TARGET, "snapshot", CoreValue::Null))).await;
        assert_eq!(response.result.unwrap_err().code, "PEER_OUTBOUND_NOT_AUTHORIZED");
        assert!(ios.pairedDeviceOnline("tcp-mac".into()).unwrap());
        // A real encrypted business rejection is not a disconnect on Mac either.
        let response = macPeer.call("tcp-ios", makeRequest("tcp-mac", "tcp-ios", CoreCallRequest::new("rejected", "core/server.runtimeRemoteLinkService", "deviceSpaceControlAudit", CoreValue::Null))).await;
        assert_eq!(response.result.unwrap_err().code, "LOCAL_MANAGEMENT_ONLY");
        assert!(mac.pairedDeviceOnline("tcp-ios".into()).unwrap());
        assert!(ios.pairedDeviceOnline("tcp-mac".into()).unwrap());
        // Admission establishes a scoped return channel, never a reverse pairing.
        let request = mac.requestDeviceSpaceJoin("tcp-ios".into()).await.unwrap();
        assert!(iosRouter.spaceChannelScope("tcp-mac").unwrap().is_none());
        assert!(macRouter.spaceChannelScope("tcp-ios").unwrap().is_none());
        assert_eq!(iosPeer.call("tcp-mac", makeRequest("tcp-ios", "tcp-mac",
            CoreCallRequest::new("pending-return", NODE_SPACE_TARGET, "snapshot", CoreValue::Null)))
            .await.result.unwrap_err().code, "PEER_OUTBOUND_NOT_AUTHORIZED");
        ios.incomingDeviceSpaceJoins().await.unwrap();
        ios.decideDeviceSpaceJoin(request.requestId.clone(), request.assignmentVersion, true).await.unwrap();
        assert_eq!(mac.refreshDeviceSpaceJoin(request.requestId).await.unwrap().status, SpaceJoinStatus::Joined);
        assert!(!iosPeer.pairedPeers().unwrap().iter().find(|p| p.nodeId == "tcp-mac").unwrap().outbound);
        assert!(iosRouter.spaceChannelScope("tcp-mac").unwrap().is_some());
        let returnRequest = || RoutedCoreRequest {
            spaceId: ios.deviceSpace().unwrap().spaceId,
            originNodeId: "tcp-ios".into(), targetNodeId: "tcp-mac".into(), ttl: 0,
            routeKind: RoutedCoreRequestKind::Target,
            payload: PeerSyncMethod::DeviceSpace.request("return-space".into(), CoreValue::Null),
        };
        let reverse = iosPeer.call("tcp-mac", returnRequest()).await;
        assert!(reverse.result.is_ok(), "{:?}", reverse.result);
        // Return credentials survive reconnect without any second confirmation.
        iosPeer.disconnectPeer("tcp-mac").await.unwrap();
        assert!(iosPeer.call("tcp-mac", returnRequest()).await.result.is_ok());
        // A Space grant does not expose local-only management surfaces.
        let mut denied = returnRequest();
        denied.payload = CoreCallRequest::new("return-management", "core/server.runtimeRemoteLinkService", "deviceSpaceControlAudit", CoreValue::Null);
        assert_eq!(iosPeer.call("tcp-mac", denied).await.result.unwrap_err().code, "LOCAL_MANAGEMENT_ONLY");
        // Listener restart clears online evidence. Restored credentials must
        // reconnect without a business call, manual sync, or reverse pairing.
        macPeer.stop().await.unwrap();
        iosPeer.stop().await.unwrap();
        assert!(!mac.pairedDeviceOnline("tcp-ios".into()).unwrap());
        assert!(!ios.pairedDeviceOnline("tcp-mac".into()).unwrap());
        macPeer.startListening(&[PeerTransport::Tcp]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!mac.pairedDeviceOnline("tcp-ios".into()).unwrap());
        iosPeer.startListening(&[PeerTransport::Tcp]).await.unwrap();
        tokio::time::timeout(Duration::from_secs(7), async {
            while !mac.pairedDeviceOnline("tcp-ios".into()).unwrap()
                || !ios.pairedDeviceOnline("tcp-mac".into()).unwrap() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.unwrap();
        assert!(!iosPeer.pairedPeers().unwrap().iter().find(|p| p.nodeId == "tcp-mac").unwrap().outbound);
        // Current policy, not the persisted grant, owns revocation.
        ios.disconnectDeviceSpaceNode("tcp-mac".into()).await.unwrap();
        for operation in iosRouter.networkControlStore.currentSpaceOperations().unwrap() {
            macRouter.networkControlStore.applyBootstrapOperation(&operation).unwrap();
        }
        assert!(iosPeer.call("tcp-mac", returnRequest()).await.result.is_err());
        let revoked = macPeer.call("tcp-ios", makeRequest("tcp-mac", "tcp-ios",
            PeerSyncMethod::DeviceSpace.request("revoked-ordinary".into(), CoreValue::Null))).await;
        assert!(revoked.result.is_err(), "Revoked ordinary pairing must not reconnect");
        assert!(!iosPeer.activePeerNodeIds().unwrap().contains("tcp-mac"));
        mac.leaveDeviceSpace().unwrap();
        assert!(macRouter.spaceChannelScope("tcp-ios").unwrap().is_none());
        assert!(iosPeer.call("tcp-mac", returnRequest()).await.result.is_err());
        // This fix must not mask a real network failure.
        iosPeer.stop().await.unwrap();
        let failed = macPeer.call("tcp-ios", makeRequest("tcp-mac", "tcp-ios", CoreCallRequest::new("offline", NODE_SPACE_TARGET, "snapshot", CoreValue::Null))).await;
        assert!(failed.result.is_err());
        assert!(!mac.pairedDeviceOnline("tcp-ios".into()).unwrap());
    }).await;
    macPeer.stop().await.unwrap();
    iosPeer.stop().await.unwrap();
    result.unwrap();
}

struct ApprovalMeshPeer {
    local: String,
    endpoints: StdMutex<BTreeMap<String, Arc<dyn TestRouteTarget>>>,
    active: StdMutex<BTreeSet<String>>,
    changes: tokio::sync::broadcast::Sender<()>,
}
impl ApprovalMeshPeer {
    fn new(local: String) -> Arc<Self> { Arc::new(Self { local, endpoints: StdMutex::new(BTreeMap::new()),
        active: StdMutex::new(BTreeSet::new()), changes: tokio::sync::broadcast::channel(16).0 }) }
    fn link(&self, router: &CoreNodeRouter) {
        self.endpoints.lock().unwrap().insert(router.localNodeId(), TestCoreNodeRouterEndpoint::new(router.clone()));
        self.active.lock().unwrap().insert(router.localNodeId());
    }
}
#[async_trait(?Send)]
impl RuntimePeerService for ApprovalMeshPeer {
    async fn discoverPeers(&self, _: u64) -> Result<Vec<DiscoveredPeer>, CoreLinkError> { unreachable!() }
    async fn startPairing(&self, _: PeerEndpoint, _: PeerTransport, _: Option<&str>) -> Result<PendingPairing, CoreLinkError> { unreachable!() }
    async fn finishPairing(&self, _: &str, _: &str) -> Result<PairedPeer, CoreLinkError> { unreachable!() }
    async fn cancelPairing(&self, _: &str) -> Result<(), CoreLinkError> { unreachable!() }
    async fn startListening(&self, _: &[PeerTransport]) -> Result<(), CoreLinkError> { unreachable!() }
    async fn stop(&self) -> Result<(), CoreLinkError> { self.active.lock().unwrap().clear(); Ok(()) }
    async fn call(&self, node: &str, request: RoutedCoreRequest<CoreCallRequest>) -> CoreCallResponse {
        if !self.active.lock().unwrap().contains(node) { return CoreCallResponse::err(request.payload.requestId, CoreLinkError::new("TEST_OFFLINE", "Offline mesh link")); }
        let endpoint = self.endpoints.lock().unwrap().get(node).cloned().unwrap();
        endpoint.routedCall(self.local.clone(), request).await
    }
    async fn watchSnapshot(&self, _: &str, _: RoutedCoreRequest<CoreWatchRequest>) -> Result<CoreEvent, CoreLinkError> { unreachable!() }
    async fn watch(&self, _: &str, _: RoutedCoreRequest<CoreWatchRequest>) -> Result<CoreEventStream, CoreLinkError> { unreachable!() }
    async fn openPush(&self, _: &str, _: RoutedCoreRequest<CorePushRequest>) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> { unreachable!() }
    fn pairedPeers(&self) -> Result<Vec<PairedPeer>, CoreLinkError> {
        Ok(self.endpoints.lock().unwrap().keys().map(|node| PairedPeer { nodeId: node.clone(), displayName: node.clone(), inbound: true, outbound: true }).collect())
    }
    fn outboundPeerNodeIds(&self) -> Result<BTreeSet<String>, CoreLinkError> { Ok(self.endpoints.lock().unwrap().keys().cloned().collect()) }
    fn activePeerNodeIds(&self) -> Result<BTreeSet<String>, CoreLinkError> { Ok(self.active.lock().unwrap().clone()) }
    fn subscribePeerChanges(&self) -> tokio::sync::broadcast::Receiver<()> { self.changes.subscribe() }
    fn pairingPrompts(&self) -> Result<Vec<PairingPrompt>, CoreLinkError> { Ok(vec![]) }
    async fn disconnectPeer(&self, node: &str) -> Result<(), CoreLinkError> { self.active.lock().unwrap().remove(node); Ok(()) }
    async fn removePairedPeer(&self, _: &str) -> Result<(), CoreLinkError> { unreachable!() }
}
#[tokio::test]
async fn nearest_authorized_reviewer_only_then_offline_transfer_and_remote_decision() {
    use operit_store::NetworkControlStore::NetworkControlIdentityAssignment;
    use operit_store::CoreSpaceStore::CoreSpaceLinkAdvertisement;
    use crate::PeerStateStore::PeerStateStore;
    let _guard = routeTestGlobalLock().lock().await;
    installTestRuntimeScheduler();
    let (applicantRouter, applicant) = approvalService("mesh-applicant");
    let (gatewayRouter, gateway) = approvalService("mesh-gateway");
    let (nearRouter, near) = approvalService("mesh-near");
    let (relayRouter, _) = approvalService("mesh-relay");
    let (farRouter, far) = approvalService("mesh-far");
    let nodes = [&gatewayRouter, &nearRouter, &relayRouter, &farRouter];
    let profiles: Vec<_> = nodes.iter().flat_map(|r| r.spaceStore.deviceProfilesForCurrentSpace().unwrap()).collect();
    // The near reviewer bootstraps this target Space, explicitly admitting the
    // gateway as a normal member and the other reviewer as admin. No auto-admin.
    for router in [&gatewayRouter, &relayRouter, &farRouter] {
        nearRouter.networkControlStore.admitMember(router.localNodeId()).unwrap();
    }
    let mut target = near.deviceSpace().unwrap();
    target.spaceRevision = 10;
    target.members = nodes.iter().map(|r| r.localNodeId()).collect();
    nearRouter.spaceStore.adopt(target.clone()).unwrap();
    nearRouter.networkControlStore.setIdentity(NetworkControlIdentityAssignment { nodeId: "mesh-far".into(), roleId: "admin".into() }).unwrap();
    let operations = nearRouter.networkControlStore.currentSpaceOperations().unwrap();
    for router in nodes {
        if router.localNodeId() != "mesh-near" { router.spaceStore.adopt(target.clone()).unwrap(); }
        for operation in &operations { router.networkControlStore.applyBootstrapOperation(operation).unwrap(); }
        router.spaceStore.importDeviceProfiles(profiles.clone()).unwrap();
    }
    let all = [&applicantRouter, &gatewayRouter, &nearRouter, &relayRouter, &farRouter];
    let peers: Vec<_> = all.iter().map(|r| ApprovalMeshPeer::new(r.localNodeId())).collect();
    for (a, b) in [(0,1), (1,2), (1,3), (3,4)] { peers[a].link(all[b]); peers[b].link(all[a]); }
    for (router, peer) in all.iter().zip(&peers) { router.installNodeServices(NodeServices::new(peer.clone())).unwrap(); }
    // Publish real directed topology and current link measurements. These are
    // the exact graph APIs used by production routing and distance selection.
    let now = operit_host_api::TimeUtils::currentTimeMillis();
    for (router, peer) in all.iter().zip(&peers).skip(1) {
        let active = peer.activePeerNodeIds().unwrap().into_iter().filter(|id| target.members.contains(id)).collect::<Vec<_>>();
        router.spaceStore.setDirectPeers(active.clone()).unwrap();
        for node in active { router.spaceStore.publishLocalLinkAdvertisement(CoreSpaceLinkAdvertisement {
            targetNodeId: node, channelEpoch: "test-epoch".into(), sequence: 1, measuredAt: now,
            expiresAt: now+60_000, smoothedRttMs: 1, lossPermille: 0, congestionPermille: 0,
        }).unwrap(); }
    }
    // Replicate topology records through the existing host fixture only.
    let topologyPath = operit_util::RuntimeStorageLayout::RUNTIME_SPACE_TOPOLOGY_DIR_PATH;
    for source in nodes {
        let storage = source.localCore.runtimeStorageHost();
        for entry in storage.list(topologyPath).unwrap() {
            if entry.isDirectory { continue; }
            let bytes = storage.readBytes(&entry.path).unwrap();
            for dest in nodes { dest.localCore.runtimeStorageHost().writeBytes(&entry.path, &bytes).unwrap(); }
        }
    }
    assert!(!gatewayRouter.networkControlStore.nodeHasCapability("mesh-gateway", "network.members.join", None).unwrap());
    let request = applicant.requestDeviceSpaceJoin("mesh-gateway".into()).await.unwrap();
    assert_eq!(request.reviewerDeviceId.as_deref(), Some("mesh-near"));
    assert_eq!(request.reviewerHops, Some(2));
    assert_eq!(near.incomingDeviceSpaceJoins().await.unwrap().len(), 1);
    assert!(far.incomingDeviceSpaceJoins().await.unwrap().is_empty());
    assert!(gateway.incomingDeviceSpaceJoins().await.unwrap().is_empty());
    peers[1].disconnectPeer("mesh-near").await.unwrap();
    let stillWaiting = applicant.refreshDeviceSpaceJoin(request.requestId.clone()).await.unwrap();
    assert_eq!(stillWaiting.reviewerDeviceId.as_deref(), Some("mesh-near")); // Grace period.
    // Advance just the record's unavailable timestamp; no real 30-second sleep.
    let records = PeerStateStore::new(gatewayRouter.localCore.runtimeStorageHost());
    let path = "runtime/link_access/space_join_inbound.preferences.json";
    let mut record = records.records::<serde_json::Value>(path).unwrap().remove(&request.requestId).unwrap();
    record["unavailableSince"] = serde_json::json!(now - 31_000);
    records.putRecord(path, &request.requestId, &record).unwrap();
    let transferred = applicant.refreshDeviceSpaceJoin(request.requestId.clone()).await.unwrap();
    assert_eq!(transferred.reviewerDeviceId.as_deref(), Some("mesh-far"));
    assert_eq!(transferred.reviewerHops, Some(3));
    assert!(transferred.assignmentVersion > request.assignmentVersion);
    assert!(near.decideDeviceSpaceJoin(request.requestId.clone(), request.assignmentVersion, true).await.is_err());
    assert_eq!(far.incomingDeviceSpaceJoins().await.unwrap().len(), 1);
    far.decideDeviceSpaceJoin(request.requestId.clone(), transferred.assignmentVersion, true).await.unwrap();
    assert_eq!(applicant.refreshDeviceSpaceJoin(request.requestId).await.unwrap().status, SpaceJoinStatus::Joined);
    assert_eq!(applicant.deviceSpace().unwrap(), gateway.deviceSpace().unwrap());
    assert!(!gatewayRouter.networkControlStore.nodeHasCapability("mesh-gateway", "network.members.join", None).unwrap());
}
