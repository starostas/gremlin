# Functions and calls

## Signatures and entry points

A function takes zero to four integer arguments and returns one integer. When a module defines `main`, execution enters `main`; otherwise it enters the first declared function.

Internal call arguments are evaluated eagerly and must match the declared function signature exactly.

## Recursion

Source modules can call other functions and recurse:

    fn main(x: u8) -> u8 {
        return factorial(x);
    }

    fn factorial(n: u8) -> u8 {
        if eq(n, 0u8) {
            return 1u8;
        } else {
            return mul(n, factorial(sub(n, 1u8)));
        }
    }

See the [full recursion example](../../examples/recursion.gremlin) in the repository.

Calls and callee returns each cost one execution step. The [execution model](execution.md) defines the shared call-depth budget and the CLI’s default limit.

## Backend support

The CPU module evaluator supports internal calls. GPU, formal, and LLVM backends currently reject them. See [Supported features](../support.md) when choosing an execution or verification workflow.
