import { DspRuntime } from './dsp-runtime';
declare const sampleRate: number;
declare class AudioWorkletProcessor { readonly port: MessagePort; constructor(options?: unknown); }
declare function registerProcessor(name: string, processor: typeof AudioWorkletProcessor): void;
class StudioProcessor extends AudioWorkletProcessor {
  private runtime: DspRuntime;
  private ticks=0;
  constructor(options: {processorOptions: {module: WebAssembly.Module}}) {
    super();this.runtime=new DspRuntime(options.processorOptions.module,sampleRate);
    this.port.onmessage=({data})=>{
      try {
        if(data.type==='load')this.runtime.load(data.project);
        else if(data.type==='play'){this.runtime.seek(data.beat);this.runtime.playing=true;}
        else if(data.type==='pause'){this.runtime.playing=false;}
        else if(data.type==='stop'){this.runtime.playing=false;this.runtime.seek(data.beat??0);}
        else if(data.type==='seek')this.runtime.seek(data.beat);
        else if(data.type==='metronome')this.runtime.metronome=data.value;
        if(data.id)this.port.postMessage({id:data.id,ok:true});
      }catch(e){this.port.postMessage({id:data.id,error:String(e)});this.runtime.playing=false;}
    };
    this.port.postMessage({ready:true});
  }
  process(_inputs: Float32Array[][],outputs: Float32Array[][]): boolean {
    const output=outputs[0];if(!output?.[0]||!output[1])return true;
    try {
      this.runtime.process(output[0],output[1]);
      if(this.runtime.project&&++this.ticks>=20){this.ticks=0;this.port.postMessage({stats:this.runtime.stats()});}
    }catch(e){output[0].fill(0);output[1].fill(0);this.runtime.playing=false;this.port.postMessage({error:String(e)});}
    return true;
  }
}
registerProcessor('operit-wasm-studio',StudioProcessor);
