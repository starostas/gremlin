# Inspect and continue a search

## Read the result

Each run directory contains the candidate and the evidence needed to evaluate it:

| File | Contents |
| --- | --- |
| `best.gremlin` | Best candidate at the end of the run |
| `best.ir.json` | The same candidate as normalized IR |
| `report.json` | Outcome, scores, validation results, timings, and configuration |
| `checkpoint.json` | Saved population, random state, and integrity information |
| `config.json` | Normalized configuration |
| `corpus.json` | Observed input/output pairs |
| `provenance.json` | Where the observations came from |
| `integrity.json` | Checksums of completed run artifacts |

`best.gremlin` can be an incorrect candidate when the budget runs out. Check the report’s `run_status` and `evidence_level` before using it.

## Evidence levels

- **E1:** matches the training corpus.
- **E2:** also passes fresh holdout inputs.
- **E4:** an equivalence proof within the report’s stated model and assumptions.

E1 and E2 are test results. A proof against a reference model is labeled `reference_model`; it is not a proof against the original binary. Native compiled artifacts receive their own validation results.

Search checkpoints are written after each completed generation. Current builds store compact population fitness totals; the final report includes detailed case results for the best candidate. A stopped process may have a recent checkpoint without final source or report files.

## Limit CPU use

On Linux, `taskset` can restrict a search and its child processes to one logical CPU. Choose a CPU from the process’s allowed set rather than assuming CPU 0 is available:

    gremlin_cpu=$(python3 -c 'import os; print(min(os.sched_getaffinity(0)))')
    taskset -c "$gremlin_cpu" target/release/gremlin synthesize --config tests/fixtures/composed_u64.toml

The CPU search loop is already single-threaded. Affinity prevents it and its helpers from using other cores; it can still use 100% of the selected core. It does not impose a GPU limit.

## Pause, stop, and resume

To pause a running search on Linux, send `STOP` to its PID:

    kill -STOP <search-pid>

This keeps the process in memory without running it. Continue that same process with:

    kill -CONT <search-pid>

To stop a foreground search, press Ctrl-C. The latest completed generation remains in `checkpoint.json`; work since that checkpoint is lost. After confirming the process has exited, remove its stale `.running` file if present, then resume:

    rm runs/composed_u64/.running  # Only if present and its process has exited.
    target/release/gremlin resume runs/composed_u64/checkpoint.json

Use the same executable build and working directory as the original search. Resume checks the configuration, target, corpus, and checkpoint integrity, then recomputes fitness before continuing. Keep a copy of the original executable if you plan to rebuild while a run is stopped. Do not edit the saved configuration to increase the budget; create a new run with the larger budget instead.

To resume on one logical CPU:

    gremlin_cpu=$(python3 -c 'import os; print(min(os.sched_getaffinity(0)))')
    taskset -c "$gremlin_cpu" target/release/gremlin resume runs/composed_u64/checkpoint.json

Deleting `.running` while its process is active allows competing writers and can damage the run. Its contents identify the process that acquired the lock; verify the PID still belongs to that search before sending signals or removing the lock.

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
