# Run a program

## Check and execute

Check a source file before running it:

    target/release/gremlin check examples/affine.gremlin

Then execute it with fixed-width hexadecimal arguments:

    target/release/gremlin run examples/affine.gremlin --args 0x0000000000000005 --max-steps 256

The result is JSON:

    {"status":"completed","steps":7,"type":"u64","value":"0x000000007f6e5d6e"}

## Arguments and result JSON

Arguments are comma-separated, fixed-width hexadecimal values. A `u8` argument uses two hex digits; a `u64` uses sixteen. Signed arguments use their two’s-complement bit pattern.

Functions accept zero to four integer arguments and return one integer. Strings, pointers, arrays, and floating-point values are not direct inputs.

Gremlin source is its own language, not Rust. It supports integer operations, mutable locals, branches, loops, and bounded module calls. See the [language reference](semantics.md), [control-flow example](../examples/control_flow.gremlin), and [recursion example](../examples/recursion.gremlin).

## Execution limits

`run` defaults to 256 execution steps and a call-depth limit of 64. Use `--max-steps` and `--max-call-depth` to change those limits. The [execution model](language/execution.md) explains how Gremlin accounts for steps and call depth.
