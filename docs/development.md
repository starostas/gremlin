# Development

Install the toolchain specified in `rust-toolchain.toml` and a system C linker (for example Ubuntu `build-essential`). Initial dependency download needs network access; execution and tests need no external services. `Cargo.lock` pins the full dependency graph. Serde/serde_json provide versioned persisted data, TOML provides strict configuration, and SHA-256 provides content and integrity hashes. The PRNG is an in-tree versioned SplitMix64 implementation; no backend dependencies are included.

Run from the workspace root:

```sh
cargo run -p gremlin-cli -- --help
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --release
```

CI runs these checks with locked dependencies. Runtime is CPU-only and single-threaded.

## Search design

A genome is a sequence of explicitly typed constants/operator instructions with a return reference. IDs refer only to function parameters and preceding genes. Lowering creates the same validated CFG used by the interpreter. Initialization uses parameter identities, configured constants, legal single-operator applications over parameters/constants, and randomly generated genomes. It never inserts a composed oracle expression as a seed. Generic seeds are considered in stable parameter/constant/operator order up to the population size; remaining slots use random generation.

Each generation keeps the configured elite and selects parents through seeded tournaments. Seven equally sampled mutation categories change the return reference, opcode, operand, constant, insert an instruction, delete an instruction, or replace an instruction. Type-invalid attempts are rejected. Deleting a referenced value repairs its uses with the lowest-ID preceding value of the required type; an unavailable replacement rejects the mutation. After eight failed attempts, mutation clones the parent. Validation does not repair anything. Internal boolean values permit comparison/select mutations with typed operands.

Fitness orders incomplete cases, mismatching completed cases, summed width-masked bit error, instruction count, total steps, and canonical IR bytes. Constants count as instructions. Each candidate retains every case outcome and its optional semantic error. Failed execution has no invented integer observation. Evaluation count measures candidate-case executions; retained elite fitness is reused. Interpreter buffers are allocated once per candidate and reused across its corpus cases; operator execution uses stack buffers.

SplitMix64-v1 uses its specified 64-bit state and wrapping operations. Index selection uses modulo reduction (small sampling bias, deterministic). There is no wall-clock fitness criterion or concurrency. Population initialization is completed generation 1, and the configured generation budget includes it. Resume recomputes persisted fitness for integrity without counting those checks as new search evaluations.

Corpora combine per-argument boundary values, synchronized/staggered interactions, and a configured count of random cases generated from the run seed. Duplicate inputs retain all provenance. Holdout uses `run_seed XOR 0xd1b54a32d192ed03`, excludes corpus inputs, and deduplicates its own inputs. For domains up to 16 bits it uses a seeded odd-stride permutation to guarantee finite-domain termination; larger domains use independent random samples. A fully exhausted domain is reported. Holdout counterexamples are recorded but never fed into M2 evolution.

All configuration fields are required, so there are no hidden configuration defaults. Implementation resource limits: at most 100,000 population members, 4,096 genes, and 1,000,000 requested cases per random/holdout budget. These are input guards, not equivalence bounds. The CLI's default `run` interpreter budget is 256 steps; its source/IR execution semantics are documented separately.

## Repository structure

`gremlin-core` owns syntax, types, CFG validation, interpretation, fixture oracles, corpus encoding, and the pinned PRNG. `gremlin-search` owns strict configuration, typed genomes, fitness, and generation state. `gremlin-cli` owns commands, atomic checkpoint/artifact persistence, holdout validation, and reports. The fixture adapter is called outside the search engine.

The mandatory acceptance tests load the checked-in fixture budgets (population 256, 1,000 generations, 8 instructions, 256 steps, 64 random corpus requests, 256 independent holdout requests). Tests separately exercise checkpoint serialization/continuation, deletion repair, mixed-type mutation, correctness-before-size ranking, trap/timeout rejection, provenance deduplication, conflicting labels, CLI exit statuses, and artifact integrity. Debug synthesis tests can take roughly two minutes on a single CPU; release tests are much faster.
