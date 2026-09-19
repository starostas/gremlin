#!/usr/bin/env python3
"""Reproduce a bounded CRC synthesis experiment; outputs must be a fresh directory."""
import argparse
import functools
import hashlib
import json
from pathlib import Path
import random
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def command(argv, log=None, allowed=(0,)):
    if log:
        with log.open('w') as stream:
            result = subprocess.run([str(x) for x in argv], cwd=ROOT, stdout=stream,
                                    stderr=subprocess.STDOUT)
    else:
        result = subprocess.run([str(x) for x in argv], cwd=ROOT)
    if result.returncode not in allowed:
        raise subprocess.CalledProcessError(result.returncode, argv)


def configure(out, byte_generations=300):
    so = out / 'target.so'
    command(['cc', '-O2', '-shared', '-fPIC', '-nostdlib', '-Wl,-z,noexecstack',
             ROOT / 'examples/cksum-crc/target.c', '-o', so])
    for name, args, instructions in [('byte', ['u32', 'u32'], 64),
                                     ('feedback', ['u32'], 12)]:
        # The search receives the polynomial and shift constants, not a candidate.
        config = f'''schema_version = 1
seed = 1
[target]
kind = "binary"
name = "crc_{name}"
arguments = {json.dumps(args)}
return_type = "u32"
[search]
population = 256
generations = {byte_generations if name == "byte" else 300}
max_instructions = {instructions}
max_steps = 256
elite = 8
tournament_size = 4
enumeration_depth = 3
enumeration_proposals = 128
operators = ["shl", "lshr", "ashr", "xor", "and", "sub", "mul"]
constants = ["0u32", "1u32", "8u32", "24u32", "31u32", "0x04c11db7u32"]
[corpus]
random_cases = 64
holdout_cases = 256
[output]
directory = {json.dumps(str(out / name))}
[target.binary]
path = {json.dumps(str(so))}
sha256 = "{hashlib.sha256(so.read_bytes()).hexdigest()}"
architecture = "x86_64"
format = "elf"
symbol = "crc_{name}"
abi = "sysv64"
environment = "empty"
wall_timeout_ms = 5000
cpu_seconds = 2
memory_mb = 256
'''
        (out / (name + '.toml')).write_text(config)


def assemble(out):
    # Explicit human decomposition: mix the byte, then apply eight CRC rounds.
    # Only the feedback body below is discovered by the synthesis engine.
    source = (out / 'feedback/best.gremlin').read_text()
    body = source.split('{', 1)[1].rsplit('}', 1)[0]
    text = ('fn candidate(state: u32, byte: u32) -> u32 {\n'
            '    let s0: u32 = xor(state, shl(byte, 24u32));\n')
    reference = text
    for i in range(8):
        renamed = re.sub(r'\bv(\d+)\b', lambda m: f's{i}' if m[1] == '0'
                         else f'r{i}v{m[1]}', body)
        renamed = re.sub(r'return (\w+);', lambda m:
                         f'let s{i+1}: u32 = xor(shl(s{i}, 1u32), {m[1]});', renamed)
        text += renamed
        reference += (f'    let s{i+1}: u32 = xor(shl(s{i}, 1u32), '
                      f'select(uge(s{i}, 0x80000000u32), 0x04c11db7u32, 0u32));\n')
    (out / 'assembled-byte.gremlin').write_text(text + '    return s8;\n}\n')
    (out / 'byte-reference.gremlin').write_text(reference + '    return s8;\n}\n')
    (out / 'feedback-reference.gremlin').write_text(
        'fn candidate(state: u32) -> u32 {\n'
        '    return select(uge(state, 0x80000000u32), 0x04c11db7u32, 0u32);\n}\n')


def validate(out, gremlin):
    @functools.lru_cache(maxsize=None)
    def update(state, byte):
        result = json.loads(subprocess.check_output([
            str(gremlin), 'run', str(out / 'assembled-byte.gremlin'),
            '--args', f'0x{state:08x},0x{byte:08x}', '--max-steps', '256'], cwd=ROOT))
        assert result['status'] == 'completed', result
        return int(result['value'], 16)

    def reference(state, byte):
        state ^= (byte << 24) & 0xffffffff
        for _ in range(8):
            feedback = 0x04c11db7 if state & 0x80000000 else 0
            state = ((state << 1) ^ feedback) & 0xffffffff
        return state

    rng = random.Random(1)
    cases = [(s, b) for s in (0, 1, 0x80000000, 0xffffffff) for b in range(256)]
    cases += [(rng.getrandbits(32), rng.getrandbits(32)) for _ in range(256)]
    for state, byte in cases:
        assert update(state, byte) == reference(state, byte), (state, byte)

    # Length folding and complement are an explicit Python harness, not synthesized.
    messages = [b'', b'123456789', b'hello\n', b'Gremlin\n', bytes(range(256))]
    messages += [bytes([b]) for b in range(256)]
    results = []
    for message in messages:
        state = 0
        for byte in message:
            state = update(state, byte)
        length = len(message)
        while length:
            state = update(state, length & 255)
            length >>= 8
        actual = (~state) & 0xffffffff
        expected, size = map(int, subprocess.check_output(['cksum'], input=message).split())
        assert (actual, len(message)) == (expected, size), (message, actual, expected)
        if len(results) < 5:
            results.append({'input_hex': message.hex(), 'bytes': size, 'cksum': actual})
    reports = {name: json.loads((out / name / 'report.json').read_text())
               for name in ('byte', 'feedback')}
    summary = {
        'description': 'Independent POSIX CRC recurrence, not a lifted coreutils executable',
        'direct_byte_search': {k: reports['byte'][k] for k in
                              ('run_status', 'generations', 'runtime_seconds', 'evidence_level')},
        'feedback_search': {k: reports['feedback'][k] for k in
                            ('run_status', 'generations', 'runtime_seconds', 'evidence_level')},
        'feedback_holdout_mismatches': reports['feedback']['holdout']['mismatch_count'],
        'assembled_byte': 'Human byte mixing and eight rounds with synthesized feedback inlined',
        'proofs': {name: {k: json.loads((out / (name + '-proof.json')).read_text())[k]
                         for k in ('status', 'evidence_level', 'evidence_scope')}
                   for name in ('feedback', 'byte')},
        'native_artifact': {k: json.loads((out / 'native/report.json').read_text())[k]
                            for k in ('status', 'artifact_evidence_level', 'corpus_count', 'holdout_count')},
        'byte_reference_comparisons': len(cases),
        'system_cksum_comparisons': len(messages),
        'system_cksum_version': subprocess.check_output(['cksum', '--version'], text=True).splitlines()[0],
        'examples': results,
    }
    (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--gremlin', type=Path, default=ROOT / 'target/release/gremlin')
    parser.add_argument('--output', type=Path, default=ROOT / 'runs/cksum-crc-demo')
    parser.add_argument('--byte-generations', type=int, default=300)
    args = parser.parse_args()
    if args.byte_generations < 1:
        parser.error("--byte-generations must be positive")
    gremlin, out = args.gremlin.resolve(), args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    configure(out, args.byte_generations)
    for name in ('byte', 'feedback'):
        budget = args.byte_generations if name == 'byte' else 300
        print(f'Searching {name} ({budget} generations maximum)', flush=True)
        # Exit 3 means the bounded search exhausted its budget, a recorded result.
        command([gremlin, 'synthesize', '--config', out / (name + '.toml')],
                out / (name + '.log'), allowed=(0, 3))
    report = json.loads((out / 'feedback/report.json').read_text())
    if report['evidence_level'] != 'E2':
        raise RuntimeError('Feedback search did not pass; inspect its report')
    assemble(out)
    for name, candidate in [('feedback', out / 'feedback/best.gremlin'),
                            ('byte', out / 'assembled-byte.gremlin')]:
        command([gremlin, 'verify-reference', '--reference', out / (name + '-reference.gremlin'),
                 '--candidate', candidate, '--solver', '/usr/bin/z3', '--timeout-ms', '10000',
                 '--output', out / (name + '-proof.json')])
        proof = json.loads((out / (name + '-proof.json')).read_text())
        if proof['status'] != 'Equivalent':
            raise RuntimeError(f'{name} reference equivalence failed: {proof["status"]}')
    command([gremlin, 'compile', '--config', out / 'byte.toml', '--candidate',
             out / 'assembled-byte.gremlin', '--min-evidence', 'E2', '--compiler',
             '/usr/bin/clang-18', '--timeout-ms', '10000', '--output', out / 'native'])
    validate(out, gremlin)


if __name__ == '__main__':
    main()
