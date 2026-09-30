#![allow(non_snake_case)]

use async_trait::async_trait;
use operit_link::{decodeLink, encodeLink};
use crate::{LinkFrame, LinkFramePayload};
use std::sync::Arc;

use crate::pairing::{signSession, verifySessionSignature, PeerSession};
use crate::transport::LinkChannel;

/// Adds the existing Link-style session identity and HMAC to every frame.
pub struct AuthenticatedLinkChannel {
    inner: Arc<dyn LinkChannel>,
    session: PeerSession,
}

impl AuthenticatedLinkChannel {
    pub fn new(inner: Arc<dyn LinkChannel>, session: PeerSession) -> Arc<Self> {
        Arc::new(Self { inner, session })
    }

    fn wrap(&self, frame: LinkFrame) -> Result<LinkFrame, String> {
        let payloadBytes = encodeLink(&frame).map_err(|error| error.to_string())?;
        Ok(LinkFrame {
            messageId: frame.messageId,
            payload: LinkFramePayload::Authenticated {
                sessionId: self.session.sessionId.clone(),
                deviceId: self.session.deviceId.clone(),
                signature: signSession(&self.session.sessionSecret, &payloadBytes),
                payloadBytes,
            },
        })
    }

    fn unwrap(&self, frame: LinkFrame) -> Result<LinkFrame, String> {
        let LinkFramePayload::Authenticated {
            sessionId,
            deviceId,
            signature,
            payloadBytes,
        } = frame.payload
        else {
            return Err("Peer Link frame is not authenticated".to_string());
        };
        if sessionId != self.session.sessionId {
            return Err("Peer Link session id mismatch".to_string());
        }
        if deviceId != self.session.peerDeviceId {
            return Err("Peer Link device id mismatch".to_string());
        }
        if !verifySessionSignature(&self.session.sessionSecret, &payloadBytes, &signature) {
            return Err("Peer Link signature mismatch".to_string());
        }
        decodeLink(&payloadBytes).map_err(|error| error.to_string())
    }
}

#[async_trait]
impl LinkChannel for AuthenticatedLinkChannel {
    async fn send(&self, frame: LinkFrame) -> Result<(), String> {
        self.inner.send(self.wrap(frame)?).await
    }

    async fn receive(&self) -> Result<Option<LinkFrame>, String> {
        let Some(frame) = self.inner.receive().await? else {
            return Ok(None);
        };
        self.unwrap(frame).map(Some)
    }

    async fn close(&self) {
        self.inner.close().await;
    }
}
