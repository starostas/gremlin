# gremlin language and execution contract (gremlin-1)

The source subset is straight-line; direct JSON IR also supports CFG branches and loops. The schema version is 1. Types are `bool`, `u8/u16/u32/u64`, and `i8/i16/i32/i64`. Function signatures allow 0–4 integer arguments and an integer return. No implicit conversions, inference, casts, infix expressions, mutable variables, or user calls exist.

```ebnf
function = "fn", identifier, "(", [parameter, {",", parameter}], ")",
           "->", type, "{", {binding}, "return", expression, ";", "}" ;
parameter = identifier, ":", type ;
binding = "let", identifier, ":", type, "=", expression, ";" ;
expression = identifier | literal | operator, "(", [expression, {",", expression}], ")" ;
literal = "true" | "false" | ["-"], decimal_digits, integer_type
        | "0x", hexadecimal_digits, integer_type ;
identifier = (ASCII_letter | "_"), {ASCII_letter | digit | "_"} ;
```

Keywords cannot be identifiers. Bindings are immutable and cannot shadow. Whitespace and `//` line comments are ignored. Decimal values must fit the signed/unsigned range. Negative literals require signed types. Hex literals represent width-limited patterns (for example `0xffi8` is -1). The source limit is 1 MB and expression nesting is at most 128. Source printing uses canonical IDs and hexadecimal constants and round-trips to the same normalized IR; CFG source printing is explicitly unsupported.

All integer values carry a type and a width-limited pattern. `add sub mul` wrap modulo 2^width. `and or xor not` mask results. `udiv urem` require unsigned integers; `sdiv srem` require signed integers and truncate quotient toward zero, with remainder sign matching the dividend. Zero divisors trap, as do both signed MIN/-1 and MIN%-1. `shl lshr ashr rotl rotr` interpret the count as an unsigned pattern modulo width. `ashr` sign-extends the top bit even on unsigned types. Rotations wrap within the declared width.

`eq ne` compare equal-typed patterns (including bool). `ult ule ugt uge` require unsigned integers; `slt sle sgt sge` require signed integers. All comparisons return bool. `select` requires a bool and two equal-typed alternatives. Arguments are evaluated left-to-right, eagerly, including both select alternatives. `not` has one operand, select has three, and all others have two. Integer operands must share a type.

SSA values and blocks have separate ID namespaces. Values have one definition. Instructions must use earlier local definitions or definitions in dominating blocks. Function parameters dominate all blocks. Every block must be reachable, and the entry block has no block parameters. Edges pass equal-typed arguments simultaneously. Normalization numbers blocks in breadth-first edge order from entry and values in parameter/instruction order. Validation rejects malformed IR without repair.

Each executed instruction or terminator costs one step. Argument setup and edge binding are free. The budget is checked before each step; return on the final step succeeds. Zero immediately times out after input validation. Outcomes are completed, trap, timeout, or invalid. These limits affect interpreter observations, not a future compiled function's semantics. Reusable evaluator register and edge buffers avoid per-case allocations on completed execution.

Corpus values use exact-width `0x` hexadecimal strings, signed values included. Transport encodes each argument in signature order, little-endian at its width; results use the same representation. Corpus identity includes schema, fixture implementation fingerprint, signature, contract, and sorted observations. Provenance is separate from the content hash. Only matching completed results count as observations.

The corpus content hash is SHA-256 over the `gremlin-corpus-content-v1\0` prefix, the u64 little-endian byte length of a canonical JSON `[schema_version, target_identity]` header, that header, a u64 little-endian case count, and each sorted case's concatenated argument bytes followed by its return bytes. Argument/result encodings are fixed by the signature, so observation boundaries are unambiguous. Provenance is excluded; whole artifact file hashes are recorded separately.
