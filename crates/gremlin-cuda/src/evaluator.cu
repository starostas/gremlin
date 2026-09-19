#include <cuda_runtime.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <chrono>
#include <vector>
struct Instruction {uint32_t op,dst,a,b,c,width,result_width,pad;uint64_t literal;};
struct Block {uint32_t start,len,params_offset,params_len,term,a,yes,no;};
struct Edge {uint32_t target,args_offset,len;};
struct Program {uint64_t registers_offset;uint32_t registers,scratch,entry,argument_count;};
struct Result {uint64_t bits,steps;uint32_t status,reason;};
struct Metrics {double setup_ms,transfer_ms,kernel_ms,total_ms;int driver,runtime,major,minor;char device[256];};
__device__ uint64_t mask(uint32_t width){return UINT64_MAX>>(64-width);}
__device__ uint64_t negate(uint64_t a,uint64_t m){return (~a+1)&m;}
__global__ void evaluate(const Program *programs,const Block *blocks,const Instruction *instructions,const Edge *edges,const uint32_t *indices,const uint64_t *inputs,uint64_t *storage,Result *results,uint32_t cases,uint64_t budget){
 uint32_t program_index=blockIdx.x;uint32_t test=blockIdx.y*32+threadIdx.x;if(test>=cases)return;Program p=programs[program_index];uint64_t *values=storage+p.registers_offset+uint64_t(test)*(p.registers+p.scratch);uint64_t *scratch=values+p.registers;Result result{0,0,0,0};Result *output=results+uint64_t(program_index)*cases+test;
 for(uint32_t i=0;i<p.argument_count;++i)values[i]=inputs[uint64_t(test)*p.argument_count+i];uint32_t block_id=p.entry;
 for(;;){Block block=blocks[block_id];for(uint32_t pc=0;pc<block.len;++pc){if(result.steps==budget){result.status=1;*output=result;return;}++result.steps;Instruction i=instructions[block.start+pc];if(i.op==0){values[i.dst]=i.literal;continue;}uint64_t a=values[i.a],b=values[i.b],c=values[i.c],m=mask(i.width),sign=uint64_t(1)<<(i.width-1),r=0;uint32_t k=b%i.width;bool sa=(a&sign)!=0,sb=(b&sign)!=0;
 switch(i.op){
 case 1:r=a+b;break;case 2:r=a-b;break;case 3:r=a*b;break;
 case 4:case 5:case 6:case 7:{if(b==0){result.status=2;result.reason=1;*output=result;return;}if((i.op==5||i.op==7)&&a==sign&&b==m){result.status=2;result.reason=2;*output=result;return;}if(i.op==4)r=a/b;else if(i.op==6)r=a%b;else{uint64_t x=sa?negate(a,m):a,y=sb?negate(b,m):b;if(i.op==5){r=x/y;if(sa!=sb)r=negate(r,m);}else{r=x%y;if(sa)r=negate(r,m);}}break;}
 case 8:r=a&b;break;case 9:r=a|b;break;case 10:r=a^b;break;case 11:r=~a;break;
 case 12:r=a<<k;break;case 13:r=a>>k;break;case 14:r=(a>>k)|(sa?(m^(m>>k)):0);break;case 15:r=k==0?a:((a<<k)|(a>>(i.width-k)));break;case 16:r=k==0?a:((a>>k)|(a<<(i.width-k)));break;
 case 17:r=a==b;break;case 18:r=a!=b;break;case 19:r=a<b;break;case 20:r=a<=b;break;case 21:r=a>b;break;case 22:r=a>=b;break;
 case 23:r=(a^sign)<(b^sign);break;case 24:r=(a^sign)<=(b^sign);break;case 25:r=(a^sign)>(b^sign);break;case 26:r=(a^sign)>=(b^sign);break;case 27:r=a!=0?b:c;break;default:result.status=3;*output=result;return;
 }values[i.dst]=r&mask(i.result_width);}
 if(result.steps==budget){result.status=1;*output=result;return;}++result.steps;if(block.term==0){result.bits=values[block.a];*output=result;return;}Edge edge=edges[block.term==1?block.yes:(values[block.a]!=0?block.yes:block.no)];for(uint32_t j=0;j<edge.len;++j)scratch[j]=values[indices[edge.args_offset+j]];Block destination=blocks[edge.target];for(uint32_t j=0;j<edge.len;++j)values[indices[destination.params_offset+j]]=scratch[j];block_id=edge.target;
 }
}
struct Allocations{std::vector<void*> pointers;~Allocations(){for(void *p:pointers)cudaFree(p);}};
using Clock=std::chrono::steady_clock;
static double milliseconds(Clock::time_point start){return std::chrono::duration<double,std::milli>(Clock::now()-start).count();}
extern "C" int gremlin_cuda_evaluate(const Program *programs,size_t program_count,const Block *blocks,size_t block_count,const Instruction *instructions,size_t instruction_count,const Edge *edges,size_t edge_count,const uint32_t *indices,size_t index_count,const uint64_t *inputs,size_t input_count,uint64_t register_count,uint32_t cases,uint64_t budget,Result *results,Metrics *metrics,char *error,size_t error_size){
 auto start=Clock::now();Allocations allocations;cudaError_t status;cudaDeviceProp properties{};
 #define CHECK(expression) do{status=(expression);if(status!=cudaSuccess){snprintf(error,error_size,"%s: %s",#expression,cudaGetErrorString(status));return int(status);}}while(0)
 CHECK(cudaGetDeviceProperties(&properties,0));CHECK(cudaDriverGetVersion(&metrics->driver));CHECK(cudaRuntimeGetVersion(&metrics->runtime));metrics->major=properties.major;metrics->minor=properties.minor;snprintf(metrics->device,sizeof(metrics->device),"%s",properties.name);
 auto copy=[&](const void *source,size_t size,void **target)->cudaError_t{if(size==0){*target=nullptr;return cudaSuccess;}cudaError_t result=cudaMalloc(target,size);if(result!=cudaSuccess)return result;allocations.pointers.push_back(*target);if(source)return cudaMemcpy(*target,source,size,cudaMemcpyHostToDevice);return cudaSuccess;};
 Program *dp;Block *db;Instruction *di;Edge *de;uint32_t *dx;uint64_t *da,*dr;Result *dresults;metrics->setup_ms=milliseconds(start);auto transfer_start=Clock::now();
 CHECK(copy(programs,program_count*sizeof(Program),(void**)&dp));CHECK(copy(blocks,block_count*sizeof(Block),(void**)&db));CHECK(copy(instructions,instruction_count*sizeof(Instruction),(void**)&di));CHECK(copy(edges,edge_count*sizeof(Edge),(void**)&de));CHECK(copy(indices,index_count*sizeof(uint32_t),(void**)&dx));CHECK(copy(inputs,input_count*sizeof(uint64_t),(void**)&da));CHECK(copy(nullptr,register_count*sizeof(uint64_t),(void**)&dr));CHECK(copy(nullptr,program_count*cases*sizeof(Result),(void**)&dresults));metrics->transfer_ms=milliseconds(transfer_start);
 auto kernel_start=Clock::now();evaluate<<<dim3(program_count,(cases+31)/32),32>>>(dp,db,di,de,dx,da,dr,dresults,cases,budget);CHECK(cudaGetLastError());CHECK(cudaDeviceSynchronize());metrics->kernel_ms=milliseconds(kernel_start);auto back=Clock::now();CHECK(cudaMemcpy(results,dresults,program_count*cases*sizeof(Result),cudaMemcpyDeviceToHost));metrics->transfer_ms+=milliseconds(back);metrics->total_ms=milliseconds(start);return 0;
 #undef CHECK
}
