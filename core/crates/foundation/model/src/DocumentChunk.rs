use serde::{Deserialize, Serialize};

/// Persisted document paragraph. UUID binding survives import/sync ID remapping.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DocumentChunk {
    pub id: i64,
    pub memoryUuid: String,
    pub chunkIndex: i32,
    pub content: String,
}
