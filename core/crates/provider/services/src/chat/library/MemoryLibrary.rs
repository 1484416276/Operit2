use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::chat::config::FunctionalPrompts::FunctionalPrompts;
use crate::chat::enhance::MultiServiceManager::SharedAIServiceHandle;
use crate::chat::llmprovider::AIService::SendMessageRequest;
use crate::runtime_support::{ProviderRuntimeContext, ProviderRuntimeSupport};
use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use operit_host_api::TimeUtils::currentTimeMillis;
use operit_model::FunctionType::FunctionType;
use operit_model::Memory::{Memory, MemoryTag};
use operit_model::PromptTurn::{toPromptTurns, PromptTurn, PromptTurnKind};
use operit_store::repository::MemoryAutoSaveCandidateRepository::MemoryAutoSaveCandidateRepository;
use operit_store::repository::MemoryRepository::MemoryRepository;
use operit_store::repository::MemorySettingsRepository::MemorySettingsRepository;
use operit_store::repository::UsageStatisticsStore::{UsageRequestSource, UsageStatisticsStore};
use operit_store::repository::UserMarkdownRepository::UserMarkdownRepository;
use operit_store::RuntimeStorageHost::defaultRuntimeStorageHost;
use operit_util::stream::Stream::Stream;
use operit_util::AppLogger::AppLogger;
use operit_util::ChatMarkupRegex::{tag_ranges, ChatMarkupRegex};
use operit_util::ChatUtils::ChatUtils;

const TAG: &str = "MemoryLibrary";

pub struct MemoryLibrary;

#[derive(Clone, Debug)]
struct ParsedLink {
    sourceTitle: String,
    targetTitle: String,
    type_: String,
    description: String,
    weight: f32,
}

#[derive(Clone, Debug)]
struct ParsedEntity {
    title: String,
    content: String,
    tags: Vec<String>,
    aliasFor: Option<String>,
    folderPath: Option<String>,
}

#[derive(Clone, Debug)]
struct ParsedUpdate {
    titleToUpdate: String,
    newContent: String,
    reason: String,
    newCredibility: Option<f32>,
    newImportance: Option<f32>,
}

#[derive(Clone, Debug)]
struct ParsedMerge {
    sourceTitles: Vec<String>,
    newTitle: String,
    newContent: String,
    newTags: Vec<String>,
    folderPath: String,
    reason: String,
}

#[derive(Clone, Debug)]
struct ParsedAnalysis {
    mainProblem: Option<ParsedEntity>,
    extractedEntities: Vec<ParsedEntity>,
    links: Vec<ParsedLink>,
    updatedEntities: Vec<ParsedUpdate>,
    mergedEntities: Vec<ParsedMerge>,
    userPreferences: String,
}

impl ParsedAnalysis {
    fn empty() -> Self {
        Self {
            mainProblem: None,
            extractedEntities: Vec::new(),
            links: Vec::new(),
            updatedEntities: Vec::new(),
            mergedEntities: Vec::new(),
            userPreferences: String::new(),
        }
    }
}

impl MemoryLibrary {
    /// Enqueues one finalized reply for owner-scoped background extraction.
    #[allow(non_snake_case)]
    pub fn enqueueAutoSaveCandidate(
        ownerKey: String,
        chatId: String,
        triggerMessageTimestamp: i64,
    ) -> Result<(), String> {
        MemoryAutoSaveCandidateRepository::new(&ownerKey).enqueue(chatId, triggerMessageTimestamp)
    }

    /// Starts memory persistence with an explicit provider runtime context.
    #[allow(non_snake_case)]
    pub fn saveMemoryAsync(
        conversationHistory: Vec<(String, String)>,
        content: String,
        aiService: SharedAIServiceHandle,
        characterCardId: Option<String>,
        runtimeContext: ProviderRuntimeContext,
    ) {
        defaultHostRuntimeTaskSchedulerHost()
            .scheduleHostRuntimeAsyncTask(
                "operit-memory-persistence",
                Box::new(move || {
                    Box::pin(async move {
                        let result = Self::saveMemoryNow(
                            conversationHistory,
                            content,
                            aiService,
                            characterCardId,
                            runtimeContext,
                        )
                        .await;
                        if let Err(error) = result {
                            AppLogger::e(TAG, &format!("保存记忆失败: {error}"));
                        }
                    })
                }),
            )
            .expect("memory persistence task must be scheduled");
    }

    /// Persists memory immediately with an explicit provider runtime context.
    #[allow(non_snake_case)]
    pub async fn saveMemoryNow(
        conversationHistory: Vec<(String, String)>,
        content: String,
        aiService: SharedAIServiceHandle,
        characterCardId: Option<String>,
        runtimeContext: ProviderRuntimeContext,
    ) -> Result<(), String> {
        Self::saveMemory(
            conversationHistory,
            content,
            aiService,
            characterCardId,
            runtimeContext,
        )
        .await
    }

    /// Persists memory immediately in an explicitly addressed owner namespace.
    #[allow(non_snake_case)]
    pub async fn saveMemoryNowForOwner(
        conversationHistory: Vec<(String, String)>,
        content: String,
        aiService: SharedAIServiceHandle,
        ownerKey: String,
        runtimeContext: ProviderRuntimeContext,
    ) -> Result<(), String> {
        let mutex = memoryMutex();
        let _guard = mutex.lock().await;
        Self::saveMemoryForOwner(
            conversationHistory,
            content,
            aiService,
            ownerKey,
            runtimeContext,
            10,
        )
        .await
    }

    /// Rebuild uses the complete planned window instead of the default ten-message tail.
    pub async fn saveMemoryWindowNowForOwner(
        conversationHistory: Vec<(String, String)>, content: String,
        aiService: SharedAIServiceHandle, ownerKey: String,
        runtimeContext: ProviderRuntimeContext, analysisHistoryLimit: usize,
    ) -> Result<(), String> {
        if analysisHistoryLimit == 0 { return Err("Analysis history limit must be positive".into()); }
        let _guard = memoryMutex().lock().await;
        Self::saveMemoryForOwner(conversationHistory, content, aiService, ownerKey, runtimeContext, analysisHistoryLimit).await
    }

    /// Original Kotlin batch-of-ten categorization, preserving owner bindings and serial writes.
    pub async fn autoCategorizeForOwner(ownerKey:String,aiService:SharedAIServiceHandle,runtimeContext:ProviderRuntimeContext)->Result<i32,String> {
        let _guard=memoryMutex().lock().await;
        let repository=MemoryRepository::new(&ownerKey);
        let memories=repository.searchMemories("",None,0.0,None,None)?.into_iter().filter(|m|m.folderPath.as_deref().unwrap_or("").is_empty()).collect::<Vec<_>>();
        let folders=repository.getAllFolderPaths()?;let mut changed=0;let mut failures=Vec::new();
        for batch in memories.chunks(10) {
            let digest=batch.iter().map(|m|format!("- title: {}, content: {}...",m.title,m.content.chars().take(100).collect::<String>())).collect::<Vec<_>>().join("\n");
            let prompt=FunctionalPrompts::buildMemoryAutoCategorizePrompt(&folders,&digest,false);
            let result:Result<String,String>=async {
                let mut service=aiService.lock().await;let mut output=String::new();
                let mut stream=service.send_message(SendMessageRequest {
                    chat_history:toPromptTurns(&[("system".into(),prompt),("user".into(),"请为这些记忆分类。".into())]),
                    model_parameters:Vec::new(),enable_thinking:false,thinking_quality_level:1,thinking_configurations:"[]".into(),thinking_option_id:String::new(),
                    stream:true,available_tools:Vec::new(),preserve_think_in_history:false,enable_retry:true,on_non_fatal_error:None,on_tool_invocation:None,
                }).await.map_err(|e|e.to_string())?;
                stream.collect(&mut |chunk|output.push_str(&chunk)).await;
                runtimeContext.support().updateTokensForProviderModel(&service.provider_model(),service.input_token_count(),service.output_token_count(),service.cached_input_token_count())?;
                Ok(output)
            }.await;
            match result.and_then(|output|serde_json::from_str::<Vec<Value>>(&ChatUtils::extract_json_array(&output)).map_err(|e|e.to_string())) {
                Ok(rows)=>for row in rows {
                    let (Some(title),Some(folder))=(row.get("title").and_then(Value::as_str),row.get("folder").and_then(Value::as_str)) else {continue;};
                    if let Some(memory)=batch.iter().find(|m|m.title==title) {
                        repository.moveMemoriesToFolder(&[memory.id],folder)?;changed+=1;
                    }
                },
                Err(error)=> {AppLogger::e(TAG,&format!("memory categorization batch failed: {error}"));failures.push(error);},
            }
        }
        if !failures.is_empty() {return Err(format!("Categorized {changed} memories; {} batches failed: {}",failures.len(),failures.join("; ")));}
        Ok(changed)
    }

    /// Resolves the character card owner and persists its extracted memory.
    #[allow(non_snake_case)]
    async fn saveMemory(
        conversationHistory: Vec<(String, String)>,
        content: String,
        aiService: SharedAIServiceHandle,
        characterCardId: Option<String>,
        runtimeContext: ProviderRuntimeContext,
    ) -> Result<(), String> {
        let mutex = memoryMutex();
        let _guard = mutex.lock().await;
        let characterCardId = characterCardId
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "characterCardId is required for memory auto update".to_string())?;
        let ownerKey = runtimeContext
            .support()
            .memoryOwnerKeyForCharacterCard(&characterCardId)?;
        Self::saveMemoryForOwner(
            conversationHistory,
            content,
            aiService,
            ownerKey,
            runtimeContext,
            10,
        )
        .await
    }

    /// Performs the analysis and writes graph memory plus USER.md for one owner.
    #[allow(non_snake_case)]
    async fn saveMemoryForOwner(
        conversationHistory: Vec<(String, String)>,
        content: String,
        aiService: SharedAIServiceHandle,
        ownerKey: String,
        runtimeContext: ProviderRuntimeContext,
        analysisHistoryLimit: usize,
    ) -> Result<(), String> {
        let memoryRepository = MemoryRepository::new(ownerKey.clone());
        let prunedContent =
            ChatUtils::remove_thinking_content(&ChatUtils::strip_gemini_thought_signature_meta(&pruneToolResultContent(&content)));

        let processedHistory = conversationHistory
            .into_iter()
            .filter(|(role, _)| role != "system")
            .map(|(role, msgContent)| {
                let cleanedContent = if role == "user" {
                    removeMemoryTags(&msgContent).trim().to_string()
                } else {
                    msgContent
                };
                (
                    role,
                    ChatUtils::remove_thinking_content(&ChatUtils::strip_gemini_thought_signature_meta(&pruneToolResultContent(
                        &cleanedContent,
                    ))),
                )
            })
            .collect::<Vec<_>>();

        if processedHistory.is_empty() {
            return Ok(());
        }
        let Some((_, query)) = processedHistory
            .iter()
            .rev()
            .find(|(role, _)| role == "user")
        else {
            return Ok(());
        };
        if query.is_empty() {
            return Ok(());
        }

        let analysis = Self::generateAnalysis(
            aiService,
            query,
            &prunedContent,
            &processedHistory,
            &memoryRepository,
            &ownerKey,
            &runtimeContext,
            analysisHistoryLimit,
        )
        .await?;

        if analysis.mainProblem.is_none()
            && analysis.extractedEntities.is_empty()
            && analysis.updatedEntities.is_empty()
            && analysis.mergedEntities.is_empty()
            && analysis.links.is_empty()
            && analysis.userPreferences.is_empty()
        {
            return Ok(());
        }

        let mut createdMemories = HashMap::<String, Memory>::new();

        for merge in &analysis.mergedEntities {
            let _ = &merge.reason;
            if let Some(mergedMemory) = memoryRepository.mergeMemories(
                merge.sourceTitles.clone(),
                merge.newTitle.clone(),
                merge.newContent.clone(),
                merge.newTags.clone(),
                merge.folderPath.clone(),
            )? {
                createdMemories.insert(mergedMemory.title.clone(), mergedMemory);
            }
        }

        for update in &analysis.updatedEntities {
            let _ = &update.reason;
            if let Some(memoryToUpdate) =
                memoryRepository.findMemoryByTitle(&update.titleToUpdate)?
            {
                let updatedMemory = memoryRepository.updateMemory(
                    memoryToUpdate.id,
                    memoryToUpdate.title,
                    update.newContent.clone(),
                    memoryToUpdate.contentType,
                    memoryToUpdate.source,
                    update.newCredibility.unwrap_or(memoryToUpdate.credibility),
                    update.newImportance.unwrap_or(memoryToUpdate.importance),
                    memoryToUpdate.folderPath,
                    Some(
                        memoryToUpdate
                            .tags
                            .into_iter()
                            .map(|tag| tag.name)
                            .collect(),
                    ),
                )?;
                createdMemories.insert(updatedMemory.title.clone(), updatedMemory);
            }
        }

        if !analysis.userPreferences.is_empty() {
            let settings = MemorySettingsRepository::new(&ownerKey).load()?;
            if settings.profileAutoUpdateEnabled && !settings.profileAutoUpdateLocked {
                UserMarkdownRepository::new(&ownerKey, defaultRuntimeStorageHost())
                    .writeUserMarkdown(analysis.userPreferences.clone())?;
            }
        }

        if let Some(mainProblem) = analysis.mainProblem.clone() {
        let mainProblemMemory = if let Some(mut existingMemory) =
            memoryRepository.findMemoryByTitle(&mainProblem.title)?
        {
            existingMemory.content = mainProblem.content.clone();
            memoryRepository.saveMemory(existingMemory)?
        } else {
            let mut memory = newMemory(
                mainProblem.title.clone(),
                mainProblem.content.clone(),
                "memory_analysis".to_string(),
                mainProblem.folderPath.clone(),
                1.0,
                0.8,
            );
            memory.tags = buildTags(mainProblem.tags.clone());
            memoryRepository.saveMemory(memory)?
        };
        createdMemories.insert(mainProblemMemory.title.clone(), mainProblemMemory);
        }

        for entity in &analysis.extractedEntities {
            let mut memory = None;
            if let Some(aliasFor) = entity
                .aliasFor
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                memory = createdMemories
                    .get(aliasFor)
                    .cloned()
                    .or(memoryRepository.findMemoryByTitle(aliasFor)?);
            }
            if memory.is_none() {
                let mut created = newMemory(
                    entity.title.clone(),
                    entity.content.clone(),
                    "memory_analysis".to_string(),
                    entity
                        .folderPath
                        .clone(),
                    0.5,
                    0.5,
                );
                created.tags = buildTags(entity.tags.clone());
                memory = Some(memoryRepository.saveMemory(created)?);
            }
            if let Some(memory) = memory {
                createdMemories.insert(entity.title.clone(), memory);
            }
        }

        for link in &analysis.links {
            let source = match createdMemories.get(&link.sourceTitle).cloned() {
                Some(memory) => Some(memory),
                None => memoryRepository.findMemoryByTitle(&link.sourceTitle)?,
            };
            let target = match createdMemories.get(&link.targetTitle).cloned() {
                Some(memory) => Some(memory),
                None => memoryRepository.findMemoryByTitle(&link.targetTitle)?,
            };
            if let (Some(source), Some(target)) = (source, target) {
                memoryRepository.linkMemories(
                    source.id,
                    target.id,
                    link.type_.clone(),
                    link.weight,
                    link.description.clone(),
                )?;
            }
        }

        Ok(())
    }

    #[allow(non_snake_case)]
    async fn generateAnalysis(
        aiService: SharedAIServiceHandle,
        query: &str,
        solution: &str,
        conversationHistory: &[(String, String)],
        memoryRepository: &MemoryRepository,
        ownerKey: &str,
        runtimeContext: &ProviderRuntimeContext,
        analysisHistoryLimit: usize,
    ) -> Result<ParsedAnalysis, String> {
        let useEnglish = false;
        let settings = MemorySettingsRepository::new(ownerKey).load()?;
        let profileUpdateEnabled = settings.profileAutoUpdateEnabled && !settings.profileAutoUpdateLocked;
        let currentPreferences = if profileUpdateEnabled {
            UserMarkdownRepository::new(ownerKey, defaultRuntimeStorageHost()).readUserMarkdown()?
        } else { String::new() };
        let contextQuery = buildCandidateSearchQuery(query, solution, conversationHistory);
        let searchConfig = runtimeContext.support().memorySearchConfig(ownerKey)?;
        let candidateMemories = memoryRepository
            .searchMemoriesWithConfig(&contextQuery, None, 0.0, None, None, searchConfig.clone())?
            .into_iter()
            .take(15)
            .collect::<Vec<_>>();
        let duplicatesPromptPart =
            findAndDescribeDuplicates(&candidateMemories, memoryRepository, useEnglish)?;
        let existingMemoriesPrompt = if candidateMemories.is_empty() {
            FunctionalPrompts::knowledgeGraphNoExistingMemoriesMessage(useEnglish).to_string()
        } else {
            format!(
                "{}{}",
                FunctionalPrompts::knowledgeGraphExistingMemoriesPrefix(useEnglish),
                candidateMemories
                    .iter()
                    .map(|memory| {
                        format!(
                            "- \"{}\": {}...",
                            memory.title,
                            memory
                                .content
                                .replace('\n', " ")
                                .chars()
                                .take(150)
                                .collect::<String>()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        let existingFoldersPrompt = FunctionalPrompts::knowledgeGraphExistingFoldersPrompt(
            &memoryRepository.getAllFolderPaths()?,
            useEnglish,
        );
        let systemPrompt = FunctionalPrompts::buildKnowledgeGraphExtractionPrompt(
            &duplicatesPromptPart,
            &existingMemoriesPrompt,
            &existingFoldersPrompt,
            &currentPreferences,
            useEnglish,
            profileUpdateEnabled,
            &settings.memoryExtractionCustomRules,
        );
        let analysisMessage =
            buildAnalysisMessage(query, solution, conversationHistory, useEnglish, analysisHistoryLimit);
        let messages = toPromptTurns(&[
            ("system".to_string(), systemPrompt),
            ("user".to_string(), analysisMessage),
        ]);
        let mut result = String::new();
        let (providerModel, inputTokens, cachedInputTokens, outputTokens) = {
            let mut service = aiService.lock().await;
            let providerModel = service.provider_model();
            let mut stream = service
                .send_message(SendMessageRequest {
                    chat_history: messages,
                    model_parameters: Vec::new(),
                    enable_thinking: false,
                    thinking_quality_level: 1,
                    thinking_configurations: "[]".to_string(),
                    thinking_option_id: String::new(),
                    stream: true,
                    available_tools: Vec::new(),
                    preserve_think_in_history: false,
                    enable_retry: true,
                    on_non_fatal_error: None,
                    on_tool_invocation: None,
                })
                .await
                .map_err(|error| error.to_string())?;
            stream
                .collect(&mut |chunk| {
                    result.push_str(&chunk);
                })
                .await;
            (
                providerModel,
                service.input_token_count(),
                service.cached_input_token_count(),
                service.output_token_count(),
            )
        };
        runtimeContext
            .support()
            .updateTokensForProviderModel(
                &providerModel,
                inputTokens,
                outputTokens,
                cachedInputTokens,
            )
            .map_err(|error| error.to_string())?;
        UsageStatisticsStore::new()
            .recordProviderModelRequest(
                providerModel,
                FunctionType::MEMORY,
                UsageRequestSource::MEMORY_ANALYSIS,
                None,
                inputTokens,
                outputTokens,
                cachedInputTokens,
            )
            .map_err(|error| error.to_string())?;
        let _ = searchConfig;
        parseAnalysisResult(&ChatUtils::remove_thinking_content(&result), useEnglish)
    }
}

#[allow(non_snake_case)]
pub fn promptTurnsToMemoryPairs(turns: &[PromptTurn]) -> Vec<(String, String)> {
    turns
        .iter()
        .map(|turn| (turn.role().to_string(), turn.content.clone()))
        .collect()
}

fn memoryMutex() -> &'static tokio::sync::Mutex<()> {
    static MEMORY_MUTEX: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    MEMORY_MUTEX.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[allow(non_snake_case)]
fn buildCandidateSearchQuery(query: &str, solution: &str, history:&[(String,String)]) -> String {
    let coreQuestion=extractCoreQuestionText(query);
    let selectedQuestion=if coreQuestion.trim().is_empty() {normalizeCandidateSearchText(query,800)} else {coreQuestion};
    let conciseSolution=normalizeCandidateSearchText(solution,180);
    let recent=history.iter().skip(history.len().saturating_sub(12)).map(|(_,c)|c.as_str()).collect::<Vec<_>>().join("\n");
    [selectedQuestion,conciseSolution,normalizeCandidateSearchText(&recent,1200)].into_iter().filter(|c|!c.trim().is_empty()).collect::<Vec<_>>().join("\n")
}

#[allow(non_snake_case)]
fn extractCoreQuestionText(rawQuery: &str) -> String {
    let compact = rawQuery.replace("\r\n", "\n");
    let cn = Regex::new(r"(?s)问题\s*[：:]\s*(.+?)(?:\n\s*解决方案\s*[：:]|\z)")
        .expect("memory regex must compile")
        .captures(&compact)
        .and_then(|captures| {
            captures
                .get(1)
                .map(|value| value.as_str().trim().to_string())
        });
    let en = Regex::new(r"(?s)Question\s*:\s*(.+?)(?:\n\s*Solution\s*:|\z)")
        .expect("memory regex must compile")
        .captures(&compact)
        .and_then(|captures| {
            captures
                .get(1)
                .map(|value| value.as_str().trim().to_string())
        });
    let selected = cn.or(en).unwrap_or(compact);
    let filtered = selected
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("历史记录:") && !trimmed.starts_with("History:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    normalizeCandidateSearchText(&filtered, 500)
}

#[allow(non_snake_case)]
fn normalizeCandidateSearchText(raw: &str, maxLen: usize) -> String {
    let mut text = raw.to_string();
    for pattern in [
        r"(?is)<tool(?:_[A-Za-z0-9_]+)?\b[^>]*>.*?</tool(?:_[A-Za-z0-9_]+)?>",
        r"(?is)<tool(?:_[A-Za-z0-9_]+)?\b[^>]*/>",
        r"(?is)<tool_result(?:_[A-Za-z0-9_]+)?\b[^>]*>.*?</tool_result(?:_[A-Za-z0-9_]+)?>",
        r"(?is)<tool_result(?:_[A-Za-z0-9_]+)?\b[^>]*/>",
        r"(?is)<status\b[^>]*>.*?</status>",
        r"(?is)<status\b[^>]*/>",
        r"(?is)<think(?:ing)?\b[^>]*>.*?</think(?:ing)?>",
        r"(?is)<think(?:ing)?\b[^>]*/>",
        r"(?is)<search\b[^>]*>.*?</search>",
        r"(?is)<search\b[^>]*/>",
        r"https?://\S+",
        r"[`*_#>]+",
        r"\s+",
    ] {
        text = Regex::new(pattern)
            .expect("memory cleanup regex must compile")
            .replace_all(&text, " ")
            .to_string();
    }
    text.trim().chars().take(maxLen).collect()
}

#[allow(non_snake_case)]
fn findAndDescribeDuplicates(
    candidateMemories: &[Memory],
    memoryRepository: &MemoryRepository,
    useEnglish: bool,
) -> Result<String, String> {
    let mut titles = candidateMemories
        .iter()
        .map(|memory| memory.title.clone())
        .collect::<Vec<_>>();
    titles.sort();
    titles.dedup();
    let mut duplicatesFound = Vec::new();
    for title in titles {
        let memoriesWithSameTitle = memoryRepository.findMemoriesByTitle(&title)?;
        if memoriesWithSameTitle.len() > 1 {
            duplicatesFound.push(FunctionalPrompts::knowledgeGraphDuplicateTitleInstruction(
                &title,
                memoriesWithSameTitle.len(),
                useEnglish,
            ));
        }
    }
    if duplicatesFound.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!(
            "{}{}\n",
            FunctionalPrompts::knowledgeGraphDuplicateHeader(useEnglish),
            duplicatesFound.join("\n")
        ))
    }
}

#[allow(non_snake_case)]
fn buildAnalysisMessage(
    query: &str,
    solution: &str,
    conversationHistory: &[(String, String)],
    useEnglish: bool,
    historyLimit: usize,
) -> String {
    let mut message = String::new();
    if useEnglish {
        message.push_str("Question:\n");
        message.push_str(query);
        message.push_str("\n\nSolution:\n");
        message.push_str(&solution.chars().take(3000).collect::<String>());
        message.push_str("\n\n");
    } else {
        message.push_str("问题：\n");
        message.push_str(query);
        message.push_str("\n\n解决方案：\n");
        message.push_str(&solution.chars().take(3000).collect::<String>());
        message.push_str("\n\n");
    }
    let recentHistory = conversationHistory
        .iter()
        .rev()
        .take(historyLimit)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    if !recentHistory.is_empty() {
        message.push_str(if useEnglish {
            "History:\n"
        } else {
            "历史记录：\n"
        });
        for (index, (role, content)) in recentHistory.iter().enumerate() {
            message.push_str(&format!(
                "#{} {}: {}\n",
                index + 1,
                role,
                content.chars().take(4000).collect::<String>()
            ));
        }
    }
    message
}

/// Strict object-based protocol. Invalid model output is a failed extraction, not a successful empty one.
#[allow(non_snake_case)]
fn parseAnalysisResult(jsonString: &str, _useEnglish: bool) -> Result<ParsedAnalysis, String> {
    let clean = ChatUtils::extract_json(jsonString);
    let json: Value = serde_json::from_str(&clean).map_err(|e|format!("Invalid memory analysis JSON: {e}"))?;
    let object = json.as_object().ok_or("Memory analysis must return a JSON object")?;
    if object.is_empty() { return Ok(ParsedAnalysis::empty()); }
    let main = object.get("main").ok_or("main is required")?;
    let mainProblem = if main.is_null() { None } else { Some(parseEntity(main, false)?) };
    let array = |key: &str| -> Result<&Vec<Value>, String> {
        object.get(key).and_then(Value::as_array).ok_or_else(||format!("{key} must be an object array"))
    };
    Ok(ParsedAnalysis {
        mainProblem,
        extractedEntities: array("new")?.iter().map(|v|parseEntity(v, true)).collect::<Result<_,_>>()?,
        updatedEntities: array("update")?.iter().map(|v|Ok(ParsedUpdate {
            titleToUpdate: requiredString(v,"title")?, newContent: requiredString(v,"content")?,
            reason: requiredString(v,"reason")?, newCredibility: unitFloat(v,"credibility",false)?,
            newImportance: unitFloat(v,"importance",false)?,
        })).collect::<Result<_,String>>()?,
        mergedEntities: array("merge")?.iter().map(|v|Ok(ParsedMerge {
            sourceTitles: requiredStrings(v,"source_titles")?, newTitle: requiredString(v,"title")?,
            newContent: requiredString(v,"content")?, newTags: requiredStrings(v,"tags")?,
            folderPath: requiredString(v,"folder_path")?, reason: requiredString(v,"reason")?,
        })).collect::<Result<_,String>>()?,
        links: array("links")?.iter().map(|v|Ok(ParsedLink {
            sourceTitle: requiredString(v,"source")?, targetTitle: requiredString(v,"target")?,
            type_: requiredString(v,"type")?, description: requiredString(v,"description")?,
            weight: unitFloat(v,"weight",true)?.ok_or("weight is required")?,
        })).collect::<Result<_,String>>()?,
        userPreferences: match object.get("profile_markdown") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.trim().to_string(),
            _ => return Err("profile_markdown must be a string or null".into()),
        },
    })
}
fn requiredString(value: &Value, key: &str) -> Result<String,String> {
    value.as_object().and_then(|v|v.get(key)).and_then(Value::as_str)
        .map(ToString::to_string).ok_or_else(||format!("{key} must be a string in a named object"))
}
fn requiredStrings(value: &Value, key: &str) -> Result<Vec<String>,String> {
    value.get(key).and_then(Value::as_array).ok_or_else(||format!("{key} must be a string array"))?
        .iter().map(|v|v.as_str().map(ToString::to_string).ok_or_else(||format!("{key} contains a non-string"))).collect()
}
fn unitFloat(value: &Value, key: &str, required: bool) -> Result<Option<f32>,String> {
    match value.get(key) {
        None | Some(Value::Null) if !required => Ok(None),
        Some(v) => { let n = v.as_f64().ok_or_else(||format!("{key} must be a number"))?;
            if !n.is_finite() || !(0.0..=1.0).contains(&n) { return Err(format!("{key} must be between 0 and 1")); }
            Ok(Some(n as f32)) },
        None => Err(format!("{key} is required")),
    }
}
fn parseEntity(value: &Value, aliasAllowed: bool) -> Result<ParsedEntity,String> {
    Ok(ParsedEntity { title: requiredString(value,"title")?, content: requiredString(value,"content")?,
        tags: requiredStrings(value,"tags")?, folderPath: Some(requiredString(value,"folder_path")?),
        aliasFor: if aliasAllowed { match value.get("alias_for") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            _ => return Err("alias_for must be a string or null".into()),
        } } else { None },
    })
}

#[allow(non_snake_case)]
fn pruneToolResultContent(message: &str) -> String {
    let blocks = ChatMarkupRegex::tool_result_blocks(message);
    if blocks.is_empty() {
        return message.to_string();
    }
    let mut output = String::new();
    let mut cursor = 0;
    for block in blocks {
        output.push_str(&message[cursor..block.start]);
        let openEnd = block
            .raw
            .find('>')
            .map(|index| index + 1)
            .unwrap_or(block.raw.len());
        output.push_str(&block.raw[..openEnd]);
        output.push_str("[工具结果已省略]");
        output.push_str(&format!("</{}>", block.tag_name));
        cursor = block.end;
    }
    output.push_str(&message[cursor..]);
    output
}

#[allow(non_snake_case)]
fn removeMemoryTags(message: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    for (start, end) in tag_ranges(message, "memory") {
        output.push_str(&message[cursor..start]);
        cursor = end;
    }
    output.push_str(&message[cursor..]);
    output
}

#[allow(non_snake_case)]
fn newMemory(
    title: String,
    content: String,
    source: String,
    folderPath: Option<String>,
    credibility: f32,
    importance: f32,
) -> Memory {
    let now = currentTimeMillis();
    Memory {
        id: 0,
        uuid: uuid::Uuid::new_v4().to_string(),
        title,
        content,
        contentType: "text".to_string(),
        source,
        credibility,
        importance,
        documentPath: None,
        isDocumentNode: false,
        chunkIndexFilePath: None,
        folderPath,
        createdAt: now,
        updatedAt: now,
        lastAccessedAt: now,
        tags: Vec::new(),
        properties: Vec::new(),
    }
}

#[allow(non_snake_case)]
fn buildTags(tags: Vec<String>) -> Vec<MemoryTag> {
    let mut result = Vec::new();
    for tag in tags {
        let name = tag.trim();
        if name.is_empty()
            || result
                .iter()
                .any(|existing: &MemoryTag| existing.name == name)
        {
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
fn stringArray(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod protocolTests {
    use super::*;
    fn blank()->Value {serde_json::json!({"main":null,"new":[],"update":[],"merge":[],"links":[]})}
    #[test]
    fn object_protocol_accepts_operations_without_main() {
        let json=serde_json::json!({"main":null,"new":[{"title":"TMUX state","content":"Verified close works","tags":["tmux"],"folder_path":"Tools","alias_for":null}],
            "update":[{"title":"SSH","content":"Timeout resolved","reason":"Verified","credibility":1.0,"importance":0.6}],
            "merge":[{"source_titles":["old A","old B"],"title":"state","content":"Merged","tags":[],"folder_path":"Tools","reason":"Same fact"}],
            "links":[{"source":"TMUX state","target":"SSH","type":"INVOLVES","description":"Uses tool","weight":0.8}],"profile_markdown":"# User\nPrefers Chinese"});
        let analysis=parseAnalysisResult(&json.to_string(),false).unwrap();
        assert!(analysis.mainProblem.is_none());assert_eq!(analysis.extractedEntities.len(),1);
        assert_eq!(analysis.updatedEntities[0].titleToUpdate,"SSH");assert_eq!(analysis.mergedEntities[0].sourceTitles.len(),2);
        assert_eq!(analysis.links[0].weight,0.8);assert!(analysis.userPreferences.starts_with("# User"));
    }
    #[test]
    fn rejects_positional_missing_wrong_type_and_out_of_range_fields() {
        let mut positional=blank();positional["new"]=serde_json::json!([["title","content",[],"folder",null]]);
        let mut missing=blank();missing.as_object_mut().unwrap().remove("links");
        let mut wrong=blank();wrong["profile_markdown"]=serde_json::json!([]);
        let mut range=blank();range["links"]=serde_json::json!([{"source":"a","target":"b","type":"REL","description":"x","weight":1.1}]);
        for json in [positional,missing,wrong,range] {assert!(parseAnalysisResult(&json.to_string(),false).is_err());}
        for raw in ["not json", "[]", "null", "{\"main\":null}"] {assert!(parseAnalysisResult(raw,false).is_err(),"{raw}");}
    }
    #[test] fn explicit_empty_response_and_markdown_fences_are_supported() {
        assert!(parseAnalysisResult("{}",false).unwrap().mainProblem.is_none());
        assert!(parseAnalysisResult(&format!("```json\n{}\n```",blank()),false).is_ok());
    }
    #[test] fn candidate_query_keeps_subject_of_short_final_question() {
        let history=vec![("user".into(),"Verify SSH timeout was resolved".into()),("assistant".into(),"SSH is now healthy".into())];
        let query=buildCandidateSearchQuery("try ls","works",&history);
        assert!(query.contains("SSH timeout"));assert!(query.starts_with("try ls"));
    }
    #[test] fn extraction_prompt_has_no_kotlin_interpolation_or_unexpected_profile_updates() {
        for english in [true,false] {for enabled in [true,false] {
            let prompt=FunctionalPrompts::buildKnowledgeGraphExtractionPrompt("duplicates","existing","folders","# Profile",english,enabled,"remember SSH");
            assert!(!prompt.contains('$'));assert!(prompt.contains("remember SSH"));
            assert_eq!(prompt.contains("<user_profile_document>"),enabled);
        }}
    }
}
