# Compiled targets and verification

## Requirements

The CPU-only workflow needs no additional Gremlin runtime dependencies beyond Rust and a system linker. These workflows add their own requirements:

| Task | Additional requirements |
| --- | --- |
| Observe a compiled target | Linux x86-64, Bubblewrap, working namespaces, and seccomp |
| Evaluate candidates on GPU | CUDA toolkit for the build; a supported NVIDIA GPU at runtime |
| Prove equivalence | Z3 and a supported model |
| Compile a candidate | Clang 18; binary-oracle prerequisites for validation |
| Run a fuzzing campaign | Clang 18 and compiler-rt/libFuzzer; binary-oracle prerequisites |

For binary isolation setup, see [D1](design/D1.md). GPU setup and measurements are in [D3](design/D3.md).

## Refine a binary target

A binary target is a Linux x86-64 ELF shared library exposing a named System V integer function. Gremlin needs its path, SHA-256, symbol, and argument/return types. Arbitrary command-line programs with files and printed output are outside this interface.

The affine example builds a shared library, emits a configuration, and supplies a deliberately wrong starting candidate:

    python3 examples/build_binary.py
    target/release/gremlin refine --config runs/binary-example-input/affine.toml --candidate runs/binary-example-input/wrong.gremlin

Refinement checks the candidate, retains counterexamples, and searches for a replacement. See the [binary workflow](design/D1.md) and [supported target contract](support.md).

## Prove a candidate

For a small proof example with a known candidate:

    python3 examples/build_formal.py
    target/release/gremlin verify --config runs/formal-example/affine.toml --candidate runs/formal-example/candidate.gremlin --solver /usr/bin/z3 --timeout-ms 10000 --output runs/formal-example/proof.json

The proof output path must be new. The binary proof model supports a narrow straight-line instruction subset; a target can be executable by the oracle but unsupported by the verifier. See [formal verification](design/D4.md).

## More workflows

See [native compilation](design/D5.md), [corpus import and fuzzing](design/D6.md), and [CUDA evaluation](design/D3.md).
