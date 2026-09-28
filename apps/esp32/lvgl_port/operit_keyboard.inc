/* The SVG's four compact rows. EN is the available input method; the desktop
 * host can also supply IME text. No unavailable Chinese engine is advertised. */
static const char *const keyboard_lower[] = {
    "q","w","e","r","t","y","u","i","o","p","\n",
    "a","s","d","f","g","h","j","k","l","\n",
    LV_SYMBOL_UP,"z","x","c","v","b","n","m",LV_SYMBOL_BACKSPACE,"\n",
    "123","EN","空格","完成",""
};
static const char *const keyboard_upper[] = {
    "Q","W","E","R","T","Y","U","I","O","P","\n",
    "A","S","D","F","G","H","J","K","L","\n",
    LV_SYMBOL_UP,"Z","X","C","V","B","N","M",LV_SYMBOL_BACKSPACE,"\n",
    "123","EN","空格","完成",""
};
static const char *const keyboard_numbers[] = {
    "1","2","3","4","5","6","7","8","9","0","\n",
    "@","#","$","%","&","*","(",")","+","\n",
    "-","_","=","/",":",";",".",",",LV_SYMBOL_BACKSPACE,"\n",
    "ABC","EN","空格","完成",""
};
static const lv_buttonmatrix_ctrl_t keyboard_controls[] = {
    1,1,1,1,1,1,1,1,1,1,
    1,1,1,1,1,1,1,1,1,
    1,1,1,1,1,1,1,1,1,
    2,2,6,2 | LV_BUTTONMATRIX_CTRL_CHECKED,
};
static void keyboard_value_changed(lv_event_t *event) {
    lv_obj_t *keyboard = lv_event_get_target(event);
    const char *key = lv_buttonmatrix_get_button_text(keyboard, lv_buttonmatrix_get_selected_button(keyboard));
    if (!key || !chat_input) return;
    if (!strcmp(key, "完成")) { remember_chat_draft(); hide_chat_keyboard(); }
    else if (!strcmp(key, "123")) lv_keyboard_set_mode(keyboard, LV_KEYBOARD_MODE_SPECIAL);
    else if (!strcmp(key, "ABC") || !strcmp(key, "EN")) lv_keyboard_set_mode(keyboard, LV_KEYBOARD_MODE_TEXT_LOWER);
    else if (!strcmp(key, LV_SYMBOL_UP)) lv_keyboard_set_mode(keyboard,
        lv_keyboard_get_mode(keyboard) == LV_KEYBOARD_MODE_TEXT_LOWER ? LV_KEYBOARD_MODE_TEXT_UPPER : LV_KEYBOARD_MODE_TEXT_LOWER);
    else if (!strcmp(key, LV_SYMBOL_BACKSPACE)) lv_textarea_delete_char(chat_input);
    else lv_textarea_add_text(chat_input, !strcmp(key, "空格") ? " " : key);
}
