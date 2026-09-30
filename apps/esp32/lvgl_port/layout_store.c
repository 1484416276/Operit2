#include "layout_store.h"
#include <string.h>
#include <stdatomic.h>
#include <stdlib.h>
#ifndef __EMSCRIPTEN__
#include "esp_partition.h"
#include "esp_spi_flash.h"
#endif
#define SLOT 32768
#define LIMIT 28672
static const uint8_t *mapped, *active;
static atomic_int active_slot=-1;
static int writing_slot=-1;
static size_t expected,written;
static uint8_t header[16];
static atomic_int pending=-1;
static atomic_uint revision;
#ifndef __EMSCRIPTEN__
static const esp_partition_t *partition;
static esp_partition_mmap_handle_t map_handle;
#endif
static uint16_t r16(const uint8_t *p){return p[0]|((uint16_t)p[1]<<8);}
static uint32_t r32(const uint8_t *p){return r16(p)|((uint32_t)r16(p+2)<<16);}
static uint32_t crc32(const uint8_t *p,size_t n){uint32_t c=~0u;while(n--){c^=*p++;for(int i=0;i<8;i++)c=(c>>1)^((c&1)?0xedb88320:0);}return ~c;}
static const uint8_t *string(const uint8_t *p,char *out){unsigned n=*p++;memcpy(out,p,n);out[n]=0;return p+n;}
static const uint8_t *page_header(const uint8_t *p,operit_packed_page_t *o){p=string(p,o->id);p=string(p,o->left);p=string(p,o->right);o->color=r32(p);p+=4;o->count=*p++;o->nodes=p;return p;}
const uint8_t *operit_store_node(const uint8_t *p,operit_packed_node_t *n,char *strings){
 n->type=*p++;n->parent=*p++;n->x=r16(p);p+=2;n->y=r16(p);p+=2;n->w=r16(p);p+=2;n->h=r16(p);p+=2;n->color=r32(p);p+=4;n->radius=*p++;n->value=*p++;n->font=*p++;
 n->text=strings;p=string(p,strings);strings+=161;n->action=strings;p=string(p,strings);strings+=24;n->hold=strings;p=string(p,strings);strings+=24;n->binding=strings;return string(p,strings);
}
static bool checked_string(const uint8_t **p,const uint8_t *end,unsigned max){if(*p>=end)return false;unsigned n=*(*p)++;if(n>max||(size_t)(end-*p)<n)return false;for(unsigned i=0;i<n;i++)if(((*p)[i]<32&&(*p)[i]!='\n')||(*p)[i]>126)return false;*p+=n;return true;}
typedef struct {
 char ids[12][24],targets[12][2][24];
 uint16_t widths[24],heights[24];
 uint8_t types[24];
} layout_validation_workspace_t;
static bool valid(const uint8_t *data,const uint8_t *head){
 if(memcmp(head,"OUI2",4)||r32(head+12)!=1)return false;unsigned length=r32(head+4);if(length<6||length>LIMIT-16||crc32(data,length)!=r32(head+8))return false;
 layout_validation_workspace_t *workspace = calloc(1, sizeof(*workspace));
 if (!workspace) return false;
 const uint8_t *p=data,*end=data+length;unsigned pages=*p++,entry=*p++;if(!pages||pages>12||entry>=pages||r16(p)!=320||r16(p+2)!=240)goto invalid;p+=4;
 char (*ids)[24]=workspace->ids;char (*targets)[2][24]=workspace->targets;
 for(unsigned i=0;i<pages;i++){
  const uint8_t *start=p;for(unsigned k=0;k<3;k++)if(!checked_string(&p,end,18))goto invalid;
  string(start,ids[i]);if(!*ids[i])goto invalid;for(unsigned j=0;j<i;j++)if(!strcmp(ids[j],ids[i]))goto invalid;
  start+=1+*start;start=string(start,targets[i][0]);string(start,targets[i][1]);
  if(end-p<5)goto invalid;p+=4;unsigned count=*p++,cost=0;if(count>24)goto invalid;
  uint16_t *widths=workspace->widths,*heights=workspace->heights;uint8_t *types=workspace->types;
  for(unsigned j=0;j<count;j++){
   if(end-p<17)goto invalid;unsigned type=p[0],parent=p[1],x=r16(p+2),y=r16(p+4),w=r16(p+6),h=r16(p+8);
   if(type>28||w<8||h<8||p[14]>120||p[15]>100||(p[16]!=14&&p[16]!=48))goto invalid;
   if(parent!=255&&(parent>=j||types[parent]!=0))goto invalid;
   if(x+w>(parent==255?320:widths[parent])||y+h>(parent==255?240:heights[parent]))goto invalid;
   widths[j]=w;heights[j]=h;types[j]=type;cost+=(type==20||type==21||type==22||type==23||type>=26)?5:1;p+=17;
   if(!checked_string(&p,end,160)||!checked_string(&p,end,23)||!checked_string(&p,end,23)||!checked_string(&p,end,16))goto invalid;
  }
  if(cost>40)goto invalid;
 }
 if(p!=end)goto invalid;
 for(unsigned i=0;i<pages;i++)for(unsigned k=0;k<2;k++)if(*targets[i][k]){bool found=false;for(unsigned j=0;j<pages;j++)if(!strcmp(targets[i][k],ids[j]))found=true;if(!found)goto invalid;}
 free(workspace);return true;
invalid:
 free(workspace);return false;
}
bool operit_store_validate(const uint8_t *data,size_t length){return length>=22&&length<=LIMIT&&r32(data+4)+16==length&&valid(data+16,data);}
static const uint8_t *skip_nodes(const uint8_t *p,unsigned count){while(count--){p+=17;for(int i=0;i<4;i++)p+=1+*p;}return p;}
const char *operit_store_entry(void){static char id[24];if(!active)return NULL;const uint8_t *p=active+6;for(unsigned i=0;i<=active[1];i++){operit_packed_page_t page;p=page_header(p,&page);if(i==active[1]){strcpy(id,page.id);return id;}p=skip_nodes(p,page.count);}return NULL;}
bool operit_store_page(const char *id,operit_packed_page_t *page){if(!active)return false;const uint8_t *p=active+6;for(unsigned i=0;i<active[0];i++){p=page_header(p,page);if(!strcmp(page->id,id))return true;p=skip_nodes(p,page->count);}return false;}
void operit_store_init(void){
#ifndef __EMSCRIPTEN__
 partition=esp_partition_find_first(ESP_PARTITION_TYPE_DATA,ESP_PARTITION_SUBTYPE_DATA_SPIFFS,"ui_layout");if(!partition||partition->size<2*SLOT)return;
 if(esp_partition_mmap(partition,0,2*SLOT,ESP_PARTITION_MMAP_DATA,(const void**)&mapped,&map_handle)!=ESP_OK){mapped=NULL;return;}
 /* Generation is stored outside the checked packet. */
 for(int i=0;i<2;i++){const uint8_t *slot=mapped+i*SLOT;if(valid(slot+16,slot)){unsigned gen=r32(slot+SLOT-4);if(active_slot<0||gen>atomic_load(&revision)){active_slot=i;active=slot+16;atomic_store(&revision,gen);}}}
#endif
}
bool operit_store_poll(void){int slot=atomic_load(&pending);if(slot<0)return false;active_slot=slot;active=mapped+slot*SLOT+16;atomic_store(&revision,r32(mapped+slot*SLOT+SLOT-4));atomic_store(&pending,-1);return true;}
unsigned operit_store_revision(void){return atomic_load(&revision);}
int operit_store_begin(size_t length){
#ifndef __EMSCRIPTEN__
 if(!mapped||writing_slot>=0||atomic_load(&pending)>=0||length<22||length>LIMIT)return -1;
 writing_slot=active_slot==0?1:0;expected=length;written=0;
 if(esp_partition_erase_range(partition,writing_slot*SLOT,SLOT)!=ESP_OK){writing_slot=-1;return -1;}return 0;
#else
 (void)length;return -1;
#endif
}
int operit_store_write(const uint8_t *data,size_t length){
#ifndef __EMSCRIPTEN__
 if(writing_slot<0||written+length>expected)return -1;
 if(written<16){size_t n=16-written;if(n>length)n=length;memcpy(header+written,data,n);written+=n;data+=n;length-=n;}
 if(length&&esp_partition_write(partition,writing_slot*SLOT+written,data,length)!=ESP_OK)return -1;written+=length;return 0;
#else
 (void)data;(void)length;return -1;
#endif
}
void operit_store_abort(void){writing_slot=-1;}
int operit_store_finish(void){
#ifndef __EMSCRIPTEN__
 if(writing_slot<0||written!=expected||r32(header+4)+16!=expected||!valid(mapped+writing_slot*SLOT+16,header)){operit_store_abort();return -1;}
 unsigned gen=atomic_load(&revision)+1;int slot=writing_slot;
 if(esp_partition_write(partition,slot*SLOT+SLOT-4,&gen,4)!=ESP_OK||esp_partition_write(partition,slot*SLOT,header,16)!=ESP_OK){operit_store_abort();return -1;}
 writing_slot=-1;atomic_store(&pending,slot);return 0;
#else
 return -1;
#endif
}
