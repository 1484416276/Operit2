# ESP32 Edge pairing

The ESP32 advertises `_operit-edge._tcp` over mDNS. Use the Core application's
device scan to discover it, start pairing, and enter the code displayed on the
ESP32. Core and Edge must be on the same reachable LAN. Opening the browser
editor automatically starts a separately identified `esp32-edge-simulator`.

The firmware registers a native `device.status` plugin. Its `read` action
returns board id, expression, IP address, and Wi-Fi name over the paired Link.
The pairing code is excluded from plugin results. Core ToolPkg UI is not
rendered on this display unless it has an explicit Edge adaptation.

## Local configuration

The ESP32 uses the same Edge token as the Core/CLI main link. Put that token in
the local `.edge-token` file under `apps/esp32`; the firmware build reads it
automatically. The file is ignored by Git and must never be committed or
included in logs. `OPERIT_EDGE_TOKEN` can still override it for CI or a
deliberate one-off build:

```powershell
cargo build --release
```

Leave the setup page's token field empty to keep using this same token. Enter a
different value only when the Core/CLI token is intentionally being rotated.

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
their constituent glyphs; full Unicode shaping is not provided.
`tools/esp32-editor/generate-fonts.py` regenerates this font and pairing digits
from supplied OFL font files. Uncompressed bitmaps match the firmware LVGL build.

Color emoji is an optional font pack and is excluded from the default firmware;
the default retains the original monochrome font. To embed the optional color
pack, generate it and set `OPERIT_EMOJI_COLOR_ENABLED` to `1` in
`lvgl_port/operit_emoji_config.h`; this adds about 65 KiB to firmware. The device's
**Settings → Emoji** then offers **原有单色** and **Google 彩色** with a live
🐖/🥺 preview. Preference persists via NVS or browser localStorage.

The color style uses 284 common Noto Color Emoji glyphs, stored as 16 px indexed
images with 16 RGBA colors per glyph to fit the 4 MB board. Characters outside
this subset fall back to the original monochrome font. `emoji-color.json` defines
coverage, the exact upstream font revision, its SHA-256 and an 80,000-byte asset
budget. It includes 🐖 U+1F416 and 🥺 U+1F97A. No Minecraft assets are used.

These images are derived from the **font**, not from separately licensed SVG
artwork: <https://github.com/googlefonts/noto-emoji/tree/e20cbc2bbec1926686be9f9bee7d1d2cfa1fea0e/2D/fonts>.
Noto Color Emoji is Copyright 2013 Google LLC, distributed under SIL OFL 1.1;
`lvgl_port/OFL-emoji.txt` accompanies both the source-derived glyphs and the
monochrome font. Google is credited as the source, without implying endorsement.

To reproduce the checked-in color glyph data (Python with Pillow and fonttools):

```powershell
python tools/esp32-editor/generate-color-emoji.py
# An optional --font path uses a local copy with the same verified SHA-256.
cd tools/esp32-editor
npm run build:firmware
node --test tests/emoji-style.test.mjs tests/ui-runtime.test.mjs
```

The optional color emoji regression sends 🐖 through the UI action, checks the
pink framebuffer pixels, verifies unchanged fallback glyphs, switches back to
the original style, and checks saved preferences and storage failure. Physical
display verification still requires flashing the matching firmware.

## Chat image transfer and preview

Core owns original image bytes. Authenticated Edge uploads PNG/JPEG image bytes
through Link (512 KiB maximum, 1 MiB TCP frame maximum); Core registers the
original in the existing media pool and sends its normal image media link through
`sendUserMessage`. The editor's computer composer has the matching image picker.
An existing image link in chat appears as a preview button on Edge. Core decodes
the original, applies allocation and dimension limits, scales it to at most
128×96, converts it to RGB565 and displays it centered at native 1:1 pixel
scale. Preview data crosses Link as 1 KiB binary
chunks. LVGL allocates one 24 KiB buffer only while the preview is open, draws it
without adding JPEG/PNG decoder code to firmware, and frees it on close, chat
switch, pairing revoke or failed transfer. Old Core media-pool entries can expire;
the UI reports that case and can retry by reopening a current image.

The preview is intentionally a static, bounded thumbnail. This board profile has
no enabled PSRAM, and its SPI SD card chip-select is only reserved, not an
implemented storage surface. Small RGB565 previews fit internal RAM and network
bandwidth budgets. Camera capture and live video would need a camera sensor on
free pins plus a verified PSRAM board variant; neither is claimed here.
