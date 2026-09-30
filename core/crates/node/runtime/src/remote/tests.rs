use super::*;

use operit_host_api::{HostError, RuntimeStorageEntry};
use operit_store::SyncOperationStore::SyncOperationStore;
use operit_util::RuntimeStorageLayout::{RUNTIME_SPACE_TOPOLOGY_DIR_PATH, RUNTIME_SYNC_DIR_PATH};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[test]
fn pairing_replacement_is_atomic_and_late_cancellation_cannot_remove_it() {
    let store = LinkAccessStore::new(Arc::new(MemoryStorageHost::default()));
    let mut record = PairedPeerSessionRecord {
        endpoint: "127.0.0.1:8765".into(),
        transport: PeerTransport::Tcp,
        sessionId: "first".into(),
        deviceId: "core".into(),
        peerNodeId: "edge".into(),
        peerDeviceInfo: LinkDeviceInfo {
            platform: "esp32".into(),
            model: "test".into(),
        },
        pairingServiceVersion: 1,
        sessionSecret: "secret".into(),
    };
    store
        .saveOutboundSession("old-name".into(), record.clone())
        .unwrap();
    record.sessionId = "replacement".into();
    store
        .saveOutboundSession("new-name".into(), record.clone())
        .unwrap();
    store.removeOutboundSessionCredentials("edge", "first").unwrap();
    assert_eq!(
        store.outboundSessions().unwrap(),
        BTreeMap::from([("new-name".into(), record)])
    );
    store
        .removeOutboundSessionCredentials("edge", "replacement")
        .unwrap();
    assert!(store.outboundSessions().unwrap().is_empty());
}

#[derive(Clone, Default)]
struct MemoryStorageHost {
    files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
}

impl RuntimeStorageHost for MemoryStorageHost {
    /// Does not expose a physical runtime root for in-memory test storage.
    fn runtimeRootDir(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// Does not expose a physical workspace root for in-memory test storage.
    fn workspaceRootDir(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// Reads one in-memory runtime storage entry.
    fn readBytes(&self, path: &str) -> operit_host_api::HostResult<Vec<u8>> {
        let files = self
            .files
            .lock()
            .map_err(|error| HostError::new(error.to_string()))?;
        files
            .get(path)
            .cloned()
            .ok_or_else(|| HostError::new(format!("missing runtime storage entry: {path}")))
    }

    /// Writes one in-memory runtime storage entry.
    fn writeBytes(&self, path: &str, content: &[u8]) -> operit_host_api::HostResult<()> {
        self.files
            .lock()
            .map_err(|error| HostError::new(error.to_string()))?
            .insert(path.to_string(), content.to_vec());
        Ok(())
    }

    /// Appends bytes to one in-memory runtime storage entry.
    fn appendBytes(&self, path: &str, content: &[u8]) -> operit_host_api::HostResult<()> {
        self.files
            .lock()
            .map_err(|error| HostError::new(error.to_string()))?
            .entry(path.to_string())
            .or_default()
            .extend_from_slice(content);
        Ok(())
    }

    /// Removes one in-memory runtime storage entry.
    fn delete(&self, path: &str, _recursive: bool) -> operit_host_api::HostResult<()> {
        self.files
            .lock()
            .map_err(|error| HostError::new(error.to_string()))?
            .remove(path);
        Ok(())
    }

    /// Checks whether one in-memory runtime storage entry exists.
    fn exists(&self, path: &str) -> operit_host_api::HostResult<bool> {
        Ok(self
            .files
            .lock()
            .map_err(|error| HostError::new(error.to_string()))?
            .contains_key(path))
    }

    /// Lists in-memory runtime storage entries with the requested prefix.
    fn list(&self, prefix: &str) -> operit_host_api::HostResult<Vec<RuntimeStorageEntry>> {
        Ok(self
            .files
            .lock()
            .map_err(|error| HostError::new(error.to_string()))?
            .iter()
            .filter(|(path, _)| path.starts_with(prefix))
            .map(|(path, content)| RuntimeStorageEntry {
                path: path.clone(),
                isDirectory: false,
                size: content.len() as i64,
            })
            .collect())
    }
}

/// Verifies Link Access storage no longer creates a global execution route.
#[test]
fn link_access_store_constructs_without_global_route() {
    let _store = LinkAccessStore::new(Arc::new(MemoryStorageHost::default()));
}

/// Verifies Space adoption carries device profiles for every joined member.
#[test]
fn space_adopt_envelope_carries_joined_device_profiles() {
    let storage = Arc::new(MemoryStorageHost::default());
    CoreNodeIdentityStore::new(storage.clone())
        .writeNodeId("node-a".to_string())
        .expect("local CoreNode identity must be written");
    let spaceStore = CoreSpaceStore::new(storage.clone());
    spaceStore
        .writeLocalDeviceProfile(
            "Local".to_string(),
            "test".to_string(),
            "local".to_string(),
            "test-core".to_string(),
        )
        .expect("local device profile must be written");
    let joinedSpace = spaceStore
        .merge(CoreSpace {
            spaceId: "space-peer".to_string(),
            spaceName: "Peer Space".to_string(),
            spaceRevision: 2,
            members: vec!["node-b".to_string()],
        })
        .expect("joined Space must merge");
    let peerProfile = CoreSpaceDeviceProfile {
        nodeId: "node-b".to_string(),
        displayName: "Peer".to_string(),
        userName: "Peer User".to_string(),
        platform: "test".to_string(),
        model: "peer".to_string(),
        coreVersion: Some("test-core".to_string()),
        updatedAt: 1,
    };
    let envelope = RemoteSpaceAdoptEnvelope {
        space: joinedSpace,
        deviceProfiles: vec![peerProfile.clone()],
    };
    let decoded: RemoteSpaceAdoptEnvelope =
        operit_link::decodeLink(&operit_link::encodeLink(&envelope).unwrap()).unwrap();
    spaceStore
        .importDeviceProfiles(decoded.deviceProfiles)
        .expect("joined device profiles must import");
    let profiles = spaceStore
        .deviceProfilesForCurrentSpace()
        .expect("current Space profiles must be complete");

    assert!(profiles.iter().any(|profile| profile.nodeId == "node-a"));
    assert!(profiles.iter().any(|profile| profile == &peerProfile));
}

/// Verifies a newly joined device receives the admission command that grants chat.read.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn space_join_returns_the_admission_that_grants_chat_read() {
    let storage = Arc::new(MemoryStorageHost::default());
    CoreNodeIdentityStore::new(storage.clone())
        .writeNodeId("node-a".to_string())
        .expect("server CoreNode identity must be written");
    let spaceStore = CoreSpaceStore::new(storage.clone());
    spaceStore
        .writeLocalDeviceProfile(
            "Local".to_string(),
            "test".to_string(),
            "local".to_string(),
            "test-core".to_string(),
        )
        .expect("server device profile must be written");
    NetworkControlStore::new(storage.clone())
        .expect("server control store must initialize")
        .bootstrapCurrentSpace()
        .expect("server Space must bootstrap");
    let currentSpace = spaceStore
        .initialize()
        .expect("server Space must initialize");
    let proposal = CoreSpace {
        spaceId: currentSpace.spaceId.clone(),
        spaceName: currentSpace.spaceName.clone(),
        spaceRevision: currentSpace.spaceRevision + 1,
        members: vec!["node-a".to_string(), "node-b".to_string()],
    };

    let accepted = acceptAuthenticatedSpaceJoin(
        storage.clone(),
        "node-b",
        RemoteSpaceAdoptEnvelope {
            space: proposal,
            deviceProfiles: Vec::new(),
        },
    )
    .expect("authenticated join must be accepted");
    let controlOperations = accepted.controlOperations;

    let joinerStorage = Arc::new(MemoryStorageHost::default());
    CoreNodeIdentityStore::new(joinerStorage.clone())
        .writeNodeId("node-b".to_string())
        .expect("joiner CoreNode identity must be written");
    CoreSpaceStore::new(joinerStorage.clone())
        .adopt(accepted.space.clone())
        .expect("joiner must adopt the accepted Space");
    let joinerOperations = SyncOperationStore::new(joinerStorage.clone(), RUNTIME_SYNC_DIR_PATH);
    for operation in controlOperations {
        joinerOperations
            .appendOperation(&operation)
            .expect("joiner must persist the Space control command");
    }
    let joinerControl =
        NetworkControlStore::new(joinerStorage).expect("joiner control store must initialize");

    assert!(joinerControl
        .nodeHasCapability("node-b", "chat.read", None)
        .expect("joined capability query must succeed"));
}

/// Verifies inbound pair completion writes the paired endpoint profile.
#[test]
fn inbound_session_persists_paired_device_profile() {
    let storage = Arc::new(MemoryStorageHost::default());
    CoreNodeIdentityStore::new(storage.clone())
        .writeNodeId("node-a".to_string())
        .expect("local CoreNode identity must be written");
    let spaceStore = CoreSpaceStore::new(storage.clone());
    spaceStore
        .writeLocalDeviceProfile(
            "Local".to_string(),
            "test".to_string(),
            "local".to_string(),
            "test-core".to_string(),
        )
        .expect("local device profile must be written");
    let accessStore = LinkAccessStore::new(storage.clone());
    accessStore
        .saveInboundSession("inbound-1".to_string(), acceptedSession("node-b"))
        .expect("inbound session must persist");
    spaceStore
        .merge(CoreSpace {
            spaceId: "space-peer".to_string(),
            spaceName: "Peer Space".to_string(),
            spaceRevision: 2,
            members: vec!["node-b".to_string()],
        })
        .expect("joined Space must merge");

    let profiles = spaceStore
        .deviceProfilesForCurrentSpace()
        .expect("current Space profiles must be complete");

    assert!(profiles.iter().any(|profile| profile.nodeId == "node-a"));
    assert!(profiles.iter().any(|profile| profile.nodeId == "node-b"));
}

/// Verifies outbound pair completion writes the paired endpoint profile.
#[test]
fn outbound_session_persists_paired_device_profile() {
    let storage = Arc::new(MemoryStorageHost::default());
    CoreNodeIdentityStore::new(storage.clone())
        .writeNodeId("node-a".to_string())
        .expect("local CoreNode identity must be written");
    let spaceStore = CoreSpaceStore::new(storage.clone());
    spaceStore
        .writeLocalDeviceProfile(
            "Local".to_string(),
            "test".to_string(),
            "local".to_string(),
            "test-core".to_string(),
        )
        .expect("local device profile must be written");
    let accessStore = LinkAccessStore::new(storage.clone());
    accessStore
        .saveOutboundSession("outbound".to_string(), outboundSession("node-a", "node-b"))
        .expect("outbound session must persist");
    spaceStore
        .merge(CoreSpace {
            spaceId: "space-peer".to_string(),
            spaceName: "Peer Space".to_string(),
            spaceRevision: 2,
            members: vec!["node-b".to_string()],
        })
        .expect("joined Space must merge");

    let profiles = spaceStore
        .deviceProfilesForCurrentSpace()
        .expect("current Space profiles must be complete");

    assert!(profiles.iter().any(|profile| profile.nodeId == "node-a"));
    assert!(profiles.iter().any(|profile| profile.nodeId == "node-b"));
}

/// Verifies stored pairings can republish their device profiles.
#[test]
fn stored_pairings_republish_device_profiles() {
    let storage = Arc::new(MemoryStorageHost::default());
    CoreNodeIdentityStore::new(storage.clone())
        .writeNodeId("node-a".to_string())
        .expect("local CoreNode identity must be written");
    let spaceStore = CoreSpaceStore::new(storage.clone());
    spaceStore
        .writeLocalDeviceProfile(
            "Local".to_string(),
            "test".to_string(),
            "local".to_string(),
            "test-core".to_string(),
        )
        .expect("local device profile must be written");
    let accessStore = LinkAccessStore::new(storage.clone());
    accessStore
        .writeMapRecord(
            RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH,
            "inbound-1",
            &acceptedSession("node-b"),
        )
        .expect("existing inbound session fixture must be written");
    accessStore
        .syncPairedDeviceProfiles()
        .expect("stored pairing profiles must publish");
    spaceStore
        .merge(CoreSpace {
            spaceId: "space-peer".to_string(),
            spaceName: "Peer Space".to_string(),
            spaceRevision: 2,
            members: vec!["node-b".to_string()],
        })
        .expect("joined Space must merge");

    let profiles = spaceStore
        .deviceProfilesForCurrentSpace()
        .expect("current Space profiles must be complete");

    assert!(profiles.iter().any(|profile| profile.nodeId == "node-a"));
    assert!(profiles.iter().any(|profile| profile.nodeId == "node-b"));
}

/// Creates one accepted inbound session record for topology lifecycle tests.
#[allow(non_snake_case)]
fn acceptedSession(peerNodeId: &str) -> AcceptedRemoteSessionRecord {
    AcceptedRemoteSessionRecord {
        deviceId: peerNodeId.to_string(),
        deviceInfo: LinkDeviceInfo {
            platform: "test".to_string(),
            model: "peer".to_string(),
        },
        pairingServiceVersion: 1,
        sessionSecret: "inbound-secret".to_string(),
    }
}

/// Creates one outbound session record targeting the same direct CoreNode.
#[allow(non_snake_case)]
fn outboundSession(localNodeId: &str, peerNodeId: &str) -> PairedPeerSessionRecord {
    PairedPeerSessionRecord {
        endpoint: "http://peer.invalid".to_string(),
        sessionId: "outbound-session".to_string(),
        deviceId: localNodeId.to_string(),
        peerNodeId: peerNodeId.to_string(),
        peerDeviceInfo: LinkDeviceInfo {
            platform: "test".to_string(),
            model: "peer".to_string(),
        },
        pairingServiceVersion: 1,
        sessionSecret: "outbound-secret".to_string(),
        transport: PeerTransport::Http,
    }
}

/// Verifies pairing session persistence never reads the active Peer Link topology projection.
#[test]
fn session_persistence_does_not_read_space_topology() {
    let storage = Arc::new(MemoryStorageHost::default());
    CoreNodeIdentityStore::new(storage.clone())
        .writeNodeId("node-a".to_string())
        .expect("local CoreNode identity must be written");
    CoreSpaceStore::new(storage.clone())
        .initialize()
        .expect("local Space must initialize");
    storage
        .writeBytes(
            &format!("{RUNTIME_SPACE_TOPOLOGY_DIR_PATH}/node-a.preferences.json"),
            b"",
        )
        .expect("empty topology fixture must be written");
    let accessStore = LinkAccessStore::new(storage.clone());
    accessStore
        .saveInboundSession("inbound-1".to_string(), acceptedSession("node-b"))
        .expect("first inbound session must persist");
    accessStore
        .saveInboundSession("inbound-2".to_string(), acceptedSession("node-b"))
        .expect("second inbound session must persist");
    accessStore
        .saveOutboundSession("outbound".to_string(), outboundSession("node-a", "node-b"))
        .expect("outbound session must persist");
    accessStore
        .removeInboundSession("inbound-1")
        .expect("first inbound session must be removed");
    accessStore
        .removeInboundSession("inbound-2")
        .expect("second inbound session must be removed");
    accessStore
        .removeOutboundSession("outbound")
        .expect("outbound session must be removed");
    assert!(accessStore
        .inboundSessions()
        .expect("inbound sessions must read")
        .is_empty());
    assert!(accessStore
        .outboundSessions()
        .expect("outbound sessions must read")
        .is_empty());
}

#[test]
fn one_pending_store_records_the_selected_transport() {
    let storage = Arc::new(MemoryStorageHost::default());
    let store = LinkAccessStore::new(storage.clone());
    let device = LinkDeviceInfo { platform: "test".into(), model: "test".into() };
    for (index, (endpoint, transport)) in [("https://host", PeerTransport::Http), ("wss://host", PeerTransport::WebSocket),
        ("tcp://host:42", PeerTransport::Tcp), ("serial://COM1", PeerTransport::Serial)].into_iter().enumerate() {
        let id = format!("pair-{index}");
        store.savePendingOutboundPairing(id.clone(), PendingOutboundPairingRecord {
            endpoint: endpoint.into(), transport: transport.clone(), state: PairStartState {
                pairingId: id.clone(), pairingServiceVersion: 1, clientDeviceId: "local".into(),
                clientDeviceInfo: device.clone(), clientPublicKey: "key".into(), peerNodeId: "remote".into(),
                peerDeviceInfo: device.clone(), clientNonce: "client".into(), serverNonce: "server".into(), sharedSecret: vec![1; 32],
            },
        }).unwrap();
        let reopened = LinkAccessStore::new(storage.clone());
        assert_eq!(reopened.pendingOutboundPairings().unwrap()[&id].transport, transport);
        assert_eq!(PeerTransport::forEndpoint(endpoint).unwrap(), transport);
        reopened.removePendingOutboundPairing(&id).unwrap();
        assert!(reopened.pendingOutboundPairings().unwrap().is_empty());
    }
}
