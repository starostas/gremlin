---
title: Project status
description: What the current CPU baseline supports and what remains future work.
---

## Available now

The current CPU baseline can parse and execute typed integer programs, validate their control-flow representation, search for small straight-line candidates, and save reproducible run reports and checkpoints.

It includes examples and fixture configurations for identity, increment, XOR, addition, and a composed arithmetic function. These are useful for exercising the workflow end to end.

## Important boundaries

- Search results are tested against examples and holdout inputs, not formally proven equivalent.
- Source programs are straight-line; the project does not currently accept source-level loops or branches.
- The target adapters are checked-in Rust fixtures, not arbitrary binaries.
- Runs are CPU-only and single-threaded.

## Future directions

Potential future work includes binary targets, counterexample-guided refinement, GPU evaluation, formal verification, native-code output, and broader program structures. Those are not part of the current baseline, so the docs avoid presenting them as available features.
