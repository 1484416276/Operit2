//! 节点通信持久化。沿用原 link_access 路径和 Preferences 格式，不恢复旧握手或 HTTP 接口。
use crate::NodeServices::PairedPeer;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use operit_host_api::RuntimeStorageHost;
use operit_link::protocol::LinkDeviceInfo;
use operit_store::PreferencesDataStore::{stringPreferencesKey, CoreNodeStateStore, Preferences};
use operit_util::RuntimeStorageLayout::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy)]
pub(crate) enum StoredDirection {
    Inbound,
    Outbound,
}

/// 原监听配置；token 只供本地 runtime 使用，不生成 UI DTO，也不实现 Debug。
#[derive(Clone, Serialize, Deserialize)]
pub struct PeerHostConfig {
    pub bindAddress: String,
    pub token: String,
    pub webAccessEnabled: bool,
    pub discoveryEnabled: bool,
    pub portMode: PeerHostPortMode,
    pub updatedAt: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerHostPortMode {
    #[serde(rename = "automatic")]
    Automatic,
    #[serde(rename = "fixed")]
    Fixed,
}

/// 原有凭证仅作为持久化数据读取；并不构成旧协议的实现。
#[derive(Deserialize)]
struct StoredInbound {
    deviceId: String,
    deviceInfo: LinkDeviceInfo,
    pairingServiceVersion: i32,
    sessionSecret: String,
}
#[derive(Deserialize)]
struct StoredOutbound {
    endpoint: String,
    sessionId: String,
    deviceId: String,
    peerNodeId: String,
    peerDeviceInfo: LinkDeviceInfo,
    pairingServiceVersion: i32,
    sessionSecret: String,
    transport: String,
}

#[derive(Clone)]
pub struct PeerStateStore {
    storage: Arc<dyn RuntimeStorageHost>,
}
impl PeerStateStore {
    pub fn new(storage: Arc<dyn RuntimeStorageHost>) -> Self {
        Self { storage }
    }
    fn store(&self, path: &str) -> CoreNodeStateStore {
        CoreNodeStateStore::newWithStorage(self.storage.clone(), path)
    }
    fn preferences(&self, path: &str) -> Result<Preferences, String> {
        self.store(path).data().map_err(|error| error.to_string())
    }
    fn records<T: DeserializeOwned>(&self, path: &str) -> Result<BTreeMap<String, T>, String> {
        self.preferences(path)?
            .entries()
            .into_iter()
            .map(|(name, encoded)| {
                let record = serde_json::from_str(&encoded)
                    .map_err(|_| format!("Invalid peer state record at {path}, key {name}"))?;
                Ok((name, record))
            })
            .collect()
    }

    /// 在原 identity.preferences.json 上读取/更新展示资料；稳定节点 ID 和未知字段不变。
    /// replace=false 保留原配置中的设备名；显式改名才使用 replace=true。
    pub fn deviceInfo(&self, supplied: LinkDeviceInfo, replace: bool) -> Result<LinkDeviceInfo, String> {
        use operit_store::CoreNodeIdentityStore::CoreNodeIdentityStore;
        use operit_store::PreferencesDataStore::PreferencesDataStoreError;
        let identity = CoreNodeIdentityStore::new(self.storage.clone()).initialize()?;
        self.store(RUNTIME_LINK_ACCESS_IDENTITY_PATH).try_edit_result(|preferences| {
            let key = stringPreferencesKey("record");
            let encoded = preferences.get(&key).ok_or_else(||
                PreferencesDataStoreError::Message("Node identity is missing".into()))?;
            let mut record: Value = serde_json::from_str(encoded)?;
            if record.get("deviceId").and_then(Value::as_str) != Some(identity.nodeId.as_str()) {
                return Err(PreferencesDataStoreError::Message("Node identity changed".into()));
            }
            let persisted = record.get("deviceInfo").cloned()
                .map(serde_json::from_value::<LinkDeviceInfo>).transpose()?;
            if !replace {
                if let Some(info) = persisted { return Ok(info); }
            }
            let info = record.as_object_mut().ok_or_else(||
                PreferencesDataStoreError::Message("Node identity is not an object".into()))?
                .entry("deviceInfo").or_insert_with(|| serde_json::json!({}));
            let object = info.as_object_mut().ok_or_else(||
                PreferencesDataStoreError::Message("Device info is not an object".into()))?;
            object.insert("platform".into(), supplied.platform.clone().into());
            object.insert("model".into(), supplied.model.clone().into());
            preferences.set(&key, serde_json::to_string(&record)?);
            Ok::<_, PreferencesDataStoreError>(supplied)
        }).map_err(|error| error.to_string())
    }

    /// 不创建第二份配置；文件缺失返回 None，已存在但损坏则报错，不能用默认值覆盖。
    pub fn hostConfig(&self) -> Result<Option<PeerHostConfig>, String> {
        let preferences = self.preferences(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH)?;
        if preferences.entries().is_empty() {
            return Ok(None);
        }
        decodeHostConfig(&preferences).map(Some)
    }
    /// 仅供本机管理界面主动显示/复制；缺失时不临时生成一个监听器尚未采用的 token。
    pub fn localPairingToken(&self) -> Result<String, String> {
        let config = self.hostConfig()?.ok_or("Node listener is not configured")?;
        if config.token.trim().is_empty() {
            return Err("Node pairing token is not configured".into());
        }
        Ok(config.token)
    }

    /// 更新同一份配置，并保留文件中未来或外围组件增加的字段。
    pub fn saveHostConfig(&self, config: &PeerHostConfig) -> Result<(), String> {
        self.store(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH)
            .edit(|preferences| {
                for (key, value) in [
                    ("bindAddress", config.bindAddress.clone()),
                    ("token", config.token.clone()),
                    ("webAccessEnabled", config.webAccessEnabled.to_string()),
                    ("discoveryEnabled", config.discoveryEnabled.to_string()),
                    (
                        "portMode",
                        match config.portMode {
                            PeerHostPortMode::Automatic => "automatic",
                            PeerHostPortMode::Fixed => "fixed",
                        }
                        .into(),
                    ),
                    ("updatedAt", config.updatedAt.to_string()),
                ] {
                    preferences.set(&stringPreferencesKey(key), value);
                }
            })
            .map_err(|error| error.to_string())
    }

    /// 从原入站/出站文件分别读取授权，只在 UI 投影中按节点归并，不合并凭证或反向授权。
    pub fn pairedPeers(&self, localNodeId: &str) -> Result<Vec<PairedPeer>, String> {
        let mut peers = BTreeMap::<String, PairedPeer>::new();
        for (_, record) in
            self.records::<StoredInbound>(RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH)?
        {
            validateCredential(record.pairingServiceVersion, &record.sessionSecret)?;
            mergePeer(
                &mut peers,
                &record.deviceId,
                &record.deviceInfo,
                StoredDirection::Inbound,
            )?;
        }
        for (_, record) in
            self.records::<StoredOutbound>(RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH)?
        {
            validateCredential(record.pairingServiceVersion, &record.sessionSecret)?;
            if record.deviceId != localNodeId
                || record.endpoint.is_empty()
                || record.sessionId.is_empty()
                || !matches!(
                    record.transport.as_str(),
                    "http" | "ws" | "tcp" | "serial" | "bluetooth"
                )
            {
                return Err("Invalid outbound peer identity or transport".into());
            }
            mergePeer(
                &mut peers,
                &record.peerNodeId,
                &record.peerDeviceInfo,
                StoredDirection::Outbound,
            )?;
        }
        Ok(peers.into_values().collect())
    }

    /// 设备级撤销的持久化部分：清除两个方向、全部渠道和该设备的待确认事务。
    /// 调用方 runtime 必须串行化配对/撤销并失效内存凭证、关闭连接；跨文件写失败必须报错，
    /// 不能将这里当成跨文件原子事务或在失败后继续授予旧连接访问权。重复调用可安全重试。
    pub fn removePairedPeer(&self, nodeId: &str) -> Result<(), String> {
        if nodeId.trim().is_empty() {
            return Err("Invalid paired node id".into());
        }
        let paths = [
            (RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH, "/deviceId"),
            (RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH, "/peerNodeId"),
            (RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH, "/clientDeviceId"),
            (
                RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH,
                "/state/peerNodeId",
            ),
        ];
        // 先验证四份原记录，避免发现损坏数据前就删除其中一个方向。
        let records = paths
            .iter()
            .map(|(path, pointer)| {
                let keys = self
                    .records::<Value>(path)?
                    .into_iter()
                    .filter_map(|(key, record)| {
                        (record.pointer(pointer).and_then(Value::as_str) == Some(nodeId))
                            .then_some(key)
                    })
                    .collect::<Vec<_>>();
                Ok((*path, keys))
            })
            .collect::<Result<Vec<_>, String>>()?;
        for (path, keys) in records {
            if keys.is_empty() {
                continue;
            }
            self.store(path)
                .edit(|preferences| {
                    for key in &keys {
                        preferences.remove(&stringPreferencesKey(key));
                    }
                })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// 原待确认记录保留读取，但旧事务不能绕过新协议 token/配对码校验而直接完成。
    /// 不向 UI 返回这些原始数据：旧出站状态可能包含临时密钥。
    pub(crate) fn pendingRecords(
        &self,
        direction: StoredDirection,
    ) -> Result<BTreeMap<String, Value>, String> {
        self.records(match direction {
            StoredDirection::Inbound => RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH,
            StoredDirection::Outbound => RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH,
        })
    }
}
fn decodeHostConfig(preferences: &Preferences) -> Result<PeerHostConfig, String> {
    let get = |key| {
        preferences
            .get(&stringPreferencesKey(key))
            .cloned()
            .ok_or_else(|| format!("Peer host config is missing {key}"))
    };
    let parseBool = |key| {
        get(key)?
            .parse::<bool>()
            .map_err(|_| format!("Invalid peer host config field {key}"))
    };
    Ok(PeerHostConfig {
        bindAddress: get("bindAddress")?,
        token: get("token")?,
        webAccessEnabled: parseBool("webAccessEnabled")?,
        discoveryEnabled: parseBool("discoveryEnabled")?,
        portMode: match get("portMode")?.as_str() {
            "automatic" => PeerHostPortMode::Automatic,
            "fixed" => PeerHostPortMode::Fixed,
            _ => return Err("Invalid peer host config field portMode".into()),
        },
        updatedAt: get("updatedAt")?
            .parse()
            .map_err(|_| "Invalid peer host config field updatedAt".to_string())?,
    })
}
fn validateCredential(version: i32, secret: &str) -> Result<(), String> {
    if version <= 0
        || BASE64
            .decode(secret)
            .map_err(|_| "Invalid peer credential encoding")?
            .len()
            != 32
    {
        return Err("Invalid peer credential version or length".into());
    }
    Ok(())
}
fn mergePeer(
    peers: &mut BTreeMap<String, PairedPeer>,
    nodeId: &str,
    info: &LinkDeviceInfo,
    direction: StoredDirection,
) -> Result<(), String> {
    if nodeId.trim().is_empty() {
        return Err("Invalid paired node id".into());
    }
    let peer = peers.entry(nodeId.into()).or_insert_with(|| PairedPeer {
        nodeId: nodeId.into(),
        displayName: info.displayName(),
        inbound: false,
        outbound: false,
    });
    if peer.displayName != info.displayName() {
        return Err("Conflicting paired device metadata".into());
    }
    match direction {
        StoredDirection::Inbound => peer.inbound = true,
        StoredDirection::Outbound => peer.outbound = true,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use operit_host_api::{HostError, HostResult, RuntimeStorageEntry};
    use operit_store::PreferencesDataStore::emptyPreferences;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Storage(Mutex<BTreeMap<String, Vec<u8>>>);
    impl RuntimeStorageHost for Storage {
        fn runtimeRootDir(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn workspaceRootDir(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn readBytes(&self, path: &str) -> HostResult<Vec<u8>> {
            self.0
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| HostError::new("missing"))
        }
        fn writeBytes(&self, path: &str, bytes: &[u8]) -> HostResult<()> {
            self.0.lock().unwrap().insert(path.into(), bytes.into());
            Ok(())
        }
        fn appendBytes(&self, _: &str, _: &[u8]) -> HostResult<()> {
            Err(HostError::new("unused"))
        }
        fn delete(&self, path: &str, _: bool) -> HostResult<()> {
            self.0.lock().unwrap().remove(path);
            Ok(())
        }
        fn exists(&self, path: &str) -> HostResult<bool> {
            Ok(self.0.lock().unwrap().contains_key(path))
        }
        fn list(&self, _: &str) -> HostResult<Vec<RuntimeStorageEntry>> {
            Ok(vec![])
        }
    }
    fn writeRecord(store: &PeerStateStore, path: &str, key: &str, record: Value) {
        let mut preferences = emptyPreferences();
        preferences.set(&stringPreferencesKey(key), record.to_string());
        store.store(path).replace(preferences).unwrap();
    }
    #[test]
    fn original_identity_and_unknown_fields_survive_device_info_updates() {
        let store = PeerStateStore::new(Arc::new(Storage::default()));
        writeRecord(&store, RUNTIME_LINK_ACCESS_IDENTITY_PATH, "record", serde_json::json!({
            "deviceId": "stable-node", "future": "preserved",
            "deviceInfo": { "platform": "old", "model": "old-name", "extra": 7 }
        }));
        let supplied = LinkDeviceInfo { platform: "new".into(), model: "new-name".into() };
        assert_eq!(store.deviceInfo(supplied.clone(), false).unwrap().model, "old-name");
        assert_eq!(store.deviceInfo(supplied, true).unwrap().model, "new-name");
        let records = store.records::<Value>(RUNTIME_LINK_ACCESS_IDENTITY_PATH).unwrap();
        assert_eq!(records["record"]["deviceId"], "stable-node");
        assert_eq!(records["record"]["future"], "preserved");
        assert_eq!(records["record"]["deviceInfo"]["extra"], 7);
        writeRecord(&store, RUNTIME_LINK_ACCESS_IDENTITY_PATH, "record", serde_json::json!({
            "deviceId": "stable-node", "deviceInfo": "broken"
        }));
        let before = store.preferences(RUNTIME_LINK_ACCESS_IDENTITY_PATH).unwrap().entries();
        assert!(store.deviceInfo(LinkDeviceInfo { platform: "new".into(), model: "name".into() }, true).is_err());
        assert_eq!(before, store.preferences(RUNTIME_LINK_ACCESS_IDENTITY_PATH).unwrap().entries());
    }

    #[test]
    fn local_token_is_explicit_and_does_not_create_or_rewrite_configuration() {
        let store = PeerStateStore::new(Arc::new(Storage::default()));
        assert!(store.localPairingToken().is_err());
        assert!(store.hostConfig().unwrap().is_none());
        let mut config = PeerHostConfig {
            bindAddress: "0.0.0.0:37194".into(), token: "saved-token".into(),
            webAccessEnabled: false, discoveryEnabled: false,
            portMode: PeerHostPortMode::Fixed, updatedAt: 12,
        };
        store.saveHostConfig(&config).unwrap();
        let before = store.preferences(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH).unwrap().entries();
        assert_eq!(store.localPairingToken().unwrap(), "saved-token");
        assert_eq!(before, store.preferences(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH).unwrap().entries());
        config.token = " ".into();
        store.saveHostConfig(&config).unwrap();
        assert!(store.localPairingToken().is_err());
    }

    #[test]
    fn original_host_config_is_read_and_updated_in_place_without_losing_fields() {
        let storage = Arc::new(Storage::default());
        let store = PeerStateStore::new(storage.clone());
        assert!(store.hostConfig().unwrap().is_none());
        let mut preferences = emptyPreferences();
        for (key, value) in [
            ("bindAddress", "127.0.0.1:37194"),
            ("token", "old-token"),
            ("webAccessEnabled", "true"),
            ("discoveryEnabled", "false"),
            ("portMode", "fixed"),
            ("updatedAt", "123"),
            ("extra", "keep"),
        ] {
            preferences.set(&stringPreferencesKey(key), value.into());
        }
        store
            .store(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH)
            .replace(preferences)
            .unwrap();
        let before = storage.0.lock().unwrap().clone();
        let mut config = store.hostConfig().unwrap().unwrap();
        assert_eq!(config.token, "old-token");
        assert_eq!(config.portMode, PeerHostPortMode::Fixed);
        assert_eq!(
            storage.0.lock().unwrap().clone(),
            before,
            "reading must not rewrite original files"
        );
        config.discoveryEnabled = true;
        store.saveHostConfig(&config).unwrap();
        assert_eq!(
            store
                .preferences(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH)
                .unwrap()
                .get(&stringPreferencesKey("extra"))
                .unwrap(),
            "keep"
        );
    }
    #[test]
    fn original_directional_records_do_not_create_reverse_authorization() {
        let store = PeerStateStore::new(Arc::new(Storage::default()));
        writeRecord(
            &store,
            RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH,
            "session",
            serde_json::json!({
                "deviceId":"incoming", "deviceInfo":{"platform":"test","model":"one"}, "pairingServiceVersion":1, "sessionSecret":BASE64.encode([1;32])
            }),
        );
        writeRecord(
            &store,
            RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH,
            "saved-name",
            serde_json::json!({
                "endpoint":"tcp-address", "sessionId":"other-session", "deviceId":"local", "peerNodeId":"outgoing", "peerDeviceInfo":{"platform":"test","model":"two"}, "pairingServiceVersion":1, "sessionSecret":BASE64.encode([2;32]), "transport":"tcp"
            }),
        );
        let peers = store.pairedPeers("local").unwrap();
        assert!(peers[0].inbound && !peers[0].outbound);
        assert!(!peers[1].inbound && peers[1].outbound);
        assert!(store.pairedPeers("wrong-local").is_err());
    }
    #[test]
    fn device_revocation_clears_both_directions_all_channels_and_pending_transactions() {
        let storage = Arc::new(Storage::default());
        let store = PeerStateStore::new(storage.clone());
        for (path, record) in [
            (
                RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH,
                serde_json::json!({"deviceId":"board"}),
            ),
            (
                RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH,
                serde_json::json!({"peerNodeId":"board"}),
            ),
            (
                RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH,
                serde_json::json!({"clientDeviceId":"board"}),
            ),
            (
                RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH,
                serde_json::json!({"state":{"peerNodeId":"board"}}),
            ),
        ] {
            let other = serde_json::json!({"deviceId":"other", "peerNodeId":"other", "clientDeviceId":"other", "state":{"peerNodeId":"other"}});
            store
                .store(path)
                .edit(|preferences| {
                    preferences.set(&stringPreferencesKey("channel-a"), record.to_string());
                    preferences.set(&stringPreferencesKey("channel-b"), record.to_string());
                    preferences.set(&stringPreferencesKey("other"), other.to_string());
                })
                .unwrap();
        }
        writeRecord(
            &store,
            RUNTIME_LINK_ACCESS_IDENTITY_PATH,
            "record",
            serde_json::json!({"deviceId":"local"}),
        );
        let identity = storage
            .readBytes(RUNTIME_LINK_ACCESS_IDENTITY_PATH)
            .unwrap();
        store.removePairedPeer("board").unwrap();
        store.removePairedPeer("board").unwrap();
        for path in [
            RUNTIME_LINK_ACCESS_INBOUND_SESSIONS_PATH,
            RUNTIME_LINK_ACCESS_OUTBOUND_SESSIONS_PATH,
            RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH,
            RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH,
        ] {
            assert_eq!(
                store
                    .records::<Value>(path)
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                ["other"]
            );
        }
        assert_eq!(
            storage
                .readBytes(RUNTIME_LINK_ACCESS_IDENTITY_PATH)
                .unwrap(),
            identity
        );
    }

    #[test]
    fn malformed_config_is_not_replaced_and_pending_records_are_retained() {
        let storage = Arc::new(Storage::default());
        let store = PeerStateStore::new(storage.clone());
        let mut p = emptyPreferences();
        p.set(&stringPreferencesKey("token"), "secret".into());
        store
            .store(RUNTIME_LINK_ACCESS_HOST_CONFIG_PATH)
            .replace(p)
            .unwrap();
        for (direction, path) in [
            (
                StoredDirection::Inbound,
                RUNTIME_LINK_ACCESS_PENDING_PAIRINGS_PATH,
            ),
            (
                StoredDirection::Outbound,
                RUNTIME_LINK_ACCESS_PENDING_OUTBOUND_PAIRINGS_PATH,
            ),
        ] {
            writeRecord(
                &store,
                path,
                "pending",
                serde_json::json!({"old-state":"retained"}),
            );
            assert_eq!(
                store.pendingRecords(direction).unwrap()["pending"]["old-state"],
                "retained"
            );
        }
        let before = storage.0.lock().unwrap().clone();
        assert!(store.hostConfig().is_err());
        assert_eq!(*storage.0.lock().unwrap(), before);
    }
}
