use operit_host_api::TimeUtils::currentTimeMillis;
use operit_model::MemorySettings::MemorySettings;
use operit_model::MemorySearchConfig::MemorySearchConfig;
use operit_util::OperitPaths;
use crate::PreferencesDataStore::{stringPreferencesKey, PreferencesDataStore};
use crate::RuntimeStorageHost::defaultRuntimeStorageHost;

#[derive(Clone)]
pub struct MemorySettingsRepository { dataStore: PreferencesDataStore }
impl MemorySettingsRepository {
    pub fn new(ownerKey: &str) -> Self {
        Self { dataStore: PreferencesDataStore::newWithStorage(defaultRuntimeStorageHost(),
            OperitPaths::memorySearchSettingsStoragePath(ownerKey).expect("valid memory owner")) }
    }
    pub fn load(&self) -> Result<MemorySettings, String> {
        let preferences = self.dataStore.data().map_err(|e| e.to_string())?;
        preferences.get(&stringPreferencesKey("memory_settings"))
            .map(|s| serde_json::from_str::<MemorySettings>(s).map(MemorySettings::normalized).map_err(|e|e.to_string()))
            .unwrap_or_else(|| Ok(MemorySettings::default()))
    }
    pub fn save(&self, settings: MemorySettings) -> Result<(), String> {
        let mut settings = settings.normalized();
        let old = self.load()?;
        // Editing preferences must not overwrite a scheduler's newer next-run timestamp.
        settings.nextAutoSaveRunAtMs = if old.autoSaveIntervalMinutes != settings.autoSaveIntervalMinutes {
            currentTimeMillis() + i64::from(settings.autoSaveIntervalMinutes) * 60_000
        } else { old.nextAutoSaveRunAtMs };
        self.writeSettings(&settings)
    }
    fn writeSettings(&self, settings: &MemorySettings) -> Result<(), String> {
        let encoded = serde_json::to_string(settings).map_err(|e|e.to_string())?;
        self.dataStore.edit(|p|p.set(&stringPreferencesKey("memory_settings"), encoded.clone())).map_err(|e|e.to_string())
    }
    pub fn nextRunAt(&self, now: i64) -> Result<i64, String> {
        let settings = self.load()?;
        if settings.nextAutoSaveRunAtMs > 0 { return Ok(settings.nextAutoSaveRunAtMs); }
        let next = now + i64::from(settings.autoSaveIntervalMinutes) * 60_000;
        self.scheduleNextRun(next)?;
        Ok(next)
    }
    pub fn scheduleNextRun(&self, next: i64) -> Result<(), String> {
        let mut settings = self.load()?;
        settings.nextAutoSaveRunAtMs = next.max(0);
        self.writeSettings(&settings)
    }
    pub fn loadSearchConfig(&self) -> Result<MemorySearchConfig, String> {
        let preferences = self.dataStore.data().map_err(|e|e.to_string())?;
        preferences.get(&stringPreferencesKey("memory_search_config"))
            .map(|s|serde_json::from_str::<MemorySearchConfig>(s).map(MemorySearchConfig::normalized).map_err(|e|e.to_string()))
            .unwrap_or_else(||Ok(MemorySearchConfig::default()))
    }
    pub fn saveSearchConfig(&self, config: MemorySearchConfig) -> Result<(), String> {
        let encoded = serde_json::to_string(&config.normalized()).map_err(|e|e.to_string())?;
        self.dataStore.edit(|p|p.set(&stringPreferencesKey("memory_search_config"), encoded.clone())).map_err(|e|e.to_string())
    }
}
