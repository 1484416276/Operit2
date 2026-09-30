pub use operit_peer_link::pairing::{linkTokenHash, signSession, verifySessionSignature};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(target_arch = "wasm32"))]
use std::convert::Infallible;
#[cfg(not(target_arch = "wasm32"))]
use std::net::SocketAddr;
#[cfg(not(target_arch = "wasm32"))]
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
#[cfg(not(target_arch = "wasm32"))]
use std::task::{Context, Poll};

use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use axum::body::Body;
#[cfg(not(target_arch = "wasm32"))]
use axum::body::Bytes;
#[cfg(not(target_arch = "wasm32"))]
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
#[cfg(not(target_arch = "wasm32"))]
use axum::extract::{Json, State};
#[cfg(not(target_arch = "wasm32"))]
use axum::http::{HeaderMap, StatusCode};
#[cfg(not(target_arch = "wasm32"))]
use axum::response::{IntoResponse, Response};
#[cfg(not(target_arch = "wasm32"))]
use axum::routing::{get, post};
#[cfg(not(target_arch = "wasm32"))]
use axum::Router;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
#[cfg(not(target_arch = "wasm32"))]
use futures_util::Stream as FuturesStream;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use uuid::Uuid;

use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use operit_host_api::HostManager::{defaultHttpHost, defaultWebSocketHost};
use operit_host_api::{HttpRequestData, RuntimeStorageHost, TimeUtils::currentTimeMillis, WebSocketClosedCallback, WebSocketHost, WebSocketMessageCallback, WebSocketOpenedCallback, WebSocketRequestData};
use operit_link::CoreLinkClient;
use operit_link::{CoreCallRequest, CoreCallResponse, CoreEvent, CoreEventStream, CoreLinkError, CoreLinkPushSession, CorePushRequest, CoreWatchRequest};
#[cfg(not(target_arch = "wasm32"))]
use operit_runtime::services::RuntimeHostInteractionService::{publishOwnerWebAccessPairing, RuntimeHostInteractionWebAccessPairingPayload};
use operit_store::CoreNodeIdentityStore::CoreNodeIdentityStore;
use operit_store::CoreSpaceStore::{CoreSpace, CoreSpaceDeviceProfile, CoreSpaceStore};
use operit_store::NetworkControlStore::NetworkControlStore;
use operit_store::PreferencesDataStore::{emptyPreferences, stringPreferencesKey, CoreNodeStateStore, Flow, Preferences, PreferencesDataStoreError};
use operit_store::SyncOperationStore::SyncOperation;
use operit_util::RuntimeStorageLayout::{RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH, RUNTIME_LINK_ACCESS_IDENTITY_PATH, RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH, RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH, RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH, RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH};

pub(crate) mod connections;
pub(crate) mod pairing;
pub(crate) mod target;
pub(crate) mod topology;
pub mod transport;
use operit_peer_link::transport::http::encodePeerFrame;
use topology::SpacePeerObserver;

const LINK_HTTP_READ_TIMEOUT_SECONDS: u64 = 120;
const SESSION_INFO_READ_TIMEOUT_SECONDS: u64 = 10;

#[cfg(not(target_arch = "wasm32"))]
use operit_peer_link::{receivePeerFrame, registerPeerLink, PeerConnection, PeerFrameBatch, PeerFrameSender};
use operit_peer_link::{CoreNodeLinkClient, CoreNodeTransportClient};
use operit_peer_link::{PeerChannelOpenEnvelope, PeerFrame};

#[cfg(test)]
mod tests;

pub const REMOTE_PAIRING_SERVICE_VERSION: i32 = 1;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct RemoteLinkServerConfig {
    pub bindAddress: String,
    pub token: String,
    pub deviceId: String,
    pub deviceInfo: LinkDeviceInfo,
    pub webAccess: Option<RemoteWebAccessConfig>,
    pub printStartupInfo: bool,
    pub accessStore: LinkAccessStore,
}

#[cfg(not(target_arch = "wasm32"))]
pub struct RemoteLinkServer;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemotePairingCodeRecord {
    pub pairingId: String,
    pub pairingServiceVersion: i32,
    pub clientDeviceId: String,
    pub clientDeviceInfo: LinkDeviceInfo,
    pub pairingCode: String,
    pub createdAt: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedRemoteSessionRecord {
    pub deviceId: String,
    pub deviceInfo: LinkDeviceInfo,
    pub pairingServiceVersion: i32,
    pub sessionSecret: String,
}

const LINK_ACCESS_RECORD_KEY: &str = "record";
const LINK_ACCESS_BIND_ADDRESS_KEY: &str = "bindAddress";
const LINK_ACCESS_TOKEN_KEY: &str = "token";
const LINK_ACCESS_WEB_ACCESS_ENABLED_KEY: &str = "webAccessEnabled";
const LINK_ACCESS_DISCOVERY_ENABLED_KEY: &str = "discoveryEnabled";
const LINK_ACCESS_PORT_MODE_KEY: &str = "portMode";
const LINK_ACCESS_UPDATED_AT_KEY: &str = "updatedAt";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkAccessIdentity {
    pub deviceId: String,
    pub deviceInfo: LinkDeviceInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkAccessHostConfig {
    pub bindAddress: String,
    pub token: String,
    pub webAccessEnabled: bool,
    pub discoveryEnabled: bool,
    pub portMode: LinkAccessHostPortMode,
    pub updatedAt: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LinkAccessHostPortMode {
    #[serde(rename = "automatic")]
    Automatic,
    #[serde(rename = "fixed")]
    Fixed,
}

#[derive(Clone)]
pub struct LinkAccessStore {
    storage: Arc<dyn RuntimeStorageHost>,
}

impl LinkAccessStore {
    /// Creates the Link Access datastore for the current runtime host.
    #[allow(non_snake_case)]
    pub fn getInstance(context: &operit_host_api::HostManager::HostManager) -> Self {
        let storage = context
            .runtimeStorageHost
            .clone()
            .expect("LinkAccessStore requires a RuntimeStorageHost");
        Self::new(storage)
    }

    /// Creates the repository that owns Link Access records for one runtime.
    pub fn new(storage: Arc<dyn RuntimeStorageHost>) -> Self {
        Self { storage }
    }

    /// Initializes and returns the runtime's persisted Link device identity.
    pub fn initializeIdentity(
        &self,
        deviceInfo: LinkDeviceInfo,
    ) -> Result<LinkAccessIdentity, String> {
        let coreNodeIdentity = CoreNodeIdentityStore::new(self.storage.clone()).initialize()?;
        let store = self.dataStore(RUNTIME_LINK_ACCESS_IDENTITY_PATH);
        let preferences = self.readPreferences(&store)?;
        let encoded = requiredPreference(
            &preferences,
            LINK_ACCESS_RECORD_KEY,
            RUNTIME_LINK_ACCESS_IDENTITY_PATH,
        )?;
        let record: serde_json::Value =
            serde_json::from_str(&encoded).map_err(|error| error.to_string())?;
        let persistedDeviceId = record
            .get("deviceId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Link Access identity is missing deviceId".to_string())?;
        if persistedDeviceId != coreNodeIdentity.nodeId {
            return Err(format!(
                "device identity mismatch: current={}, link={persistedDeviceId}",
                coreNodeIdentity.nodeId
            ));
        }
        let identity = if let Some(persistedDeviceInfo) = record.get("deviceInfo") {
            LinkAccessIdentity {
                deviceId: coreNodeIdentity.nodeId,
                deviceInfo: serde_json::from_value(persistedDeviceInfo.clone())
                    .map_err(|error| error.to_string())?,
            }
        } else {
            let identity = LinkAccessIdentity {
                deviceId: coreNodeIdentity.nodeId,
                deviceInfo,
            };
            writeSingleRecord(&store, &identity)?;
            identity
        };
        CoreSpaceStore::new(self.storage.clone()).writeLocalDeviceProfile(
            identity.deviceInfo.displayName(),
            identity.deviceInfo.platform.clone(),
            identity.deviceInfo.model.clone(),
            operit_runtime::CORE_VERSION.to_string(),
        )?;
        self.syncPairedDeviceProfiles()?;
        NetworkControlStore::new(self.storage.clone())?.initializeCurrentSpace()?;
        Ok(identity)
    }

    /// Updates and returns the runtime's Link device information for its stable identity.
    #[allow(non_snake_case)]
    pub fn updateIdentityDeviceInfo(
        &self,
        deviceInfo: LinkDeviceInfo,
    ) -> Result<LinkAccessIdentity, String> {
        let coreNodeIdentity = CoreNodeIdentityStore::new(self.storage.clone()).initialize()?;
        let identity = LinkAccessIdentity {
            deviceId: coreNodeIdentity.nodeId,
            deviceInfo,
        };
        writeSingleRecord(
            &self.dataStore(RUNTIME_LINK_ACCESS_IDENTITY_PATH),
            &identity,
        )?;
        CoreSpaceStore::new(self.storage.clone()).writeLocalDeviceProfile(
            identity.deviceInfo.displayName(),
            identity.deviceInfo.platform.clone(),
            identity.deviceInfo.model.clone(),
            operit_runtime::CORE_VERSION.to_string(),
        )?;
        self.syncPairedDeviceProfiles()?;
        Ok(identity)
    }

    /// Returns every accepted inbound session owned by this runtime.
    pub fn inboundSessions(&self) -> Result<BTreeMap<String, AcceptedRemoteSessionRecord>, String> {
        self.readRecordMap(RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH)
    }

    /// Observes every accepted inbound session owned by this runtime.
    #[allow(non_snake_case)]
    pub fn inboundSessionsFlow(&self) -> Flow<BTreeMap<String, AcceptedRemoteSessionRecord>> {
        self.recordMapFlow(RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH)
    }

    /// Persists one accepted inbound session owned by this runtime.
    pub fn saveInboundSession(
        &self,
        sessionId: String,
        record: AcceptedRemoteSessionRecord,
    ) -> Result<(), String> {
        self.validateInboundSessionRecord(&sessionId, &record)?;
        self.writePairedDeviceProfile(&record.deviceId, &record.deviceInfo)?;
        self.writeMapRecord(RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH, &sessionId, &record)
    }

    /// Removes one accepted inbound session owned by this runtime.
    pub fn removeInboundSession(&self, sessionId: &str) -> Result<(), String> {
        let sessions = self.inboundSessions()?;
        sessions
            .get(sessionId)
            .ok_or_else(|| format!("accepted remote session does not exist: {sessionId}"))?;
        self.removeMapRecord(RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH, sessionId)
    }

    /// Returns every named outbound session owned by this runtime.
    pub fn outboundSessions(&self) -> Result<BTreeMap<String, PairedPeerSessionRecord>, String> {
        self.readRecordMap(RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH)
    }

    /// Observes every named outbound session owned by this runtime.
    #[allow(non_snake_case)]
    pub fn outboundSessionsFlow(&self) -> Flow<BTreeMap<String, PairedPeerSessionRecord>> {
        self.recordMapFlow(RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH)
    }

    /// Persists one named outbound session owned by this runtime.
    pub fn saveOutboundSession(
        &self,
        name: String,
        record: PairedPeerSessionRecord,
    ) -> Result<(), String> {
        self.validateOutboundSessionRecord(&name, &record)?;
        self.writePairedDeviceProfile(&record.peerNodeId, &record.peerDeviceInfo)?;
        let encoded = serde_json::to_string(&record).map_err(|error| error.to_string())?;
        self.dataStore(RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH).edit(|preferences| {
            let previous = preferences.entries().into_iter().filter_map(|(key, value)| {
                serde_json::from_str::<PairedPeerSessionRecord>(&value).ok()
                    .filter(|old| old.peerNodeId == record.peerNodeId).map(|_| key)
            }).collect::<Vec<_>>();
            for key in previous { preferences.remove(&stringPreferencesKey(&key)); }
            preferences.set(&stringPreferencesKey(&name), encoded.clone());
        }).map_err(|error| error.to_string())
    }

    /// Writes synchronized device profiles for every stored pairing endpoint.
    #[allow(non_snake_case)]
    pub fn syncPairedDeviceProfiles(&self) -> Result<(), String> {
        for record in self.inboundSessions()?.into_values() {
            self.writePairedDeviceProfile(&record.deviceId, &record.deviceInfo)?;
        }
        for record in self.outboundSessions()?.into_values() {
            self.writePairedDeviceProfile(&record.peerNodeId, &record.peerDeviceInfo)?;
        }
        Ok(())
    }

    /// Removes one named outbound session owned by this runtime.
    pub fn removeOutboundSession(&self, name: &str) -> Result<(), String> {
        let sessions = self.outboundSessions()?;
        sessions
            .get(name)
            .ok_or_else(|| format!("paired remote session does not exist: {name}"))?;
        self.removeMapRecord(RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH, name)
    }

    /// Returns every pending pairing owned by this runtime.
    pub fn pendingPairings(&self) -> Result<BTreeMap<String, RemotePairingCodeRecord>, String> {
        self.readRecordMap(RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH)
    }

    /// Persists one pending pairing owned by this runtime.
    pub fn savePendingPairing(&self, record: RemotePairingCodeRecord) -> Result<(), String> {
        self.writeMapRecord(
            RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH,
            &record.pairingId.clone(),
            &record,
        )
    }

    /// Removes one pending pairing owned by this runtime.
    pub fn removePendingPairing(&self, pairingId: &str) -> Result<(), String> {
        self.removeMapRecord(RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH, pairingId)
    }

    /// Returns every pending outbound pairing initiated by this runtime.
    #[allow(non_snake_case)]
    pub fn pendingOutboundPairings(
        &self,
    ) -> Result<BTreeMap<String, PendingOutboundPairingRecord>, String> {
        self.readRecordMap(RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH)
    }

    /// Persists one pending outbound pairing initiated by this runtime.
    #[allow(non_snake_case)]
    pub fn savePendingOutboundPairing(
        &self,
        pairingId: String,
        record: PendingOutboundPairingRecord,
    ) -> Result<(), String> {
        self.writeMapRecord(
            RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH,
            &pairingId,
            &record,
        )
    }

    /// Removes one pending outbound pairing after it has completed or been cancelled.
    #[allow(non_snake_case)]
    pub fn removePendingOutboundPairing(&self, pairingId: &str) -> Result<(), String> {
        self.removeMapRecord(
            RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH,
            pairingId,
        )
    }

    /// Removes only the cancelled credentials, atomically with replacement writes.
    pub fn removeOutboundSessionCredentials(
        &self,
        deviceId: &str,
        sessionId: &str,
    ) -> Result<(), String> {
        self.dataStore(RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH)
            .edit(|preferences| {
                let obsolete = preferences
                    .entries()
                    .into_iter()
                    .filter_map(|(key, value)| {
                        serde_json::from_str::<PairedPeerSessionRecord>(&value)
                            .ok()
                            .filter(|old| {
                                old.peerNodeId == deviceId && old.sessionId == sessionId
                            })
                            .map(|_| key)
                    })
                    .collect::<Vec<_>>();
                for key in obsolete {
                    preferences.remove(&stringPreferencesKey(&key));
                }
            })
            .map_err(|error| error.to_string())
    }

    /// Persists the active Link Access host configuration for this runtime.
    pub fn saveHostConfig(&self, config: LinkAccessHostConfig) -> Result<(), String> {
        writeHostConfigPreferences(
            &self.dataStore(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH),
            &config,
        )
    }

    /// Initializes and returns the active Link Access host configuration.
    pub fn initializeHostConfig(&self) -> Result<LinkAccessHostConfig, String> {
        let store = self.dataStore(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH);
        let preferences = self.readPreferences(&store)?;
        if !preferences.entries().is_empty() {
            return hostConfigFromPreferences(&preferences);
        }
        let config = LinkAccessHostConfig {
            bindAddress: "0.0.0.0:37194".to_string(),
            token: link_access_token(),
            webAccessEnabled: false,
            discoveryEnabled: false,
            portMode: LinkAccessHostPortMode::Automatic,
            updatedAt: currentTimeMillis(),
        };
        writeHostConfigPreferences(&store, &config)?;
        Ok(config)
    }

    /// Reads the active Link Access host configuration for this runtime.
    pub fn hostConfig(&self) -> Result<LinkAccessHostConfig, String> {
        hostConfigFromPreferences(
            &self.readPreferences(&self.dataStore(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH))?,
        )
    }

    /// Creates one local datastore for a Link Access preferences path.
    fn dataStore(&self, path: &str) -> CoreNodeStateStore {
        CoreNodeStateStore::newWithStorage(self.storage.clone(), path)
    }

    /// Writes the Space device profile carried by one paired endpoint record.
    #[allow(non_snake_case)]
    fn writePairedDeviceProfile(
        &self,
        deviceId: &str,
        deviceInfo: &LinkDeviceInfo,
    ) -> Result<(), String> {
        let spaceStore = CoreSpaceStore::new(self.storage.clone());
        let displayName = deviceInfo.displayName();
        let profiles = spaceStore.deviceProfiles()?;
        let profile = match profiles.get(deviceId) {
            Some(profile)
                if profile.displayName == displayName
                    && profile.platform == deviceInfo.platform
                    && profile.model == deviceInfo.model =>
            {
                return Ok(());
            }
            Some(profile) => CoreSpaceDeviceProfile {
                nodeId: deviceId.to_string(),
                displayName,
                userName: profile.userName.clone(),
                platform: deviceInfo.platform.clone(),
                model: deviceInfo.model.clone(),
                coreVersion: profile.coreVersion.clone(),
                updatedAt: unix_millis(),
            },
            None => CoreSpaceDeviceProfile {
                nodeId: deviceId.to_string(),
                displayName,
                userName: String::new(),
                platform: deviceInfo.platform.clone(),
                model: deviceInfo.model.clone(),
                coreVersion: None,
                updatedAt: unix_millis(),
            },
        };
        spaceStore.importDeviceProfiles(vec![profile])
    }

    /// Reads one Link Access preferences snapshot.
    fn readPreferences(&self, store: &CoreNodeStateStore) -> Result<Preferences, String> {
        store.data().map_err(|error| error.to_string())
    }

    /// Reads every keyed record from a Link Access datastore.
    fn readRecordMap<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<BTreeMap<String, T>, String> {
        let preferences = self.readPreferences(&self.dataStore(path))?;
        recordMapFromPreferences(preferences).map_err(|error| error.to_string())
    }

    /// Observes every keyed record stored at one Link Access preferences path.
    #[allow(non_snake_case)]
    fn recordMapFlow<T>(&self, path: &str) -> Flow<BTreeMap<String, T>>
    where
        T: serde::de::DeserializeOwned + 'static,
    {
        self.dataStore(path)
            .dataFlow()
            .mapResult(recordMapFromPreferences)
    }

    /// Validates one inbound record against every persisted direction for the same device.
    #[allow(non_snake_case)]
    fn validateInboundSessionRecord(
        &self,
        sessionId: &str,
        record: &AcceptedRemoteSessionRecord,
    ) -> Result<(), String> {
        for (existingSessionId, existing) in self.inboundSessions()? {
            if existingSessionId != sessionId
                && existing.deviceId == record.deviceId
                && existing.deviceInfo != record.deviceInfo
            {
                return Err(format!(
                    "paired device {} has conflicting device information",
                    record.deviceId
                ));
            }
        }
        for existing in self.outboundSessions()?.into_values() {
            if existing.peerNodeId == record.deviceId
                && existing.peerDeviceInfo != record.deviceInfo
            {
                return Err(format!(
                    "paired device {} has conflicting device information",
                    record.deviceId
                ));
            }
        }
        Ok(())
    }

    /// Validates one outbound record against every persisted direction for the same device.
    #[allow(non_snake_case)]
    fn validateOutboundSessionRecord(
        &self,
        name: &str,
        record: &PairedPeerSessionRecord,
    ) -> Result<(), String> {
        for (existingName, existing) in self.outboundSessions()? {
            if existingName == name && existing.peerNodeId != record.peerNodeId {
                return Err("Session name already belongs to another peer".into());
            }
            if existing.peerNodeId == record.peerNodeId
                && existing.peerDeviceInfo != record.peerDeviceInfo
            {
                return Err(format!(
                    "paired device {} has conflicting device information",
                    record.peerNodeId
                ));
            }
        }
        for existing in self.inboundSessions()?.into_values() {
            if existing.deviceId == record.peerNodeId
                && existing.deviceInfo != record.peerDeviceInfo
            {
                return Err(format!(
                    "paired device {} has conflicting device information",
                    record.peerNodeId
                ));
            }
        }
        Ok(())
    }

    /// Writes one keyed record into a Link Access datastore.
    fn writeMapRecord<T: Serialize>(
        &self,
        path: &str,
        name: &str,
        value: &T,
    ) -> Result<(), String> {
        let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
        self.dataStore(path)
            .edit(|preferences| {
                preferences.set(&stringPreferencesKey(name), encoded.clone());
            })
            .map_err(|error| error.to_string())
    }

    /// Removes one keyed record from a Link Access datastore.
    fn removeMapRecord(&self, path: &str, name: &str) -> Result<(), String> {
        self.dataStore(path)
            .edit(|preferences| {
                preferences.remove(&stringPreferencesKey(name));
            })
            .map_err(|error| error.to_string())
    }
}

/// Decodes every keyed JSON record from one preferences snapshot.
#[allow(non_snake_case)]
fn recordMapFromPreferences<T: serde::de::DeserializeOwned>(
    preferences: Preferences,
) -> Result<BTreeMap<String, T>, PreferencesDataStoreError> {
    let mut records = BTreeMap::new();
    for (name, encoded) in preferences.entries() {
        records.insert(name, serde_json::from_str(&encoded)?);
    }
    Ok(records)
}

/// Writes one single-record datastore snapshot.
fn writeSingleRecord<T: Serialize>(store: &CoreNodeStateStore, value: &T) -> Result<(), String> {
    let mut preferences = emptyPreferences();
    preferences.set(
        &stringPreferencesKey(LINK_ACCESS_RECORD_KEY),
        serde_json::to_string(value).map_err(|error| error.to_string())?,
    );
    store
        .replace(preferences)
        .map_err(|error| error.to_string())
}

/// Reads one required preference string.
fn requiredPreference(preferences: &Preferences, key: &str, path: &str) -> Result<String, String> {
    preferences
        .get(&stringPreferencesKey(key))
        .cloned()
        .ok_or_else(|| format!("Link Access store {path} is missing key {key}"))
}

/// Reads one required boolean preference string.
fn requiredBoolPreference(
    preferences: &Preferences,
    key: &str,
    path: &str,
) -> Result<bool, String> {
    requiredPreference(preferences, key, path)?
        .parse::<bool>()
        .map_err(|error| error.to_string())
}

/// Reads one required integer preference string.
fn requiredI64Preference(preferences: &Preferences, key: &str, path: &str) -> Result<i64, String> {
    requiredPreference(preferences, key, path)?
        .parse::<i64>()
        .map_err(|error| error.to_string())
}

/// Converts persisted host config preferences into the typed model.
fn hostConfigFromPreferences(preferences: &Preferences) -> Result<LinkAccessHostConfig, String> {
    Ok(LinkAccessHostConfig {
        bindAddress: requiredPreference(
            preferences,
            LINK_ACCESS_BIND_ADDRESS_KEY,
            RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH,
        )?,
        token: requiredPreference(
            preferences,
            LINK_ACCESS_TOKEN_KEY,
            RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH,
        )?,
        webAccessEnabled: requiredBoolPreference(
            preferences,
            LINK_ACCESS_WEB_ACCESS_ENABLED_KEY,
            RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH,
        )?,
        discoveryEnabled: requiredBoolPreference(
            preferences,
            LINK_ACCESS_DISCOVERY_ENABLED_KEY,
            RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH,
        )?,
        portMode: hostPortModeFromPreference(&requiredPreference(
            preferences,
            LINK_ACCESS_PORT_MODE_KEY,
            RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH,
        )?)?,
        updatedAt: requiredI64Preference(
            preferences,
            LINK_ACCESS_UPDATED_AT_KEY,
            RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH,
        )?,
    })
}

/// Persists one host config through the local datastore API.
fn writeHostConfigPreferences(
    store: &CoreNodeStateStore,
    config: &LinkAccessHostConfig,
) -> Result<(), String> {
    let mut preferences = emptyPreferences();
    preferences.set(
        &stringPreferencesKey(LINK_ACCESS_BIND_ADDRESS_KEY),
        config.bindAddress.clone(),
    );
    preferences.set(
        &stringPreferencesKey(LINK_ACCESS_TOKEN_KEY),
        config.token.clone(),
    );
    preferences.set(
        &stringPreferencesKey(LINK_ACCESS_WEB_ACCESS_ENABLED_KEY),
        config.webAccessEnabled.to_string(),
    );
    preferences.set(
        &stringPreferencesKey(LINK_ACCESS_DISCOVERY_ENABLED_KEY),
        config.discoveryEnabled.to_string(),
    );
    preferences.set(
        &stringPreferencesKey(LINK_ACCESS_PORT_MODE_KEY),
        hostPortModePreference(&config.portMode).to_string(),
    );
    preferences.set(
        &stringPreferencesKey(LINK_ACCESS_UPDATED_AT_KEY),
        config.updatedAt.to_string(),
    );
    store
        .replace(preferences)
        .map_err(|error| error.to_string())
}

/// Returns the persisted literal for one host port mode.
fn hostPortModePreference(value: &LinkAccessHostPortMode) -> &'static str {
    match value {
        LinkAccessHostPortMode::Automatic => "automatic",
        LinkAccessHostPortMode::Fixed => "fixed",
    }
}

/// Parses one host port mode preference literal.
fn hostPortModeFromPreference(value: &str) -> Result<LinkAccessHostPortMode, String> {
    match value {
        "automatic" => Ok(LinkAccessHostPortMode::Automatic),
        "fixed" => Ok(LinkAccessHostPortMode::Fixed),
        other => Err(format!("invalid Link Access host port mode: {other}")),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct RemoteWebAccessConfig {
    pub shutdownToken: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct RemoteLinkState {
    coreNodeTransport: Arc<dyn CoreNodeTransportClient>,
    token: String,
    authority: Arc<operit_peer_link::PairingAuthority>,
    deviceId: String,
    deviceInfo: LinkDeviceInfo,
    accessStore: LinkAccessStore,
    webAccess: Option<RemoteWebAccessState>,
}

#[derive(Clone)]
struct SharedAccessCoreClient {
    core: Arc<StdMutex<Box<dyn CoreNodeLinkClient + Send>>>,
}

impl SharedAccessCoreClient {
    /// Executes one local call on the runtime scheduler.
    async fn callOnRuntime(&self, request: CoreCallRequest) -> CoreCallResponse {
        let requestId = request.requestId.clone();
        let (sender, receiver) = oneshot::channel();
        let core = self.core.clone();
        if let Err(error) = defaultHostRuntimeTaskSchedulerHost().scheduleHostRuntimeAsyncTask(
            "link-access-call",
            Box::new(move || {
                Box::pin(async move {
                    let mut client = core
                        .lock()
                        .expect("Link Access core mutex poisoned")
                        .cloneCoreNodeLinkClient();
                    let response = client.call(request).await;
                    let _ = sender.send(response);
                })
            }),
        ) {
            return CoreCallResponse::err(requestId, CoreLinkError::internal(error.to_string()));
        }
        receiver.await.unwrap_or_else(|error| {
            CoreCallResponse::err(requestId, CoreLinkError::internal(error.to_string()))
        })
    }

    /// Reads one local watch snapshot on the runtime scheduler.
    #[allow(non_snake_case)]
    async fn watchSnapshotOnRuntime(
        &self,
        request: CoreWatchRequest,
    ) -> Result<CoreEvent, CoreLinkError> {
        let (sender, receiver) = oneshot::channel();
        let core = self.core.clone();
        defaultHostRuntimeTaskSchedulerHost()
            .scheduleHostRuntimeAsyncTask(
                "link-access-watch-snapshot",
                Box::new(move || {
                    Box::pin(async move {
                        let mut client = core
                            .lock()
                            .expect("Link Access core mutex poisoned")
                            .cloneCoreNodeLinkClient();
                        let response = client.watchSnapshot(request).await;
                        let _ = sender.send(response);
                    })
                }),
            )
            .map_err(|error| CoreLinkError::internal(error.to_string()))?;
        receiver
            .await
            .map_err(|error| CoreLinkError::internal(error.to_string()))?
    }

    /// Opens one local watch on the runtime scheduler.
    async fn watchOnRuntime(
        &self,
        request: CoreWatchRequest,
    ) -> Result<CoreEventStream, CoreLinkError> {
        let (sender, receiver) = oneshot::channel();
        let core = self.core.clone();
        defaultHostRuntimeTaskSchedulerHost()
            .scheduleHostRuntimeAsyncTask(
                "link-access-watch",
                Box::new(move || {
                    Box::pin(async move {
                        let mut client = core
                            .lock()
                            .expect("Link Access core mutex poisoned")
                            .cloneCoreNodeLinkClient();
                        let response = client.watch(request).await;
                        let _ = sender.send(response);
                    })
                }),
            )
            .map_err(|error| CoreLinkError::internal(error.to_string()))?;
        receiver
            .await
            .map_err(|error| CoreLinkError::internal(error.to_string()))?
    }

    /// Opens one local reverse stream on the runtime scheduler.
    #[allow(non_snake_case)]
    async fn openPushOnRuntime(
        &self,
        request: CorePushRequest,
    ) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
        let (sender, receiver) = oneshot::channel();
        let core = self.core.clone();
        defaultHostRuntimeTaskSchedulerHost()
            .scheduleHostRuntimeAsyncTask(
                "link-access-push-open",
                Box::new(move || {
                    Box::pin(async move {
                        let mut client = core
                            .lock()
                            .expect("Link Access core mutex poisoned")
                            .cloneCoreNodeLinkClient();
                        let response = client.openPush(request).await;
                        let _ = sender.send(response);
                    })
                }),
            )
            .map_err(|error| CoreLinkError::internal(error.to_string()))?;
        receiver
            .await
            .map_err(|error| CoreLinkError::internal(error.to_string()))?
    }
}

#[async_trait]
impl CoreNodeTransportClient for SharedAccessCoreClient {
    /// Executes one local call through the runtime scheduler.
    async fn call(&self, request: CoreCallRequest) -> CoreCallResponse {
        self.callOnRuntime(request).await
    }

    /// Reads one local watch snapshot through the runtime scheduler.
    #[allow(non_snake_case)]
    async fn watchSnapshot(&self, request: CoreWatchRequest) -> Result<CoreEvent, CoreLinkError> {
        self.watchSnapshotOnRuntime(request).await
    }

    /// Opens one local watch through the runtime scheduler.
    async fn watch(&self, request: CoreWatchRequest) -> Result<CoreEventStream, CoreLinkError> {
        self.watchOnRuntime(request).await
    }

    /// Opens one local push through the runtime scheduler.
    #[allow(non_snake_case)]
    async fn openPush(
        &self,
        request: CorePushRequest,
    ) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
        self.openPushOnRuntime(request).await
    }

    /// Executes one routed call through the runtime scheduler.
    #[allow(non_snake_case)]
    async fn routedCall(
        &self,
        previousNodeId: String,
        request: operit_peer_link::RoutedCoreRequest<CoreCallRequest>,
    ) -> CoreCallResponse {
        let requestId = request.payload.requestId.clone();
        let (sender, receiver) = oneshot::channel();
        let core = self.core.clone();
        if let Err(error) = defaultHostRuntimeTaskSchedulerHost().scheduleHostRuntimeAsyncTask(
            "link-access-routed-call",
            Box::new(move || {
                Box::pin(async move {
                    let mut client = core
                        .lock()
                        .expect("Link Access core mutex poisoned")
                        .cloneCoreNodeLinkClient();
                    let response = client.routedCall(previousNodeId, request).await;
                    let _ = sender.send(response);
                })
            }),
        ) {
            return CoreCallResponse::err(requestId, CoreLinkError::internal(error.to_string()));
        }
        receiver.await.unwrap_or_else(|error| {
            CoreCallResponse::err(requestId, CoreLinkError::internal(error.to_string()))
        })
    }

    /// Reads one routed watch snapshot through the runtime scheduler.
    #[allow(non_snake_case)]
    async fn routedWatchSnapshot(
        &self,
        previousNodeId: String,
        request: operit_peer_link::RoutedCoreRequest<CoreWatchRequest>,
    ) -> Result<CoreEvent, CoreLinkError> {
        let (sender, receiver) = oneshot::channel();
        let core = self.core.clone();
        defaultHostRuntimeTaskSchedulerHost()
            .scheduleHostRuntimeAsyncTask(
                "link-access-routed-watch-snapshot",
                Box::new(move || {
                    Box::pin(async move {
                        let mut client = core
                            .lock()
                            .expect("Link Access core mutex poisoned")
                            .cloneCoreNodeLinkClient();
                        let response = client.routedWatchSnapshot(previousNodeId, request).await;
                        let _ = sender.send(response);
                    })
                }),
            )
            .map_err(|error| CoreLinkError::internal(error.to_string()))?;
        receiver
            .await
            .map_err(|error| CoreLinkError::internal(error.to_string()))?
    }

    /// Opens one routed watch through the runtime scheduler.
    #[allow(non_snake_case)]
    async fn routedWatch(
        &self,
        previousNodeId: String,
        request: operit_peer_link::RoutedCoreRequest<CoreWatchRequest>,
    ) -> Result<CoreEventStream, CoreLinkError> {
        let (sender, receiver) = oneshot::channel();
        let core = self.core.clone();
        defaultHostRuntimeTaskSchedulerHost()
            .scheduleHostRuntimeAsyncTask(
                "link-access-routed-watch",
                Box::new(move || {
                    Box::pin(async move {
                        let mut client = core
                            .lock()
                            .expect("Link Access core mutex poisoned")
                            .cloneCoreNodeLinkClient();
                        let response = client.routedWatch(previousNodeId, request).await;
                        let _ = sender.send(response);
                    })
                }),
            )
            .map_err(|error| CoreLinkError::internal(error.to_string()))?;
        receiver
            .await
            .map_err(|error| CoreLinkError::internal(error.to_string()))?
    }

    /// Opens one routed push through the runtime scheduler.
    #[allow(non_snake_case)]
    async fn routedOpenPush(
        &self,
        previousNodeId: String,
        request: operit_peer_link::RoutedCoreRequest<CorePushRequest>,
    ) -> Result<Box<dyn CoreLinkPushSession>, CoreLinkError> {
        let (sender, receiver) = oneshot::channel();
        let core = self.core.clone();
        defaultHostRuntimeTaskSchedulerHost()
            .scheduleHostRuntimeAsyncTask(
                "link-access-routed-push-open",
                Box::new(move || {
                    Box::pin(async move {
                        let mut client = core
                            .lock()
                            .expect("Link Access core mutex poisoned")
                            .cloneCoreNodeLinkClient();
                        let response = client.routedOpenPush(previousNodeId, request).await;
                        let _ = sender.send(response);
                    })
                }),
            )
            .map_err(|error| CoreLinkError::internal(error.to_string()))?;
        receiver
            .await
            .map_err(|error| CoreLinkError::internal(error.to_string()))?
    }
}

/// Wraps a cloneable CoreNode router for use by Send-safe Peer Link callbacks.
#[allow(non_snake_case)]
pub fn coreNodeTransportClient(
    core: impl CoreNodeLinkClient + Send + 'static,
) -> Arc<dyn CoreNodeTransportClient> {
    Arc::new(SharedAccessCoreClient {
        core: Arc::new(StdMutex::new(Box::new(core))),
    })
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct RemoteWebAccessState {
    shutdownToken: String,
    shutdownSender: Arc<StdMutex<Option<oneshot::Sender<()>>>>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
struct RemoteSession {
    deviceId: String,
    deviceInfo: LinkDeviceInfo,
    pairingServiceVersion: i32,
    sessionSecret: Vec<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
struct VerifiedRemoteSession {
    sessionId: String,
    deviceId: String,
}

pub use operit_link::protocol::LinkDeviceInfo;

/// Describes the live device space summary exposed during nearby discovery.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteDeviceSpaceInfo {
    pub spaceId: String,
    pub spaceName: String,
    pub spaceRevision: i64,
    pub deviceCount: usize,
    pub userName: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HelloResponse {
    pub protocolVersion: i32,
    pub pairingServiceVersion: i32,
    pub peerNodeId: String,
    pub peerDeviceInfo: LinkDeviceInfo,
    pub deviceSpace: RemoteDeviceSpaceInfo,
    pub peerPublicKey: String,
    pub transports: Vec<String>,
    pub pairingRequired: bool,
}

pub use operit_peer_link::{LinkPairStartRequest, LinkPairStartResponse, LinkPairFinishRequest, LinkPairFinishResponse};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSessionInfoEnvelope {
    pub nonce: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSpaceAdoptEnvelope {
    pub space: CoreSpace,
    pub deviceProfiles: Vec<CoreSpaceDeviceProfile>,
}

/// Returns the joined Space together with the control commands that establish its policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSpaceAdoptResult {
    pub space: CoreSpace,
    #[serde(default)]
    pub controlOperations: Vec<SyncOperation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSessionInfoResponse {
    pub protocolVersion: i32,
    pub pairingServiceVersion: i32,
    pub peerNodeId: String,
    pub peerDeviceInfo: LinkDeviceInfo,
    pub clientDeviceId: String,
    pub clientDeviceInfo: LinkDeviceInfo,
    pub transports: Vec<String>,
    pub deviceSpace: CoreSpace,
    pub deviceProfiles: Vec<CoreSpaceDeviceProfile>,
    pub nonce: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteWsEnvelope {
    pub protocolVersion: i32,
    pub sessionId: String,
    pub deviceId: String,
    pub signature: String,
    pub requestId: String,
    #[serde(with = "serde_bytes")]
    pub payloadBytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "body")]
pub enum RemoteWsPayload {
    SessionInfo(RemoteSessionInfoEnvelope),
    PeerChannelOpen(PeerChannelOpenEnvelope),
    PeerChannelClose(String),
    PeerFrame { channelId: String, frame: PeerFrame },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "body")]
pub enum RemoteWsResponse {
    SessionInfo(RemoteSessionInfoResponse),
    PeerOpened(String),
    PeerFrame(PeerFrame),
    PeerClosed(String),
    PeerAccepted,
    Error(CoreLinkError),
}

/// Selects the concrete carrier used by one paired remote session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PeerTransport {
    Http,
    #[serde(rename = "ws")]
    WebSocket,
    Tcp,
    Serial,
}

impl PeerTransport {
    pub fn forEndpoint(endpoint: &str) -> Result<Self, String> {
        if endpoint.starts_with("http://") || endpoint.starts_with("https://") { Ok(Self::Http) }
        else if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") { Ok(Self::WebSocket) }
        else if endpoint.starts_with("serial://") { Ok(Self::Serial) }
        else if endpoint.starts_with("tcp://") || (!endpoint.contains("://") && endpoint.contains(':')) { Ok(Self::Tcp) }
        else { Err("Expected http(s), ws(s), tcp or serial peer endpoint".into()) }
    }
    pub fn isFramed(&self) -> bool { matches!(self, Self::Tcp | Self::Serial) }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedPeerSessionRecord {
    pub endpoint: String,
    pub sessionId: String,
    pub deviceId: String,
    pub peerNodeId: String,
    pub peerDeviceInfo: LinkDeviceInfo,
    pub pairingServiceVersion: i32,
    pub sessionSecret: String,
    pub transport: PeerTransport,
}

impl PairedPeerSessionRecord {
    /// Returns this paired session with an updated remote endpoint.
    #[allow(non_snake_case)]
    pub fn withEndpoint(&self, endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            sessionId: self.sessionId.clone(),
            deviceId: self.deviceId.clone(),
            peerNodeId: self.peerNodeId.clone(),
            peerDeviceInfo: self.peerDeviceInfo.clone(),
            pairingServiceVersion: self.pairingServiceVersion,
            sessionSecret: self.sessionSecret.clone(),
            transport: self.transport.clone(),
        }
    }
}

pub use operit_peer_link::pairing::PairStartState;

/// Stores the client-side state needed to finish one outbound pairing after user confirmation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingOutboundPairingRecord {
    pub endpoint: String,
    pub transport: PeerTransport,
    pub state: PairStartState,
}

#[derive(Clone, Debug)]
pub struct RemoteLinkClient {
    endpoint: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl RemoteLinkServer {
    /// Binds and serves one authenticated Link endpoint from its configured address.
    pub async fn serve(
        core: impl CoreNodeLinkClient + Send + 'static,
        config: RemoteLinkServerConfig,
    ) -> Result<(), String> {
        let address: SocketAddr = config
            .bindAddress
            .parse()
            .map_err(|error| format!("invalid bind address: {error}"))?;
        let listener = TcpListener::bind(address)
            .await
            .map_err(|error| error.to_string())?;
        Self::serveWithListener(core, config, listener, address).await
    }

    /// Serves one authenticated Link endpoint from an already bound listener.
    #[allow(non_snake_case)]
    pub async fn serveWithListener(
        core: impl CoreNodeLinkClient + Send + 'static,
        config: RemoteLinkServerConfig,
        listener: TcpListener,
        address: SocketAddr,
    ) -> Result<(), String> {
        CoreSpaceStore::new(config.accessStore.storage.clone()).initialize()?;
        let authority = pairingAuthority(&config.token, &config.deviceId, config.deviceInfo.clone(), config.accessStore.clone());
        let webAccessConfig = config.webAccess.clone();
        let (shutdownSender, shutdownReceiver) = oneshot::channel::<()>();
        let webAccess = webAccessConfig.clone().map(|value| RemoteWebAccessState {
            shutdownToken: value.shutdownToken,
            shutdownSender: Arc::new(StdMutex::new(Some(shutdownSender))),
        });
        let core = Arc::new(StdMutex::new(
            Box::new(core) as Box<dyn CoreNodeLinkClient + Send>
        ));
        let state = RemoteLinkState {
            coreNodeTransport: Arc::new(SharedAccessCoreClient { core }),
            token: config.token.clone(),
            authority,
            deviceId: config.deviceId.clone(),
            deviceInfo: config.deviceInfo.clone(),
            accessStore: config.accessStore.clone(),
            webAccess,
        };
        let mut app = Router::new()
            .route("/link/hello", get(hello))
            .route("/link/pair/start", post(pair_start))
            .route("/link/pair/finish", post(pair_finish))
            .route("/link/session", post(session_info))
            .route("/link/space/adopt", post(space_adopt))
            .route("/link/peer/channel/events", post(peer_channel_events))
            .route("/link/peer/channel/frame", post(peer_channel_frame))
            .route("/link/ws", get(ws));
        if webAccessConfig.is_some() {
            app = app.route("/client/web-access/close", post(web_access_close));
        }
        let app = app.with_state(state);
        if config.printStartupInfo {
            println!("operit link server listening on http://{address}");
            println!("link token: {}", config.token);
        }
        if webAccessConfig.is_some() {
            return axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdownReceiver.await;
                })
                .await
                .map_err(|error| error.to_string());
        }
        axum::serve(listener, app)
            .await
            .map_err(|error| error.to_string())
    }
}

impl RemoteLinkClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
        }
    }

    pub async fn hello(&self, tokenHash: &str) -> Result<HelloResponse, String> {
        decodeRemoteHttpJson(remoteHttpRequest(
            "GET",
            format!("{}/link/hello", self.endpoint),
            vec![(
                "x-operit-link-token-hash".to_string(),
                tokenHash.to_string(),
            )],
            Vec::new(),
        )?)
    }

    pub async fn pairStart(
        &self,
        tokenHash: &str,
        clientDeviceId: String,
        clientDeviceInfo: LinkDeviceInfo,
    ) -> Result<PairStartState, String> {
        self.pairStartInternal(tokenHash, clientDeviceId, clientDeviceInfo, false)
            .await
    }

    /// Starts one pairing transaction with an explicit bootstrap mode.
    async fn pairStartInternal(
        &self,
        tokenHash: &str,
        clientDeviceId: String,
        clientDeviceInfo: LinkDeviceInfo,
        autoBootstrap: bool,
    ) -> Result<PairStartState, String> {
        let client = operit_peer_link::pairing::PairingClient::new(tokenHash.into(), clientDeviceId, clientDeviceInfo, autoBootstrap)?;
        let response: LinkPairStartResponse = decodeRemoteHttpJson(remoteHttpJsonRequest(
            format!("{}/link/pair/start", self.endpoint), &client.request())?)?;
        client.accept(response)
    }

    pub async fn pairFinish(
        &self,
        state: &PairStartState,
        pairingCode: &str,
    ) -> Result<PairedRemoteSession, String> {
        self.finishPairingRequest(state, state.finishRequest(pairingCode.into(), None)).await
    }

    async fn finishPairingRequest(&self, state: &PairStartState, request: LinkPairFinishRequest) -> Result<PairedRemoteSession, String> {
        let response: LinkPairFinishResponse = decodeRemoteHttpJson(remoteHttpJsonRequest(
            format!("{}/link/pair/finish", self.endpoint), &request)?)?;
        let session = state.acceptFinish(response)?;
        Ok(PairedRemoteSession { endpoint: self.endpoint.clone(), sessionId: session.sessionId,
            deviceId: session.deviceId, peerNodeId: session.peerDeviceId, peerDeviceInfo: session.peerDeviceInfo,
            pairingServiceVersion: session.pairingServiceVersion, transport: PeerTransport::Http, sessionSecret: session.sessionSecret })
    }

    pub async fn pairBootstrap(&self, tokenHash: &str, clientDeviceId: String, clientDeviceInfo: LinkDeviceInfo) -> Result<PairedRemoteSession, String> {
        let state = self.pairStartInternal(tokenHash, clientDeviceId, clientDeviceInfo, true).await?;
        let request = state.finishRequest(String::new(), Some(tokenHash.into()));
        self.finishPairingRequest(&state, request).await
    }

}

/// Executes one authenticated Link HTTP request through the configured runtime host.
fn remoteHttpRequest(
    method: &str,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
) -> Result<Vec<u8>, String> {
    remoteHttpRequestWithReadTimeout(method, url, headers, body, LINK_HTTP_READ_TIMEOUT_SECONDS)
}

/// Executes one authenticated Link HTTP request with an explicit host read window.
fn remoteHttpRequestWithReadTimeout(
    method: &str,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    readTimeoutSeconds: u64,
) -> Result<Vec<u8>, String> {
    let response = defaultHttpHost()
        .executeHttpRequest(HttpRequestData {
            url: url.clone(),
            method: method.to_string(),
            headers,
            body,
            formFields: Vec::new(),
            fileParts: Vec::new(),
            connectTimeoutSeconds: 10,
            readTimeoutSeconds,
            followRedirects: false,
            ignoreSsl: false,
            proxyHost: String::new(),
            proxyPort: 0,
        })
        .map_err(|error| error.to_string())?;
    if !(200..300).contains(&response.statusCode) {
        let description =
            remoteHttpErrorDescription(method, response.statusCode, &url, &response.body);
        return Err(description);
    }
    Ok(response.body)
}

/// Formats one failed Link HTTP response while preserving structured session-auth metadata.
fn remoteHttpErrorDescription(method: &str, statusCode: i32, url: &str, body: &[u8]) -> String {
    let base = format!("Link HTTP {method} {url} failed with status {statusCode}");
    let Ok(error) = operit_link::decodeLink::<CoreLinkError>(body) else {
        return format!("{base}: {}", String::from_utf8_lossy(body));
    };
    if let Some(reason) = coreLinkRemoteSessionAuthReason(&error) {
        return format!(
            "REMOTE_SESSION_AUTH/{reason}: {base}: {}: {}",
            error.code, error.message
        );
    }
    format!("{base}: {}: {}", error.code, error.message)
}

/// Formats one structured Link error and retains remote-session auth metadata.
fn remoteCoreLinkErrorDescription(error: &CoreLinkError) -> String {
    if let Some(reason) = coreLinkRemoteSessionAuthReason(&error) {
        return format!(
            "REMOTE_SESSION_AUTH/{reason}: {}: {}",
            error.code, error.message
        );
    }
    format!("{}: {}", error.code, error.message)
}

/// Extracts the stable auth reason from one structured remote-session Link error.
fn coreLinkRemoteSessionAuthReason(error: &CoreLinkError) -> Option<&str> {
    let Some(operit_link::CoreValue::Map(details)) = error.details.as_ref() else {
        return None;
    };
    let Some(operit_link::CoreValue::String(errorType)) = details.get("type") else {
        return None;
    };
    if errorType != "remote_session_auth" {
        return None;
    }
    let Some(operit_link::CoreValue::String(reason)) = details.get("authReason") else {
        return None;
    };
    Some(reason.as_str())
}

/// Returns the structured auth reason preserved in one remote Link error string.
pub fn remoteSessionAuthReason(error: &str) -> Option<&str> {
    let rest = error.strip_prefix("REMOTE_SESSION_AUTH/")?;
    let (reason, _) = rest.split_once(": ")?;
    Some(reason)
}

/// Encodes one JSON Link control request and executes it through the runtime HTTP host.
fn remoteHttpJsonRequest<T: Serialize>(url: String, request: &T) -> Result<Vec<u8>, String> {
    remoteHttpRequest(
        "POST",
        url,
        vec![("content-type".to_string(), "application/json".to_string())],
        serde_json::to_vec(request).map_err(|error| error.to_string())?,
    )
}

/// Decodes a JSON Link control response from host HTTP response bytes.
fn decodeRemoteHttpJson<T: serde::de::DeserializeOwned>(bytes: Vec<u8>) -> Result<T, String> {
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

#[derive(Clone)]
pub struct PairedRemoteSession {
    endpoint: String,
    pub sessionId: String,
    pub deviceId: String,
    pub peerNodeId: String,
    pub peerDeviceInfo: LinkDeviceInfo,
    pub pairingServiceVersion: i32,
    pub transport: PeerTransport,
    sessionSecret: Vec<u8>,
}

/// Owns one authenticated WebSocket used by a single Link carrier operation.
pub(crate) struct RemoteWsConnection {
    host: Arc<dyn WebSocketHost>,
    streamId: String,
    sessionId: String,
    deviceId: String,
    sessionSecret: Vec<u8>,
    receiver: Mutex<tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>>,
}

struct PendingWebSocketOpen {
    host: Arc<dyn WebSocketHost>,
    streamId: Option<String>,
}
impl Drop for PendingWebSocketOpen {
    fn drop(&mut self) {
        if let Some(id) = self.streamId.take() {
            let _ = self.host.closeWebSocket(&id);
        }
    }
}

impl RemoteWsConnection {
    /// Opens one authenticated WebSocket through the configured host capability.
    pub(crate) async fn open(
        session: &PairedRemoteSession,
        streamLabel: &str,
    ) -> Result<Arc<Self>, String> {
        let streamId = format!("link-ws-{streamLabel}-{}", Uuid::new_v4().simple());
        let (messageSender, messageReceiver) = tokio::sync::mpsc::unbounded_channel();
        let (openedSender, openedReceiver) = tokio::sync::oneshot::channel();
        let openedSignal = Arc::new(StdMutex::new(Some(openedSender)));
        let openedForClose = openedSignal.clone();
        let openedForOpen = openedSignal.clone();
        let messageCallback: WebSocketMessageCallback = Arc::new(move |message| {
            let _ = messageSender.send(message);
        });
        let openedCallback: WebSocketOpenedCallback = Arc::new(move || {
            if let Some(sender) = openedForOpen
                .lock()
                .expect("WebSocket open signal lock poisoned")
                .take()
            {
                let _ = sender.send(Ok(()));
            }
        });
        let closedCallback: WebSocketClosedCallback = Arc::new(move |result| {
            if let Some(sender) = openedForClose
                .lock()
                .expect("WebSocket open signal lock poisoned")
                .take()
            {
                let _ = sender.send(result);
            }
        });
        let mut pending = PendingWebSocketOpen {
            host: defaultWebSocketHost(),
            streamId: Some(streamId.clone()),
        };
        let openResult = defaultWebSocketHost().openWebSocket(
            streamId.clone(),
            WebSocketRequestData {
                url: webSocketUrl(&session.endpoint)?,
                headers: Vec::new(),
                connectTimeoutSeconds: 10,
                ignoreSsl: false,
            },
            openedCallback,
            messageCallback,
            closedCallback,
        );
        if let Err(error) = openResult {
            return Err(error.to_string());
        }
        openedReceiver
            .await
            .map_err(|error| format!("WebSocket open signal closed: {error}"))??;
        pending.streamId.take();
        Ok(Arc::new(Self {
            host: defaultWebSocketHost(),
            streamId,
            sessionId: session.sessionId.clone(),
            deviceId: session.deviceId.clone(),
            sessionSecret: session.sessionSecret.clone(),
            receiver: Mutex::new(messageReceiver),
        }))
    }

    /// Sends one signed Link payload through the opened WebSocket.
    pub(crate) fn sendPayload(&self, payload: RemoteWsPayload) -> Result<(), String> {
        let payloadBytes = operit_link::encodeLink(&payload).map_err(|error| error.to_string())?;
        let envelope = RemoteWsEnvelope {
            protocolVersion: 4,
            sessionId: self.sessionId.clone(),
            deviceId: self.deviceId.clone(),
            signature: signSession(&self.sessionSecret, &payloadBytes),
            requestId: Uuid::new_v4().to_string(),
            payloadBytes,
        };
        let body = operit_link::encodeLink(&envelope).map_err(|error| error.to_string())?;
        self.host
            .sendWebSocketMessage(&self.streamId, body)
            .map_err(|error| error.to_string())
    }

    /// Waits for one typed WebSocket response from the remote Link endpoint.
    pub(crate) async fn nextResponse(&self) -> Result<RemoteWsResponse, String> {
        let mut receiver = self.receiver.lock().await;
        let message = receiver
            .recv()
            .await
            .ok_or_else(|| "WebSocket closed before producing a response".to_string())?;
        operit_link::decodeLink(&message).map_err(|error| error.to_string())
    }

    /// Closes the host-owned WebSocket carrier.
    pub(crate) fn close(&self) -> Result<(), String> {
        self.host
            .closeWebSocket(&self.streamId)
            .map_err(|error| error.to_string())
    }
}

/// Converts one authenticated HTTP Link endpoint into its WebSocket endpoint.
fn webSocketUrl(endpoint: &str) -> Result<String, String> {
    if let Some(rest) = endpoint.strip_prefix("https://") {
        return Ok(format!("wss://{rest}/link/ws"));
    }
    if let Some(rest) = endpoint.strip_prefix("http://") {
        return Ok(format!("ws://{rest}/link/ws"));
    }
    Err("Link base URL must use http:// or https://".to_string())
}

impl PairedRemoteSession {
    #[allow(non_snake_case)]
    pub fn exportRecord(&self) -> PairedPeerSessionRecord {
        PairedPeerSessionRecord {
            endpoint: self.endpoint.clone(),
            sessionId: self.sessionId.clone(),
            deviceId: self.deviceId.clone(),
            peerNodeId: self.peerNodeId.clone(),
            peerDeviceInfo: self.peerDeviceInfo.clone(),
            pairingServiceVersion: self.pairingServiceVersion,
            sessionSecret: BASE64.encode(&self.sessionSecret),
            transport: self.transport.clone(),
        }
    }

    #[allow(non_snake_case)]
    pub fn fromRecord(record: PairedPeerSessionRecord) -> Result<Self, String> {
        Ok(Self {
            endpoint: record.endpoint.trim_end_matches('/').to_string(),
            sessionId: record.sessionId,
            deviceId: record.deviceId,
            peerNodeId: record.peerNodeId,
            peerDeviceInfo: record.peerDeviceInfo,
            pairingServiceVersion: record.pairingServiceVersion,
            transport: record.transport,
            sessionSecret: BASE64
                .decode(record.sessionSecret)
                .map_err(|error| error.to_string())?,
        })
    }

    #[allow(non_snake_case)]
    pub async fn sessionInfo(&self) -> Result<RemoteSessionInfoResponse, String> {
        match self.transport {
            PeerTransport::Http => self.sessionInfoHttp().await,
            PeerTransport::WebSocket => self.sessionInfoWebSocket().await,
            PeerTransport::Tcp | PeerTransport::Serial => Err("Session control requires an HTTP/WebSocket endpoint".into()),
        }
    }

    /// Adopts one joined Space projection through the authenticated pairing control plane.
    pub async fn adoptDeviceSpace(
        &self,
        space: CoreSpace,
        deviceProfiles: Vec<CoreSpaceDeviceProfile>,
    ) -> Result<RemoteSpaceAdoptResult, String> {
        let body = operit_link::encodeLink(&RemoteSpaceAdoptEnvelope {
            space,
            deviceProfiles,
        })
        .map_err(|error| error.to_string())?;
        operit_link::decodeLink(&self.signedRemotePost("space/adopt", body)?)
            .map_err(|error| error.to_string())
    }

    /// Reads session metadata through the HTTP Link carrier.
    #[allow(non_snake_case)]
    async fn sessionInfoHttp(&self) -> Result<RemoteSessionInfoResponse, String> {
        let body = operit_link::encodeLink(&RemoteSessionInfoEnvelope {
            nonce: Uuid::new_v4().to_string(),
        })
        .map_err(|error| error.to_string())?;
        operit_link::decodeLink(&self.signedRemotePostWithReadTimeout(
            "session",
            body,
            SESSION_INFO_READ_TIMEOUT_SECONDS,
        )?)
        .map_err(|error| error.to_string())
    }

    /// Reads session metadata through a dedicated WebSocket carrier.
    #[allow(non_snake_case)]
    async fn sessionInfoWebSocket(&self) -> Result<RemoteSessionInfoResponse, String> {
        let connection = RemoteWsConnection::open(self, "session").await?;
        connection.sendPayload(RemoteWsPayload::SessionInfo(RemoteSessionInfoEnvelope {
            nonce: Uuid::new_v4().to_string(),
        }))?;
        let response = tokio::select! {
            response = connection.nextResponse() => response?,
            delay = defaultHostRuntimeTaskSchedulerHost().waitForHostRuntimeDelay(SESSION_INFO_READ_TIMEOUT_SECONDS * 1000) => {
                let _ = connection.close();
                delay.map_err(|error| error.to_string())?;
                return Err("Remote session info timed out".to_string());
            }
        };
        let _ = connection.close();
        match response {
            RemoteWsResponse::SessionInfo(value) => Ok(value),
            RemoteWsResponse::Error(error) => Err(remoteCoreLinkErrorDescription(&error)),
            _ => Err("unexpected WebSocket session response".to_string()),
        }
    }

    /// Sends one signed HTTP Link request with the standard host read window.
    fn signedRemotePost(&self, path: &str, body: Vec<u8>) -> Result<Vec<u8>, String> {
        self.signedRemotePostWithReadTimeout(path, body, LINK_HTTP_READ_TIMEOUT_SECONDS)
    }

    /// Sends one signed HTTP Link request with an explicit host read window.
    fn signedRemotePostWithReadTimeout(
        &self,
        path: &str,
        body: Vec<u8>,
        readTimeoutSeconds: u64,
    ) -> Result<Vec<u8>, String> {
        remoteHttpRequestWithReadTimeout(
            "POST",
            format!("{}/link/{path}", self.endpoint),
            vec![
                ("x-operit-link-version".to_string(), "4".to_string()),
                ("x-operit-session".to_string(), self.sessionId.clone()),
                ("x-operit-device".to_string(), self.deviceId.clone()),
                (
                    "x-operit-signature".to_string(),
                    signSession(&self.sessionSecret, &body),
                ),
            ],
            body,
            readTimeoutSeconds,
        )
    }
}

/// Returns the authenticated CoreNode and its current live Space identity.
#[cfg(not(target_arch = "wasm32"))]
async fn hello(State(state): State<RemoteLinkState>, headers: HeaderMap) -> Response {
    if !token_matches(&state, &headers) {
        return unauthorized("invalid token");
    }
    let spaceStore = CoreSpaceStore::new(state.accessStore.storage.clone());
    let space = match spaceStore.initialize() {
        Ok(space) => space,
        Err(error) => return internal_server_error(error),
    };
    let profiles = match spaceStore.deviceProfiles() {
        Ok(profiles) => profiles,
        Err(error) => return internal_server_error(error),
    };
    let profile = match profiles.get(&state.deviceId) {
        Some(profile) => profile,
        None => return internal_server_error("Current device profile is not initialized"),
    };
    Json(HelloResponse {
        protocolVersion: 4,
        pairingServiceVersion: REMOTE_PAIRING_SERVICE_VERSION,
        peerNodeId: state.deviceId,
        peerDeviceInfo: state.deviceInfo,
        deviceSpace: RemoteDeviceSpaceInfo {
            spaceId: space.spaceId,
            spaceName: space.spaceName,
            spaceRevision: space.spaceRevision,
            deviceCount: space.members.len(),
            userName: profile.userName.clone(),
        },
        peerPublicKey: state.authority.publicKey(),
        transports: vec!["http".to_string(), "ws".to_string()],
        pairingRequired: true,
    })
    .into_response()
}

#[cfg(not(target_arch = "wasm32"))]
async fn pair_start(State(state): State<RemoteLinkState>, Json(request): Json<LinkPairStartRequest>) -> Response {
    startPairingHttp(&state.authority, &state.accessStore, request)
}

#[cfg(not(target_arch = "wasm32"))]
async fn pair_finish(State(state): State<RemoteLinkState>, Json(request): Json<LinkPairFinishRequest>) -> Response {
    finishPairingHttp(&state.authority, &state.accessStore, request)
}

#[cfg(not(target_arch = "wasm32"))]
async fn session_info(
    State(state): State<RemoteLinkState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let verified = match verify_session(&state, &headers, &body).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let envelope = match operit_link::decodeLink::<RemoteSessionInfoEnvelope>(&body) {
        Ok(value) => value,
        Err(error) => {
            return encode_link_response(
                StatusCode::BAD_REQUEST,
                CoreLinkError::new("BAD_REQUEST", error.to_string()),
            );
        }
    };
    let spaceStore = CoreSpaceStore::new(state.accessStore.storage.clone());
    let space = match spaceStore.initialize() {
        Ok(value) => value,
        Err(error) => return internal_server_error(error),
    };
    let deviceProfiles = match spaceStore.deviceProfilesForCurrentSpace() {
        Ok(value) => value,
        Err(error) => return internal_server_error(error),
    };
    let sessions = match state.accessStore.inboundSessions() { Ok(s) => s, Err(e) => return internal_server_error(e) };
    let Some(session) = sessions.get(&verified.sessionId) else {
        return encode_link_response(
            StatusCode::UNAUTHORIZED,
            remote_session_auth_error("invalid session", "invalid_session"),
        );
    };
    encode_link_response(
        StatusCode::OK,
        RemoteSessionInfoResponse {
            protocolVersion: 4,
            pairingServiceVersion: session.pairingServiceVersion,
            peerNodeId: state.deviceId,
            peerDeviceInfo: state.deviceInfo,
            clientDeviceId: session.deviceId.clone(),
            clientDeviceInfo: session.deviceInfo.clone(),
            transports: vec!["http".to_string(), "ws".to_string()],
            deviceSpace: space,
            deviceProfiles,
            nonce: envelope.nonce,
        },
    )
}

#[cfg(not(target_arch = "wasm32"))]
struct ServerPeerFrameSender {
    sender: StdMutex<Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>,
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl PeerFrameSender for ServerPeerFrameSender {
    /// Queues one length-prefixed frame for the connected CoreNode.
    async fn send(&self, frame: PeerFrame) -> Result<(), String> {
        self.sender
            .lock()
            .map_err(|error| error.to_string())?
            .as_ref()
            .ok_or_else(|| "Peer Link response stream is closed".to_string())?
            .send(encodePeerFrame(&frame)?)
            .map_err(|_| "Peer Link response stream is closed".to_string())
    }

    /// Ends the server response stream owned by this carrier.
    fn close(&self) {
        let _ = self
            .sender
            .lock()
            .expect("Peer Link response sender lock poisoned")
            .take();
    }
}

/// Adapts queued Peer Link frames into an Axum response body stream.
#[cfg(not(target_arch = "wasm32"))]
struct ServerPeerFrameStream {
    receiver: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    connection: Arc<PeerConnection>,
}

#[cfg(not(target_arch = "wasm32"))]
impl FuturesStream for ServerPeerFrameStream {
    type Item = Result<Bytes, Infallible>;

    /// Polls the next queued Peer Link frame.
    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver)
            .poll_recv(context)
            .map(|item| item.map(|bytes| Ok(Bytes::from(bytes))))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ServerPeerFrameStream {
    /// Removes the active Peer Link when its HTTP response stream closes.
    fn drop(&mut self) {
        operit_util::AppLogger::AppLogger::w(
            "PeerCarrierTrace",
            "server_response_stream_drop reason=Peer Link response stream closed",
        );
        self.connection
            .close("Peer Link response stream closed".to_string());
    }
}

/// Opens the server-to-client event stream for one authenticated direct Peer Link.
#[cfg(not(target_arch = "wasm32"))]
async fn peer_channel_events(
    State(state): State<RemoteLinkState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let verified = match verify_session(&state, &headers, &body).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let envelope = match operit_link::decodeLink::<PeerChannelOpenEnvelope>(&body) {
        Ok(value) => value,
        Err(error) => {
            return encode_link_response(
                StatusCode::BAD_REQUEST,
                CoreLinkError::new("BAD_REQUEST", error.to_string()),
            );
        }
    };
    if envelope.channelId.trim().is_empty() {
        return encode_link_response(
            StatusCode::BAD_REQUEST,
            CoreLinkError::new("BAD_REQUEST", "Peer Link channel id must not be empty"),
        );
    }
    let spaceStore = CoreSpaceStore::new(state.accessStore.storage.clone());
    match spaceStore.contains(verified.deviceId.clone()) {
        Ok(true) => {}
        Ok(false) => {
            return encode_link_response(
                StatusCode::FORBIDDEN,
                CoreLinkError::new(
                    "SPACE_MEMBER_REQUIRED",
                    "Paired device is not in this device space",
                ),
            );
        }
        Err(error) => return internal_server_error(error),
    }
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let connection = PeerConnection::new(
        state.deviceId.clone(),
        verified.deviceId,
        envelope.channelId,
        Arc::new(ServerPeerFrameSender {
            sender: StdMutex::new(Some(sender)),
        }),
        state.coreNodeTransport.clone(),
        Some(Arc::new(SpacePeerObserver(spaceStore))),
    );
    if let Err(error) = registerPeerLink(connection.clone()) {
        return encode_link_response(
            StatusCode::CONFLICT,
            CoreLinkError::new("PEER_LINK_ALREADY_ACTIVE", error),
        );
    }
    Response::builder()
        .header("content-type", "application/msgpack-seq")
        .body(Body::from_stream(ServerPeerFrameStream {
            receiver,
            connection,
        }))
        .expect("Peer Link channel response must build")
}

/// Receives one ordered batch of client-to-server frames for an active Peer Link.
#[cfg(not(target_arch = "wasm32"))]
async fn peer_channel_frame(
    State(state): State<RemoteLinkState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let verified = match verify_session(&state, &headers, &body).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let batch = match operit_link::decodeLink::<PeerFrameBatch>(&body) {
        Ok(value) => value,
        Err(error) => {
            return encode_link_response(
                StatusCode::BAD_REQUEST,
                CoreLinkError::new("PEER_FRAME_BATCH_INVALID", error.to_string()),
            );
        }
    };
    if batch.frames.is_empty() {
        return encode_link_response(
            StatusCode::BAD_REQUEST,
            CoreLinkError::new(
                "PEER_FRAME_BATCH_EMPTY",
                "Peer frame batch must not be empty",
            ),
        );
    }
    for frame in batch.frames {
        if let Err(error) = receivePeerFrame(&state.deviceId, &verified.deviceId, frame).await {
            return encode_link_response(
                StatusCode::BAD_REQUEST,
                CoreLinkError::new("PEER_FRAME_REJECTED", error),
            );
        }
    }
    encode_link_response(StatusCode::OK, serde_json::json!({ "ok": true }))
}

/// Applies one authenticated Space projection during the pairing join handshake.
#[cfg(not(target_arch = "wasm32"))]
async fn space_adopt(
    State(state): State<RemoteLinkState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let verified = match verify_session(&state, &headers, &body).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let envelope = match operit_link::decodeLink::<RemoteSpaceAdoptEnvelope>(&body) {
        Ok(value) => value,
        Err(error) => {
            return encode_link_response(
                StatusCode::BAD_REQUEST,
                CoreLinkError::new("BAD_REQUEST", error.to_string()),
            )
        }
    };
    match acceptAuthenticatedSpaceJoin(
        state.accessStore.storage.clone(),
        &verified.deviceId,
        envelope,
    ) {
        Ok(result) => encode_link_response(StatusCode::OK, result),
        Err(error) => encode_link_response(StatusCode::CONFLICT, CoreLinkError::internal(error)),
    }
}

/// Accepts exactly one authenticated paired device into the server's current Space.
#[cfg(not(target_arch = "wasm32"))]
fn acceptAuthenticatedSpaceJoin(
    storage: Arc<dyn RuntimeStorageHost>,
    joiningNodeId: &str,
    envelope: RemoteSpaceAdoptEnvelope,
) -> Result<RemoteSpaceAdoptResult, String> {
    let spaceStore = CoreSpaceStore::new(storage.clone());
    let currentSpace = spaceStore.initialize()?;
    let joiningExistingMember = currentSpace
        .members
        .iter()
        .any(|nodeId| nodeId == joiningNodeId);
    let mut expectedMembers = currentSpace
        .members
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    expectedMembers.insert(joiningNodeId.to_string());
    let proposedMembers = envelope
        .space
        .members
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if proposedMembers != expectedMembers {
        return Err(
            "join proposal members must equal the current Space plus the authenticated device"
                .to_string(),
        );
    }
    if envelope.space.spaceId != currentSpace.spaceId
        || envelope.space.spaceName != currentSpace.spaceName
    {
        return Err("join proposal must preserve the server Space identity".to_string());
    }
    if joiningExistingMember && envelope.space.spaceRevision != currentSpace.spaceRevision {
        return Err(
            "existing Space member must not change the Space revision during pairing".to_string(),
        );
    }
    if !joiningExistingMember
        && envelope.space.spaceRevision
            != currentSpace
                .spaceRevision
                .checked_add(1)
                .ok_or_else(|| "Space revision overflow during join".to_string())?
    {
        return Err("new Space member must advance the Space revision exactly once".to_string());
    }
    let control = NetworkControlStore::new(storage.clone())?;
    if !joiningExistingMember {
        control.admitMember(joiningNodeId.to_string())?;
    }
    let controlOperations = control.currentSpaceOperations()?;
    spaceStore.importDeviceProfiles(envelope.deviceProfiles)?;
    Ok(RemoteSpaceAdoptResult {
        space: spaceStore.adopt(envelope.space)?,
        controlOperations,
    })
}

#[cfg(not(target_arch = "wasm32"))]
async fn ws(State(state): State<RemoteLinkState>, upgrade: WebSocketUpgrade) -> Response {
    upgrade
        .on_upgrade(move |socket| handle_ws(socket, state))
        .into_response()
}

#[cfg(not(target_arch = "wasm32"))]
async fn handle_ws(mut socket: WebSocket, state: RemoteLinkState) {
    while let Some(Ok(message)) = socket.recv().await {
        match message {
            Message::Binary(bytes) => {
                let envelope = match operit_link::decodeLink::<RemoteWsEnvelope>(&bytes) {
                    Ok(value) => value,
                    Err(error) => {
                        let response = operit_link::encodeLink(RemoteWsResponse::Error(
                            CoreLinkError::new("BAD_REQUEST", error.to_string()),
                        ))
                        .expect("RemoteWsResponse must serialize");
                        let _ = socket.send(Message::Binary(response)).await;
                        continue;
                    }
                };
                let payload =
                    match operit_link::decodeLink::<RemoteWsPayload>(&envelope.payloadBytes) {
                        Ok(value) => value,
                        Err(error) => {
                            let response = operit_link::encodeLink(RemoteWsResponse::Error(
                                CoreLinkError::new("BAD_REQUEST", error.to_string()),
                            ))
                            .expect("RemoteWsResponse must serialize");
                            let _ = socket.send(Message::Binary(response)).await;
                            continue;
                        }
                    };
                match payload {
                    #[cfg(not(target_arch = "wasm32"))]
                    RemoteWsPayload::PeerChannelOpen(request) => {
                        handle_ws_peer(&mut socket, &state, envelope, request).await;
                        return;
                    }
                    RemoteWsPayload::SessionInfo(_) => {
                        let response = handle_ws_binary(&state, &bytes).await;
                        let _ = socket.send(Message::Binary(response)).await;
                    }
                    _ => {
                        let response =
                            operit_link::encodeLink(RemoteWsResponse::Error(CoreLinkError::new(
                                "WS_STREAM_MODE_REMOVED",
                                "Legacy remote Link streams are removed",
                            )))
                            .expect("RemoteWsResponse must serialize");
                        let _ = socket.send(Message::Binary(response)).await;
                    }
                }
            }
            Message::Close(frame) => {
                let _ = socket.send(Message::Close(frame)).await;
                break;
            }
            _ => {}
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn send_ws_response(
    socket: &mut WebSocket,
    response: RemoteWsResponse,
) -> Result<(), String> {
    let bytes = operit_link::encodeLink(response).map_err(|error| error.to_string())?;
    socket
        .send(Message::Binary(bytes))
        .await
        .map_err(|error| error.to_string())
}

/// Verifies one signed WebSocket request and returns its authenticated session.
#[cfg(not(target_arch = "wasm32"))]
async fn verify_ws_envelope(
    state: &RemoteLinkState,
    envelope: &RemoteWsEnvelope,
) -> Result<VerifiedRemoteSession, CoreLinkError> {
    if envelope.protocolVersion != 4 {
        return Err(CoreLinkError::new(
            "LINK_VERSION_MISMATCH",
            "Link protocol version 4 is required",
        ));
    }
    verify_session_parts(
        state,
        &envelope.sessionId,
        &envelope.deviceId,
        &envelope.signature,
        &envelope.payloadBytes,
    )
    .await
}

/// Sends and receives one authenticated Peer Link over a WebSocket connection.
#[cfg(not(target_arch = "wasm32"))]
async fn handle_ws_peer(
    socket: &mut WebSocket,
    state: &RemoteLinkState,
    envelope: RemoteWsEnvelope,
    request: PeerChannelOpenEnvelope,
) {
    let verified = match verify_ws_envelope(state, &envelope).await {
        Ok(value) => value,
        Err(error) => {
            let _ = send_ws_response(socket, RemoteWsResponse::Error(error)).await;
            return;
        }
    };
    if request.channelId.trim().is_empty() {
        let _ = send_ws_response(
            socket,
            RemoteWsResponse::Error(CoreLinkError::new(
                "BAD_REQUEST",
                "Peer Link channel id must not be empty",
            )),
        )
        .await;
        return;
    }
    let spaceStore = CoreSpaceStore::new(state.accessStore.storage.clone());
    match spaceStore.contains(verified.deviceId.clone()) {
        Ok(true) => {}
        Ok(false) => {
            let _ = send_ws_response(
                socket,
                RemoteWsResponse::Error(CoreLinkError::new(
                    "SPACE_MEMBER_REQUIRED",
                    "Paired device is not in this device space",
                )),
            )
            .await;
            return;
        }
        Err(error) => {
            let _ = send_ws_response(
                socket,
                RemoteWsResponse::Error(CoreLinkError::internal(error)),
            )
            .await;
            return;
        }
    }
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let connection = PeerConnection::new(
        state.deviceId.clone(),
        verified.deviceId.clone(),
        request.channelId.clone(),
        Arc::new(WsPeerFrameSender {
            sender: StdMutex::new(Some(sender)),
        }),
        state.coreNodeTransport.clone(),
        Some(Arc::new(SpacePeerObserver(spaceStore))),
    );
    if let Err(error) = registerPeerLink(connection.clone()) {
        let _ = send_ws_response(
            socket,
            RemoteWsResponse::Error(CoreLinkError::new("PEER_LINK_ALREADY_ACTIVE", error)),
        )
        .await;
        return;
    }
    if send_ws_response(
        socket,
        RemoteWsResponse::PeerOpened(request.channelId.clone()),
    )
    .await
    .is_err()
    {
        connection.close("Peer WebSocket closed before opening".to_string());
        return;
    }
    loop {
        tokio::select! {
            frame = receiver.recv() => {
                let Some(frame) = frame else { return; };
                if send_ws_response(socket, RemoteWsResponse::PeerFrame(frame)).await.is_err() {
                    connection.close("Peer WebSocket send failed".to_string());
                    return;
                }
            }
            message = socket.recv() => {
                let Some(Ok(message)) = message else {
                    connection.close("Peer WebSocket closed".to_string());
                    return;
                };
                let Message::Binary(bytes) = message else { continue; };
                let incoming = match operit_link::decodeLink::<RemoteWsEnvelope>(&bytes) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = send_ws_response(socket, RemoteWsResponse::Error(CoreLinkError::new("BAD_REQUEST", error.to_string()))).await;
                        continue;
                    }
                };
                if verify_ws_envelope(state, &incoming).await.is_err() {
                    let _ = send_ws_response(socket, RemoteWsResponse::Error(CoreLinkError::new("UNAUTHORIZED", "invalid WebSocket session"))).await;
                    connection.close("Peer WebSocket authentication failed".to_string());
                    return;
                }
                let payload = match operit_link::decodeLink::<RemoteWsPayload>(&incoming.payloadBytes) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = send_ws_response(socket, RemoteWsResponse::Error(CoreLinkError::new("BAD_REQUEST", error.to_string()))).await;
                        continue;
                    }
                };
                match payload {
                    RemoteWsPayload::PeerFrame { channelId, frame } if channelId == request.channelId => {
                        if let Err(error) = receivePeerFrame(&state.deviceId, &verified.deviceId, frame).await {
                            let _ = send_ws_response(socket, RemoteWsResponse::Error(CoreLinkError::new("PEER_FRAME_REJECTED", error))).await;
                            connection.close("Peer frame rejected".to_string());
                            return;
                        }
                    }
                    RemoteWsPayload::PeerChannelClose(channelId) if channelId == request.channelId => {
                        connection.close("Peer WebSocket closed by owner".to_string());
                        let _ = send_ws_response(socket, RemoteWsResponse::PeerClosed(channelId)).await;
                        return;
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Sends server-originated Peer frames through a WebSocket writer queue.
#[cfg(not(target_arch = "wasm32"))]
struct WsPeerFrameSender {
    sender: StdMutex<Option<tokio::sync::mpsc::UnboundedSender<PeerFrame>>>,
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl PeerFrameSender for WsPeerFrameSender {
    /// Queues one ordered Peer frame for the active WebSocket.
    async fn send(&self, frame: PeerFrame) -> Result<(), String> {
        let sender = self
            .sender
            .lock()
            .map_err(|error| error.to_string())?
            .as_ref()
            .cloned()
            .ok_or_else(|| "Peer WebSocket sender is closed".to_string())?;
        sender
            .send(frame)
            .map_err(|_| "Peer WebSocket sender is closed".to_string())
    }

    /// Closes the server-to-client WebSocket frame queue.
    fn close(&self) {
        let _ = self
            .sender
            .lock()
            .expect("Peer WebSocket sender lock poisoned")
            .take();
    }
}

/// Decodes one signed websocket envelope and encodes its response.
#[cfg(not(target_arch = "wasm32"))]
async fn handle_ws_binary(state: &RemoteLinkState, bytes: &[u8]) -> Vec<u8> {
    let response = match operit_link::decodeLink::<RemoteWsEnvelope>(bytes) {
        Ok(envelope) => handle_ws_envelope(state, envelope).await,
        Err(error) => RemoteWsResponse::Error(CoreLinkError::new("BAD_REQUEST", error.to_string())),
    };
    operit_link::encodeLink(&response).expect("RemoteWsResponse must serialize")
}

/// Verifies the raw websocket payload bytes and dispatches the decoded payload.
#[cfg(not(target_arch = "wasm32"))]
async fn handle_ws_envelope(
    state: &RemoteLinkState,
    envelope: RemoteWsEnvelope,
) -> RemoteWsResponse {
    if envelope.protocolVersion != 4 {
        return RemoteWsResponse::Error(CoreLinkError::new(
            "LINK_VERSION_MISMATCH",
            "Link protocol version 4 is required",
        ));
    }
    let payload = match operit_link::decodeLink::<RemoteWsPayload>(&envelope.payloadBytes) {
        Ok(value) => value,
        Err(error) => {
            return RemoteWsResponse::Error(CoreLinkError::new("BAD_REQUEST", error.to_string()))
        }
    };
    let verified = match verify_session_parts(
        state,
        &envelope.sessionId,
        &envelope.deviceId,
        &envelope.signature,
        &envelope.payloadBytes,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return RemoteWsResponse::Error(error),
    };
    match payload {
        RemoteWsPayload::SessionInfo(request) => {
            let spaceStore = CoreSpaceStore::new(state.accessStore.storage.clone());
            let space = match spaceStore.initialize() {
                Ok(value) => value,
                Err(error) => return RemoteWsResponse::Error(CoreLinkError::internal(error)),
            };
            let deviceProfiles = match spaceStore.deviceProfilesForCurrentSpace() {
                Ok(value) => value,
                Err(error) => return RemoteWsResponse::Error(CoreLinkError::internal(error)),
            };
            let sessions = match state.accessStore.inboundSessions() { Ok(s) => s, Err(e) => return RemoteWsResponse::Error(CoreLinkError::internal(e)) };
            let Some(session) = sessions.get(&envelope.sessionId) else {
                return RemoteWsResponse::Error(remote_session_auth_error(
                    "invalid session",
                    "invalid_session",
                ));
            };
            RemoteWsResponse::SessionInfo(RemoteSessionInfoResponse {
                protocolVersion: 4,
                pairingServiceVersion: session.pairingServiceVersion,
                peerNodeId: state.deviceId.clone(),
                peerDeviceInfo: state.deviceInfo.clone(),
                clientDeviceId: session.deviceId.clone(),
                clientDeviceInfo: session.deviceInfo.clone(),
                transports: vec!["http".to_string(), "ws".to_string()],
                deviceSpace: space,
                deviceProfiles,
                nonce: request.nonce,
            })
        }
        _ => RemoteWsResponse::Error(CoreLinkError::new(
            "WS_STREAM_MODE_REMOVED",
            "Legacy remote Link streams are removed",
        )),
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn verify_session(
    state: &RemoteLinkState,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<VerifiedRemoteSession, Response> {
    if header_string(headers, "x-operit-link-version").as_deref() != Some("4") {
        return Err(encode_link_response(
            StatusCode::BAD_REQUEST,
            CoreLinkError::new(
                "LINK_VERSION_MISMATCH",
                "Link protocol version 4 is required",
            ),
        ));
    }
    let Some(sessionId) = header_string(headers, "x-operit-session") else {
        return Err(encode_link_response(
            StatusCode::UNAUTHORIZED,
            CoreLinkError::new("UNAUTHORIZED", "missing session"),
        ));
    };
    let Some(deviceId) = header_string(headers, "x-operit-device") else {
        return Err(encode_link_response(
            StatusCode::UNAUTHORIZED,
            CoreLinkError::new("UNAUTHORIZED", "missing device"),
        ));
    };
    let Some(signature) = header_string(headers, "x-operit-signature") else {
        return Err(encode_link_response(
            StatusCode::UNAUTHORIZED,
            CoreLinkError::new("UNAUTHORIZED", "missing signature"),
        ));
    };
    verify_session_parts(state, &sessionId, &deviceId, &signature, body)
        .await
        .map_err(|error| encode_link_response(StatusCode::UNAUTHORIZED, error))
}

#[cfg(not(target_arch = "wasm32"))]
async fn verify_session_parts(
    state: &RemoteLinkState,
    sessionId: &str,
    deviceId: &str,
    signature: &str,
    body: &[u8],
) -> Result<VerifiedRemoteSession, CoreLinkError> {
    let records = state
        .accessStore
        .inboundSessions()
        .map_err(CoreLinkError::internal)?;
    let Some(record) = records.get(sessionId) else {
        return Err(remote_session_auth_error(
            "invalid session",
            "invalid_session",
        ));
    };
    let session = accepted_session_from_record(record)?;
    if session.deviceId != deviceId {
        return Err(remote_session_auth_error(
            "device mismatch",
            "device_mismatch",
        ));
    }
    if !verifySessionSignature(&session.sessionSecret, body, &signature) {
        return Err(remote_session_auth_error(
            "signature mismatch",
            "signature_mismatch",
        ));
    }
    if NetworkControlStore::new(state.accessStore.storage.clone())
        .and_then(|store| store.nodeIsDisconnected(deviceId))
        .map_err(CoreLinkError::internal)?
    {
        return Err(remote_session_auth_error(
            "device is prohibited from this Space connection",
            "disconnected_node",
        ));
    }
    Ok(VerifiedRemoteSession {
        sessionId: sessionId.to_string(),
        deviceId: deviceId.to_string(),
    })
}

/// Creates a structured unauthorized error for a remote session auth failure.
#[cfg(not(target_arch = "wasm32"))]
fn remote_session_auth_error(message: &'static str, auth_reason: &'static str) -> CoreLinkError {
    CoreLinkError::withDetails(
        "UNAUTHORIZED",
        message,
        operit_link::CoreValue::Map(BTreeMap::from([
            (
                "type".to_string(),
                operit_link::CoreValue::String("remote_session_auth".to_string()),
            ),
            (
                "authReason".to_string(),
                operit_link::CoreValue::String(auth_reason.to_string()),
            ),
            (
                "resetWebAccessSession".to_string(),
                operit_link::CoreValue::Bool(true),
            ),
        ])),
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn accepted_session_from_record(
    record: &AcceptedRemoteSessionRecord,
) -> Result<RemoteSession, CoreLinkError> {
    Ok(RemoteSession {
        deviceId: record.deviceId.clone(),
        deviceInfo: record.deviceInfo.clone(),
        pairingServiceVersion: record.pairingServiceVersion,
        sessionSecret: BASE64
            .decode(record.sessionSecret.as_bytes())
            .map_err(|error| CoreLinkError::new("INVALID_SESSION_STORE", error.to_string()))?,
    })
}

#[cfg(not(target_arch = "wasm32"))]
/// Stops the local Link Access server after validating its shutdown token.
async fn web_access_close(State(state): State<RemoteLinkState>, headers: HeaderMap) -> Response {
    let Some(webAccess) = state.webAccess.as_ref() else {
        return bad_request("web access is not enabled");
    };
    let token = header_string(&headers, "x-operit-web-access-shutdown-token");
    if token.as_deref() != Some(webAccess.shutdownToken.as_str()) {
        return unauthorized("invalid web access shutdown token");
    }
    let sender = webAccess
        .shutdownSender
        .lock()
        .expect("web access shutdown mutex poisoned")
        .take();
    let Some(sender) = sender else {
        return bad_request("web access close already requested");
    };
    if sender.send(()).is_err() {
        return bad_request("web access shutdown receiver is closed");
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

#[cfg(not(target_arch = "wasm32"))]
fn token_matches(state: &RemoteLinkState, headers: &HeaderMap) -> bool {
    header_string(headers, "x-operit-link-token-hash")
        .map(|value| token_hash_matches(state, &value))
        .unwrap_or(false)
}



#[cfg(not(target_arch = "wasm32"))]
fn token_hash_matches(state: &RemoteLinkState, tokenHash: &str) -> bool {
    tokenHash == linkTokenHash(&state.token)
}

#[cfg(not(target_arch = "wasm32"))]
fn unauthorized(message: impl Into<String>) -> Response {
    (StatusCode::UNAUTHORIZED, Json(CoreLinkError::new("UNAUTHORIZED", message.into()))).into_response()
}

#[cfg(not(target_arch = "wasm32"))]
fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
}




fn link_access_token() -> String {
    let mut bytes = [0u8; 18];
    OsRng.fill_bytes(&mut bytes);
    format!("ow-{}", URL_SAFE_NO_PAD.encode(bytes))
}

/// Returns the host-owned Unix clock used by Link Access records.
fn unix_millis() -> i64 {
    currentTimeMillis()
}





#[cfg(not(target_arch = "wasm32"))]
fn bad_request(message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(CoreLinkError::new("BAD_REQUEST", message.into())),
    )
        .into_response()
}
#[cfg(not(target_arch = "wasm32"))]
fn internal_server_error(message: impl Into<String>) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(CoreLinkError::new("INTERNAL_SERVER_ERROR", message.into())),
    )
        .into_response()
}

/// Encodes a typed Link response as MessagePack bytes.
#[cfg(not(target_arch = "wasm32"))]
fn encode_link_response(status: StatusCode, value: impl Serialize) -> Response {
    match operit_link::encodeLink(value) {
        Ok(bytes) => Response::builder()
            .status(status)
            .header("content-type", "application/msgpack")
            .body(Body::from(bytes))
            .expect("Link response must build"),
        Err(error) => internal_server_error(error.to_string()),
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn pairingAuthority(token: &str, deviceId: &str, info: LinkDeviceInfo, store: LinkAccessStore) -> Arc<operit_peer_link::PairingAuthority> {
    Arc::new(operit_peer_link::PairingAuthority::new(token, deviceId, info, |_| {}).withSessionCommit(move |session| {
        store.saveInboundSession(session.sessionId.clone(), AcceptedRemoteSessionRecord {
            deviceId: session.peerDeviceId.clone(), deviceInfo: session.peerDeviceInfo.clone(),
            pairingServiceVersion: session.pairingServiceVersion, sessionSecret: BASE64.encode(&session.sessionSecret),
        })
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn startPairingHttp(authority: &operit_peer_link::PairingAuthority, store: &LinkAccessStore, request: LinkPairStartRequest) -> Response {
    let auto = request.autoBootstrap;
    let (response, prompt) = match authority.startPairing(request) { Ok(value) => value, Err(e) => return bad_request(e) };
    let record = RemotePairingCodeRecord { pairingId: prompt.pairingId, pairingServiceVersion: response.pairingServiceVersion,
        clientDeviceId: prompt.clientDeviceId, clientDeviceInfo: prompt.clientDeviceInfo,
        pairingCode: prompt.pairingCode, createdAt: unix_millis() };
    if let Err(e) = store.savePendingPairing(record.clone()) { return internal_server_error(e); }
    if !auto {
        eprintln!("operit node pairing code for {}: {}", record.clientDeviceId, record.pairingCode);
        publishOwnerWebAccessPairing(RuntimeHostInteractionWebAccessPairingPayload {
            pairingId: record.pairingId, clientDeviceId: record.clientDeviceId,
            clientPlatform: record.clientDeviceInfo.platform, clientModel: record.clientDeviceInfo.model,
            pairingCode: record.pairingCode, createdAt: record.createdAt,
        });
    }
    Json(response).into_response()
}

#[cfg(not(target_arch = "wasm32"))]
fn finishPairingHttp(authority: &operit_peer_link::PairingAuthority, store: &LinkAccessStore, request: LinkPairFinishRequest) -> Response {
    let (session, proof) = match authority.acceptPairFinish(request) { Ok(value) => value, Err(e) => return unauthorized(e) };
    if let Err(e) = store.removePendingPairing(&session.sessionId) { return internal_server_error(e); }
    Json(LinkPairFinishResponse { sessionId: session.sessionId, pairingServiceVersion: session.pairingServiceVersion, coreProof: proof }).into_response()
}
