use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use base64::{engine::general_purpose::STANDARD, Engine};
use operit_host_api::RuntimeStorageHost;
use operit_util::RuntimeStorageLayout::RUNTIME_SYNC_DIR_PATH;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::RuntimeFileSyncStore::RuntimeFileSyncStore;
use crate::RuntimeStorageHost::defaultRuntimeStorageHost;

static CATALOG_REVISION: OnceLock<crate::PreferencesDataStore::MutableStateFlow<i64>> =
    OnceLock::new();

/// Observes completed local and remote extension catalog mutations.
pub fn catalogRevisionFlow() -> crate::PreferencesDataStore::StateFlow<i64> {
    CATALOG_REVISION
        .get_or_init(|| crate::PreferencesDataStore::MutableStateFlow::new(0))
        .asStateFlow()
}

/// Publishes one completed extension mutation to subscribed package-manager pages.
pub fn notifyCatalogChanged() {
    let flow =
        CATALOG_REVISION.get_or_init(|| crate::PreferencesDataStore::MutableStateFlow::new(0));
    flow.set_value(flow.value() + 1);
}

pub const SPACE_EXTENSION_RECORDS: &str = "runtime/extensions/space/records";

/// Keeps one extension's scope, concrete source, configuration and portable source snapshot together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtensionRecord {
    pub kind: String,
    pub id: String,
    pub scope: String,
    pub sourceName: String,
    pub settings: Value,
    pub files: BTreeMap<String, String>,
}

/// Stores independent extension entities through the cross-platform runtime storage host.
#[derive(Clone)]
pub struct ExtensionStore {
    storage: Arc<dyn RuntimeStorageHost>,
}

impl ExtensionStore {
    /// Opens the registered runtime storage host, without platform-specific storage branches.
    pub fn default() -> Self {
        Self::new(defaultRuntimeStorageHost())
    }

    /// Opens an explicit host for isolated runtimes and tests.
    pub fn new(storage: Arc<dyn RuntimeStorageHost>) -> Self {
        Self { storage }
    }

    /// Resolves the single declared content directory for a validated scope and extension kind.
    pub fn root(kind: &str, scope: &str) -> Result<String, String> {
        let directory = match kind {
            "package" => "packages",
            "skill" => "skills",
            "mcp" => "mcp",
            _ => return Err(format!("Unknown extension kind: {kind}")),
        };
        validateScope(scope)?;
        Ok(format!("runtime/extensions/{scope}/{directory}"))
    }

    /// Lists every independently stored record, keeping conflicting identities visible to callers.
    pub fn records(&self, kind: &str) -> Result<Vec<ExtensionRecord>, String> {
        Self::root(kind, "device")?;
        let mut records = Vec::new();
        for scope in ["device", "space"] {
            for entry in self
                .storage
                .list(&format!("runtime/extensions/{scope}/records"))
                .map_err(|e| e.to_string())?
            {
                if entry.isDirectory {
                    return Err("An extension record cannot be a directory".to_string());
                }
                let record: ExtensionRecord = serde_json::from_slice(
                    &self
                        .storage
                        .readBytes(&entry.path)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                self.validate(&record)?;
                if recordPath(&record.kind, &record.id, &record.scope)? != entry.path {
                    return Err("Extension identity does not match its record path".to_string());
                }
                if record.kind == kind {
                    records.push(record);
                }
            }
        }
        Ok(records)
    }

    /// Reads an exact identity and rejects multiple installed owners rather than overriding either.
    pub fn record(&self, kind: &str, id: &str) -> Result<ExtensionRecord, String> {
        let mut records = self
            .records(kind)?
            .into_iter()
            .filter(|record| record.id == id)
            .collect::<Vec<_>>();
        match records.len() {
            1 => Ok(records.remove(0)),
            0 => Err(format!("Extension is not registered: {kind}:{id}")),
            _ => Err(format!(
                "Extension ownership conflict: {kind}:{id} exists in both locations"
            )),
        }
    }

    /// Registers newly imported device content or a built-in resource with an explicit initial state.
    pub fn registerDevice(
        &self,
        kind: &str,
        id: &str,
        sourceName: &str,
        settings: Value,
    ) -> Result<(), String> {
        let records = self.records(kind)?;
        let matching = records
            .iter()
            .filter(|record| record.id == id)
            .collect::<Vec<_>>();
        if matching.len() > 1 {
            return Err(format!("Extension ownership conflict: {kind}:{id}"));
        }
        if matching.len() == 1 {
            return Ok(());
        }
        let record = ExtensionRecord {
            kind: kind.to_string(),
            id: id.to_string(),
            scope: "device".to_string(),
            sourceName: sourceName.to_string(),
            settings,
            files: BTreeMap::new(),
        };
        self.write(&record)
    }

    /// Writes configuration in its owner scope and keeps portable file bytes unchanged.
    pub fn setSettings(&self, kind: &str, id: &str, settings: Value) -> Result<(), String> {
        let mut record = self.record(kind, id)?;
        if record.settings == settings {
            return Ok(());
        }
        record.settings = settings;
        self.write(&record)?;
        notifyCatalogChanged();
        Ok(())
    }

    /// Moves a complete extension and its configuration after validating target conflicts and deployment rules.
    pub fn moveScope(&self, kind: &str, id: &str, scope: &str) -> Result<(), String> {
        validateScope(scope)?;
        let source = self.record(kind, id)?;
        if source.settings.get("builtin").and_then(Value::as_bool) == Some(true) {
            return Err(
                "Built-in application resources do not have an installation scope".to_string(),
            );
        }
        if source.scope == scope {
            return Ok(());
        }
        let mut target = source.clone();
        target.scope = scope.to_string();
        self.validate(&target)?;
        for path in self.ownedPaths(&target)? {
            if self.storage.exists(&path).map_err(|e| e.to_string())? {
                return Err(format!(
                    "Target location already contains extension content: {path}"
                ));
            }
        }
        target.files = self.snapshot(&source)?;
        self.validate(&target)?;
        self.materialize(&target)?;
        self.write(&target)?;
        self.remove(&source)?;
        notifyCatalogChanged();
        Ok(())
    }

    /// Deletes one exact installation and publishes a shared tombstone for a space-owned record.
    pub fn delete(&self, kind: &str, id: &str) -> Result<(), String> {
        self.remove(&self.record(kind, id)?)?;
        notifyCatalogChanged();
        Ok(())
    }

    /// Resolves the current scope-owned plugin config path using the existing stable directory naming rule.
    pub fn configPath(&self, id: &str) -> Result<String, String> {
        self.configRoot(&self.record("package", id)?)
    }

    /// Publishes mutations of extension-owned files as one complete shared entity snapshot.
    pub fn publishFileChanges(&self, storagePaths: &[String]) -> Result<(), String> {
        for kind in ["package", "skill"] {
            for mut record in self.records(kind)? {
                if record.scope != "space" {
                    continue;
                }
                let owned = self.ownedPaths(&record)?;
                let changed = storagePaths.iter().any(|path| {
                    owned.iter().any(|root| {
                        path == root
                            || path
                                .strip_prefix(root)
                                .is_some_and(|tail| tail.starts_with('/'))
                    })
                });
                if !changed {
                    continue;
                }
                self.record(kind, &record.id)?;
                record.files = self.snapshot(&record)?;
                self.write(&record)?;
            }
        }
        Ok(())
    }

    /// Captures the pre-apply record so deleted installations and obsolete files can be removed safely.
    pub fn beforeSync(&self, path: &str) -> Result<Option<ExtensionRecord>, String> {
        if !isRecordPath(path) || !self.storage.exists(path).map_err(|e| e.to_string())? {
            return Ok(None);
        }
        let record: ExtensionRecord =
            serde_json::from_slice(&self.storage.readBytes(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        self.validate(&record)?;
        if recordPath(&record.kind, &record.id, "space")? != path || record.scope != "space" {
            return Err("Synchronized extension path mismatch".to_string());
        }
        Ok(Some(record))
    }

    /// Materializes a received installation, preserving separately synchronized config changes on settings-only updates.
    pub fn afterSync(
        &self,
        path: &str,
        operation: &str,
        previous: Option<ExtensionRecord>,
    ) -> Result<(), String> {
        if !isRecordPath(path) {
            return Ok(());
        }
        match operation {
            "delete" => {
                if let Some(record) = previous {
                    self.deleteContent(&record)?;
                }
                Ok(())
            }
            "upsert" => {
                let record = self
                    .beforeSync(path)?
                    .ok_or("Synchronized extension record is missing")?;
                if let Some(old) = previous.as_ref() {
                    for relative in old
                        .files
                        .keys()
                        .filter(|relative| !record.files.contains_key(*relative))
                    {
                        let path = self.filePath(old, relative)?;
                        if self.storage.exists(&path).map_err(|e| e.to_string())? {
                            self.storage
                                .delete(&path, false)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
                for (relative, encoded) in &record.files {
                    if previous
                        .as_ref()
                        .is_some_and(|old| old.files.get(relative) == Some(encoded))
                    {
                        continue;
                    }
                    self.storage
                        .writeBytes(
                            &self.filePath(&record, relative)?,
                            &STANDARD.decode(encoded).map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            _ => Err(format!("Unknown extension sync operation: {operation}")),
        }
    }

    /// Validates content paths and rejects space-local executable MCP definitions before any mutation.
    fn validate(&self, record: &ExtensionRecord) -> Result<(), String> {
        Self::root(&record.kind, &record.scope)?;
        if record.id.trim().is_empty() || !record.settings.is_object() {
            return Err("Extension requires a nonempty identity and object settings".to_string());
        }
        if !record.sourceName.is_empty() {
            validateSegment(&record.sourceName)?;
        }
        match record.kind.as_str() {
            "package" => {
                for field in ["members", "enabledNames", "disabledNames"] {
                    let _: Vec<String> = serde_json::from_value(record.settings[field].clone())
                        .map_err(|e| format!("Invalid package {field}: {e}"))?;
                }
                let _: BTreeMap<String, bool> =
                    serde_json::from_value(record.settings["subpackageStates"].clone())
                        .map_err(|e| e.to_string())?;
                if record.settings["order"].as_u64().is_none()
                    || record.settings["builtin"].as_bool().is_none()
                {
                    return Err("Invalid package ownership schema".to_string());
                }
                let identity = record.settings["installationId"]
                    .as_str()
                    .ok_or("Package installation identity is missing")?;
                if identity.len() != 32 || !identity.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("Invalid package installation identity".to_string());
                }
            }
            "skill" => {
                if record.settings["visible"].as_bool().is_none() {
                    return Err("Skill visibility must be a boolean".to_string());
                }
            }
            "mcp" => {
                if !record.settings["server"].is_object()
                    || !record.settings["metadata"].is_object()
                {
                    return Err("MCP definition and metadata must be objects".to_string());
                }
            }
            _ => return Err("Unknown extension kind".to_string()),
        }
        if record.kind == "mcp" && record.scope == "space" {
            let config = record
                .settings
                .get("server")
                .ok_or("MCP server definition is missing")?;
            let url = config
                .get("url")
                .and_then(Value::as_str)
                .ok_or("Local MCP can only belong to this device")?;
            if url.trim().is_empty()
                || config
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| !command.trim().is_empty())
            {
                return Err("Local MCP can only belong to this device".to_string());
            }
            if !record.sourceName.is_empty() {
                return Err(
                    "Shared remote MCP cannot contain a local deployment directory".to_string(),
                );
            }
        }
        for (relative, content) in &record.files {
            self.filePath(record, relative)?;
            STANDARD.decode(content).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Writes a validated complete record through the ownership-specific persistence API.
    fn write(&self, record: &ExtensionRecord) -> Result<(), String> {
        self.validate(record)?;
        let path = recordPath(&record.kind, &record.id, &record.scope)?;
        let content = serde_json::to_vec(record).map_err(|e| e.to_string())?;
        match record.scope.as_str() {
            "device" => self
                .storage
                .writeBytes(&path, &content)
                .map_err(|e| e.to_string()),
            "space" => RuntimeFileSyncStore::new(self.storage.clone(), RUNTIME_SYNC_DIR_PATH)
                .writeBytes(&path, &content),
            _ => Err("Unknown extension scope".to_string()),
        }
    }

    /// Removes owned content and records only after the target move has been published.
    fn remove(&self, record: &ExtensionRecord) -> Result<(), String> {
        self.deleteContent(record)?;
        let path = recordPath(&record.kind, &record.id, &record.scope)?;
        match record.scope.as_str() {
            "device" => self.storage.delete(&path, false).map_err(|e| e.to_string()),
            "space" => {
                RuntimeFileSyncStore::new(self.storage.clone(), RUNTIME_SYNC_DIR_PATH).delete(&path)
            }
            _ => Err("Unknown extension scope".to_string()),
        }
    }

    /// Deletes only the installation's validated content and configuration roots.
    fn deleteContent(&self, record: &ExtensionRecord) -> Result<(), String> {
        self.validate(record)?;
        for path in self.ownedPaths(record)? {
            if self.storage.exists(&path).map_err(|e| e.to_string())? {
                self.storage
                    .delete(&path, true)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    /// Returns the exact directories or archive files owned by one extension.
    fn ownedPaths(&self, record: &ExtensionRecord) -> Result<Vec<String>, String> {
        let mut paths = Vec::new();
        if !record.sourceName.is_empty() {
            paths.push(format!(
                "{}/{}",
                Self::root(&record.kind, &record.scope)?,
                record.sourceName
            ));
        }
        if record.kind == "package" {
            paths.push(self.configRoot(record)?);
        }
        Ok(paths)
    }

    /// Resolves one package configuration root without changing its stable plugin directory name.
    fn configRoot(&self, record: &ExtensionRecord) -> Result<String, String> {
        let path = operit_util::OperitPaths::pluginConfigDir(&record.id)?;
        let name = path
            .file_name()
            .ok_or("Plugin config directory is invalid")?
            .to_string_lossy();
        Ok(format!(
            "runtime/extensions/{}/plugins/configs/{name}",
            record.scope
        ))
    }

    /// Captures the full portable content tree and existing configuration during a scope move.
    fn snapshot(&self, record: &ExtensionRecord) -> Result<BTreeMap<String, String>, String> {
        let mut files = BTreeMap::new();
        if !record.sourceName.is_empty() {
            let path = format!(
                "{}/{}",
                Self::root(&record.kind, &record.scope)?,
                record.sourceName
            );
            if !self.storage.exists(&path).map_err(|e| e.to_string())? {
                return Err(format!("Extension source is missing: {path}"));
            }
            if record.kind == "package" {
                files.insert(
                    format!("content/{}", record.sourceName),
                    STANDARD.encode(self.storage.readBytes(&path).map_err(|e| e.to_string())?),
                );
            } else {
                self.snapshotTree(&path, &format!("content/{}", record.sourceName), &mut files)?;
            }
        }
        if record.kind == "package" {
            self.snapshotTree(&self.configRoot(record)?, "config", &mut files)?;
        }
        Ok(files)
    }

    /// Recursively records host directory entries and rejects any listing outside its requested root.
    fn snapshotTree(
        &self,
        root: &str,
        prefix: &str,
        files: &mut BTreeMap<String, String>,
    ) -> Result<(), String> {
        for entry in self.storage.list(root).map_err(|e| e.to_string())? {
            let name = entry
                .path
                .strip_prefix(&format!("{root}/"))
                .ok_or("Storage entry escaped its requested root")?;
            validateSegment(name)?;
            let relative = format!("{prefix}/{name}");
            if entry.isDirectory {
                self.snapshotTree(&entry.path, &relative, files)?;
            } else {
                files.insert(
                    relative,
                    STANDARD.encode(
                        self.storage
                            .readBytes(&entry.path)
                            .map_err(|e| e.to_string())?,
                    ),
                );
            }
        }
        Ok(())
    }

    /// Maps a validated bundle entry to its strictly owned materialization path.
    fn filePath(&self, record: &ExtensionRecord, relative: &str) -> Result<String, String> {
        for segment in relative.split('/') {
            validateSegment(segment)?;
        }
        let (section, tail) = relative
            .split_once('/')
            .ok_or("Extension bundle section is missing")?;
        match section {
            "content" => {
                let allowed = match record.kind.as_str() {
                    "package" => tail == record.sourceName,
                    "skill" => tail
                        .strip_prefix(&format!("{}/", record.sourceName))
                        .is_some(),
                    _ => false,
                };
                if !allowed || record.sourceName.is_empty() {
                    return Err("Extension bundle escaped its owned source".to_string());
                }
                Ok(format!(
                    "{}/{tail}",
                    Self::root(&record.kind, &record.scope)?
                ))
            }
            "config" if record.kind == "package" => {
                Ok(format!("{}/{tail}", self.configRoot(record)?))
            }
            _ => Err("Unknown extension bundle section".to_string()),
        }
    }

    /// Writes all validated bundle files without registering duplicate local sync mutations.
    fn materialize(&self, record: &ExtensionRecord) -> Result<(), String> {
        for (relative, encoded) in &record.files {
            self.storage
                .writeBytes(
                    &self.filePath(record, relative)?,
                    &STANDARD.decode(encoded).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// Identifies synchronized installation records by their exact registered root.
pub fn isRecordPath(path: &str) -> bool {
    path.strip_prefix(&format!("{SPACE_EXTENSION_RECORDS}/"))
        .is_some()
}

/// Creates a collision-resistant entity path without deriving filesystem paths from display names.
fn recordPath(kind: &str, id: &str, scope: &str) -> Result<String, String> {
    ExtensionStore::root(kind, scope)?;
    if id.trim().is_empty() {
        return Err("Extension id cannot be empty".to_string());
    }
    Ok(format!(
        "runtime/extensions/{scope}/records/{kind}-{:x}.json",
        Sha256::digest(id.as_bytes())
    ))
}

/// Accepts only the two explicit installation scopes.
fn validateScope(scope: &str) -> Result<(), String> {
    match scope {
        "device" | "space" => Ok(()),
        _ => Err(format!("Unknown extension scope: {scope}")),
    }
}

/// Rejects absolute, traversing and multi-segment names at all filesystem boundaries.
fn validateSegment(segment: &str) -> Result<(), String> {
    if segment.is_empty()
        || segment == "."
        || segment == ".."
        || segment
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
    {
        return Err(format!("Invalid extension path segment: {segment}"));
    }
    Ok(())
}
