# Fitness comparators

A comparator guides which candidates survive and reproduce. It does not redefine correctness. Corpus success still requires every candidate execution to complete and every returned bit to equal the oracle. Holdout validation, binary proofs and native-artifact checks remain exact.

Configure a comparator in the same TOML file as the search. Omitting it preserves the original ranking and deterministic search trajectory.

```toml
[search.comparator]
kind = "correctness_first"
```

This default ranks candidates by execution failures, exact mismatches, summed bit errors, instruction count, executed steps, and canonical program bytes. For bit-oriented functions, try:

```toml
[search.comparator]
kind = "bit_error_first"
```

This ranks by execution failures, whether the entire corpus matches, summed bit errors, then the default tie breakers. It can prefer a candidate that gets fewer complete outputs right but has fewer wrong bits overall. Both rankings always prefer a fully correct candidate and penalize incomplete executions before their scores.

## Custom scoring functions

Use a bounded Gremlin function with signature `(actual: u64, expected: u64) -> u64`. Lower scores are better. The function receives the target's raw integer bit patterns, zero-extended to 64 bits: for example, target `i8` value -1 is passed as `0x00000000000000ff`. Signed interpretation must be explicit in the scoring function. Inputs to the target itself are not passed to the scorer.

For example, rank by unsigned absolute distance:

```toml
[search.comparator]
kind = "gremlin"
max_steps = 32
source = """
fn score(actual: u64, expected: u64) -> u64 {
    return select(uge(actual, expected),
                  sub(actual, expected),
                  sub(expected, actual));
}
"""
```

The scorer runs once per completed candidate/corpus case. Its unsigned scores are summed using checked 128-bit arithmetic, then minimized. Ties use the default ranking. A fully exact candidate is always preferred, even if a custom scorer assigns it a worse score. A scorer returning zero for everything cannot make a wrong candidate pass. Traps, timeouts, or invalid scorer execution abort the search with an explicit error and no new correctness evidence; they are not silently treated as poor candidate scores.

Custom source supports the same integer operations and bounded CFGs as ordinary Gremlin functions. Internal calls are excluded. Limits: source at most 64 KiB, at most 64 blocks and 4,096 instructions, and `max_steps` from 1 to 10,000 for each scoring invocation. Scorer steps are independent of the candidate's `search.max_steps`. Custom scoring uses the CPU interpreter even when candidates are evaluated on CUDA, so expensive scorers can dominate runtime. The built-in Hamming comparator avoids that interpreter overhead.

[examples/comparators/hamming.gremlin](../examples/comparators/hamming.gremlin) is a custom Hamming-distance implementation. Embed its contents in `source` with `max_steps = 64` to reproduce the built-in bit-error ordering. It is tested against the built-in comparator across evolving populations.

## Checkpoints and reports

The normalized configuration, checkpoint, and final report include the comparator kind, source and step budget. These are covered by the existing configuration/checkpoint integrity hashes. Resume requires the same configuration and executable; it recomputes fitness and comparator scores before continuing. Counterexample refinement recomputes scores on the expanded corpus. Editing the persisted comparator configuration is rejected.

`best_fitness.selection_cost` is absent for the default comparator. Otherwise it records the unsigned 128-bit sum as `[high_64_bits, low_64_bits]`, avoiding floating-point serialization or overflow. For custom scorers, each completed case also records its `custom_cost`; failed candidate cases have no custom cost. Existing mismatch and bit-error fields remain available independently of the selected ranking. Compare fitness values only within the same run configuration and corpus.

Validation covers changed ordering, exact-success priority, candidate failures, maliciously unhelpful scores, sums above 64 bits, bounded scorer failures, strict configuration, signed bit patterns, deterministic resume, corpus regrading, configuration tampering, CLI error evidence, and CPU/CUDA custom-score state parity.
