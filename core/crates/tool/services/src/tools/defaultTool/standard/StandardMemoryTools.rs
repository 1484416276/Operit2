use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone, Timelike};

use crate::runtime_support::ToolRuntimeSupport;
use operit_model::CharacterCard::CharacterCardMemoryBindingMode;
use operit_store::repository::MemoryRepository::{MemoryLinkInfo, MemoryRepository};
use operit_store::repository::UserMarkdownRepository::UserMarkdownRepository;
use operit_store::RuntimeStorageHost::defaultRuntimeStorageHost;
use operit_tools::tools::ToolResultDataClasses::{
    stringResultData, JsOptional, LinkInfo, MemoryInfo, MemoryLinkQueryResultData,
    MemoryLinkResultData, MemoryQueryResultData, ToolResultData,
};
use operit_tools::ConversationMarkupManager::ToolResult;
use operit_tools::ToolExecutionManager::{
    AITool, ToolAccessSpec, ToolBoundary, ToolEffect, ToolExecutionManager, ToolExecutor,
    ToolValidationResult,
};
use operit_util::OperitPaths::{
    characterMemoryOwnerKey, parseMemoryOwnerKey, sharedMemoryOwnerKey,
};

const MAX_QUERY_SNAPSHOTS_PER_OWNER: usize = 32;
const DEFAULT_RELEVANCE_THRESHOLD: f64 = 0.0;

#[derive(Clone, Debug)]
pub enum MemoryToolOperation {
    GetMemoryOwnerKey,
    QueryMemory,
    GetMemoryByTitle,
    CreateMemory,
    UpdateMemory,
    DeleteMemory,
    MoveMemory,
    UpdateUserPreferences,
    LinkMemories,
    QueryMemoryLinks,
    UpdateMemoryLink,
    DeleteMemoryLink,
}

#[derive(Clone)]
pub struct MemoryToolExecutor {
    pub operation: MemoryToolOperation,
    pub runtimeSupport: Arc<dyn ToolRuntimeSupport>,
}

#[derive(Clone, Debug)]
struct QuerySnapshotState {
    seenMemoryKeys: HashSet<String>,
    lastAccessAtMs: i64,
}

#[derive(Clone, Debug)]
struct OwnedMemoryResult {
    ownerKey: String,
    memory: operit_model::Memory::Memory,
}

impl ToolExecutor for MemoryToolExecutor {
    fn validateParameters(&self, tool: &AITool) -> ToolValidationResult {
        if matches!(self.operation, MemoryToolOperation::QueryMemory)
            && optionalParameterValue(tool, "query")
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
        {
            return ToolValidationResult {
                valid: false,
                errorMessage: "Missing or empty required parameter: query".to_string(),
            };
        }
        ToolValidationResult {
            valid: true,
            errorMessage: String::new(),
        }
    }

    fn accessSpec(&self, _tool: &AITool) -> Result<ToolAccessSpec, String> {
        let effect = match self.operation {
            MemoryToolOperation::GetMemoryOwnerKey
            | MemoryToolOperation::QueryMemory
            | MemoryToolOperation::GetMemoryByTitle
            | MemoryToolOperation::QueryMemoryLinks => ToolEffect::READ,
            MemoryToolOperation::CreateMemory
            | MemoryToolOperation::UpdateMemory
            | MemoryToolOperation::DeleteMemory
            | MemoryToolOperation::MoveMemory
            | MemoryToolOperation::UpdateUserPreferences
            | MemoryToolOperation::LinkMemories
            | MemoryToolOperation::UpdateMemoryLink
            | MemoryToolOperation::DeleteMemoryLink => ToolEffect::WRITE,
        };
        Ok(ToolAccessSpec {
            effect,
            boundary: ToolBoundary::None,
        })
    }

    fn invokeAndStream(&mut self, tool: &AITool) -> Vec<ToolResult> {
        let result = match self.operation {
            MemoryToolOperation::GetMemoryOwnerKey => {
                executeGetMemoryOwnerKey(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::QueryMemory => {
                executeQueryMemory(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::GetMemoryByTitle => {
                executeGetMemoryByTitle(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::CreateMemory => {
                executeCreateMemory(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::UpdateMemory => {
                executeUpdateMemory(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::DeleteMemory => {
                executeDeleteMemory(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::MoveMemory => {
                executeMoveMemory(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::UpdateUserPreferences => {
                executeUpdateUserPreferences(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::LinkMemories => {
                executeLinkMemories(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::QueryMemoryLinks => {
                executeQueryMemoryLinks(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::UpdateMemoryLink => {
                executeUpdateMemoryLink(tool, self.runtimeSupport.as_ref())
            }
            MemoryToolOperation::DeleteMemoryLink => {
                executeDeleteMemoryLink(tool, self.runtimeSupport.as_ref())
            }
        };
        vec![result]
    }
}

/// Returns the registered memory owner bound to the specified calling character card.
fn executeGetMemoryOwnerKey(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let callerCardId = parameterValue(tool, "caller_card_id");
    if callerCardId.trim().is_empty() {
        return errorResult(tool, "caller_card_id parameter is required");
    }
    match boundOwnerKeyForCaller(tool, runtimeSupport).and_then(|ownerKey| {
        runtimeSupport.assertMemoryOwnerExists(&ownerKey)?;
        Ok(ownerKey)
    }) {
        Ok(ownerKey) => success(tool, ownerKey),
        Err(error) => errorResult(tool, &error),
    }
}

fn executeQueryMemory(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let ownerKeys = match resolveReadableOwnerKeys(tool, runtimeSupport) {
        Ok(ownerKeys) => ownerKeys,
        Err(error) => return errorResult(tool, &error),
    };
    let query = parameterValue(tool, "query");
    if query.trim().is_empty() {
        return errorResult(tool, "Query parameter cannot be empty.");
    }
    let folderPath = optionalParameterValue(tool, "folder_path");
    let limitParam = optionalParameterValue(tool, "limit");
    let limit = match parseLimit(
        limitParam.as_deref(),
        if query.trim() == "*" { usize::MAX } else { 20 },
    ) {
        Ok(value) => value,
        Err(error) => return errorResult(tool, &error),
    };
    let threshold = match optionalParameterValue(tool, "threshold")
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.parse::<f64>())
    {
        Some(Ok(value)) if value >= 0.0 => value,
        Some(_) => return errorResult(tool, "Invalid threshold. Expected a non-negative number."),
        None => DEFAULT_RELEVANCE_THRESHOLD,
    };
    let startTime =
        match parseTimeBoundary(optionalParameterValue(tool, "start_time").as_deref(), false) {
            Ok(value) => value,
            Err(error) => return errorResult(tool, &error),
        };
    let endTime = match parseTimeBoundary(optionalParameterValue(tool, "end_time").as_deref(), true)
    {
        Ok(value) => value,
        Err(error) => return errorResult(tool, &error),
    };
    if let (Some(start), Some(end)) = (startTime, endTime) {
        if start > end {
            return errorResult(tool, "Invalid time range: start_time must be <= end_time.");
        }
    }

    let snapshotIdParam = optionalParameterValue(tool, "snapshot_id")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let snapshotScope = ownerKeys.join("|");
    let (snapshotId, snapshotCreated) = resolveSnapshot(&snapshotScope, snapshotIdParam);
    let mut results = Vec::new();
    for ownerKey in &ownerKeys {
        match MemoryRepository::new(ownerKey.clone()).searchMemoriesWithConfig(
            &query,
            folderPath.as_deref(),
            threshold,
            startTime,
            endTime,
            match operit_store::repository::MemorySettingsRepository::MemorySettingsRepository::new(ownerKey).loadSearchConfig() {
                Ok(config)=>config, Err(error)=>return errorResult(tool,&error),
            },
        ) {
            Ok(ownerResults) => {
                results.extend(ownerResults.into_iter().map(|memory| OwnedMemoryResult {
                    ownerKey: ownerKey.clone(),
                    memory,
                }))
            }
            Err(error) => {
                return errorResult(tool, &format!("Failed to execute memory query: {error}"))
            }
        }
    }

    let (excluded, returned) = selectSnapshotResults(&snapshotScope, &snapshotId, results, limit);
    let mut data=memoryQueryResultData(&returned,Some(snapshotId),snapshotCreated,excluded);
    for (entry,info) in returned.iter().zip(&mut data.memories) {
        if entry.memory.isDocumentNode {
            let repo=MemoryRepository::new(entry.ownerKey.clone());
            let chunks=match repo.searchChunksInDocument(entry.memory.id,query.clone(),limit.min(20) as i32) {
                Ok(chunks)=>chunks,Err(error)=>return errorResult(tool,&error),
            };
            let total=match repo.getTotalChunkCount(entry.memory.id) {Ok(n)=>n,Err(error)=>return errorResult(tool,&error)};
            info.content=if query.trim()=="*" || limit>20 {format!("Document: {} ({total} chunks)",entry.memory.title)} else {formatDocumentChunks(&entry.memory.title,total,&chunks)};
            if !chunks.is_empty() {
                info.chunkInfo=JsOptional::Value(format!("Chunks {}/{total}",chunks.iter().take(5).map(|c|(c.chunkIndex+1).to_string()).collect::<Vec<_>>().join(", ")));
                info.chunkIndices=JsOptional::Value(chunks.iter().map(|c|c.chunkIndex).collect());
            }
        }
    }
    successData(
        tool,
        ToolResultData::MemoryQueryResultData(data),
    )
}

fn executeGetMemoryByTitle(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let title = parameterValue(tool, "title");
    if title.trim().is_empty() {
        return errorResult(tool, "title parameter is required");
    }
    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let repository = MemoryRepository::new(ownerKey.clone());
    match repository.findMemoryByTitle(&title) {
        Ok(Some(memory)) if memory.isDocumentNode => {
            let query=optionalParameterValue(tool,"query").unwrap_or_default();
            let index=optionalParameterValue(tool,"chunk_index").unwrap_or_default();
            let range=optionalParameterValue(tool,"chunk_range").unwrap_or_default();
            let limit=match parseLimit(optionalParameterValue(tool,"limit").as_deref(),20) {Ok(v)=>v.min(i32::MAX as usize) as i32,Err(error)=>return errorResult(tool,&error)};
            let chunks=match repository.getChunksForMemory(memory.id) {Ok(c)=>c,Err(error)=>return errorResult(tool,&error)};
            let total=chunks.len() as i32;
            if query.trim().is_empty() && index.trim().is_empty() && range.trim().is_empty() {
                return success(tool,format!("Document: {} ({total} chunks). Specify query, chunk_range (1-based start-end) or chunk_index (1-based).",memory.title));
            }
            let selected=if !query.trim().is_empty() {
                match repository.searchChunksInDocument(memory.id,query,limit) {Ok(c)=>c,Err(error)=>return errorResult(tool,&error)}
            } else {
                let (start,end)=if !range.trim().is_empty() {
                    let Some((a,b))=range.split_once('-') else {return errorResult(tool,"Invalid chunk_range. Expected start-end");};
                    match (a.trim().parse::<i32>(),b.trim().parse::<i32>()) {(Ok(a),Ok(b))=>(a,b),_=>return errorResult(tool,"Invalid chunk range")}
                } else {match index.trim().parse::<i32>() {Ok(i)=>(i,i),Err(_)=>return errorResult(tool,"Invalid chunk index")}};
                if start<1 || end>total || start>end {return errorResult(tool,&format!("Chunk range out of bounds. Valid range: 1-{total}"));}
                chunks.into_iter().filter(|c|c.chunkIndex>=start-1 && c.chunkIndex<=end-1).collect()
            };
            if selected.is_empty() {return errorResult(tool,"No matching chunks found");}
            success(tool,formatDocumentChunks(&memory.title,total,&selected))
        },
        Ok(Some(memory)) => successData(
            tool,
            ToolResultData::MemoryQueryResultData(memoryQueryResultData(
                &[OwnedMemoryResult { ownerKey, memory }],
                None,
                false,
                0,
            )),
        ),
        Ok(None) => errorResult(tool, &format!("Memory not found with title: {title}")),
        Err(error) => errorResult(tool, &format!("Failed to get memory by title: {error}")),
    }
}

fn executeCreateMemory(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let title = parameterValue(tool, "title");
    let content = parameterValue(tool, "content");
    if title.trim().is_empty() || content.trim().is_empty() {
        return errorResult(tool, "Both title and content parameters are required");
    }
    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let repository = MemoryRepository::new(ownerKey);
    let contentType =
        optionalParameterValue(tool, "content_type").unwrap_or_else(|| "text/plain".to_string());
    let source = optionalParameterValue(tool, "source").unwrap_or_else(|| "ai_created".to_string());
    let folderPath = optionalParameterValue(tool, "folder_path").unwrap_or_default();
    let tags = optionalParameterValue(tool, "tags").map(parseTags);
    match repository.createMemory(
        title.clone(),
        content,
        contentType,
        source,
        folderPath,
        tags,
    ) {
        Ok(memory) => success(
            tool,
            format!(
                "Successfully created memory: '{}' (UUID: {})",
                title, memory.uuid
            ),
        ),
        Err(error) => errorResult(tool, &format!("Failed to create memory: {error}")),
    }
}

fn executeUpdateMemory(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let oldTitle = parameterValue(tool, "old_title");
    if oldTitle.trim().is_empty() {
        return errorResult(
            tool,
            "old_title parameter is required to identify the memory",
        );
    }
    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let repository = MemoryRepository::new(ownerKey);
    let memory = match repository.findMemoryByTitle(&oldTitle) {
        Ok(Some(memory)) => memory,
        Ok(None) => return errorResult(tool, &format!("Memory not found with title: {oldTitle}")),
        Err(error) => return errorResult(tool, &format!("Failed to update memory: {error}")),
    };
    let newTitle =
        optionalParameterValue(tool, "new_title").unwrap_or_else(|| memory.title.clone());
    let newContent =
        optionalParameterValue(tool, "content").unwrap_or_else(|| memory.content.clone());
    let newContentType =
        optionalParameterValue(tool, "content_type").unwrap_or_else(|| memory.contentType.clone());
    let newSource = optionalParameterValue(tool, "source").unwrap_or_else(|| memory.source.clone());
    let newCredibility = optionalParameterValue(tool, "credibility")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(memory.credibility);
    let newImportance = optionalParameterValue(tool, "importance")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(memory.importance);
    let newFolderPath = optionalParameterValue(tool, "folder_path").or(memory.folderPath.clone());
    let newTags = optionalParameterValue(tool, "tags").map(parseTags);
    match repository.updateMemory(
        memory.id,
        newTitle.clone(),
        newContent,
        newContentType,
        newSource,
        newCredibility,
        newImportance,
        newFolderPath,
        newTags,
    ) {
        Ok(_) => success(
            tool,
            format!("Successfully updated memory from '{oldTitle}' to '{newTitle}'"),
        ),
        Err(error) => errorResult(tool, &format!("Failed to update memory: {error}")),
    }
}

fn executeDeleteMemory(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let title = parameterValue(tool, "title");
    if title.trim().is_empty() {
        return errorResult(tool, "title parameter is required to identify the memory");
    }
    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let repository = MemoryRepository::new(ownerKey);
    let memory = match repository.findMemoryByTitle(&title) {
        Ok(Some(memory)) => memory,
        Ok(None) => return errorResult(tool, &format!("Memory not found with title: {title}")),
        Err(error) => return errorResult(tool, &format!("Failed to delete memory: {error}")),
    };
    match repository.deleteMemory(memory.id) {
        Ok(true) => success(tool, format!("Successfully deleted memory: '{title}'")),
        Ok(false) => errorResult(tool, "Failed to delete memory"),
        Err(error) => errorResult(tool, &format!("Failed to delete memory: {error}")),
    }
}

fn executeMoveMemory(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let targetFolderPath = match optionalParameterValue(tool, "target_folder_path") {
        Some(value) => value,
        None => return errorResult(tool, "target_folder_path parameter is required"),
    };
    let sourceFolderPath = optionalParameterValue(tool, "source_folder_path");
    let hasSourceFolderParam = tool
        .parameters
        .iter()
        .any(|parameter| parameter.name == "source_folder_path");
    let titles = optionalParameterValue(tool, "titles")
        .map(parseTitles)
        .unwrap_or_default();
    if titles.is_empty() && !hasSourceFolderParam {
        return errorResult(
            tool,
            "Provide titles and/or source_folder_path to select memories to move",
        );
    }

    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let repository = MemoryRepository::new(ownerKey);
    let mut selected = Vec::new();
    if !titles.is_empty() {
        for title in titles {
            match repository.findMemoriesByTitle(&title) {
                Ok(memories) => selected.extend(memories),
                Err(error) => {
                    return errorResult(tool, &format!("Failed to move memories: {error}"))
                }
            }
        }
    }
    if hasSourceFolderParam {
        match repository.getMemoriesByFolderPath(sourceFolderPath.as_deref().unwrap_or("")) {
            Ok(memories) => {
                if selected.is_empty() {
                    selected = memories;
                } else {
                    let folderIds = memories
                        .into_iter()
                        .map(|memory| memory.id)
                        .collect::<HashSet<_>>();
                    selected.retain(|memory| folderIds.contains(&memory.id));
                }
            }
            Err(error) => return errorResult(tool, &format!("Failed to move memories: {error}")),
        }
    }
    let mut ids = selected
        .into_iter()
        .map(|memory| memory.id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return errorResult(tool, "No matching memories found to move");
    }
    match repository.moveMemoriesToFolder(&ids, &targetFolderPath) {
        Ok(true) => success(
            tool,
            format!(
                "Successfully moved {} memories to '{}'",
                ids.len(),
                targetFolderPath
            ),
        ),
        Ok(false) => errorResult(tool, "Failed to move selected memories"),
        Err(error) => errorResult(tool, &format!("Failed to move memories: {error}")),
    }
}

fn executeUpdateUserPreferences(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let content = parameterValue(tool, "content");
    if content.trim().is_empty() {
        return errorResult(tool, "content parameter is required");
    }
    match UserMarkdownRepository::new(ownerKey, defaultRuntimeStorageHost())
        .writeUserMarkdown(content)
    {
        Ok(_) => success(tool, "Successfully updated USER.md".to_string()),
        Err(error) => errorResult(tool, &format!("Failed to update USER.md: {error}")),
    }
}

fn executeLinkMemories(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let sourceTitle = parameterValue(tool, "source_title");
    let targetTitle = parameterValue(tool, "target_title");
    if sourceTitle.trim().is_empty() || targetTitle.trim().is_empty() {
        return errorResult(
            tool,
            "Both source_title and target_title parameters are required",
        );
    }
    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let repository = MemoryRepository::new(ownerKey);
    let source = match repository.findMemoryByTitle(&sourceTitle) {
        Ok(Some(memory)) => memory,
        Ok(None) => {
            return errorResult(
                tool,
                &format!("Source memory not found with title: {sourceTitle}"),
            )
        }
        Err(error) => return errorResult(tool, &format!("Failed to link memories: {error}")),
    };
    let target = match repository.findMemoryByTitle(&targetTitle) {
        Ok(Some(memory)) => memory,
        Ok(None) => {
            return errorResult(
                tool,
                &format!("Target memory not found with title: {targetTitle}"),
            )
        }
        Err(error) => return errorResult(tool, &format!("Failed to link memories: {error}")),
    };
    let linkType =
        optionalParameterValue(tool, "link_type").unwrap_or_else(|| "related".to_string());
    let weight = optionalParameterValue(tool, "weight")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(MemoryRepository::MEDIUM_LINK)
        .clamp(0.0, 1.0);
    let description = optionalParameterValue(tool, "description").unwrap_or_default();
    match repository.linkMemories(
        source.id,
        target.id,
        linkType.clone(),
        weight,
        description.clone(),
    ) {
        Ok(_) => successData(
            tool,
            ToolResultData::MemoryLinkResultData(MemoryLinkResultData {
                sourceTitle,
                targetTitle,
                linkType,
                weight,
                description,
            }),
        ),
        Err(error) => errorResult(tool, &format!("Failed to link memories: {error}")),
    }
}

fn executeQueryMemoryLinks(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let repository = MemoryRepository::new(ownerKey);
    let linkId = match optionalParameterValue(tool, "link_id")
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.parse::<i64>())
    {
        Some(Ok(value)) => Some(value),
        Some(Err(_)) => return errorResult(tool, "Invalid link_id. Expected integer."),
        None => None,
    };
    let sourceMemoryId = match optionalParameterValue(tool, "source_title")
        .filter(|value| !value.trim().is_empty())
    {
        Some(title) => match repository.findMemoryByTitle(&title) {
            Ok(Some(memory)) => Some(memory.id),
            Ok(None) => {
                return errorResult(
                    tool,
                    &format!("Source memory not found with title: {title}"),
                )
            }
            Err(error) => {
                return errorResult(tool, &format!("Failed to query memory links: {error}"))
            }
        },
        None => None,
    };
    let targetMemoryId = match optionalParameterValue(tool, "target_title")
        .filter(|value| !value.trim().is_empty())
    {
        Some(title) => match repository.findMemoryByTitle(&title) {
            Ok(Some(memory)) => Some(memory.id),
            Ok(None) => {
                return errorResult(
                    tool,
                    &format!("Target memory not found with title: {title}"),
                )
            }
            Err(error) => {
                return errorResult(tool, &format!("Failed to query memory links: {error}"))
            }
        },
        None => None,
    };
    let limit = match parseLimit(optionalParameterValue(tool, "limit").as_deref(), 20) {
        Ok(value) => value.min(200),
        Err(error) => return errorResult(tool, &error),
    };
    let linkType = optionalParameterValue(tool, "link_type");
    match repository.queryMemoryLinks(
        linkId,
        sourceMemoryId,
        targetMemoryId,
        linkType.as_deref(),
        limit,
    ) {
        Ok(links) => successData(
            tool,
            ToolResultData::MemoryLinkQueryResultData(memoryLinkQueryResultData(&links)),
        ),
        Err(error) => errorResult(tool, &format!("Failed to query memory links: {error}")),
    }
}

fn executeUpdateMemoryLink(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let repository = MemoryRepository::new(ownerKey);
    let newLinkType = optionalParameterValue(tool, "new_link_type");
    let newWeight =
        optionalParameterValue(tool, "weight").and_then(|value| value.parse::<f32>().ok());
    let newDescription = optionalParameterValue(tool, "description");
    if newLinkType
        .as_ref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
        && newWeight.is_none()
        && newDescription.is_none()
    {
        return errorResult(
            tool,
            "At least one of new_link_type, weight, description must be provided",
        );
    }
    let link = match resolveLink(tool, &repository) {
        Ok(link) => link,
        Err(error) => return errorResult(tool, &error),
    };
    let type_ = newLinkType.unwrap_or_else(|| link.link.type_.clone());
    let weight = newWeight.unwrap_or(link.link.weight).clamp(0.0, 1.0);
    let description = newDescription.unwrap_or_else(|| link.link.description.clone());
    match repository.updateLink(link.link.id, type_, weight, description) {
        Ok(updated) => successData(
            tool,
            ToolResultData::MemoryLinkQueryResultData(memoryLinkQueryResultData(&[updated])),
        ),
        Err(error) => errorResult(tool, &format!("Failed to update memory link: {error}")),
    }
}

fn executeDeleteMemoryLink(tool: &AITool, runtimeSupport: &dyn ToolRuntimeSupport) -> ToolResult {
    let ownerKey = match resolveTargetOwnerKey(tool, runtimeSupport) {
        Ok(ownerKey) => ownerKey,
        Err(error) => return errorResult(tool, &error),
    };
    let repository = MemoryRepository::new(ownerKey);
    let link = match resolveLink(tool, &repository) {
        Ok(link) => link,
        Err(error) => return errorResult(tool, &error),
    };
    match repository.deleteLink(link.link.id) {
        Ok(true) => success(
            tool,
            format!("Successfully deleted memory link: {}", link.link.id),
        ),
        Ok(false) => errorResult(
            tool,
            &format!("Failed to delete memory link with id: {}", link.link.id),
        ),
        Err(error) => errorResult(tool, &format!("Failed to delete memory link: {error}")),
    }
}

fn resolveLink(tool: &AITool, repository: &MemoryRepository) -> Result<MemoryLinkInfo, String> {
    let linkId =
        optionalParameterValue(tool, "link_id").and_then(|value| value.parse::<i64>().ok());
    if let Some(linkId) = linkId {
        return repository
            .findLinkById(linkId)?
            .ok_or_else(|| format!("Link not found with id: {linkId}"));
    }
    let sourceTitle = parameterValue(tool, "source_title");
    let targetTitle = parameterValue(tool, "target_title");
    if sourceTitle.trim().is_empty() || targetTitle.trim().is_empty() {
        return Err("Provide link_id, or provide both source_title and target_title".to_string());
    }
    let source = repository
        .findMemoryByTitle(&sourceTitle)?
        .ok_or_else(|| format!("Source memory not found with title: {sourceTitle}"))?;
    let target = repository
        .findMemoryByTitle(&targetTitle)?
        .ok_or_else(|| format!("Target memory not found with title: {targetTitle}"))?;
    let linkType = optionalParameterValue(tool, "link_type");
    let links = repository.queryMemoryLinks(
        None,
        Some(source.id),
        Some(target.id),
        linkType.as_deref(),
        2,
    )?;
    match links.len() {
        0 => Err("No matching link found".to_string()),
        1 => Ok(links[0].clone()),
        _ => {
            Err("Multiple links matched. Provide link_id or a more specific link_type.".to_string())
        }
    }
}

fn resolveCallerCardId(tool: &AITool) -> Option<String> {
    let explicitCallerCardId = tool
        .parameters
        .iter()
        .find(|parameter| parameter.name == "caller_card_id")
        .map(|parameter| parameter.value.trim().to_string())
        .filter(|value| !value.is_empty());
    if explicitCallerCardId.is_some() {
        return explicitCallerCardId;
    }
    ToolExecutionManager::currentToolRuntimeContext()
        .and_then(|context| context.callerCardId)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn explicitTargetOwnerKey(tool: &AITool) -> Option<String> {
    optionalParameterValue(tool, "target_owner_key")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Resolves the memory owner for a write or exact-lookup tool.
///
/// An explicit `target_owner_key` must already exist. When omitted, the current
/// character card's bound store is used.
fn resolveTargetOwnerKey(
    tool: &AITool,
    runtimeSupport: &dyn ToolRuntimeSupport,
) -> Result<String, String> {
    if let Some(ownerKey) = explicitTargetOwnerKey(tool) {
        parseMemoryOwnerKey(&ownerKey)?;
        runtimeSupport.assertMemoryOwnerExists(&ownerKey)?;
        return Ok(ownerKey);
    }
    boundOwnerKeyForCaller(tool, runtimeSupport)
}

/// Resolves the memory owner bound to the calling character card.
fn boundOwnerKeyForCaller(
    tool: &AITool,
    runtimeSupport: &dyn ToolRuntimeSupport,
) -> Result<String, String> {
    let callerCardId = resolveCallerCardId(tool)
        .ok_or_else(|| "caller_card_id or target_owner_key parameter is required".to_string())?;
    let characterCard = runtimeSupport.characterMemoryBinding(&callerCardId)?;
    if characterCard.memoryBindingMode == CharacterCardMemoryBindingMode::SHARED {
        let sharedMemoryId = characterCard
            .sharedMemoryId
            .ok_or_else(|| "shared memory binding requires sharedMemoryId".to_string())?;
        return Ok(sharedMemoryOwnerKey(&sharedMemoryId)?);
    }
    characterMemoryOwnerKey(&characterCard.id)
}

fn resolveReadableOwnerKeys(
    tool: &AITool,
    runtimeSupport: &dyn ToolRuntimeSupport,
) -> Result<Vec<String>, String> {
    if let Some(ownerKey) = explicitTargetOwnerKey(tool) {
        parseMemoryOwnerKey(&ownerKey)?;
        runtimeSupport.assertMemoryOwnerExists(&ownerKey)?;
        return Ok(vec![ownerKey]);
    }
    Ok(vec![boundOwnerKeyForCaller(tool, runtimeSupport)?])
}

fn parseTimeBoundary(value: Option<&str>, isEnd: bool) -> Result<Option<i64>, String> {
    let Some(trimmed) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if let Ok(parsed) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
        let parsed = parsed
            .with_second(if isEnd { 59 } else { 0 })
            .and_then(|value| value.with_nanosecond(if isEnd { 999_000_000 } else { 0 }))
            .ok_or_else(|| "Invalid date-time value.".to_string())?;
        return localTimestampMillis(parsed);
    }
    if let Ok(parsed) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let parsed = parsed
            .and_hms_milli_opt(
                if isEnd { 23 } else { 0 },
                if isEnd { 59 } else { 0 },
                if isEnd { 59 } else { 0 },
                if isEnd { 999 } else { 0 },
            )
            .ok_or_else(|| "Invalid date value.".to_string())?;
        return localTimestampMillis(parsed);
    }
    if isEnd {
        Err("Invalid end_time. Expected format YYYY-MM-DD or YYYY-MM-DD HH:mm.".to_string())
    } else {
        Err("Invalid start_time. Expected format YYYY-MM-DD or YYYY-MM-DD HH:mm.".to_string())
    }
}

fn localTimestampMillis(value: NaiveDateTime) -> Result<Option<i64>, String> {
    let Some(local) = Local.from_local_datetime(&value).single() else {
        return Err("Local time is ambiguous or invalid.".to_string());
    };
    Ok(Some(local.timestamp_millis()))
}

fn resolveSnapshot(ownerScope: &str, requestedSnapshotId: Option<String>) -> (String, bool) {
    let mut snapshots = snapshotStore()
        .lock()
        .expect("memory query snapshot store mutex poisoned");
    let ownerSnapshots = snapshots.entry(ownerScope.to_string()).or_default();
    trimOldSnapshots(ownerSnapshots);
    let id = requestedSnapshotId.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let created = !ownerSnapshots.contains_key(&id);
    ownerSnapshots
        .entry(id.clone())
        .or_insert(QuerySnapshotState {
            seenMemoryKeys: HashSet::new(),
            lastAccessAtMs: operit_host_api::TimeUtils::currentTimeMillis(),
        });
    (id, created)
}

fn selectSnapshotResults(
    ownerScope: &str,
    snapshotId: &str,
    results: Vec<OwnedMemoryResult>,
    limit: usize,
) -> (usize, Vec<OwnedMemoryResult>) {
    let mut snapshots = snapshotStore()
        .lock()
        .expect("memory query snapshot store mutex poisoned");
    let snapshot = snapshots
        .entry(ownerScope.to_string())
        .or_default()
        .entry(snapshotId.to_string())
        .or_insert(QuerySnapshotState {
            seenMemoryKeys: HashSet::new(),
            lastAccessAtMs: operit_host_api::TimeUtils::currentTimeMillis(),
        });
    let excluded = results
        .iter()
        .filter(|entry| snapshot.seenMemoryKeys.contains(&memorySnapshotKey(entry)))
        .count();
    let selected = results
        .into_iter()
        .filter(|entry| !snapshot.seenMemoryKeys.contains(&memorySnapshotKey(entry)))
        .take(limit)
        .collect::<Vec<_>>();
    for entry in &selected {
        snapshot.seenMemoryKeys.insert(memorySnapshotKey(entry));
    }
    snapshot.lastAccessAtMs = operit_host_api::TimeUtils::currentTimeMillis();
    (excluded, selected)
}

fn trimOldSnapshots(ownerSnapshots: &mut HashMap<String, QuerySnapshotState>) {
    if ownerSnapshots.len() <= MAX_QUERY_SNAPSHOTS_PER_OWNER {
        return;
    }
    let overflow = ownerSnapshots.len() - MAX_QUERY_SNAPSHOTS_PER_OWNER;
    let mut entries = ownerSnapshots
        .iter()
        .map(|(id, state)| (id.clone(), state.lastAccessAtMs))
        .collect::<Vec<_>>();
    entries.sort_by_key(|(_, lastAccessAtMs)| *lastAccessAtMs);
    for (id, _) in entries.into_iter().take(overflow) {
        ownerSnapshots.remove(&id);
    }
}

fn memorySnapshotKey(entry: &OwnedMemoryResult) -> String {
    format!("{}:{}", entry.ownerKey, entry.memory.id)
}

fn snapshotStore() -> &'static Mutex<HashMap<String, HashMap<String, QuerySnapshotState>>> {
    static SNAPSHOTS: OnceLock<Mutex<HashMap<String, HashMap<String, QuerySnapshotState>>>> =
        OnceLock::new();
    SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn memoryQueryResultData(
    memories: &[OwnedMemoryResult],
    snapshotId: Option<String>,
    snapshotCreated: bool,
    excludedBySnapshotCount: usize,
) -> MemoryQueryResultData {
    MemoryQueryResultData {
        memories: memories.iter().map(memoryInfo).collect(),
        snapshotId: JsOptional::from_nullable_option(snapshotId),
        snapshotCreated: Some(snapshotCreated),
        excludedBySnapshotCount: Some(excludedBySnapshotCount as i32),
    }
}

fn memoryInfo(entry: &OwnedMemoryResult) -> MemoryInfo {
    let memory = &entry.memory;
    MemoryInfo {
        ownerKey: entry.ownerKey.clone(),
        title: memory.title.clone(),
        content: memory.content.clone(),
        source: memory.source.clone(),
        tags: memory.tags.iter().map(|tag| tag.name.clone()).collect(),
        createdAt: formatMillis(memory.createdAt),
        chunkInfo: JsOptional::Null,
        chunkIndices: JsOptional::Null,
    }
}

fn memoryLinkQueryResultData(links: &[MemoryLinkInfo]) -> MemoryLinkQueryResultData {
    MemoryLinkQueryResultData {
        totalCount: links.len() as i32,
        links: links
            .iter()
            .map(|info| LinkInfo {
                linkId: info.link.id,
                sourceTitle: info.sourceTitle.clone(),
                targetTitle: info.targetTitle.clone(),
                linkType: info.link.type_.clone(),
                weight: info.link.weight,
                description: info.link.description.clone(),
            })
            .collect(),
    }
}

fn formatMemoryResults(
    query: &str,
    memories: &[operit_model::Memory::Memory],
    snapshotId: Option<&str>,
    snapshotCreated: bool,
    excludedBySnapshotCount: usize,
    settingsText: &str,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("query: {query}"));
    if let Some(snapshotId) = snapshotId {
        lines.push(format!("snapshot_id: {snapshotId}"));
        lines.push(format!("snapshot_created: {snapshotCreated}"));
        lines.push(format!(
            "excluded_by_snapshot_count: {excludedBySnapshotCount}"
        ));
    }
    if !settingsText.is_empty() {
        lines.push(settingsText.to_string());
    }
    lines.push(format!("total_count: {}", memories.len()));
    for memory in memories {
        let folder = memory.folderPath.clone().unwrap_or_default();
        let tags = memory
            .tags
            .iter()
            .map(|tag| tag.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "\n- title: {}\n  content: {}\n  source: {}\n  tags: {}\n  folder_path: {}\n  created_at: {}",
            memory.title,
            wildcardSummary(memory.content.as_str(), query),
            memory.source,
            tags,
            folder,
            formatMillis(memory.createdAt)
        ));
    }
    lines.join("\n")
}

fn wildcardSummary(content: &str, query: &str) -> String {
    if query.trim() == "*" && content.chars().count() > 10 {
        format!("{}...", content.chars().take(10).collect::<String>())
    } else {
        content.to_string()
    }
}

fn formatMemoryLinks(links: &[MemoryLinkInfo]) -> String {
    let mut lines = Vec::new();
    lines.push(format!("total_count: {}", links.len()));
    for info in links {
        lines.push(format!(
            "\n- link_id: {}\n  source_title: {}\n  target_title: {}\n  link_type: {}\n  weight: {}\n  description: {}",
            info.link.id,
            info.sourceTitle,
            info.targetTitle,
            info.link.type_,
            info.link.weight,
            info.link.description
        ));
    }
    lines.join("\n")
}

fn formatMillis(value: i64) -> String {
    let Some(value) = Local.timestamp_millis_opt(value).single() else {
        return value.to_string();
    };
    value.format("%Y-%m-%d %H:%M").to_string()
}

fn parseLimit(value: Option<&str>, defaultLimit: usize) -> Result<usize, String> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(defaultLimit.max(1));
    };
    let parsed = raw
        .parse::<usize>()
        .map_err(|_| "Invalid limit. Expected integer.".to_string())?;
    Ok(parsed.max(1))
}

fn parseTags(raw: String) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parseTitles(raw: String) -> Vec<String> {
    raw.split(|ch| matches!(ch, ',' | '\n' | '|'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn success(tool: &AITool, result: String) -> ToolResult {
    ToolResult {
        toolName: tool.name.clone(),
        success: true,
        result: stringResultData(result),
        error: None,
    }
}

fn successData(tool: &AITool, data: ToolResultData) -> ToolResult {
    ToolResult {
        toolName: tool.name.clone(),
        success: true,
        result: data,
        error: None,
    }
}

fn errorResult(tool: &AITool, message: &str) -> ToolResult {
    ToolResult {
        toolName: tool.name.clone(),
        success: false,
        result: stringResultData(""),
        error: Some(message.to_string()),
    }
}

fn parameterValue(tool: &AITool, name: &str) -> String {
    optionalParameterValue(tool, name).unwrap_or_default()
}

fn optionalParameterValue(tool: &AITool, name: &str) -> Option<String> {
    tool.parameters
        .iter()
        .find(|parameter| parameter.name == name)
        .map(|parameter| parameter.value.clone())
}

fn formatDocumentChunks(title:&str,total:i32,chunks:&[operit_model::DocumentChunk::DocumentChunk])->String {
    format!("Document: {title}\n{}",chunks.iter().take(5).map(|c|format!("Chunk {}/{total}:\n{}",c.chunkIndex+1,c.content)).collect::<Vec<_>>().join("\n---\n"))
}
