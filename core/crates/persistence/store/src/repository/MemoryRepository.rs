use std::collections::{BTreeSet, HashMap, HashSet};

use operit_host_api::TimeUtils::currentTimeMillis;
use uuid::Uuid;

use operit_model::Memory::{
    Memory, MemoryGraph, MemoryGraphEdge, MemoryGraphNode, MemoryLink, MemoryTag,
};
use operit_model::MemoryExportModel::{
    ImportStrategy, MemoryExportData, MemoryImportResult, SerializableLink, SerializableMemory,
};
use operit_util::OperitPaths::{memoryLinkSqlitePath, memorySqlitePath};

use crate::ObjectBoxStore::ObjectBox;

/// Repository for memories and typed links owned by one memory namespace.
#[derive(Clone)]
pub struct MemoryRepository {
    ownerKey: String,
    memoryBox: ObjectBox<Memory>,
    linkBox: ObjectBox<MemoryLink>,
    chunkBox: ObjectBox<operit_model::DocumentChunk::DocumentChunk>,
}

/// Link record enriched with source and target memory titles.
#[derive(Clone, Debug)]
pub struct MemoryLinkInfo {
    pub link: MemoryLink,
    pub sourceTitle: String,
    pub targetTitle: String,
}

impl MemoryRepository {
    /// Weight used for a strong semantic relationship between memories.
    pub const STRONG_LINK: f32 = 1.0;
    /// Weight used for a medium semantic relationship between memories.
    pub const MEDIUM_LINK: f32 = 0.7;
    /// Weight used for a weak semantic relationship between memories.
    pub const WEAK_LINK: f32 = 0.3;

    /// Opens the memory and link stores for one owner namespace.
    pub fn new(ownerKey: impl Into<String>) -> Self {
        let ownerKey = ownerKey.into();
        Self {
            ownerKey: ownerKey.clone(),
            memoryBox: ObjectBox::new(
                memorySqlitePath(&ownerKey).expect("memory sqlite path must be valid"),
            ),
            chunkBox: ObjectBox::new(
                memorySqlitePath(&ownerKey).expect("memory sqlite path must be valid"),
            ),
            linkBox: ObjectBox::new(
                memoryLinkSqlitePath(&ownerKey).expect("memory link sqlite path must be valid"),
            ),
        }
    }

    /// Returns the namespace key backing this repository.
    #[allow(non_snake_case)]
    pub fn ownerKey(&self) -> &str {
        &self.ownerKey
    }

    /// Normalizes user-facing folder paths into slash-delimited memory folder keys.
    pub fn normalizeFolderPath(folderPath: Option<&str>) -> Option<String> {
        let raw = folderPath.map(str::trim)?;
        if raw.is_empty() {
            return None;
        }
        let parts = raw
            .replace('\\', "/")
            .split('/')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("/"))
        }
    }

    /// Searches a read-only memory snapshot without changing timestamps or recording sync operations.
    pub fn searchMemories(
        &self,
        query: &str,
        folderPath: Option<&str>,
        relevanceThreshold: f64,
        createdAtStartMs: Option<i64>,
        createdAtEndMs: Option<i64>,
    ) -> Result<Vec<Memory>, String> {
        let normalizedFolder = Self::normalizeFolderPath(folderPath);
        let query = query.trim();
        // Kotlin searches only read entities; routing a search through editEntities
        // would rewrite the collection and replay stale full-record sync states.
        let memories = self
            .memoryBox
            .all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|memory| {
                if normalizedFolder.is_some()
                    && normalizedFolder.as_deref() != memory.folderPath.as_deref()
                {
                    return false;
                }
                if let Some(start) = createdAtStartMs {
                    if memory.createdAt < start {
                        return false;
                    }
                }
                if let Some(end) = createdAtEndMs {
                    if memory.createdAt > end {
                        return false;
                    }
                }
                true
            })
            .collect::<Vec<_>>();
        // Kotlin returns the scoped snapshot directly for wildcard and blank searches.
        if query == "*" || query.is_empty() {
            return Ok(memories);
        }
        self.searchMemoriesWithConfig(query, folderPath, relevanceThreshold, createdAtStartMs,
            createdAtEndMs, super::MemorySettingsRepository::MemorySettingsRepository::new(&self.ownerKey).loadSearchConfig()?)
    }

    /// Kotlin-compatible hybrid search using the actual persisted owner weights.
    pub fn searchMemoriesWithConfig(
        &self, query: &str, folderPath: Option<&str>, relevanceThreshold: f64,
        createdAtStartMs: Option<i64>, createdAtEndMs: Option<i64>,
        config: operit_model::MemorySearchConfig::MemorySearchConfig,
    ) -> Result<Vec<Memory>,String> {
        let scoped=self.searchMemories("*",folderPath,0.0,createdAtStartMs,createdAtEndMs)?;
        if query.trim().is_empty() || query.trim()=="*" {return Ok(scoped);}
        let debug=self.searchMemoriesDebug(query,folderPath,relevanceThreshold,createdAtStartMs,createdAtEndMs,config)?;
        let byId=scoped.into_iter().map(|m|(m.id,m)).collect::<HashMap<_,_>>();
        Ok(debug.finalResultIds.into_iter().filter_map(|id|byId.get(&id).cloned()).collect())
    }
    pub fn searchMemoriesDebug(
        &self, query: &str, folderPath: Option<&str>, relevanceThreshold: f64,
        createdAtStartMs: Option<i64>, createdAtEndMs: Option<i64>,
        config: operit_model::MemorySearchConfig::MemorySearchConfig,
    ) -> Result<operit_model::MemorySearchDebugInfo::MemorySearchDebugInfo,String> {
        let memories=self.searchMemories("*",folderPath,0.0,createdAtStartMs,createdAtEndMs)?;
        let links=self.linkBox.all().map_err(|e|e.to_string())?;
        let semantic=if config.vectorWeight>0.0 { self.semanticSimilarities(query,&memories)? } else {Vec::new()};
        let mut debug=super::MemorySearch::compute(query,&memories,&links,config,relevanceThreshold,&semantic);
        if query.trim().is_empty() || query.trim()=="*" {
            debug.finalResultIds=memories.iter().map(|m|m.id).collect();
            debug.passedThresholdCount=memories.len() as i32;
        }
        Ok(debug)
    }

    fn semanticSimilarities(&self,query:&str,memories:&[Memory])->Result<Vec<Vec<(i64,f32)>>,String> {
        let settings=super::MemorySettingsRepository::MemorySettingsRepository::new(&self.ownerKey).load()?;
        if !settings.cloudEmbeddingEnabled || settings.cloudEmbeddingEndpoint.is_empty() || settings.cloudEmbeddingModel.is_empty() {return Ok(Vec::new());}
        let mut vectors=Vec::new();
        for memory in memories {vectors.push((memory.id,self.cloudEmbedding(if memory.isDocumentNode { &memory.title } else { &memory.content },&settings)?));}
        let mut result=Vec::new();
        for keyword in super::MemorySearch::keywords(query) {
            let query=self.cloudEmbedding(&keyword,&settings)?;
            let mut matches=vectors.iter().filter_map(|(id,v)|cosineSimilarity(&query,v).map(|score|(*id,score))).collect::<Vec<_>>();
            matches.sort_by(|a,b|b.1.total_cmp(&a.1));matches.truncate(200);result.push(matches);
        }
        Ok(result)
    }

    /// Provider-independent cloud embedding through the configured Host HTTP capability.
    fn cloudEmbedding(&self,text:&str,settings:&operit_model::MemorySettings::MemorySettings)->Result<Vec<f32>,String> {
        use sha2::{Digest,Sha256};
        use crate::PreferencesDataStore::{PreferencesDataStore,stringPreferencesKey};
        let dataStore=PreferencesDataStore::newWithStorage(crate::RuntimeStorageHost::defaultRuntimeStorageHost(),
            operit_util::OperitPaths::memorySearchSettingsStoragePath(&self.ownerKey)?);
        let digest=format!("{:x}",Sha256::digest(format!("{}\n{}\n{}",settings.cloudEmbeddingEndpoint,settings.cloudEmbeddingModel,text)));
        let key=stringPreferencesKey(&format!("embedding_{digest}"));
        if let Some(encoded)=dataStore.data().map_err(|e|e.to_string())?.get(&key) {return serde_json::from_str(encoded).map_err(|e|e.to_string());}
        let endpoint=settings.cloudEmbeddingEndpoint.trim_end_matches('/');
        let url=if endpoint.ends_with("/embeddings") {endpoint.to_string()} else {format!("{endpoint}/embeddings")};
        let mut headers=vec![("Content-Type".into(),"application/json".into())];
        if !settings.cloudEmbeddingApiKey.is_empty() {headers.push(("Authorization".into(),format!("Bearer {}",settings.cloudEmbeddingApiKey)));}
        let response=operit_host_api::HostManager::defaultHttpHost().executeHttpRequest(operit_host_api::HttpRequestData {
            url,method:"POST".into(),headers,body:serde_json::to_vec(&serde_json::json!({"input":text,"model":settings.cloudEmbeddingModel,"encoding_format":"float"})).map_err(|e|e.to_string())?,
            formFields:Vec::new(),fileParts:Vec::new(),connectTimeoutSeconds:30,readTimeoutSeconds:60,followRedirects:true,
            ignoreSsl:false,proxyHost:String::new(),proxyPort:0,
        }).map_err(|e|e.to_string())?;
        if !(200..300).contains(&response.statusCode) {return Err(format!("Embedding request failed: HTTP {}",response.statusCode));}
        let json:serde_json::Value=serde_json::from_slice(&response.body).map_err(|e|e.to_string())?;
        let vector=json.pointer("/data/0/embedding").and_then(serde_json::Value::as_array).ok_or("Embedding response has no vector")?
            .iter().map(|v|v.as_f64().filter(|n|n.is_finite()).map(|n|n as f32).ok_or("Invalid embedding number".to_string())).collect::<Result<Vec<_>,_>>()?;
        if vector.is_empty() {return Err("Embedding response has an empty vector".into());}
        let encoded=serde_json::to_string(&vector).map_err(|e|e.to_string())?;
        dataStore.edit(|p|p.set(&key,encoded.clone())).map_err(|e|e.to_string())?;
        Ok(vector)
    }
    pub fn rebuildEmbeddings(&self)->Result<i32,String> {
        let settings=super::MemorySettingsRepository::MemorySettingsRepository::new(&self.ownerKey).load()?;
        if !settings.cloudEmbeddingEnabled {return Err("Cloud embedding is disabled".into());}
        let memories=self.memoryBox.all().map_err(|e|e.to_string())?;
        for memory in &memories {
            self.cloudEmbedding(if memory.isDocumentNode { &memory.title } else { &memory.content },&settings)?;
            for chunk in self.getChunksForMemory(memory.id)? {self.cloudEmbedding(&chunk.content,&settings)?;}
        }
        Ok(memories.len() as i32)
    }

    /// Kotlin paragraph splitting, stored inside the owner database and synchronized as entities.
    pub fn createMemoryFromDocument(&self, documentName:String, originalPath:String, text:String, folderPath:String)->Result<Memory,String> {
        if documentName.trim().is_empty() || text.trim().is_empty() {return Err("document name and text must not be empty".into());}
        let split=regex::Regex::new(r"(\r?\n[\t ]*){2,}").map_err(|e|e.to_string())?;
        let separators=regex::Regex::new(r"(?m)^[*\-=_]{3,}\s*$").map_err(|e|e.to_string())?;
        let paragraphs=split.split(&text).map(|p|separators.replace_all(p, "").trim().to_string()).filter(|p|!p.is_empty()).collect::<Vec<_>>();
        if paragraphs.is_empty() {return Err("document has no readable paragraphs".into());}
        let mut memory=self.createMemory(documentName.clone(),format!("Document: {documentName}"),"text/plain".into(),"document_import".into(),folderPath,Some(Vec::new()))?;
        memory.isDocumentNode=true;memory.documentPath=Some(originalPath);
        let memory=self.saveMemory(memory)?;
        for (index,content) in paragraphs.into_iter().enumerate() {
            self.chunkBox.put(operit_model::DocumentChunk::DocumentChunk {id:0,memoryUuid:memory.uuid.clone(),chunkIndex:index as i32,content}).map_err(|e|e.to_string())?;
        }
        Ok(memory)
    }
    pub fn getChunksForMemory(&self,memoryId:i64)->Result<Vec<operit_model::DocumentChunk::DocumentChunk>,String> {
        let Some(memory)=self.memoryBox.get(memoryId).map_err(|e|e.to_string())? else {return Ok(Vec::new());};
        let mut chunks=self.chunkBox.all().map_err(|e|e.to_string())?.into_iter().filter(|c|c.memoryUuid==memory.uuid).collect::<Vec<_>>();
        chunks.sort_by_key(|c|c.chunkIndex);Ok(chunks)
    }
    pub fn getChunkByIndex(&self,memoryId:i64,chunkIndex:i32)->Result<Option<operit_model::DocumentChunk::DocumentChunk>,String> {
        Ok(self.getChunksForMemory(memoryId)?.into_iter().find(|c|c.chunkIndex==chunkIndex))
    }
    pub fn getChunksByRange(&self,memoryId:i64,startIndex:i32,endIndex:i32)->Result<Vec<operit_model::DocumentChunk::DocumentChunk>,String> {
        Ok(self.getChunksForMemory(memoryId)?.into_iter().filter(|c|c.chunkIndex>=startIndex && c.chunkIndex<=endIndex).collect())
    }
    pub fn getTotalChunkCount(&self,memoryId:i64)->Result<i32,String> {Ok(self.getChunksForMemory(memoryId)?.len() as i32)}
    pub fn searchChunksInDocument(&self,memoryId:i64,query:String,limit:i32)->Result<Vec<operit_model::DocumentChunk::DocumentChunk>,String> {
        let Some(memory)=self.memoryBox.get(memoryId).map_err(|e|e.to_string())? else {return Ok(Vec::new());};
        if !memory.isDocumentNode {return Ok(Vec::new());}
        let chunks=self.getChunksForMemory(memoryId)?;
        if query.trim().is_empty() || query.trim()=="*" {return Ok(chunks.into_iter().take(limit.max(1) as usize).collect());}
        let config=super::MemorySettingsRepository::MemorySettingsRepository::new(&self.ownerKey).loadSearchConfig()?;
        let settings=super::MemorySettingsRepository::MemorySettingsRepository::new(&self.ownerKey).load()?;
        let mut semantic=Vec::new();
        if config.vectorWeight>0.0 && settings.cloudEmbeddingEnabled {
            let mut vectors=Vec::new();for chunk in &chunks {vectors.push((chunk.id,self.cloudEmbedding(&chunk.content,&settings)?));}
            for keyword in super::MemorySearch::keywords(&query) {
                let embedding=self.cloudEmbedding(&keyword,&settings)?;
                let mut found=vectors.iter().filter_map(|(id,v)|cosineSimilarity(&embedding,v).map(|score|(*id,score))).collect::<Vec<_>>();
                found.sort_by(|a,b|b.1.total_cmp(&a.1));found.truncate(200);semantic.push(found);
            }
        }
        let ids=super::MemorySearch::computeChunks(&query,&chunks,config,&semantic);
        let byId=chunks.into_iter().map(|c|(c.id,c)).collect::<HashMap<_,_>>();
        Ok(ids.into_iter().take(limit.max(1) as usize).filter_map(|id|byId.get(&id).cloned()).collect())
    }

    /// Finds the first memory with the exact normalized title.
    pub fn findMemoryByTitle(&self, title: &str) -> Result<Option<Memory>, String> {
        let normalizedTitle = title.trim();
        self.memoryBox
            .query()
            .filter({
                let normalizedTitle = normalizedTitle.to_string();
                move |memory| memory.title == normalizedTitle
            })
            .build()
            .findFirst()
            .map_err(|error| error.to_string())
    }

    /// Finds all memories with the exact normalized title.
    pub fn findMemoriesByTitle(&self, title: &str) -> Result<Vec<Memory>, String> {
        let normalizedTitle = title.trim();
        self.memoryBox
            .query()
            .filter({
                let normalizedTitle = normalizedTitle.to_string();
                move |memory| memory.title == normalizedTitle
            })
            .build()
            .find()
            .map_err(|error| error.to_string())
    }

    /// Returns memories stored under one normalized folder path.
    pub fn getMemoriesByFolderPath(&self, folderPath: &str) -> Result<Vec<Memory>, String> {
        let normalizedFolder = Self::normalizeFolderPath(Some(folderPath));
        self.memoryBox
            .query()
            .filter(move |memory| memory.folderPath == normalizedFolder)
            .build()
            .find()
            .map_err(|error| error.to_string())
    }

    /// Creates and stores a new memory with generated identity and timestamps.
    pub fn createMemory(
        &self,
        title: String,
        content: String,
        contentType: String,
        source: String,
        folderPath: String,
        tags: Option<Vec<String>>,
    ) -> Result<Memory, String> {
        let now = nowMillis();
        let memory = Memory {
            id: 0,
            uuid: Uuid::new_v4().to_string(),
            title,
            content,
            contentType,
            source,
            credibility: 0.5,
            importance: 0.5,
            documentPath: None,
            isDocumentNode: false,
            chunkIndexFilePath: None,
            folderPath: Self::normalizeFolderPath(Some(&folderPath)),
            createdAt: now,
            updatedAt: now,
            lastAccessedAt: now,
            tags: buildTags(tags.unwrap_or_default()),
            properties: Vec::new(),
        };
        self.memoryBox
            .put(memory)
            .map_err(|error| error.to_string())
    }

    /// Saves a memory after normalizing identity, timestamps, and folder path.
    #[allow(non_snake_case)]
    pub fn saveMemory(&self, mut memory: Memory) -> Result<Memory, String> {
        let now = nowMillis();
        if memory.uuid.trim().is_empty() {
            memory.uuid = Uuid::new_v4().to_string();
        }
        if memory.createdAt <= 0 {
            memory.createdAt = now;
        }
        memory.updatedAt = now;
        if memory.lastAccessedAt <= 0 {
            memory.lastAccessedAt = now;
        }
        memory.folderPath = Self::normalizeFolderPath(memory.folderPath.as_deref());
        self.memoryBox
            .put(memory)
            .map_err(|error| error.to_string())
    }

    /// Adds a tag to a memory and returns the updated memory.
    #[allow(non_snake_case)]
    pub fn addTagToMemory(&self, memoryId: i64, tagName: &str) -> Result<Option<Memory>, String> {
        let tagName = tagName.trim();
        if tagName.is_empty() {
            return Ok(self
                .memoryBox
                .get(memoryId)
                .map_err(|error| error.to_string())?);
        }
        self.memoryBox
            .editEntities(|memories| {
                let Some(memory) = memories.iter_mut().find(|memory| memory.id == memoryId) else {
                    return Ok(None);
                };
                if !memory.tags.iter().any(|tag| tag.name == tagName) {
                    let nextId = memory.tags.iter().map(|tag| tag.id).max().unwrap_or(0) + 1;
                    memory.tags.push(MemoryTag {
                        id: nextId,
                        name: tagName.to_string(),
                    });
                    memory.updatedAt = nowMillis();
                }
                Ok(Some(memory.clone()))
            })
            .map_err(|error| error.to_string())?
    }

    /// Merges memories with matching source titles into one primary memory.
    #[allow(non_snake_case)]
    pub fn mergeMemories(
        &self,
        sourceTitles: Vec<String>,
        newTitle: String,
        newContent: String,
        newTags: Vec<String>,
        folderPath: String,
    ) -> Result<Option<Memory>, String> {
        let mut sourceMemories = Vec::new();
        for title in &sourceTitles {
            sourceMemories.extend(self.findMemoriesByTitle(title)?);
        }
        if sourceMemories.is_empty() {
            return Ok(None);
        }
        let primaryId = sourceMemories[0].id;
        let sourceIds = sourceMemories
            .iter()
            .map(|memory| memory.id)
            .collect::<HashSet<_>>();
        let merged = self.updateMemory(
            primaryId,
            newTitle,
            newContent,
            "text".to_string(),
            "memory_analysis".to_string(),
            sourceMemories
                .iter()
                .map(|memory| memory.credibility)
                .fold(0.0_f32, f32::max),
            sourceMemories
                .iter()
                .map(|memory| memory.importance)
                .fold(0.0_f32, f32::max),
            Self::normalizeFolderPath(Some(&folderPath)),
            Some(newTags),
        )?;
        self.linkBox
            .editEntities(|links| {
                for link in links.iter_mut() {
                    if sourceIds.contains(&link.sourceMemoryId) {
                        link.sourceMemoryId = primaryId;
                    }
                    if sourceIds.contains(&link.targetMemoryId) {
                        link.targetMemoryId = primaryId;
                    }
                }
                links.retain(|link| link.sourceMemoryId != link.targetMemoryId);
            })
            .map_err(|error| error.to_string())?;
        let redundant = sourceIds
            .into_iter()
            .filter(|id| *id != primaryId)
            .collect::<Vec<_>>();
        self.memoryBox
            .removeByIds(&redundant)
            .map_err(|error| error.to_string())?;
        Ok(Some(merged))
    }

    /// Updates one memory's content, scoring metadata, folder, and tags.
    pub fn updateMemory(
        &self,
        memoryId: i64,
        newTitle: String,
        newContent: String,
        newContentType: String,
        newSource: String,
        newCredibility: f32,
        newImportance: f32,
        newFolderPath: Option<String>,
        newTags: Option<Vec<String>>,
    ) -> Result<Memory, String> {
        self.memoryBox
            .editEntities(|memories| {
                let memory = memories
                    .iter_mut()
                    .find(|memory| memory.id == memoryId)
                    .ok_or_else(|| format!("Memory not found with id: {memoryId}"))?;
                memory.title = newTitle;
                memory.content = newContent;
                memory.contentType = newContentType;
                memory.source = newSource;
                memory.credibility = newCredibility.clamp(0.0, 1.0);
                memory.importance = newImportance.clamp(0.0, 1.0);
                memory.folderPath = Self::normalizeFolderPath(newFolderPath.as_deref());
                if let Some(tags) = newTags {
                    memory.tags = buildTags(tags);
                }
                memory.updatedAt = nowMillis();
                Ok(memory.clone())
            })
            .map_err(|error| error.to_string())?
    }

    /// Deletes one memory and removes links that reference it.
    pub fn deleteMemory(&self, memoryId: i64) -> Result<bool, String> {
        let chunks=self.getChunksForMemory(memoryId)?;
        let deleted = self
            .memoryBox
            .remove(memoryId)
            .map_err(|error| error.to_string())?;
        if deleted {
            self.chunkBox.removeByIds(&chunks.iter().map(|c|c.id).collect::<Vec<_>>()).map_err(|e|e.to_string())?;
            self.linkBox
                .editEntities(|links| {
                    links.retain(|link| {
                        link.sourceMemoryId != memoryId && link.targetMemoryId != memoryId
                    });
                })
                .map_err(|error| error.to_string())?;
        }
        Ok(deleted)
    }

    /// Moves selected memories into a normalized target folder.
    pub fn moveMemoriesToFolder(
        &self,
        memoryIds: &[i64],
        targetFolderPath: &str,
    ) -> Result<bool, String> {
        let selected = memoryIds.iter().copied().collect::<HashSet<_>>();
        let normalizedFolder = Self::normalizeFolderPath(Some(targetFolderPath));
        let mut changed = false;
        self.memoryBox
            .editEntities(|memories| {
                for memory in memories {
                    if selected.contains(&memory.id) {
                        memory.folderPath = normalizedFolder.clone();
                        memory.updatedAt = nowMillis();
                        changed = true;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(changed)
    }

    /// Creates a typed weighted link between two existing memories.
    pub fn linkMemories(
        &self,
        sourceMemoryId: i64,
        targetMemoryId: i64,
        type_: String,
        weight: f32,
        description: String,
    ) -> Result<MemoryLink, String> {
        let memories = self.memoryBox.all().map_err(|error| error.to_string())?;
        if !memories.iter().any(|memory| memory.id == sourceMemoryId) {
            return Err(format!("Source memory not found with id: {sourceMemoryId}"));
        }
        if !memories.iter().any(|memory| memory.id == targetMemoryId) {
            return Err(format!("Target memory not found with id: {targetMemoryId}"));
        }
        let link = MemoryLink {
            id: 0,
            sourceMemoryId,
            targetMemoryId,
            type_,
            weight: weight.clamp(0.0, 1.0),
            description,
        };
        self.linkBox.put(link).map_err(|error| error.to_string())
    }

    /// Queries links and enriches them with source and target titles.
    pub fn queryMemoryLinks(
        &self,
        linkId: Option<i64>,
        sourceMemoryId: Option<i64>,
        targetMemoryId: Option<i64>,
        linkType: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryLinkInfo>, String> {
        let links = self.linkBox.all().map_err(|error| error.to_string())?;
        let memories = self.memoryBox.all().map_err(|error| error.to_string())?;
        let mut result = Vec::new();
        for link in &links {
            if linkId.is_some_and(|value| value != link.id) {
                continue;
            }
            if sourceMemoryId.is_some_and(|value| value != link.sourceMemoryId) {
                continue;
            }
            if targetMemoryId.is_some_and(|value| value != link.targetMemoryId) {
                continue;
            }
            if linkType.is_some_and(|value| value != link.type_) {
                continue;
            }
            let sourceTitle = memories
                .iter()
                .find(|memory| memory.id == link.sourceMemoryId)
                .map(|memory| memory.title.clone())
                .ok_or_else(|| format!("Dangling source memory id: {}", link.sourceMemoryId))?;
            let targetTitle = memories
                .iter()
                .find(|memory| memory.id == link.targetMemoryId)
                .map(|memory| memory.title.clone())
                .ok_or_else(|| format!("Dangling target memory id: {}", link.targetMemoryId))?;
            result.push(MemoryLinkInfo {
                link: link.clone(),
                sourceTitle,
                targetTitle,
            });
            if result.len() >= limit {
                break;
            }
        }
        Ok(result)
    }

    /// Finds one enriched memory link by id.
    pub fn findLinkById(&self, linkId: i64) -> Result<Option<MemoryLinkInfo>, String> {
        Ok(self
            .queryMemoryLinks(Some(linkId), None, None, None, 1)?
            .into_iter()
            .next())
    }

    /// Updates a memory link's type, weight, and description.
    pub fn updateLink(
        &self,
        linkId: i64,
        type_: String,
        weight: f32,
        description: String,
    ) -> Result<MemoryLinkInfo, String> {
        self.linkBox
            .editEntities(|links| {
                let link = links
                    .iter_mut()
                    .find(|link| link.id == linkId)
                    .ok_or_else(|| format!("Link not found with id: {linkId}"))?;
                link.type_ = type_;
                link.weight = weight.clamp(0.0, 1.0);
                link.description = description;
                Ok::<(), String>(())
            })
            .map_err(|error| error.to_string())??;
        self.findLinkById(linkId)?
            .ok_or_else(|| format!("Link not found with id: {linkId}"))
    }

    /// Deletes one memory link by id.
    pub fn deleteLink(&self, linkId: i64) -> Result<bool, String> {
        self.linkBox
            .remove(linkId)
            .map_err(|error| error.to_string())
    }

    /// Lists every folder path currently used by stored memories.
    pub fn getAllFolderPaths(&self) -> Result<Vec<String>, String> {
        let mut folders = BTreeSet::new();
        for memory in self.memoryBox.all().map_err(|error| error.to_string())? {
            if let Some(folder) = memory.folderPath {
                folders.insert(folder);
            }
        }
        Ok(folders.into_iter().collect())
    }

    /// Builds the current memory graph after removing dangling links.
    #[allow(non_snake_case)]
    pub fn getMemoryGraph(&self) -> Result<MemoryGraph, String> {
        self.cleanupDanglingLinksIfNeeded()?;
        self.buildGraphFromMemories(self.memoryBox.all().map_err(|error| error.to_string())?)
    }

    /// Removes links whose source or target memory no longer exists.
    #[allow(non_snake_case)]
    fn cleanupDanglingLinksIfNeeded(&self) -> Result<(), String> {
        let memories = self.memoryBox.all().map_err(|error| error.to_string())?;
        let memoryIds = memories
            .iter()
            .map(|memory| memory.id)
            .collect::<HashSet<_>>();
        let danglingLinkIds = self
            .linkBox
            .all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|link| {
                !memoryIds.contains(&link.sourceMemoryId)
                    || !memoryIds.contains(&link.targetMemoryId)
            })
            .map(|link| link.id)
            .collect::<Vec<_>>();
        if danglingLinkIds.is_empty() {
            return Ok(());
        }
        self.linkBox
            .removeByIds(&danglingLinkIds)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Builds graph nodes and edges from stored memories and links.
    #[allow(non_snake_case)]
    fn buildGraphFromMemories(&self, memories: Vec<Memory>) -> Result<MemoryGraph, String> {
        let memoryByObjectId = memories
            .iter()
            .map(|memory| (memory.id, memory))
            .collect::<HashMap<_, _>>();
        let memoryUuids = memories
            .iter()
            .map(|memory| memory.uuid.clone())
            .collect::<HashSet<_>>();
        let nodes = memories
            .iter()
            .map(|memory| MemoryGraphNode {
                id: memory.uuid.clone(),
                label: memory.title.clone(),
                color: memoryGraphNodeColor(memory),
                metadata: HashMap::new(),
            })
            .collect::<Vec<_>>();
        let mut edgeKeys = HashSet::new();
        let mut edges = Vec::new();
        for link in self.linkBox.all().map_err(|error| error.to_string())? {
            let Some(sourceMemory) = memoryByObjectId.get(&link.sourceMemoryId) else {
                continue;
            };
            let Some(targetMemory) = memoryByObjectId.get(&link.targetMemoryId) else {
                continue;
            };
            if !memoryUuids.contains(&sourceMemory.uuid)
                || !memoryUuids.contains(&targetMemory.uuid)
            {
                continue;
            }
            let edgeKey = (
                link.id,
                sourceMemory.uuid.clone(),
                targetMemory.uuid.clone(),
                link.type_.clone(),
            );
            if !edgeKeys.insert(edgeKey) {
                continue;
            }
            edges.push(MemoryGraphEdge {
                id: link.id,
                sourceId: sourceMemory.uuid.clone(),
                targetId: targetMemory.uuid.clone(),
                label: Some(link.type_),
                weight: link.weight,
                metadata: HashMap::new(),
                isCrossFolderLink: Self::normalizeFolderPath(sourceMemory.folderPath.as_deref())
                    != Self::normalizeFolderPath(targetMemory.folderPath.as_deref()),
            });
        }
        Ok(MemoryGraph { nodes, edges })
    }

    #[allow(non_snake_case)]
    /// Exports user memories and their internal links as a portable JSON document.
    pub fn exportMemoriesToJson(&self) -> Result<String, String> {
        let memories = self
            .memoryBox
            .all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|memory| !memory.isDocumentNode)
            .collect::<Vec<_>>();
        let memoryUuids = memories
            .iter()
            .map(|memory| memory.uuid.clone())
            .collect::<HashSet<_>>();
        let memoryUuidById = memories
            .iter()
            .map(|memory| (memory.id, memory.uuid.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let serializableMemories = memories
            .into_iter()
            .map(|memory| SerializableMemory {
                uuid: memory.uuid,
                title: memory.title,
                content: memory.content,
                contentType: memory.contentType,
                source: memory.source,
                credibility: memory.credibility,
                importance: memory.importance,
                folderPath: memory.folderPath,
                createdAt: memory.createdAt,
                updatedAt: memory.updatedAt,
                tagNames: memory.tags.into_iter().map(|tag| tag.name).collect(),
            })
            .collect::<Vec<_>>();
        let mut seenLinks = BTreeSet::new();
        let mut serializableLinks = Vec::new();
        for link in self.linkBox.all().map_err(|error| error.to_string())? {
            let Some(sourceUuid) = memoryUuidById.get(&link.sourceMemoryId) else {
                continue;
            };
            let Some(targetUuid) = memoryUuidById.get(&link.targetMemoryId) else {
                continue;
            };
            if !memoryUuids.contains(sourceUuid) || !memoryUuids.contains(targetUuid) {
                continue;
            }
            let key = (
                sourceUuid.clone(),
                targetUuid.clone(),
                link.type_.clone(),
                link.weight.to_bits(),
                link.description.clone(),
            );
            if !seenLinks.insert(key) {
                continue;
            }
            serializableLinks.push(SerializableLink {
                sourceUuid: sourceUuid.clone(),
                targetUuid: targetUuid.clone(),
                type_: link.type_,
                weight: link.weight,
                description: link.description,
            });
        }
        let exportData = MemoryExportData {
            memories: serializableMemories,
            links: serializableLinks,
            exportDate: nowMillis(),
            version: "1.0".to_string(),
        };
        serde_json::to_string_pretty(&exportData).map_err(|error| error.to_string())
    }

    #[allow(non_snake_case)]
    /// Imports memories and links from a portable JSON document.
    pub fn importMemoriesFromJson(
        &self,
        jsonString: String,
        strategy: ImportStrategy,
    ) -> Result<MemoryImportResult, String> {
        let exportData: MemoryExportData =
            serde_json::from_str(&jsonString).map_err(|error| error.to_string())?;
        let mut result = MemoryImportResult::default();
        let mut uuidMap = std::collections::HashMap::<String, Memory>::new();

        for serializableMemory in exportData.memories {
            let existingMemory = self.findMemoryByUuid(&serializableMemory.uuid)?;
            match (existingMemory, &strategy) {
                (Some(existing), ImportStrategy::SKIP) => {
                    result.skippedMemories += 1;
                    uuidMap.insert(serializableMemory.uuid, existing);
                }
                (Some(mut existing), ImportStrategy::UPDATE) => {
                    existing.title = serializableMemory.title;
                    existing.content = serializableMemory.content;
                    existing.contentType = serializableMemory.contentType;
                    existing.source = serializableMemory.source;
                    existing.credibility = serializableMemory.credibility;
                    existing.importance = serializableMemory.importance;
                    existing.folderPath =
                        Self::normalizeFolderPath(serializableMemory.folderPath.as_deref());
                    existing.updatedAt = nowMillis();
                    existing.tags = buildTags(serializableMemory.tagNames);
                    let saved = self
                        .memoryBox
                        .put(existing)
                        .map_err(|error| error.to_string())?;
                    result.updatedMemories += 1;
                    uuidMap.insert(serializableMemory.uuid, saved);
                }
                (_, _) => {
                    let sourceUuid = serializableMemory.uuid.clone();
                    let forceNewUuid = strategy == ImportStrategy::CREATE_NEW;
                    let memory =
                        self.createMemoryFromSerializable(serializableMemory, forceNewUuid)?;
                    result.newMemories += 1;
                    uuidMap.insert(sourceUuid, memory);
                }
            }
        }

        for serializableLink in exportData.links {
            let Some(sourceMemory) = uuidMap.get(&serializableLink.sourceUuid) else {
                continue;
            };
            let Some(targetMemory) = uuidMap.get(&serializableLink.targetUuid) else {
                continue;
            };
            if self.memoryLinkExists(sourceMemory.id, targetMemory.id, &serializableLink.type_)? {
                continue;
            }
            let link = MemoryLink {
                id: 0,
                sourceMemoryId: sourceMemory.id,
                targetMemoryId: targetMemory.id,
                type_: serializableLink.type_,
                weight: serializableLink.weight,
                description: serializableLink.description,
            };
            self.linkBox.put(link).map_err(|error| error.to_string())?;
            result.newLinks += 1;
        }

        Ok(result)
    }

    #[allow(non_snake_case)]
    fn findMemoryByUuid(&self, uuid: &str) -> Result<Option<Memory>, String> {
        self.memoryBox
            .query()
            .filter({
                let uuid = uuid.to_string();
                move |memory| memory.uuid == uuid
            })
            .build()
            .findFirst()
            .map_err(|error| error.to_string())
    }

    #[allow(non_snake_case)]
    fn createMemoryFromSerializable(
        &self,
        serializable: SerializableMemory,
        forceNewUuid: bool,
    ) -> Result<Memory, String> {
        let now = nowMillis();
        let memory = Memory {
            id: 0,
            uuid: if forceNewUuid {
                Uuid::new_v4().to_string()
            } else {
                serializable.uuid
            },
            title: serializable.title,
            content: serializable.content,
            contentType: serializable.contentType,
            source: serializable.source,
            credibility: serializable.credibility,
            importance: serializable.importance,
            documentPath: None,
            isDocumentNode: false,
            chunkIndexFilePath: None,
            folderPath: Self::normalizeFolderPath(serializable.folderPath.as_deref()),
            createdAt: serializable.createdAt,
            updatedAt: serializable.updatedAt,
            lastAccessedAt: now,
            tags: buildTags(serializable.tagNames),
            properties: Vec::new(),
        };
        self.memoryBox
            .put(memory)
            .map_err(|error| error.to_string())
    }

    #[allow(non_snake_case)]
    fn memoryLinkExists(
        &self,
        sourceMemoryId: i64,
        targetMemoryId: i64,
        linkType: &str,
    ) -> Result<bool, String> {
        Ok(self
            .linkBox
            .all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .any(|link| {
                link.sourceMemoryId == sourceMemoryId
                    && link.targetMemoryId == targetMemoryId
                    && link.type_ == linkType
            }))
    }
}

fn nowMillis() -> i64 {
    currentTimeMillis()
}

fn buildTags(tags: Vec<String>) -> Vec<MemoryTag> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for tag in tags {
        let name = tag.trim();
        if name.is_empty() || !seen.insert(name.to_string()) {
            continue;
        }
        result.push(MemoryTag {
            id: result.len() as i64 + 1,
            name: name.to_string(),
        });
    }
    result
}

#[allow(non_snake_case)]
fn memoryGraphNodeColor(memory: &Memory) -> i64 {
    if memory.isDocumentNode {
        return 0xFF9575CD_i64;
    }
    match memory.tags.first().map(|tag| tag.name.as_str()) {
        Some("Person") => 0xFF81C784_i64,
        Some("Concept") => 0xFF64B5F6_i64,
        _ => 0xFFD3D3D3_i64,
    }
}

fn lexicalTokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '|' | ',' | ';' | '，' | '；'))
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn lexicalScore(memory: &Memory, tokens: &[String]) -> f64 {
    if tokens.is_empty() {
        return 0.0;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}",
        memory.title,
        memory.content,
        memory.source,
        memory
            .tags
            .iter()
            .map(|tag| tag.name.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    )
    .to_lowercase();
    let mut matched = 0usize;
    for token in tokens {
        if haystack.contains(token) {
            matched += 1;
        }
    }
    matched as f64 / tokens.len() as f64
}

fn cosineSimilarity(a:&[f32],b:&[f32])->Option<f32> {
    if a.len()!=b.len() || a.is_empty() {return None;}
    let dot=a.iter().zip(b).map(|(a,b)|a*b).sum::<f32>();
    let norm=(a.iter().map(|n|n*n).sum::<f32>()*b.iter().map(|n|n*n).sum::<f32>()).sqrt();
    if norm>0.0 {Some((dot/norm).clamp(-1.0,1.0))} else {None}
}
