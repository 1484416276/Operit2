//! Post-commit notifications. Observers may schedule work, but cannot change a sync result.
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::SyncOperationStore::SyncOperation;
use operit_host_api::RuntimeStorageHost;

type Listener = Arc<dyn Fn(&Arc<dyn RuntimeStorageHost>, &[SyncOperation]) + Send + Sync>;
static LISTENERS: OnceLock<Mutex<BTreeMap<usize, Listener>>> = OnceLock::new();
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

pub struct SyncAppliedSubscription(usize);
impl Drop for SyncAppliedSubscription {
    fn drop(&mut self) {
        if let Some(listeners) = LISTENERS.get() {
            listeners
                .lock()
                .expect("sync observers poisoned")
                .remove(&self.0);
        }
    }
}

/// Subscriptions are runtime-owned and detached on shutdown, not process-wide services.
pub fn subscribe(
    storage: Arc<dyn RuntimeStorageHost>,
    listener: impl Fn(&[SyncOperation]) + Send + Sync + 'static,
) -> SyncAppliedSubscription {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    LISTENERS
        .get_or_init(Default::default)
        .lock()
        .expect("sync observers poisoned")
        .insert(
            id,
            Arc::new(move |source, changes| {
                if Arc::ptr_eq(&storage, source) {
                    listener(changes);
                }
            }),
        );
    SyncAppliedSubscription(id)
}

/// Invoke only after data and sync clocks have committed, and outside store locks.
pub fn publish(storage: &Arc<dyn RuntimeStorageHost>, changes: &[SyncOperation]) {
    if changes.is_empty() {
        return;
    }
    let listeners: Vec<_> = LISTENERS
        .get_or_init(Default::default)
        .lock()
        .expect("sync observers poisoned")
        .values()
        .cloned()
        .collect();
    for listener in listeners {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| listener(storage, changes)))
            .is_err()
        {
            operit_util::AppLogger::AppLogger::e(
                "SyncAppliedChanges",
                "Post-commit observer panicked; synchronized data remains committed",
            );
        }
    }
}
