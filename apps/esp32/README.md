# ESP32 Edge pairing

The ESP32 advertises `_operit-edge._tcp` over mDNS. Use the Core application's
device scan to discover it, start pairing, and enter the code displayed on the
ESP32. Core and Edge must be on the same reachable LAN. Opening the browser
editor automatically starts a separately identified `esp32-edge-simulator`.

## Local configuration

Supply `OPERIT_EDGE_TOKEN` when building firmware. A local `.edge-token` file
may be used to keep this value between builds; it is ignored by Git and must
never be committed or included in logs. For example, from `apps/esp32`:

```powershell
$env:OPERIT_EDGE_TOKEN = (Get-Content .edge-token -Raw).Trim()
cargo build --release
```

Use the ESP32 toolchain and partition table documented in the repository's
`BUILDING.md`. Select the actual serial port for your device rather than assuming
the port in a developer's local configuration. Preserve existing Wi-Fi/NVS data
when updating only the application image.

## Memory and connection lifetime

- The firmware's Edge worker uses a 32 KiB stack; the previous 6 KiB stack
  overflowed while processing pairing requests.
- The optional RGB332 diagnostic mirror is disabled to leave heap space for
  pairing. The physical TFT remains enabled.
- Native Core requests share a process-lifetime Tokio executor so the saved
  pairing socket and background receivers survive between commands. The Rust
  DLL requires a native rebuild and application restart, not Dart hot reload.

## Verification and current limitations

Native scheduler regression tests cover socket reuse across requests, receiver
survival after request completion, and non-Send request futures. On the
ESP32-2432S028, three consecutive PairStart requests completed after flashing
the stack/mirror fix without rebooting. This is **not** verification of complete
correct-code pairing, Space admission, reconnect, or chat.

The drawer requests confirmation before clearing pairing credentials. Only a
successful storage update revokes the session and opens pairing; failures keep
the current page and show an error. A paired device stays in chat while offline.

The shared LVGL view displays up to 12 recent messages (2 KiB of UTF-8 per row)
and 24 conversation list items. Core owns the complete history. Conversations
are grouped by their character metadata, and selecting or creating a chat opens
its routed message subscription without changing the desktop's selection.

After `npm run build` in `tools/esp32-editor`, run
`node --test tests/ui-runtime.test.mjs` for the shared framebuffer/interaction
regression. Hardware flashing and touch validation remain separate checks.

## Third-party font

`lvgl_port/operit_font_zh_14.c` is an LVGL 14 px, 1 bpp subset of Noto Sans CJK SC
Regular, covering GB2312 first-level characters and punctuation. The source
project is <https://github.com/notofonts/noto-cjk>. The generation options are
recorded in the generated C header. Adobe's copyright and the SIL Open Font
License 1.1 are retained in the file and `lvgl_port/OFL.txt`.

`operit_font_emoji_16.c` adds monochrome Noto Emoji glyphs for the Unicode
symbol, pictograph and emoticon ranges, including 🥺. The source is
<https://github.com/google/fonts/tree/main/ofl/notoemoji>; see `OFL-emoji.txt`.
Formatting selectors have zero width; complex joined emoji are displayed as
their constituent glyphs. Color emoji and full Unicode shaping are not provided.
`tools/esp32-editor/generate-fonts.py` regenerates this font and pairing digits
from supplied OFL font files. Uncompressed bitmaps match the firmware LVGL build.
