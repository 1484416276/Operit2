import { type Project, type Track, noteName } from "../../src/shared/model";
import type { WasmAudioEngine } from "../audio/wasm-engine";
export function context(canvas: HTMLCanvasElement): { ctx: CanvasRenderingContext2D; w: number; h: number } {
  const rect = canvas.getBoundingClientRect(); const ratio = Math.min(2, devicePixelRatio || 1); const width = Math.max(1, Math.round(rect.width * ratio)); const height = Math.max(1, Math.round(rect.height * ratio));
  if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
  const ctx = canvas.getContext("2d")!; ctx.setTransform(ratio, 0, 0, ratio, 0, 0); ctx.clearRect(0, 0, rect.width, rect.height); return { ctx, w: rect.width, h: rect.height };
}
export function drawTrack(canvas: HTMLCanvasElement, track: Track, project: Project, beat: number): void {
  const { ctx, w, h } = context(canvas); const total = project.bars * project.beatsPerBar;
  ctx.fillStyle = "#15191a"; ctx.fillRect(0, 0, w, h);
  for (let b = 0; b <= project.bars; b++) { ctx.strokeStyle = b % 2 ? "#242a2b" : "#303636"; ctx.beginPath(); ctx.moveTo(b * w / project.bars, 0); ctx.lineTo(b * w / project.bars, h); ctx.stroke(); }
  if (track.notes.length) {
    let lo = 127, hi = 0; for (const n of track.notes) { lo = Math.min(lo, n.pitch); hi = Math.max(hi, n.pitch); }
    ctx.fillStyle = track.color; ctx.globalAlpha = track.mute ? 0.2 : 0.8;
    for (const n of track.notes) { const y = 10 + (hi - n.pitch) / Math.max(12, hi - lo) * (h - 22); ctx.fillRect(n.start / total * w, y, Math.max(2, n.duration / total * w - 1), track.synth.engine === "drums" ? 5 : 4); }
    ctx.globalAlpha = 1;
  }
  ctx.fillStyle = "#d5fc97"; ctx.fillRect(beat / total * w, 0, 1.5, h);
}
export function pitchBounds(track: Track): [number, number] { let low = track.synth.engine === "drums" ? 35 : 48; let high = track.synth.engine === "drums" ? 57 : 72; for (const n of track.notes) { low = Math.min(low, n.pitch - 1); high = Math.max(high, n.pitch + 1); } return [Math.max(0, low), Math.min(127, high)]; }
export function drawPiano(canvas: HTMLCanvasElement, track: Track, project: Project, beat: number, barsVisible: number, startBar: number): void {
  const { ctx, w, h } = context(canvas); const [low, high] = pitchBounds(track); const rows = high - low + 1; const rowH = h / rows; const gutter = 42; const total = barsVisible * project.beatsPerBar; const start = startBar * project.beatsPerBar;
  for (let pitch = low; pitch <= high; pitch++) {
    const y = (high - pitch) * rowH; const black = [1, 3, 6, 8, 10].includes(pitch % 12);
    ctx.fillStyle = black ? "#141819" : "#1a1f20"; ctx.fillRect(gutter, y, w - gutter, rowH);
    ctx.fillStyle = black ? "#242a2a" : "#b5beb5"; ctx.fillRect(0, y, gutter - 2, rowH - 1);
    if (pitch % 12 === 0 || track.synth.engine === "drums") { ctx.fillStyle = black ? "#b4c0b2" : "#24312c"; ctx.font = "9px monospace"; ctx.fillText(noteName(pitch), 5, y + Math.min(rowH - 1, 10)); }
  }
  for (let i = 0; i <= total * 4; i++) { ctx.strokeStyle = i % (project.beatsPerBar * 4) === 0 ? "#414c43" : i % 4 === 0 ? "#303834" : "#242b28"; const x = gutter + i / (total * 4) * (w - gutter); ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke(); }
  for (const note of track.notes) {
    if (note.start + note.duration <= start || note.start >= start + total) continue;
    const x = gutter + Math.max(0, note.start - start) / total * (w - gutter); const width = Math.min(note.start + note.duration, start + total) - Math.max(start, note.start);
    ctx.fillStyle = track.color; ctx.globalAlpha = 0.4 + note.velocity * 0.6; ctx.fillRect(x + 1, (high - note.pitch) * rowH + 1, Math.max(2, width / total * (w - gutter) - 2), Math.max(2, rowH - 2));
  }
  ctx.globalAlpha = 1;
  if (beat >= start && beat < start + total) { ctx.fillStyle = "#edffd4"; ctx.fillRect(gutter + (beat - start) / total * (w - gutter), 0, 1.5, h); }
}
export function drawScope(canvas: HTMLCanvasElement, engine: WasmAudioEngine, mode: "spectrum" | "waveform"): void {
  const { ctx, w, h } = context(canvas);
  ctx.strokeStyle = "#27302c"; for (let i = 1; i < 4; i++) { ctx.beginPath(); ctx.moveTo(0, h * i / 4); ctx.lineTo(w, h * i / 4); ctx.stroke(); }
  if (mode === "spectrum") {
    const data = engine.spectrum(); const count = 40; const step = w / count;
    for (let i = 0; i < count; i++) { const at = Math.floor(Math.pow(i / count, 2) * (data.length - 1)); const height = Math.max(2, data[at] / 255 * (h - 10)); ctx.fillStyle = i > 29 ? "#8baf7e" : "#b8f36b"; ctx.globalAlpha = 0.3 + height / h * 0.7; ctx.fillRect(i * step + 1, h - height, Math.max(1, step - 3), height); } ctx.globalAlpha = 1;
  } else { const data = engine.waveform(); ctx.strokeStyle = "#b8f36b"; ctx.lineWidth = 1.5; ctx.beginPath(); for (let i = 0; i < data.length; i += 2) { const x = i / data.length * w; const y = h / 2 - data[i] * h * 0.46; if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y); } ctx.stroke(); }
}
