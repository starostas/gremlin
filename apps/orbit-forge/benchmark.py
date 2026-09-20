"""Compile actual Gremlin LLVM, verify grid parity, then benchmark equal-tolerance solvers."""
import json
from pathlib import Path
import platform
import re
import statistics
import subprocess
import tempfile
ROOT=Path(__file__).resolve().parent

class AccuracyFailure(Exception):
    def __init__(self,error,m,e):
        self.error,self.m,self.e=error,m,e
        super().__init__(f"Additional benchmark input exceeds the accuracy budget: error {error:.3g} rad")

def benchmark(result):
    with tempfile.TemporaryDirectory(prefix='orbit-forge-') as directory:
        d=Path(directory)
        (d/'winner.ll').write_text(result['llvm'])
        compiler='/usr/bin/clang-18'
        for command in ([compiler,'-O3','-shared','-fPIC','-x','ir',str(d/'winner.ll'),'-o',str(d/'winner.so')],
                        [compiler,'-O3','-std=c++17','-x','c++',str(ROOT/'benchmark.cpp'),'-lstdc++','-lm','-ldl','-o',str(d/'bench')]):
            subprocess.run(command,check=True,capture_output=True,text=True,timeout=30)
        run=subprocess.run([str(d/'bench'),str(d/'winner.so'),str(result['tolerance'])],capture_output=True,text=True,timeout=60)
        if run.returncode:
            failure=re.search(r'candidate exceeds tolerance: error=(\S+) m=(\S+) e=(\S+)',run.stderr)
            if failure: raise AccuracyFailure(*(float(x) for x in failure.groups()))
            run.check_returncode()
        data=json.loads(run.stdout)
        if data['grid_hash']!=result['grid_hash']:
            raise ValueError('Compiled winner disagrees with the interpreter on the validation grid')
        groups={}
        for t in data['trials']:
            groups.setdefault((t['distribution'],t['count']),[]).append(t)
        summaries=[]
        for (distribution,count),trials in groups.items():
            a=statistics.median(t['candidate_ns'] for t in trials)
            b=statistics.median(t['reference_ns'] for t in trials)
            summaries.append({'distribution':distribution,'count':count,'candidate_ns':a,'reference_ns':b,'ratio':b/a})
        # A speedup badge requires a consistent win, not the fastest selected trial.
        wins=all(t['candidate_ns'] < t['reference_ns'] for t in data['trials']) and min(g['ratio'] for g in summaries)>1.05
        data.update(available=True,compiled_grid_checked=65536,consistent_win=wins,summaries=summaries,
                    compiler=subprocess.check_output([compiler,'--version'],text=True).splitlines()[0],
                    flags='-O3, no fast-math; scalar native calls',host=platform.machine(),
                    cpu=next((line.split(':',1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')),platform.processor()))
        if wins:data['speedup']=min(g['ratio'] for g in summaries)
        return data
