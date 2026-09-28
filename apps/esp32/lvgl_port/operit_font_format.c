/* Unicode formatting controls have no visible glyph. LVGL renders individual
 * emoji glyphs; variation selectors and joiners must not become missing boxes. */
#include "lvgl.h"
#include <string.h>
LV_FONT_DECLARE(operit_font_emoji_16);

static bool format_glyph(const lv_font_t *font, lv_font_glyph_dsc_t *glyph,
                         uint32_t codepoint, uint32_t next) {
    (void)font; (void)next;
    if (codepoint != 0xfe0e && codepoint != 0xfe0f && codepoint != 0x200d) return false;
    memset(glyph, 0, sizeof(*glyph));
    return true;
}
const lv_font_t operit_font_format = {
    .get_glyph_dsc = format_glyph,
    .line_height = 19,
    .base_line = 4,
    .fallback = &operit_font_emoji_16,
};
