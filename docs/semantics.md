# gremlin language and execution contract (gremlin-2)

Source and JSON IR support CFG branches and loops; source modules also support typed internal calls and recursion. The schema version is 1. Types are `bool`, `u8/u16/u32/u64`, and `i8/i16/i32/i64`. Function signatures allow 0–4 integer arguments and an integer return. There are no implicit conversions, inference, casts or infix expressions.

```ebnf
module = function, {function} ;
function = "fn", identifier, "(", [parameter, {",", parameter}], ")",
           "->", type, "{", {statement}, "}" ;
parameter = identifier, ":", type ;
statement = "let", ["mut"], identifier, ":", type, "=", expression, ";"
          | identifier, "=", expression, ";"
          | "return", expression, ";"
          | "if", expression, "{", {statement}, "}", ["else", "{", {statement}, "}"]
          | "while", expression, "{", {statement}, "}"
          | "loop", "{", {statement}, "}"
          | "break", ";" | "continue", ";" ;
expression = identifier | literal | identifier, "(", [expression, {",", expression}], ")" ;
literal = "true" | "false" | ["-"], decimal_digits, integer_type
        | "0x", hexadecimal_digits, integer_type ;
identifier = (ASCII_letter | "_"), {ASCII_letter | digit | "_"} ;
```

Keywords cannot be identifiers. Bindings are immutable unless declared `let mut`; names cannot shadow. Assignments require the original type. Branches and loops lower carried locals to block parameters. Break/continue target the nearest loop. Unreachable statements are rejected. Whitespace and `//` line comments are ignored. Decimal values must fit the signed/unsigned range. Negative literals require signed types. Hex literals represent width-limited patterns (for example `0xffi8` is -1). The source limit is 1 MB and expression nesting is at most 128. Source printing uses canonical IDs and hexadecimal constants and round-trips to the same normalized IR; general CFGs print with explicit `block`, `jump` and `branch` syntax to preserve edge bindings and exact step counts.

All integer values carry a type and a width-limited pattern. `add sub mul` wrap modulo 2^width. `and or xor not` mask results. `udiv urem` require unsigned integers; `sdiv srem` require signed integers and truncate quotient toward zero, with remainder sign matching the dividend. Zero divisors trap, as do both signed MIN/-1 and MIN%-1. `shl lshr ashr rotl rotr` interpret the count as an unsigned pattern modulo width. `ashr` sign-extends the top bit even on unsigned types. Rotations wrap within the declared width.

`eq ne` compare equal-typed patterns (including bool). `ult ule ugt uge` require unsigned integers; `slt sle sgt sge` require signed integers. All comparisons return bool. `select` requires a bool and two equal-typed alternatives. Arguments are evaluated left-to-right, eagerly, including both select alternatives. `not` has one operand, select has three, and all others have two. Integer operands must share a type.

SSA values and blocks have separate ID namespaces. Values have one definition. Instructions must use earlier local definitions or definitions in dominating blocks. Function parameters dominate all blocks. Every block must be reachable, and the entry block has no block parameters. Edges pass equal-typed arguments simultaneously. Normalization numbers blocks in breadth-first edge order from entry and values in parameter/instruction order. Validation rejects malformed IR without repair.

Each executed instruction or terminator costs one step. Argument setup and edge binding are free. The budget is checked before each step; return on the final step succeeds. Zero immediately times out after input validation. Outcomes are completed, trap, timeout, or invalid. The CPU, CUDA and LLVM implementations preserve the same configured step budget. Reusable evaluator register and edge buffers avoid per-case allocations on completed execution.

Corpus values use exact-width `0x` hexadecimal strings, signed values included. Transport encodes each argument in signature order, little-endian at its width; results use the same representation. Corpus identity includes schema, target implementation fingerprint, signature, contract, and sorted observations. Provenance is separate from the content hash. Only matching completed results count as observations.

The corpus content hash is SHA-256 over the `gremlin-corpus-content-v1\0` prefix, the u64 little-endian byte length of a canonical JSON `[schema_version, target_identity]` header, that header, a u64 little-endian case count, and each sorted case's concatenated argument bytes followed by its return bytes. Argument/result encodings are fixed by the signature, so observation boundaries are unambiguous. Provenance is excluded; whole artifact file hashes are recorded separately.

Modules enter `main` when defined, otherwise their first declared function. Call arguments are eager and types must match a declared function signature. Calls cost one step; callee returns cost one step. One global step budget covers the whole call tree. Entry depth is 1. An attempted call at the maximum depth consumes its call step and yields a depth timeout. A zero depth times out after validation. The supported depth range ends at 1024; CLI `run` defaults to 64 and uses explicit heap frames rather than host recursion. GPU, formal and LLVM backends currently reject internal calls.
