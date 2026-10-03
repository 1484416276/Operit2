use serde::{Deserialize, Serialize};
use crate::MemorySearchConfig::MemoryScoreMode;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemorySearchDebugInfo {
    pub query: String,
    pub keywords: Vec<String>,
    pub lexicalTokens: Vec<String>,
    pub scoreMode: MemoryScoreMode,
    pub relevanceThreshold: f64,
    pub effectiveKeywordWeight: f64,
    pub effectiveTagWeight: f64,
    pub effectiveSemanticWeight: f32,
    pub semanticKeywordNormFactor: f64,
    pub effectiveEdgeWeight: f64,
    pub memoriesInScopeCount: i32,
    pub keywordMatchesCount: i32,
    pub tagMatchesCount: i32,
    pub reverseContainmentMatchesCount: i32,
    pub semanticMatchesCount: i32,
    pub graphEdgesTraversed: i32,
    pub scoredCount: i32,
    pub passedThresholdCount: i32,
    pub candidates: Vec<MemorySearchDebugCandidate>,
    pub finalResultIds: Vec<i64>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MemorySearchDebugCandidate {
    pub memoryId: i64,
    pub title: String,
    pub folderPath: Option<String>,
    pub matchedKeywordTokenCount: i32,
    pub keywordScore: f64,
    pub tagScore: f64,
    pub reverseContainmentScore: f64,
    pub semanticScore: f64,
    pub edgeScore: f64,
    pub totalScore: f64,
    pub passedThreshold: bool,
}
