#include "operit_emoji.h"
#include "operit_emoji_config.h"
#include "lvgl.h"
#include <string.h>
#ifdef __EMSCRIPTEN__
#include <emscripten.h>
EM_JS(int, load_style, (void), {
    try { return Number(globalThis.localStorage?.getItem('operit.esp32.emojiStyle') ?? 0); }
    catch (_) { return 0; }
});
EM_JS(int, save_style, (unsigned style), {
    try { globalThis.localStorage?.setItem('operit.esp32.emojiStyle', String(style)); return 1; }
    catch (_) { return 0; }
});
#else
#include "nvs.h"
static int load_style(void) {
    nvs_handle_t handle;
    uint8_t value = OPERIT_EMOJI_MONO;
    if (nvs_open("operit_ui", NVS_READONLY, &handle) == ESP_OK) {
        nvs_get_u8(handle, "emoji_style", &value);
        nvs_close(handle);
    }
    return value;
}
static int save_style(unsigned style) {
    nvs_handle_t handle;
    if (nvs_open("operit_ui", NVS_READWRITE, &handle) != ESP_OK) return 0;
    esp_err_t result = nvs_set_u8(handle, "emoji_style", (uint8_t)style);
    if (result == ESP_OK) result = nvs_commit(handle);
    nvs_close(handle);
    return result == ESP_OK;
}
#endif

#if OPERIT_EMOJI_COLOR_ENABLED
#include "operit_emoji_color.inc"
#endif
LV_FONT_DECLARE(operit_font_emoji_16);
static unsigned selected_style;

void operit_emoji_init(void) {
    unsigned saved = (unsigned)load_style();
    selected_style = saved < OPERIT_EMOJI_STYLE_COUNT && (saved != OPERIT_EMOJI_GOOGLE || OPERIT_EMOJI_COLOR_ENABLED)
        ? saved : OPERIT_EMOJI_MONO;
}
unsigned operit_emoji_style(void) { return selected_style; }
bool operit_emoji_color_available(void) { return OPERIT_EMOJI_COLOR_ENABLED; }
bool operit_emoji_set_style(unsigned style) {
    if (style >= OPERIT_EMOJI_STYLE_COUNT || (style == OPERIT_EMOJI_GOOGLE && !OPERIT_EMOJI_COLOR_ENABLED)) return false;
    if (style == selected_style) return true;
    if (!save_style(style)) return false;
    selected_style = style;
    return true;
}

static bool color_glyph(const lv_font_t *font, lv_font_glyph_dsc_t *glyph,
                        uint32_t codepoint, uint32_t next) {
    (void)font; (void)next;
#if OPERIT_EMOJI_COLOR_ENABLED
    if (selected_style != OPERIT_EMOJI_GOOGLE) return false;
    size_t low = 0, high = sizeof(color_glyphs) / sizeof(color_glyphs[0]);
    while (low < high) {
        size_t mid = low + (high - low) / 2;
        if (color_glyphs[mid].codepoint < codepoint) low = mid + 1;
        else if (color_glyphs[mid].codepoint > codepoint) high = mid;
        else {
            const lv_image_dsc_t *image = color_glyphs[mid].image;
            memset(glyph, 0, sizeof(*glyph));
            glyph->adv_w = image->header.w + 1;
            glyph->box_w = image->header.w;
            glyph->box_h = image->header.h;
            glyph->ofs_y = -2;
            glyph->format = LV_FONT_GLYPH_FORMAT_IMAGE;
            glyph->gid.src = image;
            return true;
        }
    }
#else
    (void)glyph; (void)codepoint;
#endif
    return false;
}
static const void *color_bitmap(lv_font_glyph_dsc_t *glyph, lv_draw_buf_t *buffer) {
    (void)buffer;
    return glyph->gid.src;
}
const lv_font_t operit_font_google_color = {
    .get_glyph_dsc = color_glyph,
    .get_glyph_bitmap = color_bitmap,
    .line_height = 19,
    .base_line = 4,
    .fallback = &operit_font_emoji_16,
};
