use operit_host_api::TimeUtils::currentTimeMillis;
use operit_model::MemoryAutoSaveCandidate::MemoryAutoSaveCandidate;
use operit_util::OperitPaths::memoryAutoSaveCandidateSqlitePath;

use crate::ObjectBoxStore::ObjectBox;

/// Persists deferred memory extraction requests for one owner namespace.
#[derive(Clone)]
pub struct MemoryAutoSaveCandidateRepository {
    candidateBox: ObjectBox<MemoryAutoSaveCandidate>,
}

impl MemoryAutoSaveCandidateRepository {
    /// Opens the owner-scoped deferred memory candidate store.
    pub fn new(ownerKey: &str) -> Self {
        Self {
            candidateBox: ObjectBox::new(
                memoryAutoSaveCandidateSqlitePath(ownerKey)
                    .expect("memory auto-save candidate path must be valid"),
            ),
        }
    }

    /// Enqueues one finalized reply for deferred memory extraction.
    pub fn enqueue(&self, chatId: String, triggerMessageTimestamp: i64) -> Result<(), String> {
        self.candidateBox
            .put(MemoryAutoSaveCandidate::replyFinalized(
                chatId,
                triggerMessageTimestamp,
                currentTimeMillis(),
            ))
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Enqueues selected user messages for deferred memory extraction.
    pub fn enqueueSelectedUserMessages(
        &self,
        chatId: String,
        triggerMessageTimestamps: Vec<i64>,
    ) -> Result<(), String> {
        let now = currentTimeMillis();
        if chatId.trim().is_empty() { return Err("memory queue chatId is required".into()); }
        let triggerMessageTimestamps = triggerMessageTimestamps.into_iter().filter(|t|*t>0)
            .collect::<std::collections::BTreeSet<_>>();
        for timestamp in triggerMessageTimestamps {
            self.candidateBox
                .put(MemoryAutoSaveCandidate::selectedUserMessage(
                    chatId.clone(),
                    timestamp,
                    now,
                ))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn allCandidates(&self) -> Result<Vec<MemoryAutoSaveCandidate>, String> {
        self.candidateBox.all().map_err(|e|e.to_string())
    }
    pub fn countPendingAndFailedCandidates(&self) -> Result<i32,String> {
        Ok(self.getPendingAndFailedCandidates()?.len() as i32)
    }
    pub fn countPendingAndFailedChats(&self) -> Result<i32,String> {
        Ok(self.getPendingAndFailedCandidates()?.into_iter().map(|c|c.chatId)
            .collect::<std::collections::BTreeSet<_>>().len() as i32)
    }
    pub fn markPending(&self, ids: &[i64]) -> Result<(),String> {
        self.updateCandidates(ids, |c| { c.status = MemoryAutoSaveCandidate::STATUS_PENDING.into(); c.lastError.clear(); })
    }

    /// Loads candidates that are ready to be processed or retried.
    pub fn getPendingAndFailedCandidates(&self) -> Result<Vec<MemoryAutoSaveCandidate>, String> {
        let mut candidates = self
            .candidateBox
            .all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|candidate| {
                candidate.status == MemoryAutoSaveCandidate::STATUS_PENDING
                    || candidate.status == MemoryAutoSaveCandidate::STATUS_FAILED
            })
            .collect::<Vec<_>>();
        candidates
            .sort_by_key(|candidate| (candidate.triggerMessageTimestamp, candidate.createdAt));
        Ok(candidates)
    }

    /// Marks candidates as actively processing.
    pub fn markProcessing(&self, ids: &[i64]) -> Result<(), String> {
        self.updateCandidates(ids, |candidate| {
            candidate.status = MemoryAutoSaveCandidate::STATUS_PROCESSING.to_string();
            candidate.lastError.clear();
        })
    }

    /// Deletes candidates after successful extraction or deliberate discard.
    pub fn deleteCandidates(&self, ids: &[i64]) -> Result<(), String> {
        self.candidateBox
            .removeByIds(ids)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Marks candidates failed so the next scheduler pass can retry them.
    pub fn markFailed(&self, ids: &[i64], errorMessage: &str) -> Result<(), String> {
        let normalizedError = errorMessage.chars().take(500).collect::<String>();
        self.updateCandidates(ids, |candidate| {
            candidate.status = MemoryAutoSaveCandidate::STATUS_FAILED.to_string();
            candidate.attemptCount += 1;
            candidate.lastError = normalizedError.clone();
        })
    }

    /// Applies one status mutation to the supplied candidate ids.
    fn updateCandidates<F>(&self, ids: &[i64], mut update: F) -> Result<(), String>
    where
        F: FnMut(&mut MemoryAutoSaveCandidate),
    {
        let mut candidates = self
            .candidateBox
            .getMany(ids)
            .map_err(|error| error.to_string())?;
        let now = currentTimeMillis();
        for candidate in &mut candidates {
            update(candidate);
            candidate.updatedAt = now;
        }
        self.candidateBox
            .putMany(candidates)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
