//! Connection observations. The node layer owns routing and persistence.
#[derive(Clone, Debug)]
pub struct LinkMeasurement {
    pub targetNodeId: String,
    pub channelEpoch: String,
    pub sequence: u64,
    pub measuredAt: i64,
    pub expiresAt: i64,
    pub smoothedRttMs: u64,
    pub lossPermille: u16,
    pub congestionPermille: u16,
}
pub trait PeerLinkObserver: Send + Sync {
    fn setDirectPeers(&self, peers: Vec<String>) -> Result<(), String>;
    fn publishLocalLinkAdvertisement(&self, measurement: LinkMeasurement) -> Result<(), String>;
}
