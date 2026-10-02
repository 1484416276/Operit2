use operit_store::ExtensionStore::ExtensionStore;
use operit_store::PreferencesDataStore::PreferencesDataStoreError;
use operit_store::RuntimeStorePaths::RuntimeStorePaths;

/// Owns skill visibility in the same scope as the skill itself.
pub struct SkillVisibilityPreferences;

impl SkillVisibilityPreferences {
    /// Opens the scope-aware skill visibility facade.
    #[allow(non_snake_case)]
    pub fn getInstance() -> Self {
        Self
    }

    /// Creates the facade; all data is owned by the installed skill record.
    pub fn new(_paths: RuntimeStorePaths) -> Self {
        Self
    }

    /// Reads the installed skill's explicitly stored AI visibility.
    #[allow(non_snake_case)]
    pub fn isSkillVisibleToAi(&self, skillName: &str) -> bool {
        let record = ExtensionStore::default()
            .record("skill", skillName)
            .expect("A scanned skill must have a readable ownership record");
        record.settings["visible"]
            .as_bool()
            .expect("Registered skill visibility must be a boolean")
    }

    /// Persists AI visibility through the skill's exact owning storage contract.
    #[allow(non_snake_case)]
    pub fn setSkillVisibleToAi(
        &self,
        skillName: &str,
        visible: bool,
    ) -> Result<(), PreferencesDataStoreError> {
        let store = ExtensionStore::default();
        let mut record = store
            .record("skill", skillName)
            .map_err(PreferencesDataStoreError::Message)?;
        record.settings["visible"] = serde_json::json!(visible);
        store
            .setSettings("skill", skillName, record.settings)
            .map_err(PreferencesDataStoreError::Message)
    }
}
