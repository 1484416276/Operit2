use super::*;
use operit_util::stream::HotStream::MutableSharedStreamImpl;

fn request() -> CoreWatchRequest {
    CoreWatchRequest::new("lifecycle", CORE_INTERNAL_TARGET, "values", CoreValue::Null)
}

fn event(kind: CoreEventKind, value: CoreValue) -> CoreEvent {
    let request = request();
    CoreEvent {
        requestId: Some(request.requestId),
        target: request.target,
        propertyName: request.propertyName,
        kind,
        value,
    }
}

async fn wait_for_subscribers(stream: &MutableSharedStreamImpl<String>, count: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while stream.subscription_count() != count {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }).await.expect("collector registrations must settle without another emitted item");
}

#[tokio::test(flavor = "current_thread")]
async fn reconstructed_state_flow_releases_silent_upstream_on_last_drop() {
    installTestRuntimeScheduler();
    let (sender, upstream) = core_event_stream_channel();
    let (closed, closure) = oneshot::channel();
    let upstream = upstream.withOnClose(move || { let _ = closed.send(()); });
    sender.send(event(CoreEventKind::Snapshot, CoreValue::Unsigned(1))).unwrap();
    let flow = core_state_flow_from_stream::<i32>(upstream).await.unwrap();
    let retained = flow.clone();
    drop(flow);
    assert_eq!(retained.value(), 1);
    drop(retained);
    tokio::time::timeout(Duration::from_secs(2), closure).await
        .expect("last flow drop must cancel even a silent watch").unwrap();
    assert!(sender.is_closed());
}

#[tokio::test(flavor = "current_thread")]
async fn string_watch_drop_removes_only_its_collector() {
    installTestRuntimeScheduler();
    let producer = MutableSharedStreamImpl::<String>::new(8);
    let old_ui = core_string_event_stream(producer.clone(), request());
    let mut runtime_consumer = core_string_event_stream(producer.clone(), request());
    wait_for_subscribers(&producer, 2).await;
    drop(old_ui);
    wait_for_subscribers(&producer, 1).await;
    assert!(producer.try_emit("generation continues".into()));
    let next = tokio::time::timeout(Duration::from_secs(2), runtime_consumer.recv()).await.unwrap().unwrap();
    assert_eq!(next.kind, CoreEventKind::Changed);
    assert_eq!(next.value, CoreValue::String("generation continues".into()));
    producer.close();
    assert_eq!(runtime_consumer.recv().await.unwrap().kind, CoreEventKind::Completed);
    assert!(runtime_consumer.recv().await.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn forwarding_cancels_silent_upstream_when_receiver_closes() {
    installTestRuntimeScheduler();
    let (source, upstream) = core_event_stream_channel();
    let (closed, closure) = oneshot::channel();
    let upstream = upstream.withOnClose(move || { let _ = closed.send(()); });
    let (downstream, receiver) = core_event_stream_channel();
    forward_core_event_stream(upstream, downstream, "test-forward-cancel").unwrap();
    drop(receiver);
    tokio::time::timeout(Duration::from_secs(2), closure).await
        .expect("closed output must release a silent upstream").unwrap();
    assert!(source.is_closed());
}

#[tokio::test(flavor = "current_thread")]
async fn state_watch_keeps_snapshots_and_updates_until_its_receiver_drops() {
    installTestRuntimeScheduler();
    let (sender, upstream) = core_event_stream_channel();
    let (closed, closure) = oneshot::channel();
    let upstream = upstream.withOnClose(move || { let _ = closed.send(()); });
    sender.send(event(CoreEventKind::Snapshot, CoreValue::Unsigned(10))).unwrap();
    let flow = core_state_flow_from_stream::<i32>(upstream).await.unwrap();
    let mut watch = core_route_state_flow_event_stream(flow.clone(), request()).unwrap();
    drop(flow);
    assert_eq!(watch.recv().await.unwrap().value, CoreValue::Unsigned(10));
    sender.send(event(CoreEventKind::Changed, CoreValue::Unsigned(20))).unwrap();
    let changed = tokio::time::timeout(Duration::from_secs(2), watch.recv()).await.unwrap().unwrap();
    assert_eq!(changed.value, CoreValue::Unsigned(20));
    drop(watch);
    tokio::time::timeout(Duration::from_secs(2), closure).await
        .expect("dropping wire subscriber must release its reconstructed StateFlow").unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn resubscribing_string_watch_replays_all_output_without_stopping_generation() {
    installTestRuntimeScheduler();
    let producer = MutableSharedStreamImpl::<String>::new(8);
    let mut first = core_string_event_stream(producer.clone(), request());
    wait_for_subscribers(&producer, 1).await;
    producer.emit("prefix".into());
    assert_eq!(first.recv().await.unwrap().value, CoreValue::String("prefix".into()));
    drop(first);
    wait_for_subscribers(&producer, 0).await;
    producer.emit("while away".into());
    let mut reopened = core_string_event_stream(producer.clone(), request());
    wait_for_subscribers(&producer, 1).await;
    producer.emit("suffix".into());
    producer.close();
    for expected in ["prefix", "while away", "suffix"] {
        assert_eq!(reopened.recv().await.unwrap().value, CoreValue::String(expected.into()));
    }
    assert_eq!(reopened.recv().await.unwrap().kind, CoreEventKind::Completed);
    assert!(reopened.recv().await.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn state_watch_preserves_incremental_deltas_and_normal_completion() {
    installTestRuntimeScheduler();
    let (sender, upstream) = core_event_stream_channel();
    let (closed, closure) = oneshot::channel();
    let upstream = upstream.withOnClose(move || { let _ = closed.send(()); });
    let initial = BTreeMap::from([("stable".to_string(), "x".repeat(2048)), ("text".into(), "prefix".into())]);
    let mut final_value = initial.clone();
    final_value.insert("text".into(), "prefix and final suffix".into());
    let initial_wire = to_core_value(initial.clone()).unwrap();
    let mut previous = Some(initial_wire.clone());
    let (kind, delta) = CoreValue::incrementalEvent(&mut previous, to_core_value(final_value.clone()).unwrap());
    assert_eq!(kind, CoreEventKind::Delta);
    sender.send(event(CoreEventKind::Snapshot, initial_wire)).unwrap();
    let state = core_state_flow_from_stream::<BTreeMap<String, String>>(upstream).await.unwrap();
    let mut watch = core_route_state_flow_event_stream(state.clone(), request()).unwrap();
    watch.recv().await.unwrap();
    sender.send(event(kind, delta)).unwrap();
    tokio::time::timeout(Duration::from_secs(2), watch.recv()).await.unwrap().unwrap();
    assert_eq!(state.value(), final_value);
    sender.send(event(CoreEventKind::Completed, CoreValue::Null)).unwrap();
    tokio::time::timeout(Duration::from_secs(2), closure).await.unwrap().unwrap();
    // Ending the collection must not erase the final saved snapshot.
    assert_eq!(state.value(), final_value);
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_state_watch_rebinding_releases_every_previous_collection() {
    installTestRuntimeScheduler();
    for _ in 0..25 {
        let (sender, upstream) = core_event_stream_channel();
        let (closed, closure) = oneshot::channel();
        let upstream = upstream.withOnClose(move || { let _ = closed.send(()); });
        sender.send(event(CoreEventKind::Snapshot, CoreValue::Unsigned(1))).unwrap();
        let state = core_state_flow_from_stream::<i32>(upstream).await.unwrap();
        let mut watch = core_route_state_flow_event_stream(state, request()).unwrap();
        assert_eq!(watch.recv().await.unwrap().value, CoreValue::Unsigned(1));
        drop(watch);
        tokio::time::timeout(Duration::from_secs(2), closure).await
            .expect("each previous watch must finish before another notification").unwrap();
        assert!(sender.is_closed());
    }
}
