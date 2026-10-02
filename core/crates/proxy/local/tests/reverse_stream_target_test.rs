//! Regression: string targets must not be debug-formatted twice by codegen.
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use operit_host_api::HostManager::{HostManager, setDefaultHostRuntimeTaskSchedulerHost};
use operit_host_api::RuntimeStorageHost;
use operit_host_native_common::{NativeHostRuntimeTaskSchedulerHost, NativeRuntimeStorageHost, PosixFileSystemHost, NativeHostJavaScriptRuntimeHost};
use operit_link::{toCoreValue, CorePushRequest};
use operit_proxy_local::LocalCoreProxy;
use operit_runtime::core::application::OperitApplication::OperitApplication;
use operit_store::RuntimeFileSyncStore::{RuntimeFileSyncReference, RuntimeFileSyncStore};
use operit_util::RuntimeStorageLayout::RUNTIME_SYNC_DIR_PATH;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generated_blob_stream_accepts_schema_target_and_persists_chunks() {
    let root = std::env::temp_dir().join(format!("operit-reverse-stream-{}-{}",
        std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    let storage = Arc::new(NativeRuntimeStorageHost::new(root.join("runtime"), root.join("workspaces")));
    let scheduler = Arc::new(NativeHostRuntimeTaskSchedulerHost::new());
    setDefaultHostRuntimeTaskSchedulerHost(scheduler.clone());
    let host = HostManager {
        hostJavaScriptRuntimeHost: Some(Arc::new(NativeHostJavaScriptRuntimeHost::new())),
        fileSystemHost: Some(Arc::new(PosixFileSystemHost::new())),
        runtimeStorageHost: Some(storage.clone()),
        runtimeStorageWriteHost: Some(storage.clone()),
        runtimeSqliteHost: Some(storage.clone()),
        hostRuntimeTaskSchedulerHost: Some(scheduler),
        ..HostManager::default()
    };
    let proxy = LocalCoreProxy::new(OperitApplication::newWithContext(host));
    let target = LocalCoreProxy::generatedTargetForSchema("services.syncBlobTransferManager").unwrap();
    assert_eq!(target, "core/services.syncBlobTransferManager");
    // SHA-256 of abc; upload more than one chunk to exercise stream completion.
    let reference = RuntimeFileSyncReference {
        contentHash: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
        size: 3,
    };
    let request = CorePushRequest::new("blob-regression", target, "syncReceiveBlob")
        .withArgs(toCoreValue(json!({"contentHash": reference.contentHash, "size": reference.size})).unwrap());
    assert!(proxy.isReverseStreamRequest(&request));
    let mut stream = proxy.openPushLocal(request).expect("recognized reverse stream target must also open");
    tokio::time::timeout(Duration::from_secs(10), async {
        stream.send(toCoreValue(b"a".to_vec()).unwrap()).await.unwrap();
        stream.send(toCoreValue(b"bc".to_vec()).unwrap()).await.unwrap();
        stream.close().await.unwrap();
    }).await.expect("blob stream must complete");
    let store = RuntimeFileSyncStore::new(storage.clone(), RUNTIME_SYNC_DIR_PATH);
    let path = store.blobStoragePath(&reference).unwrap();
    assert_eq!(storage.readBytes(&path).unwrap(), b"abc");

    for bad_target in [format!("\"{target}\""), "core/services.notDeclared".into()] {
        let request = CorePushRequest::new("bad-target", bad_target, "syncReceiveBlob");
        assert!(!proxy.isReverseStreamRequest(&request));
        let error = match proxy.openPushLocal(request) {
            Ok(_) => panic!("undeclared reverse stream must be rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("REVERSE_STREAM_NOT_FOUND"));
    }
    std::fs::remove_dir_all(root).unwrap();
}
