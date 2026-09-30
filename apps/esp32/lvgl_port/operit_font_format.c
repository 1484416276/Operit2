/* Keep text rendering total: unsupported glyphs use a visible, bounded '?' fallback
 * instead of an empty descriptor that later becomes a box or corrupt bitmap. */
#include "lvgl.h"
#include <stdbool.h>
#include <stdint.h>

LV_FONT_DECLARE(operit_font_punctuation_14);
LV_FONT_DECLARE(operit_font_google_color);
LV_FONT_DECLARE(operit_font_emoji_16);
LV_FONT_DECLARE(lv_font_montserrat_14);

static bool is_format_control(uint32_t codepoint) {
    return codepoint == 0xFE0E || codepoint == 0xFE0F || codepoint == 0x200D;
}

bool operit_font_text_glyph(const lv_font_t *font, lv_font_glyph_dsc_t *glyph,
                            uint32_t codepoint, uint32_t next) {
    (void)font;
    if (is_format_control(codepoint)) {
        *glyph = (lv_font_glyph_dsc_t){
            .resolved_font = NULL,
            .adv_w = 0,
            .box_w = 0,
            .box_h = 0,
            .ofs_x = 0,
            .ofs_y = 0,
            .bpp = 1,
            .is_placeholder = 0,
        };
        return true;
    }

    /* Let the linked fallback chain resolve punctuation, emoji, Latin and CJK. */
    if (codepoint != 0 && codepoint != 0xFFFD) return false;

    /* Replacement characters are common in malformed UTF-8 input. Render a
     * visible fallback rather than allowing a zero sized or placeholder glyph. */
    if (lv_font_montserrat_14.get_glyph_dsc(&lv_font_montserrat_14, glyph, '?', next)) {
        glyph->is_placeholder = 0;
        return true;
    }
    return false;
}

static const uint8_t *format_bitmap(const lv_font_t *font, uint32_t codepoint) {
    (void)font;
    if (is_format_control(codepoint)) return NULL;
    if (codepoint == 0 || codepoint == 0xFFFD) {
        return lv_font_montserrat_14.get_glyph_bitmap(&lv_font_montserrat_14, '?');
    }
    return NULL;
}

const lv_font_t operit_font_format = {
    .get_glyph_dsc = operit_font_text_glyph,
    .get_glyph_bitmap = format_bitmap,
    .line_height = 19,
    .base_line = 4,
    .fallback = &operit_font_punctuation_14,
};
