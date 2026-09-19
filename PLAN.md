# gremlin: agent implementation specification

## 0. Assignment and boundaries

Build `gremlin`, a Rust system that searches for programs matching an executable function's observable behavior. This is program synthesis, not decompilation: the replacement need not resemble the target's implementation.

**Current assignment: implement milestones M0–M2 only.** Deliver a working CPU-based synthesis loop over small integer functions, with a language frontend, validated IR, reference interpreter, reproducible search, and reports. Stop when its acceptance tests pass. Do not build empty abstractions for later milestones.

The longer-term v1 target remains Linux x86-64 ELF shared-library functions with an explicitly supplied symbol and System V integer signature. Functions must be deterministic and side-effect-free; the integer return value is the only observable. CUDA evaluation, isolated binary execution, counterexample refinement, supported formal verification, and LLVM output are later deliverables—not prerequisites for the current assignment.

### Naming

Use `gremlin` for the project, language, repository, executable, and documentation. Use `gremlin-*` for Cargo packages and `gremlin_*` for Rust crate identifiers. Say “gremlin language,” “gremlin CLI,” or “gremlin search engine” when the distinction matters. Do not retain a second platform name or compatibility alias.

### Explicit refinement decisions

These are design defaults introduced by this rewrite, not claims that the supplied draft already resolved them:

- Keep the draft's CPU-first stopping point; use a straight-line source/search subset initially. Preserve a CFG-based semantic IR so control flow can follow without replacing it. Treat general language expressiveness as a roadmap item, not an unqualified “Turing-complete” acceptance claim.
- Begin with three crates, not a crate for every planned subsystem.
- Fix integer semantics, fitness ordering, encoding, and reproducibility rules below rather than delegating them to an agent.
- Put a working binary-oracle/CEGIS path before CUDA acceleration. Do not let GPU implementation conceal an incomplete synthesis loop.
- Separate proof about a reference model from proof about the supplied binary. A solver alone does not provide the missing binary semantics.

If an existing repository conflicts with these decisions, identify the conflict before modifying it. Preserve unrelated work. Do not expand scope to reconcile speculative future requirements.

## 1. Implementation rules

Inspect the repository first and reuse working code. Implement and test one milestone at a time. Use modules until a real integration boundary warrants another crate.

Pin the Rust toolchain and dependencies, commit `Cargo.lock`, and document dependency choices briefly. M0–M2 must build and test without CUDA, LLVM, an SMT solver, a fuzzer, network access at runtime, or proprietary services. Do not choose dependencies for deferred integrations yet.

No placeholder successes, target-specific solution tables, silently ignored configuration, or default-on future subsystems. Unsupported input must produce a useful error. Infrastructure failures must not masquerade as candidate failures or equivalence.

On completion, report implemented milestones, commands actually run and their results, remaining limitations, and artifact paths. Do not report planned or unexecuted tests as passing.

## 2. Initial repository

```text
gremlin/
  Cargo.toml
  Cargo.lock
  rust-toolchain.toml
  README.md
  crates/
    gremlin-core/     # types, syntax, IR, validation, interpreter, corpus
    gremlin-search/   # genomes, mutations, fitness, evolution, checkpoints
    gremlin-cli/      # configuration, commands, run artifacts
  docs/
    semantics.md
    development.md
  examples/
  tests/fixtures/
```

Dependency direction: `gremlin-cli -> gremlin-search -> gremlin-core`; the CLI may also use core directly. Core must not depend on search or a native backend. Add later crates only with working implementations and tests.

## 3. Language and semantics

The gremlin semantics are authoritative. The interpreter, GPU evaluator, symbolic encoding, and native backend must agree with them. Do not inherit Rust, C, CUDA, or LLVM behavior implicitly.

### 3.1 Current source subset

Support `bool`, `u8/u16/u32/u64`, and `i8/i16/i32/i64`. Target signatures contain integer arguments and one integer return value; booleans are internal values. Allow 0–4 arguments in the initial implementation; reject larger signatures explicitly.

The initial parser accepts one function, typed parameters, typed immutable `let` bindings, and a final `return`. Expressions are identifiers, typed literals, or calls to the built-in operators below. Operator calls are not user-defined function calls. Do not implement infix syntax, inference, casts, mutable bindings, or structured control-flow syntax in M0–M2.

```gremlin
fn candidate(x: u64) -> u64 {
    let a: u64 = xor(x, 0x12345678u64);
    let b: u64 = mul(a, 7u64);
    return add(b, 3u64);
}
```

Specify and test the grammar in `docs/semantics.md`. Decimal and hexadecimal integer literals require type suffixes. Signed decimal literals may be negative; hexadecimal literals denote width-limited bit patterns. Reject out-of-range literals. Boolean literals are `true` and `false`. Support whitespace and `//` comments. Printed source must parse back to equivalent normalized IR.

### 3.2 Operators

```text
Arithmetic: add sub mul udiv sdiv urem srem
Bitwise:    and or xor not shl lshr ashr rotl rotr
Comparison: eq ne ult ule ugt uge slt sle sgt sge
Selection:  select(condition, if_true, if_false)
```

Except for `select`, integer operands must have identical types. Comparison results are `bool`. `eq` and `ne` also accept two booleans. Unsigned division/remainder/comparisons require unsigned types; signed variants require signed types. Other bitwise operations accept signed or unsigned integers. `select` requires a boolean condition and identically typed alternatives. There are no implicit conversions.

| Case | Required behavior |
| --- | --- |
| Integer representation | A value is a type plus a width-limited bit pattern; signed interpretation uses two's complement. |
| Add, subtract, multiply | Wrap modulo `2^width`, including signed types. |
| Bitwise operations | Operate on the width-limited pattern; mask the result to that width. |
| Signed division | Quotient truncates toward zero; remainder has the dividend's sign. |
| Division/remainder faults | Zero divisor traps. Signed `MIN / -1` and `MIN % -1` both trap. |
| Shifts and rotations | Interpret the count's pattern as unsigned and reduce it modulo the operand width. |
| Right shifts | `lshr` fills with zero; `ashr` replicates the top bit, independent of the type's signedness. |
| Comparisons | Equality compares patterns; signed/unsigned ordering follows the operator's required operand type. |
| Evaluation order | Evaluate expression arguments left-to-right. `select` is eager; a trap while computing either alternative remains a trap. |

Use explicit wrapping and checked operations in the interpreter; host arithmetic must not accidentally define behavior. Account for width 64 without overflowing host-side mask calculations.

### 3.3 IR and validation

Use a typed SSA-style CFG with block parameters, not a stack machine or an untyped opcode array. Represent instructions with typed variants or equivalent validated payloads, not mandatory unused operand fields.

A function contains parameters, a return type, blocks, and an entry block. Each block contains typed block parameters, instructions defining values, and exactly one terminator: `Return`, `Jump`, or `Branch`. Jump/branch edges carry arguments for the destination block's parameters. Bind edge arguments simultaneously. Represent constants explicitly as value-producing instructions.

The initial frontend and search emit one block. The validator and interpreter must nevertheless support branches and backedges supplied directly as IR; this makes CFG semantics testable before structured source lowering or structural mutation exists.

Validation must reject duplicate or missing IDs, invalid entry blocks, unreachable blocks, use-before-definition, non-dominating uses, bad operand types/arity, invalid edge arguments, non-boolean branch conditions, and return-type mismatches. Canonicalize IDs for serialization. Validation never repairs programs; genome repair is a separate search operation.

### 3.4 Execution outcomes and budgets

```text
Completed(value) | Timeout(reason) | Invalid(diagnostic) | Trap(reason)
```

A step is one executed instruction or terminator. Function-argument setup and block-parameter binding cost zero steps. Check the remaining budget before each step; a return consuming the final allowed step succeeds. Budget zero immediately times out. Invalid IR is rejected before execution.

An interpreter timeout is not an integer observation. Traps, invalid programs, and timeouts cannot match any test case. Host allocation failures or worker failures are run-level infrastructure errors. The no-memory subset has no candidate `OutOfMemory` outcome; add one only with an explicit candidate memory model.

Use a single step budget initially. Later calls require a documented call-depth limit; native/GPU workers require external watchdogs. Search limits do not silently become semantic limits on a compiled replacement.

## 4. Corpus, configuration, and commands

### 4.1 Input and observation encoding

Represent integer values in JSON as fixed-width hexadecimal strings, including signed values encoded as two's-complement patterns. A signature gives their types. This avoids numeric precision loss through JSON consumers.

For hashing and oracle transport, concatenate arguments in signature order, each encoded little-endian at its declared width. Encode the result the same way. Reject wrong counts, widths, or types; never truncate imported data silently.

Target identity includes the signature/contract and an implementation fingerprint: a fixture/build fingerprint initially, and binary plus relevant dependency/environment identities later. A case stores input, expected return value, and a set of provenance records. Deduplicate by input within the same target/contract, retaining all provenance. Conflicting outputs for the same input are an error, not competing fitness labels.

Maintain a sorted, canonical corpus. Its content hash covers the schema, target/contract identity, and sorted input/output pairs. Persist provenance separately; changing provenance alone does not invalidate fitness. Hash complete persisted files separately for integrity. Every fitness result records the corpus content hash.

Seed cases with zero, one, extrema, powers of two, adjacent boundary values, alternating-bit patterns, and seeded random inputs. For multiple arguments, include combinations and interactions without requiring the full Cartesian product. Record counts and seeds; these cases do not establish exhaustive coverage.

### 4.2 Current target adapter

M0–M2 use named, checked-in Rust reference functions as development oracles. Implement them independently of the interpreter and cross-check them with explicit test vectors. These are fixtures, not a claim that binary loading already works.

Search receives only the signature, corpus observations, and explicitly configured operators/constants. It must not inspect fixture implementations or receive their expression trees. Any constants supplied as search hints must be recorded in the configuration and report.

### 4.3 Configuration and CLI

Provide and validate this configuration shape:

```toml
schema_version = 1
seed = 1

[target]
kind = "fixture"
name = "affine_u64"
arguments = ["u64"]
return_type = "u64"

[search]
population = 256
generations = 1000
max_instructions = 12
max_steps = 256
elite = 8
tournament_size = 4
operators = ["add", "mul", "xor"]
constants = ["0u64", "1u64", "3u64", "7u64", "0x12345678u64"]

[corpus]
random_cases = 64
holdout_cases = 256

[output]
directory = "runs/affine"
```

For this fixture, `affine_u64(x) = ((x XOR 0x12345678) * 7) + 3`, using wrapping `u64` arithmetic. Declaring its constants makes the search task explicit; recovering arbitrary large constants without hints is not an acceptance requirement.

Reject unknown fields, unsupported target kinds, empty corpora, mismatched fixture signatures, and invalid limits. Require positive population, generation, instruction, and step limits; `1 <= elite < population` and `1 <= tournament_size <= population`. Record all expanded defaults. Refuse to overwrite an existing run directory unless resuming that run.

```sh
cargo run -p gremlin-cli -- check examples/affine.gremlin
cargo run -p gremlin-cli -- run examples/affine.gremlin --args 0x0000000000000005 --max-steps 256
cargo run -p gremlin-cli -- synthesize --config examples/affine.toml
cargo run -p gremlin-cli -- resume runs/affine/checkpoint.json
```

`check` parses, lowers, and validates. `run` emits a structured execution outcome. `synthesize` and `resume` emit the artifacts below. Write human-readable progress to stderr and machine-readable command results to stdout.

Exit status: `0` for a successful check/run or a synthesis candidate passing the corpus and every requested holdout case; `2` for input/configuration errors; `3` for search-budget exhaustion without a corpus match; `4` for infrastructure errors; `5` for a valid program that traps or times out under `run`; `6` for a holdout counterexample. A deliberately disabled holdout permits an E1-only success; exit zero is never a proof claim.

## 5. CPU search

Use a bounded straight-line genome that lowers to the validated CFG. Operands reference parameters or earlier instructions. Allocate reusable evaluation buffers; do not allocate independently for every candidate/case execution.

Implement seeded initialization, tournament selection, elitism, opcode/operand/constant mutation, instruction insertion/deletion, replacement, and checkpointing. Defer crossover, novelty/lexicase selection, learned proposals, and structural mutation until the baseline is measured.

Initialization may use documented generic seeds such as identity, constant returns, and single-operator expressions. Do not plant composed target solutions. Random generation and mutation must preserve types. After deletion, repair dangling references by selecting the lowest-ID preceding value of the required type; when none exists, reject that mutation. Retry at most eight times, then clone the valid parent. Repair and retry must be deterministic. Never silently mutate a program during validation.

Retain each case's outcome and semantic error. For a completed integer result, error is the popcount of `actual XOR expected`, restricted to the return width. Failed execution has a distinct status, not a fabricated return value.

Rank candidates lexicographically, lower first:

```text
(noncompleted_case_count,
 mismatching_completed_case_count,
 summed_bit_error,
 instruction_count,
 total_executed_steps,
 canonical_program_bytes)
```

A candidate matches the corpus only when every case completes with the expected result. Size and cost must never outweigh a correctness failure. Wall-clock timing is telemetry, not a fitness tie-breaker.

Use a pinned PRNG implementation and serialize its full state. Start single-threaded. Stable corpus order, iteration order, and tie-breaking are required. Resume at completed generation boundaries and reproduce the uninterrupted population/best-candidate sequence for the same executable, semantic version, configuration, and corpus.

When a first corpus match is found, evaluate it on a separate holdout generated using a recorded independent seed. Exclude corpus inputs; if a finite input domain is exhausted, record that fact and the actual case count. M2 records holdout mismatches and returns a failed validation status, but does not feed them into evolution; the later CEGIS milestone closes that loop.

## 6. Evidence and persisted artifacts

Every run writes normalized configuration, corpus/provenance, best candidate source and IR, checkpoint, and `report.json`. A checkpoint includes schema/semantics versions, target/config/corpus hashes, population, current generation, RNG state, best candidate, and fitness. Persist atomically. Reject incompatible resume attempts; never silently continue with a different target or corpus.

Reports contain at least: run/build identity; target and contract identity; seed and expanded configuration; candidate hash; corpus hash/count; backend; evaluation count; generations; runtime; execution-failure counts; holdout seed/count/results; stop reason; evidence; and unsupported/unattempted stages. Record binary/GPU/compiler/solver/fuzzer identities when those stages actually exist. Do not fill unavailable metrics with zero.

Use separate fields for `run_status`, `evidence_level`, and `evidence_scope`. Run status is `completed`, `budget_exhausted`, `counterexample_found`, or `error`. A budget-exhausted run is not equivalence. A failed holdout may leave an E1 corpus match, but the known mismatch must be prominent and the candidate must not be called a successful replacement.

| Level | Minimum evidence |
| --- | --- |
| E0 | All explicitly supplied examples match; record their identity. |
| E1 | The candidate matches the identified current corpus. |
| E2 | E1 plus no mismatch during an independently recorded differential-testing budget. |
| E3 | No counterexample under an explicitly bounded symbolic model; state every bound. |
| E4 | Equivalence established over the full declared input domain under the supported semantic model, without unexplained truncation of execution. |

A candidate not matching even its claimed examples has no achieved level. Keep individual evidence records rather than treating the level as a substitute for their assumptions. Only E4 may be labeled `VERIFIED`; E0–E2 are `TESTED`, and E3 is `BOUNDED`, not full verification.

M0–M2 may report only E0–E2, with `evidence_scope = fixture`. Distinguish later `binary`, `reference_model`, and `native_artifact` evidence. Proof about gremlin IR does not automatically prove a compiled artifact or a supplied binary equivalent.

## 7. Current milestones and acceptance gates

Complete these in order. No later milestone may compensate for a failed earlier gate.

### M0 — Runnable workspace and contract

Deliver the three-crate workspace, CLI shell, strict configuration parsing, versioned serialization, pinned toolchain, CI, and examples.

Acceptance: help and configuration-error paths work; malformed/unknown fields fail usefully; CI builds without deferred-system dependencies. Document the exact local test commands.

### M1 — Executable semantics

Deliver the parser, type checker/lowering, validated CFG, normalized serialization/source printer, and bounded CPU interpreter.

Acceptance:

- Table-driven tests cover every operator at every applicable width, overflow boundaries, signed extrema, zero divisors, `MIN / -1`, `MIN % -1`, and shift counts `0`, `width-1`, `width`, and `width+1`.
- Source-to-IR-to-source round trips preserve behavior. Invalid literals, types, references, dominance, and branch arguments fail validation.
- Direct-IR tests cover branching, simultaneous block-parameter binding, a terminating loop, and an infinite loop stopped at the exact step budget.
- Generated small programs never panic on valid inputs; malformed input is rejected without entering the evaluator. Debug and release builds agree on semantic vectors.

### M2 — Reproducible CPU synthesis

Deliver the search algorithm, fixture adapter, corpus handling, holdout evaluation, checkpoints, and complete reports.

Mandatory synthesis fixtures: identity, increment, XOR with a configured mask, two-argument addition, and one composed expression such as `x * 3 + 1`. Provide fixture-specific operator/constant pools. Use fixed seeds `1`, `2`, and `3` and checked-in budgets no greater than the example's population/generation caps. Each mandatory fixture must reach a corpus match and pass its independent holdout on all three seeds.

This is an acceptance target, not a claim that an unimplemented search will meet it. If it fails, improve the implementation or report the failed gate. Do not relax the fixture, increase caps, or plant its solution silently. The larger `affine_u64` example is a non-gating benchmark in this milestone.

Also require tests showing that a shorter incorrect candidate loses to a correct one; traps/timeouts never count as matches; duplicate provenance survives deduplication; conflicting labels fail; and checkpoint/resume produces identical deterministic state to an uninterrupted run. Exclude timestamps and timing metrics from that comparison.

Run and report:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --release
```

**Stop here for the current assignment.** Deliver a working CPU baseline, not a scaffold claiming complete v1 support.

## 8. Deferred roadmap and gates

These preserve the full-system direction. They are not instructions to implement future stubs. Before starting a gate, record its concrete dependency choices and unresolved semantic decisions in a short design note.

### D1 — Isolated binary oracle and differential CEGIS

Load a configured ELF shared library with an explicit symbol and supported System V integer signature. Extend target configuration with binary path/hash, architecture, format, symbol, ABI, and fixed environment assumptions. Normalize signed arguments and narrow return values according to the declared ABI; test every supported signature type. Do not infer the ABI.

Execute the target in a separate restricted worker with CPU/wall-time/memory limits, disabled network, filesystem restrictions, controlled environment, and crash handling. A child process alone is not the isolation acceptance criterion. Load the library—including constructors—inside the isolation boundary. Test timeout, crash, missing-symbol, forbidden-access, and worker-restart paths. Do not accept arbitrary third-party binaries before this gate passes.

The target must complete without observable side effects over the declared input domain. An observed oracle trap, timeout, inconsistent output, or contract violation aborts the affected run; do not turn it into a desired return value or silently remove that input. Sampled determinism checks support testing but do not prove purity or totality.

CEGIS loop: evolve against the current corpus; test only corpus-matching candidates against independent inputs; replay any counterexample against the oracle; persist it with provenance; change the corpus hash; invalidate old fitness; resume search. Persist progress before budget exhaustion. Solver `Unknown`, `Unsupported`, and `Timeout` never mean equivalence.

Golden integration test: compile the affine example as an ELF library, seed the corpus only with `x = 0`, and submit the constant candidate `0x000000007f6e5d4bu64` for refinement. It matches that case. A differential case such as `x = 1` must refute it and remain in the corpus after resume. Then search for a corrected candidate using the declared constant pool. The injected wrong candidate tests refinement; it must not be confused with the separate test that search can discover a replacement.

### D2 — Extended language and search representation

Add structured `if/else`, `while`, `loop`, `break`, `continue`, and mutable-local lowering to CFG block parameters. Add internal calls/recursion only after specifying call/return behavior, frame storage, and depth exhaustion. These were part of the draft's language direction; they are not implemented by the current straight-line subset.

Add bounded branch/loop genomes and structural mutations with validity and convergence benchmarks. Test nontermination, nested control flow, and later recursive depth exhaustion. Do not use “Turing-complete” as a substitute for an actual supported-feature matrix. Pointers, aggregates, floating point, allocation, and general memory remain out of the v1 target contract.

### D3 — CUDA evaluator

Implement evaluation first, retaining the CPU interpreter as reference. Start with one warp per candidate and one lane per input. Tile corpora larger than 32 cases, handle partial tiles, bound device memory, and batch populations. Keep unsupported features explicit; never silently evaluate different semantics on the GPU.

Gate: CPU/GPU parity for results, traps, and budget exhaustion over fixed edge cases and at least 10,000 seeded generated programs, including divergent CFGs and corpus sizes `1`, `31`, `32`, `33`, and `65`. CUDA absence must not break CPU builds. Required GPU CI must fail or report an explicit unexecuted gate when hardware is unavailable, not silently pass.

Measure transfer/setup costs, end-to-end synthesis time, and candidate-case throughput against CPU. Do not claim acceleration unless measured. Defer GPU mutation/selection and population residency until profiling justifies them.

### D4 — Binary models and formal verification

Begin with fixed-width, straight-line integer functions. Implement candidate bit-vector semantics and a target-model path; support a narrow documented binary instruction subset rather than pretending arbitrary ELF code has been modeled.

For binary-scoped evidence, tie the model to the actual binary hash, decoded function/entry, ABI, lifter version, supported instructions, environmental assumptions, and treatment of flags/control flow/memory. Unsupported instructions, unresolved calls, or uncovered paths produce `Unsupported`, never an unconstrained value or success. A handwritten reference model proves only reference-model equivalence unless its relationship to the binary is independently justified.

Ask whether there exists a valid input for which outcomes differ, including completion/trap behavior—not just return bits. Replay solver counterexamples against concrete evaluators and the original oracle. A replay discrepancy is a modeling error requiring investigation, not a training example to trust blindly.

Return `Equivalent`, `Counterexample`, `Timeout`, `Unsupported`, or `Unknown`, with scope and assumptions. Bound-dependent proofs are E3 unless the bounds are shown complete for the entire declared contract. E4 requires a complete supported model and sufficient termination/coverage justification. Differential tests of a lifter are valuable checks, not a proof that its model is sound.

Gate: equivalent and inequivalent fixtures, replayed counterexamples, unsupported instructions, solver timeout/unknown, and a negative test preventing reference-model evidence from being labeled binary E4. Until that path works, report tested binary results without claiming formal verification.

### D5 — LLVM lowering and optimization

Choose one LLVM lowering route; MLIR is optional, not an additional required backend. Compile only selected candidates after an explicit evidence gate, never every search candidate. A tested artifact requires an explicit requested minimum level and remains labeled tested.

Preserve wrapping arithmetic, defined shift behavior, eager evaluation, traps, and ABI normalization. Do not attach optimization assumptions that contradict gremlin semantics. Test generated artifacts in isolation against both the CPU interpreter and the binary oracle, including holdouts and counterexamples. Record the source/IR/artifact hashes, compiler versions, flags, and separate evidence for each artifact.

Any candidate rewrite invalidates evidence attached to the old candidate hash and must repeat the applicable gate. Optimize size/runtime only subject to correctness. Runtime measurements and compilation success are not proof.

### D6 — External fuzzing and analysis integrations

Do not build a fuzzer. Start with a versioned corpus-import format; imported expected outputs are not trusted without oracle replay. Add a vendor-neutral campaign adapter only after a concrete tool is selected for binary support, licensing, automation, corpus export, and deployment compatibility.

Keep campaign creation/start/stop/poll/export and optional coverage separate from search. Test duplicate import, corrupt cases, unavailable tools, provenance retention, and replay. Similarly separate ELF parsing, disassembly/lifting, optional CFG extraction, constants, and traces; no analysis package defines language semantics.

## 9. Full v1 completion

Declare full v1 complete only when a configured supported ELF target runs under isolation; its corpus survives restart; search generates the documented control-flow-capable candidate subset; CPU and CUDA evaluators agree; CEGIS retains counterexamples; at least one documented binary-model subset supports correctly scoped E4 results; selected candidates lower through LLVM; and native output is differentially checked with an honest machine-readable report.

Publish supported signatures, language features, GPU operations, binary instructions, and verification limitations. Include unsupported-target and resource-failure tests. Report time to an evidence level as the primary research metric, together with evaluation throughput, memory, counterexamples, compilation time, and final runtime/size where measured.

Whole-program equivalence, unknown ABI recovery, external side effects, concurrency, network/filesystem behavior, self-modifying code, general memory, advanced evolutionary methods, and performance claims without measurements are outside this acceptance boundary.
