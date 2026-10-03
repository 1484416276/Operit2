import "./styles.css";
import { type Project, type Track, type Snapshot, type Operation, type Request, type Command, type PlaybackStatus, copy, uid, LIMITS } from "../src/shared/model";
import { PRESETS, EFFECTS } from "../src/shared/presets";
import { TEMPLATES, PATTERNS } from "../src/shared/composition";
import { SYNTH_RANGES } from "../src/shared/validation";
import { WasmAudioEngine } from "./audio/wasm-engine";
import { renderWav } from "./audio/wasm-render";
import { request, saveFile, hostMode, base64 } from "./host";
import { studioLayout, escapeHtml, type LayoutState } from "./ui/layout";
import { drawTrack, drawPiano, drawScope, pitchBounds } from "./ui/canvas";
const root = document.getElementById("app")!;
let snapshot: Snapshot;
let engine: WasmAudioEngine;
let selected = "";
let tab = "piano";
let category = "All";
let search = "";
let page = 0;
let visibleBars = 4;
let mutationBusy = false;
let rendering = false;
let syncing = false;
let paintDirty = true;
let panel: LayoutState["panel"] = null;
let projectMenuOpen = false;
let editorOpen = false;
let editorHeight = 310;
let scopeOpen = false;
let timelineZoom = 0; // 0 = responsive full-song overview; editing zoom is opt-in.
let resizing = false;
const disclosures = new Set<string>();
let lastFrame = 0;
let lastCanvasBeat = -1;
let lastTrackPaint = 0;
let pollTimer: ReturnType<typeof setTimeout> | undefined;
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
const processing = new Set<string>();
const activity: { time: string; text: string }[] = [];
const $ = <T extends HTMLElement = HTMLElement>(selector: string): T => root.querySelector<T>(selector)!;
const esc = escapeHtml;
const project = (): Project => snapshot.project;
const track = (): Track | undefined => project().tracks.find(t => t.id === selected);
function revision(): { projectId: string; revision: number } { return { projectId: project().id, revision: project().revision }; }
function log(message: string): void { activity.unshift({ time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }), text: message }); activity.splice(6); renderActivity(); }
function notify(message: string, error = false): void {
  document.querySelector(".notice")?.remove(); if (noticeTimer) clearTimeout(noticeTimer);
  const div = document.createElement("div"); div.className = `notice${error ? " error" : ""}`; div.setAttribute("role", error ? "alert" : "status"); div.textContent = message; document.body.append(div); noticeTimer = setTimeout(() => div.remove(), error ? 11000 : 6000);
}
function accept(next: Snapshot, source = "工程已更新"): void {
  const changed = !snapshot || snapshot.project.id !== next.project.id || snapshot.project.revision !== next.project.revision;
  const switched = snapshot && snapshot.project.id !== next.project.id;
  snapshot = next;
  if (!changed) return;
  if (!engine) engine = new WasmAudioEngine(next.project); else engine.setProject(next.project);
  if (!next.project.tracks.some(t => t.id === selected)) selected = next.project.tracks[0]?.id ?? "";
  if (switched) page = 0; page = Math.min(page, Math.max(0, project().bars - visibleBars));
  log(source); render();
}
async function mutate(value: Request, message: string): Promise<void> {
  if (mutationBusy || rendering) { notify("正在处理，请稍候"); return; }
  mutationBusy = true;
  try { const next = await request({ ...value, ...revision() }); projectMenuOpen = false; accept(next, message); }
  catch (error) { notify(String(error), true); try { accept(await request({ action: "get" }), "已同步最新工程，请重试修改"); } catch { /* Preserve original error. */ } }
  finally { mutationBusy = false; }
}
function batch(operations: Operation[], message = "已自动保存"): Promise<void> { return mutate({ action: "batch", operations }, message); }
function currentTrackOp(type: string, extras: Record<string, unknown>): void { if (track()) void batch([{ type, trackId: selected, ...extras }]); }
function param(key: string, label: string, value: number, min: number, max: number, step: number, group = "synth", extra = ""): string {
  return `<div class="parameter"><label for="${esc(group + key + extra)}">${label}<output>${value >= 1000 ? `${(value / 1000).toFixed(1)}k` : Number(value.toFixed(3))}</output></label><input id="${esc(group + key + extra)}" aria-label="${label}" type="range" min="${min}" max="${max}" step="${step}" value="${value}" data-param="${key}" data-group="${group}" data-effect="${extra}"></div>`;
}
/** Rebuild only on discrete edits; preserve viewport/focus and disclosure state. */
function render(): void {
  const timeline = root.querySelector("#timeline-scroll");
  const editor = root.querySelector("#editor-body");
  const panelScroll = root.querySelector(".panel-scroll");
  const scrolls = [["#timeline-scroll", timeline?.scrollLeft ?? 0, timeline?.scrollTop ?? 0], ["#editor-body", editor?.scrollLeft ?? 0, editor?.scrollTop ?? 0], [".panel-scroll", panelScroll?.scrollLeft ?? 0, panelScroll?.scrollTop ?? 0]] as const;
  const focused = document.activeElement as HTMLElement | null;
  const focusId = focused?.id;
  const focusAction = focused?.dataset.action;
  root.innerHTML = studioLayout({ snapshot, selected, tab, editorOpen, editorHeight, panel, menu: projectMenuOpen, category, search, scope: scopeOpen, zoom: timelineZoom, metronome: engine.metronome, host: hostMode() });
  renderPresets(); renderEditor(); renderActivity(); bind(); restoreDisclosures();
  for (const [selector, left, top] of scrolls) { const node = root.querySelector(selector); if (node) { node.scrollLeft = left; node.scrollTop = top; } }
  if (focusId) root.querySelector<HTMLElement>(`#${CSS.escape(focusId)}`)?.focus({ preventScroll: true });
  else if (focusAction) root.querySelector<HTMLElement>(`[data-action="${CSS.escape(focusAction)}"]`)?.focus({ preventScroll: true });
  paintDirty = true; updateTransport();
}
function restoreDisclosures(): void {
  for (const details of root.querySelectorAll<HTMLDetailsElement>("details")) {
    const key = details.dataset.disclosure ?? details.querySelector("summary")?.textContent ?? "";
    details.open = disclosures.has(key);
    details.ontoggle = () => { if (details.open) disclosures.add(key); else disclosures.delete(key); paintDirty = true; };
  }
}
function setPanel(next: LayoutState["panel"]): void {
  panel = next; projectMenuOpen = false; render();
  if (next) $("#side-panel")?.focus({ preventScroll: true });
}
function openEditor(next: string): void {
  tab = next; editorOpen = true; projectMenuOpen = false;
  if (matchMedia("(max-width: 899px)").matches) panel = null;
  render();
}
function formatTime(seconds: number, millis = true): string { return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(Math.floor(seconds % 60)).padStart(2, "0")}${millis ? "." + String(Math.floor(seconds % 1 * 1000)).padStart(3, "0") : ""}`; }
function renderActivity(): void { const node = root.querySelector("#activity"); if (node) node.innerHTML = activity.map(a => `<div class="activity-entry"><time>${esc(a.time)}</time>${esc(a.text)}</div>`).join(""); }
function renderPresets(): void {
  const list = $("#presets"); if (!list) return;
  list.innerHTML = PRESETS.filter(p => (category === "All" || p.category === category) && (p.name + p.category + p.description).toLowerCase().includes(search.toLowerCase())).map(p => `<button class="preset" data-preset="${p.id}" title="${esc(p.description)}"><span class="preset-icon">${p.category === "Drums" ? "▦" : p.category === "Bass" ? "≋" : p.category === "Texture" ? "◌" : "∿"}</span><span><strong>${esc(p.name.split(" / ")[0])}</strong><small>${esc(p.name.split(" / ")[1])} · ${p.category}</small></span></button>`).join("");
}
function renderEditor(): void {
  const t = track(); const body = $("#editor-body"); if (!body) return;
  if (!editorOpen) return;
  if (tab === "settings") { renderSettings(); return; }
  if (tab === "help") {
    body.innerHTML = `<div class="help-panel"><strong style="color:var(--fg)">AI 是编曲师，这里是你的聆听空间。</strong><br>七组工具：<code>music_project / tracks / notes / instruments / effects / arrangement / transport</code><br>先调用 <code>music_project.get</code> 与 <code>catalog</code>，所有修改携带工程 ID 和 revision。<br>音符时间单位为拍（四分音符），音高用 MIDI 编号。<br>可让 AI：「做一段 A 小调、112 BPM 的电子音乐，增加渐进的鼓组与宽阔织体。」<br>编辑立即保存，AI 改动自动同步。播放需首次点击解锁音频；关闭页面后停止播放。<br>这是独立的纯合成工作台，不支持 VST、采样库、录音或硬件 MIDI。<br><button data-action="export-json">导出当前工程 JSON</button> <button data-action="import">导入工程</button> <button data-action="new">新建空白工程</button></div>`; return;
  }
  if (!t) { body.innerHTML = '<div class="fx-empty">添加或选择一条轨道，然后探索声音。</div>'; return; }
  if (tab === "piano") {
    body.innerHTML = `<div class="piano-toolbar"><span>BAR ${page + 1}–${Math.min(project().bars, page + visibleBars)}</span><button data-action="prev-bars" aria-label="前一组小节">←</button><button data-action="next-bars" aria-label="后一组小节">→</button><select id="zoom" aria-label="钢琴卷帘显示小节数">${[2, 4, 8].map(n => `<option value="${n}" ${visibleBars === n ? "selected" : ""}>${n} 小节</option>`).join("")}</select><div class="spacer"></div><span class="editing-hint">点击添加／删除 · 1/16 网格</span><select id="pattern" aria-label="生成音型">${PATTERNS.map(k => `<option>${k}</option>`).join("")}</select><button data-action="generate">生成</button></div><canvas class="piano-canvas" id="piano" aria-label="可编辑钢琴卷帘，点击添加或删除音符"></canvas>`;
  } else if (tab === "synth") {
    const s = t.synth; const names = { spectral: "SPECTRA / 双振荡器", fm: "ION / 双算子 FM", ensemble: "ATELIER / 合成乐器", drums: "CIRCUIT / 模拟鼓机", atmosphere: "AETHER / 立体声环境" };
    const synthParam = (key: string): string => { const [min, max] = SYNTH_RANGES[key]; return param(key, ({ blend: "OSC BLEND", detune: "DETUNE · ct", unison: "UNISON", attack: "ATTACK · s", decay: "DECAY · s", sustain: "SUSTAIN", release: "RELEASE · s", cutoff: "CUTOFF · Hz", resonance: "RESONANCE", fmRatio: "FM RATIO", fmDepth: "FM DEPTH", brightness: "BRIGHTNESS", width: "STEREO WIDTH", filterEnv: "FILTER ENV · oct", lfoRate: "LFO · Hz", lfoDepth: "LFO DEPTH · oct", pitchSweep: "PITCH SWEEP · st" } as Record<string, string>)[key], s[key as keyof typeof s] as number, min, max, key === "unison" ? 1 : key === "cutoff" ? 10 : key === "detune" ? 1 : 0.01); };
    const core = s.engine === "drums" ? ["decay", "brightness"] : s.engine === "fm" ? ["fmRatio", "fmDepth"] : ["cutoff", "blend"];
    const extra = s.engine === "drums" ? [] : Object.keys(SYNTH_RANGES).filter(key => !core.includes(key) && (s.engine === "fm" ? !["unison", "detune", "blend"].includes(key) : !["fmRatio", "fmDepth"].includes(key)));
    body.innerHTML = `<div class="synth-panel"><div class="synth-top"><div class="synth-symbol">∿</div><div><div class="synth-name">${names[s.engine]}</div><div class="synth-sub">${esc(t.preset)} · PURE SYNTHESIS</div></div><div class="spacer"></div><select id="change-preset" aria-label="更换轨道音色">${PRESETS.map(p => `<option value="${p.id}" ${p.id === t.preset ? "selected" : ""}>${esc(p.name)}</option>`).join("")}</select></div>
    <div class="parameters">${core.map(synthParam).join("")}${param("gain", "TRACK LEVEL", t.gain, 0, 1.5, 0.01, "track")}${param("pan", "PAN · L / R", t.pan, -1, 1, 0.01, "track")}</div>
    ${s.engine === "spectral" || s.engine === "ensemble" ? `<details class="device-disclosure" data-disclosure="oscillators"><summary>振荡器 / OSCILLATORS</summary><div class="parameters">${["wave", "waveB"].map((key, i) => `<div class="parameter"><label>${i ? "OSC B" : "OSC A"}</label><select data-synth-choice="${key}" aria-label="${key}">${["sine", "triangle", "sawtooth", "square", "glass", "hollow"].map(w => `<option ${s[key as "wave"] === w ? "selected" : ""}>${w}</option>`).join("")}</select></div>`).join("")}</div></details>` : ""}
    ${extra.length ? `<details class="device-disclosure" data-disclosure="synthesis-${s.engine}"><summary>包络与高级参数 / SOUND DESIGN</summary><div class="parameters">${extra.map(synthParam).join("")}</div></details>` : ""}
    <details class="device-disclosure" data-disclosure="instrument-notes"><summary>关于此乐器</summary><p class="small muted">FM 使用频比／深度；模拟鼓使用衰减／明亮度。弦乐等为振荡器合成近似音色，并非采样真实乐器。</p></details></div>`;
  } else if (tab === "effects") {
    body.innerHTML = `<div class="fx-list">${t.effects.map((fx, i) => `<div class="fx"><div class="fx-title"><span class="mono muted">0${i + 1}</span><strong>${EFFECTS[fx.type].name}</strong><span class="spacer"></span><button data-fx-toggle="${fx.id}" class="${fx.enabled ? "active" : ""}">${fx.enabled ? "ON" : "BYPASS"}</button><button data-fx-remove="${fx.id}" aria-label="移除效果器">×</button></div><details class="fx-details" data-disclosure="fx-${fx.id}"><summary>展开参数 / PARAMETERS</summary><div class="parameters">${param("mix", "DRY / WET", fx.mix, 0, 1, 0.01, "effect", fx.id)}${Object.entries(EFFECTS[fx.type].ranges).map(([key, [min, max]]) => param(key, key.toUpperCase(), fx.params[key], min, max, max > 100 ? 10 : max > 10 ? 0.1 : 0.001, "fxparam", fx.id)).join("")}</div></details></div>`).join("") || '<div class="fx-empty">干净的声音，留给你无限可能。<br><br>添加一个效果器，开始塑造空间与质感。</div>'}</div><div class="fx-add"><select id="effect-type" aria-label="效果器类型">${Object.entries(EFFECTS).map(([k, v]) => `<option value="${k}">${v.name}</option>`).join("")}</select><button data-action="add-effect">＋ 添加效果器</button></div>`;
  } else if (tab === "drums") {
    if (t.synth.engine !== "drums") { body.innerHTML = '<div class="fx-empty">先选择一条鼓轨，或添加一套模拟鼓组。<br><br><button data-preset="analog-kit">＋ Circuit 模拟鼓组</button></div>'; return; }
    const bar = Math.min(page, project().bars - 1); const start = bar * project().beatsPerBar;
    body.innerHTML = `<div class="piano-toolbar"><span>BAR ${bar + 1} · 前4拍 / 16 STEPS</span><button data-action="prev-drum">←</button><button data-action="next-drum">→</button><span class="spacer"></span><select id="pattern" aria-label="鼓组节奏类型">${["four-floor", "half-time", "breakbeat"].map(k => `<option>${k}</option>`).join("")}</select><button data-action="generate">生成全曲节奏</button></div><div class="drum-grid">${[[36, "KICK"], [38, "SNARE"], [39, "CLAP"], [42, "CLOSED HAT"], [46, "OPEN HAT"], [45, "TOM"], [49, "CRASH"], [56, "COWBELL"]].map(([pitch, name]) => `<div class="drum-row"><span>${name}</span>${Array.from({ length: 16 }, (_, step) => `<button class="drum-step ${t.notes.some(n => n.pitch === pitch && Math.abs(n.start - start - step / 4) < 0.01) ? "active" : ""}" data-drum="${pitch}" data-step="${step}" ${step / 4 >= project().beatsPerBar ? "disabled" : ""} aria-label="${name} 第${step + 1}步"></button>`).join("")}</div>`).join("")}</div>`;
  }
  paintDirty = true;
}
function bind(): void {
  // Reassign properties to avoid accumulating event listeners when the shell is rebuilt.
  root.onclick = e => { const target = (e.target as HTMLElement).closest<HTMLElement>("button,[data-select],canvas,#ruler,[data-action]"); if (!target) return; void handleClick(target, e).catch(error => notify(String(error), true)); };
  root.oninput = e => {
    const input = e.target as HTMLInputElement;
    if (input.id === "preset-search") { search = input.value; renderPresets(); }
    if (input.type === "range") { const out = input.parentElement?.querySelector("output"); if (out) out.textContent = Number(input.value).toFixed(Number(input.step) < 0.01 ? 3 : 2); }
  };
  root.onchange = e => { void handleChange(e.target as HTMLInputElement).catch(error => notify(String(error), true)); };
  const splitter = root.querySelector<HTMLElement>("#dock-resizer");
  if (splitter) {
    splitter.onpointerdown = e => {
      if (matchMedia("(max-width: 899px), (max-height: 550px)").matches) return;
      e.preventDefault(); const initial = editorHeight; const startY = e.clientY; resizing = true; splitter.setPointerCapture(e.pointerId);
      const resize = (event: PointerEvent): void => {
        editorHeight = Math.max(180, Math.min(Math.min(600, innerHeight - 280), initial + startY - event.clientY));
        $(".work-area").style.setProperty("--editor-height", `${editorHeight}px`); splitter.setAttribute("aria-valuenow", String(editorHeight)); paintDirty = true;
      };
      splitter.onpointermove = resize;
      splitter.onpointerup = splitter.onpointercancel = () => { resizing = false; splitter.onpointermove = null; };
    };
    splitter.onkeydown = e => { if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return; e.preventDefault(); editorHeight = Math.max(180, Math.min(Math.min(600, innerHeight - 280), editorHeight + (e.key === "ArrowUp" ? 20 : -20))); render(); };
  }
  root.onkeydown = e => {
    const target = e.target as HTMLElement;
    if ((e.key === "Enter" || e.key === " ") && target.hasAttribute("data-select")) { e.preventDefault(); selected = target.dataset.select!; render(); }
    if (e.key === "Escape") { e.preventDefault(); if (projectMenuOpen) projectMenuOpen = false; else if (panel) panel = null; else editorOpen = false; render(); }
    if (e.key === "Tab" && projectMenuOpen) {
      const dialog = root.querySelector(".project-menu")!;
      const focusable = [...dialog.querySelectorAll<HTMLElement>('button,select,input,[tabindex="0"]')];
      const first = focusable[0], last = focusable[focusable.length - 1];
      if (e.shiftKey && (document.activeElement === first || document.activeElement === dialog)) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    }
  };
}
async function handleChange(input: HTMLInputElement): Promise<void> {
  if (input.id === "projects") await mutate({ action: "open", id: input.value }, "切换工程");
  else if (input.id === "bpm") await batch([{ type: "project.set", patch: { bpm: Number(input.value) } }]);
  else if (input.id === "zoom") { visibleBars = Number(input.value); page = Math.min(page, Math.max(0, project().bars - visibleBars)); renderEditor(); restoreDisclosures(); }
  else if (input.id === "section-jump") { const beat = Number(input.value); engine.seek(beat); page = Math.min(project().bars - 1, Math.floor(beat / project().beatsPerBar)); const timeline = $("#timeline-scroll"); const ruler = $("#ruler"); if (timeline && ruler) timeline.scrollLeft = beat / (project().bars * project().beatsPerBar) * ruler.clientWidth; paintDirty = true; updateTransport(); }
  else if (input.id === "change-preset") currentTrackOp("track.preset", { preset: input.value });
  else if (input.dataset.synthChoice) currentTrackOp("synth.set", { patch: { [input.dataset.synthChoice]: input.value } });
  else if (input.dataset.group) {
    const value = Number(input.value), key = input.dataset.param!; const group = input.dataset.group;
    if (group === "project") await batch([{ type: "project.set", patch: { [key]: value } }]);
    else if (group === "track") currentTrackOp("track.set", { patch: { [key]: value } });
    else if (group === "synth") currentTrackOp("synth.set", { patch: { [key]: value } });
    else currentTrackOp("effect.set", { effectId: input.dataset.effect, patch: group === "effect" ? { [key]: value } : { params: { [key]: value } } });
  } else if (input.id === "import-file" && input.files?.[0]) {
    const file = input.files[0]; if (file.size > 2000000) throw new Error("工程文件不能超过2MB"); await mutate({ action: "import", json: await file.text() }, "导入工程");
  }
}
async function handleClick(target: HTMLElement, event: MouseEvent): Promise<void> {
  const d = target.dataset;
  if (d.category) { category = d.category; render(); return; }
  if (d.tab) { if (tab === d.tab && editorOpen) { editorOpen = false; render(); } else openEditor(d.tab); return; }
  if (d.device) { selected = d.device; openEditor("synth"); return; }
  if (d.select) { selected = d.select; render(); return; }
  if (d.preset) { await batch([{ type: "track.add", preset: d.preset }], "添加合成音色"); selected = project().tracks[project().tracks.length - 1]?.id ?? selected; panel = null; openEditor("synth"); return; }
  if (d.template) { panel = null; await mutate({ action: "create", template: d.template }, "从模板创建工程"); return; }
  if (d.mute || d.solo) { const id = d.mute || d.solo; const t = project().tracks.find(t => t.id === id)!; const key = d.mute ? "mute" : "solo"; await batch([{ type: "track.set", trackId: id, patch: { [key]: !t[key] } }]); return; }
  if (d.fxRemove) { currentTrackOp("effect.remove", { effectId: d.fxRemove }); return; }
  if (d.fxToggle) { const fx = track()!.effects.find(e => e.id === d.fxToggle)!; currentTrackOp("effect.set", { effectId: fx.id, patch: { enabled: !fx.enabled } }); return; }
  if (d.drum) {
    const t = track()!; const pitch = Number(d.drum), start = page * project().beatsPerBar + Number(d.step) / 4;
    const exists = t.notes.find(n => n.pitch === pitch && Math.abs(n.start - start) < 0.01);
    currentTrackOp(exists ? "notes.remove" : "notes.add", exists ? { ids: [exists.id] } : { notes: [{ pitch, start, duration: 0.1, velocity: 0.7 }] }); return;
  }
  if (target.id === "piano") {
    const t = track()!; const rect = target.getBoundingClientRect(); const x = event.clientX - rect.left; if (x < 42) return;
    const [low, high] = pitchBounds(t); const pitch = Math.max(low, Math.min(high, high - Math.floor((event.clientY - rect.top) / rect.height * (high - low + 1))));
    const start = Math.floor((x - 42) / (rect.width - 42) * visibleBars * project().beatsPerBar * 4) / 4 + page * project().beatsPerBar;
    if (start >= project().bars * project().beatsPerBar) return;
    const exists = t.notes.find(n => n.pitch === pitch && start >= n.start && start < n.start + n.duration);
    currentTrackOp(exists ? "notes.remove" : "notes.add", exists ? { ids: [exists.id] } : { notes: [{ pitch, start, duration: Math.min(t.synth.engine === "drums" ? 0.1 : 0.25, project().bars * project().beatsPerBar - start), velocity: 0.7 }] }); return;
  }
  if (d.canvas || target.id === "ruler") {
    const rect = target.getBoundingClientRect(); if (d.canvas) { selected = d.canvas; render(); }
    // A replaced canvas is detached; use its saved geometry before rebuilding instead.
    if (rect.width) engine.seek((event.clientX - rect.left) / rect.width * project().bars * project().beatsPerBar); paintDirty = true; return;
  }
  switch (d.action) {
    case "play": if (engine.playing) engine.pause(); else await engine.play(); updateTransport(); break;
    case "stop": engine.stop(); updateTransport(); paintDirty = true; break;
    case "loop": await batch([{ type: "project.set", patch: { loop: { ...project().loop, enabled: !project().loop.enabled } } }]); break;
    case "metronome": engine.metronome = !engine.metronome; target.classList.toggle("active", engine.metronome); target.setAttribute("aria-pressed", String(engine.metronome)); break;
    case "library": setPanel(panel === "library" ? null : "library"); break;
    case "master": setPanel(panel === "master" ? null : "master"); break;
    case "close-panel": setPanel(null); break;
    case "project-menu": projectMenuOpen = !projectMenuOpen; panel = null; render(); if (projectMenuOpen) root.querySelector<HTMLElement>(".project-menu")?.focus(); break;
    case "close-menu": projectMenuOpen = false; render(); break;
    case "add-track": setPanel("library"); $("#preset-search").focus(); break;
    case "close-editor": editorOpen = false; render(); break;
    case "toggle-editor": editorOpen = !editorOpen; render(); break;
    case "scope": scopeOpen = !scopeOpen; render(); break;
    case "zoom-in": timelineZoom = timelineZoom === 0 ? 0.5 : Math.min(4, timelineZoom * 1.5); render(); break;
    case "zoom-out": timelineZoom = timelineZoom / 1.5; if (timelineZoom < 0.1) timelineZoom = 0; render(); break;
    case "zoom-fit": timelineZoom = 0; render(); break;
    case "prev-bars": page = Math.max(0, page - visibleBars); renderEditor(); break;
    case "next-bars": page = Math.min(Math.max(0, project().bars - visibleBars), page + visibleBars); renderEditor(); break;
    case "prev-drum": page = Math.max(0, page - 1); renderEditor(); break;
    case "next-drum": page = Math.min(project().bars - 1, page + 1); renderEditor(); break;
    case "add-effect": currentTrackOp("effect.add", { effectType: $<HTMLSelectElement>("#effect-type").value }); break;
    case "generate": if (track()) currentTrackOp("pattern.generate", { options: { kind: $<HTMLSelectElement>("#pattern").value, root: track()!.preset.includes("bass") ? 45 : 57, bars: project().bars, seed: 1 } }); break;
    case "undo": case "redo": await mutate({ action: d.action }, d.action === "undo" ? "撤销修改" : "重做修改"); break;
    case "new": await mutate({ action: "create", name: "Untitled session" }, "新建空白工程"); break;
    case "import": $<HTMLInputElement>("#import-file").click(); break;
    case "export-json": { const p = project(); const path = await saveFile(`${p.id}_${p.revision}.operitmusic.json`, new TextEncoder().encode(JSON.stringify(p, null, 2))); notify(`工程已导出：${path}`); break; }
    case "render": await exportAudio(); break;
    case "settings": showSettings(); break;
    case "save-settings": {
      const name = $<HTMLInputElement>("#setting-name").value; const bpm = Number($<HTMLInputElement>("#setting-bpm").value); const bars = Number($<HTMLInputElement>("#setting-bars").value); const beatsPerBar = Number($<HTMLInputElement>("#setting-beats").value); const swing = Number($<HTMLInputElement>("#setting-swing").value);
      await batch([{ type: "project.set", patch: { name, bpm, bars, beatsPerBar, swing, loop: { enabled: project().loop.enabled, start: 0, end: bars * beatsPerBar } } }], "更新工程设置"); break;
    }
    case "duplicate": await mutate({ action: "duplicate" }, "复制工程"); break;
    case "remove-track": if (track() && confirm(`移除轨道「${track()!.name}」及其所有音符？`)) currentTrackOp("track.remove", {}); break;
    case "delete-project": if (confirm(`永久删除工程「${project().name}」？此操作不能撤销。`)) await mutate({ action: "delete", confirm: project().id }, "已删除工程"); break;
  }
}
function showSettings(): void { openEditor("settings"); }
function renderSettings(): void {
  const p = project();
  $("#editor-body").innerHTML = `<div class="synth-panel"><div class="synth-top"><div class="synth-name">工程 / SESSION</div><span class="spacer"></span><span class="small muted">${esc(p.id)}</span></div><div class="parameters">${[["name", "工程名称", p.name, "text"], ["bpm", "BPM", p.bpm, "number"], ["bars", "小节数", p.bars, "number"], ["beats", "每小节拍数", p.beatsPerBar, "number"], ["swing", "Swing 0–0.65", p.swing, "number"]].map(([key, name, value, type]) => `<div class="parameter"><label for="setting-${key}">${name}</label><input style="width:100%;margin-top:8px" id="setting-${key}" type="${type}" ${key === "swing" ? 'step="0.01"' : ""} value="${esc(value)}"></div>`).join("")}</div><p class="small muted">缩短工程时，越界音符或段落会导致校验失败，不会静默裁剪。请先通过 AI 编辑处理。</p><div class="row wrap"><button class="primary" data-action="save-settings">保存设置</button><button data-action="duplicate">复制工程</button><button data-action="new">新建工程</button><button data-action="import">导入</button><button data-action="remove-track">移除所选轨道</button><button data-action="delete-project">删除工程</button></div></div>`;
}
function updateTransport(): void {
  if (!engine || !snapshot) return;
  const p = project(), beat = engine.beat; const button = $("#play"); if (button) button.textContent = engine.playing ? "Ⅱ" : "▶";
  const clock = $("#clock"); if (clock) clock.textContent = `${String(Math.floor(beat / p.beatsPerBar) + 1).padStart(3, "0")} : ${String(Math.floor(beat % p.beatsPerBar) + 1).padStart(2, "0")}`;
  const time = $("#time"); if (time) time.textContent = `${formatTime(beat * 60 / p.bpm)} / ${formatTime(p.bars * p.beatsPerBar * 60 / p.bpm, false)}`;
}
function frame(now: number): void {
  requestAnimationFrame(frame); if (!engine || !snapshot || document.hidden || now - lastFrame < 33) return; lastFrame = now;
  updateTransport(); const peak = engine.peak(); const transportMeter = $("#transport-meter"); if (transportMeter) transportMeter.style.height = `${Math.min(100, peak * 140)}%`; const db = peak > 0.00001 ? Math.max(-60, 20 * Math.log10(peak)) : -60;
  const stereo = engine.stereoPeak();
  ["meter-left", "meter-right"].forEach((key, i) => { const el = $("#" + key); const channelDb = 20 * Math.log10(Math.max(0.000001, stereo[i])); if (el) el.style.height = `${Math.max(0, Math.min(100, (60 + channelDb) / 60 * 100))}%`; });
  const scopeDb = $("#scope-db"); if (scopeDb) scopeDb.textContent = `${peak > 0.00001 ? db.toFixed(1) : "−∞"} dB`;
  const d = $("#master-db"); if (d) d.textContent = peak > 0.00001 ? db.toFixed(1) : "−∞";
  const voices = $("#voices"); if (voices) voices.textContent = `${engine.activeVoices} / ${LIMITS.voices}`;
  const rate = $("#sample-rate"); if (rate) rate.textContent = engine.context ? `${(engine.context.sampleRate / 1000).toFixed(1)} kHz` : "—";
  const dropped = $("#dropped"); if (dropped) dropped.textContent = String(engine.dropped);
  for (const node of root.querySelectorAll<HTMLElement>("[data-meter]")) node.style.width = `${Math.min(100, engine.peak(node.dataset.meter) * 180)}%`;
  const scope = $<HTMLCanvasElement>("#overview-scope"); if (scope) drawScope(scope, engine, "waveform");
  const spectrum = $<HTMLCanvasElement>("#spectrum"); if (spectrum) drawScope(spectrum, engine, "spectrum");
  const beat = engine.beat;
  if ((paintDirty || beat !== lastCanvasBeat) && now - lastTrackPaint > 50) {
    for (const canvas of root.querySelectorAll<HTMLCanvasElement>("[data-canvas]")) { const t = project().tracks.find(t => t.id === canvas.dataset.canvas); if (t) drawTrack(canvas, t, project(), beat); }
    const piano = $<HTMLCanvasElement>("#piano"); if (piano && track()) drawPiano(piano, track()!, project(), beat, visibleBars, page);
    paintDirty = false; lastCanvasBeat = beat; lastTrackPaint = now;
  }
}
function status(): PlaybackStatus { return { connected: true, unlocked: engine.unlocked, playing: engine.playing, beat: engine.beat, peak: engine.peak(), voices: engine.activeVoices, dropped: engine.dropped, updatedAt: Date.now(), projectId: project().id }; }
async function command(c: Command): Promise<void> {
  if (processing.has(c.id)) return; processing.add(c.id);
  try {
    if (c.projectId !== project().id || c.revision !== project().revision) throw new Error("Command revision changed; not executed");
    let path: string | undefined;
    if (c.type === "play") { if (!engine.unlocked) throw new Error("请用户先点击一次播放，解锁音频"); await engine.play(); }
    else if (c.type === "pause") engine.pause();
    else if (c.type === "stop") engine.stop();
    else if (c.type === "seek") engine.seek(c.beat!);
    else path = await exportAudio(c);
    await request({ action: "ack", id: c.id, state: "done", message: c.type === "render" ? "WAV rendered and saved" : `Applied ${c.type}`, ...(path ? { path } : {}) });
    log(`AI · ${c.type === "render" ? "完成渲染" : c.type}`);
  } catch (error) {
    try { await request({ action: "ack", id: c.id, state: "failed", message: String(error).slice(0, 900) }); } catch { /* Project switch already cancelled the command. */ }
    notify(String(error), true);
  } finally { updateTransport(); paintDirty = true; processing.delete(c.id); }
}
async function sync(): Promise<void> {
  if (syncing || !engine) return; syncing = true;
  try {
    const next = await request<Partial<Snapshot>>({ action: "sync", ...revision(), status: status() });
    if (next.project && next.projects && !resizing) accept(next as Snapshot, "AI / 外部编辑已同步");
    const node = $("#transport-status"); if (node) node.textContent = `${hostMode() ? "AI 已连接" : "浏览器预览"} · 工程已自动保存\n${engine.unlocked ? "音频就绪" : "点击播放启用音频"}`;
    // Do not await a render here: heartbeat must continue while native offline rendering runs.
    for (const c of next.commands ?? []) if (!processing.has(c.id)) void command(c);
  } catch (error) { const node = $("#transport-status"); if (node) node.textContent = `连接异常：${String(error).slice(0, 90)}`; }
  finally { syncing = false; }
}
async function exportAudio(command?: Command): Promise<string> {
  if (rendering) throw new Error("已有渲染任务进行中"); rendering = true; engine.pause(); const p = copy(project());
  const overlay = document.createElement("div"); overlay.className = "render-overlay"; overlay.setAttribute("role", "status"); overlay.innerHTML = '<div class="render-spinner"></div><h2>把灵感变成声音文件。</h2><div class="muted" id="render-progress">准备离线渲染…</div><div class="small muted">本地合成 · 不上传任何音频</div>'; document.body.append(overlay);
  try {
    const result = await renderWav(p, message => { overlay.querySelector("#render-progress")!.textContent = message; });
    if (command) { const current = await request({ action: "get" }); if (current.project.id !== command.projectId || current.project.revision !== command.revision || !current.commands.some(c => c.id === command.id)) throw new Error("工程在渲染期间被修改，已取消导出；请重新渲染"); }
    const path = await saveFile(`${p.id}_r${p.revision}_${Date.now()}.wav`, result.bytes);
    log(`WAV · ${(result.bytes.length / 1048576).toFixed(2)} MB · ${result.dropped ? `丢弃${result.dropped}个超限音符` : "完成"}`);
    notify(`WAV 已保存：${path}${result.dropped ? `（${result.dropped} 个音符超过声部上限）` : ""}`); return path;
  } finally { rendering = false; overlay.remove(); updateTransport(); }
}
async function initialize(): Promise<void> {
  root.innerHTML = '<div class="welcome"><strong>operit / music studio</strong><span>正在唤醒合成器…</span></div>';
  try {
    let next: Snapshot | undefined; let lastError: unknown;
    for (let attempt = 0; attempt < 8; attempt++) { try { next = await request({ action: "get" }); break; } catch (e) { lastError = e; await new Promise(resolve => setTimeout(resolve, 400)); } }
    if (!next) throw lastError; accept(next, "工程就绪 · 所有音色均为本地合成");
    const schedulePoll = async (): Promise<void> => { await sync(); pollTimer = setTimeout(schedulePoll, document.hidden ? 2000 : 700); }; void schedulePoll();
    engine.onEnded = updateTransport;
    window.addEventListener("resize", () => { paintDirty = true; });
    new ResizeObserver(() => { paintDirty = true; }).observe(root);
    document.addEventListener("visibilitychange", () => { if (document.hidden && engine.playing) { engine.pause(); log("页面进入后台，已暂停以避免节拍漂移"); } paintDirty = true; });
    window.addEventListener("pagehide", () => { engine.stop(); if (pollTimer) clearTimeout(pollTimer); void engine.dispose(); });
    window.addEventListener("keydown", e => { if (e.defaultPrevented) return; if (e.key === "Escape") { if (projectMenuOpen) projectMenuOpen = false; else if (panel) panel = null; else editorOpen = false; render(); return; } if (["INPUT", "SELECT", "TEXTAREA", "BUTTON"].includes((e.target as HTMLElement).tagName) || e.ctrlKey || e.metaKey || rendering) return; if (e.code === "Space") { e.preventDefault(); if (engine.playing) engine.pause(); else void engine.play().catch(e => notify(String(e), true)); } });
    // Diagnostics are opt-in on a local browser preview, never exposed to packaged production pages.
    if (new URLSearchParams(location.search).has("test") && !hostMode()) window.__musicTest = {
      project: () => copy(project()), snapshot: () => request({ action: "get" }), request: value => request(value),
      render: async (includeAudio = false) => { const r = await renderWav(project()); return { audio: includeAudio ? base64(r.bytes) : undefined, peak: r.peak, bytes: r.bytes.length, dropped: r.dropped, duration: r.duration, analysis: r.analysis }; },
      play: () => engine.play(), stop: () => engine.stop(), state: () => ({ playing: engine.playing, beat: engine.beat, voices: engine.activeVoices, peak: engine.peak(), stereo: engine.stereoPeak(), dropped: engine.dropped, state: engine.context?.state, dsp: engine.diagnostics() }),
    };
    requestAnimationFrame(frame);
  } catch (error) { root.innerHTML = `<div class="welcome"><strong>无法打开音乐工作台</strong><span class="loading-error">${esc(error)}</span><button id="retry">重试连接</button><span>已有工程不会被覆盖。</span></div>`; document.getElementById("retry")!.onclick = () => void initialize(); }
}
void initialize();
