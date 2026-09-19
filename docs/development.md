# Development

Install the toolchain specified in `rust-toolchain.toml` and a system C linker (for example Ubuntu `build-essential`). Initial dependency download needs network access; default CPU execution needs no external service. Full integration tests require Bubblewrap with working namespaces/seccomp, Z3, Clang 18 and compiler-rt/libFuzzer. `Cargo.lock` pins the full dependency graph. Serde/serde_json provide versioned persisted data, TOML provides strict configuration, and SHA-256 provides content and integrity hashes. The PRNG is an in-tree versioned SplitMix64 implementation; backend dependencies and prerequisites are documented in `docs/design/D1.md` through `D6.md`.

Run from the workspace root:

```sh
cargo run -p gremlin-cli -- --help
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --release
```

CI runs these checks with locked dependencies. Search selection/mutation remain deterministic on one CPU thread; an optional CUDA backend evaluates population batches.

## Search design

The default genome is a sequence of explicitly typed constants/operator instructions with a return reference. IDs refer only to function parameters and preceding genes. Lowering creates the same validated CFG used by the interpreter. Initialization uses parameter identities, configured constants, legal single-operator applications over parameters/constants, and randomly generated genomes. It never inserts a composed oracle expression as a seed. Generic seeds are considered in stable parameter/constant/operator order up to the population size; remaining slots use random generation.

Each generation keeps the configured elite and selects parents through seeded tournaments. Seven equally sampled mutation categories change the return reference, opcode, operand, constant, insert an instruction, delete an instruction, or replace an instruction. Type-invalid attempts are rejected. Deleting a referenced value repairs its uses with the lowest-ID preceding value of the required type; an unavailable replacement rejects the mutation. After eight failed attempts, mutation clones the parent. Validation does not repair anything. Internal boolean values permit comparison/select mutations with typed operands.

Fitness orders incomplete cases, mismatching completed cases, summed width-masked bit error, instruction count, total steps, and canonical IR bytes. Constants count as instructions. Each candidate retains every case outcome and its optional semantic error. Failed execution has no invented integer observation. Evaluation count measures candidate-case executions; retained elite fitness is reused. Interpreter buffers are allocated once per candidate and reused across its corpus cases; operator execution uses stack buffers.

SplitMix64-v1 uses its specified 64-bit state and wrapping operations. Index selection uses modulo reduction (small sampling bias, deterministic). There is no wall-clock fitness criterion or concurrency. Population initialization is completed generation 1, and the configured generation budget includes it. Resume recomputes persisted fitness for integrity without counting those checks as new search evaluations.

Corpora combine per-argument boundary values, synchronized/staggered interactions, and a configured count of random cases generated from the run seed. Duplicate inputs retain all provenance. Holdout uses `run_seed XOR 0xd1b54a32d192ed03`, excludes corpus inputs, and deduplicates its own inputs. For domains up to 16 bits it uses a seeded odd-stride permutation to guarantee finite-domain termination; larger domains use independent random samples. A fully exhausted domain is reported. Without refinement, holdout counterexamples end M2 evolution with E1 retained. Explicit D1 refinement replays and adds them, regrades the population, and resumes search.

Normalized configuration records all defaults, including optional enumeration, CFG mutation, CUDA and initial-corpus fields; unknown fields fail. Implementation resource limits: at most 100,000 population members, 4,096 genes, and 1,000,000 requested cases per random/holdout budget. These are input guards, not equivalence bounds. The CLI's default `run` interpreter budget is 256 steps; its source/IR execution semantics are documented separately.

## Repository structure

`gremlin-core` owns syntax, types, CFG validation, interpretation, fixture oracles, corpus encoding, and the pinned PRNG. `gremlin-search` owns strict configuration, typed genomes, fitness, and generation state. `gremlin-cli` owns commands, atomic checkpoint/artifact persistence, holdout validation, and reports. `gremlin-native` owns the isolation boundary; `gremlin-cuda` owns optional device evaluation; `gremlin-verify` owns the narrow binary model/SMT protocol; `gremlin-codegen` owns LLVM lowering and native artifact checks. Oracle labeling and external campaign control remain outside the search engine.

The mandatory acceptance tests load the checked-in fixture budgets (population 256, 1,000 generations, 8 instructions, 256 steps, 64 random corpus requests, 256 independent holdout requests). Tests separately exercise checkpoint serialization/continuation, deletion repair, mixed-type mutation, correctness-before-size ranking, trap/timeout rejection, provenance deduplication, conflicting labels, CLI exit statuses, and artifact integrity. Debug integration and synthesis tests take longer than release tests. SHA-256 is optimized in the debug dependency profile because the isolation boundary fingerprints executable/runtime bytes for each observation.

Opt-in structural mutations insert/alter branches and bounded loops, swap/collapse edges, and mutate typed instructions. Every candidate passes IR and configured structural limits. Opt-in bounded chain enumeration keeps a checkpointed cursor and enumerates only configured roots/operators/constants. Neither path consults fixture implementations. Imported corpora are replayed by the configured oracle; expected labels alone never become fitness data.

On Ubuntu 24.04 an AppArmor policy must allow Bubblewrap to create user namespaces. CI installs a launcher-specific `userns` profile and runs a namespace smoke test. Targets still run with the fixed empty filesystem/network namespace, resource limits and default-deny seccomp policy; there is no isolation fallback.
