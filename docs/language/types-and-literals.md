# Types and literals

## Value types

| Type family | Types |
| --- | --- |
| Boolean | `bool` |
| Unsigned integers | `u8`, `u16`, `u32`, `u64` |
| Signed integers | `i8`, `i16`, `i32`, `i64` |

Every integer value carries its declared type and a width-limited bit pattern. Function signatures accept zero to four integer arguments and return one integer; `bool` is available inside the language for conditions and comparisons, not as a public function argument or return type.

Gremlin has no implicit conversions, type inference, casts, or infix expressions. Write the operation and its arguments explicitly, such as `add(x, 1u64)`.

## Literals

Use `true` and `false` for booleans. Integer literals carry a type suffix:

    let a: u64 = 42u64;
    let b: i8 = -1i8;
    let c: i8 = 0xffi8;

Decimal values must fit the declared signed or unsigned range, and negative decimal literals require a signed type. Hexadecimal literals represent width-limited patterns, so `0xffi8` represents `-1`.

## Names and bindings

An identifier starts with an ASCII letter or underscore and may continue with ASCII letters, digits, or underscores. Keywords cannot be identifiers.

Bindings are immutable unless declared with `let mut`. Names cannot shadow an existing name, and assignments must retain the binding’s original type. See [control flow](control-flow.md) for examples of mutable bindings and loops.

## Source text and limits

Whitespace and `//` line comments are ignored. Source files are limited to 8 MB, and expression nesting is limited to 128 levels.
