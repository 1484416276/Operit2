import { chromium } from "playwright";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile, mkdir, writeFile } from "node:fs/promises";
const server = createServer(async (_req, res) => { res.writeHead(200, { "Content-Type": "text/html;charset=utf-8" }); res.end(await readFile("resources/studio.html")); });
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const url = `http://127.0.0.1:${server.address().port}/?test=1`;
const browser = await chromium.launch({ channel: "chrome", headless: true });
const errors = [];
await mkdir(".test-output", { recursive: true });
try {
  for (const [width, height] of [[1440, 900], [1024, 768], [768, 1024], [390, 844], [320, 568], [844, 390]]) {
    const context = await browser.newContext({ viewport: { width, height } }); const page = await context.newPage(); page.on("pageerror", error => errors.push(String(error)));
    await page.goto(url); await page.waitForFunction(() => window.__musicTest);
    async function contained(label) {
      const metrics = await page.evaluate(() => ({ width: innerWidth, height: innerHeight, docW: document.documentElement.scrollWidth, docH: document.documentElement.scrollHeight, bodyW: document.body.scrollWidth, bodyH: document.body.scrollHeight, transport: document.querySelector(".transport").getBoundingClientRect().toJSON() }));
      assert.ok(metrics.docW <= width + 1 && metrics.bodyW <= width + 1, `${width} ${label}: horizontal page overflow ${JSON.stringify(metrics)}`);
      assert.ok(metrics.docH <= height + 1 && metrics.bodyH <= height + 1, `${width} ${label}: vertical page overflow ${JSON.stringify(metrics)}`);
      assert.ok(metrics.transport.bottom <= height + 1 && metrics.transport.top >= 0, `${width} ${label}: transport offscreen`);
    }
    await contained("initial");
    const ruler = await page.locator('#ruler').evaluate(el => ({width:el.clientWidth, childrenWidth:[...el.children].reduce((n,c)=>n+c.getBoundingClientRect().width,0)}));
    assert.ok(Math.abs(ruler.childrenWidth-ruler.width) < 2, 'ruler bars must align with fitted lanes');
    assert.equal(await page.locator(".side-panel").count(), 0); assert.equal(await page.locator("#editor-body").isVisible(), false); assert.equal(await page.locator("#overview-scope").count(), 0);
    await page.screenshot({ path: `.test-output/layout-${width}x${height}.png` });
    await page.locator('[data-action="library"]').click(); assert.ok(await page.locator(".library").isVisible()); await contained("library");
    assert.equal(await page.locator(".library details").getAttribute("open"), null);
    await page.locator('[data-action="master"]').first().click(); assert.equal(await page.locator(".library").count(), 0); assert.ok(await page.locator(".master").isVisible());
    assert.equal(await page.locator(".master details[open]").count(), 0); await contained("master");
    await page.locator('[data-action="close-panel"]').last().click();
    await page.locator('[data-tab="synth"]').click(); assert.ok(await page.locator(".synth-panel").isVisible()); assert.equal(await page.locator(".device-disclosure[open]").count(), 0);
    if (width <= 899 || height <= 550) assert.equal(await page.locator(".arrangement-pane").isVisible(), false); else assert.ok(await page.locator(".arrangement-pane").isVisible());
    await contained("synth");
    await page.locator('[data-tab="piano"]').click(); assert.ok(await page.locator("#piano").isVisible());
    const canvas = await page.locator("#piano").boundingBox(); assert.ok(canvas.height > 50, `piano height ${canvas.height}`); await contained("piano");
    await page.screenshot({ path: `.test-output/piano-${width}x${height}.png` });
    await page.locator('[data-action="close-editor"]').click(); assert.ok(await page.locator(".arrangement-pane").isVisible()); assert.equal(await page.locator("#editor-body").isVisible(), false);
    await page.locator('[data-action="project-menu"]').click(); await page.locator('[data-action="settings"]').click(); assert.ok(await page.locator("#setting-name").isVisible()); assert.equal(await page.locator(".project-menu").count(), 0); await contained("settings");
    await page.keyboard.press("Escape");
    await page.locator('[data-action="scope"]').click(); assert.ok(await page.locator("#overview-scope").isVisible()); await contained("scope");
    if (width === 1440) {
      await page.locator('[data-action="play"]').click(); await page.waitForTimeout(450);
      assert.equal((await page.evaluate(() => window.__musicTest.state())).playing, true);
      await page.locator('[data-action="library"]').click(); assert.equal((await page.evaluate(() => window.__musicTest.state())).playing, true);
      await page.locator('[data-action="close-panel"]').last().click(); await page.locator('[data-action="stop"]').click();
      // Seek into the full drop and verify real stereo analysers, then cross short loop boundaries.
      await page.locator('#section-jump').selectOption('64');
      await page.locator('[data-action="play"]').click(); await page.waitForTimeout(900);
      const live = await page.evaluate(() => window.__musicTest.state());
      assert.ok(live.beat > 64 && live.beat < 68 && live.stereo.every(x => x > 0.0001)); assert.equal(live.dropped, 0);
      await page.locator('[data-action="stop"]').click();
      await page.evaluate(async () => { const p = window.__musicTest.project(); await window.__musicTest.request({action:'batch',projectId:p.id,revision:p.revision,operations:[{type:'project.set',patch:{loop:{enabled:true,start:64,end:66}}}]}); });
      await page.waitForFunction(() => window.__musicTest.project().loop.enabled);
      await page.locator('[data-action="play"]').click(); await page.waitForTimeout(850);
      const loopBeats = []; for (let i=0;i<12;i++) { loopBeats.push((await page.evaluate(() => window.__musicTest.state())).beat); await page.waitForTimeout(90); }
      assert.ok(loopBeats.every(b => b >= 64 && b < 66)); assert.ok(Math.max(...loopBeats)-Math.min(...loopBeats) > 1, 'loop playhead must not freeze');
      await page.locator('[data-action="stop"]').click();
      await page.evaluate(async () => { const p = window.__musicTest.project(); await window.__musicTest.request({action:'batch',projectId:p.id,revision:p.revision,operations:[{type:'project.set',patch:{loop:{enabled:false,start:0,end:192}}}]}); });
      await page.waitForFunction(() => !window.__musicTest.project().loop.enabled);
      const renderStart = Date.now();
      const rendered = await page.evaluate(() => window.__musicTest.render(true));
      await writeFile('.test-output/eclipse.wav', Buffer.from(rendered.audio, 'base64')); delete rendered.audio;
      assert.ok(rendered.peak > 0.1 && rendered.peak < 0.99 && rendered.bytes > 100000);
      assert.equal(rendered.dropped, 0); assert.ok(Math.abs(rendered.duration - 80) < 0.001);
      assert.ok(rendered.analysis.rms.every(x => x > 0.01));
      assert.ok(rendered.analysis.sideRms > 0.002 && rendered.analysis.correlation < 0.999);
      assert.ok(rendered.analysis.midRms > rendered.analysis.sideRms, 'mono fold-down must preserve body');
      const sections = rendered.analysis.sections;
      assert.ok(sections[2].rms > sections[0].rms * 1.3, 'first drop energy must exceed intro');
      assert.ok(sections[5].rms > sections[3].rms * 1.3, 'main drop energy must exceed breakdown');
      console.log("WAV render verified", rendered, 'render seconds', (Date.now()-renderStart)/1000);
      await writeFile('.test-output/eclipse-analysis.json', JSON.stringify(rendered, null, 2));
      // Effects parameters are collapsed and can still be edited after disclosure.
      await page.locator('[data-select="e_chords"]').click(); await page.locator('[data-tab="effects"]').click(); assert.equal(await page.locator(".fx-details[open]").count(), 0);
      await page.locator(".fx-details summary").first().click(); assert.ok(await page.locator('.fx-details input').first().isVisible());
      await page.locator('[data-action="close-editor"]').click();
      // AI/model updates preserve open state, selected tab, and timeline scroll.
      await page.locator('[data-action="zoom-in"]').click(); await page.locator('#timeline-scroll').evaluate(el => { el.scrollLeft = 100; });
      await page.evaluate(async () => { const p = window.__musicTest.project(); await window.__musicTest.request({ action: "batch", projectId: p.id, revision: p.revision, operations: [{ type: "project.set", patch: { name: "AI updated" } }] }); });
      await page.waitForFunction(() => document.querySelector('#session-name').textContent === 'AI updated');
      assert.equal(await page.locator('#editor-body').isVisible(), false);
    }
    console.log(`PASS responsive DAW ${width}×${height}`); await context.close();
  }
  assert.deepEqual(errors, []); console.log("No browser runtime errors.");
} finally { await browser.close(); await new Promise(resolve => server.close(resolve)); }
