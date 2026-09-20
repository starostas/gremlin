# Using Gremlin

Run commands from the repository root. The examples below use `target/release/gremlin`, built with:

```sh
cargo build --release --locked -p gremlin-cli
```

The CPU build needs Rust 1.90.0 and a system linker. Other dependencies depend on what you run:

| Task | Additional requirements |
| --- | --- |
| Interpret source or synthesize against fixtures | None |
| Observe a compiled target | Linux x86-64, Bubblewrap, working namespaces and seccomp |
| Evaluate candidates on GPU | CUDA toolkit for the build; a supported NVIDIA GPU at runtime |
| Prove equivalence | Z3 and a supported model |
| Compile a candidate | Clang 18; binary-oracle prerequisites for validation |
| Run a fuzzing campaign | Clang 18 and compiler-rt/libFuzzer; binary-oracle prerequisites |

For binary isolation setup, see [D1](design/D1.md). GPU setup and measurements are in [D3](design/D3.md).

## Run an existing program

```sh
target/release/gremlin check examples/affine.gremlin
target/release/gremlin run examples/affine.gremlin \
  --args 0x0000000000000005 --max-steps 256
```

The result is JSON:

```json
{"status":"completed","steps":7,"type":"u64","value":"0x000000007f6e5d6e"}
```

Arguments are comma-separated, fixed-width hexadecimal values. A `u8` argument uses two hex digits; a `u64` uses sixteen. Signed arguments use their two’s-complement bit pattern. Functions accept zero to four integer arguments and return one integer. Strings, pointers, arrays and floating point are not direct inputs.

Gremlin source is its own language, not Rust. It supports integer operations, mutable locals, branches, loops and bounded module calls. See [language semantics](semantics.md), [control flow](../examples/control_flow.gremlin) and [recursion](../examples/recursion.gremlin). `run` defaults to 256 execution steps and a call-depth limit of 64; use `--max-steps` and `--max-call-depth` to change them.

## Search for a program

```sh
target/release/gremlin synthesize --config tests/fixtures/composed_u64.toml
```

This example searches for a program matching `x * 3 + 1` with wrapping 64-bit arithmetic. The search receives observations, a type signature, an operator pool and constant hints. It does not receive the fixture’s expression.

Copy a fixture configuration to customize a search. The main settings are:

| Setting | Meaning |
| --- | --- |
| `seed` | Reproducible random sequence |
| `search.population` | Candidates in each generation |
| `search.generations` | Maximum generations |
| `search.max_instructions` | Candidate size limit |
| `search.max_steps` | Execution limit per candidate/input pair |
| `search.operators`, `search.constants` | Operations and constants available to search |
| `corpus.random_cases` | Random observations added to the boundary corpus |
| `corpus.holdout_cases` | Fresh observations used to check a corpus match |
| `output.directory` | A new directory for this run |

A larger budget allows more attempts; it does not guarantee a solution. In the [CRC experiment](../examples/cksum-crc/README.md), increasing the default-ranking budget from 300 to 3,000 generations left the result unchanged.

The default ranking favors exact output matches. To prioritize the number of wrong bits instead, add:

```toml
[search.comparator]
kind = "bit_error_first"
```

You can also write a scoring function. See [fitness comparators](comparators.md) for the API and examples. Scores guide selection; they do not replace exact correctness checks.

Output paths are relative to the working directory. Gremlin refuses to overwrite an existing run directory. Progress goes to stderr and the final summary goes to stdout as JSON.

## Read the results

| File | Contents |
| --- | --- |
| `best.gremlin` | Best candidate at the end of the run |
| `best.ir.json` | The same candidate as normalized IR |
| `report.json` | Outcome, scores, validation results, timings and configuration |
| `checkpoint.json` | Saved population, random state and integrity information |
| `config.json` | Normalized configuration |
| `corpus.json` | Observed input/output pairs |
| `provenance.json` | Where the observations came from |
| `integrity.json` | Checksums of completed run artifacts |

`best.gremlin` can be an incorrect candidate when the budget runs out. Check the report’s `run_status` and `evidence_level` before using it.

- **E1:** matches the training corpus.
- **E2:** also passes fresh holdout inputs.
- **E4:** an equivalence proof within the report’s stated model and assumptions.

E1 and E2 are test results. A proof against a reference model is labeled `reference_model`; it is not a proof against the original binary. Native compiled artifacts receive their own validation results.

Search checkpoints are written after each completed generation. Current builds store compact population fitness totals; the final report includes detailed case results for the best candidate. A stopped process may have a recent checkpoint without final source or report files.

## Limit CPU use

On Linux, `taskset` can restrict a search and its child processes to one logical CPU. Choose a CPU from the process’s allowed set rather than assuming CPU 0 is available:

```sh
gremlin_cpu=$(python3 -c 'import os; print(min(os.sched_getaffinity(0)))')
taskset -c "$gremlin_cpu" target/release/gremlin synthesize \
  --config tests/fixtures/composed_u64.toml
```

The CPU search loop is single-threaded already. Affinity prevents it and its helpers from using other cores; it can still use 100% of the selected core. It does not impose a GPU limit.

## Pause, stop and resume

To pause a running search on Linux, use its PID:

```sh
kill -STOP <search-pid>
```

This keeps the process in memory without running it. Continue that same process with:

```sh
kill -CONT <search-pid>
```

To stop a foreground search, press Ctrl-C. The latest completed generation remains in `checkpoint.json`; work since that checkpoint is lost. After confirming the process has exited, remove its stale `.running` file if present, then resume:

```sh
rm runs/composed_u64/.running  # Only if present and its process has exited.
target/release/gremlin resume runs/composed_u64/checkpoint.json
```

Use the same executable build and working directory as the original search. Resume checks the configuration, target, corpus and checkpoint integrity, then recomputes fitness before continuing. Keep a copy of the original executable if you plan to rebuild while a run is stopped. Do not edit the saved configuration to increase the budget; create a new run with the larger budget instead.

To resume on one logical CPU:

```sh
gremlin_cpu=$(python3 -c 'import os; print(min(os.sched_getaffinity(0)))')
taskset -c "$gremlin_cpu" target/release/gremlin resume \
  runs/composed_u64/checkpoint.json
```

Deleting `.running` while its process is active allows competing writers and can damage the run. Its contents identify the process that acquired the lock; verify the PID still belongs to that search before sending signals or removing the lock.

## Use a compiled target

A binary target is a Linux x86-64 ELF shared library exposing a named System V integer function. Gremlin needs its path, SHA-256, symbol and argument/return types. Arbitrary command-line programs with files and printed output are outside this interface.

The affine example builds a shared library, emits a configuration and supplies a deliberately wrong starting candidate:

```sh
python3 examples/build_binary.py
target/release/gremlin refine \
  --config runs/binary-example-input/affine.toml \
  --candidate runs/binary-example-input/wrong.gremlin
```

Refinement checks the candidate, retains counterexamples and searches for a replacement. See the [binary workflow](design/D1.md) and [supported target contract](support.md).

For a small proof example with a known candidate:

```sh
python3 examples/build_formal.py
target/release/gremlin verify \
  --config runs/formal-example/affine.toml \
  --candidate runs/formal-example/candidate.gremlin \
  --solver /usr/bin/z3 --timeout-ms 10000 \
  --output runs/formal-example/proof.json
```

The proof output path must be new. The binary proof model supports a narrow straight-line instruction subset; a target can be executable by the oracle but unsupported by the verifier. See [formal verification](design/D4.md).

Further workflows: [native compilation](design/D5.md), [corpus import and fuzzing](design/D6.md), and [CUDA evaluation](design/D3.md).

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Command succeeded or requested checks passed |
| 2 | Invalid input or configuration |
| 3 | Search budget exhausted without a corpus match |
| 4 | Infrastructure failure |
| 5 | Program execution trapped or timed out |
| 6 | Holdout counterexample found |

Use the JSON report for verification-specific outcomes such as unsupported models and solver timeouts.
