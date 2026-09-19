# Validation record

Implemented scope: M0–M2 from `PLAN.md`. Validation ran locally on Linux x86-64 with Rust/Cargo 1.90.0. The environment initially had neither Rust nor a C linker; the pinned toolchain and Ubuntu `build-essential` were installed before compiling. No existing implementation or Git repository was present. `Cargo.lock` is included; no Git commit was created.

## Acceptance commands

The following commands were executed using `/root/.cargo/bin/cargo`:

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed, no warnings |
| `cargo test --workspace` | Passed, 23 tests |
| `cargo test --workspace --release` | Passed, 23 tests |

The initial build failed because the environment lacked a linker; installing it resolved that failure. Implementation checks caught and fixed a Rust parsing ambiguity, a Clippy style warning, and stricter literal/hash encoding details before the final runs. CI configuration is supplied but was not executed on a remote CI service.

## Mandatory synthesis gate

All five checked-in fixture budgets were tested on seeds 1, 2, and 3. Each successful candidate matched its corpus and all 256 independent holdout cases. The holdout seed is derived separately and holdout inputs exclude corpus inputs.

| Fixture | Seed 1 generation | Seed 2 generation | Seed 3 generation | Corpus cases |
| --- | ---: | ---: | ---: | ---: |
| identity_u64 | 1 | 1 | 1 | 257 |
| increment_u64 | 1 | 1 | 1 | 257 |
| xor_u64 | 1 | 1 | 1 | 257 |
| add_u64 | 1 | 1 | 1 | 832 |
| composed_u64 | 10 | 233 | 400 | 257 |

Generation 1 includes generic initialization; the first four fixtures can be expressed by an identity or a single operator. The composed fixture requires mutation/evolution. No target expression is planted and budgets remain population 256 and at most 1,000 generations.

Additional tests cover all operators and integer widths, signed faults, wrapping arithmetic, modulo shifts/rotations, eager select, strict literal encoding, source round trips, ID normalization, CFG dominance/edge validation, simultaneous edge binding, terminating/infinite loops and exact budgets, generated programs, typed mutations, deletion repair, correctness-first fitness, execution failures, provenance deduplication, conflicting labels, finite-domain holdouts, serialized checkpoint continuation, strict configuration, CLI errors, artifact integrity, and deliberately failed holdout reporting.

## Exercised CLI and retained artifacts

These commands completed successfully:

```sh
cargo run -p gremlin-cli -- --help
cargo run -p gremlin-cli -- check examples/affine.gremlin
cargo run -p gremlin-cli -- run examples/affine.gremlin --args 0x0000000000000005 --max-steps 256
cargo run --release -p gremlin-cli -- synthesize --config tests/fixtures/composed_u64.toml
cargo run --release -p gremlin-cli -- resume runs/composed_u64/checkpoint.json
```

The affine source returned `0x000000007f6e5d6e` in seven steps. The retained composed synthesis run reached generation 10 with 257 corpus cases, 256 holdout cases, zero holdout mismatches, and 639,416 search candidate-case executions. Its result is E2, `TESTED`, scope `fixture`. Resume succeeded using the same executable. The seven final artifact checksums were independently checked with Python SHA-256.

Artifacts: `runs/composed_u64/config.json`, `corpus.json`, `provenance.json`, `best.gremlin`, `best.ir.json`, `checkpoint.json`, `report.json`, and `integrity.json`. Runtime output is ignored by `.gitignore`; the source, configurations, tests, and documentation are repository deliverables.

## Remaining limits

Only checked-in Rust fixtures are available as target adapters. Source/search remain straight-line; direct IR has CFG interpretation. Evidence is sampled fixture evidence, not proof. Large affine synthesis is a non-gating benchmark and was not run; only its source checking/execution was exercised. Binary isolation, CEGIS, CUDA, formal verification, native compilation, and external fuzzing remain deferred as requested. Search can exhaust its budget, and unknown large constants are not recovered without configured hints. Checkpoints retain per-case results and can be large (the retained checkpoint is approximately 8.4 MiB).
