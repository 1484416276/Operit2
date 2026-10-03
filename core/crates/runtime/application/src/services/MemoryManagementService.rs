//! Owner-scoped Kotlin memory controls, exposed to Flutter without changing card bindings.
use crate::data::preferences::CharacterCardManager::CharacterCardManager;
use crate::services::core::ChatMemoryOwnerResolver::resolveMemoryOwner;
use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use operit_model::FunctionType::FunctionType;
use operit_model::MemorySettings::{MemoryAutoSaveStatus, MemoryRebuildProgress, MemorySettings};
use operit_providers::chat::enhance::MultiServiceManager::MultiServiceManager;
use operit_providers::chat::library::{
    ChatMemoryWindowPlanner::planWindows, MemoryAutoSaveScheduler::MemoryAutoSaveScheduler,
    MemoryLibrary::MemoryLibrary,
};
use operit_providers::runtime_support::{ProviderMemoryAutoSaveMessage, ProviderRuntimeContext};
use operit_store::repository::{
    ChatHistoryManager::ChatHistoryManager, MemorySettingsRepository::MemorySettingsRepository,
};
use operit_store::RuntimeStorePaths::RuntimeStorePaths;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone)]
pub struct MemoryManagementService {
    ownerKey: String,
    runtime: ProviderRuntimeContext,
}
impl MemoryManagementService {
    pub fn new(ownerKey: String, runtime: ProviderRuntimeContext) -> Self {
        Self { ownerKey, runtime }
    }
    fn assertOwner(&self) -> Result<(), String> {
        if !self
            .runtime
            .support()
            .memoryAutoSaveOwnerKeys()?
            .contains(&self.ownerKey)
        {
            return Err("memory owner does not exist".into());
        }
        Ok(())
    }
    pub fn loadSettings(&self) -> Result<MemorySettings, String> {
        self.assertOwner()?;
        MemorySettingsRepository::new(&self.ownerKey).load()
    }
    pub fn saveSettings(&self, settings: MemorySettings) -> Result<(), String> {
        self.assertOwner()?;
        MemorySettingsRepository::new(&self.ownerKey).save(settings)
    }
    pub fn autoSaveStatus(&self) -> Result<MemoryAutoSaveStatus, String> {
        self.assertOwner()?;
        MemoryAutoSaveScheduler::status(self.ownerKey.clone())
    }
    pub fn loadSearchConfig(
        &self,
    ) -> Result<operit_model::MemorySearchConfig::MemorySearchConfig, String> {
        self.assertOwner()?;
        MemorySettingsRepository::new(&self.ownerKey).loadSearchConfig()
    }
    pub fn saveSearchConfig(
        &self,
        config: operit_model::MemorySearchConfig::MemorySearchConfig,
    ) -> Result<(), String> {
        self.assertOwner()?;
        MemorySettingsRepository::new(&self.ownerKey).saveSearchConfig(config)
    }
    pub fn boundChats(&self) -> Result<Vec<operit_model::ChatHistory::ChatHistory>, String> {
        self.assertOwner()?;
        let manager = ChatHistoryManager::getInstance(RuntimeStorePaths::default())
            .map_err(|e| e.to_string())?;
        Ok(manager
            .loadChatHistories()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|chat| {
                resolveMemoryOwner(&self.runtime, &chat, &CharacterCardManager::getInstance())
                    .ok()
                    .as_deref()
                    == Some(&self.ownerKey)
            })
            .collect())
    }
    /// Manual extraction deliberately bypasses the five-candidate automatic threshold.
    pub async fn updateChatMemory(&self, chatId: String) -> Result<(), String> {
        self.assertOwner()?;
        let manager = ChatHistoryManager::getInstance(RuntimeStorePaths::default())
            .map_err(|e| e.to_string())?;
        let chat = manager
            .loadChatHistory(chatId.clone())
            .map_err(|e| e.to_string())?
            .ok_or("chat not found")?;
        if resolveMemoryOwner(&self.runtime, &chat, &CharacterCardManager::getInstance())?
            != self.ownerKey
        {
            return Err("chat is bound to a different memory owner".into());
        }
        let history = manager
            .loadChatMessages(&chatId)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|m| m.sender == "user" || m.sender == "ai")
            .map(|m| {
                let role = if m.sender == "user" {
                    "user"
                } else {
                    "assistant"
                };
                (role.into(), m.displayText())
            })
            .collect::<Vec<_>>();
        let content = history
            .iter()
            .rev()
            .find(|(role, _)| role == "assistant")
            .map(|(_, c)| c.clone())
            .ok_or("manual update requires an assistant reply")?;
        let mut services = MultiServiceManager::from_runtime_context(self.runtime.clone())
            .map_err(|e| e.to_string())?;
        let service = services
            .getServiceForFunction(FunctionType::MEMORY)
            .map_err(|e| e.to_string())?;
        MemoryLibrary::saveMemoryNowForOwner(
            history,
            content,
            service,
            self.ownerKey.clone(),
            self.runtime.clone(),
        )
        .await
    }
    pub async fn autoCategorize(&self) -> Result<i32, String> {
        self.assertOwner()?;
        let mut manager = MultiServiceManager::from_runtime_context(self.runtime.clone())
            .map_err(|e| e.to_string())?;
        let service = manager
            .getServiceForFunction(FunctionType::MEMORY)
            .map_err(|e| e.to_string())?;
        MemoryLibrary::autoCategorizeForOwner(self.ownerKey.clone(), service, self.runtime.clone())
            .await
    }
    pub fn rebuildProgress(&self) -> MemoryRebuildProgress {
        if rebuildOwner().lock().expect("memory owner lock").as_deref() != Some(&self.ownerKey) {
            return MemoryRebuildProgress {
                status: "idle".into(),
                ..Default::default()
            };
        }
        progress().lock().expect("memory progress lock").clone()
    }
    pub fn cancelRebuild(&self) {
        if rebuildOwner().lock().expect("memory owner lock").as_deref() == Some(&self.ownerKey) {
            CANCEL.store(true, Ordering::SeqCst);
        }
    }
    /// Kotlin-style complete-history rebuild with progress, cancellation and inclusive time range.
    pub fn startRebuild(
        &self,
        chatIds: Vec<String>,
        windowMessageCount: i32,
        fromInclusive: Option<i64>,
        toInclusive: Option<i64>,
    ) -> Result<(), String> {
        self.assertOwner()?;
        if matches!((fromInclusive,toInclusive),(Some(a),Some(b)) if a>b) {
            return Err("start time must precede end time".into());
        }
        if chatIds.is_empty() {
            return Err("select at least one chat".into());
        }
        if REBUILDING.swap(true, Ordering::SeqCst) {
            return Err("memory rebuild already running".into());
        }
        *rebuildOwner().lock().expect("memory owner lock") = Some(self.ownerKey.clone());
        CANCEL.store(false, Ordering::SeqCst);
        *progress().lock().expect("memory progress lock") = MemoryRebuildProgress {
            status: "preparing".into(),
            ..Default::default()
        };
        let runtime = self.runtime.clone();
        let owner = self.ownerKey.clone();
        if let Err(e) = defaultHostRuntimeTaskSchedulerHost().scheduleHostRuntimeAsyncTask(
            "operit-memory-rebuild",
            Box::new(move || {
                Box::pin(async move {
                    let result = rebuild(
                        runtime,
                        owner,
                        chatIds,
                        windowMessageCount,
                        fromInclusive,
                        toInclusive,
                    )
                    .await;
                    let mut p = progress().lock().expect("memory progress lock");
                    match result {
                        Ok(()) => p.status = "completed".into(),
                        Err(error) if CANCEL.load(Ordering::SeqCst) => {
                            p.status = "cancelled".into();
                            p.lastError = error;
                        }
                        Err(error) => {
                            p.status = "failed".into();
                            p.lastError = error;
                        }
                    }
                    REBUILDING.store(false, Ordering::SeqCst);
                })
            }),
        ) {
            REBUILDING.store(false, Ordering::SeqCst);
            return Err(e.to_string());
        }
        Ok(())
    }
}
fn rebuildOwner() -> &'static Mutex<Option<String>> {
    static OWNER: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    OWNER.get_or_init(|| Mutex::new(None))
}
static REBUILDING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
fn progress() -> &'static Mutex<MemoryRebuildProgress> {
    static STATE: OnceLock<Mutex<MemoryRebuildProgress>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(MemoryRebuildProgress {
            status: "idle".into(),
            ..Default::default()
        })
    })
}

async fn rebuild(
    runtime: ProviderRuntimeContext,
    owner: String,
    chatIds: Vec<String>,
    size: i32,
    from: Option<i64>,
    to: Option<i64>,
) -> Result<(), String> {
    let manager =
        ChatHistoryManager::getInstance(RuntimeStorePaths::default()).map_err(|e| e.to_string())?;
    let mut plans = Vec::new();
    for id in chatIds
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let chat = manager
            .loadChatHistory(id.clone())
            .map_err(|e| e.to_string())?
            .ok_or("chat not found")?;
        if resolveMemoryOwner(&runtime, &chat, &CharacterCardManager::getInstance())? != owner {
            return Err(format!(
                "chat {} belongs to another memory owner",
                chat.title
            ));
        }
        let messages = manager
            .loadChatMessages(&id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|m| ProviderMemoryAutoSaveMessage {
                timestamp: m.timestamp,
                sender: m.sender.clone(),
                content: m.displayText(),
            })
            .collect();
        plans.push((
            chat.title,
            planWindows(messages, size.clamp(8, 48) as usize, from, to),
        ));
    }
    let mut p = MemoryRebuildProgress {
        status: "running".into(),
        totalChats: plans.len() as i32,
        totalWindows: plans.iter().map(|(_, w)| w.len() as i32).sum(),
        totalSourceMessages: plans
            .iter()
            .flat_map(|(_, w)| w)
            .map(|w| w.sourceMessageCount as i32)
            .sum(),
        ..Default::default()
    };
    if p.totalWindows == 0 {
        return Err("selected histories have no user context in the specified range".into());
    }
    *progress().lock().expect("memory progress lock") = p.clone();
    let mut services =
        MultiServiceManager::from_runtime_context(runtime.clone()).map_err(|e| e.to_string())?;
    let service = services
        .getServiceForFunction(FunctionType::MEMORY)
        .map_err(|e| e.to_string())?;
    for (title, windows) in plans {
        p.currentChatTitle = title;
        for window in windows {
            if CANCEL.load(Ordering::SeqCst) {
                return Err("memory rebuild cancelled".into());
            }
            let history = window
                .messages
                .iter()
                .map(|m| {
                    (
                        if m.sender == "user" {
                            "user"
                        } else {
                            "assistant"
                        }
                        .into(),
                        m.content.clone(),
                    )
                })
                .collect::<Vec<_>>();
            let content = history
                .iter()
                .rev()
                .find(|(r, _)| r == "assistant")
                .or(history.last())
                .map(|(_, c)| c.clone())
                .unwrap_or_default();
            if let Err(error) = MemoryLibrary::saveMemoryWindowNowForOwner(
                history,
                content,
                service.clone(),
                owner.clone(),
                runtime.clone(),
                window.messages.len(),
            )
            .await
            {
                p.failedWindows += 1;
                p.lastError = error;
            }
            p.completedWindows += 1;
            p.processedSourceMessages += window.sourceMessageCount as i32;
            *progress().lock().expect("memory progress lock") = p.clone();
        }
        p.completedChats += 1;
        *progress().lock().expect("memory progress lock") = p.clone();
    }
    Ok(())
}
