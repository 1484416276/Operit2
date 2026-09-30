#![allow(non_snake_case)]
//! Transport-independent authenticated peer connections. No aggregate node-runtime, store or business-runtime dependency.
pub mod connection;
pub mod observer;
pub mod timing;
pub mod transport;
pub use connection::*;

pub mod auth;
pub mod pairing;
pub mod client;
pub use auth::AuthenticatedLinkChannel;
pub use client::PeerRouteClient;
pub use pairing::{finishPairAsClient, startPairAsClient, linkTokenHash, PairStartState,
    PairingAuthority, PairingPersistentState, PairingStore, PeerSession, PAIRING_SERVICE_VERSION};

pub mod protocol;
pub use protocol::{LinkFrame, LinkFramePayload, LinkPairStartRequest, LinkPairStartResponse, LinkPairFinishRequest, LinkPairFinishResponse};
