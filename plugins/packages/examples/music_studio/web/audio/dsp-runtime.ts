import { type Project, type Effect, audible, eventBeat } from '../../src/shared/model';
import { automationValue } from '../../src/shared/automation';
export interface DspExports {
  memory: WebAssembly.Memory; scratch(): number; output(): number;
  init(rate: number, gain: number): void; set_track(t: number): void;
  add_fx(t: number, type: number, mix: number): number;
  automate(t: number, level: number, pan: number, cutoff: number): void;
  reset(): void; note(t: number, pitch: number, velocity: number, frames: number): void;
  render(frames: number): void; active(): number; dropped(): number; max_active(): number; track_peak(t: number): number;
}
interface Event { frame: number; duration: number; track: number; pitch: number; velocity: number }
const engines = ['spectral', 'fm', 'ensemble', 'drums', 'atmosphere'];
const waves = ['sine', 'triangle', 'sawtooth', 'square', 'glass', 'hollow'];
const effects = ['eq', 'filter', 'drive', 'chorus', 'delay', 'reverb', 'compressor'];
/** RBJ normalized biquads, coefficients calculated once off the audio render hot path. */
function biquad(type: string, frequency: number, gain: number, q: number, rate: number): number[] {
  const w = 2 * Math.PI * Math.min(rate * .45, frequency) / rate, c = Math.cos(w), s = Math.sin(w);
  const A = Math.pow(10, gain / 40), alpha = s / (2 * q), root = 2 * Math.sqrt(A) * alpha;
  let b0: number, b1: number, b2: number, a0: number, a1: number, a2: number;
  if (type === 'lowpass') { b0 = (1-c)/2; b1=1-c; b2=b0; a0=1+alpha; a1=-2*c; a2=1-alpha; }
  else if (type === 'low') { b0=A*((A+1)-(A-1)*c+root); b1=2*A*((A-1)-(A+1)*c); b2=A*((A+1)-(A-1)*c-root); a0=(A+1)+(A-1)*c+root; a1=-2*((A-1)+(A+1)*c); a2=(A+1)+(A-1)*c-root; }
  else if (type === 'high') { b0=A*((A+1)+(A-1)*c+root); b1=-2*A*((A-1)+(A+1)*c); b2=A*((A+1)+(A-1)*c-root); a0=(A+1)-(A-1)*c+root; a1=2*((A-1)-(A+1)*c); a2=(A+1)-(A-1)*c-root; }
  else { b0=1+alpha*A; b1=-2*c; b2=1-alpha*A; a0=1+alpha/A; a1=-2*c; a2=1-alpha/A; }
  return [b0/a0,b1/a0,b2/a0,a1/a0,a2/a0];
}
function effectParams(e: Effect, rate: number, bpm: number): number[] {
  const p=e.params;
  switch(e.type) {
    case 'eq': return [...biquad('low',250,p.low,.707,rate), ...biquad('mid',1000,p.mid,.7,rate), ...biquad('high',4000,p.high,.707,rate)];
    case 'filter': return biquad('lowpass',p.cutoff,0,p.resonance,rate);
    case 'drive': return [p.amount];
    case 'chorus': return [p.rate,p.depth];
    case 'delay': { const samples=Math.ceil(p.beats*60/bpm*rate); return [samples,p.feedback,samples+1,p.pingPong]; }
    case 'reverb': { const scale=Math.min(1.58,p.seconds/1.8*rate/44100); const rt=p.seconds*2.5/p.decay; return [scale,...[1499,1789,2131,2539].map(n=>Math.pow(.001,n*scale/rate/rt))]; }
    case 'compressor': return [Math.pow(10,p.threshold/20),p.ratio,Math.exp(-1/(rate*p.attack)),Math.exp(-1/(rate*p.release))];
  }
}
/** Shared scheduler for AudioWorklet and export Worker. Only bounded block DSP runs here. */
export class DspRuntime {
  readonly api: DspExports;
  readonly left: Float32Array;
  readonly right: Float32Array;
  readonly scratch: Float32Array;
  project!: Project;
  frame=0;
  playing=false;
  metronome=false;
  blocks=0;
  wraps=0;
  private rate: number;
  private events: Event[]=[];
  private index=0;
  private end=0;
  private loopStart=0;
  private loopEnd=0;
  private loop=false;
  private clickLeft=0;
  private clickPhase=0;
  private nextClick=0;
  private peaks=new Float32Array(24);
  constructor(module: WebAssembly.Module, rate: number) {
    this.api=new WebAssembly.Instance(module,{}).exports as unknown as DspExports;
    this.rate=rate;
    this.scratch=new Float32Array(this.api.memory.buffer,this.api.scratch(),128);
    this.left=new Float32Array(this.api.memory.buffer,this.api.output(),128);
    this.right=new Float32Array(this.api.memory.buffer,this.api.output()+128*4,128);
  }
  load(project: Project, allowLoop=true): void {
    this.project=project; this.playing=false;this.blocks=this.wraps=0;
    this.api.init(this.rate,project.masterGain);
    project.tracks.forEach((t,i)=>{
      const s=t.synth;
      this.scratch.fill(0);
      this.scratch.set([engines.indexOf(s.engine), waves.indexOf(s.wave), waves.indexOf(s.waveB), s.blend,s.detune,s.unison,s.attack,s.decay,s.sustain,s.release,s.cutoff,s.resonance,s.fmRatio,s.fmDepth,s.brightness,s.width,s.filterEnv,s.lfoRate,s.lfoDepth,s.pitchSweep,audible(t,project)?t.gain:0,t.pan]);
      this.api.set_track(i);
      for(const e of t.effects.filter(e=>e.enabled)) {
        this.scratch.fill(0);this.scratch.set(effectParams(e,this.rate,project.bpm));
        if(this.api.add_fx(i,effects.indexOf(e.type),e.mix)<0) throw Error('DSP 效果器固定内存不足，请减少长延迟效果器');
      }
    });
    const unit=60/project.bpm*this.rate;
    this.events=project.tracks.flatMap((t,track)=>audible(t,project)?t.notes.map(n=>({frame:Math.round(eventBeat(n,project.swing)*unit),duration:Math.max(1,Math.round(n.duration*unit)),track,pitch:n.pitch,velocity:n.velocity})):[]).sort((a,b)=>a.frame-b.frame||a.pitch-b.pitch);
    this.end=Math.round(project.bars*project.beatsPerBar*unit);
    this.loop=allowLoop&&project.loop.enabled;this.loopStart=Math.round(project.loop.start*unit);this.loopEnd=Math.round(project.loop.end*unit);
    this.seek(0);
  }
  seek(beat: number): void {
    let frame=Math.round(beat*60/this.project.bpm*this.rate);
    if(this.loop&&(frame<this.loopStart||frame>=this.loopEnd))frame=this.loopStart;
    this.frame=Math.max(0,Math.min(this.end,frame));this.index=0;this.clickLeft=0;
    this.updateAutomation();this.api.reset();
    while(this.index<this.events.length&&this.events[this.index].frame<this.frame)this.index++;
    this.nextClick=Math.ceil(this.frame/(60/this.project.bpm*this.rate));
  }
  get beat(): number {return this.frame/this.rate*this.project.bpm/60;}
  private updateAutomation(): void {
    const beat=this.beat;
    for(let i=0;i<this.project.tracks.length;i++) {
      const t=this.project.tracks[i];let level=1,pan=t.pan,cutoff=18000;
      for(const lane of t.automation){const v=automationValue(lane.points,beat);if(lane.target==='level')level=v;else if(lane.target==='pan')pan=v;else cutoff=v;}
      this.api.automate(i,level,pan,cutoff);
    }
  }
  /** Fills preallocated output buffers; no per-sample JS synthesis or object allocation. */
  process(left: Float32Array,right: Float32Array,tail=false): void {
    left.fill(0);right.fill(0);this.peaks.fill(0);
    if(!this.playing)return;
    let offset=0;
    while(offset<left.length) {
      const boundary=this.loop?this.loopEnd:this.end;
      if(this.frame>=boundary&&!tail){if(this.loop){this.wraps++;this.seek(this.project.loop.start);}else{this.playing=false;break;}}
      while(this.index<this.events.length&&this.events[this.index].frame<=this.frame) {
        const e=this.events[this.index++];
        if(e.frame<boundary)this.api.note(e.track,e.pitch,e.velocity,Math.min(e.duration,boundary-e.frame));
      }
      const next=this.index<this.events.length?this.events[this.index].frame:Infinity;
      let count=Math.min(128,left.length-offset,next-this.frame);
      if(!tail)count=Math.min(count,boundary-this.frame);
      if(count<=0)continue;
      this.updateAutomation();this.api.render(count);
      for(let t=0;t<this.project.tracks.length;t++)this.peaks[t]=Math.max(this.peaks[t],this.api.track_peak(t));
      for(let i=0;i<count;i++) {left[offset+i]=this.left[i];right[offset+i]=this.right[i];}
      // Metronome is optional UI timing audio; musical DSP is entirely WASM.
      if(this.metronome&&!tail)for(let i=0;i<count;i++) {
        if(this.frame+i>=this.nextClick*60/this.project.bpm*this.rate){this.clickLeft=Math.round(.02*this.rate);this.clickPhase=0;this.nextClick++;}
        if(this.clickLeft>0){const click=Math.sin(this.clickPhase)*.09*this.clickLeft/(.02*this.rate);this.clickPhase+=2*Math.PI*1200/this.rate;this.clickLeft--;left[offset+i]+=click;right[offset+i]+=click;}
      }
      offset+=count;this.frame+=count;
    }
    this.blocks++;
  }
  stats(): { backend: string; beat: number; playing: boolean; voices: number; dropped: number; maxVoices: number; memoryBytes: number; blocks: number; wraps: number; trackPeaks: number[] } {
    return {backend:'wasm-audio-worklet',beat:this.beat,playing:this.playing,voices:this.api.active(),dropped:this.api.dropped(),maxVoices:this.api.max_active(),memoryBytes:this.api.memory.buffer.byteLength,blocks:this.blocks,wraps:this.wraps,trackPeaks:Array.from(this.peaks.subarray(0,this.project.tracks.length))};
  }
}
