// Compiles the winner's Gremlin LLVM, verifies grid parity against the interpreter,
// then benchmarks equal-tolerance solvers. This mirrors apps/orbit-forge/benchmark.py
// so a live GPU run reports the same evidence the recorded run does.
import { execFile } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';

const run = promisify(execFile);
const compiler = process.env.ORBIT_FORGE_CLANG ?? '/usr/bin/clang-18';

export class AccuracyFailure extends Error {
  constructor(error, m, e) {
    super(`Additional benchmark input exceeds the accuracy budget: error ${error.toPrecision(3)} rad`);
    this.error = error;
    this.m = m;
    this.e = e;
  }
}

function median(values) {
  const sorted = [...values].sort((left, right) => left - right);
  const middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

function cpuModel() {
  try {
    const line = readFileSync('/proc/cpuinfo', 'utf8')
      .split('\n')
      .find((entry) => entry.startsWith('model name'));
    return line ? line.split(':', 2)[1].trim() : process.arch;
  } catch {
    return process.arch;
  }
}

/**
 * @param result the engine's terminal `done` event, including its `llvm` text.
 * @param sourceRoot the repository's apps/orbit-forge directory, holding benchmark.cpp.
 */
export async function benchmark(result, sourceRoot) {
  const directory = mkdtempSync(join(tmpdir(), 'orbit-forge-'));
  try {
    const llvm = join(directory, 'winner.ll');
    const shared = join(directory, 'winner.so');
    const bench = join(directory, 'bench');
    writeFileSync(llvm, result.llvm, 'utf8');

    await run(compiler, ['-O3', '-shared', '-fPIC', '-x', 'ir', llvm, '-o', shared], { timeout: 30_000 });
    await run(
      compiler,
      ['-O3', '-std=c++17', '-x', 'c++', join(sourceRoot, 'benchmark.cpp'), '-lstdc++', '-lm', '-ldl', '-o', bench],
      { timeout: 30_000 }
    );

    let stdout;
    try {
      ({ stdout } = await run(bench, [shared, String(result.tolerance)], {
        timeout: 60_000,
        maxBuffer: 8 * 1024 * 1024
      }));
    } catch (error) {
      const failure = /candidate exceeds tolerance: error=(\S+) m=(\S+) e=(\S+)/.exec(error.stderr ?? '');
      if (failure) throw new AccuracyFailure(Number(failure[1]), Number(failure[2]), Number(failure[3]));
      throw error;
    }

    const data = JSON.parse(stdout);
    if (data.grid_hash !== result.grid_hash) {
      throw new Error('Compiled winner disagrees with the interpreter on the validation grid');
    }

    const groups = new Map();
    for (const trial of data.trials) {
      const key = `${trial.distribution}:${trial.count}`;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(trial);
    }
    const summaries = [...groups.values()].map((trials) => {
      const candidate = median(trials.map((trial) => trial.candidate_ns));
      const reference = median(trials.map((trial) => trial.reference_ns));
      return {
        distribution: trials[0].distribution,
        count: trials[0].count,
        candidate_ns: candidate,
        reference_ns: reference,
        ratio: reference / candidate
      };
    });

    // A speedup badge requires a consistent win, not the fastest selected trial.
    const smallestRatio = Math.min(...summaries.map((summary) => summary.ratio));
    const wins = data.trials.every((trial) => trial.candidate_ns < trial.reference_ns) && smallestRatio > 1.05;
    const { stdout: version } = await run(compiler, ['--version'], { timeout: 10_000 });

    return {
      ...data,
      available: true,
      compiled_grid_checked: 65536,
      consistent_win: wins,
      summaries,
      compiler: version.split('\n')[0],
      flags: '-O3, no fast-math; scalar native calls',
      host: process.arch,
      cpu: cpuModel(),
      ...(wins ? { speedup: smallestRatio } : {})
    };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}
