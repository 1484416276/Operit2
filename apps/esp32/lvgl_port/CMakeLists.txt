idf_component_register(
    SRCS "operit_lvgl.c" "operit_font_zh_14.c" "operit_font_emoji_16.c" "operit_font_format.c" "operit_font_digits_28.c" "layout_store.c"
    INCLUDE_DIRS "."
    REQUIRES lvgl__lvgl esp_timer esp_partition spi_flash
)
