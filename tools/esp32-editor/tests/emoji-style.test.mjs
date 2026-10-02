import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile, mkdir, writeFile} from 'node:fs/promises';
import {setTimeout as delay} from 'node:timers/promises';

test('emoji styles render different pig pixels, preserve fallback, persist and handle storage failure', async () => {
  globalThis.window ??= {};
  const saved = new Map();
  let failSave = false;
  Object.defineProperty(globalThis, 'localStorage', {configurable: true, value: {
    getItem: key => saved.get(key) ?? null,
    setItem: (key, value) => { if (failSave) throw new Error('storage full'); saved.set(key, value); },
  }});
  const {default: create} = await import('../generated/ui.mjs');
  const wasmBinary = await readFile(new URL('../generated/ui.wasm', import.meta.url));
  const ui = await create({wasmBinary});
  assert.equal(ui._simulator_init(), 1);
  if (!ui._operit_emoji_color_available()) {
    assert.equal(ui._operit_lvgl_set_emoji_style(1), 0, 'An uninstalled resource pack cannot be selected');
    assert.equal(ui._operit_lvgl_emoji_style(), 0);
    delete globalThis.localStorage;
    return;
  }
  const call = (name, types = [], args = []) => ui.ccall('operit_lvgl_' + name, null, types, args);
  const tree = () => JSON.parse(ui.ccall('operit_lvgl_debug_tree', 'string', [], []));
  const node = id => tree().nodes.find(n => n.id === id);
  const pump = async () => { await delay(50); ui._operit_lvgl_pump(0); };
  const tap = async id => {
    assert.equal(ui.ccall('operit_lvgl_debug_tap', 'number', ['string'], [id]), 1, id);
    await pump();
  };
  const frame = () => {
    const pointer = ui._simulator_frame();
    return Buffer.from(ui.HEAPU8.slice(pointer, pointer + 320 * 240 * 2));
  };
  const crop = (buffer, rect) => {
    const rows = [];
    for (let y = Math.max(0, rect.y); y < Math.min(240, rect.y + rect.h); y++) {
      rows.push(buffer.subarray((y * 320 + rect.x) * 2, (y * 320 + rect.x + rect.w) * 2));
    }
    return Buffer.concat(rows);
  };
  const pinkCount = buffer => {
    let count = 0;
    for (let i = 0; i < buffer.length; i += 2) {
      const pixel = buffer.readUInt16LE(i), r = (pixel >> 11) * 255 / 31;
      const g = ((pixel >> 5) & 63) * 255 / 63, b = (pixel & 31) * 255 / 31;
      if (r > 130 && r > g * 1.12 && r > b * 1.15) count++;
    }
    return count;
  };
  call('set_paired', ['number'], [1]);
  call('set_connection', ['number','number'], [1,1]);
  call('set_chat_identity', ['string','string'], ['emoji-verification','Emoji']);
  call('set_chat_screen', ['string'], ['']);
  const sent = [];
  ui.onAction = action => {
    if (action === 'edge_send') sent.push(ui.ccall('operit_lvgl_chat_draft', 'string', [], []));
  };
  call('set_chat_draft', ['string'], ['🐖']);
  await tap('send_button');
  assert.deepEqual(sent, ['🐖']);
  call('chat_send_result', ['number','string'], [1,'']);
  call('set_message', ['number','number','string'], [0,1,'🐖']);
  call('set_message', ['number','number','string'], [1,0,'🚀']); // Not in the color subset.
  call('finish_messages', ['number'], [2]);
  await pump();
  const mono = frame();
  const monoPig = crop(mono, node('message_text_0').rect);
  const monoFallback = crop(mono, node('message_text_1').rect);
  assert.equal(pinkCount(monoPig), 0);
  assert.equal(tree().emojiStyle, 0);
  call('set_chat_draft', ['string'], ['保留草稿 🐖']);
  await tap('sidebar_toggle'); await delay(150); await pump(); await tap('settings');
  assert(node('emoji_preview').text.includes('🐖'));
  await tap('emoji_google');
  assert.equal(tree().emojiStyle, 1);
  assert.equal(saved.get('operit.esp32.emojiStyle'), '1');
  await tap('edge_chat');
  assert.equal(node('message_input').text, '保留草稿 🐖');
  const color = frame();
  const colorPig = crop(color, node('message_text_0').rect);
  assert(pinkCount(colorPig) > 15, `Google pig must render actual pink pixels (${pinkCount(colorPig)}, ${JSON.stringify(node('message_text_0').rect)})`);
  assert.deepEqual(crop(color, node('message_text_1').rect), monoFallback, 'Uncovered emoji retain original glyph');
  assert.equal(ui._operit_lvgl_set_emoji_style(999), 0);
  assert.equal(tree().emojiStyle, 1);
  const fresh = await create({wasmBinary});
  fresh._simulator_init();
  assert.equal(fresh._operit_lvgl_emoji_style(), 1, 'Reload must restore selection');
  failSave = true;
  assert.equal(ui._operit_lvgl_set_emoji_style(0), 0);
  assert.equal(tree().emojiStyle, 1, 'Failed persistence must preserve current style');
  failSave = false;
  assert.equal(ui._operit_lvgl_set_emoji_style(0), 1);
  await pump();
  assert.deepEqual(crop(frame(), node('message_text_0').rect), monoPig, 'Switching back restores original pig');
  saved.set('operit.esp32.emojiStyle', '999');
  const invalid = await create({wasmBinary}); invalid._simulator_init();
  assert.equal(invalid._operit_lvgl_emoji_style(), 0, 'Invalid saved value falls back safely');
  if (process.env.OPERIT_UI_SNAPSHOTS) {
    await mkdir(process.env.OPERIT_UI_SNAPSHOTS, {recursive: true});
    for (const [name, pixels] of Object.entries({mono, google: color})) {
      const rgb = Buffer.alloc(320 * 240 * 3);
      for (let i = 0; i < 320 * 240; i++) {
        const p = pixels.readUInt16LE(i * 2);
        rgb[i*3] = (p >> 11) * 255 / 31; rgb[i*3+1] = ((p >> 5) & 63) * 255 / 63; rgb[i*3+2] = (p & 31) * 255 / 31;
      }
      await writeFile(`${process.env.OPERIT_UI_SNAPSHOTS}/emoji-${name}.ppm`, Buffer.concat([Buffer.from('P6\n320 240\n255\n'), rgb]));
    }
  }
  delete globalThis.localStorage;
});
