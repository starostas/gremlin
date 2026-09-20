# What Gremlin is

Gremlin searches for a small program that reproduces the behaviour you can observe from another one. You give it inputs and the outputs they should produce; it evolves candidate programs, scores them against those examples, and returns the best one as Gremlin source you can read.

This is **program synthesis, not decompilation**. The result does not have to resemble whatever produced the original behaviour, and usually does not. A recovered colour transform may use the same three operations in the same order, or reach the same answer by an entirely different route. Both count, because the only thing being matched is behaviour.

## What it works on

The contract is deliberately narrow, and the narrowness is what makes the search tractable:

- Functions take **0–4 integer arguments** and return **one integer**.
- Widths are **8, 16, 32 or 64 bits**, signed or unsigned. Booleans exist inside a program but never at its edges.
- **27 operators**: arithmetic, bitwise, shifts and rotates, comparisons, and a `select`.

Memory, pointers, floats, allocation, concurrency, external calls and I/O are all outside the contract. Not "unimplemented" — outside it. A function that touches any of them is not a target Gremlin can accept.

## What a result actually means

Gremlin distinguishes between a program that *passed the tests it was given* and a program that is *proven correct*, and it never conflates them.

A normal search result is **tested**: it matches the training examples and an independent holdout set. That is evidence, not proof, and a page that reports one says so. Every experiment on this site shows its holdout numbers for that reason.

Separately, a candidate can be checked against a formal model with an SMT solver, and it can be compiled through LLVM and measured as a native artifact. These are different claims about different things. In particular, proving something about a reference model does not prove anything about a binary you started from — that gap is real, and the documentation is explicit about it rather than papering over it.

## How the search runs

The search itself is deterministic and runs on CPU; the same seed gives the same run. A CUDA device can evaluate candidates in parallel, which is what the experiments here use, but **CUDA is an evaluator, not a search backend** — it scores candidates, it does not decide anything. On small workloads the GPU is measurably *slower* end to end than the CPU, because there is not enough work to cover the setup.

Where a binary is the source of truth rather than a table of examples, Gremlin can call into a Linux x86-64 ELF function directly, inside an empty mount, network, user and PID namespace with resource limits and a default-deny seccomp filter. There is no reduced-isolation fallback: if the sandbox cannot be established, the run fails.

## Where to go next

The [experiments](/experiments/) are the fastest way to see what this looks like in practice — each one runs a bounded search and shows you the program it found beside the thing it was matching. [Getting started](/usage/) builds the CLI and runs a first search locally. The [language reference](/semantics/) defines the semantics that the interpreter, the GPU evaluator, the solver encoding and the native backend all have to agree on, and [supported features](/support/) is the precise statement of what is and is not in the contract.
