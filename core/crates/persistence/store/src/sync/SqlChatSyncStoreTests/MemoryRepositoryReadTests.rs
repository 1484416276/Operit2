use super::{installTestHosts, DATABASE_MUTEX, NEXT_ID};

use std::sync::atomic::Ordering;

use crate::repository::MemoryRepository::MemoryRepository;
use crate::SyncOperationStore::{SyncClock, SyncOperationStore};

/// Verifies matching, missing, filtered, wildcard, and blank searches preserve entities and sync clocks.
#[test]
fn memory_search_preserves_entities_and_does_not_record_sync_operations() {
    let _guard = DATABASE_MUTEX.lock().unwrap();
    installTestHosts();
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let repository = MemoryRepository::new(format!("character:read-only-search-{id}"));
    let mut alpha = repository
        .createMemory(
            "alpha".to_string(),
            "first memory".to_string(),
            "text/plain".to_string(),
            "test".to_string(),
            "work".to_string(),
            Some(Vec::new()),
        )
        .unwrap();
    alpha.lastAccessedAt = 17;
    let alpha = repository.saveMemory(alpha).unwrap();
    let mut beta = repository
        .createMemory(
            "beta".to_string(),
            "second memory".to_string(),
            "text/plain".to_string(),
            "test".to_string(),
            "work".to_string(),
            Some(Vec::new()),
        )
        .unwrap();
    beta.lastAccessedAt = 23;
    let beta = repository.saveMemory(beta).unwrap();
    let before = repository.getMemoriesByFolderPath("work").unwrap();
    let syncStore =
        SyncOperationStore::native(crate::RuntimeStorePaths::RuntimeStorePaths::default());
    let clockBefore = syncStore.localClock().unwrap();
    let cases = [
        ("alpha", 1.0, Some("work"), None, None, vec![alpha]),
        ("missing", 1.0, None, None, None, Vec::new()),
        ("beta", 1.0, Some("other"), None, None, Vec::new()),
        ("alpha", 2.0, None, None, None, Vec::new()),
        ("*", 0.0, None, Some(i64::MAX), None, Vec::new()),
        ("", 0.0, None, None, Some(-1), Vec::new()),
        ("*", 2.0, Some("work"), None, None, before.clone()),
        ("", 2.0, Some("work"), None, None, before.clone()),
        ("   ", 2.0, Some("work"), None, None, before.clone()),
        ("beta", 1.0, None, None, None, vec![beta]),
    ];
    for (query, threshold, folder, start, end, expected) in cases {
        let found = repository
            .searchMemories(query, folder, threshold, start, end)
            .unwrap();
        assert_eq!(found, expected, "unexpected results for {query:?}");
        assert_eq!(
            repository.getMemoriesByFolderPath("work").unwrap(),
            before,
            "search changed stored entities for {query:?}"
        );
        assert_eq!(
            syncStore.localClock().unwrap(),
            clockBefore,
            "search advanced the sync clock for {query:?}"
        );
        assert!(
            syncStore
                .operationsSince(
                    &clockBefore,
                    &[crate::ObjectBoxStore::OBJECTBOX_SYNC_DOMAIN.to_string()],
                    usize::MAX,
                )
                .unwrap()
                .is_empty(),
            "search recorded sync operations for {query:?}"
        );
    }
}

/// Verifies searches on an empty memory store leave its synchronization state unchanged.
#[test]
fn memory_search_on_empty_store_does_not_record_sync_operations() {
    let _guard = DATABASE_MUTEX.lock().unwrap();
    installTestHosts();
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let repository = MemoryRepository::new(format!("character:empty-search-{id}"));
    let syncStore =
        SyncOperationStore::native(crate::RuntimeStorePaths::RuntimeStorePaths::default());
    let clockBefore: SyncClock = syncStore.localClock().unwrap();
    for query in ["alpha", "*", "", "   "] {
        assert!(repository
            .searchMemories(query, None, 0.0, None, None)
            .unwrap()
            .is_empty());
        assert_eq!(syncStore.localClock().unwrap(), clockBefore);
        assert!(syncStore
            .operationsSince(
                &clockBefore,
                &[crate::ObjectBoxStore::OBJECTBOX_SYNC_DOMAIN.to_string()],
                usize::MAX,
            )
            .unwrap()
            .is_empty());
    }
}
