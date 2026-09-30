use crate::{connection::*, observer::PeerLinkObserver};
use async_trait::async_trait;
use operit_host_api::{
    HostManager::{defaultHostRuntimeTaskSchedulerHost, defaultHttpHost},
    HttpRequestData,
    TimeUtils::currentTimeMillis,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex as StdMutex,
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

/// HTTP authentication supplied by node session admission; the carrier owns framing and ordered I/O.
pub trait HttpPeerSession: Send + Sync {
    fn localNodeId(&self) -> String;
    fn peerNodeId(&self) -> String;
    fn sessionId(&self) -> String;
    fn baseUrl(&self) -> &str;
    fn sign(&self, bytes: &[u8]) -> String;
    fn signedRemotePost(&self, path: &str, bytes: Vec<u8>) -> Result<Vec<u8>, String>;
}
const OUTBOUND_PEER_FRAME_BATCH_MAX_FRAMES: usize = 128;

/// Stores the shared state of one queued outbound HTTP Peer carrier.
struct OutboundPeerFrameBatchState {
    session: Arc<dyn HttpPeerSession>,
    streamId: String,
    closed: AtomicBool,
    failure: StdMutex<Option<String>>,
    closeSender: StdMutex<Option<oneshot::Sender<()>>>,
}

/// Queues client-originated frames for ordered HTTP batch delivery.
struct OutboundPeerFrameSender {
    state: Arc<OutboundPeerFrameBatchState>,
    frameSender: mpsc::UnboundedSender<(i64, PeerFrame)>,
}

#[async_trait]
impl PeerFrameSender for OutboundPeerFrameSender {
    /// Queues one frame without waiting for a separate HTTP request.
    async fn send(&self, frame: PeerFrame) -> Result<(), String> {
        if let Some(error) = self
            .state
            .failure
            .lock()
            .map_err(|error| error.to_string())?
            .clone()
        {
            return Err(error);
        }
        if self.state.closed.load(Ordering::Acquire) {
            return Err("Outbound Peer frame sender is closed".to_string());
        }
        self.frameSender
            .send((currentTimeMillis(), frame))
            .map_err(|_| "Outbound Peer frame queue is closed".to_string())
    }

    /// Stops the batch worker and closes the Host-owned response stream.
    fn close(&self) {
        if self.state.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(sender) = self
            .state
            .closeSender
            .lock()
            .expect("Outbound Peer close mutex poisoned")
            .take()
        {
            let _ = sender.send(());
        }
        let _ = defaultHttpHost().closeHttpByteStream(&self.state.streamId);
    }
}

/// Records one terminal error and closes the queued outbound carrier.
fn failOutboundPeerFrameBatch(state: &Arc<OutboundPeerFrameBatchState>, error: String) {
    if let Ok(mut failure) = state.failure.lock() {
        *failure = Some(error.clone());
    }
    state.closed.store(true, Ordering::Release);
    let _ = defaultHttpHost().closeHttpByteStream(&state.streamId);
    operit_host_api::tryLogHostConsole(
        6,
        "PeerCarrierTrace",
        &format!("outbound_peer_frame_batch_failed error={error}"),
    );
}

/// Drains queued Peer frames into ordered bounded HTTP batches.
async fn runOutboundPeerFrameBatchSender(
    state: Arc<OutboundPeerFrameBatchState>,
    mut receiver: mpsc::UnboundedReceiver<(i64, PeerFrame)>,
    mut closeReceiver: oneshot::Receiver<()>,
) {
    loop {
        let firstFrame = tokio::select! {
            biased;
            _ = &mut closeReceiver => return,
            frame = receiver.recv() => frame,
        };
        let Some((queuedAt, firstFrame)) = firstFrame else {
            return;
        };
        let mut frames = Vec::with_capacity(OUTBOUND_PEER_FRAME_BATCH_MAX_FRAMES);
        frames.push(firstFrame);
        while frames.len() < OUTBOUND_PEER_FRAME_BATCH_MAX_FRAMES {
            match receiver.try_recv() {
                Ok((_, frame)) => frames.push(frame),
                Err(mpsc::error::TryRecvError::Empty)
                | Err(mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        let frameCount = frames.len();
        let queueWaitMs = currentTimeMillis().saturating_sub(queuedAt);
        let body = match operit_link::encodeLink(&PeerFrameBatch { frames }) {
            Ok(body) => body,
            Err(error) => {
                failOutboundPeerFrameBatch(&state, error.to_string());
                return;
            }
        };
        let postStartedAt = currentTimeMillis();
        let result = state.session.signedRemotePost("peer/channel/frame", body);
        operit_host_api::tryLogHostConsole(
            2,
            "PeerCarrierTrace",
            &format!(
                "http_frame_batch_sent frames={} queueWaitMs={} postMs={}",
                frameCount,
                queueWaitMs,
                currentTimeMillis().saturating_sub(postStartedAt)
            ),
        );
        if let Err(error) = result {
            failOutboundPeerFrameBatch(&state, error);
            return;
        }
    }
}

/// A timed-out/cancelled open must close host resources even before registration.
struct PendingPeerConnection(Option<Arc<PeerConnection>>);
impl Drop for PendingPeerConnection {
    fn drop(&mut self) {
        if let Some(connection) = self.0.take() {
            connection.close("Peer open cancelled".into());
        }
    }
}
/// Opens and registers the HTTP-carried outbound Peer Link.
#[allow(non_snake_case)]
pub async fn open(
    session: Arc<dyn HttpPeerSession>,
    core: Arc<dyn CoreNodeTransportClient>,
    topologyStore: Option<Arc<dyn PeerLinkObserver>>,
) -> Result<PeerLinkClient, String> {
    let localNodeId = session.localNodeId();
    let peerNodeId = session.peerNodeId();
    let channelId = format!("peer-channel-{}", Uuid::new_v4().simple());
    let streamId = format!("peer-http-{}", Uuid::new_v4().simple());
    let (batchFrameSender, batchFrameReceiver) = mpsc::unbounded_channel();
    let (batchCloseSender, batchCloseReceiver) = oneshot::channel();
    let batchState = Arc::new(OutboundPeerFrameBatchState {
        session: session.clone(),
        streamId: streamId.clone(),
        closed: AtomicBool::new(false),
        failure: StdMutex::new(None),
        closeSender: StdMutex::new(Some(batchCloseSender)),
    });
    let sender = Arc::new(OutboundPeerFrameSender {
        state: batchState.clone(),
        frameSender: batchFrameSender,
    });
    let senderForWorkerError = sender.clone();
    let connection = PeerConnection::new(
        localNodeId.clone(),
        peerNodeId.clone(),
        channelId.clone(),
        sender,
        core,
        topologyStore,
    );
    let mut pending = PendingPeerConnection(Some(connection.clone()));
    let (frameQueueSender, mut frameQueueReceiver) = mpsc::unbounded_channel::<PeerFrame>();
    let frameQueueSender = Arc::new(StdMutex::new(Some(frameQueueSender)));
    let frameDispatchConnection = connection.clone();
    defaultHostRuntimeTaskSchedulerHost()
        .scheduleHostRuntimeAsyncTask(
            "peer-frame-ordered-receive",
            Box::new(move || {
                Box::pin(async move {
                    while let Some(frame) = frameQueueReceiver.recv().await {
                        if let Err(error) = frameDispatchConnection.receiveFrame(frame).await {
                            frameDispatchConnection
                                .close(format!("Peer Link frame dispatch failed: {error}"));
                            return;
                        }
                    }
                })
            }),
        )
        .map_err(|error| error.to_string())?;
    let body = operit_link::encodeLink(&PeerChannelOpenEnvelope {
        channelId: channelId.clone(),
    })
    .map_err(|error| error.to_string())?;
    let signature = session.sign(&body);
    let buffer = Arc::new(StdMutex::new(Vec::new()));
    let chunkBuffer = buffer.clone();
    let closedConnection = connection.clone();
    let (openedSender, openedReceiver) = oneshot::channel();
    let openedSender = Arc::new(StdMutex::new(Some(openedSender)));
    let openedSignal = openedSender.clone();
    let closedSignal = openedSender.clone();
    let chunkFrameQueueSender = frameQueueSender.clone();
    let closedFrameQueueSender = frameQueueSender.clone();
    let openedLocalNodeId = localNodeId.clone();
    let openedPeerNodeId = peerNodeId.clone();
    let openedChannelId = channelId.clone();
    let chunkLocalNodeId = localNodeId.clone();
    let chunkPeerNodeId = peerNodeId.clone();
    let chunkChannelId = channelId.clone();
    let closedLocalNodeId = localNodeId.clone();
    let closedPeerNodeId = peerNodeId.clone();
    let closedChannelId = channelId.clone();
    let openResult = defaultHttpHost().openHttpByteStream(
        streamId,
        HttpRequestData {
            url: format!("{}/link/peer/channel/events", session.baseUrl()),
            method: "POST".to_string(),
            headers: vec![
                ("x-operit-link-version".to_string(), "4".to_string()),
                ("x-operit-session".to_string(), session.sessionId()),
                ("x-operit-device".to_string(), session.localNodeId()),
                ("x-operit-signature".to_string(), signature),
            ],
            body,
            formFields: Vec::new(),
            fileParts: Vec::new(),
            connectTimeoutSeconds: 10,
            readTimeoutSeconds: 0,
            followRedirects: false,
            ignoreSsl: false,
            proxyHost: String::new(),
            proxyPort: 0,
        },
        Arc::new(move || {
            operit_host_api::tryLogHostConsole(
                2,
                "PeerCarrierTrace",
                &format!(
                    "http_stream_opened local={} peer={} channel={}",
                    openedLocalNodeId, openedPeerNodeId, openedChannelId
                ),
            );
            if let Some(sender) = openedSignal
                .lock()
                .expect("Peer Link open signal lock poisoned")
                .take()
            {
                let _ = sender.send(Ok(()));
            }
        }),
        Arc::new(move |chunk| {
            let frames = decodePeerFrameChunks(&chunkBuffer, chunk)
                .expect("Peer Link frame stream must decode");
            operit_host_api::tryLogHostConsole(
                2,
                "PeerCarrierTrace",
                &format!(
                    "http_stream_chunk local={} peer={} channel={} frames={}",
                    chunkLocalNodeId,
                    chunkPeerNodeId,
                    chunkChannelId,
                    frames.len()
                ),
            );
            let sender = chunkFrameQueueSender
                .lock()
                .expect("Peer Link frame queue lock poisoned");
            let sender = sender
                .as_ref()
                .expect("Peer Link frame queue must remain open while receiving chunks");
            for frame in frames {
                sender
                    .send(frame)
                    .expect("Peer Link ordered frame receiver must remain active");
            }
        }),
        Arc::new(move |result| {
            closedFrameQueueSender
                .lock()
                .expect("Peer Link frame queue lock poisoned")
                .take();
            let reason = match result {
                Ok(()) => "Peer Link stream closed".to_string(),
                Err(error) => error,
            };
            operit_host_api::tryLogHostConsole(
                2,
                "PeerCarrierTrace",
                &format!(
                    "http_stream_closed local={} peer={} channel={} reason={}",
                    closedLocalNodeId, closedPeerNodeId, closedChannelId, reason
                ),
            );
            if let Some(sender) = closedSignal
                .lock()
                .expect("Peer Link close signal lock poisoned")
                .take()
            {
                let _ = sender.send(Err(reason.clone()));
            }
            closedConnection.close(reason);
        }),
    );
    if let Err(error) = openResult {
        frameQueueSender
            .lock()
            .map_err(|lockError| lockError.to_string())?
            .take();
        connection.close(error.to_string());
        return Err(error.to_string());
    }
    openedReceiver
        .await
        .map_err(|error| format!("Peer Link open signal closed: {error}"))??;
    let workerState = batchState.clone();
    defaultHostRuntimeTaskSchedulerHost()
        .scheduleHostRuntimeAsyncTask(
            "peer-frame-batch-send",
            Box::new(move || {
                Box::pin(runOutboundPeerFrameBatchSender(
                    workerState,
                    batchFrameReceiver,
                    batchCloseReceiver,
                ))
            }),
        )
        .map_err(|error| {
            senderForWorkerError.close();
            error.to_string()
        })?;
    let client = registerPeerLink(connection)?;
    pending.0.take();
    Ok(client)
}

/// Decodes complete length-prefixed Peer Link frames from one HTTP stream chunk.
#[allow(non_snake_case)]
fn decodePeerFrameChunks(
    buffer: &Arc<StdMutex<Vec<u8>>>,
    chunk: Vec<u8>,
) -> Result<Vec<PeerFrame>, String> {
    let mut buffer = buffer.lock().map_err(|error| error.to_string())?;
    buffer.extend_from_slice(&chunk);
    let mut frames = Vec::new();
    while buffer.len() >= 4 {
        let frameLength = u32::from_be_bytes(
            buffer[..4]
                .try_into()
                .expect("Peer Link frame prefix must contain four bytes"),
        ) as usize;
        if buffer.len() < 4 + frameLength {
            break;
        }
        let encoded = buffer.drain(..4 + frameLength).collect::<Vec<_>>();
        frames.push(
            operit_link::decodeLink::<PeerFrame>(&encoded[4..])
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(frames)
}

/// Encodes one Peer Link frame with the stream carrier length prefix.
#[allow(non_snake_case)]
pub fn encodePeerFrame(frame: &PeerFrame) -> Result<Vec<u8>, String> {
    let payload = operit_link::encodeLink(frame).map_err(|error| error.to_string())?;
    let length = u32::try_from(payload.len())
        .map_err(|_| "Peer Link frame exceeds u32 length".to_string())?;
    let mut encoded = Vec::with_capacity(4 + payload.len());
    encoded.extend_from_slice(&length.to_be_bytes());
    encoded.extend_from_slice(&payload);
    Ok(encoded)
}
