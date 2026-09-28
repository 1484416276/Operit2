use super::*;
use operit_edge_transport::{
    finishPairAsClient, linkTokenHash, startPairAsClient, AuthenticatedLinkChannel, LinkChannel,
};
use operit_link::*;

async fn peerFrame(channel: &Arc<AuthenticatedLinkChannel>) -> PeerFrame {
    let frame = tokio::time::timeout(std::time::Duration::from_secs(3), channel.receive())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match frame.payload {
        LinkFramePayload::PeerFrame(frame) => frame,
        other => panic!("unexpected {other:?}"),
    }
}
async fn response(channel: &Arc<AuthenticatedLinkChannel>, id: String, payload: PeerFramePayload) {
    channel
        .send(LinkFrame {
            messageId: id.clone(),
            payload: LinkFramePayload::PeerFrame(PeerFrame {
                messageId: id,
                payload,
            }),
        })
        .await
        .unwrap();
}

// Exercise real TCP, crypto pairing, the firmware session entry and firmware chat
// UI. The adjacent Core wire fixture only checks routed requests and emits events;
// CoreNodeRouter's own integration tests cover Binding resolution and execution.
#[tokio::test]
async fn firmware_pairs_routes_chat_and_reconnects_with_persisted_identity() {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let dir = std::env::temp_dir().join(format!("operit-simulator-test-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        let store = Arc::new(FileStore(dir.join("pairing.json")));
        let code = Arc::new(Mutex::new(String::new()));
        let authority = Arc::new(EdgePairingAuthority::newWithStore("test-token", "test-edge", LinkDeviceInfo {
            platform: "esp32".into(), model: "test".into(),
        }, store.clone(), { let code = code.clone(); move |value| *code.lock().unwrap() = value }).unwrap());
        let listener = TcpLinkChannel::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            edge_session::handleChannel(authority, TcpLinkChannel::fromStream(stream)).await
        });
        let channel = TcpLinkChannel::connect(&address).await.unwrap();
        let start = startPairAsClient(channel.clone(), linkTokenHash("test-token"), "test-core".into(),
            LinkDeviceInfo { platform: "test".into(), model: "core".into() }).await.unwrap();
        let pairCode = code.lock().unwrap().clone();
        assert_eq!(pairCode.len(), 6);
        let session = finishPairAsClient(channel.clone(), start, pairCode).await.unwrap();
        assert_eq!(store.load().unwrap().unwrap().sessions.len(), 1);
        let authenticated = AuthenticatedLinkChannel::new(channel, session.clone());
        let context = || LinkFrame {messageId: "context".into(), payload: LinkFramePayload::SpaceContext {
            spaceId: "test-space".into(), adjacentNodeId: "test-core".into(), ttl: 3, chatId: "edge-chat-test-edge".into(),
        }};
        authenticated.send(context()).await.unwrap();
        let frame = peerFrame(&authenticated).await;
        let PeerFramePayload::Request(PeerRequest::WatchOpen(watch)) = frame.payload else { panic!("expected watch") };
        assert_eq!(watch.request.routeKind, RoutedCoreRequestKind::SpaceBinding);
        assert_eq!(watch.request.payload.propertyName, "chatMessagesFlow");
        response(&authenticated, frame.messageId, PeerFramePayload::Response(PeerResponse::Operation(Ok(())))).await;
        let frame = peerFrame(&authenticated).await;
        let PeerFramePayload::Request(PeerRequest::WatchOpen(histories)) = frame.payload else { panic!("expected histories watch") };
        assert_eq!(histories.request.payload.propertyName, "routedChatListFlow");
        response(&authenticated, frame.messageId, PeerFramePayload::Response(PeerResponse::Operation(Ok(())))).await;
        response(&authenticated, "histories".into(), PeerFramePayload::WatchEvent(PeerWatchEvent {
            subscriptionId: histories.subscriptionId,
            event: CoreEvent { requestId: None, target: CORE_INTERNAL_TARGET.into(),
                propertyName: "routedChatListFlow".into(), kind: CoreEventKind::Snapshot,
                value: toCoreValue(serde_json::json!([
                    {"id":"edge-chat-test-edge", "title":"First", "characterCardName":"Assistant"},
                    {"id":"second-chat", "title":"Second", "characterCardName":"Assistant"}
                ])).unwrap(),
            },
        })).await;
        response(&authenticated, "messages".into(), PeerFramePayload::WatchEvent(PeerWatchEvent {
            subscriptionId: watch.subscriptionId,
            event: CoreEvent { requestId: None, target: CORE_INTERNAL_TARGET.into(),
                propertyName: "chatMessagesFlow".into(), kind: CoreEventKind::Snapshot,
                value: toCoreValue(serde_json::json!([{"sender":"ai","parts":[{"kind":"markdown","content":"routed reply"}]}])).unwrap(),
            },
        })).await;
        loop {
            if edge_chat::snapshot()["messages"][0]["text"] == "routed reply" { break; }
            tokio::task::yield_now().await;
        }
        edge_chat::send("hello from firmware".into()).unwrap();
        let frame = peerFrame(&authenticated).await;
        let PeerFramePayload::Request(PeerRequest::Call(call)) = frame.payload else { panic!("expected call") };
        assert_eq!(call.routeKind, RoutedCoreRequestKind::SpaceBinding);
        assert_eq!(call.targetNodeId, "test-core");
        assert_eq!(call.payload.methodName, "sendUserMessage");
        let args = serde_json::to_value(&call.payload.args).unwrap();
        assert_eq!(args["chatIdOverride"], "edge-chat-test-edge");
        assert_eq!(args["messageText"], "hello from firmware");
        response(&authenticated, frame.messageId, PeerFramePayload::Response(PeerResponse::Call(
            CoreCallResponse::err(call.payload.requestId, CoreLinkError::new("RUNTIME_EXECUTION_DENIED", "executor denied"))
        ))).await;
        loop {
            if edge_chat::snapshot()["error"].as_str().unwrap_or("").contains("executor denied") { break; }
            tokio::task::yield_now().await;
        }
        authenticated.close().await;
        task.await.unwrap().unwrap();
        assert_eq!(edge_chat::snapshot()["connected"], false);
        // Recreate authority from persisted state; reconnect without a pairing code.
        let restored = Arc::new(EdgePairingAuthority::newWithStore("test-token", "test-edge", LinkDeviceInfo {
            platform: "esp32".into(), model: "test".into(),
        }, store.clone(), |_| panic!("reconnect must not pair again")).unwrap());
        let listener = TcpLinkChannel::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            edge_session::handleChannel(restored, TcpLinkChannel::fromStream(stream)).await
        });
        let channel = TcpLinkChannel::connect(&address).await.unwrap();
        let authenticated = AuthenticatedLinkChannel::new(channel, session);
        authenticated.send(context()).await.unwrap();
        let frame = peerFrame(&authenticated).await;
        assert!(matches!(frame.payload, PeerFramePayload::Request(PeerRequest::WatchOpen(_))));
        assert_eq!(edge_chat::snapshot()["chatId"], "edge-chat-test-edge");
        authenticated.close().await;
        task.await.unwrap().unwrap();
        std::fs::remove_file(store.0.clone()).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }).await.expect("TCP pairing/chat/reconnect test timed out");
}
