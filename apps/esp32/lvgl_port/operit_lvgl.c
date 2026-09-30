#include "operit_lvgl.h"
#include "lvgl.h"
#include "esp_timer.h"
#include "layout_store.h"
#include <string.h>
#include <stdlib.h>
#include <stdio.h>
#include <time.h>
#include "operit_font_zh_14.h"
#include "operit_emoji.h"
LV_FONT_DECLARE(operit_font_digits_28);

/* A shared palette and geometry keep every component aligned at 320x240. */
enum {
    OPERIT_SCREEN_WIDTH = 320,
    OPERIT_SCREEN_HEIGHT = 240,
    OPERIT_HEADER_HEIGHT = 40,
    OPERIT_COMPOSER_HEIGHT = 34,
    OPERIT_MESSAGE_WIDTH = 266,
    OPERIT_MESSAGE_NAV_WIDTH = 30,
    OPERIT_MESSAGE_NAV_HEIGHT = 26,
    OPERIT_MESSAGE_NAV_GAP = 6,
    OPERIT_DRAWER_WIDTH = 240,
    OPERIT_DRAWER_VIEWPORT_HEIGHT = 119,
    OPERIT_DRAWER_FOOTER_HEIGHT = 44,
    OPERIT_KEYBOARD_WIDTH = 304,
    OPERIT_KEYBOARD_HEIGHT = 124,
    OPERIT_KEYBOARD_GAP = 10,
    OPERIT_DRAWER_ANIMATION_MS = 120,
    OPERIT_DRAWER_CLOSE_MS = 96,
};

typedef struct {
    uint32_t background, surface, surface_inset, selection;
    uint32_t header;
    uint32_t accent, on_accent, text, text_secondary, border, scrim, disabled;
    const char *name;
} theme_t;
static const theme_t themes[] = {
    {0x14211c, 0x20332c, 0x182920, 0x304c3e, 0x172522, 0x8bd3b7, 0x14291f,
     0xe4eeeb, 0x91a99f, 0x2b3d37, 0x08120d, 0x71877e, "Aurora"},
    {0x20121d, 0x382436, 0x30202d, 0x5a3546, 0x2c1c28, 0xffb575, 0x2b1720,
     0xfff5eb, 0xc4a2b3, 0x553747, 0x160b13, 0x9b788c, "Ember"},
    {0x111827, 0x253149, 0x1d273b, 0x3c5278, 0x192335, 0x94b8ff, 0x18253c,
     0xf4f7ff, 0x9daecb, 0x34435e, 0x090e18, 0x7183a5, "Orbit"},
};
typedef struct {
    int type, parent, x, y, w, h;
    uint32_t color;
    int radius, value;
    const char *text, *action, *long_action, *binding;
    int font_size;
} operit_layout_node_t;
typedef struct { const char *id; uint32_t background; const operit_layout_node_t *nodes; unsigned count; const char *swipe_left, *swipe_right; } operit_layout_page_t;
#include "layout.generated.h"
static void document_home(void);
static bool document_page(const char *id);
static void execute_route(const char *action);
static char pending_route[128], active_page[24], swipe_left[24], swipe_right[24];
static lv_indev_t *pointer_input;
static size_t copy_utf8(char *, size_t, const char *);
static void queue_route(const char *action) { if(!*pending_route && action) { copy_utf8(pending_route, sizeof(pending_route), action); } }
static unsigned theme_index;
static char page_name[32] = "Chat";
static const char *current_page = page_name;
static bool round_icons;
static lv_display_t *display;
static lv_obj_t *root, *page_host, *overlay_host, *tiles, *clock_label, *connection_label, *face_label;
static lv_obj_t *pairing_label, *space_label, *chat_label, *chat_task_label;
static lv_obj_t *chat_page, *chat_header, *chat_title_label, *message_viewport, *composer_host, *composer_row;
static lv_obj_t *chat_input, *chat_keyboard, *keyboard_window, *keyboard_header;
static lv_obj_t *keyboard_drag_handle, *keyboard_close, *chat_send_button;
static lv_obj_t *chat_voice_button;
static lv_obj_t *drawer_layer, *drawer_panel, *drawer_scrim, *character_viewport;
static lv_obj_t *sidebar_preview_label;
static lv_obj_t *message_list, *dialog_layer;
static lv_obj_t *message_page_label, *message_prev_button, *message_next_button;
static void reset_message_nodes(void);
static char chat_draft[512] = "";
static char submitted_draft[512] = "";
static bool chat_send_pending;
static lv_obj_t *wifi_label, *pairing_hint;
static uint16_t touch_x, touch_y;
static bool touch_pressed, wifi_ready, edge_ready;
static bool paired;
static bool unpair_requested;
static char expression[24] = "neutral";
static char pairing_code[20] = "";
static char space_state[40] = "等待连接 Operit";
static char chat_preview[96] = "尚未连接对话";
static char chat_screen[8192] = "尚未连接对话";
static char chat_task[192] = "离线";
static bool sidebar_open;
static int sidebar_width = OPERIT_DRAWER_WIDTH;
static bool last_touch_pressed;
static uint16_t touch_start_x, touch_start_y;
static int message_scroll_y;
static bool keyboard_dragging;
static int keyboard_saved_scroll;
static int keyboard_drag_offset_x, keyboard_drag_offset_y;
static operit_lvgl_flush_cb_t flush_cb;
static operit_lvgl_action_cb_t action_cb;
static void *context;
static uint8_t draw_buffer[320 * 10 * 2] __attribute__((aligned(4)));
static int64_t last_tick;
static void home(void);
static void builtin_home(void);
static void page(const char *name);
static void close_image(void);
static void show_image(const char *id);
static size_t copy_utf8(char *destination, size_t capacity, const char *source);
static const theme_t *theme(void) { return &themes[theme_index]; }

static const char *const UI_NEW_CHAT = "新建对话";
static const char *const UI_ROLE_CARD = "角色卡";
static const char *const UI_CHAT_GROUP = "对话";
static const char *const UI_PLUGINS = "插件";
static const char *const UI_SETTINGS = "设置";
static const char *const UI_PAIR = "配对";
static const char *const UI_UNPAIR = "取消配对";
static const char *const UI_VOICE = "语音";

/*
 * A small accessibility-like registry is deliberately kept next to the
 * shared LVGL implementation instead of being inferred from the browser
 * canvas.  The same registry is therefore available on a real ESP32 build
 * and in the WebAssembly editor.  Nodes are metadata only; LVGL remains the
 * source of truth for geometry, visibility and text.
 */
#ifdef __EMSCRIPTEN__
#define OPERIT_DEBUG_NODE_LIMIT 384
#define OPERIT_DEBUG_JSON_SIZE 65536
#else
/* Hardware diagnostics must leave DRAM available to pairing and chat. */
#define OPERIT_DEBUG_NODE_LIMIT 192
#define OPERIT_DEBUG_JSON_SIZE 16384
#endif
#define OPERIT_DEBUG_ID_SIZE 48
#define OPERIT_DEBUG_ROLE_SIZE 20
#define OPERIT_DEBUG_ACTION_SIZE 40
#define OPERIT_DEBUG_TEXT_SIZE 256
typedef struct {
    lv_obj_t *object;
    char id[OPERIT_DEBUG_ID_SIZE];
    char role[OPERIT_DEBUG_ROLE_SIZE];
    char action[OPERIT_DEBUG_ACTION_SIZE];
    unsigned sequence;
} operit_debug_node_t;
static operit_debug_node_t debug_nodes[OPERIT_DEBUG_NODE_LIMIT];
static unsigned debug_count;
static char debug_json[OPERIT_DEBUG_JSON_SIZE];

static void debug_copy(char *destination, size_t capacity, const char *source) {
    if (!destination || capacity == 0) return;
    copy_utf8(destination, capacity, source ? source : "");
}

static int debug_find_object(lv_obj_t *object) {
    for (unsigned i = 0; i < debug_count; ++i) {
        if (debug_nodes[i].object == object) return (int)i;
    }
    return -1;
}

static const char *debug_action_id(const char *action) {
    if (!action || !*action) return NULL;
    if (!strcmp(action, "builtin:Pairing")) return "edge_pair";
    if (!strcmp(action, "builtin:Tasks")) return "tasks";
    if (!strcmp(action, "builtin:Settings")) return "settings";
    return action;
}

static void debug_deleted(lv_event_t *event) {
    int index = debug_find_object(lv_event_get_target(event));
    if (index >= 0) debug_nodes[index].object = NULL;
}

static int debug_register(lv_obj_t *object, const char *role, const char *id,
                          const char *action) {
    if (!object) return -1;
    int existing = debug_find_object(object);
    if (existing >= 0) {
        if (role && *role) debug_copy(debug_nodes[existing].role,
                                      sizeof(debug_nodes[existing].role), role);
        if (id && *id) debug_copy(debug_nodes[existing].id,
                                  sizeof(debug_nodes[existing].id), id);
        if (action && *action) debug_copy(debug_nodes[existing].action,
                                          sizeof(debug_nodes[existing].action), action);
        return existing;
    }
    unsigned index = 0;
    while (index < debug_count && debug_nodes[index].object) ++index;
    if (index == OPERIT_DEBUG_NODE_LIMIT) return -1;
    if (index == debug_count) ++debug_count;
    lv_obj_add_event_cb(object, debug_deleted, LV_EVENT_DELETE, NULL);
    operit_debug_node_t *node = &debug_nodes[index];
    memset(node, 0, sizeof(*node));
    node->object = object;
    node->sequence = index;
    debug_copy(node->role, sizeof(node->role), role && *role ? role : "panel");
    debug_copy(node->action, sizeof(node->action), action);
    if (id && *id) debug_copy(node->id, sizeof(node->id), id);
    else {
        char generated[OPERIT_DEBUG_ID_SIZE];
        snprintf(generated, sizeof(generated), "node_%u", index);
        debug_copy(node->id, sizeof(node->id), generated);
    }
    return (int)index;
}

static void debug_set_id(lv_obj_t *object, const char *id) {
    int index = debug_find_object(object);
    if (index >= 0 && id && *id) debug_copy(debug_nodes[index].id,
                                            sizeof(debug_nodes[index].id), id);
}

static void debug_reset(void) {
    memset(debug_nodes, 0, sizeof(debug_nodes));
    debug_count = 0;
}

static bool debug_visible(lv_obj_t *object) {
    for (lv_obj_t *current = object; current; current = lv_obj_get_parent(current)) {
        if (lv_obj_has_flag(current, LV_OBJ_FLAG_HIDDEN)) return false;
    }
    return true;
}

static void debug_json_char(size_t *offset, char value) {
    if (*offset + 2 >= sizeof(debug_json)) return;
    debug_json[(*offset)++] = value;
    debug_json[*offset] = 0;
}

static void debug_json_string(size_t *offset, const char *value) {
    debug_json_char(offset, '"');
    const unsigned char *text = (const unsigned char *)(value ? value : "");
    while (*text && *offset + 8 < sizeof(debug_json)) {
        unsigned char c = *text++;
        if (c == '"' || c == '\\') {
            debug_json_char(offset, '\\'); debug_json_char(offset, (char)c);
        } else if (c == '\n') {
            debug_json_char(offset, '\\'); debug_json_char(offset, 'n');
        } else if (c == '\r') {
            debug_json_char(offset, '\\'); debug_json_char(offset, 'r');
        } else if (c == '\t') {
            debug_json_char(offset, '\\'); debug_json_char(offset, 't');
        } else if (c < 0x20) {
            int written = snprintf(debug_json + *offset, sizeof(debug_json) - *offset,
                                    "\\u%04x", c);
            if (written > 0) *offset += (size_t)written;
        } else debug_json_char(offset, (char)c);
    }
    debug_json_char(offset, '"');
}

static void debug_json_text(size_t *offset, lv_obj_t *object, const char *fallback) {
    if (object && lv_obj_check_type(object, &lv_label_class)) {
        debug_json_string(offset, lv_label_get_text(object));
    } else if (object && lv_obj_check_type(object, &lv_textarea_class)) {
        debug_json_string(offset, lv_textarea_get_text(object));
    } else {
        debug_json_string(offset, fallback ? fallback : "");
    }
}

static void debug_append_node(size_t *offset, unsigned index) {
    operit_debug_node_t *node = &debug_nodes[index];
    lv_area_t area;
    lv_obj_get_coords(node->object, &area);
    int parent = -1;
    lv_obj_t *object_parent = lv_obj_get_parent(node->object);
    if (object_parent) parent = debug_find_object(object_parent);
    const char *fallback = "";
    if (!strcmp(node->id, "pairing_code")) fallback = pairing_code;
    debug_json_char(offset, '{');
    debug_json_string(offset, "id"); debug_json_char(offset, ':'); debug_json_string(offset, node->id);
    debug_json_char(offset, ','); debug_json_string(offset, "role"); debug_json_char(offset, ':'); debug_json_string(offset, node->role);
    debug_json_char(offset, ','); debug_json_string(offset, "text"); debug_json_char(offset, ':'); debug_json_text(offset, node->object, fallback);
    debug_json_char(offset, ','); debug_json_string(offset, "action"); debug_json_char(offset, ':'); debug_json_string(offset, node->action);
    debug_json_char(offset, ','); debug_json_string(offset, "parent"); debug_json_char(offset, ':');
    if (parent >= 0) debug_json_string(offset, debug_nodes[parent].id); else debug_json_string(offset, "");
    snprintf(debug_json + *offset, sizeof(debug_json) - *offset,
             ",\"rect\":{\"x\":%d,\"y\":%d,\"w\":%d,\"h\":%d},\"visible\":%s,\"enabled\":%s,\"clickable\":%s}",
             (int)area.x1, (int)area.y1, (int)(area.x2 - area.x1 + 1), (int)(area.y2 - area.y1 + 1),
             debug_visible(node->object) ? "true" : "false",
             lv_obj_has_state(node->object, LV_STATE_DISABLED) ? "false" : "true",
             lv_obj_has_flag(node->object, LV_OBJ_FLAG_CLICKABLE) ? "true" : "false");
    *offset += strlen(debug_json + *offset);
}

static const char *debug_build_json(bool snapshot) {
    size_t offset = 0;
    debug_json[0] = 0;
    debug_json_char(&offset, '{');
    debug_json_string(&offset, "page"); debug_json_char(&offset, ':'); debug_json_string(&offset, current_page);
    debug_json_char(&offset, ','); debug_json_string(&offset, "width"); debug_json_char(&offset, ':');
    offset += (size_t)snprintf(debug_json + offset, sizeof(debug_json) - offset, "320");
    debug_json_char(&offset, ','); debug_json_string(&offset, "height"); debug_json_char(&offset, ':');
    offset += (size_t)snprintf(debug_json + offset, sizeof(debug_json) - offset, "240");
    debug_json_char(&offset, ','); debug_json_string(&offset, "emojiStyle"); debug_json_char(&offset, ':');
    offset += (size_t)snprintf(debug_json + offset, sizeof(debug_json) - offset, "%u", operit_emoji_style());
    if (snapshot) {
        debug_json_char(&offset, ','); debug_json_string(&offset, "sidebarOpen"); debug_json_char(&offset, ':');
        offset += (size_t)snprintf(debug_json + offset, sizeof(debug_json) - offset, "%s", sidebar_open ? "true" : "false");
        debug_json_char(&offset, ','); debug_json_string(&offset, "sidebarWidth"); debug_json_char(&offset, ':');
        offset += (size_t)snprintf(debug_json + offset, sizeof(debug_json) - offset, "%d", sidebar_width);
        debug_json_char(&offset, ','); debug_json_string(&offset, "pairingCode"); debug_json_char(&offset, ':');
        debug_json_string(&offset, pairing_code);
        debug_json_char(&offset, ','); debug_json_string(&offset, "chat"); debug_json_char(&offset, ':');
        debug_json_string(&offset, chat_screen);
    }
    debug_json_char(&offset, ','); debug_json_string(&offset, "nodes"); debug_json_char(&offset, ':'); debug_json_char(&offset, '[');
    bool first = true;
    for (unsigned i = 0; i < debug_count; ++i) {
        if (!debug_nodes[i].object) continue;
        if (!first) debug_json_char(&offset, ',');
        first = false;
        debug_append_node(&offset, i);
    }
    debug_json_char(&offset, ']'); debug_json_char(&offset, '}');
    return debug_json;
}
/* Default palette tokens follow the selected theme; custom colors stay literal. */
static uint32_t document_color(uint32_t color) {
    if(color==0x091420)return theme()->background;
    if(color==0x172a3d)return theme()->surface;
    if(color==0x53dfc5)return theme()->accent;
    if(color==0x96abbc)return theme()->text_secondary;
    if(color==0xf4f8ff)return theme()->text;
    if(color==0x216c73)return theme()->accent;
    return color;
}

static lv_obj_t *box(lv_obj_t *parent, int x, int y, int w, int h, uint32_t color, int radius) {
    lv_obj_t *o = lv_obj_create(parent);
    lv_obj_remove_style_all(o);
    lv_obj_set_pos(o, x, y); lv_obj_set_size(o, w, h);
    lv_obj_set_style_bg_color(o, lv_color_hex(color), 0);
    lv_obj_set_style_bg_opa(o, LV_OPA_COVER, 0);
    lv_obj_set_style_radius(o, radius, 0);
    lv_obj_clear_flag(o, LV_OBJ_FLAG_SCROLLABLE);
    debug_register(o, "panel", NULL, NULL);
    return o;
}
static lv_obj_t *label(lv_obj_t *p, const char *text, int x, int y, int w, uint32_t color) {
    lv_obj_t *o = lv_label_create(p);
    lv_label_set_text(o, text); lv_obj_set_pos(o, x, y); lv_obj_set_width(o, w);
    lv_obj_set_style_text_color(o, lv_color_hex(color), 0);
    lv_obj_set_style_text_font(o, &operit_font_zh_14, 0);
    lv_label_set_long_mode(o, LV_LABEL_LONG_CLIP);
    lv_obj_clear_flag(o, LV_OBJ_FLAG_CLICKABLE);
    debug_register(o, "label", NULL, NULL);
    return o;
}
static void clicked(lv_event_t *e) {
    const char *name = lv_event_get_user_data(e);
    if (!strncmp(name,"edge_",5) || !strncmp(name,"dialog_",7) || !strncmp(name,"emoji_",6) ||
        !strncmp(name,"image_open:",11) || !strcmp(name,"image_close")) queue_route(name);
    else if (!strcmp(name,"Home")) queue_route("home");
    else if (!strcmp(name,"Palette")) queue_route("theme_next");
    else if (!strcmp(name,"Shape")) queue_route("shape_toggle");
    else if (!strcmp(name,"Online")) queue_route("face_online");
    else if (!strcmp(name,"Run")) queue_route("run_node");
    else if (!strcmp(name,"edge_search") || !strcmp(name,"edge_pair") || !strcmp(name,"edge_chat") || !strcmp(name,"edge_send") ||
             !strcmp(name,"edge_new") || !strcmp(name,"edge_unpair") || !strcmp(name,"voice_placeholder") ||
             !strcmp(name,"sidebar_toggle") || !strcmp(name,"sidebar_open") ||
             !strcmp(name,"sidebar_close") || !strcmp(name,"chat_keyboard_close") || !strcmp(name,"sidebar_width_cycle")) queue_route(name);
    else if (!strncmp(name, "builtin:", 8) || !strncmp(name, "go:", 3) || !strncmp(name, "page:", 5)) queue_route(name);
    else { char route[32];snprintf(route,sizeof(route),"builtin:%s",name);queue_route(route); }
}
static lv_obj_t *button(lv_obj_t *p, const char *text, const char *action, int x, int y, int w, int h) {
    lv_obj_t *o = box(p, x, y, w, h, theme()->surface, 12);
    debug_register(o, "button", debug_action_id(action), action);
    lv_obj_add_flag(o, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_set_style_bg_color(o, lv_color_hex(theme()->accent), LV_STATE_PRESSED);
    lv_obj_add_event_cb(o, clicked, LV_EVENT_CLICKED, (void *)action);
    lv_obj_t *t = label(o, text, 0, 0, w - 8, theme()->text);
    lv_obj_set_style_text_align(t, LV_TEXT_ALIGN_CENTER, 0); lv_obj_center(t);
    return o;
}
static void clear(void) {
    close_image();
    active_page[0]=0;
    swipe_left[0]=swipe_right[0]=0;
    clock_label = connection_label = face_label = tiles = NULL;
    page_host = overlay_host = NULL;
    pairing_label = space_label = chat_label = chat_task_label = NULL;
    chat_page = chat_header = chat_title_label = message_viewport = composer_host = composer_row = NULL;
    chat_input = chat_keyboard = keyboard_window = keyboard_header = NULL;
    keyboard_drag_handle = keyboard_close = chat_send_button = chat_voice_button = NULL;
    wifi_label = pairing_hint = NULL;
    drawer_layer = drawer_panel = drawer_scrim = character_viewport = NULL;
    sidebar_preview_label = NULL;
    message_list = dialog_layer = NULL;
    message_page_label = message_prev_button = message_next_button = NULL;
    reset_message_nodes();
    lv_obj_clean(root);
    lv_obj_set_style_bg_color(root, lv_color_hex(theme()->background), 0);
    debug_reset();
}
static void tick_clock(lv_timer_t *timer) {
    (void)timer;
    if (!clock_label) return;
    time_t now = time(NULL); struct tm tm;
    char text[24];
    if (now > 1700000000 && localtime_r(&now, &tm)) strftime(text, sizeof(text), "%H:%M", &tm);
    else snprintf(text, sizeof(text), "--:--");
    lv_label_set_text(clock_label, text);
}
static void builtin_home(void) {
    /* An unpaired device opens the real pairing guide; an authenticated device
     * keeps the chat entry point even when the Core is temporarily offline. */
    sidebar_open = false;
    page(paired ? "Chat" : "Pairing");
}
/* Copy whole UTF-8 code points so a bounded display buffer never ends in a broken glyph. */
static size_t copy_utf8(char *destination, size_t capacity, const char *source) {
    if (!destination || capacity == 0) return 0;
    if (!source) source = "";
    size_t out = 0;
    while (*source) {
        unsigned char lead = (unsigned char)*source;
        size_t bytes = lead < 0x80 ? 1 : (lead & 0xe0) == 0xc0 ? 2 :
                       (lead & 0xf0) == 0xe0 ? 3 : (lead & 0xf8) == 0xf0 ? 4 : 1;
        if (out + bytes >= capacity) break;
        bool valid = true;
        for (size_t i = 1; i < bytes; ++i) {
            if (source[i] == 0 || ((unsigned char)source[i] & 0xc0) != 0x80) { valid = false; break; }
        }
        if (!valid) bytes = 1;
        memcpy(destination + out, source, bytes);
        source += bytes;
        out += bytes;
    }
    destination[out] = 0;
    return out;
}

static void remember_chat_draft(void) {
    if (!chat_input) return;
    copy_utf8(chat_draft, sizeof(chat_draft), lv_textarea_get_text(chat_input));
}


/* The active UI follows the structure in docs/esp32-ui/esp32-ui-structure.md.
 * Page content and overlays are siblings under separate hosts so scrolling a
 * message list never moves a drawer or floating keyboard. */
static lv_obj_t *ui_box(lv_obj_t *parent, int x, int y, int w, int h,
                        uint32_t color, int radius) {
    lv_obj_t *object = box(parent, x, y, w, h, color, radius);
    return object;
}

static lv_obj_t *ui_action(lv_obj_t *parent, const char *text, const char *action,
                           int x, int y, int w, int h, uint32_t background,
                           uint32_t foreground) {
    lv_obj_t *object = ui_box(parent, x, y, w, h, background, 8);
    debug_register(object, "button", debug_action_id(action), action);
    lv_obj_add_flag(object, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_set_style_bg_color(object, lv_color_hex(theme()->selection), LV_STATE_PRESSED);
    lv_obj_add_event_cb(object, clicked, LV_EVENT_CLICKED, (void *)action);
    lv_obj_t *caption = label(object, text, 4, 0, w - 8, foreground);
    lv_obj_set_style_text_align(caption, LV_TEXT_ALIGN_CENTER, 0);
    lv_obj_center(caption);
    return object;
}

static void ui_hosts(void) {
    page_host = ui_box(root, 0, 0, OPERIT_SCREEN_WIDTH, OPERIT_SCREEN_HEIGHT,
                       theme()->background, 0);
    debug_set_id(page_host, "page_host");
    overlay_host = ui_box(root, 0, 0, OPERIT_SCREEN_WIDTH, OPERIT_SCREEN_HEIGHT,
                          theme()->background, 0);
    debug_set_id(overlay_host, "overlay_host");
    lv_obj_set_style_bg_opa(overlay_host, LV_OPA_TRANSP, 0);
    lv_obj_clear_flag(overlay_host, LV_OBJ_FLAG_CLICKABLE);
}

#include "operit_chat_view.inc"

static void layout_chat_keyboard(void) {
    if (!chat_page || !composer_host || !message_viewport) return;
    int composer_y = OPERIT_SCREEN_HEIGHT - OPERIT_COMPOSER_HEIGHT - 8;
    if (keyboard_window && !lv_obj_has_flag(keyboard_window, LV_OBJ_FLAG_HIDDEN)) {
        int keyboard_y = lv_obj_get_y(keyboard_window);
        composer_y = keyboard_y - OPERIT_KEYBOARD_GAP - OPERIT_COMPOSER_HEIGHT;
    }
    if (composer_y < OPERIT_HEADER_HEIGHT + 8) composer_y = OPERIT_HEADER_HEIGHT + 8;
    lv_obj_set_pos(composer_host, 0, composer_y);
    lv_obj_set_size(composer_host, OPERIT_SCREEN_WIDTH, OPERIT_COMPOSER_HEIGHT);
    int viewport_bottom = composer_y - OPERIT_KEYBOARD_GAP;
    int viewport_height = viewport_bottom - (OPERIT_HEADER_HEIGHT + 32);
    if (viewport_height < 8) viewport_height = 8;
    lv_obj_set_pos(message_viewport, 8, OPERIT_HEADER_HEIGHT + 32);
    lv_obj_set_size(message_viewport, OPERIT_MESSAGE_WIDTH, viewport_height);
    /* Keep pagination beside the message, including when the keyboard moves. */
    int nav_height = 2 * OPERIT_MESSAGE_NAV_HEIGHT + OPERIT_MESSAGE_NAV_GAP;
    int nav_y = OPERIT_HEADER_HEIGHT + 32 + (viewport_height - nav_height) / 2;
    lv_obj_t *nav_buttons[] = {message_prev_button, message_next_button};
    for (unsigned i = 0; i < 2; ++i) {
        if (!nav_buttons[i]) continue;
        lv_obj_set_pos(nav_buttons[i], OPERIT_SCREEN_WIDTH - 8 - OPERIT_MESSAGE_NAV_WIDTH,
                       nav_y + i * (OPERIT_MESSAGE_NAV_HEIGHT + OPERIT_MESSAGE_NAV_GAP));
        if (viewport_height < nav_height) lv_obj_add_flag(nav_buttons[i], LV_OBJ_FLAG_HIDDEN);
        else lv_obj_clear_flag(nav_buttons[i], LV_OBJ_FLAG_HIDDEN);
    }
    int max_scroll = lv_obj_get_height(message_list) - viewport_height;
    if (max_scroll < 0) max_scroll = 0;
    if (message_scroll_y > max_scroll) message_scroll_y = max_scroll;
    lv_obj_set_y(message_list, -message_scroll_y);
}

static int keyboard_y_for_touch(int y) {
    int next = y - keyboard_drag_offset_y;
    int min_y = OPERIT_HEADER_HEIGHT + OPERIT_COMPOSER_HEIGHT + OPERIT_KEYBOARD_GAP;
    int max_y = OPERIT_SCREEN_HEIGHT - OPERIT_KEYBOARD_HEIGHT - 2;
    if (min_y > max_y) min_y = max_y;
    if (next < min_y) next = min_y;
    if (next > max_y) next = max_y;
    return next;
}

static int keyboard_x_for_touch(int x) {
    int next = x - keyboard_drag_offset_x;
    int max_x = OPERIT_SCREEN_WIDTH - OPERIT_KEYBOARD_WIDTH - 4;
    if (next < 4) next = 4;
    if (next > max_x) next = max_x;
    return next;
}

static void keyboard_handle_event(lv_event_t *event) {
    lv_event_code_t code = lv_event_get_code(event);
    if (code == LV_EVENT_PRESSED) {
        keyboard_dragging = true;
        keyboard_drag_offset_x = (int)touch_x - lv_obj_get_x(keyboard_window);
        keyboard_drag_offset_y = (int)touch_y - lv_obj_get_y(keyboard_window);
    } else if (code == LV_EVENT_PRESSING && keyboard_dragging) {
        lv_obj_set_x(keyboard_window, keyboard_x_for_touch((int)touch_x));
        lv_obj_set_y(keyboard_window, keyboard_y_for_touch((int)touch_y));
        layout_chat_keyboard();
    } else if (code == LV_EVENT_RELEASED || code == LV_EVENT_PRESS_LOST) {
        keyboard_dragging = false;
    }
}

static void hide_chat_keyboard(void) {
    bool was_open = keyboard_window && !lv_obj_has_flag(keyboard_window, LV_OBJ_FLAG_HIDDEN);
    if (chat_keyboard) {
        lv_obj_add_flag(keyboard_window, LV_OBJ_FLAG_HIDDEN);
        lv_keyboard_set_textarea(chat_keyboard, NULL);
    }
    if (chat_input) lv_obj_clear_state(chat_input, LV_STATE_FOCUSED);
    keyboard_dragging = false;
    layout_chat_keyboard();
    if (was_open && message_viewport) {
        int max_scroll = lv_obj_get_height(message_list) - lv_obj_get_height(message_viewport);
        if (max_scroll < 0) max_scroll = 0;
        message_scroll_y = LV_MIN(keyboard_saved_scroll, max_scroll);
        lv_obj_set_y(message_list, -message_scroll_y);
    }
}

static void show_chat_keyboard(void) {
    if (!chat_keyboard || !chat_input || !keyboard_window) return;
    if (lv_obj_has_flag(keyboard_window, LV_OBJ_FLAG_HIDDEN))
        keyboard_saved_scroll = message_scroll_y;
    lv_keyboard_set_textarea(chat_keyboard, chat_input);
    lv_obj_clear_flag(keyboard_window, LV_OBJ_FLAG_HIDDEN);
    lv_obj_set_pos(keyboard_window, 8, OPERIT_SCREEN_HEIGHT - OPERIT_KEYBOARD_HEIGHT - 2);
    layout_chat_keyboard();
}

static void chat_keyboard_event(lv_event_t *event) {
    lv_event_code_t code = lv_event_get_code(event);
    if (code == LV_EVENT_READY) {
        remember_chat_draft();
        hide_chat_keyboard();
    } else if (code == LV_EVENT_CANCEL) {
        hide_chat_keyboard();
    }
}

static void chat_input_event(lv_event_t *event) {
    lv_event_code_t code = lv_event_get_code(event);
    if (code == LV_EVENT_FOCUSED || code == LV_EVENT_CLICKED) show_chat_keyboard();
    /* Tapping the floating header changes focus, but must not close the window. */
}

#include "operit_keyboard.inc"

static void drawer_anim_exec(void *object, int32_t x) {
    lv_obj_set_x((lv_obj_t *)object, x);
}

static void drawer_close_ready(lv_anim_t *animation) {
    (void)animation;
    if (!sidebar_open && drawer_layer) lv_obj_add_flag(drawer_layer, LV_OBJ_FLAG_HIDDEN);
}

static void animate_drawer(lv_obj_t *object, int32_t to, uint32_t duration,
                           lv_anim_ready_cb_t ready_cb) {
    if (!object) return;
    lv_anim_delete(object, drawer_anim_exec);
    lv_anim_t animation;
    lv_anim_init(&animation);
    lv_anim_set_var(&animation, object);
    lv_anim_set_values(&animation, lv_obj_get_x(object), to);
    lv_anim_set_time(&animation, duration);
    lv_anim_set_path_cb(&animation, lv_anim_path_ease_out);
    lv_anim_set_exec_cb(&animation, drawer_anim_exec);
    if (ready_cb) lv_anim_set_ready_cb(&animation, ready_cb);
    lv_anim_start(&animation);
}

static lv_obj_t *drawer_footer_button(lv_obj_t *parent, const char *icon,
                                      const char *text, const char *action, int x) {
    lv_obj_t *object = ui_box(parent, x, 0, 108, 29, theme()->surface, 7);
    debug_register(object, "button", debug_action_id(action), action);
    lv_obj_add_flag(object, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_add_event_cb(object, clicked, LV_EVENT_CLICKED, (void *)action);
    lv_obj_t *icon_label = label(object, icon, 10, 5, 18, theme()->text_secondary);
    lv_obj_set_style_text_align(icon_label, LV_TEXT_ALIGN_CENTER, 0);
    label(object, text, 34, 5, 66, theme()->text);
    return object;
}

static void draw_drawer(void) {
    drawer_layer = ui_box(overlay_host, 0, 0, OPERIT_SCREEN_WIDTH, OPERIT_SCREEN_HEIGHT,
                          theme()->background, 0);
    debug_set_id(drawer_layer, "drawer_layer");
    lv_obj_set_style_bg_opa(drawer_layer, LV_OPA_TRANSP, 0);
    drawer_scrim = ui_box(drawer_layer, 0, 0, OPERIT_SCREEN_WIDTH, OPERIT_SCREEN_HEIGHT,
                          theme()->scrim, 0);
    debug_set_id(drawer_scrim, "drawer_scrim");
    lv_obj_set_style_bg_opa(drawer_scrim, LV_OPA_50, 0);
    lv_obj_add_flag(drawer_scrim, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_add_event_cb(drawer_scrim, clicked, LV_EVENT_CLICKED, (void *)"sidebar_close");

    drawer_panel = ui_box(drawer_layer, -sidebar_width, 0, sidebar_width,
                           OPERIT_SCREEN_HEIGHT, theme()->surface, 0);
    debug_set_id(drawer_panel, "drawer_panel");
    lv_obj_clear_flag(drawer_panel, LV_OBJ_FLAG_SCROLLABLE);
    lv_obj_t *drawer_close = ui_action(drawer_scrim, LV_SYMBOL_CLOSE, "sidebar_close",
                                        sidebar_width + 24, 8, 32, 26,
                                        theme()->scrim, theme()->text_secondary);
    debug_set_id(drawer_close, "drawer_close_button");

    lv_obj_t *header = ui_box(drawer_panel, 0, 0, sidebar_width, 64, theme()->surface, 0);
    debug_set_id(header, "drawer_header");
    lv_obj_t *header_row = ui_box(header, 0, 0, sidebar_width, 34, theme()->surface, 0);
    debug_set_id(header_row, "drawer_header_row");
    label(header_row, "对话", 12, 9, 120, theme()->text);
    ui_action(header_row, UI_UNPAIR, "edge_unpair", sidebar_width - 82, 5, 74, 28,
              theme()->surface, theme()->text_secondary);
    lv_obj_t *new_chat = ui_action(header, UI_NEW_CHAT, "edge_new", 8, 36,
                                   sidebar_width - 16, 28, theme()->accent, theme()->on_accent);
    lv_obj_t *caption = lv_obj_get_child(new_chat, 0);
    lv_obj_set_width(caption, sidebar_width - 60);
    lv_obj_set_align(caption, LV_ALIGN_TOP_LEFT);
    lv_obj_set_pos(caption, 12, 4);
    lv_obj_set_style_text_align(caption, LV_TEXT_ALIGN_LEFT, 0);
    label(new_chat, LV_SYMBOL_PLUS, sidebar_width - 42, 6, 20, theme()->on_accent);
    character_viewport = ui_box(drawer_panel, 8, 69, sidebar_width - 16,
                                 OPERIT_DRAWER_VIEWPORT_HEIGHT, theme()->surface, 0);
    debug_set_id(character_viewport, "character_viewport");
    lv_obj_add_flag(character_viewport, LV_OBJ_FLAG_SCROLLABLE);
    lv_obj_set_scroll_dir(character_viewport, LV_DIR_VER);
    lv_obj_set_scrollbar_mode(character_viewport, LV_SCROLLBAR_MODE_AUTO);
    render_conversations();
    ui_box(drawer_panel, 8, 196, sidebar_width - 16, 1, theme()->border, 0);
    lv_obj_t *footer = ui_box(drawer_panel, 0, 203, sidebar_width, 29, theme()->surface, 0);
    debug_set_id(footer, "drawer_footer");
    drawer_footer_button(footer, LV_SYMBOL_DIRECTORY, UI_PLUGINS, "builtin:Plugins", 8);
    drawer_footer_button(footer, LV_SYMBOL_SETTINGS, UI_SETTINGS, "builtin:Settings", 124);

    if (sidebar_open) {
        lv_obj_set_x(drawer_panel, 0);
    } else {
        lv_obj_add_flag(drawer_layer, LV_OBJ_FLAG_HIDDEN);
    }
}

static void set_sidebar_open(bool open, bool animate) {
    sidebar_open = open;
    if (!drawer_layer || !drawer_panel || !drawer_scrim) {
        page("Chat");
        return;
    }
    hide_chat_keyboard();
    if (!animate) {
        if (open) {
            lv_obj_clear_flag(drawer_layer, LV_OBJ_FLAG_HIDDEN);
            lv_obj_set_x(drawer_panel, 0);
        } else {
            lv_obj_set_x(drawer_panel, -sidebar_width);
            lv_obj_add_flag(drawer_layer, LV_OBJ_FLAG_HIDDEN);
        }
        return;
    }
    if (open) {
        lv_obj_clear_flag(drawer_layer, LV_OBJ_FLAG_HIDDEN);
        animate_drawer(drawer_panel, 0, OPERIT_DRAWER_ANIMATION_MS, NULL);
    } else {
        animate_drawer(drawer_panel, -sidebar_width, OPERIT_DRAWER_CLOSE_MS, drawer_close_ready);
    }
}

static void draw_chat(void) {
    chat_page = ui_box(page_host, 0, 0, OPERIT_SCREEN_WIDTH, OPERIT_SCREEN_HEIGHT,
                       theme()->background, 0);
    debug_register(chat_page, "screen", "chat_page", NULL);
    chat_header = ui_box(chat_page, 0, 0, OPERIT_SCREEN_WIDTH, OPERIT_HEADER_HEIGHT,
                         theme()->header, 0);
    debug_set_id(chat_header, "chat_header");
    lv_obj_t *menu = button(chat_header, "", "sidebar_toggle", 8, 6, 30, 28);
    for (int y = 7; y <= 19; y += 6) {
        lv_obj_t *line = box(menu, 6, y, 18, 2, theme()->text, 1);
        lv_obj_clear_flag(line, LV_OBJ_FLAG_CLICKABLE);
    }
    chat_title_label = label(chat_header, chat_preview[0] ? chat_preview : "Operit",
                             48, 10, 150, theme()->text);
    debug_set_id(chat_title_label, "active_character_label");
    lv_label_set_long_mode(chat_title_label, LV_LABEL_LONG_DOT);
    connection_label = label(chat_header, edge_ready ? "● 已连接" : "● 离线",
                             214, 11, 98, edge_ready ? theme()->accent : theme()->text_secondary);
    debug_set_id(connection_label, "connection_status");
    lv_obj_set_style_text_align(connection_label, LV_TEXT_ALIGN_RIGHT, 0);

    message_prev_button = ui_box(chat_page, OPERIT_SCREEN_WIDTH - 8 - OPERIT_MESSAGE_NAV_WIDTH, 103,
                                 OPERIT_MESSAGE_NAV_WIDTH, OPERIT_MESSAGE_NAV_HEIGHT, theme()->surface, 4);
    debug_register(message_prev_button, "button", "message_prev", NULL);
    lv_obj_add_flag(message_prev_button, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_add_event_cb(message_prev_button, message_page_clicked, LV_EVENT_CLICKED, (void *)(intptr_t)-1);
    label(message_prev_button, LV_SYMBOL_UP, 7, 4, 18, theme()->text);
    message_page_label = label(chat_page, "", 48, 48, 224, theme()->text_secondary);
    debug_set_id(message_page_label, "message_page_label");
    lv_obj_set_style_text_align(message_page_label, LV_TEXT_ALIGN_CENTER, 0);
    message_next_button = ui_box(chat_page, OPERIT_SCREEN_WIDTH - 8 - OPERIT_MESSAGE_NAV_WIDTH, 135,
                                 OPERIT_MESSAGE_NAV_WIDTH, OPERIT_MESSAGE_NAV_HEIGHT, theme()->surface, 4);
    debug_register(message_next_button, "button", "message_next", NULL);
    lv_obj_add_flag(message_next_button, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_add_event_cb(message_next_button, message_page_clicked, LV_EVENT_CLICKED, (void *)(intptr_t)1);
    label(message_next_button, LV_SYMBOL_DOWN, 7, 4, 18, theme()->text);

    message_viewport = ui_box(chat_page, 8, 72, OPERIT_MESSAGE_WIDTH, 120, theme()->background, 0);
    debug_set_id(message_viewport, "message_viewport");
    /* One message is rendered at a time. Scrolling this container would make
     * a vertical swipe compete with message pagination. */
    lv_obj_clear_flag(message_viewport, LV_OBJ_FLAG_SCROLLABLE);
    lv_obj_set_scrollbar_mode(message_viewport, LV_SCROLLBAR_MODE_OFF);
    message_list = ui_box(message_viewport, 0, 0, OPERIT_MESSAGE_WIDTH, 1, theme()->background, 0);
    debug_set_id(message_list, "message_list");
    render_messages();

    composer_host = ui_box(chat_page, 0, 200, OPERIT_SCREEN_WIDTH, OPERIT_COMPOSER_HEIGHT,
                           theme()->background, 0);
    debug_set_id(composer_host, "composer_host");
    composer_row = ui_box(composer_host, 8, 0, 304, OPERIT_COMPOSER_HEIGHT,
                          theme()->background, 0);
    debug_set_id(composer_row, "composer_row");
    chat_input = lv_textarea_create(composer_row);
    debug_register(chat_input, "textbox", "message_input", NULL);
    lv_obj_set_pos(chat_input, 0, 0); lv_obj_set_size(chat_input, 220, OPERIT_COMPOSER_HEIGHT);
    lv_textarea_set_one_line(chat_input, true);
    lv_textarea_set_max_length(chat_input, 120);
    lv_textarea_set_placeholder_text(chat_input, "输入消息…");
    lv_textarea_set_text(chat_input, chat_draft);
    lv_obj_set_style_bg_color(chat_input, lv_color_hex(theme()->surface), 0);
    lv_obj_set_style_text_color(chat_input, lv_color_hex(theme()->text), 0);
    lv_obj_set_style_text_font(chat_input, &operit_font_zh_14, 0);
    lv_obj_set_style_radius(chat_input, 9, 0);
    lv_obj_set_style_border_width(chat_input, 1, LV_PART_MAIN);
    lv_obj_set_style_border_color(chat_input, lv_color_hex(theme()->border), LV_PART_MAIN);
    lv_obj_set_style_border_color(chat_input, lv_color_hex(theme()->accent), LV_STATE_FOCUSED);
    lv_obj_set_style_outline_width(chat_input, 0, LV_STATE_FOCUSED);
    lv_obj_set_style_pad_hor(chat_input, 10, 0);
    lv_obj_set_style_pad_top(chat_input, 7, 0);
    lv_obj_set_style_pad_bottom(chat_input, 6, 0);
    lv_obj_set_style_text_color(chat_input, lv_color_hex(theme()->text_secondary), LV_PART_TEXTAREA_PLACEHOLDER);
    lv_obj_set_style_border_color(chat_input, lv_color_hex(theme()->accent), LV_PART_CURSOR);
    lv_obj_add_event_cb(chat_input, chat_input_event, LV_EVENT_ALL, NULL);
    chat_send_button = ui_action(composer_row, "↑", "edge_send", 226, 0, 34,
                                 OPERIT_COMPOSER_HEIGHT, theme()->accent, theme()->on_accent);
    debug_set_id(chat_send_button, "send_button");
    chat_voice_button = ui_action(composer_row, "", "voice_placeholder", 266, 0, 38,
                                  OPERIT_COMPOSER_HEIGHT, theme()->surface, theme()->disabled);
    debug_set_id(chat_voice_button, "voice_button");
    lv_obj_add_state(chat_voice_button, LV_STATE_DISABLED);
    lv_obj_clear_flag(chat_voice_button, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_t *mic = box(chat_voice_button, 16, 7, 6, 12, theme()->disabled, 3);
    lv_obj_clear_flag(mic, LV_OBJ_FLAG_CLICKABLE);
    mic = box(chat_voice_button, 12, 13, 14, 10, theme()->surface, 6);
    lv_obj_set_style_bg_opa(mic, LV_OPA_TRANSP, 0);
    lv_obj_set_style_border_width(mic, 1, 0);
    lv_obj_set_style_border_color(mic, lv_color_hex(theme()->disabled), 0);
    lv_obj_clear_flag(mic, LV_OBJ_FLAG_CLICKABLE);
    mic = box(chat_voice_button, 18, 23, 2, 4, theme()->disabled, 0);
    lv_obj_clear_flag(mic, LV_OBJ_FLAG_CLICKABLE);

    keyboard_window = ui_box(overlay_host, 8, OPERIT_SCREEN_HEIGHT - OPERIT_KEYBOARD_HEIGHT - 2,
                             OPERIT_KEYBOARD_WIDTH, OPERIT_KEYBOARD_HEIGHT, theme()->surface, 10);
    debug_set_id(keyboard_window, "keyboard_window");
    keyboard_header = ui_box(keyboard_window, 0, 0, OPERIT_KEYBOARD_WIDTH, 24,
                             theme()->surface, 10);
    debug_set_id(keyboard_header, "keyboard_header");
    label(keyboard_header, "拖动", 12, 3, 40, theme()->text_secondary);
    keyboard_drag_handle = ui_box(keyboard_header, 133, 7, 38, 4, theme()->text_secondary, 2);
    debug_set_id(keyboard_drag_handle, "keyboard_drag_handle");
    lv_obj_clear_flag(keyboard_drag_handle, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_add_flag(keyboard_header, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_add_event_cb(keyboard_header, keyboard_handle_event, LV_EVENT_ALL, NULL);
    keyboard_close = ui_action(keyboard_header, LV_SYMBOL_CLOSE, "chat_keyboard_close",
                               278, 0, 26, 24, theme()->surface, theme()->text_secondary);
    debug_set_id(keyboard_close, "keyboard_close");
    chat_keyboard = lv_keyboard_create(keyboard_window);
    lv_obj_remove_event_cb(chat_keyboard, lv_keyboard_def_event_cb);
    lv_keyboard_set_map(chat_keyboard, LV_KEYBOARD_MODE_TEXT_LOWER, keyboard_lower, keyboard_controls);
    lv_keyboard_set_map(chat_keyboard, LV_KEYBOARD_MODE_TEXT_UPPER, keyboard_upper, keyboard_controls);
    lv_keyboard_set_map(chat_keyboard, LV_KEYBOARD_MODE_SPECIAL, keyboard_numbers, keyboard_controls);
    lv_obj_add_event_cb(chat_keyboard, keyboard_value_changed, LV_EVENT_VALUE_CHANGED, NULL);
    debug_register(chat_keyboard, "keyboard", "key_area", NULL);
    lv_keyboard_set_textarea(chat_keyboard, chat_input);
    lv_keyboard_set_popovers(chat_keyboard, false);
    lv_obj_set_align(chat_keyboard, LV_ALIGN_TOP_LEFT);
    lv_obj_set_pos(chat_keyboard, 0, 24);
    lv_obj_set_size(chat_keyboard, OPERIT_KEYBOARD_WIDTH, OPERIT_KEYBOARD_HEIGHT - 24);
    lv_obj_set_style_text_font(chat_keyboard, &operit_font_zh_14, 0);
    lv_obj_set_style_bg_color(chat_keyboard, lv_color_hex(theme()->surface), 0);
    lv_obj_set_style_pad_all(chat_keyboard, 4, 0);
    lv_obj_set_style_pad_row(chat_keyboard, 3, 0);
    lv_obj_set_style_pad_column(chat_keyboard, 3, 0);
    lv_obj_set_style_bg_color(chat_keyboard, lv_color_hex(theme()->selection), LV_PART_ITEMS);
    lv_obj_set_style_text_color(chat_keyboard, lv_color_hex(theme()->text), LV_PART_ITEMS);
    lv_obj_set_style_bg_color(chat_keyboard, lv_color_hex(theme()->accent), LV_PART_ITEMS | LV_STATE_CHECKED);
    lv_obj_set_style_text_color(chat_keyboard, lv_color_hex(theme()->on_accent), LV_PART_ITEMS | LV_STATE_CHECKED);
    lv_obj_set_style_border_width(chat_keyboard, 0, LV_PART_ITEMS);
    lv_obj_set_style_shadow_width(chat_keyboard, 0, LV_PART_ITEMS);
    lv_obj_set_style_radius(chat_keyboard, 4, LV_PART_ITEMS);
    lv_obj_set_style_bg_color(chat_keyboard, lv_color_hex(theme()->accent), LV_PART_ITEMS | LV_STATE_PRESSED);
    lv_obj_set_style_text_color(chat_keyboard, lv_color_hex(theme()->on_accent), LV_PART_ITEMS | LV_STATE_PRESSED);
    lv_obj_add_event_cb(chat_keyboard, chat_keyboard_event, LV_EVENT_ALL, NULL);
    lv_obj_add_flag(keyboard_window, LV_OBJ_FLAG_HIDDEN);
    draw_drawer();
    layout_chat_keyboard();
}

static void draw_pairing(void) {
    lv_obj_t *pairing_page = ui_box(page_host, 0, 0, OPERIT_SCREEN_WIDTH,
                                    OPERIT_SCREEN_HEIGHT, theme()->background, 0);
    debug_register(pairing_page, "screen", "pairing_page", NULL);
    lv_obj_t *header = ui_box(pairing_page, 0, 0, 320, 40, theme()->header, 0);
    debug_set_id(header, "pairing_header");
    label(header, "连接 Operit", 12, 10, 180, theme()->text);
    label(header, LV_SYMBOL_WIFI, 272, 10, 24, wifi_ready ? theme()->accent : theme()->text_secondary);
    lv_obj_t *content = ui_box(pairing_page, 0, 40, 320, 156, theme()->background, 0);
    debug_set_id(content, "pairing_content");
    label(content, "在 Operit 中添加此设备", 12, 14, 296, theme()->text);
    label(content, "设备 → 添加边缘设备", 12, 36, 296, theme()->text_secondary);
    lv_obj_t *code_card = ui_box(content, 12, 61, 296, 67, theme()->surface, 10);
    debug_set_id(code_card, "pairing_code_card");
    lv_obj_t *caption = label(code_card, "配对码", 0, 6, 296, theme()->text_secondary);
    lv_obj_set_style_text_align(caption, LV_TEXT_ALIGN_CENTER, 0);
    pairing_label = label(code_card, pairing_code[0] ? pairing_code : "等待配对码",
                          0, 28, 296, theme()->accent);
    debug_set_id(pairing_label, "pairing_code");
    if (pairing_code[0]) lv_obj_set_style_text_font(pairing_label, &operit_font_digits_28, 0);
    lv_obj_set_style_text_align(pairing_label, LV_TEXT_ALIGN_CENTER, 0);
    pairing_hint = label(content, pairing_code[0] ? "请在 Operit 中确认配对" : "等待 Core 发起配对…",
                         30, 136, 260, theme()->text_secondary);
    debug_set_id(pairing_hint, "pairing_status");
    lv_obj_t *status_dot = ui_box(content, 12, 141, 6, 6,
                                  pairing_code[0] ? theme()->accent : theme()->text_secondary, 3);
    debug_set_id(status_dot, "pairing_status_dot");
    ui_action(pairing_page, "连接帮助", "dialog_help", 12, 204, 296, 28,
              theme()->surface, theme()->text_secondary);
}

#include "operit_image_view.inc"

static void page(const char *name) {
    copy_utf8(page_name, sizeof(page_name), name);
    current_page = page_name;
    name = page_name;
    if (strcmp(name, "Chat")) sidebar_open = false;
    if (chat_input) remember_chat_draft();
    clear();
    ui_hosts();
    if (!strcmp(name, "Chat")) {
        draw_chat();
    } else if (!strcmp(name, "Pairing")) {
        draw_pairing();
    } else if (!strcmp(name, "Settings")) {
        label(page_host, "设置", 18, 14, 200, theme()->text);
        wifi_label = label(page_host, wifi_ready ? "Wi-Fi 已连接" : "Wi-Fi 不可用", 18, 43, 280, theme()->text);
        space_label = label(page_host, edge_ready ? space_state : "等待 Core 连接", 18, 64, 280, theme()->text_secondary);
        button(page_host, "切换主题", "Palette", 18, 88, 284, 28);
        label(page_host, "Emoji", 18, 124, 90, theme()->text_secondary);
        lv_obj_t *preview = label(page_host, "🐖 🥺 😊 ❤️", 124, 123, 178, theme()->text);
        debug_register(preview, "label", "emoji_preview", "");
        const char *styles[] = {"原有单色", "Google 彩色"};
        const char *actions[] = {"emoji_mono", "emoji_google"};
        for (unsigned i = 0; i < OPERIT_EMOJI_STYLE_COUNT; ++i) {
            if (i == OPERIT_EMOJI_GOOGLE && !operit_emoji_color_available()) continue;
            lv_obj_t *choice = button(page_host, styles[i], actions[i], 18 + i * 146, 151, 138, 32);
            if (operit_emoji_style() == i) {
                lv_obj_set_style_border_width(choice, 2, 0);
                lv_obj_set_style_border_color(choice, lv_color_hex(theme()->accent), 0);
            }
        }
        button(page_host, "返回聊天", "edge_chat", 18, 204, 284, 28);
    } else if (!strcmp(name, "Tasks")) {
        label(page_host, "任务", 18, 14, 200, theme()->text);
        label(page_host, "当前任务", 18, 58, 284, theme()->text_secondary);
        chat_task_label = label(page_host, chat_task, 18, 82, 284, theme()->accent);
        button(page_host, "返回聊天", "edge_chat", 18, 181, 284, 36);
    } else {
        label(page_host, name, 18, 14, 250, theme()->accent);
        label(page_host, "暂无已安装内容", 20, 92, 280, theme()->text_secondary);
        button(page_host, "返回聊天", "edge_chat", 18, 181, 284, 36);
    }
}

static void flush(lv_display_t *d, const lv_area_t *a, uint8_t *pixels) {
    operit_lvgl_area_t area = {a->x1, a->y1, a->x2, a->y2};
    flush_cb(&area, pixels, (a->x2-a->x1+1)*(a->y2-a->y1+1)*2, context);
    lv_display_flush_ready(d);
}
static void read_touch(lv_indev_t *dev, lv_indev_data_t *data) {
    (void)dev; data->point.x=touch_x; data->point.y=touch_y;
    data->state=touch_pressed ? LV_INDEV_STATE_PRESSED : LV_INDEV_STATE_RELEASED;
}
bool operit_lvgl_init(uint16_t w, uint16_t h, operit_lvgl_flush_cb_t f, operit_lvgl_touch_cb_t t, operit_lvgl_action_cb_t a, void *user) {
    (void)t; if (w!=320 || h!=240 || !f) return false;
    flush_cb=f; action_cb=a; context=user; lv_init();operit_store_init();operit_emoji_init();
    display=lv_display_create(w,h); if (!display) return false;
    lv_display_set_color_format(display, LV_COLOR_FORMAT_RGB565);
    lv_display_set_buffers(display, draw_buffer, NULL, sizeof(draw_buffer), LV_DISPLAY_RENDER_MODE_PARTIAL);
    lv_display_set_flush_cb(display, flush);
    lv_indev_t *input=lv_indev_create(); if (!input) return false; pointer_input=input;
    lv_indev_set_type(input,LV_INDEV_TYPE_POINTER); lv_indev_set_read_cb(input,read_touch);
    root=lv_obj_create(NULL); lv_obj_remove_style_all(root);
    lv_obj_set_size(root,w,h); lv_obj_set_style_bg_opa(root,LV_OPA_COVER,0);
    lv_obj_clear_flag(root,LV_OBJ_FLAG_SCROLLABLE); lv_screen_load(root);
    home(); lv_timer_create(tick_clock,1000,NULL); last_tick=esp_timer_get_time(); return true;
}
void operit_lvgl_pump(uint32_t elapsed_ms) {
    if(operit_store_poll()){touch_pressed=false;lv_indev_reset(pointer_input,NULL);lv_indev_wait_release(pointer_input);home();}
    (void)elapsed_ms; int64_t now=esp_timer_get_time();
    uint32_t ms=(uint32_t)((now-last_tick)/1000); last_tick+=(int64_t)ms*1000;
    lv_tick_inc(ms); lv_timer_handler();
    if(*pending_route) { char action[sizeof(pending_route)];memcpy(action,pending_route,sizeof(action));pending_route[0]=0;lv_indev_reset(pointer_input,NULL);lv_indev_wait_release(pointer_input);execute_route(action); }
}
void operit_lvgl_set_touch(uint16_t x,uint16_t y,bool pressed) {
    if (pressed && !last_touch_pressed) {
        touch_start_x = x; touch_start_y = y;
    } else if (!pressed && last_touch_pressed) {
        int dx = (int)x - (int)touch_start_x;
        int dy = (int)y - (int)touch_start_y;
        if (abs(dx) > 35 && abs(dx) > abs(dy) && !strcmp(current_page, "Chat")) {
            if (!sidebar_open && touch_start_x <= 24 && dx > 35) queue_route("sidebar_open");
            else if (sidebar_open && dx < -35) queue_route("sidebar_close");
        } else if (abs(dy) > 45 && abs(dy) > abs(dx) && !strcmp(current_page, "Chat") &&
                   !sidebar_open && !image_layer && message_viewport &&
                   touch_start_x >= (uint16_t)lv_obj_get_x(message_viewport) &&
                   touch_start_x < (uint16_t)(lv_obj_get_x(message_viewport) + lv_obj_get_width(message_viewport)) &&
                   touch_start_y >= (uint16_t)lv_obj_get_y(message_viewport) &&
                   touch_start_y < (uint16_t)(lv_obj_get_y(message_viewport) + lv_obj_get_height(message_viewport)) &&
                   (!keyboard_window || lv_obj_has_flag(keyboard_window, LV_OBJ_FLAG_HIDDEN))) {
            int max_scroll = lv_obj_get_height(message_list) - lv_obj_get_height(message_viewport);
            if (max_scroll < 0) max_scroll = 0;
            if (dy > 0 && message_scroll_y > 0) {
                message_scroll_y = LV_MAX(0, message_scroll_y - dy);
                lv_obj_set_y(message_list, -message_scroll_y);
            } else if (dy < 0 && message_scroll_y < max_scroll) {
                message_scroll_y = LV_MIN(max_scroll, message_scroll_y - dy);
                lv_obj_set_y(message_list, -message_scroll_y);
            } else page_message(dy > 0 ? -1 : 1);
        }
    }
    last_touch_pressed = pressed;
    touch_x=x; touch_y=y; touch_pressed=pressed;
}
void operit_lvgl_navigate_home(void) {home();}
void operit_lvgl_set_connection(bool wifi,bool edge) {
    if(wifi==wifi_ready && edge==edge_ready) return;
    wifi_ready=wifi; edge_ready=edge;
    if(connection_label) {
        lv_label_set_text(connection_label, edge ? "已连接" : "离线");
        lv_obj_set_style_text_color(connection_label, lv_color_hex(edge ? theme()->accent : theme()->text_secondary), 0);
    }
    if(wifi_label) lv_label_set_text(wifi_label, wifi ? "Wi-Fi 已连接" : "Wi-Fi 不可用");
    if(space_label) lv_label_set_text(space_label, edge ? space_state : "等待 Core 连接");
}
void operit_lvgl_set_paired(bool value) {
    if (paired == value && !unpair_requested) return;
    paired = value;
    unpair_requested = false;
    if (!paired) clear_chat_cache();
    if (paired && !strcmp(current_page, "Pairing")) page("Chat");
    else if (!paired && strcmp(current_page, "Pairing")) page("Pairing");
}
void operit_lvgl_set_pairing_code(const char *code) {
    const char *value = code ? code : "";
    if (!strcmp(pairing_code, value)) return;
    copy_utf8(pairing_code, sizeof(pairing_code), value);
    if (pairing_label) {
        lv_label_set_text(pairing_label, pairing_code[0] ? pairing_code : "等待配对码");
        lv_obj_set_style_text_font(pairing_label, pairing_code[0] ? &operit_font_digits_28 : &operit_font_zh_14, 0);
    }
    if (pairing_hint) lv_label_set_text(pairing_hint, pairing_code[0] ? "请在 Operit 中确认配对" : "等待 Core 发起配对…");
}
void operit_lvgl_set_space_state(const char *state) {
    const char *value = state ? state : "等待连接 Operit";
    if (!strcmp(space_state, value)) return;
    copy_utf8(space_state, sizeof(space_state), value);
    if (space_label) lv_label_set_text(space_label, space_state);
}
void operit_lvgl_set_chat_preview(const char *preview) {
    const char *value = preview ? preview : "尚未连接对话";
    if (!strcmp(chat_preview, value)) return;
    copy_utf8(chat_preview, sizeof(chat_preview), value);
    if (sidebar_preview_label) lv_label_set_text(sidebar_preview_label, chat_preview);
    if (chat_title_label) lv_label_set_text(chat_title_label, chat_preview[0] ? chat_preview : "Operit");
}
void operit_lvgl_set_expression(const char *value) {
    if(!value || !strcmp(expression,value)) return;
    copy_utf8(expression, sizeof(expression), value);
    if(face_label) lv_label_set_text(face_label,expression);
}

/* Host controls shared by the firmware and the WebAssembly developer host. */
void operit_lvgl_set_chat_screen(const char *text) {
    if (!text || !strcmp(chat_screen, text)) return;
    copy_utf8(chat_screen, sizeof(chat_screen), text);
    render_messages();
}
void operit_lvgl_set_chat_task(const char *text) {
    if (!text || !strcmp(chat_task, text)) return;
    copy_utf8(chat_task, sizeof(chat_task), text);
    if (chat_task_label) lv_label_set_text(chat_task_label, chat_task);
}
const char *operit_lvgl_chat_draft(void) { remember_chat_draft(); return chat_draft; }
void operit_lvgl_set_chat_draft(const char *text) {
    copy_utf8(chat_draft, sizeof(chat_draft), text);
    if (chat_input) lv_textarea_set_text(chat_input, chat_draft);
}
void operit_lvgl_submit_chat(void) { queue_route("edge_send"); }
void operit_lvgl_chat_send_result(bool ok, const char *error) {
    if (!chat_send_pending) return;
    chat_send_pending = false;
    remember_chat_draft();
    if (ok && !strcmp(chat_draft, submitted_draft)) operit_lvgl_set_chat_draft("");
    if (chat_send_button) lv_obj_clear_state(chat_send_button, LV_STATE_DISABLED);
    if (!ok) show_dialog(error && *error ? error : "发送失败，请重试", false);
}

void operit_lvgl_set_theme(unsigned index, bool circular) {
    theme_index = index % (sizeof(themes) / sizeof(themes[0]));
    round_icons = circular;
    page(current_page);
}

unsigned operit_lvgl_emoji_style(void) { return operit_emoji_style(); }
bool operit_lvgl_set_emoji_style(unsigned style) {
    if (!operit_emoji_set_style(style)) return false;
    page(current_page);
    return true;
}
void operit_lvgl_navigate_apps(void) {
    sidebar_open = true;
    page("Chat");
}
static void home(void) {
    if (OPERIT_LAYOUT_ENABLED) document_home();
    else builtin_home();
}

unsigned operit_lvgl_theme_index(void) { return theme_index; }
bool operit_lvgl_round_icons(void) { return round_icons; }

const char *operit_lvgl_current_page(void) {
    if (tiles && lv_tileview_get_tile_active(tiles) == lv_obj_get_child(tiles, 1)) return "Apps";
    return current_page;
}

const char *operit_lvgl_debug_tree(void) {
    return debug_build_json(false);
}

const char *operit_lvgl_debug_snapshot(void) {
    return debug_build_json(true);
}

bool operit_lvgl_debug_tap(const char *id) {
    if (!id || !*id) return false;
    for (unsigned i = 0; i < debug_count; ++i) {
        operit_debug_node_t *node = &debug_nodes[i];
        if (strcmp(node->id, id) || !node->object || !debug_visible(node->object) ||
            !lv_obj_has_flag(node->object, LV_OBJ_FLAG_CLICKABLE)) continue;
        lv_area_t area;
        lv_obj_get_coords(node->object, &area);
        uint16_t x = (uint16_t)((area.x1 + area.x2) / 2);
        uint16_t y = (uint16_t)((area.y1 + area.y2) / 2);
        operit_lvgl_set_touch(x, y, true);
        lv_indev_read(pointer_input);
        operit_lvgl_pump(0);
        operit_lvgl_set_touch(x, y, false);
        lv_indev_read(pointer_input);
        operit_lvgl_pump(0);
        /* A route is intentionally consumed by the next pump so callbacks
         * and page transitions follow the exact same path as a real tap. */
        operit_lvgl_pump(0);
        return true;
    }
    return false;
}

bool operit_lvgl_debug_swipe(const char *direction) {
    if (!direction || !*direction || strcmp(current_page, "Chat")) return false;
    bool right = !strcmp(direction, "right") || !strcmp(direction, "open");
    bool left = !strcmp(direction, "left") || !strcmp(direction, "close");
    if (!right && !left) return false;
    uint16_t start = right ? 4 : 300;
    uint16_t end = right ? 150 : 170;
    operit_lvgl_set_touch(start, 120, true);
    operit_lvgl_pump(0);
    operit_lvgl_set_touch(end, 120, false);
    operit_lvgl_pump(0);
    operit_lvgl_pump(0);
    return true;
}

/* The editor and firmware execute this same component factory. */
static lv_obj_t *editor_nodes[24];
static char editor_text[24][161], editor_action[24][24], editor_long_action[24][24];
static unsigned editor_count;
static void layout_action(lv_event_t *event) {
    unsigned index = (unsigned)(uintptr_t)lv_event_get_user_data(event);
    if(index >= editor_count) return;
    lv_event_code_t code = lv_event_get_code(event);
    const char *binding = NULL;
    if(code == LV_EVENT_LONG_PRESSED && *editor_long_action[index]) binding = editor_long_action[index];
    else if(code == (*editor_long_action[index] ? LV_EVENT_SHORT_CLICKED : LV_EVENT_CLICKED)) binding = editor_action[index];
    if(!binding || !*binding) return;
    queue_route(binding);
}
void operit_lvgl_layout_clear(uint32_t background) {
    pending_route[0]=0; swipe_left[0]=swipe_right[0]=0;
    clear(); current_page = "Layout"; editor_count = 0;
    memset(editor_nodes, 0, sizeof(editor_nodes));
    lv_obj_set_style_bg_color(root, lv_color_hex(document_color(background)), 0);
}
int operit_lvgl_layout_add(int type, int parent_index, int x, int y, int w, int h,
    uint32_t color, int radius, int value, const char *text, const char *action) {
    if (editor_count >= 24 || w < 8 || h < 8) return -1;
    color=document_color(color);
    if(type==2 && w==h && round_icons)radius=w/2;
    unsigned index = editor_count;
    lv_obj_t *parent = parent_index >= 0 && parent_index < (int)index ? editor_nodes[parent_index] : root;
    strncpy(editor_text[index], text ? text : "", 160); editor_text[index][160] = 0;
    strncpy(editor_action[index], action ? action : "", 23); editor_action[index][23] = 0;
    editor_long_action[index][0] = 0;
    const char *content = editor_text[index];
    lv_obj_t *obj = NULL;
    static const char *matrix[] = {"One", "Two", "\n", "Three", "", NULL};
    static const lv_point_precise_t points[] = {{0, 0}, {80, 10}};
    switch (type) {
    case 0: obj=lv_obj_create(parent); lv_obj_remove_style_all(obj); lv_obj_set_style_bg_opa(obj,255,0); break;
    case 1: obj=lv_label_create(parent); lv_label_set_text(obj,content); break;
    case 2: {obj=lv_button_create(parent);lv_obj_t *t=lv_label_create(obj);lv_label_set_text(t,content);lv_obj_center(t);break;}
    case 3: {obj=lv_label_create(parent);const char *symbol=LV_SYMBOL_EYE_OPEN;
        if(!strcmp(content,"wifi"))symbol=LV_SYMBOL_WIFI;else if(!strcmp(content,"settings"))symbol=LV_SYMBOL_SETTINGS;
        else if(!strcmp(content,"home"))symbol=LV_SYMBOL_HOME;else if(!strcmp(content,"play"))symbol=LV_SYMBOL_PLAY;
        else if(!strcmp(content,"folder")||!strcmp(content,"plugins"))symbol=LV_SYMBOL_DIRECTORY;
        else if(!strcmp(content,"theme"))symbol=LV_SYMBOL_TINT;else if(!strcmp(content,"terminal"))symbol=LV_SYMBOL_LIST;
        lv_label_set_text(obj,symbol);lv_obj_set_style_text_align(obj,LV_TEXT_ALIGN_CENTER,0);break;}
    case 4: obj=lv_arc_create(parent);lv_arc_set_value(obj,value);break;
    case 5: obj=lv_bar_create(parent);lv_bar_set_value(obj,value,LV_ANIM_OFF);break;
    case 6: obj=lv_slider_create(parent);lv_slider_set_value(obj,value,LV_ANIM_OFF);break;
    case 7: obj=lv_switch_create(parent);if(value>=50)lv_obj_add_state(obj,LV_STATE_CHECKED);break;
    case 8: obj=lv_checkbox_create(parent);lv_checkbox_set_text(obj,content);if(value>=50)lv_obj_add_state(obj,LV_STATE_CHECKED);break;
    case 9: obj=lv_dropdown_create(parent);lv_dropdown_set_options(obj,content);break;
    case 10: obj=lv_roller_create(parent);lv_roller_set_options(obj,content,LV_ROLLER_MODE_NORMAL);break;
    case 11: obj=lv_textarea_create(parent);lv_textarea_set_text(obj,content);break;
    case 12: obj=lv_spinbox_create(parent);lv_spinbox_set_range(obj,0,100);lv_spinbox_set_digit_format(obj,3,0);lv_spinbox_set_value(obj,value);break;
    case 13: obj=lv_led_create(parent);lv_led_set_color(obj,lv_color_hex(color));lv_led_set_brightness(obj,value*255/100);break;
    case 14: obj=lv_spinner_create(parent);break;
    case 15: obj=lv_line_create(parent);lv_line_set_points(obj,points,2);lv_obj_set_style_line_color(obj,lv_color_hex(color),0);break;
    case 16: {obj=lv_chart_create(parent);lv_chart_set_point_count(obj,8);lv_chart_series_t *series=lv_chart_add_series(obj,lv_color_hex(color),LV_CHART_AXIS_PRIMARY_Y);lv_chart_set_all_value(obj,series,value);break;}
    case 17: obj=lv_table_create(parent);lv_table_set_cell_value(obj,0,0,content);lv_table_set_cell_value(obj,1,0,"Value");break;
    case 18: obj=lv_buttonmatrix_create(parent);lv_buttonmatrix_set_map(obj,matrix);break;
    case 19: obj=lv_list_create(parent);lv_list_add_button(obj,LV_SYMBOL_DIRECTORY,content);break;
    case 20: obj=lv_calendar_create(parent);lv_calendar_set_showed_date(obj,2026,9);break;
    case 21: obj=lv_keyboard_create(parent);for(unsigned i=0;i<index;i++)if(lv_obj_check_type(editor_nodes[i],&lv_textarea_class)){lv_keyboard_set_textarea(obj,editor_nodes[i]);break;}break;
    case 22: {obj=lv_tabview_create(parent);lv_tabview_add_tab(obj,"One");lv_tabview_add_tab(obj,"Two");break;}
    case 23: obj=lv_tileview_create(parent);lv_tileview_add_tile(obj,0,0,LV_DIR_RIGHT);lv_tileview_add_tile(obj,1,0,LV_DIR_LEFT);break;
    case 24: obj=lv_scale_create(parent);lv_scale_set_mode(obj,LV_SCALE_MODE_HORIZONTAL_BOTTOM);lv_scale_set_range(obj,0,100);break;
    case 25: {obj=lv_spangroup_create(parent);lv_span_t *span=lv_spangroup_add_span(obj);lv_span_set_text(span,content);break;}
    case 26: {obj=lv_menu_create(parent);lv_obj_t *p=lv_menu_page_create(obj,NULL);lv_obj_t *l=lv_label_create(p);lv_label_set_text(l,content);lv_menu_set_page(obj,p);break;}
    case 27: obj=lv_msgbox_create(parent);lv_msgbox_add_title(obj,content);lv_msgbox_add_text(obj,"Message");break;
    case 28: obj=lv_win_create(parent);lv_win_add_title(obj,content);break;
    default: return -1;
    }
    if(!obj)return -1;
    editor_nodes[index]=obj;editor_count++;
    lv_obj_set_pos(obj,x,y);lv_obj_set_size(obj,w,h);
    if(type==0||type==2){lv_obj_set_style_bg_color(obj,lv_color_hex(color),0);lv_obj_set_style_radius(obj,radius,0);lv_obj_set_style_pad_all(obj,0,0);lv_obj_clear_flag(obj,LV_OBJ_FLAG_SCROLLABLE);}
    if(type==1||type==3||type==25)lv_obj_set_style_text_color(obj,lv_color_hex(color),0);
    lv_obj_add_event_cb(obj,layout_action,LV_EVENT_ALL,(void *)(uintptr_t)index);
    if(*editor_action[index])lv_obj_add_flag(obj,LV_OBJ_FLAG_CLICKABLE);
    return index;
}
void operit_lvgl_layout_bind(int index,const char *action,const char *long_action) {
    if(index<0 || index>=(int)editor_count)return;
    strncpy(editor_action[index],action ? action : "",23);editor_action[index][23]=0;
    strncpy(editor_long_action[index],long_action ? long_action : "",23);editor_long_action[index][23]=0;
    if(*editor_action[index] || *editor_long_action[index])lv_obj_add_flag(editor_nodes[index],LV_OBJ_FLAG_CLICKABLE);
}
void operit_lvgl_layout_geometry(int index,int x,int y,int w,int h) {
    if(index<0 || index>=(int)editor_count)return;
    lv_obj_set_pos(editor_nodes[index],x,y);lv_obj_set_size(editor_nodes[index],w,h);
}
static void document_gesture(lv_event_t *event) {
    (void)event;lv_dir_t direction=lv_indev_get_gesture_dir(pointer_input);
    const char *target=direction==LV_DIR_LEFT?swipe_left:direction==LV_DIR_RIGHT?swipe_right:"";
    if(*target){char action[32];snprintf(action,sizeof(action),"go:%s",target);queue_route(action);}
}
void operit_lvgl_layout_page_meta(const char *id,const char *left,const char *right) {
    strncpy(active_page,id?id:"home",23);active_page[23]=0;current_page=active_page;
    strncpy(swipe_left,left?left:"",23);swipe_left[23]=0;strncpy(swipe_right,right?right:"",23);swipe_right[23]=0;
    lv_obj_remove_event_cb(root,document_gesture);lv_obj_add_event_cb(root,document_gesture,LV_EVENT_GESTURE,NULL);
}
void operit_lvgl_layout_style(int index,const char *binding,int font_size) {
    if(index<0 || index>=(int)editor_count)return;lv_obj_t *obj=editor_nodes[index];
    if(font_size==48)lv_obj_set_style_text_font(obj,&lv_font_montserrat_48,0);
    if(!lv_obj_check_type(obj,&lv_label_class)||!binding)return;
    if(!strcmp(binding,"clock")){clock_label=obj;tick_clock(NULL);}
    else if(!strcmp(binding,"connection")){connection_label=obj;lv_label_set_text(obj,wifi_ready?"WIFI CONNECTED":"WIFI STARTING");}
    else if(!strcmp(binding,"expression")){face_label=obj;lv_label_set_text(obj,expression);}
    else if(!strcmp(binding,"theme")){lv_label_set_text(obj,theme()->name);}
    else if(!strcmp(binding,"pairing")){pairing_label=obj;lv_label_set_text(obj,pairing_code[0]?pairing_code:"等待配对码");}
    else if(!strcmp(binding,"space")){space_label=obj;lv_label_set_text(obj,space_state);}
    else if(!strcmp(binding,"chat")){chat_label=obj;lv_label_set_text(obj,chat_preview);}
}
static bool document_page(const char *id) {
    operit_packed_page_t packed;
    if(operit_store_page(id,&packed)){
        operit_lvgl_layout_clear(packed.color);const uint8_t *data=packed.nodes;
        for(unsigned i=0;i<packed.count;i++){
            operit_packed_node_t n;char strings[161+24+24+17];data=operit_store_node(data,&n,strings);
            int index=operit_lvgl_layout_add(n.type,n.parent==255?-1:n.parent,n.x,n.y,n.w,n.h,n.color,n.radius,n.value,n.text,n.action);
            operit_lvgl_layout_bind(index,n.action,n.hold);operit_lvgl_layout_style(index,n.binding,n.font);
        }
        operit_lvgl_layout_page_meta(packed.id,packed.left,packed.right);return true;
    }
    for(unsigned p=0;p<OPERIT_PAGE_COUNT;p++)if(!strcmp(layout_pages[p].id,id)){
        const operit_layout_page_t *page=&layout_pages[p];
        operit_lvgl_layout_clear(page->background);
        for(unsigned i=0;i<page->count;i++){
            const operit_layout_node_t *n=&page->nodes[i];
            int index=operit_lvgl_layout_add(n->type,n->parent,n->x,n->y,n->w,n->h,n->color,n->radius,n->value,n->text,n->action);
            operit_lvgl_layout_bind(index,n->action,n->long_action);
            operit_lvgl_layout_style(index,n->binding,n->font_size);
        }
        operit_lvgl_layout_page_meta(page->id,page->swipe_left,page->swipe_right);return true;
    }
    return false;
}
static void navigate_document(const char *id) {
#ifdef __EMSCRIPTEN__
    if(action_cb){char action[32];snprintf(action,sizeof(action),"navigate:%s",id);action_cb(action,context);return;}
#endif
    document_page(id);
}
static void execute_route(const char *action) {
    if(!strncmp(action,"go:",3)) navigate_document(action+3);
    else if(!strcmp(action,"home")) builtin_home();
    else if(!strcmp(action,"apps")) { sidebar_open = true; page("Chat"); }
    else if(!strncmp(action,"page:",5)) navigate_document(action+5);
    else if(!strcmp(action,"theme_next") || !strcmp(action,"shape_toggle")) {
        if(!strcmp(action,"theme_next")) theme_index=(theme_index+1)%(sizeof(themes)/sizeof(themes[0])); else round_icons=!round_icons;
        page(current_page);
    }
    else if(!strcmp(action,"emoji_mono") || !strcmp(action,"emoji_google")) {
        unsigned style = !strcmp(action,"emoji_google") ? OPERIT_EMOJI_GOOGLE : OPERIT_EMOJI_MONO;
        if (!operit_lvgl_set_emoji_style(style)) show_dialog("无法保存表情风格，请重试", false);
    }
    else if(!strncmp(action,"builtin:",8)) {
        page(action+8);
        if(!strcmp(action+8,"Pairing") && !edge_ready && action_cb) action_cb("edge_pair",context);
    }
    else if(!strcmp(action,"sidebar_open")) set_sidebar_open(true, true);
    else if(!strcmp(action,"sidebar_close")) set_sidebar_open(false, true);
    else if(!strcmp(action,"sidebar_toggle")) set_sidebar_open(!sidebar_open, true);
    else if(!strcmp(action,"chat_keyboard_close")) hide_chat_keyboard();
    else if(!strcmp(action,"image_close")) close_image();
    else if(!strncmp(action,"image_open:",11)) show_image(action+11);
    else if(!strcmp(action,"edge_new") || !strncmp(action,"edge_select:",12)) {
        if (!edge_ready) { show_dialog("设备已离线，请等待重新连接", false); return; }
        if(action_cb) action_cb(action,context);
    }
    else if(!strcmp(action,"edge_chat")) { sidebar_open=false; page("Chat"); if(action_cb) action_cb(action,context); }
    else if(!strcmp(action,"edge_search") || !strcmp(action,"edge_pair")) {
        sidebar_open=false;
        page("Pairing");
        if(action_cb) action_cb(action,context);
    }
    else if(!strcmp(action,"edge_unpair")) show_dialog("取消配对后，需要重新连接此设备。确定继续？", true);
    else if(!strcmp(action,"dialog_close")) close_dialog();
    else if(!strcmp(action,"dialog_confirm")) {
        close_dialog();
        unpair_requested = true;
        if(action_cb) action_cb("edge_unpair",context);
    }
    else if(!strcmp(action,"dialog_help")) show_dialog("打开 Operit 的设备设置，添加边缘设备。确保设备在同一网络，然后选择此设备并输入屏幕上的配对码。", false);
    else if(!strcmp(action,"edge_send")) {
        if (chat_send_pending) return;
        remember_chat_draft();
        if (!chat_draft[0]) { show_dialog("请输入消息", false); return; }
        if (!edge_ready) { show_dialog("设备已离线，草稿已保留", false); return; }
        copy_utf8(submitted_draft, sizeof(submitted_draft), chat_draft);
        chat_send_pending = true;
        if (chat_send_button) lv_obj_add_state(chat_send_button, LV_STATE_DISABLED);
        hide_chat_keyboard();
        operit_lvgl_set_chat_task("发送中");
        if(action_cb) action_cb(action,context);
        else operit_lvgl_chat_send_result(false, "设备尚未连接");
    }
    else if((!strcmp(action,"face_online") || !strcmp(action,"run_node")) && action_cb) action_cb(action,context);
}
static void document_home(void) { builtin_home(); }
