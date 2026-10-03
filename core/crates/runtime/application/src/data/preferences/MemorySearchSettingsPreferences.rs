use operit_model::MemorySearchConfig::MemorySearchConfig;
use operit_store::PreferencesDataStore::{
    stringPreferencesKey, PreferencesDataStore, PreferencesDataStoreError,
};
use operit_store::RuntimeStorageHost::defaultRuntimeStorageHost;
use operit_util::OperitPaths;

#[derive(Clone)]
pub struct MemorySearchSettingsPreferences {
    dataStore: PreferencesDataStore,
}

impl MemorySearchSettingsPreferences {
    /// Creates a memory search preference store backed by runtime storage.
    pub fn new(profileId: impl AsRef<str>) -> Self {
        Self {
            dataStore: PreferencesDataStore::newWithStorage(
                defaultRuntimeStorageHost(),
                OperitPaths::memorySearchSettingsStoragePath(profileId.as_ref())
                    .expect("memory search settings storage path must be valid"),
            ),
        }
    }

    /// Loads one persisted memory search configuration.
    pub fn load(&self) -> Result<MemorySearchConfig, PreferencesDataStoreError> {
        let preferences = self.dataStore.data()?;
        let Some(encoded) = preferences.get(&stringPreferencesKey("memory_search_config")) else {
            return Ok(MemorySearchConfig::default());
        };
        serde_json::from_str::<MemorySearchConfig>(encoded).map(MemorySearchConfig::normalized).map_err(PreferencesDataStoreError::from)
    }

    /// Saves one memory search configuration.
    pub fn save(&self, config: &MemorySearchConfig) -> Result<(), PreferencesDataStoreError> {
        let encoded = serde_json::to_string(&config.clone().normalized())?;
        self.dataStore.edit(|preferences| {
            preferences.set(
                &stringPreferencesKey("memory_search_config"),
                encoded.clone(),
            );
        })
    }
    /// Loads interval, extraction rules, profile locks and optional cloud embedding configuration.
    pub fn loadSettings(&self) -> Result<operit_model::MemorySettings::MemorySettings, String> {
        let preferences = self.dataStore.data().map_err(|e|e.to_string())?;
        preferences.get(&stringPreferencesKey("memory_settings"))
            .map(|s|serde_json::from_str::<operit_model::MemorySettings::MemorySettings>(s).map(|s|s.normalized()).map_err(|e|e.to_string()))
            .unwrap_or_else(||Ok(Default::default()))
    }
    pub fn saveSettings(&self, settings: operit_model::MemorySettings::MemorySettings) -> Result<(), String> {
        let old = self.loadSettings()?;
        let mut settings = settings.normalized();
        settings.nextAutoSaveRunAtMs = if old.autoSaveIntervalMinutes != settings.autoSaveIntervalMinutes {
            operit_host_api::TimeUtils::currentTimeMillis() + i64::from(settings.autoSaveIntervalMinutes) * 60_000
        } else { old.nextAutoSaveRunAtMs };
        let encoded = serde_json::to_string(&settings).map_err(|e|e.to_string())?;
        self.dataStore.edit(|p|p.set(&stringPreferencesKey("memory_settings"), encoded.clone())).map_err(|e|e.to_string())
    }
}
