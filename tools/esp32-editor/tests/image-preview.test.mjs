import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {setTimeout as delay} from 'node:timers/promises';

test('chat image links open a bounded preview, receive RGB565 chunks and release it on close', async () => {
  globalThis.window ??= {};
  const {default:create} = await import('../generated/ui.mjs');
  const ui = await create({wasmBinary:await readFile(new URL('../generated/ui.wasm', import.meta.url))});
  assert.equal(ui._simulator_init(),1);
  const call=(name,types=[],args=[])=>ui.ccall('operit_lvgl_'+name,null,types,args);
  const tree=()=>JSON.parse(ui.ccall('operit_lvgl_debug_tree','string',[],[]));
  const node=id=>tree().nodes.find(n=>n.id===id);
  const actions=[];ui.onAction=value=>actions.push(value);
  call('set_paired',['number'],[1]); call('set_connection',['number','number'],[1,1]);
  call('set_chat_identity',['string','string'],['image-chat','图片测试']);
  call('set_chat_screen',['string'],['']);
  call('set_message',['number','number','string'],[0,1,'看这张图片']);
  call('set_message_image',['number','number','string'],[0,0,'image-test-1']);
  call('finish_messages',['number'],[1]);
  assert(node('message_image_0_0')?.visible);
  assert.equal(ui.ccall('operit_lvgl_debug_tap','number',['string'],['message_image_0_0']),1);
  ui._operit_lvgl_pump(0); await delay(40);ui._operit_lvgl_pump(0);
  assert.equal(tree().page,'Chat');
  assert(node('image_preview')?.visible);
  assert(actions.some(action=>action.startsWith('edge_image:')&&action.endsWith(':image-test-1')));
  const request=ui._operit_lvgl_image_request();
  const source = Buffer.alloc(48 * 32 * 2);
  for (let y=0;y<32;y++) for (let x=0;x<48;x++) {
    const color = y<16 ? (x<24 ? 0xf800 : 0x07e0) : (x<24 ? 0x001f : 0xffff);
    source.writeUInt16LE(color,(y*48+x)*2);
  }
  for (let offset=0;offset<source.length;offset+=1024) {
    assert.equal(ui.ccall('operit_lvgl_image_chunk','number',['number','number','number','number','array','number'],
      [request,48,32,offset,new Uint8Array(source.subarray(offset,offset+1024)),1024]),1);
    await delay(24); ui._operit_lvgl_pump(0);
  }
  await delay(40); ui._operit_lvgl_pump(0);
  assert(node('image_pixels')?.visible);
  assert.equal(node('image_status')?.visible,false);
  const frame=ui._simulator_frame(),pixels=new Uint16Array(ui.HEAPU8.buffer,frame,320*240);
  const rect=node('image_pixels').rect;
  assert.equal(rect.w, 48);
  assert.equal(rect.h, 32);
  if (process.env.OPERIT_UI_SNAPSHOTS) { const b=Buffer.alloc(320*240*3); for(let i=0;i<320*240;i++){const v=pixels[i];b[i*3]=(v>>11)*255/31;b[i*3+1]=((v>>5)&63)*255/63;b[i*3+2]=(v&31)*255/31;} const {mkdir,writeFile}=await import('node:fs/promises');await mkdir(process.env.OPERIT_UI_SNAPSHOTS,{recursive:true});await writeFile(process.env.OPERIT_UI_SNAPSHOTS+'/preview.ppm',Buffer.concat([Buffer.from('P6\n320 240\n255\n'),b])); }
  assert.equal([...pixels].filter(pixel=>pixel===0xf800).length,384,'Red quadrant contains 384 pixels');
  assert.equal([...pixels].filter(pixel=>pixel===0x07e0).length,384,'Green quadrant contains 384 pixels');
  assert.equal([...pixels].filter(pixel=>pixel===0x001f).length,384,'Blue quadrant contains 384 pixels');
  assert.equal([...pixels].filter(pixel=>pixel===0xffff).length,384,'White quadrant contains 384 pixels');
  assert.equal(ui.ccall('operit_lvgl_image_chunk','number',['number','number','number','number','array','number'],
    [request,129,1,0,new Uint8Array([0,0]),2]),0);
  await delay(40); ui._operit_lvgl_pump(0);
  assert(node('image_status')?.visible);
  assert(!node('image_pixels')?.visible);
  assert.equal(ui.ccall('operit_lvgl_debug_tap','number',['string'],['image_close']),1);
  ui._operit_lvgl_pump(0);
  assert(!node('image_preview')?.visible);
  assert(actions.includes('edge_image_cancel'));
  assert.equal(ui._operit_lvgl_image_request(),request+1);
  call('set_message_image',['number','number','string'],[0,0,'image-test-2']);
  call('finish_messages',['number'],[1]);
  assert(node('message_image_0_0')?.visible);
  assert.equal(ui.ccall('operit_lvgl_debug_tap','number',['string'],['message_image_0_0']),1);
  ui._operit_lvgl_pump(0);await delay(40);ui._operit_lvgl_pump(0);
  const stale=ui._operit_lvgl_image_request();
  call('set_chat_identity',['string','string'],['other-chat','另一对话']);
  assert.equal(ui.ccall('operit_lvgl_image_chunk','number',['number','number','number','number','array','number'],
    [stale,1,1,0,new Uint8Array([0,0]),2]),0);
  assert(!node('image_preview')?.visible);
});
