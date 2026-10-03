use super::{installTestHosts, DATABASE_MUTEX, NEXT_ID};
use std::fmt::Debug;
use std::sync::atomic::Ordering;

use crate::ObjectBoxStore::{
    applyObjectBoxSyncOperation, ObjectBox, ObjectBoxEntity, OBJECTBOX_SYNC_DOMAIN,
};
use crate::RuntimeStorageHost::runtimeStoragePath;
use crate::RuntimeStorePaths::RuntimeStorePaths;
use crate::SyncOperationStore::{
    SyncClock, SyncOperation, SyncOperationSemantics, SyncOperationStore,
};
use operit_model::DocumentChunk::DocumentChunk;
use operit_model::Memory::{Memory, MemoryLink};
use operit_model::MemoryAutoSaveCandidate::MemoryAutoSaveCandidate;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;

// Replay real outgoing operations into a second database under the same test host.
// Only the database path changes; the wire type, payload and stable entity id do not.
fn roundTrip<T>(entity: T, update: impl FnOnce(&mut T))
where
    T: ObjectBoxEntity + Serialize + DeserializeOwned + Send + Sync + Debug + PartialEq + 'static,
{
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let paths = RuntimeStorePaths::default();
    let source = ObjectBox::<T>::new(
        paths
            .runtime_dir()
            .join(format!("sync-tests/source-{id}.sqlite")),
    );
    let replicaPath = paths
        .runtime_dir()
        .join(format!("sync-tests/replica-{id}.sqlite"));
    let replica = ObjectBox::<T>::new(replicaPath.clone());
    let sync = SyncOperationStore::native(paths);
    let mut clock = sync.localClock().unwrap();
    let mut saved = source.put(entity).unwrap();
    assert!(saved.objectBoxId() > 0);

    let replay = |clock: &mut SyncClock, expectedOperation: &str| {
        let operations = sync
            .operationsSince(clock, &[OBJECTBOX_SYNC_DOMAIN.to_string()], usize::MAX)
            .unwrap();
        assert_eq!(operations.len(), 1);
        let mut operation = operations[0].clone();
        assert_eq!(operation.entityType, T::ENTITY_TYPE);
        assert_eq!(operation.operation, expectedOperation);
        assert_eq!(operation.payload["__entityType"], T::ENTITY_TYPE);
        let (_, entityId) = operation.entityId.rsplit_once('#').unwrap();
        operation.entityId = format!("{}#{entityId}", runtimeStoragePath(&replicaPath));
        *clock = sync.localClock().unwrap();
        // Replay is idempotent and must not feed back into the outgoing operation log.
        applyObjectBoxSyncOperation(&operation).unwrap();
        applyObjectBoxSyncOperation(&operation).unwrap();
        assert_eq!(sync.localClock().unwrap(), *clock);
        assert!(sync
            .operationsSince(clock, &[OBJECTBOX_SYNC_DOMAIN.to_string()], usize::MAX)
            .unwrap()
            .is_empty());
    };

    replay(&mut clock, "upsert");
    assert_eq!(replica.all().unwrap(), vec![saved.clone()]);
    update(&mut saved);
    let saved = source.put(saved).unwrap();
    replay(&mut clock, "upsert");
    assert_eq!(
        replica.get(saved.objectBoxId()).unwrap(),
        Some(saved.clone())
    );
    assert!(source.remove(saved.objectBoxId()).unwrap());
    replay(&mut clock, "delete");
    assert!(replica.all().unwrap().is_empty());
}

#[test]
fn objectbox_sync_replays_all_registered_entities_without_echo() {
    let _guard = DATABASE_MUTEX.lock().unwrap();
    installTestHosts();
    roundTrip(
        MemoryAutoSaveCandidate::replyFinalized("chat-sync".into(), 100, 200),
        |candidate| {
            candidate.status = MemoryAutoSaveCandidate::STATUS_FAILED.into();
            candidate.attemptCount = 1;
            candidate.lastError = "retry".into();
        },
    );
    roundTrip(
        MemoryAutoSaveCandidate::selectedUserMessage("chat-selected".into(), 101, 201),
        |candidate| {
            candidate.status = MemoryAutoSaveCandidate::STATUS_PROCESSING.into();
        },
    );
    roundTrip(
        DocumentChunk {
            id: 0,
            memoryUuid: "memory-uuid".into(),
            chunkIndex: 0,
            content: "before".into(),
        },
        |chunk| {
            chunk.content = "after".into();
        },
    );
    roundTrip(
        MemoryLink {
            id: 0,
            sourceMemoryId: 1,
            targetMemoryId: 2,
            type_: "related".into(),
            weight: 0.5,
            description: "before".into(),
        },
        |link| {
            link.description = "after".into();
        },
    );
    let memory: Memory = serde_json::from_value(json!({
        "id": 0, "uuid": "memory-uuid", "title": "before", "content": "body",
        "contentType": "text/plain", "source": "test", "credibility": 0.5,
        "importance": 0.5, "isDocumentNode": false, "createdAt": 100,
        "updatedAt": 100, "lastAccessedAt": 100, "tags": [], "properties": []
    }))
    .unwrap();
    roundTrip(memory, |memory| {
        memory.title = "after".into();
    });
}

fn candidateOperation() -> SyncOperation {
    SyncOperation {
        opId: "peer:1".into(),
        originDeviceId: "peer".into(),
        sequence: 1,
        domain: OBJECTBOX_SYNC_DOMAIN.into(),
        entityType: "MemoryAutoSaveCandidate".into(),
        entityId: "unused.sqlite#42".into(),
        operation: "upsert".into(),
        semantics: SyncOperationSemantics::EntityState,
        payload: json!({"__entityType": "MemoryAutoSaveCandidate", "entity": MemoryAutoSaveCandidate::replyFinalized("chat".into(), 100, 200)}),
        createdAt: 200,
        schemaVersion: 1,
    }
}

#[test]
fn objectbox_sync_rejects_unknown_operations_and_mismatched_types_before_opening_database() {
    for (domain, entityType, name) in [
        ("other", "MemoryAutoSaveCandidate", "upsert"),
        (OBJECTBOX_SYNC_DOMAIN, "Unknown", "upsert"),
        (OBJECTBOX_SYNC_DOMAIN, "MemoryAutoSaveCandidate", "truncate"),
    ] {
        let mut operation = candidateOperation();
        operation.domain = domain.into();
        operation.entityType = entityType.into();
        operation.operation = name.into();
        assert_eq!(
            applyObjectBoxSyncOperation(&operation)
                .unwrap_err()
                .to_string(),
            format!("unsupported sync operation: {domain}/{entityType}/{name}")
        );
    }
    for name in ["upsert", "delete"] {
        let mut operation = candidateOperation();
        operation.operation = name.into();
        operation.payload["__entityType"] = json!("Memory");
        assert!(applyObjectBoxSyncOperation(&operation)
            .unwrap_err()
            .to_string()
            .contains("entity type mismatch"));
        operation.payload = json!({});
        assert!(applyObjectBoxSyncOperation(&operation)
            .unwrap_err()
            .to_string()
            .contains("missing __entityType"));
    }
}

#[test]
fn objectbox_sync_accepts_legacy_candidate_payload_and_uses_wire_entity_id() {
    let _guard = DATABASE_MUTEX.lock().unwrap();
    installTestHosts();
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let path = RuntimeStorePaths::default()
        .runtime_dir()
        .join(format!("sync-tests/legacy-{id}.sqlite"));
    let replica = ObjectBox::<MemoryAutoSaveCandidate>::new(path.clone());
    let mut operation = candidateOperation();
    operation.entityId = format!("{}#42", runtimeStoragePath(&path));
    applyObjectBoxSyncOperation(&operation).unwrap();
    let saved = replica.get(42).unwrap().unwrap();
    assert_eq!(saved.id, 42);
    assert_eq!(saved.chatId, "chat");
    // Malformed typed payload must fail rather than replacing the valid existing row.
    operation.payload["entity"] = json!({"id": 42});
    assert!(applyObjectBoxSyncOperation(&operation).is_err());
    assert_eq!(replica.get(42).unwrap(), Some(saved));
}
