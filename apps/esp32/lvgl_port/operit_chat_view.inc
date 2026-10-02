/* Bounded display cache only. Core owns conversations and message history.
 * This view is compiled unchanged for the device and the browser. */
enum { MESSAGE_LIMIT = 12, CONVERSATION_LIMIT = 24, MESSAGE_TEXT_BYTES = 2048 };
typedef struct {
    char *text;
    bool user;
    lv_obj_t *row, *bubble, *speaker, *label;
    struct { char id[81]; lv_obj_t *button; } images[4];
} message_view_t;
typedef struct {
    char id[80], title[128], character[96], action[96];
    bool selected;
} conversation_view_t;
static message_view_t messages[MESSAGE_LIMIT];
static conversation_view_t conversations[CONVERSATION_LIMIT];
static unsigned message_count, conversation_count;
static unsigned message_page;
static bool message_follow_latest = true;
static bool messages_dirty;
static bool conversations_dirty;
static char active_chat_id[80];
static struct { char id[80], text[512]; } drafts[4];
static unsigned draft_cursor;
/* Markdown and input bounding share one heap buffer instead of consuming the
 * LVGL task stack on every message update. */
static char *message_scratch;
static char visible_image_ids[4][81];

static char *message_scratch_buffer(void) {
    if (!message_scratch) message_scratch = malloc(MESSAGE_TEXT_BYTES);
    return message_scratch;
}

/* Render a small Markdown subset without storing a second copy per message.
 * Core keeps the original Markdown; this pass only removes presentation
 * markers that cannot be represented by a compact LVGL label. */
/* Append whole UTF-8 characters, always reserving the final NUL byte. */
static void markdown_copy(const char *p, const char *end, char *output, size_t *out, size_t capacity) {
    while (p < end) {
        unsigned char lead = (unsigned char)*p;
        size_t bytes = lead < 0x80 ? 1 : lead < 0xe0 ? 2 : lead < 0xf0 ? 3 : 4;
        if ((size_t)(end - p) < bytes || *out + bytes >= capacity) break;
        memcpy(output + *out, p, bytes); *out += bytes; p += bytes;
    }
}

static void markdown_inline(const char *p, const char *end, char *output, size_t *out,
                            size_t capacity, unsigned depth) {
    const char *start = p;
    while (p < end && *out + 1 < capacity) {
        if (*p == '\\' && p + 1 < end && strchr("\\`*_{}[]()#+-.!>~", p[1])) {
            markdown_copy(p + 1, p + 2, output, out, capacity); p += 2; continue;
        }
        if (*p == '[' && depth < 4) {
            const char *close = memchr(p + 1, ']', end - p - 1);
            if (close && close + 1 < end && close[1] == '(') {
                const char *url_end = memchr(close + 2, ')', end - close - 2);
                if (url_end) {
                    markdown_inline(p + 1, close, output, out, capacity, depth + 1);
                    p = url_end + 1; continue;
                }
            }
        }
        if (strchr("*_~`", *p) && depth < 4) {
            size_t count = 1;
            while (p + count < end && p[count] == *p) ++count;
            bool boundary = p == start || p[-1] == ' ' || p[-1] == '(' || (unsigned char)p[-1] >= 0x80;
            bool eligible = *p == '`' || ((*p != '_' || boundary) && (*p != '~' || count == 2));
            const char *close = p + count;
            if (eligible && close < end && *close != ' ') {
                while (close + count <= end) {
                    if (close > p + count && !memcmp(p, close, count) && close[-1] != ' ' &&
                        (close + count == end || close[count] != *p)) break;
                    ++close;
                }
                if (close + count <= end) {
                    if (*p == '`') markdown_copy(p + count, close, output, out, capacity);
                    else markdown_inline(p + count, close, output, out, capacity, depth + 1);
                    p = close + count; continue;
                }
            }
            markdown_copy(p, p + count, output, out, capacity); p += count; continue;
        }
        unsigned char lead = (unsigned char)*p;
        size_t bytes = lead < 0x80 ? 1 : lead < 0xe0 ? 2 : lead < 0xf0 ? 3 : 4;
        if ((size_t)(end - p) < bytes || *out + bytes >= capacity) break;
        markdown_copy(p, p + bytes, output, out, capacity); p += bytes;
    }
}

static void markdown_plain(const char *source, char *output, size_t capacity) {
    if (!capacity) return;
    size_t out = 0, fence_length = 0;
    char fence = 0;
    const char *p = source ? source : "";
    while (*p && out + 1 < capacity) {
        const char *end = strchr(p, '\n');
        if (!end) end = p + strlen(p);
        size_t marker_length = 0;
        if (*p == '`' || *p == '~') while (p + marker_length < end && p[marker_length] == *p) ++marker_length;
        if (marker_length >= 3 && (!fence || (*p == fence && marker_length >= fence_length))) {
            if (fence) fence = 0;
            else { fence = *p; fence_length = marker_length; }
            p = *end ? end + 1 : end; continue;
        }
        if (fence) markdown_copy(p, end, output, &out, capacity);
        else {
            unsigned hashes = 0;
            while (p + hashes < end && p[hashes] == '#' && hashes < 6) ++hashes;
            if (hashes && p + hashes < end && p[hashes] == ' ') p += hashes + 1;
            else if (end - p >= 2 && strchr("-*+", p[0]) && p[1] == ' ') {
                if (out + 3 >= capacity) break;
                memcpy(output + out, "\xc2\xb7 ", 3); out += 3; p += 2;
            } else if (end - p >= 2 && p[0] == '>' && p[1] == ' ') {
                if (out + 2 >= capacity) break;
                memcpy(output + out, "| ", 2); out += 2; p += 2;
            }
            markdown_inline(p, end, output, &out, capacity, 0);
        }
        if (*end && out + 1 < capacity) output[out++] = '\n';
        p = *end ? end + 1 : end;
    }
    output[out] = 0;
}

static void reset_message_nodes(void) {
    for (unsigned i = 0; i < MESSAGE_LIMIT; ++i) {
        messages[i].row = messages[i].bubble = messages[i].speaker = messages[i].label = NULL;
        for (unsigned j = 0; j < 4; ++j) messages[i].images[j].button = NULL;
    }
}

static void page_message(int direction);

static void message_page_clicked(lv_event_t *event) {
    page_message((int)(intptr_t)lv_event_get_user_data(event));
}

static void image_clicked(lv_event_t *event) {
    const char *id = lv_event_get_user_data(event);
    char action[96]; snprintf(action, sizeof(action), "image_open:%s", id); queue_route(action);
}

static void render_messages(void) {
    if (!message_list) return;
    messages_dirty = false;
    char page_text[40];
    snprintf(page_text, sizeof(page_text), "%u / %u", message_count ? message_page + 1 : 0, message_count);
    if (message_page_label) lv_label_set_text(message_page_label, page_text);
    if (message_prev_button) {
        if (message_count && message_page > 0) lv_obj_clear_state(message_prev_button, LV_STATE_DISABLED);
        else lv_obj_add_state(message_prev_button, LV_STATE_DISABLED);
    }
    if (message_next_button) {
        if (message_count && message_page + 1 < message_count) lv_obj_clear_state(message_next_button, LV_STATE_DISABLED);
        else lv_obj_add_state(message_next_button, LV_STATE_DISABLED);
    }
    int y = 0;
    if (!chat_label) {
        chat_label = label(message_list, "", 4, 0, OPERIT_MESSAGE_WIDTH - 12, theme()->text_secondary);
        debug_set_id(chat_label, "chat_notice");
        lv_label_set_long_mode(chat_label, LV_LABEL_LONG_WRAP);
    }
    if (message_count) {
        message_view_t *m = &messages[message_page];
        message_view_t *view = &messages[0];
        if (!view->row) {
            view->row = ui_box(message_list, 0, y, OPERIT_MESSAGE_WIDTH, 1, theme()->background, 0);
            view->bubble = ui_box(view->row, 0, 0, OPERIT_MESSAGE_WIDTH - 12, 1, theme()->background, 8);
            view->speaker = label(view->row, chat_preview, 4, 0, OPERIT_MESSAGE_WIDTH - 12, theme()->text_secondary);
            view->label = label(view->bubble, "", 4, 0, OPERIT_MESSAGE_WIDTH - 28, theme()->text);
            lv_label_set_long_mode(view->label, LV_LABEL_LONG_WRAP);
            lv_obj_set_style_text_line_space(view->label, 3, 0);
            debug_set_id(view->row, "message_0");
            debug_set_id(view->label, "message_text_0");
        }
        lv_obj_clear_flag(view->row, LV_OBJ_FLAG_HIDDEN);
        char *scratch = message_scratch_buffer();
        const char *rendered = m->text ? m->text : "";
        if (scratch) {
            markdown_plain(rendered, scratch, MESSAGE_TEXT_BYTES);
            rendered = scratch;
        }
        if (strcmp(lv_label_get_text(view->label), rendered))
            lv_label_set_text(view->label, rendered);
        lv_point_t measured;
        lv_text_get_size(&measured, rendered, &operit_font_zh_14,
                         0, 3, 228, LV_TEXT_FLAG_NONE);
        int width = m->user ? LV_MIN(measured.x + 16, 244) : OPERIT_MESSAGE_WIDTH - 12;
        if (m->user) lv_obj_add_flag(view->speaker, LV_OBJ_FLAG_HIDDEN);
        else { lv_obj_clear_flag(view->speaker, LV_OBJ_FLAG_HIDDEN); lv_label_set_text(view->speaker, chat_preview); }
        lv_obj_set_width(view->label, width - 16);
        lv_obj_set_pos(view->label, 8, m->user ? 8 : 2);
        lv_obj_update_layout(view->label);
        int h = lv_obj_get_height(view->label) + (m->user ? 16 : 4);
        for (unsigned j = 0; j < 4; ++j) {
            if (!m->images[j].id[0]) {
                if (view->images[j].button) lv_obj_add_flag(view->images[j].button, LV_OBJ_FLAG_HIDDEN);
                continue;
            }
            copy_utf8(visible_image_ids[j], sizeof(visible_image_ids[j]), m->images[j].id);
            if (!view->images[j].button) {
                view->images[j].button = ui_box(view->bubble, 8, h, 200, 28, theme()->surface, 6);
                lv_obj_add_flag(view->images[j].button, LV_OBJ_FLAG_CLICKABLE);
                lv_obj_add_event_cb(view->images[j].button, image_clicked, LV_EVENT_CLICKED, visible_image_ids[j]);
                char caption[48]; snprintf(caption, sizeof(caption), "查看图片 %u", j+1);
                label(view->images[j].button, caption, 8, 4, 180, theme()->text);
                char id[40]; snprintf(id, sizeof(id), "message_image_0_%u", j);
                debug_register(view->images[j].button, "button", id, "image_open");
            }
            lv_obj_clear_flag(view->images[j].button, LV_OBJ_FLAG_HIDDEN);
            lv_obj_set_y(view->images[j].button, h); h += 32;
            width = LV_MAX(width, 216);
        }
        lv_obj_set_pos(view->bubble, m->user ? OPERIT_MESSAGE_WIDTH - width : 0, m->user ? 0 : 22);
        lv_obj_set_size(view->bubble, width, h);
        lv_obj_set_style_bg_color(view->bubble, lv_color_hex(m->user ? theme()->selection : theme()->background), 0);
        h += m->user ? 0 : 22;
        lv_obj_set_pos(view->row, 0, y);
        lv_obj_set_height(view->row, h);
        y += h + 8;
    } else if (messages[0].row) {
        lv_obj_add_flag(messages[0].row, LV_OBJ_FLAG_HIDDEN);
    }
    lv_label_set_text(chat_label, chat_screen);
    lv_obj_set_y(chat_label, y);
    if (chat_screen[0]) {
        lv_obj_clear_flag(chat_label, LV_OBJ_FLAG_HIDDEN);
        lv_obj_update_layout(chat_label);
        y += lv_obj_get_height(chat_label) + 8;
    } else lv_obj_add_flag(chat_label, LV_OBJ_FLAG_HIDDEN);
    lv_obj_set_height(message_list, y > 0 ? y : 1);
    int max_scroll = y - lv_obj_get_height(message_viewport);
    if (max_scroll < 0) max_scroll = 0;
    if (message_scroll_y > max_scroll) message_scroll_y = max_scroll;
    lv_obj_set_y(message_list, -message_scroll_y);
}

static void page_message(int direction) {
    if (!message_count || !message_viewport) return;
    if (direction < 0 && message_page > 0) --message_page;
    else if (direction > 0 && message_page + 1 < message_count) ++message_page;
    else return;
    message_follow_latest = message_page + 1 == message_count;
    message_scroll_y = 0;
    render_messages();
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
        close_image();
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
        message_page = 0;
        message_scroll_y = 0;
        message_follow_latest = true;
        if (switching) {
            chat_send_pending = false;
            operit_lvgl_set_chat_draft("");
            for (unsigned i = 0; i < 4; ++i) if (!strcmp(drafts[i].id, id))
                operit_lvgl_set_chat_draft(drafts[i].text);
            if (drawer_layer) { sidebar_open = false; lv_obj_add_flag(drawer_layer, LV_OBJ_FLAG_HIDDEN); }
        }
    }
    operit_lvgl_set_chat_preview(character);
}

static void clear_chat_cache(void) {
    active_chat_id[0] = 0;
    memset(drafts, 0, sizeof(drafts)); draft_cursor = 0;
    operit_lvgl_set_chat_draft("");
    chat_send_pending = false;
    message_page = 0;
    message_scroll_y = 0;
    message_follow_latest = true;
    for (unsigned i = 0; i < MESSAGE_LIMIT; ++i) { free(messages[i].text); messages[i].text = NULL; }
    message_count = conversation_count = 0;
}

void operit_lvgl_set_message(unsigned index, bool user, const char *text) {
    if (index >= MESSAGE_LIMIT) return;
    const char *source = text ? text : "";
    char *scratch = message_scratch_buffer();
    if (!scratch) return;
    copy_utf8(scratch, MESSAGE_TEXT_BYTES, source);
    size_t bounded_length = strlen(scratch);
    message_view_t *m = &messages[index];
    if (!m->text || strcmp(m->text, scratch)) {
        char *next = malloc(bounded_length + 1);
        if (!next) return;
        memcpy(next, scratch, bounded_length + 1);
        free(m->text); m->text = next;
        messages_dirty = true;
    }
    if (m->user != user) messages_dirty = true;
    m->user = user;
}
void operit_lvgl_set_message_image(unsigned index, unsigned image, const char *id) {
    if (index >= MESSAGE_LIMIT || image >= 4) return;
    char *current = messages[index].images[image].id;
    if (strcmp(current, id ? id : "")) { copy_utf8(current, 81, id); messages_dirty = true; }
}
void operit_lvgl_finish_messages(unsigned count) {
    unsigned next = count > MESSAGE_LIMIT ? MESSAGE_LIMIT : count;
    if (next != message_count) messages_dirty = true;
    message_count = next;
    if (!message_count) message_page = 0;
    else if (message_follow_latest) message_page = message_count - 1;
    else if (message_page >= message_count) message_page = message_count - 1;
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
