#pragma once
#include <stdbool.h>

typedef enum {
    OPERIT_EMOJI_MONO = 0,
    OPERIT_EMOJI_GOOGLE = 1,
    OPERIT_EMOJI_STYLE_COUNT
} operit_emoji_style_t;

void operit_emoji_init(void);
unsigned operit_emoji_style(void);
bool operit_emoji_color_available(void);
/* Persist before applying; a failure leaves the current style intact. */
bool operit_emoji_set_style(unsigned style);
