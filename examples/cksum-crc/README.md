# A cksum CRC synthesis experiment

This example uses an independent implementation of the POSIX `cksum` CRC recurrence, with polynomial `0x04c11db7`. It is an integer-function experiment, not a translation of the coreutils executable. GNU describes the CRC and command behavior in its [cksum manual](https://www.gnu.org/software/coreutils/manual/html_node/cksum-invocation.html).

The target shared library exports:

- `crc_byte(state: u32, byte: u32) -> u32`: consume the low eight bits of `byte` and update the CRC state. Both parameters are u32 because Gremlin does not currently expose integer-width conversions.
- `crc_feedback(state: u32) -> u32`: return the polynomial when the state's high bit is set, otherwise zero.

## Reproduce

Use Linux x86-64 with the project's working Bubblewrap isolation prerequisites, a C compiler, Clang 18, Z3 and GNU `cksum`:

```sh
cargo build --release --locked -p gremlin-cli
python3 examples/cksum-crc/demo.py --output runs/cksum-crc-demo
```

The output directory must be fresh. Use `--gremlin /path/to/gremlin` to select another built executable. Both searches use population 256 and default to a 300-generation limit. Increase the full byte-update budget with `--byte-generations`, for example:

```sh
python3 examples/cksum-crc/demo.py --output runs/cksum-crc-larger --byte-generations 3000
```

The feedback subproblem retains its 300-generation limit. The generic enumeration/mutation search receives operator and constant pools, including the CRC polynomial; it is not given the target source expression or a seeded correct candidate. The polynomial itself is not discovered.

The script builds the isolated binary target, runs both searches, inlines the discovered feedback into an explicitly constructed eight-round byte update, proves equivalence to handwritten Gremlin references, compiles a native artifact, and runs differential checks. Logs, configurations, source, checkpoints, proof queries and native outputs remain in the output directory. `summary.json` records both successes and failures.

## Measured result

Seed 1 on the development host:

| Experiment | Result |
| --- | --- |
| Whole byte-update search | Failed to converge within 300 generations; no correctness evidence |
| Feedback search | Converged at generation 44; 256 fresh holdouts passed |
| Feedback and assembled byte-update proofs | E4 **reference-model** equivalence, not binary-scoped proofs |
| Compiled byte-update artifact | E2, tested against the isolated target on 448 corpus and 256 holdout inputs |
| Interpreter vs independent bitwise recurrence | 1,280 input pairs passed |
| Assembled checksum vs GNU coreutils 9.4 `cksum` | 261 messages passed: every single-byte message, empty input, text and a 256-byte message |

The search produced the feedback implementation in [discovered-feedback.gremlin](discovered-feedback.gremlin). Its useful expression is equivalent to:

```text
polynomial & ashr(ashr(state, polynomial), polynomial)
```

Gremlin masks shift amounts modulo 32. Here the polynomial's low five bits equal 23, so two arithmetic shifts by 23 turn the original high bit into an all-zero or all-one mask. The generated program also contains unused calculations; the checked-in output is preserved without manual simplification.

The eight-round structure and initial byte mixing are provided by the harness. File-length folding and final bitwise complement are provided by Python. Only the feedback body was synthesized successfully. The assembled byte update is also available as [assembled-byte.gremlin](assembled-byte.gremlin).

For the nine bytes `123456789`, the assembled implementation produces `930766865`, matching `printf 123456789 | cksum`. The raw one-byte update alone is not a complete POSIX checksum: length folding and complement matter.

See [measured summary](../../docs/measurements/cksum-crc.json). Runtime varies by machine; these are bounded-search results, not a claim that arbitrary CRC implementations can currently be synthesized end to end.
