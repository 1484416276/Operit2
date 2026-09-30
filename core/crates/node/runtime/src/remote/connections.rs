use operit_peer_link::{isPeerLinkActive, PeerLinkClient};
// Application-owned PeerLink lifecycle. Transport records are adapters, not node roles.
use crate::remote::target::PeerConnectionTarget;
use crate::remote::LinkAccessStore;
use crate::CoreNodeRouter::CoreNodeRouter;
use futures_util::{stream::FuturesUnordered, StreamExt};
use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use operit_store::{CoreSpaceStore::CoreSpaceStore, NetworkControlStore::NetworkControlStore};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, OnceLock, Weak},
};

type PeerLocks = BTreeMap<(String, String), Weak<tokio::sync::Mutex<()>>>;
static PEER_LOCKS: OnceLock<Mutex<PeerLocks>> = OnceLock::new();
/// Serialize only competing attempts to the same identity, never unrelated devices.
pub(crate) fn peerLock(local: &str, peer: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = PEER_LOCKS.get_or_init(Default::default).lock().unwrap();
    locks.retain(|_, lock| lock.strong_count() > 0);
    let key = (local.into(), peer.into());
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

/// A session-scoped owner: closing an old handle cannot close a replacement connection.
struct OwnedLink {
    target: PeerConnectionTarget,
    client: PeerLinkClient,
}
impl Drop for OwnedLink {
    fn drop(&mut self) {
        self.client.close("Connection owner released".into());
    }
}

fn retryDelayMs(failures: u32) -> u64 {
    2u64.saturating_pow(failures.min(5)).min(30) * 1000
}

#[derive(Clone)]
pub(crate) struct PeerConnectionManager {
    router: CoreNodeRouter,
    access: LinkAccessStore,
    space: CoreSpaceStore,
    control: NetworkControlStore,
    run: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    // A new generation waits for cancelled attempts and owned links to be dropped.
    lifecycle: Arc<tokio::sync::Mutex<()>>,
}
impl PeerConnectionManager {
    pub(crate) fn new(
        router: CoreNodeRouter,
        access: LinkAccessStore,
        space: CoreSpaceStore,
        control: NetworkControlStore,
    ) -> Self {
        Self {
            router,
            access,
            space,
            control,
            run: Default::default(),
            lifecycle: Default::default(),
        }
    }
    pub(crate) fn start(&self) -> Result<(), String> {
        let mut run = self.run.lock().map_err(|e| e.to_string())?;
        if run.as_ref().is_some_and(|sender| !sender.is_closed()) {
            return Ok(());
        }
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let manager = self.clone();
        defaultHostRuntimeTaskSchedulerHost().scheduleHostRuntimeAsyncTask("peer-connections", Box::new(move || Box::pin(async move {
            let _generation = tokio::select! {
                _ = &mut stopped => return,
                guard = manager.lifecycle.lock() => guard,
            };
            let mut owned = Vec::<OwnedLink>::new();
            type Attempt = std::pin::Pin<Box<dyn std::future::Future<Output = (String, Result<Option<OwnedLink>, String>)>>>;
            let mut work = FuturesUnordered::<Attempt>::new();
            let mut pending = BTreeSet::new();
            let scheduler = defaultHostRuntimeTaskSchedulerHost();
            let mut retries = BTreeMap::<String, u32>::new();
            let mut cooling = BTreeSet::new();
            type RetryWait = std::pin::Pin<Box<dyn std::future::Future<Output = (String, operit_host_api::HostResult<()>)>>>;
            let mut retryWaits = FuturesUnordered::<RetryWait>::new();
            // Keep this future across select iterations: completed connections must not reset the scan delay.
            let mut scanDelay = scheduler.waitForHostRuntimeDelay(0);
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stopped => break,
                    Some((peer, result)) = work.next(), if !work.is_empty() => {
                        pending.remove(&peer);
                        match result {
                            Ok(link) => { retries.remove(&peer); if let Some(link) = link { owned.push(link); } }
                            Err(error) => {
                                let failures = retries.get(&peer).map_or(1, |count| count.saturating_add(1));
                                retries.insert(peer.clone(), failures);
                                cooling.insert(peer.clone());
                                let delay = scheduler.waitForHostRuntimeDelay(retryDelayMs(failures));
                                retryWaits.push(Box::pin(async move { (peer, delay.await) }));
                                operit_util::AppLogger::AppLogger::d("PeerConnectionManager", &error);
                            }
                        }
                    }
                    Some((peer, result)) = retryWaits.next(), if !retryWaits.is_empty() => {
                        if let Err(error) = result {
                            operit_util::AppLogger::AppLogger::w("PeerConnectionManager", &format!("Host retry delay failed: {error}"));
                            break;
                        }
                        cooling.remove(&peer);
                    }
                    result = &mut scanDelay => {
                        if let Err(error) = result {
                            operit_util::AppLogger::AppLogger::w("PeerConnectionManager", &format!("Host scan delay failed: {error}"));
                            break;
                        }
                        scanDelay = scheduler.waitForHostRuntimeDelay(1000);
                        owned.retain(|link| !link.client.isClosed() && link.target.validate(
                            &manager.router, &manager.access, &manager.space, &manager.control).is_ok());
                        let targets = match PeerConnectionTarget::load(&manager.access) {
                            Ok(targets) => targets,
                            Err(error) => { operit_util::AppLogger::AppLogger::w("PeerConnectionManager", &error); continue; }
                        };
                        retries.retain(|peer, _| targets.iter().any(|target| &target.peerNodeId == peer));
                        for target in targets {
                            let peer = target.peerNodeId.clone();
                            if pending.contains(&peer) || cooling.contains(&peer) {
                                continue;
                            }
                            pending.insert(peer.clone());
                            let m = manager.clone();
                            work.push(Box::pin(async move {
                                let result = operit_peer_link::timing::withHostTimeout(10_000, m.connect(target)).await;
                                (peer, result)
                            }));
                        }
                    }
                }
            }
            // Drop in-flight handshakes before releasing this generation's lifecycle lock.
            drop(retryWaits);
            drop(work);
            drop(owned);

        }))).map_err(|e| e.to_string())?;
        *run = Some(stop);
        Ok(())
    }
    async fn connect(&self, target: PeerConnectionTarget) -> Result<Option<OwnedLink>, String> {
        let lock = peerLock(&target.localNodeId, &target.peerNodeId);
        let _guard = lock.lock().await;
        target.validate(&self.router, &self.access, &self.space, &self.control)?;
        if isPeerLinkActive(&target.localNodeId, &target.peerNodeId)? {
            return Ok(None);
        }
        let client = target
            .connect(&self.router, &self.space)
            .await?;
        let owned = OwnedLink { target, client };
        // Authorization/credential changes during a handshake apply before publication to the owner.
        owned
            .target
            .validate(&self.router, &self.access, &self.space, &self.control)?;
        Ok(Some(owned))
    }
    pub(crate) fn stop(&self) -> Result<(), String> {
        if let Some(stop) = self.run.lock().map_err(|e| e.to_string())?.take() {
            let _ = stop.send(());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_backoff_is_bounded() {
        assert_eq!(retryDelayMs(1), 2000);
        assert_eq!(retryDelayMs(3), 8000);
        assert_eq!(retryDelayMs(u32::MAX), 30_000);
    }
    #[tokio::test]
    async fn peer_locks_isolate_devices_and_share_same_identity() {
        let a = peerLock("local", "a");
        let same = peerLock("local", "a");
        let b = peerLock("local", "b");
        let _guard = a.lock().await;
        assert!(same.try_lock().is_err());
        assert!(b.try_lock().is_ok());
    }
}
