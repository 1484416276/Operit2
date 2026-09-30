use operit_store::CoreSpaceStore::{CoreSpaceLinkAdvertisement, CoreSpaceStore};
pub(crate) struct SpacePeerObserver(pub CoreSpaceStore);

/// Persists transport observations without coupling the connection engine to storage.
impl operit_peer_link::observer::PeerLinkObserver for SpacePeerObserver {
    fn setDirectPeers(&self, peers: Vec<String>) -> Result<(), String> {
        CoreSpaceStore::setDirectPeers(&self.0, peers)
    }
    fn publishLocalLinkAdvertisement(
        &self,
        m: operit_peer_link::observer::LinkMeasurement,
    ) -> Result<(), String> {
        CoreSpaceStore::publishLocalLinkAdvertisement(
            &self.0,
            CoreSpaceLinkAdvertisement {
                targetNodeId: m.targetNodeId,
                channelEpoch: m.channelEpoch,
                sequence: m.sequence,
                measuredAt: m.measuredAt,
                expiresAt: m.expiresAt,
                smoothedRttMs: m.smoothedRttMs,
                lossPermille: m.lossPermille,
                congestionPermille: m.congestionPermille,
            },
        )
    }
}
