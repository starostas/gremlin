---
title: Project status
description: What gremlin supports today and the boundaries of each workflow.
---

## Available now

The default CPU workflow can parse and execute typed integer programs, validate their control-flow representation, search for small candidates (including bounded CFGs when configured), and save reproducible reports and checkpoints.

It includes fixture configurations for identity, increment, XOR, addition, bounded sums, and composed arithmetic. These are useful for exercising the workflow end to end.

Additional capabilities are available with explicit prerequisites:

- Isolated observation and counterexample-guided refinement for a supported Linux x86-64 ELF target.
- CUDA evaluation when built with the optional feature and run on a supported NVIDIA device.
- Formal equivalence checks for a deliberately narrow, straight-line ELF register subset.

## Important boundaries

- Most search results are tested against examples and holdout inputs, not formally proven equivalent.
- Binary observation is restricted to a documented ABI and isolation environment; it is not support for arbitrary binaries.
- CUDA is opt-in, and CPU remains the default evaluation path.
- The formal verifier is intentionally limited to its documented binary subset and assumptions.
- Search is bounded and single-threaded, and it is not guaranteed to recover arbitrary programs or unknown constants.

## Future directions

Potential future work includes LLVM/native-code output and external fuzzing. The project documentation keeps these distinct from the implemented workflows above.
