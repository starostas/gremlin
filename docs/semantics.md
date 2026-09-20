# Language reference

Gremlin is a small language for fixed-width integer programs. It has a readable source form and a schema-version-1 JSON IR; both can represent branches and loops. Source modules can also make typed internal calls and recurse.

## At a glance

| Area | Contract |
| --- | --- |
| Values | `bool`, `u8/u16/u32/u64`, and `i8/i16/i32/i64` |
| Function interface | Zero to four integer arguments and one integer return value |
| Expressions | Prefix calls with explicit types—no implicit conversions, inference, casts, or infix expressions |
| Control flow | `if`, `while`, `loop`, `break`, and `continue` |
| Execution | A bounded, deterministic step budget and call-depth limit |

## A small program

    fn candidate(x: u64) -> u64 {
        let a: u64 = xor(x, 0x12345678u64);
        let b: u64 = mul(a, 7u64);
        return add(b, 3u64);
    }

Every operation is written as a call, and every binding declares its type. See the [affine example](../examples/affine.gremlin) in the repository for the complete file.

## Explore the reference

- [Types and literals](language/types-and-literals.md) explains values, names, and source limits.
- [Operators](language/operators.md) covers fixed-width arithmetic, traps, and eager evaluation.
- [Control flow](language/control-flow.md) introduces bindings, loops, and the source grammar.
- [Functions and calls](language/functions.md) covers entry points, recursion, and backend limits.
- [Execution model](language/execution.md) defines steps, outcomes, and call-depth behavior.
- [IR and corpus format](language/ir-and-corpus.md) documents validation, canonical source, and observation identity.
