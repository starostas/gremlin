---
title: How it works
description: An approachable overview of gremlin's program-search loop.
---

gremlin works in three connected stages.

## 1. Describe a small program

The gremlin language is intentionally narrow: functions take up to four integer arguments, use explicit types, and work with fixed-width integer arithmetic. It supports mutable locals, branches, loops, and bounded calls/recursion; arithmetic wraps at the chosen integer width, so a `u8` program behaves like an eight-bit machine value.

## 2. Observe behavior

The getting-started workflow uses checked-in Rust fixtures to provide known input/output examples. These examples form a corpus: the target behavior that candidate programs are trying to match. Supported Linux x86-64 ELF targets can also be observed in an isolated, opt-in workflow.

## 3. Search and check

The search engine creates and mutates short candidate programs, runs them against the corpus, and ranks them by correctness before size or cost. It can generate bounded control-flow graphs when configured. When a candidate matches the corpus, gremlin also checks it against a separately generated holdout set; an opt-in binary workflow can retain counterexamples and refine the search.

Every run records the configuration, observed data, best candidate, and a report. This makes the result reproducible: rerunning with the same tool version, configuration, and seed follows the same search path.

## Why the limits matter

The project is deliberately bounded, so it is not a general binary decompiler. Ordinary search results are tested evidence rather than proofs. The optional verifier can make a stronger claim only for its documented straight-line ELF register subset and assumptions.
