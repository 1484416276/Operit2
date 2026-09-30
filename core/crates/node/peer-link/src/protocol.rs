//! Node pairing and authenticated carrier envelopes, not the Link application protocol.
use operit_link::{protocol::LinkDeviceInfo, PeerFrame};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkFrame {
    pub messageId: String,
    pub payload: LinkFramePayload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPairStartRequest {
    pub pairingServiceVersion: i32,
    pub tokenHash: String,
    pub clientDeviceId: String,
    pub clientDeviceInfo: LinkDeviceInfo,
    pub clientPublicKey: String,
    pub clientNonce: String,
    pub autoBootstrap: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPairStartResponse {
    pub pairingId: String,
    pub pairingServiceVersion: i32,
    pub peerNodeId: String,
    pub peerDeviceInfo: LinkDeviceInfo,
    pub peerPublicKey: String,
    pub serverNonce: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPairFinishRequest {
    pub pairingId: String,
    pub pairingCode: String,
    pub clientProof: String,
    pub tokenHash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPairFinishResponse {
    pub sessionId: String,
    pub pairingServiceVersion: i32,
    pub coreProof: String,
}

/// Node session control envelope; application operations use the canonical PeerFrame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "body")]
pub enum LinkFramePayload {
    PairStart(LinkPairStartRequest),
    PairStartResponse(LinkPairStartResponse),
    PairFinish(LinkPairFinishRequest),
    PairFinishResponse(LinkPairFinishResponse),
    Authenticated {
        sessionId: String,
        deviceId: String,
        signature: String,
        #[serde(with = "serde_bytes")]
        payloadBytes: Vec<u8>,
    },
    /// One complete standard Space PeerLink frame. TCP/UART Edge carriers
    /// transport this exact payload instead of defining a parallel route
    /// protocol.
    PeerFrame(PeerFrame),
    /// Authenticated Space admission metadata for a storage-free route source.
    SpaceContext { spaceId: String, adjacentNodeId: String, ttl: u32 },
    Close {
        code: String,
        message: String,
    },
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_removed_parallel_operation_tags() {
        #[derive(Serialize)]
        struct OldFrame { #[serde(rename = "messageId")] message_id: String, payload: OldPayload }
        #[derive(Serialize)]
        struct OldPayload { #[serde(rename = "type")] kind: String, body: () }
        for kind in ["Call", "CallResponse", "WatchSnapshot", "WatchSnapshotResponse",
            "WatchOpen", "WatchEvent", "WatchClose", "Operation", "Heartbeat"] {
            let bytes = operit_link::encodeLink(&OldFrame {
                message_id: "old-operation".into(),
                payload: OldPayload { kind: kind.into(), body: () },
            }).unwrap();
            assert!(operit_link::decodeLink::<LinkFrame>(&bytes).is_err(), "{kind}");
        }
    }

    #[test]
    fn carrier_roundtrips_canonical_peer_frame() {
        let operation = operit_link::PeerFrame {
            messageId: "call".into(),
            payload: operit_link::PeerFramePayload::Heartbeat(
                operit_link::PeerHeartbeat::Probe { sequence: 1, sentAt: 0 }),
        };
        let frame = LinkFrame { messageId: "carrier".into(), payload: LinkFramePayload::PeerFrame(operation) };
        let bytes = operit_link::encodeLink(&frame).unwrap();
        assert_eq!(operit_link::decodeLink::<LinkFrame>(&bytes).unwrap(), frame);
    }
}
