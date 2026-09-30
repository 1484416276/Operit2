//! A peer has one identity/session record. Transport is a property, never a device role.
use crate::remote::{coreNodeTransportClient, LinkAccessStore, PairedPeerSessionRecord,
    PairedRemoteSession};
use crate::CoreNodeRouter::CoreNodeRouter;
use operit_peer_link::PeerLinkClient;
use operit_store::{CoreSpaceStore::CoreSpaceStore, NetworkControlStore::NetworkControlStore};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PeerConnectionTarget {
    pub(crate) localNodeId: String,
    pub(crate) peerNodeId: String,
    record: PairedPeerSessionRecord,
}
impl PeerConnectionTarget {
    pub(crate) fn load(store: &LinkAccessStore) -> Result<Vec<Self>, String> {
        Ok(store.outboundSessions()?.into_values().map(|record| Self {
            localNodeId: record.deviceId.clone(), peerNodeId: record.peerNodeId.clone(), record,
        }).collect())
    }
    pub(crate) fn validate(&self, router: &CoreNodeRouter, store: &LinkAccessStore,
        space: &CoreSpaceStore, control: &NetworkControlStore) -> Result<(), String> {
        if self.localNodeId != router.localNodeId() || !space.contains(self.peerNodeId.clone())?
            || control.nodeIsDisconnected(&self.peerNodeId)? {
            return Err("Peer identity or membership is no longer current".into());
        }
        if !store.outboundSessions()?.values().any(|record| record == &self.record) {
            return Err("Peer credentials have changed".into());
        }
        Ok(())
    }
    pub(crate) async fn connect(&self, router: &CoreNodeRouter,
        space: &CoreSpaceStore) -> Result<PeerLinkClient, String> {
        super::transport::openOutboundPeerLink(
            PairedRemoteSession::fromRecord(self.record.clone())?,
            coreNodeTransportClient(router.clone()),
            space.clone(),
        ).await
    }
}
