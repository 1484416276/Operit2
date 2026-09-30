use super::*;
use operit_peer_link::{finishPairAsClient, linkTokenHash, startPairAsClient, AuthenticatedLinkChannel};
use operit_peer_link::transport::LinkChannel;
use operit_link::*;

async fn peerFrame(channel: &Arc<AuthenticatedLinkChannel>) -> PeerFrame {
    loop {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(3), channel.receive())
            .await.unwrap().unwrap().unwrap();
        let frame = match frame.payload {
            LinkFramePayload::PeerFrame(frame) => frame,
            other => panic!("unexpected {other:?}"),
        };
        if let PeerFramePayload::Heartbeat(PeerHeartbeat::Probe { sequence, sentAt }) = frame.payload {
            response(channel, frame.messageId, PeerFramePayload::Heartbeat(PeerHeartbeat::Ack { sequence, sentAt })).await;
            continue;
        }
        // Theme polling is independent of the chat/image sequence under test.
        if let PeerFramePayload::Request(PeerRequest::Call(call)) = &frame.payload {
            if call.payload.methodName == "ensureRoutedChat" {
                assert_eq!(call.originNodeId, "test-edge");
                response(channel, frame.messageId.clone(), PeerFramePayload::Response(PeerResponse::Call(
                    CoreCallResponse::ok(call.payload.requestId.clone(), operit_link::CoreValue::Null)
                ))).await;
                continue;
            }
            if call.payload.methodName == "themeSnapshot" {
                response(channel, frame.messageId.clone(), PeerFramePayload::Response(PeerResponse::Call(
                    CoreCallResponse::ok(call.payload.requestId.clone(),
                        toCoreValue(serde_json::json!({"mode":"system"})).unwrap())
                ))).await;
                continue;
            }
        }
        return frame;
    }
}
async fn admit(channel: &Arc<AuthenticatedLinkChannel>, context: LinkFrame) {
    channel.send(context).await.unwrap();

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
    operit_host_api::HostManager::setDefaultHostRuntimeTaskSchedulerHost(Arc::new(
        operit_host_native_scheduler::LocalHostRuntimeTaskSchedulerHost::new().unwrap()));

    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let dir = std::env::temp_dir().join(format!("operit-simulator-test-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        let store = Arc::new(FileStore(dir.join("pairing.json")));
        let code = Arc::new(Mutex::new(String::new()));
        let authority = Arc::new(PairingAuthority::newWithStore("test-token", "test-edge", LinkDeviceInfo {
            platform: "esp32".into(), model: "test".into(),
        }, store.clone(), { let code = code.clone(); move |value| *code.lock().unwrap() = value }).unwrap());
        let listener = operit_host_api::TcpHost::bind(&operit_host_native_common::NativeTcpHost, "127.0.0.1:0").await.unwrap();
        let address = listener.local_address().unwrap().to_string();
        let task = tokio::spawn(async move {
            let stream = listener.accept().await.unwrap();
            edge_session::handleChannel(authority, TcpLinkChannel::fromConnection(stream)).await
        });
        let channel = TcpLinkChannel::fromConnection(operit_host_api::TcpHost::connect(&operit_host_native_common::NativeTcpHost, &address).await.unwrap());
        let start = startPairAsClient(channel.clone(), linkTokenHash("test-token"), "test-core".into(),
            LinkDeviceInfo { platform: "test".into(), model: "core".into() }).await.unwrap();
        let pairCode = code.lock().unwrap().clone();
        assert_eq!(pairCode.len(), 6);
        let session = finishPairAsClient(channel.clone(), start, pairCode).await.unwrap();
        assert_eq!(store.load().unwrap().unwrap().sessions.len(), 1);
        let authenticated = AuthenticatedLinkChannel::new(channel, session.clone());
        let context = || LinkFrame {messageId: "context".into(), payload: LinkFramePayload::SpaceContext {
            spaceId: "test-space".into(), adjacentNodeId: "test-core".into(), ttl: 3,
        }};
        admit(&authenticated, context()).await;
        let frame = peerFrame(&authenticated).await;
        let PeerFramePayload::Request(PeerRequest::WatchOpen(watch)) = frame.payload else { panic!("expected watch") };
        assert_eq!(watch.request.routeKind, RoutedCoreRequestKind::SpaceBinding);
        assert_eq!(watch.request.payload.propertyName, "edgeChatMessagesFlow");
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
            subscriptionId: watch.subscriptionId.clone(),
            event: CoreEvent { requestId: None, target: CORE_INTERNAL_TARGET.into(),
                propertyName: "edgeChatMessagesFlow".into(), kind: CoreEventKind::Snapshot,
                value: toCoreValue(serde_json::json!([{"sender":"ai","parts":[{"kind":"markdown","content":"routed reply"}]}])).unwrap(),
            },
        })).await;
        loop {
            if edge_chat::snapshot()["messages"][0]["text"] == "routed reply" { break; }
            tokio::task::yield_now().await;
        }
        // PNG bytes travel as a Link binary value; Core returns its normal
        // media link, which Edge sends through the existing chat route.
        let _ = edge_chat::takeSendResult();
        let image_bytes = vec![137,80,78,71,13,10,26,10,0,0,0,0];
        edge_chat::sendImage(image_bytes.clone(), "image/png".into()).unwrap();
        let frame = peerFrame(&authenticated).await;
        let PeerFramePayload::Request(PeerRequest::Call(register)) = frame.payload else { panic!("expected image registration") };
        assert_eq!(register.payload.methodName,"registerChatImage");
        let operit_link::CoreValue::Map(fields) = &register.payload.args else { panic!("expected binary image args") };
        assert_eq!(fields.get("imageBytes"),Some(&operit_link::CoreValue::Bytes(image_bytes)));
        assert_eq!(fields.get("mimeType"),Some(&operit_link::CoreValue::String("image/png".into())));
        let image_link = "<link type=\"image\" id=\"image-test-1\"></link>";
        response(&authenticated,frame.messageId,PeerFramePayload::Response(PeerResponse::Call(
            CoreCallResponse::ok(register.payload.requestId,operit_link::CoreValue::String(image_link.into()))
        ))).await;
        let frame=peerFrame(&authenticated).await;
        let PeerFramePayload::Request(PeerRequest::Call(send))=frame.payload else {panic!("expected image chat send")};
        assert_eq!(send.payload.methodName,"sendUserMessage");
        assert_eq!(serde_json::to_value(&send.payload.args).unwrap()["messageText"],image_link);
        response(&authenticated,frame.messageId,PeerFramePayload::Response(PeerResponse::Call(
            CoreCallResponse::ok(send.payload.requestId,operit_link::CoreValue::Null)
        ))).await;
        loop { if matches!(edge_chat::takeSendResult(),Some(Ok(()))) { break; } tokio::task::yield_now().await; }
        response(&authenticated,"messages-image".into(),PeerFramePayload::WatchEvent(PeerWatchEvent {
            subscriptionId:watch.subscriptionId.clone(),
            event:CoreEvent {requestId:None,target:CORE_INTERNAL_TARGET.into(),
                propertyName:"edgeChatMessagesFlow".into(),kind:CoreEventKind::Snapshot,
                value:toCoreValue(serde_json::json!([{"sender":"user","parts":[{"kind":"markdown","content":image_link}]}])).unwrap()},
        })).await;
        loop { if edge_chat::snapshot()["messages"][0]["images"][0]=="image-test-1" { break; } tokio::task::yield_now().await; }
        edge_chat::openImage("77:image-test-1").unwrap();
        let frame=peerFrame(&authenticated).await;
        let PeerFramePayload::Request(PeerRequest::Call(preview))=frame.payload else {panic!("expected preview chunk")};
        assert_eq!(preview.payload.methodName,"chatImagePreviewChunk");
        let preview_args=serde_json::to_value(&preview.payload.args).unwrap();
        assert_eq!(preview_args["width"],128);
        assert_eq!(preview_args["height"],96);
        assert_eq!(preview_args["format"],"rgb565le");
        assert_eq!(preview_args["chunkSize"],1024);
        assert_eq!(preview_args["imageId"],"image-test-1"); assert_eq!(preview_args["offset"],0);
        let chunk=operit_edge_contract::image_preview::ImagePreviewChunk {width:2,height:1,offset:0,bytes:vec![0,248,224,7]}.into_value();
        response(&authenticated,frame.messageId,PeerFramePayload::Response(PeerResponse::Call(
            CoreCallResponse::ok(preview.payload.requestId,chunk)
        ))).await;
        loop { if let Some(event)=edge_image::take() { assert_eq!(event.request,77);
            let image=event.chunk.unwrap(); assert_eq!(image.bytes,[0,248,224,7]); break; }
            tokio::task::yield_now().await; }
        edge_image::cancel();
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
        let restored = Arc::new(PairingAuthority::newWithStore("test-token", "test-edge", LinkDeviceInfo {
            platform: "esp32".into(), model: "test".into(),
        }, store.clone(), |code| assert!(code.is_empty(), "reconnect must not pair again")).unwrap());
        let authorityOwner = restored.clone();
        let listener = operit_host_api::TcpHost::bind(&operit_host_native_common::NativeTcpHost, "127.0.0.1:0").await.unwrap();
        let address = listener.local_address().unwrap().to_string();
        let task = tokio::spawn(async move {
            let stream = listener.accept().await.unwrap();
            edge_session::handleChannel(restored, TcpLinkChannel::fromConnection(stream)).await
        });
        let channel = TcpLinkChannel::fromConnection(operit_host_api::TcpHost::connect(&operit_host_native_common::NativeTcpHost, &address).await.unwrap());
        let authenticated = AuthenticatedLinkChannel::new(channel, session.clone());
        admit(&authenticated, context()).await;
        let frame = peerFrame(&authenticated).await;
        assert!(matches!(frame.payload, PeerFramePayload::Request(PeerRequest::WatchOpen(_))));
        assert_eq!(edge_chat::snapshot()["chatId"], "edge-chat-test-edge");
        // Local cancellation notifies the already authenticated live Core.
        authorityOwner.clearPairings().unwrap();
        loop {
            let frame = authenticated.receive().await.unwrap().unwrap();
            if let LinkFramePayload::Close { code, .. } = frame.payload {
                assert_eq!(code, operit_peer_link::pairing::PAIRING_REVOKED);
                break;
            }
        }
        task.await.unwrap().unwrap();
        // Cancellation persists a signed receipt without retaining the secret.
        assert!(!authorityOwner.hasPairings());
        assert!(store.load().unwrap().unwrap().sessions.is_empty());
        let cancelled = Arc::new(PairingAuthority::newWithStore("test-token", "test-edge", LinkDeviceInfo {
            platform: "esp32".into(), model: "test".into(),
        }, store.clone(), |_| {}).unwrap());
        assert!(!cancelled.hasPairings());
        let listener = operit_host_api::TcpHost::bind(&operit_host_native_common::NativeTcpHost, "127.0.0.1:0").await.unwrap();
        let address = listener.local_address().unwrap().to_string();
        let task = tokio::spawn(async move {
            let stream = listener.accept().await.unwrap();
            edge_session::handleChannel(cancelled, TcpLinkChannel::fromConnection(stream)).await
        });
        let channel = TcpLinkChannel::fromConnection(operit_host_api::TcpHost::connect(&operit_host_native_common::NativeTcpHost, &address).await.unwrap());
        let revoked = AuthenticatedLinkChannel::new(channel, session.clone());
        revoked.send(context()).await.unwrap();
        let receipt = revoked.receive().await.unwrap().unwrap();
        assert!(matches!(receipt.payload, LinkFramePayload::Close { code, .. }
            if code == operit_peer_link::pairing::PAIRING_REVOKED));
        task.await.unwrap().unwrap();
        std::fs::remove_file(store.0.clone()).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }).await.expect("TCP pairing/chat/reconnect test timed out");
}
