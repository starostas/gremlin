# Control flow

Gremlin source uses structured control flow. The following program accumulates the integers from its input down to one:

    fn main(x: u8) -> u8 {
        let mut n: u8 = x;
        let mut sum: u8 = 0u8;
        while ne(n, 0u8) {
            sum = add(sum, n);
            n = sub(n, 1u8);
        }
        return sum;
    }

See the [full control-flow example](../../examples/control_flow.gremlin) in the repository.

## Bindings and loops

Use `let` for an immutable binding and `let mut` for a binding that will be reassigned. Assignments keep the original type; names cannot shadow one another.

`if`, `while`, and `loop` introduce structured control flow. `break` and `continue` target the nearest loop. Unreachable statements are rejected.

Branches and loops lower carried locals to block parameters in the normalized IR. For the underlying rules, see [IR and corpus format](ir-and-corpus.md).

## Source grammar

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

General CFGs can be expressed in source with explicit `block`, `jump`, and `branch` syntax. That form preserves edge bindings and exact step counts when source is printed from normalized IR.
