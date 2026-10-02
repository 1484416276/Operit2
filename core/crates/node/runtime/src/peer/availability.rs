//! Authenticated availability belongs to the peer runtime, not the persistence
//! synchronizer. Restored credentials must be usable before a peer is online.
use super::*;
use futures_util::{stream, StreamExt};
use operit_store::NetworkControlStore::NetworkControlStore;
use tokio::sync::oneshot;

const PROBE_INTERVAL_MS: u64 = 3_000;
const PROBE_DEADLINE_MS: u64 = 5_000;

pub(super) struct AvailabilityWorker {
    stop: oneshot::Sender<()>,
    done: oneshot::Receiver<()>,
}

impl HostRuntimePeerService {
    pub(super) fn requirePeerConnectionAllowed(&self, node: &str) -> Result<(), CoreLinkError> {
        let storage = self.state.host.runtimeStorageHost.clone()
            .ok_or_else(|| error("Runtime storage Host is not installed"))?;
        if NetworkControlStore::new(storage).map_err(error)?.nodeIsDisconnected(node).map_err(error)? {
            return Err(CoreLinkError::new("PEER_CONNECTION_REVOKED", "Peer is disconnected or removed by current Space policy"));
        }
        Ok(())
    }

    /// Starts authenticated peer probes independently of inbound listening support.
    pub(super) fn startAvailabilityWorker(&self) -> Result<(), CoreLinkError> {
        let mut worker = self.state.availability.lock().map_err(|_| error("Peer availability lock poisoned"))?;
        if worker.is_some() { return Ok(()); }
        let scheduler = self.state.host.hostRuntimeTaskSchedulerHost.clone()
            .ok_or_else(|| error("Host scheduler is not installed"))?;
        let tasks = scheduler.clone();
        let state = Arc::downgrade(&self.state);
        let (stop, mut stopped) = oneshot::channel();
        let (done, finished) = oneshot::channel();
        scheduler.scheduleHostRuntimeAsyncTask("peer-availability", Box::new(move || Box::pin(async move {
            loop {
                let Some(state) = state.upgrade() else { break; };
                let service = HostRuntimePeerService { state };
                // Cancellation drops pending probes before stop() closes listeners
                // and connections, so the worker cannot mark peers online later.
                tokio::select! {
                    biased;
                    _ = &mut stopped => break,
                    result = service.probePairedPeers() => {
                        if let Err(e) = result {
                            operit_util::AppLogger::AppLogger::w("RuntimePeerService", &format!("Peer availability check failed: {e}"));
                        }
                    }
                }
                drop(service);
                tokio::select! {
                    biased;
                    _ = &mut stopped => break,
                    _ = tasks.waitForHostRuntimeDelay(PROBE_INTERVAL_MS) => {}
                }
            }
            let _ = done.send(());
        }))).map_err(|e| error(e.to_string()))?;
        *worker = Some(AvailabilityWorker { stop, done: finished });
        Ok(())
    }

    /// Cancels probes and waits without retaining the worker state lock.
    pub(super) async fn stopAvailabilityWorker(&self) {
        let worker = self.state.availability.lock().unwrap().take();
        if let Some(AvailabilityWorker { stop, done }) = worker {
            let _ = stop.send(());
            let _ = done.await;
        }
    }

    async fn probePairedPeers(&self) -> Result<(), CoreLinkError> {
        let control = NetworkControlStore::new(self.state.host.runtimeStorageHost.clone().ok_or_else(|| error("Runtime storage Host is not installed"))?).map_err(error)?;
        let mut candidates = Vec::new();
        for peer in self.pairedPeers()? {
            // An inbound pairing alone is never outbound permission. A return
            // grant is usable only while the original pairing and Space agree.
            if !control.nodeIsDisconnected(&peer.nodeId).map_err(error)?
                && (peer.outbound || self.spaceOutbound(&peer.nodeId).is_ok()) {
                candidates.push(peer.nodeId);
            }
        }
        stream::iter(candidates).for_each_concurrent(4, |node| async move {
            let scheduler = self.state.host.hostRuntimeTaskSchedulerHost.as_ref().unwrap();
            let result = tokio::select! {
                result = self.connectAuthorized(&node) => result,
                _ = scheduler.waitForHostRuntimeDelay(PROBE_DEADLINE_MS) => Err(error("Authenticated availability probe timed out")),
            };
            match result {
                Ok(channel) => { channel.raw.close().await; }
                Err(e) => {
                    if failureProvesPeerUnavailable(&e) && self.state.active.lock().unwrap().remove(&node) {
                        operit_util::AppLogger::AppLogger::i("RuntimePeerService", &format!("Peer offline peer={node} error={e}"));
                        self.changed();
                    }
                    operit_util::AppLogger::AppLogger::trace("RuntimePeerService", &format!("Availability probe failed peer={node} error={e}"));
                }
            }
        }).await;
        Ok(())
    }
}
