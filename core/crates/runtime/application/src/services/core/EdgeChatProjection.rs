//! Chat-owned, bounded projection for constrained displays.
#![allow(non_snake_case)]
use operit_model::ChatMessage::ChatMessage;
use operit_model::ChatHistoryListItem::ChatHistoryListItem;
use operit_model::MessagePart::{MessagePart, MessagePartKind};
use operit_model::MessagePartCodec::MessagePartCodec;
use operit_link::{CoreStream, CoreStreamSource, CoreEventStream, CoreEventKind, CoreValue};
use operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost;
use operit_util::MarkdownRenderStream::MarkdownStreamEvent;
use std::collections::BTreeMap;
use std::sync::Arc;

const EDGE_MESSAGE_LIMIT: usize = 12;
const EDGE_PART_LIMIT: usize = 4;
const EDGE_TEXT_LIMIT: usize = 1536;
const EDGE_TOTAL_TEXT_LIMIT: usize = 12 * 1024;
const EDGE_ID_LIMIT: usize = 96;

fn appendBounded(target: &mut String, source: &str, limit: usize) {
    let mut end = source.len().min(limit.saturating_sub(target.len()));
    while !source.is_char_boundary(end) { end -= 1; }
    target.push_str(&source[..end]);
}
fn bounded(source: &str, limit: usize) -> String {
    let mut result = String::new();
    appendBounded(&mut result, source, limit);
    result
}

pub fn visibleEdgeText(source: &str) -> String {
    let mut result = String::new();
    let mut rest = source;
    while !rest.is_empty() && result.len() < EDGE_TEXT_LIMIT {
        let Some(open) = rest.find('<') else {
            appendBounded(&mut result, rest, EDGE_TEXT_LIMIT);
            break;
        };
        appendBounded(&mut result, &rest[..open], EDGE_TEXT_LIMIT);
        rest = &rest[open..];
        let Some(close) = rest.find('>') else { break; };
        let tag = &rest[1..close];
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        if name == "link" && (tag.contains("type=\"image\"") || tag.contains("type='image'")) {
            let after_tag = &rest[close + 1..];
            let Some(end) = after_tag.find("</link>") else { break; };
            appendBounded(&mut result, &rest[..=close], EDGE_TEXT_LIMIT);
            appendBounded(&mut result, "</link>", EDGE_TEXT_LIMIT);
            rest = &after_tag[end + "</link>".len()..];
            continue;
        }
        if name == "tool" && !tag.starts_with('/') {
            let tool = tag.split_once("name=\"").and_then(|(_, tail)| tail.split_once('"').map(|(name, _)| name))
                .or_else(|| tag.split_once("name='").and_then(|(_, tail)| tail.split_once('\'').map(|(name, _)| name)));
            if let Some(tool) = tool.filter(|value| !value.is_empty()) {
                if !result.is_empty() { result.push('\n'); }
                appendBounded(&mut result, "调用工具：", EDGE_TEXT_LIMIT);
                let end = tool.len().min(96);
                let end = (0..=end).rev().find(|index| tool.is_char_boundary(*index)).unwrap_or(0);
                appendBounded(&mut result, &tool[..end], EDGE_TEXT_LIMIT);
            }
        }
        rest = &rest[close + 1..];
        if !tag.starts_with('/') && !name.is_empty() {
            let closing = format!("</{name}>");
            if let Some(end) = rest.find(&closing) { rest = &rest[end + closing.len()..]; }
            else if !tag.trim_end().ends_with('/') { break; }
        }
    }
    if result.len() > EDGE_TEXT_LIMIT {
        let end = (0..=EDGE_TEXT_LIMIT.min(result.len())).rev()
            .find(|index| result.is_char_boundary(*index)).unwrap_or(0);
        result.truncate(end);
    }
    result
}

/// Limits the whole transcript, each message and all retained identifiers.
pub fn compactEdgeMessages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut remaining = EDGE_TOTAL_TEXT_LIMIT;
    let mut result = Vec::new();
    for mut message in messages.into_iter().rev() {
        let mut parts = Vec::new();
        let mut messageRemaining = EDGE_TEXT_LIMIT.min(remaining);
        for part in MessagePartCodec::orderedParts(&message.parts) {
            if parts.len() == EDGE_PART_LIMIT || messageRemaining == 0 { break; }
            let compact = match part.kind.clone() {
                MessagePartKind::Markdown | MessagePartKind::Status => {
                    let content = bounded(&visibleEdgeText(&part.content), messageRemaining);
                    if content.is_empty() { continue; }
                    messageRemaining = messageRemaining.saturating_sub(content.len());
                    remaining = remaining.saturating_sub(content.len());
                    MessagePart::new(bounded(&part.partId, EDGE_ID_LIMIT), part.sequence, part.kind.clone(), content)
                }
                MessagePartKind::ToolCall => {
                    let name = bounded(part.toolName.as_deref().unwrap_or("未知工具"), EDGE_ID_LIMIT.min(messageRemaining));
                    messageRemaining = messageRemaining.saturating_sub(name.len());
                    remaining = remaining.saturating_sub(name.len());
                    MessagePart::toolCall(bounded(&part.partId, EDGE_ID_LIMIT), part.sequence,
                        bounded(part.toolCallId.as_deref().unwrap_or(""), EDGE_ID_LIMIT), name, BTreeMap::new())
                }
                MessagePartKind::ToolResult | MessagePartKind::Thinking => continue,
            };
            parts.push(compact);
        }
        message.parts = parts;
        message.sender = bounded(&message.sender, 16);
        message.roleName = bounded(&message.roleName, EDGE_ID_LIMIT);
        message.provider.clear();
        message.modelName.clear();
        message.contentStream = message.contentStream.and_then(compactStream);
        // The stream-only assistant message is the initial live reply.
        if message.parts.is_empty() && message.contentStream.is_none() { continue; }
        result.push(message);
        if result.len() == EDGE_MESSAGE_LIMIT { break; }
    }
    result.reverse();
    result
}

pub fn compactEdgeHistories(histories: Vec<ChatHistoryListItem>) -> Vec<ChatHistoryListItem> {
    histories.into_iter().take(24).map(|mut item| {
        item.id = bounded(&item.id, EDGE_ID_LIMIT);
        item.title = bounded(&item.title, 192);
        item.updatedAt = bounded(&item.updatedAt, 32);
        item.group = None;
        item.workspaceId = None;
        item.workspaceName = None;
        item.characterCardName = None;
        item.characterGroupId = None;
        item
    }).collect()
}

/// Wraps the local source; raw XML bodies never enter the Edge stream.
fn compactStream(stream: CoreStream<MarkdownStreamEvent>) -> Option<CoreStream<MarkdownStreamEvent>> {
    let source = stream.localSource()?;
    let id = format!("edge:{}", bounded(&stream.descriptor.streamId, 128));
    let projected = CoreStreamSource::new(move |request| {
        let mut upstream = source.open(request)?;
        let (sender, receiver) = CoreEventStream::channel();
        defaultHostRuntimeTaskSchedulerHost().scheduleHostRuntimeAsyncTask("edge-chat-stream", Box::new(move || {
            Box::pin(async move {
                let mut remaining = EDGE_TEXT_LIMIT;
                let mut tools = std::collections::BTreeSet::new();
                loop {
                    let mut event = tokio::select! {
                        _ = sender.closed() => break,
                        event = upstream.recv() => match event {
                            Some(event) => event,
                            None => break,
                        },
                    };
                    if event.kind == CoreEventKind::Completed {
                        event.value = CoreValue::Null;
                        let _ = sender.send(event);
                        break;
                    }
                    let CoreValue::Map(fields) = &event.value else { continue; };
                    if fields.get("parentBlockId").is_some_and(|v| *v != CoreValue::Null) { continue; }
                    let kind = match fields.get("type") { Some(CoreValue::String(v)) => v.as_str(), _ => continue };
                    let mut output = BTreeMap::new();
                    let text = if let Some(CoreValue::Map(xml)) = fields.get("xml") {
                        let tool = matches!(xml.get("tagName"), Some(CoreValue::String(name)) if name == "tool");
                        let block = fields.get("blockId").cloned().unwrap_or(CoreValue::Null);
                        let key = format!("{block:?}");
                        if !tool || tools.contains(&key) || tools.len() >= EDGE_PART_LIMIT { continue; }
                        tools.insert(key);
                        let Some(CoreValue::Map(attrs)) = xml.get("attributes") else { continue; };
                        let Some(CoreValue::String(name)) = attrs.get("name") else { continue; };
                        output.insert("type".into(), CoreValue::String("chunk".into()));
                        bounded(&format!("\n调用工具：{}\n", bounded(name, EDGE_ID_LIMIT)), remaining)
                    } else {
                        if !matches!(kind, "chunk" | "reset" | "savepoint" | "rollback") { continue; }
                        output.insert("type".into(), CoreValue::String(kind.into()));
                        if kind == "reset" { remaining = EDGE_TEXT_LIMIT; tools.clear(); }
                        if let Some(CoreValue::String(id)) = fields.get("id") {
                            output.insert("id".into(), CoreValue::String(bounded(id, EDGE_ID_LIMIT)));
                        }
                        match fields.get("value") {
                            Some(CoreValue::String(value)) => bounded(&visibleEdgeText(value), remaining),
                            _ => String::new(),
                        }
                    };
                    remaining = remaining.saturating_sub(text.len());
                    output.insert("value".into(), CoreValue::String(text));
                    event.value = CoreValue::Map(output);
                    if sender.send(event).is_err() { break; }
                }
            })
        }))
            .map_err(|error| operit_link::CoreLinkError::new("EDGE_STREAM_SCHEDULE_FAILED", error.to_string()))?;
        Ok(receiver)
    });
    Some(CoreStream::fromSourceWithId(id, Arc::new(projected)))
}
