# Synthesize a program

## Run the fixture

Start with the included fixture:

    target/release/gremlin synthesize --config tests/fixtures/composed_u64.toml

This search looks for a program matching `x * 3 + 1` with wrapping 64-bit arithmetic. It receives observations, a type signature, an operator pool, and constant hints; it does not receive the fixture’s expression.

## Configure a search

Copy a fixture configuration to customize a search. These settings are the usual starting points:

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

## Rank candidates

The default ranking favors exact output matches. To prioritize the number of wrong bits instead, add:

    [search.comparator]
    kind = "bit_error_first"

You can also write a scoring function. See [fitness comparators](comparators.md) for the API and examples. Scores guide selection; they do not replace exact correctness checks.

## Run output

Output paths are relative to the working directory. Gremlin refuses to overwrite an existing run directory. Progress goes to stderr and the final summary goes to stdout as JSON.

When a run completes—or is stopped—use [Inspect and continue a search](results-and-resume.md) to understand its artifacts and evidence.
