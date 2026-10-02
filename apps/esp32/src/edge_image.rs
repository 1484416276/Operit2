//! One bounded chunk in flight; UI owns the only complete preview buffer.
use operit_edge_contract::image_preview::{
    ImagePreviewChunk, PREVIEW_CHUNK_BYTES, PREVIEW_HEIGHT, PREVIEW_WIDTH,
};
use operit_link::{CoreCallRequest, CORE_INTERNAL_TARGET};
use operit_link::CoreLinkSharedClient;
use crate::edge_chat::{NodeUiTask, spawnNodeUiTask};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub struct ImageEvent {
    pub request: u32,
    pub chunk: Result<ImagePreviewChunk, String>,
}
static PENDING: Mutex<Option<ImageEvent>> = Mutex::new(None);
static TASK: OnceLock<Mutex<Option<NodeUiTask>>> = OnceLock::new();

pub fn take() -> Option<ImageEvent> {
    PENDING.lock().unwrap().take()
}
pub fn cancel() {
    if let Some(task) = TASK.get_or_init(|| Mutex::new(None)).lock().unwrap().take() {
        task.abort();
    }
    *PENDING.lock().unwrap() = None;
}
pub fn start(
    client: Arc<dyn CoreLinkSharedClient + Send + Sync>,
    chat: String,
    image: String,
    request: u32,
) {
    cancel();
    let task = spawnNodeUiTask(move || async move {
        let mut offset = 0;
        let mut dimensions = None;
        loop {
            let response = tokio::time::timeout(
                Duration::from_secs(10),
                client.call(CoreCallRequest::new(
                    operit_link::nextCoreRouteRequestId("chatImagePreviewChunk"),
                    CORE_INTERNAL_TARGET,
                    "chatImagePreviewChunk",
                    operit_link::toCoreValue(serde_json::json!({
                        "chatId":chat,"imageId":image,"offset":offset,
                        "width":PREVIEW_WIDTH,"height":PREVIEW_HEIGHT,
                        "format":"rgb565le","chunkSize":PREVIEW_CHUNK_BYTES,
                    }))
                    .unwrap(),
                )),
            )
            .await;
            let result = match response {
                Ok(response) => response
                    .result
                    .map_err(|e| e.to_string())
                    .and_then(ImagePreviewChunk::from_value),
                Err(_) => Err("图片传输超时，请重新打开".into()),
            }
            .and_then(|chunk| {
                if chunk.offset != offset
                    || dimensions.is_some_and(|size| size != (chunk.width, chunk.height))
                {
                    Err("图片分块顺序或尺寸不一致".into())
                } else {
                    Ok(chunk)
                }
            });
            let done = match &result {
                Ok(chunk) => {
                    dimensions = Some((chunk.width, chunk.height));
                    offset += chunk.bytes.len();
                    offset == chunk.width * chunk.height * 2
                }
                Err(_) => true,
            };
            *PENDING.lock().unwrap() = Some(ImageEvent {
                request,
                chunk: result,
            });
            if done {
                break;
            }
            // Backpressure: never build up chunks while the UI is paused.
            if tokio::time::timeout(Duration::from_secs(10), async {
                while PENDING.lock().unwrap().is_some() {
                    operit_host_api::HostManager::defaultHostRuntimeTaskSchedulerHost()
                        .waitForHostRuntimeDelay(10).await.map_err(|error| error.to_string())?;
                }
                Ok::<(), String>(())
            })
            .await
            .is_err()
            {
                *PENDING.lock().unwrap() = Some(ImageEvent {
                    request,
                    chunk: Err("预览已暂停，请重新打开".into()),
                });
                break;
            }
        }
    });
    *TASK.get().unwrap().lock().unwrap() = Some(task);
}

/// Read canonical media-link tags without adding a regex/XML runtime to firmware.
pub fn extract(text: &str) -> (String, Vec<String>) {
    let mut output = String::new();
    let mut ids = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<link ") {
        output.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let header = &rest[6..end];
        let attr = |key: &str| -> Option<&str> {
            let start = header.find(&format!("{key}="))? + key.len() + 1;
            let quote = header.as_bytes().get(start).copied()? as char;
            if quote != '"' && quote != '\'' {
                return None;
            }
            let value = &header[start + 1..];
            Some(&value[..value.find(quote)?])
        };
        if attr("type") == Some("image") {
            if let Some(id) = attr("id").filter(|id| {
                !id.is_empty()
                    && id.len() <= 80
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            }) {
                if ids.len() < 4 && !ids.iter().any(|s| s == id) {
                    ids.push(id.to_string());
                } else {
                    output.push_str("[更多图片请在电脑端查看]");
                }
                rest = &rest[end + 1..];
                rest = rest.strip_prefix("</link>").unwrap_or(rest);
                continue;
            }
        }
        output.push_str(&rest[..end + 1]);
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    (output, ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_links_preserve_text_and_validate_ids() {
        let (text, ids) =
            extract("前<link type=\"image\" id=\"a-b\"></link>后<link id='c' type='image'></link>");
        assert_eq!(text, "前后");
        assert_eq!(ids, vec!["a-b", "c"]);
        assert!(extract("<link type=\"image\" id=\"../secret\"></link>")
            .1
            .is_empty());
    }
}
