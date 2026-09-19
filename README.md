# gremlin

gremlin searches for small integer programs matching observed behavior. This repository implements M0–M2 and D1–D5 of [PLAN.md](PLAN.md): a typed language, validated CFG IR, bounded interpreter, reproducible search, isolated ELF observations, counterexample refinement, optional CUDA evaluation, and narrow binary-scoped formal verification. The expanded assignment and measured gates are tracked in [milestones](docs/milestones.md).

## Build and use

Rust 1.90.0 and a system linker are required. Dependencies are pinned in `Cargo.toml` and `Cargo.lock`; execution needs no network, CUDA, LLVM, solver, or external service.

```sh
cargo run -p gremlin-cli -- --help
cargo run -p gremlin-cli -- check examples/affine.gremlin
cargo run -p gremlin-cli -- run examples/affine.gremlin --args 0x0000000000000005 --max-steps 256
cargo run --release -p gremlin-cli -- synthesize --config tests/fixtures/composed_u64.toml
cargo run --release -p gremlin-cli -- resume runs/composed_u64/checkpoint.json
```

The composed fixture is `x * 3 + 1` with wrapping u64 arithmetic. Five mandatory fixture configurations live in `tests/fixtures/`; each is tested with seeds 1, 2, and 3. `examples/affine.toml` is a larger, non-gating benchmark. Search sees only the signature, observations, configured operators, and configured constant hints. It does not receive oracle expressions.

Normalized configuration records all defaults; unknown fields and unsupported target kinds fail. Output paths are relative to the current working directory. A run refuses to overwrite an existing directory. CLI results go to stdout as JSON, and evolution progress goes to stderr. `run` defaults to 256 steps and accepts comma-separated exact-width hexadecimal arguments. Booleans are internal; signatures accept 0–4 integer arguments.

## Run artifacts and resume

Each run writes:

- `config.json`: normalized, fully explicit configuration.
- `corpus.json` and `provenance.json`: canonical observations and separate provenance.
- `best.gremlin` and `best.ir.json`: candidate source and normalized IR.
- `checkpoint.json`: a generation-boundary population, per-case fitness, complete RNG state, best candidate, identities, and checksums.
- `report.json`: stop reason, status, evidence scope, corpus and holdout results, execution counts, configuration, and runtime.
- `integrity.json`: SHA-256 of every completed artifact above.

Checkpoints are atomically replaced after each completed generation. Resume verifies the checkpoint payload checksum, static input file hashes, executable identity, semantics/PRNG versions, configuration, fixture fingerprint, corpus identity, population validity, and recalculated fitness. It continues from the last completed generation. Use the same executable build and working directory as the original run. A final checkpoint can also be resumed to reproduce its validation report. Timing telemetry may differ; population, RNG, candidate sequence, and fitness remain deterministic.

A `.running` file prevents concurrent writers. After an externally killed process, confirm it has stopped and remove that stale file before resuming. Temporary files can remain after a crash; the checkpoint itself is replaced by atomic rename. Checkpoint serialization retains every case outcome and therefore grows with population × corpus size.

Exit codes: 0 successful check/execution or requested tests passed; 2 invalid input/configuration; 3 exhausted search without a corpus match; 4 infrastructure failure; 5 execution trap/timeout; 6 holdout counterexample.

## Evidence and limits

Reports keep `run_status`, `evidence_level`, and `evidence_scope` separate. A corpus match is E1; a match also passing independently seeded holdout observations is E2. Both are `TESTED`, scoped to the configured fixture or isolated binary. They are not equivalence proofs. A known holdout mismatch returns status `counterexample_found` and exit 6, even though E1 remains recorded. Setting `holdout_cases = 0` explicitly allows E1-only success.

Source supports mutable locals, structured branches and loops, and module calls/recursion with explicit depth limits. Canonical CFG source round trips preserve step counts. Search can generate bounded CFGs when structural mutation is configured. Binary execution requires Linux x86-64, Bubblewrap, and working namespaces/seccomp; see [D1](docs/design/D1.md). CUDA is opt-in and requires a toolkit build and supported NVIDIA device; see [D3](docs/design/D3.md) for configuration, parity results, timing, and limitations. Formal verification supports the documented straight-line ELF register subset; see [D4](docs/design/D4.md). Selected candidates can be lowered through LLVM after an explicit evidence gate and checked as isolated native artifacts; see [D5](docs/design/D5.md). External fuzzing is not yet implemented. Search is single-threaded, bounded, and not guaranteed to find arbitrary programs or unknown constants.

See [language semantics](docs/semantics.md), [development and search design](docs/development.md), and [validation results](docs/validation.md).
