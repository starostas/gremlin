#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <dlfcn.h>
#include <vector>
using Fn=int64_t(*)(int64_t,int64_t);
constexpr int64_t Q=1LL<<28, PI=843314857;
double tolerance;
bool halley_mode=false;
__attribute__((noinline)) int64_t reference(int64_t mi,int64_t ei){double m=double(mi)/Q,e=double(ei)/Q,x=std::min(double(PI)/Q,m+.85*e),lo=0,hi=double(PI)/Q;for(int k=0;k<32;k++){double s=std::sin(x),c=std::cos(x),f=x-e*s-m,d=1-e*c;if(std::abs(f)<=tolerance*(1-e)*.99)break;if(f>0)hi=x;else lo=x;double step=f/d;if(halley_mode){double h=d-.5*e*s*step;if(h>1e-12)step=f/h;}double next=x-step;if(next<lo||next>hi)next=(lo+hi)*.5;x=next;}return std::llround(x*Q);}
double oracle(int64_t mi,int64_t ei){double m=double(mi)/Q,e=double(ei)/Q,lo=0,hi=3.141592663589793;for(int k=0;k<60;k++){double x=(lo+hi)*.5;if(x-e*std::sin(x)>m)hi=x;else lo=x;}return(lo+hi)*.5;}
volatile uint64_t sink;
struct Input{int64_t m,e;};
double timing(Fn fn,const std::vector<Input>& in){auto a=std::chrono::steady_clock::now();uint64_t sum=0;for(auto i:in)sum+=uint64_t(fn(i.m,i.e));auto b=std::chrono::steady_clock::now();sink=sum;return std::chrono::duration<double,std::nano>(b-a).count()/in.size();}
int main(int argc,char**argv){if(argc!=3)return 2;tolerance=std::atof(argv[2]);void*h=dlopen(argv[1],RTLD_NOW);if(!h){std::fprintf(stderr,"%s",dlerror());return 2;}auto candidate=reinterpret_cast<Fn>(dlsym(h,"gremlin_target"));if(!candidate)return 2;uint64_t hash=14695981039346656037ULL;double max_error=0,baseline_error=0;for(int e=0;e<256;e++)for(int m=0;m<256;m++){int64_t mi=PI*m/255,ei=Q*95*e/(100*255),v=candidate(mi,ei);for(int b=0;b<8;b++){hash^=(uint64_t(v)>>(8*b))&255;hash*=1099511628211ULL;}double truth=oracle(mi,ei);max_error=std::max(max_error,std::abs(double(v)/Q-truth));for(bool h:{false,true}){halley_mode=h;baseline_error=std::max(baseline_error,std::abs(double(reference(mi,ei))/Q-truth));}}if(max_error>tolerance||baseline_error>tolerance){std::fprintf(stderr,"accuracy failed: %.12g %.12g",max_error,baseline_error);return 3;}
 std::printf("{\"grid_hash\":\"%016llx\",\"max_error\":%.12g,\"reference_max_error\":%.12g,\"trials\":[",(unsigned long long)hash,max_error,baseline_error);bool comma=false;uint64_t rng=0x123456789;auto next=[&](){rng^=rng<<13;rng^=rng>>7;rng^=rng<<17;return rng;};
 for(int dist=0;dist<3;dist++)for(int size:{16384,262144}){std::vector<Input> in;for(int k=0;k<size;k++){int64_t m=next()%(PI+1),e=next()%(Q*95/100+1);if(dist==1)e=next()%(Q/5+1);if(dist==2){e=Q*8/10+next()%(Q*15/100+1);m=next()%(Q/5+1);}double truth=oracle(m,e);double err=std::abs(double(candidate(m,e))/Q-truth);if(err>tolerance){std::fprintf(stderr,"candidate exceeds tolerance: error=%.12g m=%.12g e=%.12g",err,double(m)/Q,double(e)/Q);return 3;}in.push_back({m,e});}timing(candidate,in);halley_mode=false;timing(reference,in);halley_mode=true;timing(reference,in);for(int t=0;t<9;t++){double a,b;auto baseline=[&](){halley_mode=false;double n=timing(reference,in);halley_mode=true;double h=timing(reference,in);return std::min(n,h);};if(t%2){a=timing(candidate,in);b=baseline();}else{b=baseline();a=timing(candidate,in);}std::printf("%s{\"distribution\":%d,\"count\":%d,\"trial\":%d,\"candidate_ns\":%.6f,\"reference_ns\":%.6f}",comma?",":"",dist,size,t,a,b);comma=true;}}
 std::printf("]}\n");dlclose(h);
}
