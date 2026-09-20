# Execution model

## Step accounting

| Event | Cost |
| --- | --- |
| Executed instruction or terminator | One step |
| Call | One step |
| Callee return | One step |
| Argument setup or edge binding | Free |

The budget is checked before each step. A return on the final available step succeeds; a zero step budget times out immediately after input validation.

## Outcomes and backends

An execution is completed, trapped, timed out, or invalid. CPU, CUDA, and LLVM implementations preserve the same configured step budget.

## Call depth

One global step budget covers the whole call tree. Entry depth is one.

- The supported maximum call depth is 1,024.
- A zero call depth times out after validation.
- An attempted call at the maximum depth consumes its call step and yields a depth timeout.
- CLI `run` defaults to a call-depth limit of 64 and uses explicit heap frames rather than host recursion.

For the language-level call rules and backend restrictions, see [Functions and calls](functions.md).
