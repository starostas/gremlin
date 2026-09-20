# Validation record

Scope: all nine gates (M0, M1, M2, D1–D6), following the user's expansion of the original assignment. CPU/native/formal/fuzzer validation runs on Linux x86-64 with Rust 1.90.0, Bubblewrap 0.9.0, Z3 4.8.12, Clang 18.1.3 and LLVM 18 libFuzzer. CUDA tests run on the supplied RTX A4000 with NVCC 13.0.88. Dependencies and tool identities are recorded in the implementation, reports and lockfile.

## Verification commands

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --release --locked
# On the GPU machine:
cargo clippy --workspace --all-targets --features cuda --locked -- -D warnings
cargo test --release --locked -p gremlin-cuda --features cuda
cargo test --release --locked -p gremlin-cli --features cuda --test cuda
```

Final local results: **62 tests passed in debug and 62 in release**, plus documentation tests. Formatting and workspace Clippy checks passed with warnings denied. The suites include import normalization and malformed ELF loader-metadata regressions. GPU parity and the CUDA synthesis/watchdog/resource tests passed on the A4000. The default build's CUDA-unavailable test is separate from the required hardware gate; it is not a substitute for GPU execution.

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

## Original CPU baseline artifacts

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

## Expanded milestone evidence

| Gate | Measured result |
| --- | --- |
| D1 | Isolated ELF ABI and resource/forbidden-operation tests; affine CEGIS retains 256 replayed counterexamples, converges at generation 79, passes 256 fresh holdouts and resumes |
| D2 | Typed structured source and bounded recursion; branch fixtures converge at generations 2/4/3 and loop fixtures at 2/4/2 for seeds 1/2/3; 3,000 structural mutations preserve valid IR and budget behavior |
| D3 | 10,000 seeded programs, all integer widths/operators, divergent CFGs and partial warps: 4,860,000 exact CPU/GPU outcome/step comparisons; full search states agree; watchdog/memory/device errors are explicit |
| D4 | Equivalent/inequivalent ELF fixtures; bound GNU/SysV loader metadata and malformed-table rejection; concrete oracle counterexample replay; explicit Unsupported/Unknown/Timeout; reference-model evidence cannot become binary evidence |
| D5 | All supported operators and widths, eager traps, modulo shifts, CFG edges and budgets match isolated native output; E4-gated affine source produces separately TESTED/E2 native artifact |
| D6 | Replay/dedup/provenance/import-to-search tests; actual libFuzzer lifecycle; an affine mismatch is exported and replayed into a canonical corpus |

A final complete pipeline used a CUDA-enabled executable on the local namespace-capable host: replayed fuzz corpus → affine refinement → resume → binary-scoped E4 → LLVM artifact validated against CPU and the original ELF. It retained 256 counterexamples, converged at generation 79 after 5,037,456 candidate-case executions, and checked the native artifact on 257 corpus plus 256 fresh holdout cases. The source candidate is binary E4 under the narrow model assumptions; the compiled artifact remains TESTED/E2.

Checked-in measurements: `docs/measurements/cuda-parity.json`, `cuda-synthesis.json`, `affine-proof.json`, `affine-counterexample.json`, `native-affine.json`, `fuzzer-affine.json`, and `final-pipeline.json`. Full local pipeline artifacts, including the fixed executable, source, checkpoints, query, LLVM IR, ELF and all observations, are in `runs/final-v1/`. Local test logs are `runs/final-debug.log` and `runs/final-release.log`. Generated run directories are intentionally gitignored.

## Limits and deployment observations

See [supported features](support.md) and the D1–D6 design notes. Calls remain CPU/module-only; the binary proof subset is deliberately narrow; LLVM artifacts retain their configured semantic budget; external fuzzing does not provide target coverage. Search is bounded and unknown large constants still need configured hints. A mutation-only affine trial exhausted its 1,000-generation budget before explicit generic enumeration was enabled.

CUDA was slower on the measured small synthesis workload: 4.263 seconds versus CPU 0.948 seconds. Setup, transfers, serialization and fresh worker/context startup are included. No general acceleration claim is made.

The supplied GPU container does not permit the user namespaces required for binary isolation. GPU evaluation was measured there; binary/proof/native integration was measured locally, including with its CUDA-enabled executable. No reduced-isolation fallback was used. GitHub Ubuntu 24.04 initially blocked Bubblewrap's network-namespace setup; a launcher-specific AppArmor userns profile now passes the CI namespace smoke test and the complete CPU workflow (commit `9620da1`). The documentation site builds as a static Astro application and is configured for Vercel deployment; this is separate from the Rust gates.

## Comparator extension

The complete debug and release suites pass 62 tests each after adding built-in and custom scoring. Formatting and Clippy pass with warnings denied. The added custom-comparator CPU/CUDA state-parity test passed on the RTX A4000, and CUDA-enabled workspace Clippy passed. Local regression logs are `runs/comparator-tests-debug.log` and `runs/comparator-tests-release.log`.

Custom scorers preserve exact-success checks, reject traps and timeouts, accumulate costs without 64-bit overflow, and are bound to persisted configuration. Tests cover ranking changes, zero-score false positives, resume/regrading integrity, signed raw-bit handling and CLI failure reports. See [fitness comparators](comparators.md).

The CRC experiment now includes a 3,000-generation default-ranking run (333,315,584 candidate/input evaluations), which remained at 315 mismatches and 4,843 bit errors. A 300-generation bit-error-first run reached 394 mismatches and 4,499 bit errors. Neither is a correct implementation. Raw summary: `docs/measurements/cksum-comparators.json`; configurations, logs and full reports: `runs/cksum-crc-budget/` and `runs/cksum-comparators/`.
