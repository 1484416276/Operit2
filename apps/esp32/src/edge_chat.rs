//! Volatile UI state; all chat execution and persistence belongs to Space.
#![allow(non_snake_case)]
use operit_link::{
    CoreCallRequest, CoreEventKind, CoreValue, CoreWatchRequest, CORE_INTERNAL_TARGET,
};
use operit_link::CoreLinkSharedClient;
use operit_node_runtime::NodeServices::NodeServices;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) struct NodeUiTask(tokio::sync::watch::Sender<bool>);
impl NodeUiTask {
    pub fn abort(&self) { let _ = self.0.send(true); }
}
pub(crate) fn spawnNodeUiTask<F>(task: impl FnOnce() -> F + Send + 'static) -> NodeUiTask
where F: std::future::Future<Output = ()> + 'static {
    let (cancel, mut cancelled) = tokio::sync::watch::channel(false);
    operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost()
        .scheduleHostRuntimeAsyncTask("edge-ui", Box::new(move || Box::pin(async move {
            tokio::select! {
                _ = async {
                    if cancelled.changed().await.is_err() {
                        std::future::pending::<()>().await;
                    }
                } => {},
                _ = task() => {}
            }
        }))).expect("Edge UI task must be scheduled by Host");
    NodeUiTask(cancel)
}

struct ChatSession {
    client: Arc<dyn CoreLinkSharedClient + Send + Sync>,
    services: NodeServices,
    histories: Mutex<CoreValue>,
    tasks: Mutex<Vec<NodeUiTask>>,
    chatId: String,
    messages: Mutex<CoreValue>,
    error: Mutex<Option<String>>,
    sending: AtomicBool,
    sendResult: Mutex<Option<Result<(), String>>>,
    streams: Mutex<BTreeMap<String, StreamText>>,
    streamTasks: Mutex<BTreeMap<String, NodeUiTask>>,
}

fn isMissingSpaceRoute(error: &operit_link::CoreLinkError) -> bool {
    let text = error.to_string();
    text.contains("SPACE_ROUTE_NOT_FOUND") || text.contains("route is not registered")
}

async fn watchChatMessages(
    client: &Arc<dyn CoreLinkSharedClient + Send + Sync>,
    chatId: &str,
) -> Result<operit_link::CoreEventStream, operit_link::CoreLinkError> {
    let args = operit_link::toCoreValue(serde_json::json!({"chatId": chatId}))
        .map_err(|error| operit_link::CoreLinkError::new("INVALID_ARGS", error.to_string()))?;
    match client
        .watch(CoreWatchRequest::new(
            "edge-ui-messages",
            CORE_INTERNAL_TARGET,
            "edgeChatMessagesFlow",
            args,
        ))
        .await
    {
        Ok(stream) => Ok(stream),
        Err(error) if isMissingSpaceRoute(&error) => Err(operit_link::CoreLinkError::new(
            "EDGE_CHAT_UNSUPPORTED",
            "当前 Core 不支持轻量聊天接口，请升级 Core 后重新连接",
        )),
        Err(error) => Err(error),
    }
}

#[derive(Default, Clone)]
struct StreamText {
    text: String,
    completed: bool,
    savepoints: BTreeMap<String, String>,
}

impl StreamText {
    fn apply(&mut self, event: &serde_json::Value) {
        if !event
            .get("parentBlockId")
            .unwrap_or(&serde_json::Value::Null)
            .is_null()
        {
            return;
        }
        match event.get("type").and_then(|v| v.as_str()) {
            Some("reset") => {
                self.text.clear();
                self.savepoints.clear();
            }
            Some("chunk") => appendBounded(
                &mut self.text,
                event.get("value").and_then(|v| v.as_str()).unwrap_or(""),
                MAX_CHAT_STRING_BYTES,
            ),
            Some("savepoint") => {
                if let Some(id) = event.get("id").and_then(|v| v.as_str()) {
                    if self.savepoints.len() >= 4 {
                        self.savepoints.clear();
                    }
                    self.savepoints.insert(id.into(), self.text.clone());
                }
            }
            Some("rollback") => {
                if let Some(text) = event
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|id| self.savepoints.get(id))
                {
                    self.text = text.clone();
                }
            }
            _ => {}
        }
    }
}

const MAX_STREAM_DESCRIPTORS: usize = 8;
const MAX_STREAM_SCAN_DEPTH: usize = 8;
const MAX_STREAM_SCAN_ITEMS: usize = 32;

fn collectMessageStreams(value: &CoreValue, streams: &mut Vec<operit_link::CoreStreamDescriptor>) {
    collectMessageStreamsBounded(value, streams, 0);
}

fn collectMessageStreamsBounded(
    value: &CoreValue,
    streams: &mut Vec<operit_link::CoreStreamDescriptor>,
    depth: usize,
) {
    if depth > MAX_STREAM_SCAN_DEPTH || streams.len() >= MAX_STREAM_DESCRIPTORS {
        return;
    }
    match value {
        CoreValue::List(items) => {
            for item in items.iter().take(MAX_STREAM_SCAN_ITEMS) {
                collectMessageStreamsBounded(item, streams, depth + 1);
            }
        }
        CoreValue::Map(fields) => {
            if let Some(CoreValue::Map(content_stream)) = fields.get("contentStream") {
                if let Some(CoreValue::Map(descriptor)) = content_stream.get("$coreStream") {
                    let streamId = match descriptor.get("streamId") {
                        Some(CoreValue::String(value)) => value.clone(),
                        _ => String::new(),
                    };
                    let target = match descriptor.get("target") {
                        Some(CoreValue::String(value)) => value.clone(),
                        _ => String::new(),
                    };
                    let propertyName = match descriptor.get("propertyName") {
                        Some(CoreValue::String(value)) => value.clone(),
                        _ => String::new(),
                    };
                    let args = descriptor
                        .get("args")
                        .cloned()
                        .unwrap_or_else(CoreValue::emptyMap);
                    if !streamId.is_empty() && !propertyName.is_empty() {
                        streams.push(operit_link::CoreStreamDescriptor {
                            streamId,
                            target,
                            propertyName,
                            args,
                        });
                    }
                }
            }
            for item in fields.values().take(MAX_STREAM_SCAN_ITEMS) {
                collectMessageStreamsBounded(item, streams, depth + 1);
            }
        }
        _ => {}
    }
}

fn openMessageStreams(session: &Arc<ChatSession>, messages: &CoreValue) {
    let mut descriptors = Vec::new();
    collectMessageStreams(messages, &mut descriptors);
    let active = descriptors
        .iter()
        .map(|descriptor| descriptor.streamId.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    session.streamTasks.lock().unwrap().retain(|id, task| {
        if active.contains(id.as_str()) {
            true
        } else {
            task.abort();
            false
        }
    });
    session
        .streams
        .lock()
        .unwrap()
        .retain(|id, _| active.contains(id.as_str()));
    for descriptor in descriptors {
        {
            let mut streams = session.streams.lock().unwrap();
            if streams.contains_key(&descriptor.streamId) {
                continue;
            }
            streams.insert(descriptor.streamId.clone(), StreamText::default());
        }
        let streamId = descriptor.streamId.clone();
        let owner = session.clone();
        let session = session.clone();
        let task = spawnNodeUiTask(move || async move {
            let mut args = match descriptor.args {
                CoreValue::Map(args) => args,
                _ => BTreeMap::new(),
            };
            args.insert(
                "streamId".into(),
                CoreValue::String(descriptor.streamId.clone()),
            );
            args.insert(
                operit_link::CORE_ROUTE_STREAM_SOURCE_METHOD_ARGUMENT.into(),
                // Reopen through the projection so XML and tool payloads stay on Core.
                CoreValue::String("edgeChatMessagesFlow".into()),
            );
            args.insert(
                operit_link::CORE_ROUTE_STREAM_SOURCE_MODE_ARGUMENT.into(),
                CoreValue::String("watch".into()),
            );
            args.insert(
                operit_link::CORE_ROUTE_STREAM_SOURCE_ARGS_ARGUMENT.into(),
                operit_link::toCoreValue(serde_json::json!({"chatId": session.chatId})).unwrap(),
            );
            let result = session
                .client
                .watch(CoreWatchRequest::new(
                    operit_link::nextCoreRouteRequestId("openCoreStream"),
                    descriptor.target,
                    descriptor.propertyName,
                    CoreValue::Map(args),
                ))
                .await;
            match result {
                Ok(mut stream) => {
                    while let Some(event) = stream.recv().await {
                        if event.kind == CoreEventKind::Completed {
                            break;
                        }
                        if let Some(text) = session
                            .streams
                            .lock()
                            .unwrap()
                            .get_mut(&descriptor.streamId)
                        {
                            text.apply(&serde_json::to_value(event.value).unwrap_or_default());
                            UI_REVISION.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                Err(error) => {
                    *session.error.lock().unwrap() = Some(error.to_string());
                    UI_REVISION.fetch_add(1, Ordering::Relaxed);
                }
            }
            if let Some(text) = session
                .streams
                .lock()
                .unwrap()
                .get_mut(&descriptor.streamId)
            {
                text.completed = true;
            }
        });
        owner.streamTasks.lock().unwrap().insert(streamId, task);
    }
}

static UI_REVISION: AtomicU32 = AtomicU32::new(1);
static SESSION: OnceLock<Mutex<Option<Arc<ChatSession>>>> = OnceLock::new();
const MAX_CHAT_MESSAGES: usize = 12;
const MAX_CONVERSATIONS: usize = 24;
const MAX_CHAT_STRING_BYTES: usize = 1536;
const MAX_TOOL_NAME_BYTES: usize = 96;
const MAX_PART_TEXT_BYTES: usize = 768;
const MAX_NESTED_LIST_ITEMS: usize = 64;
const MAX_CORE_BYTES: usize = 1024;
const MAX_CORE_VALUE_DEPTH: usize = 16;

fn boundedText(value: &str) -> String {
    if value.len() <= MAX_CHAT_STRING_BYTES {
        return value.to_string();
    }
    let mut end = MAX_CHAT_STRING_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn appendBounded(target: &mut String, value: &str, limit: usize) {
    if target.len() >= limit {
        return;
    }
    let remaining = limit - target.len();
    let mut end = value.len().min(remaining);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    target.push_str(&value[..end]);
}

fn visibleEdgeText(source: &str) -> String {
    let mut result = String::new();
    let mut rest = source;
    while !rest.is_empty() && result.len() < MAX_CHAT_STRING_BYTES {
        let Some(open) = rest.find('<') else {
            appendBounded(&mut result, rest, MAX_CHAT_STRING_BYTES);
            break;
        };
        appendBounded(&mut result, &rest[..open], MAX_CHAT_STRING_BYTES);
        rest = &rest[open..];
        let Some(close) = rest.find('>') else {
            break;
        };
        let tag = &rest[1..close];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        if name == "link" && (tag.contains("type=\"image\"") || tag.contains("type='image'")) {
            let after_tag = &rest[close + 1..];
            let Some(end) = after_tag.find("</link>") else {
                break;
            };
            appendBounded(&mut result, &rest[..=close], MAX_CHAT_STRING_BYTES);
            appendBounded(&mut result, "</link>", MAX_CHAT_STRING_BYTES);
            rest = &after_tag[end + "</link>".len()..];
            continue;
        }
        if name == "tool" && !tag.starts_with('/') {
            let tool = tag
                .split_once("name=\"")
                .and_then(|(_, tail)| tail.split_once('"').map(|(name, _)| name))
                .or_else(|| {
                    tag.split_once("name='")
                        .and_then(|(_, tail)| tail.split_once('\'').map(|(name, _)| name))
                });
            if let Some(tool) = tool.filter(|name| !name.is_empty()) {
                if !result.is_empty() {
                    appendBounded(&mut result, "\n", MAX_CHAT_STRING_BYTES);
                }
                appendBounded(&mut result, "调用工具：", MAX_CHAT_STRING_BYTES);
                appendBounded(&mut result, tool, MAX_CHAT_STRING_BYTES);
            }
        }
        rest = &rest[close + 1..];
        if !tag.starts_with('/') && !name.is_empty() {
            let closing = format!("</{name}>");
            if let Some(end) = rest.find(&closing) {
                rest = &rest[end + closing.len()..];
            } else if !tag.trim_end().ends_with('/') {
                break;
            }
        }
    }
    result
}

/// Bounds payload fields before they are retained by the long-lived chat state.
/// Collection shape is preserved so subsequent Core deltas still address valid paths.
fn boundCoreValue(value: &mut CoreValue, depth: usize) {
    if depth > MAX_CORE_VALUE_DEPTH {
        *value = CoreValue::Null;
        return;
    }
    match value {
        CoreValue::String(text) => {
            if text.len() > MAX_CHAT_STRING_BYTES {
                let bounded = boundedText(text);
                *text = bounded;
            }
        }
        CoreValue::Bytes(bytes) => bytes.truncate(MAX_CORE_BYTES),
        CoreValue::List(items) => {
            for item in items {
                boundCoreValue(item, depth + 1);
            }
        }
        CoreValue::Map(fields) => {
            for item in fields.values_mut() {
                boundCoreValue(item, depth + 1);
            }
        }
        _ => {}
    }
}

fn mapString<'a>(fields: &'a BTreeMap<String, CoreValue>, key: &str) -> &'a str {
    match fields.get(key) {
        Some(CoreValue::String(value)) => value,
        _ => "",
    }
}

fn simpleJson(value: Option<&CoreValue>) -> serde_json::Value {
    match value {
        Some(CoreValue::String(value)) => boundedText(value).into(),
        Some(CoreValue::Bool(value)) => (*value).into(),
        Some(CoreValue::Signed(value)) => (*value).into(),
        Some(CoreValue::Unsigned(value)) => (*value).into(),
        Some(CoreValue::Float(value)) => (*value).into(),
        _ => serde_json::Value::Null,
    }
}

/// Called on the authenticated Link runtime after Space provisions the chat.
pub fn install(client: Arc<dyn CoreLinkSharedClient + Send + Sync>, services: NodeServices, chatId: String) {
    clear();
    let session = Arc::new(ChatSession {
        client,
        services,
        chatId,
        histories: Mutex::new(CoreValue::List(Vec::new())),
        tasks: Mutex::new(Vec::new()),
        messages: Mutex::new(CoreValue::List(Vec::new())),
        error: Mutex::new(None),
        sending: AtomicBool::new(false),
        sendResult: Mutex::new(None),
        streams: Mutex::new(BTreeMap::new()),
        streamTasks: Mutex::new(BTreeMap::new()),
    });
    *SESSION.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(session.clone());
    let owner = session.clone();
    let historySession = session.clone();
    let task = spawnNodeUiTask(move || async move {
        let result = watchChatMessages(&session.client, &session.chatId).await;
        match result {
            Ok(mut stream) => {
                while let Some(event) = stream.recv().await {
                    if event.kind == CoreEventKind::Completed {
                        break;
                    }
                    let mut messages = session.messages.lock().unwrap();
                    if event.kind == CoreEventKind::Delta {
                        match messages.applyIncrementalDelta(&event.value) {
                            Ok(mut value) => {
                                boundCoreValue(&mut value, 0);
                                *messages = value;
                            }
                            Err(error) => {
                                *session.error.lock().unwrap() = Some(error);
                                UI_REVISION.fetch_add(1, Ordering::Relaxed);
                                break;
                            }
                        }
                    } else {
                        let mut value = event.value;
                        boundCoreValue(&mut value, 0);
                        *messages = value;
                    }
                    openMessageStreams(&session, &messages);
                    UI_REVISION.fetch_add(1, Ordering::Relaxed);
                }
                *session.error.lock().unwrap() = Some("聊天连接已断开，请等待重新连接".into());
                UI_REVISION.fetch_add(1, Ordering::Relaxed);
                for (_, task) in std::mem::take(&mut *session.streamTasks.lock().unwrap()) {
                    task.abort();
                }
            }
            Err(error) => {
                *session.error.lock().unwrap() = Some(error.to_string());
                UI_REVISION.fetch_add(1, Ordering::Relaxed);
            }
       }
    });
    owner.tasks.lock().unwrap().push(task);
    let task = spawnNodeUiTask(move || async move {
        let session = historySession;
        let result = session
            .client
            .watch(CoreWatchRequest::new(
                operit_link::nextCoreRouteRequestId("routedChatListFlow"),
                CORE_INTERNAL_TARGET,
                "routedChatListFlow",
                operit_link::toCoreValue(serde_json::json!({"chatId":session.chatId})).unwrap(),
            ))
            .await;
        match result {
            Ok(mut stream) => {
                while let Some(event) = stream.recv().await {
                    if event.kind == CoreEventKind::Completed {
                        break;
                    }
                    let mut histories = session.histories.lock().unwrap();
                    if event.kind == CoreEventKind::Delta {
                        match histories.applyIncrementalDelta(&event.value) {
                            Ok(value) => *histories = value,
                            Err(error) => {
                                *session.error.lock().unwrap() = Some(error);
                                break;
                            }
                        }
                    } else {
                        *histories = event.value;
                    }
                    UI_REVISION.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(error) if isMissingSpaceRoute(&error) => {}
            Err(error) => {
                *session.error.lock().unwrap() = Some(format!("对话列表加载失败：{}", error));
                UI_REVISION.fetch_add(1, Ordering::Relaxed);
            }
       }
    });
    owner.tasks.lock().unwrap().push(task);
}

/// Returns the live Space route state, rather than whether the pairing
/// listener is enabled.
pub fn isConnected() -> bool {
    SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|session| session.as_ref().map(|session| !session.services.peers().activePeerNodeIds().unwrap_or_default().is_empty()))
        .unwrap_or(false)
}

/// Returns a monotonic version for display consumers that repaint on change.
pub fn revision() -> u32 {
    UI_REVISION.load(Ordering::Relaxed)
}

/// Drops the local chat route and display state. Pairing credentials are
/// cleared separately by the Edge pairing authority.
pub fn clear() {
    crate::edge_image::cancel();
    UI_REVISION.fetch_add(1, Ordering::Relaxed);
    let session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|mut session| session.take());
    if let Some(session) = session {
        for task in std::mem::take(&mut *session.tasks.lock().unwrap()) {
            task.abort();
        }
        for (_, task) in std::mem::take(&mut *session.streamTasks.lock().unwrap()) {
            task.abort();
        }
    }
}

pub fn snapshot() -> serde_json::Value {
    let session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone();
    match session {
        Some(session) => {
            // Do not hold UI state locks while serializing or acquiring another
            // lock: the single-threaded network runtime also updates this state.
            let streams = session.streams.lock().unwrap().clone();
            let error = session.error.lock().unwrap().clone();
            serde_json::json!({
                "connected": !session.services.peers().activePeerNodeIds().unwrap_or_default().is_empty(), "chatId": session.chatId,
                "conversations": displayConversations(&session.histories.lock().unwrap()),
                "sending": session.sending.load(Ordering::Acquire),
                "messages": displayMessages(&session.messages.lock().unwrap(), &streams), "error": error,
            })
        }
        None => {
            serde_json::json!({"connected": false, "messages": [], "error": "请先在 Space 中配对此设备"})
        }
    }
}

/// The title comes from conversation metadata, never from a response body.
pub fn preview() -> String {
    let value = snapshot();
    activeConversation(&value)
        .and_then(|chat| chat["characterCardName"].as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Operit")
        .to_string()
}

fn activeConversation(value: &serde_json::Value) -> Option<&serde_json::Value> {
    value["conversations"]
        .as_array()?
        .iter()
        .find(|chat| chat["id"] == value["chatId"])
}

/// Changes only this device's view; the full Core continues owning the chat.
pub fn selectChat(id: &str) -> Result<(), String> {
    let session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone()
        .ok_or("设备尚未连接")?;
    if session.services.peers().activePeerNodeIds().unwrap_or_default().is_empty() {
        return Err("设备已离线".into());
    }
    let exists = match &*session.histories.lock().unwrap() {
        CoreValue::List(items) => items.iter().any(|item| match item {
            CoreValue::Map(fields) => mapString(fields, "id") == id,
            _ => false,
        }),
        _ => false,
    };
    if !exists {
        return Err("对话已不存在，请刷新列表".into());
    }
    install(session.client.clone(), session.services.clone(), id.to_string());
    Ok(())
}

pub fn newChat() -> Result<(), String> {
    let session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone()
        .ok_or("设备尚未连接")?;
    if session.services.peers().activePeerNodeIds().unwrap_or_default().is_empty() {
        return Err("设备已离线".into());
    }
    if session.sending.swap(true, Ordering::AcqRel) {
        return Err("请等待当前操作完成".into());
    }
    let value = snapshot();
    let current = activeConversation(&value).cloned().unwrap_or_default();
    spawnNodeUiTask(move || async move {
        let response = session
            .client
            .call(CoreCallRequest::new(
                operit_link::nextCoreRouteRequestId("createRoutedChat"),
                CORE_INTERNAL_TARGET,
                "createRoutedChat",
                operit_link::toCoreValue(serde_json::json!({
                    "chatId":session.chatId, "characterCardName":current["characterCardName"],
                    "group":current["group"], "characterGroupId":current["characterGroupId"],
                }))
                .unwrap(),
            ))
            .await;
        session.sending.store(false, Ordering::Release);
        match response.result {
            Ok(CoreValue::String(id)) => {
                let current = SESSION.get().unwrap().lock().unwrap().clone();
                if current
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &session))
                {
                    install(session.client.clone(), session.services.clone(), id);
                }
            }
            result => {
                *session.error.lock().unwrap() = Some(match result {
                    Err(error) => error.to_string(),
                    _ => "创建对话返回了无效结果".into(),
                });
                UI_REVISION.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    Ok(())
}

/// Returns a compact state label for the 320x240 task rail.
pub fn taskStatus() -> String {
    let session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone();
    let Some(session) = session else {
        return "离线".into();
    };
    if session.error.lock().unwrap().is_some() {
        return "错误".into();
    }
    if session.services.peers().activePeerNodeIds().unwrap_or_default().is_empty() {
        return "离线".into();
    }
    if session.sending.load(Ordering::Acquire) {
        return "发送中".into();
    }
    if session
        .streams
        .lock()
        .unwrap()
        .values()
        .any(|stream| !stream.completed)
    {
        return "生成中".into();
    }
    "就绪".into()
}

/// Empty/error state only; actual messages are rendered as individual rows.
pub fn screenText() -> String {
    let value = snapshot();
    screenTextFromSnapshot(&value)
}

pub fn screenTextFromSnapshot(value: &serde_json::Value) -> String {
    if let Some(error) = value["error"].as_str() {
        return error.to_string();
    }
    if value["messages"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
    {
        return String::new();
    }
    if isConnected() {
        "输入消息开始对话".into()
    } else {
        "已离线，等待重新连接".into()
    }
}

fn displayMessages(value: &CoreValue, streams: &BTreeMap<String, StreamText>) -> serde_json::Value {
    let CoreValue::List(items) = value else {
        return serde_json::json!([]);
    };
    let start = items.len().saturating_sub(MAX_CHAT_MESSAGES);
    let mut result = Vec::with_capacity(items.len() - start);
    for item in &items[start..] {
        let CoreValue::Map(fields) = item else {
            continue;
        };
        let mut text = String::new();
        if let Some(CoreValue::List(parts)) = fields.get("parts") {
            for part in parts.iter().take(MAX_NESTED_LIST_ITEMS) {
                let CoreValue::Map(part) = part else {
                    continue;
                };
                let kind = mapString(part, "kind");
                let (prefix, content) = match kind {
                    "markdown" | "status" => ("", mapString(part, "content")),
                    "tool_call" => ("调用工具：", mapString(part, "toolName")),
                    "tool_result" => continue,
                    _ => continue,
                };
                let visible;
                let content = if kind == "markdown" || kind == "status" {
                    visible = visibleEdgeText(content);
                    visible.as_str()
                } else {
                    content
                };
                if content.is_empty() {
                    continue;
                }
                if !text.is_empty() {
                    appendBounded(&mut text, "\n", MAX_CHAT_STRING_BYTES);
                }
                appendBounded(&mut text, prefix, MAX_CHAT_STRING_BYTES);
                if kind == "tool_call" {
                    appendBounded(&mut text, content, MAX_TOOL_NAME_BYTES);
                } else {
                    appendBounded(&mut text, content, MAX_PART_TEXT_BYTES);
                }
            }
        }
        if let Some(CoreValue::Map(content)) = fields.get("contentStream") {
            if let Some(CoreValue::Map(descriptor)) = content.get("$coreStream") {
                if let Some(stream) = streams.get(mapString(descriptor, "streamId")) {
                    if !stream.text.is_empty() {
                        text.clear();
                        text = visibleEdgeText(&stream.text);
                    }
                }
            }
        }
        let (text, images) = crate::edge_image::extract(&text);
        if text.trim().is_empty() && images.is_empty() {
            continue;
        }
        result.push(serde_json::json!({
            "sender": mapString(fields, "sender"),
            "text": text,
            "images": images,
        }));
    }
    serde_json::Value::Array(result)
}

fn displayConversations(value: &CoreValue) -> serde_json::Value {
    let CoreValue::List(items) = value else {
        return serde_json::json!([]);
    };
    let start = items.len().saturating_sub(MAX_CONVERSATIONS);
    let mut result = Vec::with_capacity(items.len() - start);
    for item in &items[start..] {
        let CoreValue::Map(fields) = item else {
            continue;
        };
        result.push(serde_json::json!({
            "id": boundedText(mapString(fields, "id")),
            "title": boundedText(mapString(fields, "title")),
            "characterCardName": boundedText(mapString(fields, "characterCardName")),
            "group": simpleJson(fields.get("group")),
            "characterGroupId": simpleJson(fields.get("characterGroupId")),
        }));
    }
    serde_json::Value::Array(result)
}

/// Bridges a synchronous firmware HTTP callback to its existing Link runtime.
pub fn openImage(input: &str) -> Result<(), String> {
    let (request, id) = input.split_once(':').ok_or("无效的图片请求")?;
    let request = request.parse::<u32>().map_err(|_| "无效的图片请求")?;
    let session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone()
        .ok_or("设备尚未连接")?;
    if session.services.peers().activePeerNodeIds().unwrap_or_default().is_empty() {
        return Err("设备已离线".into());
    }
    let messages = displayMessages(
        &session.messages.lock().unwrap(),
        &session.streams.lock().unwrap(),
    );
    if !messages.as_array().into_iter().flatten().any(|m| {
        m["images"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v == id)
    }) {
        return Err("图片已不在当前对话中".into());
    }
    crate::edge_image::start(
        session.client.clone(),
        session.chatId.clone(),
        id.into(),
        request,
    );
    Ok(())
}

/// Uploads one already bounded image as binary Link data, then sends its Core-owned media link.
pub fn sendImage(bytes: Vec<u8>, mime: String) -> Result<(), String> {
    if bytes.is_empty()
        || bytes.len() > 512 * 1024
        || !matches!(mime.as_str(), "image/png" | "image/jpeg")
    {
        return Err("只支持小于 512 KiB 的 PNG/JPEG 图片".into());
    }
    let session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone()
        .ok_or("设备尚未连接")?;
    if session.services.peers().activePeerNodeIds().unwrap_or_default().is_empty() {
        return Err("设备已离线".into());
    }
    if session.sending.swap(true, Ordering::AcqRel) {
        return Err("请等待当前消息发送完成".into());
    }
    spawnNodeUiTask(move || async move {
        let result = async {
            let args = operit_link::CoreValue::Map(std::collections::BTreeMap::from([
                ("chatId".into(), CoreValue::String(session.chatId.clone())),
                ("mimeType".into(), CoreValue::String(mime)),
                ("imageBytes".into(), CoreValue::Bytes(bytes)),
            ]));
            let response = session
                .client
                .call(CoreCallRequest::new(
                    operit_link::nextCoreRouteRequestId("registerChatImage"),
                    CORE_INTERNAL_TARGET,
                    "registerChatImage",
                    args,
                ))
                .await;
            let link = match response.result.map_err(|e| e.to_string())? {
                CoreValue::String(link) if link.len() <= 100 => link,
                _ => return Err("Core 未返回有效的图片引用".into()),
            };
            if !link.starts_with("<link type=\"image\" id=\"") {
                return Err("Core 返回了无效图片引用".into());
            }
            sendImageMessage(&session, link).await
        }
        .await;
        if let Err(error) = &result {
            *session.error.lock().unwrap() = Some(error.clone());
        }
        *session.sendResult.lock().unwrap() = Some(result);
        session.sending.store(false, Ordering::Release);
        UI_REVISION.fetch_add(1, Ordering::Relaxed);
    });
    Ok(())
}

/// Bridges a synchronous firmware callback to its existing Link runtime.
pub fn send(text: String) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("消息不能为空".into());
    }
    let session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "设备尚未连接 Space".to_string())?;
    if session.services.peers().activePeerNodeIds().unwrap_or_default().is_empty() {
        return Err("设备已离线".into());
    }
    if session.sending.swap(true, Ordering::AcqRel) {
        return Err("上一条消息仍在发送".into());
    }
    *session.error.lock().unwrap() = None;
    spawnNodeUiTask(move || async move {
        let result = sendImageMessage(&session, text).await;
        if let Err(error) = &result {
            *session.error.lock().unwrap() = Some(error.clone());
        }
        *session.sendResult.lock().unwrap() = Some(result);
        session.sending.store(false, Ordering::Release);
        UI_REVISION.fetch_add(1, Ordering::Relaxed);
    });
    Ok(())
}

async fn sendImageMessage(session: &ChatSession, text: String) -> Result<(), String> {
    let args = operit_link::toCoreValue(serde_json::json!({
        "promptFunctionType": "CHAT", "roleCardIdOverride": null,
        "chatIdOverride": session.chatId, "messageText": text,
        "proxySenderNameOverride": null, "chatProviderIdOverride": null,
        "chatModelIdOverride": null, "attachments": [], "replyToMessage": null,
        "turnOptions": {"persistTurn": true, "notifyReply": null, "hideUserMessage": false,
            "disableWarning": false, "chatInputSubmitRequestedHandled": false},
    }))
    .unwrap();
    session
        .client
        .call(CoreCallRequest::new(
            operit_link::nextCoreRouteRequestId("sendUserMessage"),
            CORE_INTERNAL_TARGET,
            "sendUserMessage",
            args,
        ))
        .await
        .result
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replyStreamHandlesRollbackWithoutDuplicatingRendererChunks() {
        let mut text = StreamText::default();
        for event in [
            serde_json::json!({"type":"reset"}),
            serde_json::json!({"type":"chunk","value":"你好"}),
            serde_json::json!({"type":"savepoint","id":"a"}),
            serde_json::json!({"type":"chunk","value":"错误分支"}),
            serde_json::json!({"type":"rollback","id":"a"}),
            serde_json::json!({"type":"markdownBlockChunk","value":"重复渲染数据"}),
            serde_json::json!({"type":"chunk","parentBlockId":1,"value":"嵌套块"}),
            serde_json::json!({"type":"chunk","value":"，世界"}),
        ] {
            text.apply(&event);
        }
        assert_eq!(text.text, "你好，世界");
        let messages = operit_link::toCoreValue(serde_json::json!([{
            "sender":"ai", "parts":[], "contentStream":{"$coreStream":{"streamId":"s"}}
        }]))
        .unwrap();
        let displayed = displayMessages(&messages, &BTreeMap::from([("s".into(), text)]));
        assert_eq!(displayed[0]["text"], "你好，世界");
    }
}

/// Acknowledge the remote send, not merely admission to the local queue.
pub fn takeSendResult() -> Option<Result<(), String>> {
    let session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone()?;
    let result = session.sendResult.lock().unwrap().take();
    result
}
