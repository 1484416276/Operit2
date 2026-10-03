use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::chat::enhance::MultiServiceManager::{MultiServiceManager, SharedAIServiceHandle};
use crate::chat::library::MemoryLibrary::MemoryLibrary;
use crate::runtime_support::{ProviderRuntimeContext, ProviderRuntimeSupport};
use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use operit_host_api::HostRuntimeTaskSchedulerHost;
use operit_model::FunctionType::FunctionType;
use operit_host_api::TimeUtils::currentTimeMillis;
use operit_model::MemorySettings::MemoryAutoSaveStatus;
use operit_store::repository::MemorySettingsRepository::MemorySettingsRepository;
use operit_model::MemoryAutoSaveCandidate::MemoryAutoSaveCandidate;
use operit_store::repository::MemoryAutoSaveCandidateRepository::MemoryAutoSaveCandidateRepository;
use operit_util::AppLogger::AppLogger;

const TAG: &str = "MemoryAutoSaveScheduler";
const LOOP_TICK_MS: u64 = 60_000;
const MAX_MESSAGES_PER_BATCH: usize = 48;
const MAX_CANDIDATES_PER_RUN_PER_CHAT: usize = 20;
const MIN_TOTAL_CANDIDATES_TO_EXTRACT: usize = 5;

static RUNNING: AtomicBool = AtomicBool::new(false);
static PROCESSING: AtomicBool = AtomicBool::new(false);
struct ProcessingGuard;
impl Drop for ProcessingGuard { fn drop(&mut self) { PROCESSING.store(false, Ordering::SeqCst); } }

/// Schedules owner-scoped deferred memory extraction.
pub struct MemoryAutoSaveScheduler;

impl MemoryAutoSaveScheduler {
    /// Starts one host-owned scheduler loop for the supplied runtime context.
    pub fn schedule(runtimeContext: ProviderRuntimeContext) {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        let scheduler = defaultHostRuntimeTaskSchedulerHost();
        let taskScheduler = scheduler.clone();
        scheduler
            .scheduleHostRuntimeAsyncTask(
                "operit-memory-auto-save-scheduler",
                Box::new(move || {
                    Box::pin(async move {
                        while RUNNING.load(Ordering::SeqCst) {
                            if let Err(error) = taskScheduler.waitForHostRuntimeDelay(LOOP_TICK_MS).await {
                                AppLogger::e(TAG, &format!("memory scheduler delay failed: {error}"));
                                RUNNING.store(false, Ordering::SeqCst);
                                break;
                            }
                            if RUNNING.load(Ordering::SeqCst) {
                                if let Err(error) = Self::runOnce(runtimeContext.clone()).await {
                                    AppLogger::e(TAG, &format!("记忆自动保存轮询失败: {error}"));
                                }
                            }
                        }
                    })
                }),
            )
            .expect("memory auto-save scheduler task must be scheduled");
    }

    /// Stops future scheduler passes after the current host task turn.
    pub fn cancel() {
        RUNNING.store(false, Ordering::SeqCst);
    }

    /// Processes every owner whose queue has reached the extraction threshold.
    pub async fn runOnce(runtimeContext: ProviderRuntimeContext) -> Result<(), String> {
        if PROCESSING.swap(true, Ordering::SeqCst) { return Ok(()); }
        let _guard = ProcessingGuard;
        let ownerKeys = runtimeContext.support().memoryAutoSaveOwnerKeys()?;
        let mut serviceManager = MultiServiceManager::from_runtime_context(runtimeContext.clone())
            .map_err(|error| error.to_string())?;
        let memoryService = serviceManager
            .getServiceForFunction(FunctionType::MEMORY)
            .map_err(|error| error.to_string())?;
        for ownerKey in ownerKeys {
            let settingsRepository = MemorySettingsRepository::new(&ownerKey);
            let now = currentTimeMillis();
            if now < settingsRepository.nextRunAt(now)? { continue; }
            let interval = i64::from(settingsRepository.load()?.autoSaveIntervalMinutes) * 60_000;
            let repository = MemoryAutoSaveCandidateRepository::new(&ownerKey);
            let candidates = repository.getPendingAndFailedCandidates()?;
            if candidates.len() < MIN_TOTAL_CANDIDATES_TO_EXTRACT {
                settingsRepository.scheduleNextRun(currentTimeMillis() + interval)?;
                continue;
            }
            let mut candidatesByChat = BTreeMap::<String, Vec<MemoryAutoSaveCandidate>>::new();
            for candidate in candidates {
                candidatesByChat
                    .entry(candidate.chatId.clone())
                    .or_default()
                    .push(candidate);
            }
            for (chatId, candidates) in candidatesByChat {
                // Kotlin applies the twenty-candidate cap before splitting selected/automatic sources.
                let (selectedCandidates, automaticCandidates): (Vec<_>, Vec<_>) = candidates
                    .into_iter().take(MAX_CANDIDATES_PER_RUN_PER_CHAT)
                    .partition(|candidate| candidate.isSelectedUserMessage());
                for batch in [selectedCandidates, automaticCandidates]
                    .into_iter()
                    .filter(|candidates| !candidates.is_empty())
                    .map(|candidates| {
                        candidates
                            .into_iter()
                            .take(MAX_CANDIDATES_PER_RUN_PER_CHAT)
                            .collect::<Vec<_>>()
                    })
                {
                    if let Err(error) = Self::processChatCandidates(
                        runtimeContext.clone(),
                        memoryService.clone(),
                        ownerKey.clone(),
                        chatId.clone(),
                        batch,
                        repository.clone(),
                    )
                    .await {
                        AppLogger::e(TAG, &format!("memory batch failed owner={ownerKey} chat={chatId}: {error}"));
                    }
                }
            }
            settingsRepository.scheduleNextRun(currentTimeMillis() + interval)?;
        }
        Ok(())
    }

    /// Processes one chat candidate batch and updates its durable queue state.
    async fn processChatCandidates(
        runtimeContext: ProviderRuntimeContext,
        memoryService: SharedAIServiceHandle,
        ownerKey: String,
        chatId: String,
        candidates: Vec<MemoryAutoSaveCandidate>,
        repository: MemoryAutoSaveCandidateRepository,
    ) -> Result<(), String> {
        let candidateIds = candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();
        repository.markProcessing(&candidateIds)?;
        let result: Result<(), String> = async {
        let selected = candidates
            .iter()
            .all(MemoryAutoSaveCandidate::isSelectedUserMessage);
        let messages = if selected {
            let timestamps = candidates
                .iter()
                .map(|candidate| candidate.triggerMessageTimestamp)
                .collect::<Vec<_>>();
            runtimeContext
                .support()
                .memoryAutoSaveMessagesByTimestamps(&chatId, &timestamps)?
        } else {
            let triggerTimestamp = candidates
                .iter()
                .map(|candidate| candidate.triggerMessageTimestamp)
                .max()
                .ok_or_else(|| "memory candidate batch is empty".to_string())?;
            let mut messages = runtimeContext.support().memoryAutoSaveMessagesBefore(
                &chatId,
                triggerTimestamp,
                MAX_MESSAGES_PER_BATCH,
            )?;
            messages.reverse();
            messages
        };
        let mut messages = messages;
        if selected {
            messages.retain(|m|m.sender == "user" && !m.content.trim().is_empty());
            messages.sort_by_key(|m|m.timestamp);
        }
        let conversationHistory = messages
            .iter()
            .filter(|message| message.sender == "user" || message.sender == "ai")
            .map(|message| {
                let role = if message.sender == "user" {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                };
                (role, message.content.clone())
            })
            .collect::<Vec<_>>();
        let memoryContent = if selected {
            conversationHistory
                .iter()
                .filter(|(role, _)| role == "user")
                .map(|(_, content)| content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        } else {
            conversationHistory
                .iter()
                .rev()
                .find(|(role, content)| role == "assistant" && !content.trim().is_empty())
                .map(|(_, content)| content.clone())
                .unwrap_or_default()
        };
        if !conversationHistory.iter().any(|(role,_)|role == "user") || memoryContent.trim().is_empty() {
            repository.deleteCandidates(&candidateIds)?;
            return Ok(());
        }
        MemoryLibrary::saveMemoryNowForOwner(
            conversationHistory, memoryContent, memoryService, ownerKey, runtimeContext,
        ).await?;
        repository.deleteCandidates(&candidateIds)
        }.await;
        if let Err(error) = &result { repository.markFailed(&candidateIds, error)?; }
        result
    }

    /// Same pending-count and countdown data displayed by Kotlin chat input menus.
    pub fn status(ownerKey: String) -> Result<MemoryAutoSaveStatus,String> {
        let repository = MemoryAutoSaveCandidateRepository::new(&ownerKey);
        let candidates = repository.allCandidates()?;
        let now = currentTimeMillis();
        let next = MemorySettingsRepository::new(&ownerKey).nextRunAt(now)?;
        let mut status = MemoryAutoSaveStatus { ownerKey, nextRunAtMs: next,
            minutesUntilNextRun: ((next-now).max(0)+59_999)/60_000, ..Default::default() };
        let mut chats = std::collections::BTreeSet::new();
        for candidate in candidates {
            match candidate.status.as_str() {
                "pending" | "failed" => { status.pendingCandidates+=1; chats.insert(candidate.chatId); },
                "processing" => status.processingCandidates+=1,
                _=>{},
            }
            if candidate.status == "failed" { status.failedCandidates+=1; status.lastError=candidate.lastError; }
        }
        status.pendingChats = chats.len() as i32;
        Ok(status)
    }
}
