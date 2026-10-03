import { type Project, type Track, type Effect, type AutomationPoint, emptyProject, uid } from "../model";
import { makeTrack, makeEffect } from "../presets";

/** ECLIPSE · 逐光 - Redesigned for Tone.js with rich synthesis and powerful drops */
export const ECLIPSE_SECTIONS = [
  { name: "01 · EMBERS / 星火", bar: 0, bars: 8, color: "#819ab5" },
  { name: "02 · ASCENT / 升空", bar: 8, bars: 8, color: "#e3be79" },
  { name: "03 · FIRST LIGHT / 破晓", bar: 16, bars: 8, color: "#bbec84" },
  { name: "04 · WEIGHTLESS / 失重", bar: 24, bars: 4, color: "#a998df" },
  { name: "05 · IGNITION / 重燃", bar: 28, bars: 4, color: "#e5a477" },
  { name: "06 · SUPERNOVA / 超新星", bar: 32, bars: 12, color: "#76d7d2" },
  { name: "07 · AFTERGLOW / 余晖", bar: 44, bars: 4, color: "#a1aac9" },
] as const;

interface Harmony { name: string; root: number; chord: number[]; arp: number[] }
const Dm9: Harmony = { name: "Dm9", root: 38, chord: [53, 57, 60, 64, 74], arp: [62, 65, 69, 72, 76, 81] };
const Bb9: Harmony = { name: "Bbmaj9", root: 34, chord: [53, 57, 60, 62, 70], arp: [62, 65, 69, 70, 72, 77] };
const F9: Harmony = { name: "Fmaj9", root: 41, chord: [52, 57, 60, 65, 67], arp: [60, 64, 65, 69, 72, 79] };
const Csus: Harmony = { name: "Csus2/4", root: 36, chord: [55, 60, 62, 65, 72], arp: [60, 62, 65, 67, 72, 74] };
const Am7: Harmony = { name: "Am7", root: 33, chord: [55, 57, 60, 64, 69], arp: [60, 64, 67, 69, 72, 76] };
const Gm9: Harmony = { name: "Gm9", root: 43, chord: [53, 57, 58, 62, 67], arp: [58, 62, 65, 67, 69, 74] };
const A7: Harmony = { name: "A7sus → A7", root: 33, chord: [52, 55, 61, 64, 69], arp: [61, 64, 67, 69, 73, 76] };

function harmony(bar: number): Harmony {
  if (bar < 8) return [Dm9, Bb9, F9, Csus][Math.floor(bar / 2)];
  if (bar < 16) return [Bb9, F9, Gm9, A7][Math.floor((bar - 8) / 2)];
  if (bar < 24) return [Dm9, Bb9, F9, Csus, Dm9, F9, Gm9, A7][bar - 16];
  if (bar < 28) return [Bb9, F9, Am7, Dm9][bar - 24];
  if (bar < 32) return [Gm9, Bb9, Csus, A7][bar - 28];
  if (bar < 40) return [Dm9, Bb9, F9, Csus][Math.floor((bar - 32) / 2)];
  if (bar < 44) return [Gm9, Bb9, A7, Dm9][bar - 40];
  return [Bb9, Csus, Dm9, Dm9][bar - 44];
}

function fx(type: Effect["type"], mix: number, params: Record<string, number> = {}): Effect {
  const e = makeEffect(type);
  e.mix = mix;
  Object.assign(e.params, params);
  return e;
}

function points(pairs: number[][]): AutomationPoint[] {
  return pairs.map(([beat, value]) => ({ beat, value }));
}

export function eclipseProject(): Project {
  const project = emptyProject("ECLIPSE · 逐光");
  project.bpm = 150;
  project.bars = 48;
  project.masterGain = 0.72;
  project.loop = { enabled: false, start: 0, end: 192 };
  project.sections = ECLIPSE_SECTIONS.map(s => ({
    id: uid("section"),
    name: s.name,
    start: s.bar * 4,
    length: s.bars * 4,
    color: s.color
  }));

  const track = (id: string, name: string, preset: string, gain: number, pan: number, color: string, effects: Effect[] = []): Track => {
    const t = makeTrack(preset);
    t.id = id;
    t.name = name;
    t.gain = gain;
    t.pan = pan;
    t.color = color;
    t.effects = effects;
    project.tracks.push(t);
    return t;
  };

  // Drum tracks with spatial FX
  const kick = track("e_kick", "01 / Punch · 底鼓", "cinematic-kit", 0.88, 0, "#dcc47c", [
    fx("eq", 1, { low: 3, mid: -2, high: -1 }),
    fx("compressor", 1, { threshold: -15, ratio: 6, attack: 0.003, release: 0.15 })
  ]);
  kick.synth.decay = 0.16;
  kick.synth.brightness = 0.45;

  const snare = track("e_snare", "02 / Thunder · 军鼓", "analog-kit", 0.84, 0, "#d5a479", [
    fx("eq", 1, { low: -8, mid: 2, high: 3 }),
    fx("reverb", 0.095, { seconds: 0.8, decay: 4.2 }),
    fx("compressor", 1, { threshold: -18, ratio: 5, attack: 0.008, release: 0.18 })
  ]);
  snare.synth.decay = 0.17;
  snare.synth.brightness = 0.75;

  const hats = track("e_tops", "03 / Sparks · 踩镲", "tight-kit", 0.56, 0.18, "#d2bc95", [
    fx("eq", 1, { low: -18, mid: -6, high: 2 })
  ]);
  hats.synth.brightness = 0.88;

  const fills = track("e_fills", "04 / Ritual · 过门", "cinematic-kit", 0.54, -0.22, "#c8957c", [
    fx("reverb", 0.18, { seconds: 1.8, decay: 3.5 })
  ]);
  fills.synth.brightness = 0.62;

  // Bass layers - powerful low end
  const sub = track("e_sub", "05 / Titan · Sub", "titan-sub", 0.92, 0, "#b995dc", [
    fx("eq", 1, { low: 4, mid: -6, high: -18 }),
    fx("compressor", 1, { threshold: -12, ratio: 8, attack: 0.005, release: 0.12 })
  ]);
  sub.synth.cutoff = 180;
  sub.synth.sustain = 0.96;

  const reese = track("e_reese", "06 / Horizon · Mid Bass", "horizon-reese", 0.68, 0, "#ad88ca", [
    fx("eq", 1, { low: -16, mid: 2, high: -5 }),
    fx("drive", 0.4, { amount: 3.2 }),
    fx("compressor", 1, { threshold: -16, ratio: 5, attack: 0.008, release: 0.15 })
  ]);
  reese.synth.unison = 3;
  reese.synth.detune = 18;
  reese.synth.width = 0.55;
  reese.synth.cutoff = 1400;

  const growl = track("e_growl", "07 / Fault · 咆哮", "fault-bass", 0.71, -0.05, "#ce8ab9", [
    fx("eq", 1, { low: -14, mid: 3, high: -6 }),
    fx("drive", 0.45, { amount: 4.5 }),
    fx("filter", 0.85, { cutoff: 850, resonance: 3.2 }),
    fx("compressor", 1, { threshold: -14, ratio: 6, attack: 0.006, release: 0.14 })
  ]);
  growl.synth.fmDepth = 6;
  growl.synth.lfoDepth = 1.8;

  // Chord layers - wide supersaw
  const chords = track("e_chords", "08 / Eclipse · 超锯", "eclipse-chords", 0.94, 0, "#79ccc1", [
    fx("eq", 1, { low: -14, mid: 0, high: 2 }),
    fx("chorus", 0.18, { rate: 0.65, depth: 0.0045 }),
    fx("reverb", 0.14, { seconds: 1.9, decay: 3.6 }),
    fx("compressor", 1, { threshold: -16, ratio: 4, attack: 0.018, release: 0.25 })
  ]);
  chords.synth.unison = 4;
  chords.synth.detune = 22;
  chords.synth.width = 0.98;
  chords.synth.wave = "sawtooth";
  chords.synth.waveB = "sawtooth";
  chords.synth.blend = 0.45;
  chords.synth.filterEnv = 0.9;
  chords.synth.cutoff = 6800;
  chords.automation = [{
    target: "level",
    points: Array.from({ length: 129 }, (_, i) => ({
      beat: i * 1.5,
      value: i < 32 ? 0.52 + i * 0.006 : i < 64 ? 0.82 : i < 112 ? 0.92 : 0.92 - (i - 112) * 0.008
    }))
  }];

  const halo = track("e_halo", "09 / Halo · 高层", "halo-chords", 0.42, -0.15, "#8cb5d4", [
    fx("eq", 1, { low: -18, mid: -3, high: 0 }),
    fx("reverb", 0.35, { seconds: 2.8, decay: 3 })
  ]);
  halo.synth.unison = 3;
  halo.synth.detune = 12;

  // Lead melody
  const lead = track("e_lead", "10 / Comet · 主旋律", "comet-lead", 0.92, 0.03, "#eaca82", [
    fx("eq", 1, { low: -14, mid: 1, high: 1 }),
    fx("delay", 0.14, { beats: 0.75, feedback: 0.32, pingPong: 1 }),
    fx("reverb", 0.16, { seconds: 2, decay: 3.4 }),
    fx("compressor", 1, { threshold: -18, ratio: 3, attack: 0.012, release: 0.22 })
  ]);
  lead.synth.unison = 3;
  lead.synth.detune = 11;
  lead.synth.lfoRate = 5.5;
  lead.synth.lfoDepth = 0.06;

  const counter = track("e_counter", "11 / Nova · 对位", "neon-lead", 0.64, -0.08, "#e5b392", [
    fx("eq", 1, { low: -16, mid: 0, high: -1 }),
    fx("delay", 0.18, { beats: 0.5, feedback: 0.25, pingPong: 0 }),
    fx("reverb", 0.2, { seconds: 2.2, decay: 2.8 })
  ]);
  counter.synth.unison = 2;
  counter.synth.detune = 8;

  // Arps and textures
  const arp = track("e_arp", "12 / Prism · 琶音", "prism-arp", 0.58, 0.12, "#a8d3b5", [
    fx("eq", 1, { low: -18, mid: -2, high: 1 }),
    fx("delay", 0.16, { beats: 0.375, feedback: 0.22, pingPong: 1 }),
    fx("reverb", 0.12, { seconds: 1.4, decay: 2.6 })
  ]);
  arp.synth.filterEnv = 2.6;
  arp.synth.resonance = 1.5;

  const pluck = track("e_pluck", "13 / Shimmer · 拨弦", "glass-pluck", 0.48, -0.1, "#c5d4a8", [
    fx("eq", 1, { low: -18, mid: -4, high: 0 }),
    fx("reverb", 0.22, { seconds: 1.6, decay: 2.4 })
  ]);

  const stars = track("e_stars", "14 / Starlight · 星尘", "starlight-bell", 0.38, 0.15, "#dfd3a1", [
    fx("eq", 1, { low: -18, mid: -6, high: -1 }),
    fx("reverb", 0.28, { seconds: 2.4, decay: 2.8 })
  ]);

  // Pad layers
  const strings = track("e_strings", "15 / Horizon · 弦幕", "afterglow-pad", 0.52, -0.05, "#a8b8d1", [
    fx("eq", 1, { low: -12, mid: -2, high: -1 }),
    fx("reverb", 0.24, { seconds: 2.6, decay: 3.2 })
  ]);
  strings.synth.unison = 3;
  strings.synth.detune = 9;

  const pad = track("e_pad", "16 / Aurora · 织体", "aurora-pad", 0.42, 0.08, "#9fb5c8", [
    fx("eq", 1, { low: -14, mid: -3, high: -2 }),
    fx("reverb", 0.32, { seconds: 3, decay: 3.8 })
  ]);

  const air = track("e_air", "17 / Aether · 空气", "aether-air", 0.36, -0.12, "#aec2d0", [
    fx("eq", 1, { low: -8, mid: -4, high: 0 }),
    fx("reverb", 0.38, { seconds: 3, decay: 4 })
  ]);

  const rise = track("e_rise", "18 / Ascension · 升", "ascension-riser", 0.48, 0, "#d9c5a7", [
    fx("eq", 1, { low: -6, mid: 0, high: 2 }),
    fx("reverb", 0.24, { seconds: 2.8, decay: 3.6 })
  ]);

  const impacts = track("e_impacts", "19 / Monolith · 冲击层", "deep-kit", 0.48, 0, "#c6a5c9", [
    fx("eq", 1, { low: 3, mid: -5, high: -8 }),
    fx("drive", 0.24, { amount: 2.4 }),
    fx("reverb", 0.12, { seconds: 1.6, decay: 3.2 })
  ]);
  impacts.synth.decay = 0.32;
  impacts.synth.brightness = 0.46;

  // Helper functions
  const note = (t: Track, pitch: number, beat: number, duration: number, velocity: number) => {
    t.notes.push({ id: uid("note"), pitch, start: beat, duration, velocity });
  };

  const chord = (t: Track, pitches: number[], beat: number, duration: number, velocity: number) => {
    for (const p of pitches) note(t, p, beat, duration, velocity);
  };

  const kickAt = (bar: number, offset: number, vel: number) => note(kick, 36, bar * 4 + offset, 0.08, vel);
  const snareAt = (bar: number, offset: number, vel: number, main = true) => note(snare, main ? 38 : 39, bar * 4 + offset, main ? 0.06 : 0.04, vel);

  const isDrop = (bar: number) => (bar >= 16 && bar < 24) || (bar >= 32 && bar < 44);

  // ===== INTRO: EMBERS (0-8) =====
  for (let bar = 0; bar < 8; bar++) {
    const b = bar * 4;
    const h = harmony(bar);

    // Ambient intro with evolving pads
    if (bar >= 0) {
      chord(pad, h.chord.slice(0, 4), b, 3.8, 0.28 + bar * 0.04);
      chord(air, [h.chord[0] + 24, h.chord[2] + 24], b, 3.9, 0.22 + bar * 0.03);
    }

    if (bar >= 2) {
      const contour = bar % 2 ? 3 : 0;
      note(pluck, h.arp[contour], b, 0.6, 0.52);
      note(pluck, h.arp[contour + 1], b + 0.5, 0.45, 0.48);
      note(pluck, h.arp[contour + 2], b + 2.25, 0.3, 0.44);
      if (bar % 2) note(pluck, h.arp[5], b + 3.25, 0.2, 0.38);
    }

    if (bar >= 3) {
      for (const offset of bar % 3 === 0 ? [0.75, 3.5] : bar % 3 === 1 ? [1.25] : [2.5]) {
        note(stars, h.arp[(bar + Math.floor(offset)) % 6] + 12, b + offset, 0.4, 0.24 + (bar % 3) * 0.08);
      }
    }

    if (bar >= 5) {
      note(sub, h.root - 12, b, 3.2, 0.32 + (bar - 5) * 0.08);
    }
  }

  // ===== BUILD: ASCENT (8-16) =====
  for (let bar = 8; bar < 16; bar++) {
    const b = bar * 4;
    const h = harmony(bar);
    const progress = (bar - 8) / 8;

    // Growing string layer
    chord(strings, h.chord.slice(0, 4), b, 3.6, 0.36 + progress * 0.28);

    // Arp 16ths
    const steps = bar < 10 ? 8 : 16;
    const sequence = bar % 2 ? [0, 2, 4, 1, 3, 5, 2, 4] : [0, 1, 3, 2, 4, 1, 5, 3];
    for (let i = 0; i < steps; i++) {
      const offset = i * 4 / steps;
      if (bar === 15 && offset >= 3.5) continue;
      note(arp, h.arp[sequence[i % 8]], b + offset, 0.16, (0.42 + progress * 0.28) * (i % 4 === 0 ? 1 : 0.76));
    }

    // Pad layer
    if (bar >= 10) {
      chord(pad, h.chord.slice(1, 4), b, 3.7, 0.32 + (bar - 10) * 0.05);
    }

    // Growing drums
    if (bar >= 8) {
      for (let i = 0; i < 4; i++) {
        if (bar === 15 && i === 3) continue;
        kickAt(bar, i, 0.54 + progress * 0.26);
      }
    }

    if (bar >= 10) {
      if (bar < 14) {
        snareAt(bar, 1, 0.38);
        snareAt(bar, 3, 0.44);
      } else {
        const interval = bar === 15 ? 0.125 : 0.25;
        for (let at = 0; at < (bar === 15 ? 3.5 : 4); at += interval) {
          note(snare, 38, b + at, 0.04, (0.24 + progress * 0.32) * (Math.round(at / interval) % 4 === 0 ? 1 : 0.7));
        }
      }
    }

    if (bar >= 12) {
      for (let i = 0; i < 8; i++) {
        if (i % 2 || progress > 0.65) {
          note(hats, i % 4 === 3 ? 46 : 42, b + i / 2, 0.09, 0.28 + progress * 0.14);
        }
      }
    }

    // Riser in build climax
    if (bar >= 14) {
      note(rise, 62, b, bar === 15 ? 3.5 : 4, 0.55 + (bar - 14) * 0.2);
    }
  }

  // ===== DROP 1: FIRST LIGHT (16-24) =====
  const melodies: number[][][] = [
    [[0, 77, 0.7, 0.94], [0.75, 76, 0.2, 0.78], [1, 74, 0.8, 0.92], [2, 69, 0.4, 0.76], [2.75, 72, 0.2, 0.8], [3, 74, 0.85, 0.96]],
    [[0, 77, 0.4, 0.92], [0.75, 81, 0.9, 0.96], [2, 79, 0.4, 0.84], [2.5, 77, 0.25, 0.76], [3, 76, 0.35, 0.86], [3.5, 74, 0.4, 0.93]],
    [[0, 72, 0.9, 0.92], [1.25, 69, 0.4, 0.74], [2, 72, 0.4, 0.88], [2.75, 76, 0.95, 0.96]],
    [[0.5, 74, 0.4, 0.88], [1, 72, 0.65, 0.84], [2, 67, 0.75, 0.78], [3, 69, 0.3, 0.82], [3.5, 72, 0.3, 0.85]],
    [[0, 81, 0.65, 0.97], [0.75, 79, 0.2, 0.78], [1, 77, 0.4, 0.89], [1.75, 76, 0.25, 0.82], [2.25, 74, 0.9, 0.97], [3.5, 77, 0.3, 0.92]],
    [[0, 79, 0.7, 0.93], [1, 81, 0.9, 0.98], [2.25, 84, 0.65, 0.94], [3.25, 81, 0.5, 0.86]],
    [[0, 79, 0.7, 0.95], [1, 77, 0.35, 0.87], [1.5, 74, 0.4, 0.8], [2.25, 70, 0.4, 0.8], [3, 69, 0.3, 0.85], [3.5, 67, 0.3, 0.76]],
    [[0, 69, 0.9, 0.84], [1.5, 73, 0.35, 0.94], [2, 76, 0.45, 0.9], [2.75, 73, 0.45, 0.83]]
  ];

  const dropKickPatterns = [
    [0, 1.5, 3.25],
    [0, 0.75, 3.5],
    [0, 1.25, 2.75],
    [0, 1.75, 3.25, 3.75],
    [0, 1.5, 3],
    [0, 0.75, 1.5, 3.5],
    [0, 1.25, 3.25],
    [0, 1.5, 2.75]
  ];

  const stabs = [
    [[0, 1.2], [1.5, 0.35], [2.25, 0.45], [3, 0.65]],
    [[0, 0.65], [0.75, 0.55], [1.5, 0.3], [2.5, 0.75], [3.5, 0.3]],
    [[0, 1.65], [2, 0.35], [2.75, 1]],
    [[0, 0.8], [1.25, 0.55], [2.5, 0.45], [3.25, 0.35]]
  ];

  for (let bar = 16; bar < 24; bar++) {
    const b = bar * 4;
    const h = harmony(bar);
    const local = bar - 16;
    const phrase = melodies[local];
    const fillBar = [19, 23].includes(bar);

    // Powerful kick pattern
    const kicks = dropKickPatterns[local % 8];
    for (const at of kicks) kickAt(bar, at, at === 0 ? 0.98 : 0.88);
    if (local % 2 === 0) note(impacts, 41, b + 0.02, 0.28, 0.72);

    // Snare on 2
    snareAt(bar, 2, 0.92);
    if (local % 3 === 2) snareAt(bar, 3.75, 0.24, false);

    // Hi-hats 8ths/16ths
    for (let step = 0; step < 16; step++) {
      if (step % 2 && !(step === 7 || step === 15 || local % 3 === 1 && step === 11)) continue;
      if (fillBar && step >= 12) continue;
      note(hats, step % 8 === 6 ? 46 : 42, b + step / 4, 0.07, (step % 4 === 0 ? 0.46 : 0.31) * (1 - (local % 3) * 0.05));
    }

    // Fill toms
    if (fillBar) {
      for (const [at, p] of [[3, 45], [3.25, 43], [3.5, 41], [3.75, 48]]) {
        note(fills, p as number, b + (at as number), 0.14, 0.76);
      }
    }

    // Wide supersaw chords
    const stabPattern = stabs[local % 4];
    for (const [at, dur] of stabPattern) {
      chord(chords, h.chord, b + (at as number), dur as number, 0.88);
      chord(halo, h.chord.map(p => p + 12), b + (at as number), (dur as number) + 0.2, 0.42);
    }

    // Sub bass
    note(sub, h.root - 12, b, 3.9, 0.82);

    // Mid bass movement
    if (local % 2 === 0) {
      note(reese, h.root, b, 1.8, 0.76);
      note(reese, h.root + 7, b + 2, 1.9, 0.72);
    } else {
      note(reese, h.root, b, 0.9, 0.74);
      note(reese, h.root + 5, b + 1, 0.9, 0.72);
      note(reese, h.root, b + 2, 1.9, 0.76);
    }

    // Growl bass accent
    if (local % 4 === 1 || local % 4 === 3) {
      note(growl, h.root - 12, b + 2.5, 0.6, 0.84);
    }

    // Lead melody
    for (const [at, p, dur, vel] of phrase) {
      note(lead, p as number, b + (at as number), dur as number, vel as number);
    }

    // Counter melody
    if (local >= 4 && local < 8 && local % 2 === 0) {
      const counterPhrase = local === 4 ? [[0.5, 65], [1.25, 67], [2.5, 69]] : [[0.5, 67], [1.5, 69], [2.75, 72]];
      for (const [at, p] of counterPhrase) {
        note(counter, p as number, b + (at as number), 0.5, 0.68);
      }
    }
  }

  // ===== BREAKDOWN: WEIGHTLESS (24-28) =====
  for (let bar = 24; bar < 28; bar++) {
    const b = bar * 4;
    const h = harmony(bar);

    chord(pad, h.chord.slice(0, 4), b, 3.8, 0.42);
    chord(strings, h.chord.slice(1, 5), b, 3.7, 0.38);

    // Sparse plucks
    const pluckPattern = bar % 4 === 0 ? [0, 1.5, 3] : bar % 4 === 1 ? [0.5, 2.5] : bar % 4 === 2 ? [0, 2, 3.5] : [1, 2.75];
    for (const at of pluckPattern) {
      note(pluck, h.arp[(Math.floor(at) + bar) % 6], b + at, 0.5, 0.48);
    }

    // Gentle kick
    if (bar % 2 === 0) {
      kickAt(bar, 0, 0.42);
      kickAt(bar, 2, 0.38);
    }
  }

  // ===== BUILD 2: IGNITION (28-32) =====
  for (let bar = 28; bar < 32; bar++) {
    const b = bar * 4;
    const h = harmony(bar);
    const progress = (bar - 28) / 4;

    chord(strings, h.chord.slice(0, 4), b, 3.6, 0.42 + progress * 0.24);
    chord(pad, h.chord.slice(1, 4), b, 3.7, 0.38 + progress * 0.18);

    // Arp building
    const steps = 16;
    const sequence = [0, 2, 4, 1, 3, 5, 2, 4];
    for (let i = 0; i < steps; i++) {
      const offset = i / 4;
      if (bar === 31 && offset >= 3.5) continue;
      note(arp, h.arp[sequence[i % 8]], b + offset, 0.16, (0.48 + progress * 0.26) * (i % 4 === 0 ? 1 : 0.74));
    }

    // Growing drums
    for (let i = 0; i < 4; i++) {
      if (bar === 31 && i === 3) continue;
      kickAt(bar, i, 0.58 + progress * 0.28);
    }

    if (bar >= 30) {
      const interval = bar === 31 ? 0.125 : 0.25;
      for (let at = 0; at < (bar === 31 ? 3.5 : 4); at += interval) {
        note(snare, 38, b + at, 0.04, (0.28 + progress * 0.38) * (Math.round(at / interval) % 4 === 0 ? 1 : 0.68));
      }
    } else {
      snareAt(bar, 1, 0.42);
      snareAt(bar, 3, 0.46);
    }

    if (bar >= 29) {
      for (let i = 0; i < 8; i++) {
        if (i % 2 || progress > 0.5) {
          note(hats, i % 4 === 3 ? 46 : 42, b + i / 2, 0.09, 0.32 + progress * 0.16);
        }
      }
    }

    // Riser climax
    if (bar >= 30) {
      note(rise, 62, b, bar === 31 ? 3.5 : 4, 0.62 + (bar - 30) * 0.22);
    }
  }

  // ===== DROP 2: SUPERNOVA (32-44) =====
  const reprise: number[][][] = [
    melodies[0],
    [[0, 77, 0.4, 0.92], [0.75, 81, 0.65, 0.98], [1.5, 79, 0.25, 0.84], [2, 77, 0.4, 0.88], [2.75, 76, 0.2, 0.8], [3, 74, 0.75, 0.95]],
    melodies[2],
    melodies[3],
    melodies[4],
    melodies[5],
    melodies[6],
    [[0, 74, 0.7, 0.91], [1, 72, 0.4, 0.86], [1.75, 79, 0.5, 0.93], [2.5, 77, 0.3, 0.85], [3, 74, 0.75, 0.93]],
    [[0, 82, 0.7, 0.97], [0.75, 81, 0.2, 0.82], [1, 79, 0.75, 0.96], [2, 77, 0.4, 0.88], [2.75, 74, 0.2, 0.84], [3, 79, 0.8, 0.99]],
    [[0, 81, 0.5, 0.98], [0.75, 77, 0.75, 0.92], [1.75, 74, 0.4, 0.85], [2.5, 72, 0.35, 0.83], [3, 77, 0.75, 0.97]],
    [[0, 76, 0.75, 0.95], [1, 73, 0.4, 0.91], [1.75, 69, 0.35, 0.83], [2.25, 73, 0.4, 0.92], [3, 76, 0.3, 0.9], [3.5, 73, 0.3, 0.97]],
    melodies[0]
  ];

  for (let bar = 32; bar < 44; bar++) {
    const b = bar * 4;
    const h = harmony(bar);
    const local = bar - 32;
    const phrase = reprise[local];
    const fillBar = [35, 39, 43].includes(bar);
    const final = bar === 43;

    // Kick pattern
    const kicks = dropKickPatterns[(local + 2) % 8];
    for (const at of kicks) {
      if (final && at >= 2.5) continue;
      kickAt(bar, at, at === 0 ? 0.98 : 0.9);
    }
    if (local % 2 === 0 && !final) note(impacts, 41, b + 0.02, 0.3, 0.78);

    // Snare
    if (!final || bar < 43) {
      snareAt(bar, 2, 0.96);
      if (local % 3 === 2) snareAt(bar, 3.75, 0.26, false);
    }

    // Hi-hats
    for (let step = 0; step < 16; step++) {
      if (step % 2 && !(step === 7 || step === 15 || local % 3 === 1 && step === 11)) continue;
      if (fillBar && step >= 12) continue;
      if (final && step >= 10) continue;
      note(hats, step % 8 === 6 ? 46 : 42, b + step / 4, 0.07, (step % 4 === 0 ? 0.48 : 0.33) * (1 - (local % 3) * 0.04));
    }

    // Fills
    if (fillBar && bar !== 43) {
      for (const [at, p] of [[3, 45], [3.25, 43], [3.5, 41], [3.75, 48]]) {
        note(fills, p as number, b + (at as number), 0.14, 0.78);
      }
    }

    // Chords
    const stabPattern = stabs[local % 4];
    for (const [at, dur] of stabPattern) {
      if (final && (at as number) >= 2) continue;
      chord(chords, h.chord, b + (at as number), dur as number, 0.92);
      chord(halo, h.chord.map(p => p + 12), b + (at as number), (dur as number) + 0.2, 0.46);
    }

    // Bass
    if (!final || bar < 43) {
      note(sub, h.root - 12, b, final ? 2.5 : 3.9, 0.86);
      
      if (local % 2 === 0) {
        note(reese, h.root, b, final ? 1.2 : 1.8, 0.78);
        if (!final) note(reese, h.root + 7, b + 2, 1.9, 0.74);
      } else {
        note(reese, h.root, b, 0.9, 0.76);
        note(reese, h.root + 5, b + 1, 0.9, 0.74);
        if (!final) note(reese, h.root, b + 2, 1.9, 0.78);
      }

      if (local % 4 === 1 || local % 4 === 3) {
        note(growl, h.root - 12, b + 2.5, 0.6, 0.88);
      }
    }

    // Lead melody
    for (const [at, p, dur, vel] of phrase) {
      if (final && (at as number) >= 2) continue;
      note(lead, p as number, b + (at as number), dur as number, vel as number);
    }

    // Counter melody
    if (local >= 4 && local < 11 && local % 2 === 0) {
      const counterPhrase = local % 4 === 0 ? [[0.5, 65], [1.25, 67], [2.5, 69]] : [[0.5, 67], [1.5, 69], [2.75, 72]];
      for (const [at, p] of counterPhrase) {
        if (final && (at as number) >= 2) continue;
        note(counter, p as number, b + (at as number), 0.5, 0.72);
      }
    }
  }

  // ===== OUTRO: AFTERGLOW (44-48) =====
  for (let bar = 44; bar < 48; bar++) {
    const b = bar * 4;
    const h = harmony(bar);
    const fade = 1 - (bar - 44) / 4;

    chord(pad, h.chord.slice(0, 4), b, 3.9, 0.38 * fade);
    chord(strings, h.chord.slice(1, 5), b, 3.8, 0.42 * fade);
    chord(air, [h.chord[0] + 24, h.chord[2] + 24], b, 3.9, 0.28 * fade);

    // Final plucks
    if (bar < 47) {
      const pluckPattern = [0, 1.5, 2.5, 3.5];
      for (const at of pluckPattern) {
        note(pluck, h.arp[(Math.floor(at) + bar) % 6], b + at, 0.6, 0.44 * fade);
      }
    }
  }

  return project;
}
