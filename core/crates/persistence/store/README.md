# operit-store

`operit-store` owns the shared storage primitives used by the Operit2 runtime.
It centralizes host-backed storage access, preference state flows, SQLite
helpers, ObjectBox-style repositories, DAO code, repository code, encryption
helpers, and sync operation logs.

## Responsibilities

- Hold runtime storage and SQLite host registrations.
- Provide typed path helpers rooted at the configured runtime data directory.
- Provide Kotlin Flow-like state containers used by preference managers.
- Persist preferences and sync metadata through `RuntimeStorageHost`.
- Persist repository-owned data for chat history, memory, avatars, custom emoji,
  markdown, usage statistics, workflow state, and UI hierarchy state.
- Provide SQLite and ObjectBox-style store helpers used by repositories.
- Compact and query sync operation streams.

## Key Files

- `src/RuntimeStorePaths.rs`: typed path builder rooted at the configured data
  directory.
- `src/RuntimeStorageHost.rs`: process-wide host registration and path
  conversion helpers.
- `src/PreferencesDataStore.rs`: observable preference storage and state flow
  abstractions.
- `src/PreferencesEncryption.rs`: encrypted preference payload support.
- `src/SqliteStore.rs`: host-backed SQLite access utilities.
- `src/ObjectBoxStore.rs`: ObjectBox-style collection persistence.
- `src/repository/`: repository implementations.
- `src/dao/` and `src/db/`: SQLite DAO and database helpers.
- `src/sync/`: chat sync stores and sync tests.
- `src/SyncOperationStore.rs`: sync operation append, clock, observation, and
  compaction logic.

## Storage Layout

`operit-util::RuntimeStorageLayout` is the source of truth for paths under
`runtime/`, including configuration preferences, memory data, package and skill
extensions, MCP configuration, caches, logs, exports, and the SQLite database
path. Workspace directories are rooted at `workspaces/` beside `runtime/`.

## ObjectBox Synchronization

`ObjectBoxStore.rs` owns both entity persistence and typed sync replay. Register
models once in `registerObjectBoxEntities!`; the declaration generates the sealed
`ObjectBoxEntity` implementation, its stable `ENTITY_TYPE` wire name, and the
incoming operation handler. Repositories open `ObjectBox::<T>::new(path)` without
supplying a second, independently maintained type name. `OperitApplication` only
routes the `objectbox` domain to `applyObjectBoxSyncOperation`.

The registered types are `Memory`, `MemoryLink`, `MemoryAutoSaveCandidate`, and
`DocumentChunk`. All support `upsert` and `delete`. Incoming payload type names
must match the operation type; unknown types, unsupported operations, and invalid
typed payloads fail explicitly. Replay does not append new local operations;
clock/conflict handling remains with the existing sync operation store.

Persisted type names, entity IDs, payloads, and database schemas are unchanged.
Existing queued operations can be replayed without clearing data. Deploy the fix
to all participating CoreNodes: an older receiver still rejects the newly handled
types. This is data replication, not distributed task claiming; the memory
scheduler's process-local guard is not a cross-node execution lock.

## Memory Search Read Semantics

Like the Kotlin memory repository, memory searches read a snapshot without
updating `lastAccessedAt` or `updatedAt`, rewriting entities, or recording sync
operations. Wildcard (`*`) and blank queries return the folder- and time-filtered
snapshot directly; nonblank queries use the existing lexical relevance ranking.
This keeps reads separate from content edits and prevents searches from emitting
stale full-record sync states. Synchronization conflict resolution is unchanged.

## Host Boundary

Runtime business logic uses relative storage paths. The installed
`RuntimeStorageHost` maps those paths to the active platform storage backend,
such as native file storage, browser storage, or an app-private directory.
