/* Noto Sans CJK SC Regular; supplementary punctuation.
 * Copyright 2014-2021 Adobe (http://www.adobe.com/). All rights reserved.
 * SIL Open Font License 1.1; see OFL.txt.
 */
/*******************************************************************************
 * Size: 14 px
 * Bpp: 2
 * Opts: --font tmp/cjk/NotoSansCJKsc-Regular.otf -r 0x2013-0x2014,0x2018-0x201f,0x2026,0x3001-0x3002,0x3008-0x3009,0x300c-0x3011,0x3014-0x3015,0xff01,0xff0c,0xff0e,0xff1a,0xff1b,0xff1f --size 14 --bpp 2 --format lvgl --lv-include lvgl.h --lv-font-name operit_font_punctuation_14 --no-kerning --no-compress --lv-fallback operit_font_google_color --output apps/esp32/lvgl_port/operit_font_punctuation_14.c
 ******************************************************************************/

#ifdef LV_LVGL_H_INCLUDE_SIMPLE
#include "lvgl.h"
#else
#include "lvgl.h"
#endif

#ifndef OPERIT_FONT_PUNCTUATION_14
#define OPERIT_FONT_PUNCTUATION_14 1
#endif

#if OPERIT_FONT_PUNCTUATION_14

/*-----------------
 *    BITMAPS
 *----------------*/

/*Store the image of the glyphs*/
static LV_ATTRIBUTE_LARGE_CONST const uint8_t glyph_bitmap[] = {
    /* U+2013 "–" */
    0x7f, 0xfc,

    /* U+2014 "—" */
    0x7f, 0xff, 0xff,

    /* U+2018 "‘" */
    0x0, 0x85, 0x2c, 0x70,

    /* U+2019 "’" */
    0x24, 0xe1, 0x48, 0x0,

    /* U+201A "‚" */
    0x14, 0x2c, 0xc, 0x24, 0x10,

    /* U+201C "“" */
    0x0, 0x1, 0x45, 0x21, 0x83, 0x6c, 0x38, 0xd0,

    /* U+201D "”" */
    0x21, 0x87, 0xac, 0x14, 0x82, 0x14, 0x0, 0x0,

    /* U+201E "„" */
    0x14, 0x82, 0xce, 0xc, 0x22, 0x48, 0x10, 0x0,

    /* U+2026 "…" */
    0x70, 0x28, 0xd, 0x70, 0x28, 0xd,

    /* U+3001 "、" */
    0x0, 0xd, 0x1, 0xd0, 0x1d, 0x1, 0x0,

    /* U+3002 "。" */
    0x19, 0x18, 0x98, 0x9, 0x46, 0x2e, 0x0,

    /* U+3008 "〈" */
    0x0, 0x80, 0x18, 0x3, 0x0, 0x90, 0x18, 0x3,
    0x0, 0xa0, 0x7, 0x0, 0x28, 0x0, 0xc0, 0x7,
    0x0, 0x24, 0x0, 0xc0, 0x0,

    /* U+3009 "〉" */
    0x20, 0x2, 0x40, 0xc, 0x0, 0x60, 0x3, 0x40,
    0xc, 0x0, 0xa0, 0xd, 0x2, 0x80, 0x30, 0xd,
    0x1, 0x80, 0x30, 0x0, 0x0,

    /* U+300C "「" */
    0xff, 0x70, 0xc, 0x3, 0x0, 0xc0, 0x30, 0xc,
    0x3, 0x0, 0xc0, 0x0, 0x0,

    /* U+300D "」" */
    0x0, 0x0, 0x30, 0xc, 0x3, 0x0, 0xc0, 0x30,
    0xc, 0x3, 0x0, 0xdf, 0xf0,

    /* U+300E "『" */
    0x95, 0x54, 0x1, 0x49, 0x44, 0x40, 0x44, 0x4,
    0x40, 0x44, 0x4, 0x40, 0x44, 0x5, 0x40,

    /* U+300F "』" */
    0x2, 0x60, 0x11, 0x1, 0x10, 0x11, 0x1, 0x10,
    0x11, 0x1, 0x15, 0x51, 0x55, 0x60,

    /* U+3010 "【" */
    0x0, 0x1f, 0xd7, 0xd1, 0xf0, 0x74, 0x1c, 0x7,
    0x1, 0xc0, 0x70, 0x1c, 0x7, 0x41, 0xf0, 0x7d,
    0x1f, 0xd0,

    /* U+3011 "】" */
    0x7f, 0x47, 0xd0, 0xf4, 0x1d, 0x3, 0x40, 0xd0,
    0x34, 0xd, 0x3, 0x41, 0xd0, 0xf4, 0x7d, 0x7f,
    0x40, 0x0,

    /* U+3014 "〔" */
    0x0, 0x0, 0x81, 0xd0, 0xc0, 0x30, 0xc, 0x3,
    0x0, 0xc0, 0x30, 0xc, 0x3, 0x0, 0xc0, 0x28,
    0x1, 0xc0, 0x0,

    /* U+3015 "〕" */
    0x0, 0x8, 0x1, 0xd0, 0xc, 0x3, 0x0, 0xc0,
    0x30, 0xc, 0x3, 0x0, 0xc0, 0x30, 0xc, 0xa,
    0xd, 0x0, 0x0,

    /* U+FF01 "！" */
    0x1, 0xc3, 0xc, 0x30, 0xc3, 0xc, 0x0, 0x83,
    0x0,

    /* U+FF0C "，" */
    0x0, 0x2c, 0x1d, 0x9, 0x28, 0x10,

    /* U+FF0E "．" */
    0x2, 0xd7, 0x0,

    /* U+FF1A "：" */
    0x72, 0xc0, 0x0, 0x0, 0x0, 0x1c, 0xb0, 0x0,

    /* U+FF1B "；" */
    0x1c, 0x2c, 0x0, 0x0, 0x0, 0x0, 0x18, 0x2d,
    0x9, 0xc, 0x34, 0x0,

    /* U+FF1F "？" */
    0xb, 0x90, 0xd6, 0xc0, 0x3, 0x40, 0xc, 0x0,
    0xe0, 0xa, 0x0, 0x30, 0x0, 0x40, 0x0, 0x0,
    0x8, 0x0, 0x70, 0x0, 0x0
};


/*---------------------
 *  GLYPH DESCRIPTION
 *--------------------*/

static const lv_font_fmt_txt_glyph_dsc_t glyph_dsc[] = {
    {.bitmap_index = 0, .adv_w = 0, .box_w = 0, .box_h = 0, .ofs_x = 0, .ofs_y = 0} /* id = 0 reserved */,
    {.bitmap_index = 0, .adv_w = 120, .box_w = 7, .box_h = 1, .ofs_x = 0, .ofs_y = 4},
    {.bitmap_index = 2, .adv_w = 200, .box_w = 12, .box_h = 1, .ofs_x = 0, .ofs_y = 4},
    {.bitmap_index = 5, .adv_w = 224, .box_w = 3, .box_h = 5, .ofs_x = 11, .ofs_y = 8},
    {.bitmap_index = 9, .adv_w = 224, .box_w = 3, .box_h = 5, .ofs_x = 0, .ofs_y = 7},
    {.bitmap_index = 13, .adv_w = 62, .box_w = 4, .box_h = 5, .ofs_x = 0, .ofs_y = -3},
    {.bitmap_index = 18, .adv_w = 224, .box_w = 6, .box_h = 5, .ofs_x = 8, .ofs_y = 8},
    {.bitmap_index = 26, .adv_w = 224, .box_w = 6, .box_h = 5, .ofs_x = 0, .ofs_y = 7},
    {.bitmap_index = 34, .adv_w = 106, .box_w = 6, .box_h = 5, .ofs_x = 0, .ofs_y = -3},
    {.bitmap_index = 42, .adv_w = 224, .box_w = 12, .box_h = 2, .ofs_x = 1, .ofs_y = 5},
    {.bitmap_index = 48, .adv_w = 224, .box_w = 5, .box_h = 5, .ofs_x = 0, .ofs_y = -1},
    {.bitmap_index = 55, .adv_w = 224, .box_w = 5, .box_h = 5, .ofs_x = 0, .ofs_y = -1},
    {.bitmap_index = 62, .adv_w = 224, .box_w = 6, .box_h = 14, .ofs_x = 8, .ofs_y = -2},
    {.bitmap_index = 83, .adv_w = 224, .box_w = 6, .box_h = 14, .ofs_x = 0, .ofs_y = -2},
    {.bitmap_index = 104, .adv_w = 224, .box_w = 5, .box_h = 10, .ofs_x = 9, .ofs_y = 2},
    {.bitmap_index = 117, .adv_w = 224, .box_w = 5, .box_h = 10, .ofs_x = 0, .ofs_y = -1},
    {.bitmap_index = 130, .adv_w = 224, .box_w = 6, .box_h = 10, .ofs_x = 8, .ofs_y = 2},
    {.bitmap_index = 145, .adv_w = 224, .box_w = 6, .box_h = 9, .ofs_x = 0, .ofs_y = -1},
    {.bitmap_index = 159, .adv_w = 224, .box_w = 5, .box_h = 14, .ofs_x = 9, .ofs_y = -1},
    {.bitmap_index = 177, .adv_w = 224, .box_w = 5, .box_h = 14, .ofs_x = 0, .ofs_y = -2},
    {.bitmap_index = 195, .adv_w = 224, .box_w = 5, .box_h = 15, .ofs_x = 9, .ofs_y = -2},
    {.bitmap_index = 214, .adv_w = 224, .box_w = 5, .box_h = 15, .ofs_x = 0, .ofs_y = -2},
    {.bitmap_index = 233, .adv_w = 224, .box_w = 3, .box_h = 12, .ofs_x = 2, .ofs_y = -1},
    {.bitmap_index = 242, .adv_w = 224, .box_w = 4, .box_h = 6, .ofs_x = 1, .ofs_y = -2},
    {.bitmap_index = 248, .adv_w = 224, .box_w = 3, .box_h = 3, .ofs_x = 2, .ofs_y = 0},
    {.bitmap_index = 251, .adv_w = 224, .box_w = 3, .box_h = 10, .ofs_x = 2, .ofs_y = -1},
    {.bitmap_index = 259, .adv_w = 224, .box_w = 4, .box_h = 12, .ofs_x = 1, .ofs_y = -3},
    {.bitmap_index = 271, .adv_w = 224, .box_w = 7, .box_h = 12, .ofs_x = 0, .ofs_y = -1}
};

/*---------------------
 *  CHARACTER MAPPING
 *--------------------*/

static const uint16_t unicode_list_0[] = {
    0x0, 0x1, 0x5, 0x6, 0x7, 0x9, 0xa, 0xb,
    0x13, 0xfee, 0xfef, 0xff5, 0xff6, 0xff9, 0xffa, 0xffb,
    0xffc, 0xffd, 0xffe, 0x1001, 0x1002, 0xdeee, 0xdef9, 0xdefb,
    0xdf07, 0xdf08, 0xdf0c
};

/*Collect the unicode lists and glyph_id offsets*/
static const lv_font_fmt_txt_cmap_t cmaps[] =
{
    {
        .range_start = 8211, .range_length = 57101, .glyph_id_start = 1,
        .unicode_list = unicode_list_0, .glyph_id_ofs_list = NULL, .list_length = 27, .type = LV_FONT_FMT_TXT_CMAP_SPARSE_TINY
    }
};



/*--------------------
 *  ALL CUSTOM DATA
 *--------------------*/

#if LVGL_VERSION_MAJOR == 8
/*Store all the custom data of the font*/
static  lv_font_fmt_txt_glyph_cache_t cache;
#endif

#if LVGL_VERSION_MAJOR >= 8
static const lv_font_fmt_txt_dsc_t font_dsc = {
#else
static lv_font_fmt_txt_dsc_t font_dsc = {
#endif
    .glyph_bitmap = glyph_bitmap,
    .glyph_dsc = glyph_dsc,
    .cmaps = cmaps,
    .kern_dsc = NULL,
    .kern_scale = 0,
    .cmap_num = 1,
    .bpp = 2,
    .kern_classes = 0,
    .bitmap_format = 0,
#if LVGL_VERSION_MAJOR == 8
    .cache = &cache
#endif
};

extern const lv_font_t operit_font_google_color;


/*-----------------
 *  PUBLIC FONT
 *----------------*/

/*Initialize a public general font descriptor*/
#if LVGL_VERSION_MAJOR >= 8
const lv_font_t operit_font_punctuation_14 = {
#else
lv_font_t operit_font_punctuation_14 = {
#endif
    .get_glyph_dsc = lv_font_get_glyph_dsc_fmt_txt,    /*Function pointer to get glyph's data*/
    .get_glyph_bitmap = lv_font_get_bitmap_fmt_txt,    /*Function pointer to get glyph's bitmap*/
    .line_height = 16,          /*The maximum line height required by the font*/
    .base_line = 3,             /*Baseline measured from the bottom of the line*/
#if !(LVGL_VERSION_MAJOR == 6 && LVGL_VERSION_MINOR == 0)
    .subpx = LV_FONT_SUBPX_NONE,
#endif
#if LV_VERSION_CHECK(7, 4, 0) || LVGL_VERSION_MAJOR >= 8
    .underline_position = -2,
    .underline_thickness = 1,
#endif
    .dsc = &font_dsc,          /*The custom font data. Will be accessed by `get_glyph_bitmap/dsc` */
#if LV_VERSION_CHECK(8, 2, 0) || LVGL_VERSION_MAJOR >= 9
    .fallback = &operit_font_google_color,
#endif
    .user_data = NULL,
};



#endif /*#if OPERIT_FONT_PUNCTUATION_14*/

