#![allow(non_snake_case)]

use std::collections::VecDeque;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Mutex;

use operit_board_esp32::{logicalDisplaySize, FaceRect, DISPLAY_ROTATION_DEGREES};
use operit_host_api::{HostError, HostResult};

use crate::status::FirmwareStatus;

#[repr(C)]
struct LvglArea {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

type FlushCallback = unsafe extern "C" fn(*const LvglArea, *const u8, usize, *mut c_void);
type ActionCallback = unsafe extern "C" fn(*const c_char, *mut c_void);

unsafe extern "C" {
    fn operit_lvgl_init(
        width: u16,
        height: u16,
        flush_cb: Option<FlushCallback>,
        touch_cb: Option<unsafe extern "C" fn(*mut u16, *mut u16, *mut c_void) -> bool>,
        action_cb: Option<ActionCallback>,
        user_data: *mut c_void,
    ) -> bool;
    fn operit_lvgl_pump(elapsed_ms: u32);
    fn operit_lvgl_set_touch(x: u16, y: u16, pressed: bool);
    fn operit_lvgl_navigate_home();
    fn operit_lvgl_set_connection(wifi_ready: bool, edge_ready: bool);
    fn operit_lvgl_set_paired(paired: bool);
    fn operit_lvgl_set_theme(index: u32, circular: bool);
    fn operit_lvgl_set_expression(expression: *const c_char);
    fn operit_lvgl_set_pairing_code(code: *const c_char);
    fn operit_lvgl_set_space_state(state: *const c_char);
    fn operit_lvgl_set_chat_preview(preview: *const c_char);
    fn operit_lvgl_set_chat_screen(text: *const c_char);
    fn operit_lvgl_set_chat_identity(id: *const c_char, character: *const c_char);
    fn operit_lvgl_set_message(index: u32, user: bool, text: *const c_char);
    fn operit_lvgl_set_message_image(index: u32, image: u32, id: *const c_char);
    fn operit_lvgl_image_request() -> u32;
    fn operit_lvgl_image_chunk(
        request: u32,
        width: u32,
        height: u32,
        offset: u32,
        bytes: *const u8,
        length: u32,
    ) -> bool;
    fn operit_lvgl_image_error(request: u32, error: *const c_char);
    fn operit_lvgl_finish_messages(count: u32);
    fn operit_lvgl_set_conversation(
        index: u32,
        id: *const c_char,
        title: *const c_char,
        character: *const c_char,
        selected: bool,
    );
    fn operit_lvgl_finish_conversations(count: u32);
    fn operit_lvgl_action_error(error: *const c_char);
    fn operit_lvgl_set_chat_task(text: *const c_char);
    fn operit_lvgl_chat_send_result(ok: bool, error: *const c_char);
    fn operit_lvgl_chat_draft() -> *const c_char;
}

/// Owns the LVGL runtime and the small action queue emitted by app buttons.
pub struct Esp32Lvgl {
    context: Box<LvglContext>,
    lastTouch: Option<(u16, u16)>,
}

struct LvglContext {
    board: *const operit_board_esp32::Esp32Board,
    actions: Mutex<VecDeque<String>>,
}

unsafe impl Send for LvglContext {}

impl Esp32Lvgl {
    /// Initializes LVGL with the board's rotated logical display dimensions.
    pub fn new(board: &operit_board_esp32::Esp32Board) -> HostResult<Self> {
        let runtime = Self {
            context: Box::new(LvglContext {
                board,
                actions: Mutex::new(VecDeque::new()),
            }),
            lastTouch: None,
        };
        let userData = runtime.context.as_ref() as *const LvglContext as *mut c_void;
        let (width, height) = logicalDisplaySize(DISPLAY_ROTATION_DEGREES);
        let initialized = unsafe {
            operit_lvgl_init(
                width,
                height,
                Some(flushCallback),
                None,
                Some(actionCallback),
                userData,
            )
        };
        if !initialized {
            return Err(HostError::new("LVGL initialization failed"));
        }
        Ok(runtime)
    }

    /// Feeds one sampled touch point to LVGL's pointer input device.
    pub fn setTouch(&mut self, point: Option<(u16, u16)>) {
        let (x, y, pressed) = match point {
            Some((x, y)) => {
                self.lastTouch = Some((x, y));
                (x, y, true)
            }
            // Keep the final sample on release so horizontal swipe recognition sees
            // the actual finger-up coordinate instead of (0, 0).
            None => self
                .lastTouch
                .map(|(x, y)| (x, y, false))
                .unwrap_or((0, 0, false)),
        };
        unsafe { operit_lvgl_set_touch(x, y, pressed) };
    }

    /// Advances LVGL animations and flushes pending display regions.
    pub fn pump(&mut self, elapsedMs: u32) {
        unsafe { operit_lvgl_pump(elapsedMs) };
    }

    /// Returns to the launcher, used by the left-edge back gesture.
    pub fn goHome(&mut self) {
        unsafe { operit_lvgl_navigate_home() };
    }

    /// Updates the connection indicators shown by the launcher and settings app.
    pub fn setConnection(&mut self, wifiReady: bool, edgeReady: bool) {
        unsafe { operit_lvgl_set_connection(wifiReady, edgeReady) };
    }

    pub fn setPaired(&mut self, paired: bool) {
        unsafe { operit_lvgl_set_paired(paired) };
    }

    pub fn setTheme(&mut self, index: usize) {
        unsafe { operit_lvgl_set_theme(index as u32, false) };
    }

    /// Updates the face app's expression label.
    pub fn setExpression(&mut self, expression: &str) {
        let mut bytes = expression.as_bytes().to_vec();
        bytes.retain(|byte| *byte != 0);
        bytes.push(0);
        unsafe { operit_lvgl_set_expression(bytes.as_ptr() as *const c_char) };
    }

    pub fn setPairingCode(&mut self, code: &str) {
        setText(code, operit_lvgl_set_pairing_code);
    }
    pub fn setSpaceState(&mut self, state: &str) {
        setText(state, operit_lvgl_set_space_state);
    }
    pub fn setChatPreview(&mut self, preview: &str) {
        setText(preview, operit_lvgl_set_chat_preview);
    }
    pub fn setChatScreen(&mut self, text: &str) {
        setText(text, operit_lvgl_set_chat_screen);
    }
    pub fn actionError(&mut self, error: &str) {
        setText(error, operit_lvgl_action_error);
    }

    pub fn setChatState(&mut self, state: &serde_json::Value) {
        let string = |value: &str| CString::new(value.replace('\0', "")).unwrap();
        let id = state["chatId"].as_str().unwrap_or("");
        let name = state["conversations"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|chat| chat["id"].as_str() == Some(id))
            .and_then(|chat| chat["characterCardName"].as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("Operit");
        unsafe {
            operit_lvgl_set_chat_identity(string(id).as_ptr(), string(&name).as_ptr());
        }
        let rows = state["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| {
                row["text"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty())
                    || row["images"].as_array().is_some_and(|a| !a.is_empty())
            })
            .collect::<Vec<_>>();
        let rows = &rows[rows.len().saturating_sub(12)..];
        for (index, row) in rows.iter().enumerate() {
            for image in 0..4 {
                unsafe {
                    operit_lvgl_set_message_image(
                        index as u32,
                        image as u32,
                        string(row["images"][image].as_str().unwrap_or("")).as_ptr(),
                    );
                }
            }
            unsafe {
                operit_lvgl_set_message(
                    index as u32,
                    row["sender"] == "user",
                    string(row["text"].as_str().unwrap_or("")).as_ptr(),
                );
            }
        }
        unsafe {
            operit_lvgl_finish_messages(rows.len() as u32);
        }
        let rows = state["conversations"]
            .as_array()
            .into_iter()
            .flatten()
            .take(24)
            .collect::<Vec<_>>();
        for (index, row) in rows.iter().enumerate() {
            unsafe {
                operit_lvgl_set_conversation(
                    index as u32,
                    string(row["id"].as_str().unwrap_or("")).as_ptr(),
                    string(row["title"].as_str().unwrap_or("")).as_ptr(),
                    string(row["characterCardName"].as_str().unwrap_or("")).as_ptr(),
                    row["id"] == id,
                );
            }
        }
        unsafe {
            operit_lvgl_finish_conversations(rows.len() as u32);
        }
    }

    pub fn setChatTask(&mut self, text: &str) {
        setText(text, operit_lvgl_set_chat_task);
    }

    pub fn imageError(&mut self, error: &str) {
        let text = CString::new(error.replace('\0', "")).unwrap();
        unsafe {
            operit_lvgl_image_error(operit_lvgl_image_request(), text.as_ptr());
        }
    }

    pub fn imageEvent(&mut self, event: crate::edge_image::ImageEvent) {
        match event.chunk {
            Ok(chunk) => unsafe {
                if !operit_lvgl_image_chunk(
                    event.request,
                    chunk.width as u32,
                    chunk.height as u32,
                    chunk.offset as u32,
                    chunk.bytes.as_ptr(),
                    chunk.bytes.len() as u32,
                ) {
                    crate::edge_image::cancel();
                }
            },
            Err(error) => {
                let text = CString::new(error.replace('\0', "")).unwrap();
                unsafe {
                    operit_lvgl_image_error(event.request, text.as_ptr());
                }
            }
        }
    }

    /// Returns the bounded draft captured by the LVGL text area.
    pub fn chatSendResult(&mut self, result: Result<(), String>) {
        let error = CString::new(
            result
                .as_ref()
                .err()
                .map(String::as_str)
                .unwrap_or("")
                .replace('\0', ""),
        )
        .unwrap();
        unsafe {
            operit_lvgl_chat_send_result(result.is_ok(), error.as_ptr());
        }
    }

    pub fn chatDraft(&self) -> String {
        unsafe {
            CStr::from_ptr(operit_lvgl_chat_draft())
                .to_string_lossy()
                .into_owned()
        }
    }

    /// Drains actions requested by LVGL app buttons.
    pub fn drainActions(&self) -> Vec<String> {
        self.context
            .actions
            .lock()
            .map(|mut actions| actions.drain(..).collect())
            .unwrap_or_default()
    }
}

fn setText(value: &str, setter: unsafe extern "C" fn(*const c_char)) {
    let mut bytes = value.as_bytes().to_vec();
    bytes.retain(|byte| *byte != 0);
    bytes.push(0);
    unsafe { setter(bytes.as_ptr() as *const c_char) };
}

unsafe extern "C" fn flushCallback(
    area: *const LvglArea,
    pixels: *const u8,
    length: usize,
    userData: *mut c_void,
) {
    if area.is_null() || pixels.is_null() || userData.is_null() {
        return;
    }
    let context = &*(userData as *const LvglContext);
    let area = &*area;
    let rect = FaceRect {
        x: area.x1.max(0) as u16,
        y: area.y1.max(0) as u16,
        width: (area.x2 - area.x1 + 1).max(0) as u16,
        height: (area.y2 - area.y1 + 1).max(0) as u16,
    };
    let pixels = std::slice::from_raw_parts(pixels, length);
    let board = &*context.board;
    if let Err(error) = board.flushLvgl(rect, pixels) {
        log::error!("operit-esp32 LVGL flush: {}", error.message);
    }
}

unsafe extern "C" fn actionCallback(action: *const c_char, userData: *mut c_void) {
    if action.is_null() || userData.is_null() {
        return;
    }
    let Ok(action) = CStr::from_ptr(action).to_str() else {
        return;
    };
    let context = &*(userData as *const LvglContext);
    if let Ok(mut actions) = context.actions.lock() {
        actions.push_back(action.to_string());
    }
}

/// Keeps status-to-LVGL updates in one place for the firmware loop.
pub fn updateStatus(
    runtime: &mut Esp32Lvgl,
    status: &FirmwareStatus,
    edgeReady: bool,
    paired: bool,
) {
    let snapshot = status.snapshot();
    // `edgeReady` is the live authenticated Space route state. It must not be
    // derived from the TCP listener, which is enabled even before pairing.
    runtime.setConnection(!snapshot.ipv4.is_empty(), edgeReady);
    runtime.setPaired(paired);
    runtime.setExpression(&snapshot.expression);
    runtime.setPairingCode(&snapshot.pairingCode);
    runtime.setSpaceState(if edgeReady {
        "Connected to Space"
    } else {
        "Waiting for Space"
    });
}
