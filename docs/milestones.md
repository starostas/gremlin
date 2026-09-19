# Expanded implementation status

The user expanded the assignment from M0–M2 to all nine gates (M0, M1, M2, D1–D6). The original PLAN.md remains the historical specification; this document records the active scope and measured progress.

| Gate | Status | Evidence |
| --- | --- | --- |
| M0–M2 | Passed | Mandatory five fixtures on seeds 1/2/3 still pass; original CPU semantics/reproducibility tests pass |
| D1 | Passed locally | Isolated ELF ABI, forbidden access, constructors, crashes, CPU/wall/memory limits, nondeterminism and restart tests; CEGIS persistence and affine discovery |
| D2 | Passed locally | Structured source, recursion with depth limits, canonical CFG round trips, and branch/loop synthesis on seeds 1/2/3 |
| D3 | Passed on RTX A4000 | 10,000 programs / 4,860,000 exact CPU/GPU comparisons; synthesis state parity, watchdog and resource failures; measured CPU/CUDA timings |
| D4 | Passed locally | Narrow complete ELF register model with bound loader metadata, binary E4, concrete counterexample replay, unsupported/timeout/unknown/scope gates |
| D5 | Passed locally | Clang 18 CFG lowering; all-width/operator native parity, trap/budget behavior, explicit evidence gates and isolated affine artifact validation |
| D6 | Passed locally | Replayed/deduplicated versioned import, provenance and search integration; libFuzzer lifecycle and actual ELF counterexample export/replay |

D1 full CLI trial: `python3 examples/build_binary.py`, then `target/release/gremlin refine --config runs/binary-example-input/affine.toml --candidate runs/binary-example-input/wrong.gremlin`, then `target/release/gremlin resume runs/binary-affine/checkpoint.json`. The run retained 256 replayed counterexamples, including x=1; reached the correct affine expression at generation 79; and passed 256 fresh holdout observations with E2 binary scope. Artifacts are in `runs/binary-affine/`. The first mutation-only trial failed at 1,000 generations and is preserved at `runs/binary-affine-mutation-only/`; the generic bounded enumeration option is explicit in the succeeding configuration.

D1 uses checkpoint schema 2 with immutable corpus/provenance snapshots and an atomic checkpoint envelope. Human-readable projections are recoverable from previous committed snapshots after interruption. Binaries, worker executable, sandbox, and exposed runtime libraries are fingerprinted; changes invalidate resume. There is no relaxed-isolation fallback. All nine gates have measured implementations; supported subsets and deployment limits remain explicit in the design notes.

Final local verification: 62 tests passed in both debug and release; formatting and Clippy passed. GPU parity and CUDA synthesis/failure tests were rerun on the A4000. The full affine pipeline (including fuzz import, retained counterexamples, resume, binary E4 and native E2) passed using the CUDA-enabled executable locally; see `docs/measurements/final-pipeline.json`.
