//! PeerFrame adaptation for any authenticated LinkChannel (TCP, serial or a Host carrier).
use std::sync::Arc;
use async_trait::async_trait;
use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use operit_link::{PeerFrame};
use crate::{LinkFrame, LinkFramePayload};
use super::LinkChannel;
use crate::PeerLinkCarrier;

pub struct ChannelPeerCarrier(pub Arc<dyn LinkChannel>);
#[async_trait]
impl PeerLinkCarrier for ChannelPeerCarrier {
    async fn sendPeerFrame(&self, frame: PeerFrame) -> Result<(), String> {
        self.0.send(LinkFrame { messageId: frame.messageId.clone(), payload: LinkFramePayload::PeerFrame(frame) }).await
    }
    fn closePeerLinkCarrier(&self) {
        let channel = self.0.clone();
        let _ = defaultHostRuntimeTaskSchedulerHost().scheduleHostRuntimeAsyncTask("peer-channel-close",
            Box::new(move || Box::pin(async move { channel.close().await })));
    }
}
