use serde::{Deserialize, Serialize};

/// Kotlin memory-space settings, addressed by Operit2's character/shared owner key.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct MemorySettings {
    pub autoSaveIntervalMinutes: i32,
    pub nextAutoSaveRunAtMs: i64,
    pub memoryExtractionCustomRules: String,
    pub profileAutoUpdateEnabled: bool,
    pub profileAutoUpdateLocked: bool,
    pub cloudEmbeddingEnabled: bool,
    pub cloudEmbeddingEndpoint: String,
    pub cloudEmbeddingApiKey: String,
    pub cloudEmbeddingModel: String,
}
impl Default for MemorySettings {
    fn default() -> Self {
        Self { autoSaveIntervalMinutes: 5, nextAutoSaveRunAtMs: 0,
            memoryExtractionCustomRules: String::new(), profileAutoUpdateEnabled: true,
            profileAutoUpdateLocked: false, cloudEmbeddingEnabled: false,
            cloudEmbeddingEndpoint: String::new(), cloudEmbeddingApiKey: String::new(),
            cloudEmbeddingModel: String::new() }
    }
}
impl MemorySettings {
    pub fn normalized(mut self) -> Self {
        self.autoSaveIntervalMinutes = self.autoSaveIntervalMinutes.clamp(1, 30);
        self.nextAutoSaveRunAtMs = self.nextAutoSaveRunAtMs.max(0);
        self.cloudEmbeddingEndpoint = self.cloudEmbeddingEndpoint.trim().to_string();
        self.cloudEmbeddingModel = self.cloudEmbeddingModel.trim().to_string();
        self.cloudEmbeddingApiKey = self.cloudEmbeddingApiKey.trim().to_string();
        self
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MemoryAutoSaveStatus {
    pub ownerKey: String,
    pub pendingCandidates: i32,
    pub pendingChats: i32,
    pub processingCandidates: i32,
    pub failedCandidates: i32,
    pub nextRunAtMs: i64,
    pub minutesUntilNextRun: i64,
    pub lastError: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MemoryRebuildProgress {
    pub status: String,
    pub totalChats: i32,
    pub completedChats: i32,
    pub totalWindows: i32,
    pub completedWindows: i32,
    pub totalSourceMessages: i32,
    pub processedSourceMessages: i32,
    pub failedWindows: i32,
    pub currentChatTitle: String,
    pub lastError: String,
}
