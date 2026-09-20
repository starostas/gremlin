#include <cuda_runtime.h>
#include <cstdint>
#include <cstdio>
#include <new>
struct Shape {int32_t x,y,rx,ry,kind,rotate;uint32_t color,alpha;};
struct State {uint32_t *target=nullptr,*canvas=nullptr;Shape *shapes=nullptr;unsigned long long *scores=nullptr;uint32_t pixels=0,width=0,capacity=0;char device[256];};
__device__ bool inside(Shape s,int x,int y){int64_t dx=x-s.x,dy=y-s.y;if(s.rotate){int64_t old=dx;dx+=dy;dy=old-dy;}dx=dx<0?-dx:dx;dy=dy<0?-dy:dy;if(s.kind==0)return dx*dx*s.ry*s.ry+dy*dy*s.rx*s.rx<=int64_t(s.rx)*s.rx*s.ry*s.ry;if(s.kind==1)return dx<=s.rx&&dy<=s.ry;return dx*s.ry+dy*s.rx<=s.rx*s.ry;}
__device__ uint32_t blend(uint32_t old,Shape s){uint32_t out=0;for(int shift=0;shift<=16;shift+=8){uint32_t a=(old>>shift)&255,b=(s.color>>shift)&255;out|=((a*(255-s.alpha)+b*s.alpha+127)/255)<<shift;}return out;}
__device__ int error(uint32_t a,uint32_t b){int e=0;for(int shift=0;shift<=16;shift+=8){int d=int((a>>shift)&255)-int((b>>shift)&255);e+=d*d;}return e;}
__global__ void score(const uint32_t *target,const uint32_t *canvas,const Shape *shapes,unsigned long long *scores,uint32_t pixels,uint32_t width){
 uint32_t candidate=blockIdx.x;Shape s=shapes[candidate];
 int radius_x=s.rotate?s.rx+s.ry:s.rx,radius_y=s.rotate?s.rx+s.ry:s.ry;
 int left=max(0,s.x-radius_x),right=min(int(width)-1,s.x+radius_x);
 int top=max(0,s.y-radius_y),bottom=min(int(width)-1,s.y+radius_y);
 int bw=right-left+1,bh=bottom-top+1;
 long long delta=0;
 for(uint32_t q=blockIdx.y*blockDim.x+threadIdx.x;q<uint32_t(bw*bh);q+=gridDim.y*blockDim.x){
  int x=left+q%bw,y=top+q/bw;uint32_t p=y*width+x;
  if(p<pixels&&inside(s,x,y)){uint32_t old=canvas[p];delta+=error(blend(old,s),target[p])-error(old,target[p]);}
 }
 __shared__ long long totals[256];totals[threadIdx.x]=delta;__syncthreads();for(int d=128;d;d>>=1){if(threadIdx.x<d)totals[threadIdx.x]+=totals[threadIdx.x+d];__syncthreads();}
 if(threadIdx.x==0)atomicAdd(scores+candidate,(unsigned long long)totals[0]);
}
__global__ void paint(uint32_t *canvas,Shape s,uint32_t pixels,uint32_t width){uint32_t p=blockIdx.x*blockDim.x+threadIdx.x;if(p<pixels&&inside(s,p%width,p/width))canvas[p]=blend(canvas[p],s);}
extern "C" void sculptor_destroy(State *s){if(!s)return;cudaFree(s->target);cudaFree(s->canvas);cudaFree(s->shapes);cudaFree(s->scores);delete s;}
#define CHECK(call) do{cudaError_t c=(call);if(c!=cudaSuccess){snprintf(err,1024,"%s: %s",#call,cudaGetErrorString(c));return int(c);}}while(0)
extern "C" int sculptor_create(const uint32_t *target,const uint32_t *canvas,uint32_t width,State **out,char *device,char *err){
 State *s=new(std::nothrow) State;if(!s){snprintf(err,1024,"host allocation failed");return -1;}*out=s;s->width=width;s->pixels=width*width;s->capacity=4096;
 cudaDeviceProp p{};CHECK(cudaGetDeviceProperties(&p,0));snprintf(device,256,"%s",p.name);
 CHECK(cudaMalloc((void**)&s->target,s->pixels*4));CHECK(cudaMalloc((void**)&s->canvas,s->pixels*4));CHECK(cudaMalloc((void**)&s->shapes,s->capacity*sizeof(Shape)));CHECK(cudaMalloc((void**)&s->scores,s->capacity*8));CHECK(cudaMemcpy(s->target,target,s->pixels*4,cudaMemcpyHostToDevice));CHECK(cudaMemcpy(s->canvas,canvas,s->pixels*4,cudaMemcpyHostToDevice));return 0;
}
extern "C" int sculptor_score(State *s,const Shape *shapes,uint32_t count,int64_t *scores,float *kernel_ms,char *err){
 if(count==0||count>s->capacity){snprintf(err,1024,"invalid candidate count");return -1;}
 CHECK(cudaMemcpy(s->shapes,shapes,count*sizeof(Shape),cudaMemcpyHostToDevice));CHECK(cudaMemset(s->scores,0,count*8));
 cudaEvent_t start=nullptr,end=nullptr;CHECK(cudaEventCreate(&start));cudaError_t status=cudaEventCreate(&end);if(status!=cudaSuccess){cudaEventDestroy(start);snprintf(err,1024,"event creation: %s",cudaGetErrorString(status));return int(status);}
 status=cudaEventRecord(start);if(status==cudaSuccess){score<<<dim3(count,64),256>>>(s->target,s->canvas,s->shapes,s->scores,s->pixels,s->width);status=cudaGetLastError();}if(status==cudaSuccess)status=cudaEventRecord(end);if(status==cudaSuccess)status=cudaEventSynchronize(end);if(status==cudaSuccess)status=cudaEventElapsedTime(kernel_ms,start,end);cudaEventDestroy(start);cudaEventDestroy(end);if(status!=cudaSuccess){snprintf(err,1024,"scoring: %s",cudaGetErrorString(status));return int(status);}
 CHECK(cudaMemcpy(scores,s->scores,count*8,cudaMemcpyDeviceToHost));return 0;
}
extern "C" int sculptor_paint(State *s,Shape shape,char *err){paint<<<(s->pixels+255)/256,256>>>(s->canvas,shape,s->pixels,s->width);CHECK(cudaGetLastError());return 0;}
extern "C" int sculptor_read(State *s,uint32_t *canvas,char *err){CHECK(cudaMemcpy(canvas,s->canvas,s->pixels*4,cudaMemcpyDeviceToHost));return 0;}
