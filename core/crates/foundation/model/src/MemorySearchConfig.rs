use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum MemoryScoreMode {
    BALANCED,
    KEYWORD_FIRST,
    SEMANTIC_FIRST,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemorySearchConfig {
    pub scoreMode: MemoryScoreMode,
    pub keywordWeight: f32,
    pub tagWeight: f32,
    pub vectorWeight: f32,
    pub edgeWeight: f32,
}

impl Default for MemorySearchConfig {
    fn default() -> Self {
        Self {
            scoreMode: MemoryScoreMode::BALANCED,
            keywordWeight: 10.0,
            tagWeight: 0.0,
            vectorWeight: 0.0,
            edgeWeight: 0.4,
        }
    }
}

impl MemorySearchConfig {
    pub fn normalized(mut self) -> Self {
        for weight in [&mut self.keywordWeight, &mut self.tagWeight, &mut self.vectorWeight, &mut self.edgeWeight] {
            *weight = if weight.is_finite() { weight.max(0.0) } else { 0.0 };
        }
        self
    }
}
