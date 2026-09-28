/* Bounded display cache only. Core owns conversations and message history.
 * This view is compiled unchanged for the device and the browser. */
enum { MESSAGE_LIMIT = 12, CONVERSATION_LIMIT = 24, MESSAGE_TEXT_BYTES = 2048 };
typedef struct {
    char *text;
    bool user;
    lv_obj_t *row, *bubble, *speaker, *label;
} message_view_t;
typedef struct {
    char id[80], title[128], character[96], action[96];
    bool selected;
} conversation_view_t;
static message_view_t messages[MESSAGE_LIMIT];
static conversation_view_t conversations[CONVERSATION_LIMIT];
static unsigned message_count, conversation_count;
static bool messages_dirty;
static bool conversations_dirty;
static char active_chat_id[80];
static struct { char id[80], text[512]; } drafts[4];
static unsigned draft_cursor;

static void reset_message_nodes(void) {
    for (unsigned i = 0; i < MESSAGE_LIMIT; ++i)
        messages[i].row = messages[i].bubble = messages[i].speaker = messages[i].label = NULL;
}

static void render_messages(void) {
    if (!message_list) return;
    messages_dirty = false;
    bool at_bottom = lv_obj_get_scroll_bottom(message_viewport) <= 4;
    int y = 0;
    if (!chat_label) {
        chat_label = label(message_list, "", 4, 0, 292, theme()->text_secondary);
        debug_set_id(chat_label, "chat_notice");
        lv_label_set_long_mode(chat_label, LV_LABEL_LONG_WRAP);
    }
    for (unsigned i = 0; i < MESSAGE_LIMIT; ++i) {
        message_view_t *m = &messages[i];
        if (i >= message_count) {
            if (m->row) lv_obj_add_flag(m->row, LV_OBJ_FLAG_HIDDEN);
            continue;
        }
        if (!m->row) {
            m->row = ui_box(message_list, 0, y, 304, 1, theme()->background, 0);
            m->bubble = ui_box(m->row, 0, 0, 288, 1, theme()->background, 10);
            m->speaker = label(m->row, chat_preview, 4, 0, 288, theme()->text_secondary);
            m->label = label(m->bubble, "", 4, 0, 280, theme()->text);
            lv_label_set_long_mode(m->label, LV_LABEL_LONG_WRAP);
            lv_obj_set_style_text_line_space(m->label, 3, 0);
            char id[40]; snprintf(id, sizeof(id), "message_%u", i);
            debug_set_id(m->row, id);
            snprintf(id, sizeof(id), "message_text_%u", i);
            debug_set_id(m->label, id);
        }
        lv_obj_clear_flag(m->row, LV_OBJ_FLAG_HIDDEN);
        if (strcmp(lv_label_get_text(m->label), m->text ? m->text : ""))
            lv_label_set_text(m->label, m->text ? m->text : "");
        lv_point_t measured;
        lv_text_get_size(&measured, m->text ? m->text : "", &operit_font_zh_14,
                         0, 3, 228, LV_TEXT_FLAG_NONE);
        int width = m->user ? LV_MIN(measured.x + 16, 244) : 292;
        if (m->user) lv_obj_add_flag(m->speaker, LV_OBJ_FLAG_HIDDEN);
        else { lv_obj_clear_flag(m->speaker, LV_OBJ_FLAG_HIDDEN); lv_label_set_text(m->speaker, chat_preview); }
        lv_obj_set_width(m->label, width - 16);
        lv_obj_set_pos(m->label, 8, m->user ? 8 : 2);
        lv_obj_update_layout(m->label);
        int h = lv_obj_get_height(m->label) + (m->user ? 16 : 4);
        lv_obj_set_pos(m->bubble, m->user ? 304 - width : 0, m->user ? 0 : 22);
        lv_obj_set_size(m->bubble, width, h);
        lv_obj_set_style_bg_color(m->bubble, lv_color_hex(m->user ? theme()->selection : theme()->background), 0);
        h += m->user ? 0 : 22;
        lv_obj_set_pos(m->row, 0, y);
        lv_obj_set_height(m->row, h);
        y += h + 12;
    }
    lv_label_set_text(chat_label, chat_screen);
    lv_obj_set_y(chat_label, y);
    if (chat_screen[0]) {
        lv_obj_clear_flag(chat_label, LV_OBJ_FLAG_HIDDEN);
        lv_obj_update_layout(chat_label);
        y += lv_obj_get_height(chat_label) + 8;
    } else lv_obj_add_flag(chat_label, LV_OBJ_FLAG_HIDDEN);
    lv_obj_set_height(message_list, y > 0 ? y : 1);
    lv_obj_update_layout(message_viewport);
    if (at_bottom) lv_obj_scroll_to_y(message_viewport, LV_COORD_MAX, LV_ANIM_OFF);
}

static void toggle_character(lv_event_t *event) {
    lv_obj_t *card = lv_obj_get_parent(lv_event_get_target(event));
    lv_obj_t *list = lv_obj_get_child(card, 1);
    bool open = lv_obj_has_flag(list, LV_OBJ_FLAG_HIDDEN);
    if (open) lv_obj_clear_flag(list, LV_OBJ_FLAG_HIDDEN);
    else lv_obj_add_flag(list, LV_OBJ_FLAG_HIDDEN);
    lv_label_set_text(lv_obj_get_child(lv_event_get_target(event), 2), open ? LV_SYMBOL_DOWN : LV_SYMBOL_RIGHT);
    lv_obj_set_height(card, open ? 40 + lv_obj_get_height(list) + 8 : 40);
    lv_obj_update_layout(card);
    lv_obj_t *parent = lv_obj_get_parent(card);
    int y = 0;
    for (unsigned i = 0; i < lv_obj_get_child_count(parent); ++i) {
        lv_obj_t *child = lv_obj_get_child(parent, i);
        lv_obj_set_y(child, y); y += lv_obj_get_height(child) + 8;
    }
    lv_obj_set_height(parent, y);
    lv_obj_update_layout(parent);
}

static void render_conversations(void) {
    if (!character_viewport) return;
    int scroll = lv_obj_get_scroll_y(character_viewport);
    lv_obj_clean(character_viewport);
    lv_obj_t *list = ui_box(character_viewport, 0, 0, sidebar_width - 16, 1, theme()->surface, 0);
    debug_set_id(list, "character_list");
    bool used[CONVERSATION_LIMIT] = {false};
    int y = 0;
    for (unsigned i = 0; i < conversation_count; ++i) {
        if (used[i]) continue;
        const char *name = conversations[i].character;
        lv_obj_t *card = ui_box(list, 0, y, sidebar_width - 16, 1, theme()->surface_inset, 9);
        lv_obj_t *header = ui_box(card, 8, 8, sidebar_width - 32, 24, theme()->surface_inset, 0);
        lv_obj_add_flag(header, LV_OBJ_FLAG_CLICKABLE);
        lv_obj_add_event_cb(header, toggle_character, LV_EVENT_CLICKED, NULL);
        lv_obj_t *avatar = ui_box(header, 0, 0, 24, 24, theme()->selection, 7);
        lv_obj_clear_flag(avatar, LV_OBJ_FLAG_CLICKABLE);
        char initial[5] = {0};
        unsigned char first = (unsigned char)*name;
        unsigned bytes = first < 0x80 ? 1 : first < 0xe0 ? 2 : first < 0xf0 ? 3 : 4;
        memcpy(initial, name, bytes);
        lv_obj_t *letter = label(avatar, initial, 0, 3, 24, theme()->text);
        lv_obj_set_style_text_align(letter, LV_TEXT_ALIGN_CENTER, 0);
        lv_obj_t *title = label(header, name, 32, 3, sidebar_width - 88, theme()->text);
        lv_label_set_long_mode(title, LV_LABEL_LONG_DOT);
        label(header, LV_SYMBOL_DOWN, sidebar_width - 50, 5, 18, theme()->text_secondary);
        lv_obj_t *items = ui_box(card, 8, 40, sidebar_width - 32, 1, theme()->surface_inset, 0);
        char id[40]; snprintf(id, sizeof(id), "character_card_%u", i); debug_set_id(card, id);
        snprintf(id, sizeof(id), "character_header_%u", i); debug_set_id(header, id);
        snprintf(id, sizeof(id), "conversation_list_%u", i); debug_set_id(items, id);
        int row_y = 0;
        for (unsigned j = i; j < conversation_count; ++j) {
            conversation_view_t *c = &conversations[j];
            if (strcmp(c->character, name)) continue;
            used[j] = true;
            lv_obj_t *row = ui_action(items, c->title, c->action, 0, row_y,
                sidebar_width - 32, 28, c->selected ? theme()->selection : theme()->surface_inset,
                c->selected ? theme()->text : theme()->text_secondary);
            lv_obj_t *caption = lv_obj_get_child(row, 0);
            lv_obj_set_align(caption, LV_ALIGN_TOP_LEFT);
            lv_obj_set_pos(caption, 28, 5); lv_obj_set_width(caption, sidebar_width - 68);
            lv_label_set_long_mode(caption, LV_LABEL_LONG_DOT);
            lv_obj_set_style_text_align(caption, LV_TEXT_ALIGN_LEFT, 0);
            label(row, LV_SYMBOL_DIRECTORY, 7, 7, 16, theme()->text_secondary);
            snprintf(id, sizeof(id), "conversation_%u", j); debug_set_id(row, id);
            row_y += 32;
        }
        lv_obj_set_height(items, row_y - 4);
        lv_obj_set_height(card, 40 + row_y + 4);
        y += 40 + row_y + 12;
    }
    if (!conversation_count) {
        lv_obj_t *empty = label(list, edge_ready ? "正在加载对话…" : "连接后显示对话", 8, 12,
                                 sidebar_width - 32, theme()->text_secondary);
        lv_label_set_long_mode(empty, LV_LABEL_LONG_WRAP); y = 64;
    }
    lv_obj_set_height(list, y);
    lv_obj_update_layout(character_viewport);
    lv_obj_scroll_to_y(character_viewport, scroll, LV_ANIM_OFF);
}

void operit_lvgl_set_chat_identity(const char *id, const char *character) {
    if (id && strcmp(active_chat_id, id)) {
        bool switching = active_chat_id[0] != 0;
        if (switching) {
            remember_chat_draft();
            unsigned slot = 0;
            while (slot < 4 && strcmp(drafts[slot].id, active_chat_id)) ++slot;
            if (slot == 4) slot = draft_cursor++ % 4;
            copy_utf8(drafts[slot].id, sizeof(drafts[slot].id), active_chat_id);
            copy_utf8(drafts[slot].text, sizeof(drafts[slot].text), chat_draft);
        }
        copy_utf8(active_chat_id, sizeof(active_chat_id), id);
        if (switching) {
            chat_send_pending = false;
            operit_lvgl_set_chat_draft("");
            for (unsigned i = 0; i < 4; ++i) if (!strcmp(drafts[i].id, id))
                operit_lvgl_set_chat_draft(drafts[i].text);
            if (drawer_layer) { sidebar_open = false; lv_obj_add_flag(drawer_layer, LV_OBJ_FLAG_HIDDEN); }
        }
        if (message_viewport) lv_obj_scroll_to_y(message_viewport, LV_COORD_MAX, LV_ANIM_OFF);
    }
    operit_lvgl_set_chat_preview(character);
}

static void clear_chat_cache(void) {
    active_chat_id[0] = 0;
    memset(drafts, 0, sizeof(drafts)); draft_cursor = 0;
    operit_lvgl_set_chat_draft("");
    chat_send_pending = false;
    for (unsigned i = 0; i < MESSAGE_LIMIT; ++i) { free(messages[i].text); messages[i].text = NULL; }
    message_count = conversation_count = 0;
}

void operit_lvgl_set_message(unsigned index, bool user, const char *text) {
    if (index >= MESSAGE_LIMIT) return;
    char bounded[MESSAGE_TEXT_BYTES]; copy_utf8(bounded, sizeof(bounded), text);
    message_view_t *m = &messages[index];
    if (!m->text || strcmp(m->text, bounded)) {
        char *next = malloc(strlen(bounded) + 1);
        if (!next) return;
        strcpy(next, bounded); free(m->text); m->text = next;
        messages_dirty = true;
    }
    if (m->user != user) messages_dirty = true;
    m->user = user;
}
void operit_lvgl_finish_messages(unsigned count) {
    unsigned next = count > MESSAGE_LIMIT ? MESSAGE_LIMIT : count;
    if (next != message_count) messages_dirty = true;
    message_count = next;
    for (unsigned i = message_count; i < MESSAGE_LIMIT; ++i) { free(messages[i].text); messages[i].text = NULL; }
    if (messages_dirty) render_messages();
}
void operit_lvgl_set_conversation(unsigned index, const char *id, const char *title,
                                 const char *character, bool selected) {
    if (index >= CONVERSATION_LIMIT) return;
    conversation_view_t next = {0};
    copy_utf8(next.id, sizeof(next.id), id);
    copy_utf8(next.title, sizeof(next.title), title && *title ? title : "未命名对话");
    copy_utf8(next.character, sizeof(next.character), character && *character ? character : "Operit");
    snprintf(next.action, sizeof(next.action), "edge_select:%s", next.id);
    next.selected = selected;
    if (memcmp(&next, &conversations[index], sizeof(next))) {
        conversations[index] = next; conversations_dirty = true;
    }
}
void operit_lvgl_finish_conversations(unsigned count) {
    unsigned next = count > CONVERSATION_LIMIT ? CONVERSATION_LIMIT : count;
    if (next != conversation_count) conversations_dirty = true;
    conversation_count = next;
    if (conversations_dirty) { render_conversations(); conversations_dirty = false; }
}

static void close_dialog(void) {
    if (dialog_layer) { lv_obj_delete(dialog_layer); dialog_layer = NULL; }
}
static void show_dialog(const char *text, bool confirm) {
    if (!overlay_host) return;
    close_dialog();
    dialog_layer = ui_box(overlay_host, 0, 0, OPERIT_SCREEN_WIDTH, OPERIT_SCREEN_HEIGHT, theme()->scrim, 0);
    debug_set_id(dialog_layer, "dialog_layer");
    lv_obj_set_style_bg_opa(dialog_layer, LV_OPA_70, 0);
    lv_obj_t *card = ui_box(dialog_layer, 20, 42, 280, 164, theme()->surface, 12);
    lv_obj_t *body = label(card, text, 12, 12, 256, theme()->text);
    lv_label_set_long_mode(body, LV_LABEL_LONG_WRAP);
    if (confirm) ui_action(card, "取消", "dialog_close", 12, 120, 122, 32, theme()->selection, theme()->text);
    ui_action(card, confirm ? "确认取消配对" : "知道了", confirm ? "dialog_confirm" : "dialog_close",
              confirm ? 142 : 12, 120, confirm ? 126 : 256, 32, theme()->accent, theme()->on_accent);
}
void operit_lvgl_action_error(const char *error) {
    unpair_requested = false;
    show_dialog(error && *error ? error : "操作失败，请重试", false);
}
