#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cerrno>
#include <string>
#include <unistd.h>
#include <fcntl.h>
#include <sys/wait.h>
extern "C" int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
 const char *worker=getenv("GREMLIN_WORKER"),*directory=getenv("GREMLIN_CAMPAIGN"),*width=getenv("GREMLIN_INPUT_BYTES");
 if(!worker||!directory||!width){fprintf(stderr,"gremlin: missing harness configuration\n");_exit(70);}
 if(size!=strtoul(width,nullptr,10))return 0;
 static const char hex[]="0123456789abcdef";std::string input;input.reserve(size*2);for(size_t i=0;i<size;++i){input.push_back(hex[data[i]>>4]);input.push_back(hex[data[i]&15]);}
 pid_t pid=fork();if(pid<0){perror("gremlin fork");_exit(70);}if(pid==0){int fd=open("/dev/null",O_WRONLY);if(fd>=0){dup2(fd,1);dup2(fd,2);close(fd);}execl(worker,worker,"--fuzz-observe",directory,input.c_str(),(char*)nullptr);_exit(70);}
 int status;while(waitpid(pid,&status,0)<0){if(errno!=EINTR){perror("gremlin wait");_exit(70);}}
 if(WIFEXITED(status)&&WEXITSTATUS(status)==86){fprintf(stderr,"gremlin: replayed candidate mismatch\n");abort();}
 if(!WIFEXITED(status)||WEXITSTATUS(status)!=0){fprintf(stderr,"gremlin: ORACLE_INFRASTRUCTURE_FAILURE\n");_exit(70);}return 0;
}
