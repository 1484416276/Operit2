/* One decoder-free RGB565 preview. No image data is kept in message history. */
enum { IMAGE_MAX_WIDTH = 128, IMAGE_MAX_HEIGHT = 96, IMAGE_CHUNK_BYTES = 1024 };
static lv_obj_t *image_layer, *image_status, *image_object;
static lv_image_dsc_t image_descriptor;
static uint8_t *image_pixels;
static size_t image_received, image_total;
static unsigned image_request;

static void close_image(void) {
    if (!image_layer) return;
    if (action_cb) action_cb("edge_image_cancel", context);
    lv_obj_delete(image_layer);
    image_layer = image_status = image_object = NULL;
    lv_image_cache_drop(&image_descriptor);
    free(image_pixels); image_pixels = NULL;
    image_received = image_total = 0;
    ++image_request;
}

static void show_image(const char *id) {
    close_image();
    hide_chat_keyboard();
    ++image_request;
    image_layer = ui_box(overlay_host, 0, 0, OPERIT_SCREEN_WIDTH, OPERIT_SCREEN_HEIGHT, theme()->background, 0);
    debug_set_id(image_layer, "image_preview");
    ui_action(image_layer, "返回", "image_close", 8, 6, 60, 28, theme()->surface, theme()->text);
    label(image_layer, "图片预览", 82, 11, 220, theme()->text);
    image_status = label(image_layer, "正在加载图片…", 16, 112, 288, theme()->text_secondary);
    debug_set_id(image_status, "image_status");
    lv_label_set_long_mode(image_status, LV_LABEL_LONG_WRAP);
    char action[128]; snprintf(action, sizeof(action), "edge_image:%u:%s", image_request, id);
    if (action_cb) action_cb(action, context);
}

unsigned operit_lvgl_image_request(void) { return image_request; }
void operit_lvgl_image_error(unsigned request, const char *error) {
    if (!image_layer || request != image_request) return;
    if (image_object) { lv_obj_delete(image_object); image_object = NULL; }
    lv_image_cache_drop(&image_descriptor);
    free(image_pixels); image_pixels = NULL; image_total = image_received = 0;
    lv_obj_clear_flag(image_status, LV_OBJ_FLAG_HIDDEN);
    lv_label_set_text(image_status, error && *error ? error : "图片加载失败，请返回后重试");
}
bool operit_lvgl_image_chunk(unsigned request, unsigned width, unsigned height, unsigned offset,
                             const uint8_t *bytes, unsigned length) {
    if (!image_layer || request != image_request) return false;
    bool valid = width && height && width <= IMAGE_MAX_WIDTH && height <= IMAGE_MAX_HEIGHT &&
        bytes && length && length <= IMAGE_CHUNK_BYTES && length % 2 == 0 && offset == image_received;
    size_t total = valid ? width * height * 2 : 0;
    bool dimensions_match = !image_pixels ||
        (image_descriptor.header.w == width && image_descriptor.header.h == height);
    if (!valid || offset > total || length > total - offset || !dimensions_match) {
        operit_lvgl_image_error(request, "图片传输中断，请返回后重试"); return false;
    }
    if (!image_pixels) {
        image_pixels = malloc(total);
        if (!image_pixels) { operit_lvgl_image_error(request, "内存不足，无法打开图片"); return false; }
        image_descriptor = (lv_image_dsc_t){
            .header = {.magic = LV_IMAGE_HEADER_MAGIC, .cf = LV_COLOR_FORMAT_RGB565,
                       .w = width, .h = height, .stride = width * 2},
            .data_size = total, .data = image_pixels,
        };
        image_total = total;
    }
    memcpy(image_pixels + offset, bytes, length); image_received += length;
    lv_label_set_text_fmt(image_status, "正在加载图片 %u%%", (unsigned)(image_received * 100 / image_total));
    if (image_received == image_total) {
        image_object = lv_image_create(image_layer);
        lv_image_set_src(image_object, &image_descriptor);
        lv_obj_set_size(image_object, width, height);
        lv_obj_align(image_object, LV_ALIGN_CENTER, 0, 16);
        debug_register(image_object, "image", "image_pixels", "");
        lv_obj_add_flag(image_status, LV_OBJ_FLAG_HIDDEN);
    }
    return true;
}
