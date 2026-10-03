/* Fixed-memory, sample-free stereo DSP. No malloc, host imports, or audio-node creation.
 * ABI: write parameters into scratch(), then configure tracks/effects; render <=128 frames.
 * All sound generation AND all seven insert effects execute in WebAssembly.
 */
typedef unsigned int u32;
#define TRACKS 24
#define VOICES 64
#define FX 144
#define ARENA (8*1024*1024)
#define PI 3.14159265358979323846f
static float sr=48000, master=.72f, scratchpad[128], output[256];
static float arena[ARENA], sine[2049], frequencies[128];
static int used, trackCount, activeCount, droppedCount, maxActive;
static u32 randomState;
static float clamp(float x,float a,float b){return x<a?a:x>b?b:x;}
static float absf(float x){return x<0?-x:x;}
static float wrap(float x){return x-__builtin_floorf(x);}
static float sinp(float p){p=wrap(p)*2048;int i=(int)p;return sine[i]+(sine[i+1]-sine[i])*(p-i);}
static float exp2f_(float x){x=clamp(x,-32,24);int n=(int)__builtin_floorf(x);float f=(x-n)*.69314718056f;union{u32 i;float f;}v;v.i=(u32)(n+127)<<23;return v.f*(1+f*(1+f*(.5f+f*(.16666667f+f*(.04166667f+f*.00833333f)))));}
static float decayFactor(float seconds){return exp2f_(-1.442695f/(sr*seconds));}
static float noise(void){randomState^=randomState<<13;randomState^=randomState>>17;randomState^=randomState<<5;return (float)(randomState>>8)*(1.f/8388608.f)-1;}
static float sat(float x){x=clamp(x,-3,3);return x*(27+x*x)/(27+9*x*x);}
static float blep(float p,float dt){if(p<dt){p/=dt;return p+p-p*p-1;}if(p>1-dt){p=(p-1)/dt;return p*p+p+p+1;}return 0;}
static float osc(int type,float p,float dt){
 if(type==0)return sinp(p);
 if(type==1)return (8.f/(PI*PI))*(sinp(p)-sinp(3*p)/9+sinp(5*p)/25-sinp(7*p)/49);
 if(type==2)return 2*p-1-blep(p,dt);
 if(type==3)return (p<.5f?1:-1)+blep(p,dt)-blep(wrap(p+.5f),dt);
 if(type==4)return .72f*sinp(p)+.16f*sinp(3*p)+.08f*sinp(7*p)+.04f*sinp(11*p);
 return .85f*sinp(p)+.11f*sinp(3*p)+.04f*sinp(5*p);
}
typedef struct{float z1,z2;} BState;
typedef struct{int type,next,offset,size,pos;float mix,p[24],state[12],phase;BState b[6];} Effect;
typedef struct{float p[24],det[8],pl[4],pr[4],level,pan,cutoff,smLevel,smPan,smCutoff,peak,lp[2];int first,last;} Track;
typedef struct{int live,track,pitch,age,gate,release,attack,decay,total;float phase[8],inc[8],env,amp,releaseStart,lp[4],fm,decayMul,noiseLow,drumPhase;} Voice;
static Track tracks[TRACKS]; static Voice voices[VOICES];static Effect effects[FX]; static int fxCount;
static float busL[TRACKS][128],busR[TRACKS][128];
__attribute__((export_name("scratch"))) float* scratch(void){return scratchpad;}
__attribute__((export_name("output"))) float* get_output(void){return output;}
__attribute__((export_name("active"))) int active(void){return activeCount;}
__attribute__((export_name("dropped"))) int dropped(void){return droppedCount;}
__attribute__((export_name("max_active"))) int max_active(void){return maxActive;}
__attribute__((export_name("track_peak"))) float track_peak(int t){return t>=0&&t<trackCount?tracks[t].peak:0;}
static void clear(void* p,int bytes){unsigned char* c=p;for(int i=0;i<bytes;i++)c[i]=0;}
__attribute__((export_name("init"))) void init(float rate,float gain){
 sr=rate;master=gain;used=fxCount=trackCount=activeCount=droppedCount=maxActive=0;randomState=0x12345678;
 clear(tracks,sizeof(tracks));clear(voices,sizeof(voices));clear(effects,sizeof(effects));
 for(int i=0;i<=2048;i++){float x=(float)i/2048*2*PI; if(x>PI)x-=2*PI;float x2=x*x;sine[i]=x*(1+x2*(-1.f/6+x2*(1.f/120+x2*(-1.f/5040+x2*(1.f/362880+x2*(-1.f/39916800))))));}
 for(int i=0;i<128;i++)frequencies[i]=440*exp2f_((i-69)/12.f);
}
__attribute__((export_name("set_track"))) void set_track(int t){
 if(t<0||t>=TRACKS)return;Track* tr=&tracks[t];for(int i=0;i<24;i++)tr->p[i]=scratchpad[i];tr->first=tr->last=-1;
 tr->level=tr->smLevel=1;tr->pan=tr->smPan=tr->p[21];tr->cutoff=tr->smCutoff=18000;trackCount=t+1;
 int n=(int)tr->p[5];for(int u=0;u<n;u++){float sp=n==1?0:2.f*u/(n-1)-1;
 tr->det[u*2]=exp2f_(sp*tr->p[4]/1200);tr->det[u*2+1]=exp2f_((sp*tr->p[4]*.87f+(tr->p[4]>0?3:0))/1200)*(tr->p[0]==2?2:1);
 tr->pl[u]=__builtin_sqrtf((1-sp*tr->p[15])*.5f);tr->pr[u]=__builtin_sqrtf((1+sp*tr->p[15])*.5f);
 }
}
__attribute__((export_name("add_fx"))) int add_fx(int t,int type,float mix){
 if(t<0||t>=trackCount||fxCount>=FX)return -1;
 Effect* e=&effects[fxCount];e->type=type;e->mix=mix;e->next=-1;for(int i=0;i<24;i++)e->p[i]=scratchpad[i];
 int size=type==4?(int)e->p[2]:type==3?4096:type==5?16384:0; // 4 independent 4096-sample reverb lines
 if(used+size*2>ARENA)return -2;
 e->offset=used;e->size=size;clear(arena+used,size*2*sizeof(float));used+=size*2;
 if(tracks[t].last>=0)effects[tracks[t].last].next=fxCount;else tracks[t].first=fxCount;
 tracks[t].last=fxCount++;return 0;
}
__attribute__((export_name("automate"))) void automate(int t,float level,float pan,float cutoff){if(t<0||t>=trackCount)return;tracks[t].level=level;tracks[t].pan=pan;tracks[t].cutoff=cutoff;}
__attribute__((export_name("reset"))) void reset(void){clear(voices,sizeof(voices));activeCount=0;randomState=0x12345678;for(int t=0;t<trackCount;t++){tracks[t].lp[0]=tracks[t].lp[1]=0;tracks[t].smLevel=tracks[t].level;tracks[t].smPan=tracks[t].pan;tracks[t].smCutoff=tracks[t].cutoff;tracks[t].peak=0;}for(int f=0;f<fxCount;f++){Effect* e=&effects[f];clear(arena+e->offset,e->size*2*sizeof(float));clear(e->state,sizeof(e->state));clear(e->b,sizeof(e->b));e->pos=0;e->phase=0;}}
__attribute__((export_name("note"))) void note(int track,int pitch,float velocity,int duration){
 if(track<0||track>=trackCount||pitch<0||pitch>127)return;
 int slot=-1;for(int i=0;i<VOICES;i++)if(!voices[i].live){slot=i;break;}
 if(slot<0){droppedCount++;return;}
 Voice* v=&voices[slot];clear(v,sizeof(*v));v->live=1;v->track=track;v->pitch=pitch;v->gate=duration;
 Track* t=&tracks[track];float* p=t->p;v->attack=(int)(clamp(p[6]*sr,1,duration*.45f));v->decay=(int)(p[7]*sr);v->release=(int)(p[9]*sr);v->amp=velocity;
 v->decayMul=decayFactor(p[7]*.3f);v->total=duration+v->release;
 if(p[0]==3){float length=pitch==36||pitch==35?.22f+p[7]*.35f:pitch==38||pitch==40?.14f+p[7]:pitch==39?.18f:pitch==42||pitch==44?.065f:pitch==46?.28f:pitch==49||pitch==51?1.1f:.32f;v->total=(int)(length*sr);v->decayMul=decayFactor(length*.2f);v->env=1;}
 for(int u=0;u<(int)p[5]*2;u++){v->phase[u]=p[5]==1?0:wrap(.173f*u+.037f*pitch+.000001f*randomState);v->inc[u]=clamp(frequencies[pitch]/sr*t->det[u],.000001f,.45f);} 
 activeCount++;if(activeCount>maxActive)maxActive=activeCount;
}
static float biquad(float x,float* c,BState* s){float y=c[0]*x+s->z1;s->z1=c[1]*x-c[3]*y+s->z2;s->z2=c[2]*x-c[4]*y;return y;}
static void fx_sample(Effect* e,float* l,float* r){
 float a=*l,b=*r,dl=a,dr=b;float* p=e->p;float* d=arena+e->offset;
 if(e->type==0){for(int j=0;j<3;j++){dl=biquad(dl,p+j*5,&e->b[j*2]);dr=biquad(dr,p+j*5,&e->b[j*2+1]);}}
 else if(e->type==1){dl=biquad(a,p,&e->b[0]);dr=biquad(b,p,&e->b[1]);}
 else if(e->type==2){float g=p[0];dl=sat(a*g)/__builtin_sqrtf(g);dr=sat(b*g)/__builtin_sqrtf(g);}
 else if(e->type==3){ // Opposite-phase interpolated delay chorus, never creates oscillators.
  int pos=e->pos;d[pos]=a;d[4096+pos]=b;
  for(int c=0;c<2;c++){float delay=sr*(.012f+p[1]*sinp(e->phase+c*.5f));float at=pos-delay;if(at<0)at+=4096;int k=(int)at;float f=at-k;float value=d[c*4096+k]*(1-f)+d[c*4096+((k+1)&4095)]*f;if(c==0)dl=value;else dr=value;}
  e->pos=(pos+1)&4095;e->phase=wrap(e->phase+p[0]/sr);
 }else if(e->type==4){int pos=e->pos;int lag=(int)p[0];if(lag>=e->size)lag=e->size-1;int at=pos-lag;if(at<0)at+=e->size;
  dl=d[at];dr=d[e->size+at];float fb=p[1];d[pos]=a+(p[3]>.5f?dr:dl)*fb;d[e->size+pos]=b+(p[3]>.5f?dl:dr)*fb;e->pos=pos+1==e->size?0:pos+1;
 }else if(e->type==5){ // Four-line damped, orthogonal feedback delay network, stereo output.
  int pos=e->pos;float v[4];int lengths[4]={1499,1789,2131,2539};
  for(int j=0;j<4;j++){int len=(int)(lengths[j]*p[0]);len= (int)clamp(len,80,4095);int at=(pos-len)&4095;float x=d[j*4096+at];e->state[j]+=.32f*(x-e->state[j]);v[j]=e->state[j];}
  float sum=(v[0]+v[1]+v[2]+v[3])*.5f;
  for(int j=0;j<4;j++)d[j*4096+pos]=((j&1)?b:a)*.4f+(sum-v[j])*p[j+1];
  dl=(v[0]+v[2]-v[1])*.65f;dr=(v[1]+v[3]-v[2])*.65f;e->pos=(pos+1)&4095;
 }else if(e->type==6){float peak=absf(a)>absf(b)?absf(a):absf(b);float coef=peak>e->state[0]?p[2]:p[3];e->state[0]=peak+(e->state[0]-peak)*coef;float ratio=e->state[0]/p[0];float gain=ratio>1?1/(1+(ratio-1)*(1-1/p[1])):1;dl=a*gain;dr=b*gain;}
 *l=a*(1-e->mix)+dl*e->mix;*r=b*(1-e->mix)+dr*e->mix;
}
__attribute__((export_name("render"))) void render(int frames){
 if(frames<1||frames>128)return;
 for(int t=0;t<trackCount;t++){tracks[t].peak=0;for(int i=0;i<frames;i++)busL[t][i]=busR[t][i]=0;}
 for(int k=0;k<VOICES;k++){
  Voice* v=&voices[k];if(!v->live)continue;Track* tr=&tracks[v->track];float* p=tr->p;int engine=(int)p[0],count=(int)p[5];
  float pitchFactor=exp2f_(p[19]/12*clamp((float)v->age/(v->gate?v->gate:1),0,1));
  float cutoff=p[10]*exp2f_(p[16]*(1-clamp((float)v->age/(p[16]<0?v->gate:(v->decay?v->decay:1)),0,1)))+p[18]*1200*sinp((float)v->age/sr*p[17]);
  cutoff=clamp(cutoff,40,sr*.4f);float alpha=1-exp2f_(-9.06472f*cutoff/sr);float norm=.22f/__builtin_sqrtf((float)count);
  for(int i=0;i<frames;i++){
   if(v->age>=v->total){v->live=0;activeCount--;break;}float l=0,r=0;
   if(engine==3){
    float t=(float)v->age/sr;int pitch=v->pitch;float n=noise();v->noiseLow+=.12f*(n-v->noiseLow);float high=n-v->noiseLow;
    if(pitch==35||pitch==36){float hz=47+125*exp2f_(-t*70);v->drumPhase=wrap(v->drumPhase+hz/sr);l=sat(sinp(v->drumPhase)*1.8f)*v->env*.72f+high*.1f*exp2f_(-t*280);}
    else if(pitch==38||pitch==40){v->drumPhase=wrap(v->drumPhase+(175+80*exp2f_(-t*50))/sr);l=(sinp(v->drumPhase)*exp2f_(-t*35)*.55f+high*.65f)*v->env;}
    else if(pitch==39){float burst=t<.024f?(.3f+.7f*sinp(t*130)*sinp(t*130)):1;l=high*.7f*v->env*burst;}
    else if(pitch==42||pitch==44||pitch==46||pitch==49||pitch==51){l=high*v->env*.25f;}
    else{float hz=pitch==41?80:pitch==43?100:pitch==45?125:pitch==48?155:500;v->drumPhase=wrap(v->drumPhase+hz/sr*(1+.5f*exp2f_(-t*30)));l=sinp(v->drumPhase)*v->env*.5f+high*.03f*v->env;}
    v->env*=v->decayMul;l*=v->amp;r=l;
   }else{
    if(v->age<v->gate){if(v->age<v->attack)v->env=(float)v->age/(v->attack?v->attack:1);else if(v->age==v->attack)v->env=1;else v->env=p[8]+(v->env-p[8])*v->decayMul;v->releaseStart=v->env;}
    else v->env=v->releaseStart*clamp(1.f-(float)(v->age-v->gate)/(v->release?v->release:1),0,1);
    if(engine==4){float n=noise(),m=noise();v->lp[2]+=.05f*(n-v->lp[2]);v->lp[3]+=.05f*(m-v->lp[3]);v->phase[0]=wrap(v->phase[0]+v->inc[0]*pitchFactor);float tone=sinp(v->phase[0])*.05f;l=(n-v->lp[2])*.3f+tone;r=(m-v->lp[3])*.3f+tone;}
    else if(engine==1){float index=p[13]*(.12f+.88f*exp2f_(-(float)v->age/(sr*p[7])*4));v->fm=wrap(v->fm+v->inc[0]*p[12]*pitchFactor);v->phase[0]=wrap(v->phase[0]+v->inc[0]*pitchFactor);l=r=sinp(v->phase[0]+sinp(v->fm)*index*.15915494f)*.28f;}
    else for(int u=0;u<count;u++){int a=u*2,b=a+1;float da=clamp(v->inc[a]*pitchFactor,.000001f,.45f),db=clamp(v->inc[b]*pitchFactor,.000001f,.45f);v->phase[a]=wrap(v->phase[a]+da);v->phase[b]=wrap(v->phase[b]+db);float x=(osc((int)p[1],v->phase[a],da)*(1-p[3])+osc((int)p[2],v->phase[b],db)*p[3])*norm;l+=x*tr->pl[u];r+=x*tr->pr[u];}
    // Stable two-pole lowpass. Resonance is bounded feedback rather than an unstable high-Q cascade.
    v->lp[0]+=alpha*(l-v->lp[0]);v->lp[1]+=alpha*(r-v->lp[1]);
    l=v->lp[0]*v->env*v->amp;r=v->lp[1]*v->env*v->amp;
   }
   busL[v->track][i]+=l;busR[v->track][i]+=r;v->age++;
  }
 }
 for(int i=0;i<frames;i++)output[i]=output[128+i]=0;
 for(int t=0;t<trackCount;t++){
  Track* tr=&tracks[t];float dl=(tr->level-tr->smLevel)/frames,dp=(tr->pan-tr->smPan)/frames,dc=(tr->cutoff-tr->smCutoff)/frames;
  for(int i=0;i<frames;i++){
   tr->smLevel+=dl;tr->smPan+=dp;tr->smCutoff+=dc;float l=busL[t][i]*tr->p[20],r=busR[t][i]*tr->p[20];
   float alpha=clamp(tr->smCutoff/sr*4,0,1);tr->lp[0]+=alpha*(l-tr->lp[0]);tr->lp[1]+=alpha*(r-tr->lp[1]);l=tr->lp[0];r=tr->lp[1];
   for(int f=tr->first;f>=0;f=effects[f].next)fx_sample(&effects[f],&l,&r);
   float pan=tr->smPan;if(pan>0){r+=l*pan;l*=1-pan;}else{l-=r*pan;r*=1+pan;}
   l*=tr->smLevel;r*=tr->smLevel;float peak=absf(l)>absf(r)?absf(l):absf(r);if(peak>tr->peak)tr->peak=peak;output[i]+=l;output[128+i]+=r;
  }
 }
 for(int i=0;i<frames;i++){output[i]=sat(output[i]*master)*.92f;output[128+i]=sat(output[128+i]*master)*.92f;}
}
