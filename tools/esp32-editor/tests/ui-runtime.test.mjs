import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile, writeFile, mkdir} from 'node:fs/promises';
import {setTimeout as delay} from 'node:timers/promises';

// Run the same compiled LVGL and RGB565 framebuffer as the browser/device.
test('SVG layout, real conversation rows, keyboard bounds and pairing states', async () => {
  globalThis.window ??= {};
  const {default: create} = await import('../generated/ui.mjs');
  const ui = await create({wasmBinary: await readFile(new URL('../generated/ui.wasm', import.meta.url))});
  assert.equal(ui._simulator_init(), 1);
  const call = (name, types = [], values = []) => ui.ccall('operit_lvgl_' + name, null, types, values);
  const tree = () => JSON.parse(ui.ccall('operit_lvgl_debug_tree', 'string', [], []));
  const node = id => tree().nodes.find(n => n.id === id);
  const pump = async (ms = 40) => { await delay(ms); ui._operit_lvgl_pump(0); };
  const touch = async (x, y, down) => { ui._simulator_touch(x, y, down); await pump(); };
  const tap = async id => {
    const n = node(id); assert(n?.visible && n.enabled, id + ' must be available');
    const {x,y,w,h} = n.rect;
    await touch(x + w / 2, y + h / 2, 1); await touch(x + w / 2, y + h / 2, 0); await pump();
  };
  const frames = {};
  const capture = async name => {
    await pump();
    const offset = ui._simulator_frame();
    frames[name] = Buffer.from(ui.HEAPU8.slice(offset, offset + 320 * 240 * 2));
  };
  const actions = []; ui.onAction = value => actions.push(value);
  assert.equal(tree().page, 'Pairing');
  call('set_pairing_code', ['string'], ['123456']); await capture('pairing');
  call('set_paired', ['number'], [1]); call('set_connection', ['number','number'], [1,1]);
  call('set_chat_identity', ['string','string'], ['one','日常助手']);
  call('set_chat_screen', ['string'], ['']);
  call('set_message', ['number','number','string'], [0,1,'帮我安排今天的工作']);
  call('set_message', ['number','number','string'], [1,0,'当然可以。🥺\n先列出最重要的三件事。😊 ❤️']);
  call('finish_messages', ['number'], [2]); await capture('chat');
  assert.equal(node('message_text_1').text, '当然可以。🥺\n先列出最重要的三件事。😊 ❤️');
  assert.equal(node('voice_button').enabled, false);
  assert.equal(node('composer_row').rect.y, 198);
  for (let i = 0; i < 6; i++) call('set_conversation',
    ['number','string','string','string','number'], [i, i === 0 ? 'one' : `chat-${i}`, `对话 ${i+1}`, i < 3 ? '日常助手' : '学习搭档', i === 0 ? 1 : 0]);
  call('finish_conversations', ['number'], [6]);
  await tap('sidebar_toggle'); await pump(140); await capture('drawer');
  assert.equal(node('drawer_panel').rect.x, 0);
  assert.equal(node('drawer_footer').rect.y, 203);
  assert.equal(node('conversation_0').parent, 'conversation_list_0');
  assert.equal(node('conversation_list_0').parent, 'character_card_0');
  assert.equal(node('character_viewport').rect.h, 119);
  await tap('character_header_0');
  assert.equal(node('conversation_0').visible, false);
  await tap('character_header_0');
  await tap('conversation_1'); await pump();
  // Selecting a row is delivered through the same action callback as hardware.
  assert(actions.includes('edge_select:chat-1'));
  await tap('edge_unpair'); assert.equal(node('dialog_layer').visible, true);
  assert(!actions.includes('edge_unpair'), 'opening confirmation must not unpair');
  await tap('dialog_close'); assert.equal(tree().page, 'Chat');
  await tap('drawer_close_button'); await pump(120);
  call('set_chat_draft', ['string'], ['保留这份草稿']);
  await tap('message_input'); await capture('keyboard');
  assert.equal(node('composer_row').rect.y, 70);
  const keys = node('key_area').rect, keyboard = node('keyboard_window').rect;
  assert.equal(keys.y, keyboard.y + 24);
  assert(keys.y + keys.h <= 240);
  await touch(100, 124, 1); await touch(0, 0, 1); await touch(0, 0, 0);
  let moved = node('keyboard_window').rect;
  assert(moved.x >= 0 && moved.y >= 84);
  await touch(moved.x + 90, moved.y + 10, 1); await touch(319, 239, 1); await touch(319, 239, 0);
  moved = node('keyboard_window').rect;
  assert(moved.x + moved.w <= 320 && moved.y + moved.h <= 240);
  await tap('keyboard_close');
  assert.equal(node('message_input').text, '保留这份草稿');
  assert.equal(node('composer_row').rect.y, 198);
  assert(!actions.includes('edge_send'), 'keyboard completion/close must not send');
  await tap('message_input');
  const keyArea = node('key_area').rect;
  await touch(keyArea.x + keyArea.w - 22, keyArea.y + keyArea.h - 12, 1);
  await touch(keyArea.x + keyArea.w - 22, keyArea.y + keyArea.h - 12, 0);
  assert.equal(node('keyboard_window').visible, false, 'Done closes the keyboard');
  assert(!actions.includes('edge_send'), 'Done must not send');
  call('set_chat_identity', ['string','string'], ['two','学习搭档']);
  call('set_chat_draft', ['string'], ['另一份草稿']);
  call('set_chat_identity', ['string','string'], ['one','日常助手']);
  assert.equal(node('message_input').text, '保留这份草稿');
  call('set_connection', ['number','number'], [1,0]);
  assert.equal(tree().page, 'Chat', 'paired offline device must stay in Chat');
  call('set_theme', ['number','number'], [1,0]); await pump();
  assert.equal(node('message_input').text, '保留这份草稿');
  await tap('sidebar_toggle'); await pump(140); await tap('edge_unpair'); await tap('dialog_confirm');
  assert(actions.includes('edge_unpair')); assert.equal(tree().page, 'Chat');
  call('action_error', ['string'], ['无法保存配对状态']); assert.equal(tree().page, 'Chat');
  call('set_paired', ['number'], [0]); assert.equal(tree().page, 'Pairing');
  if (process.env.OPERIT_UI_SNAPSHOTS) {
    await mkdir(process.env.OPERIT_UI_SNAPSHOTS, {recursive:true});
    for (const [name, frame] of Object.entries(frames)) {
      const rgb = Buffer.alloc(320*240*3);
      for (let i=0;i<320*240;i++) {
        const v=frame.readUInt16LE(i*2); rgb[i*3]=(v>>11)*255/31;
        rgb[i*3+1]=((v>>5)&63)*255/63; rgb[i*3+2]=(v&31)*255/31;
      }
      await writeFile(`${process.env.OPERIT_UI_SNAPSHOTS}/${name}.ppm`, Buffer.concat([Buffer.from('P6\n320 240\n255\n'),rgb]));
    }
  }
});
