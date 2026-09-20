# Operators

All integer operands to an operator must share a type. Integer results retain that type and its width-limited bit pattern.

| Family | Operations | Rules |
| --- | --- | --- |
| Arithmetic | `add sub mul` | Wrap modulo 2^width |
| Bitwise | `and or xor not` | Results are masked to the declared width |
| Unsigned division | `udiv urem` | Require unsigned integers |
| Signed division | `sdiv srem` | Require signed integers; quotient truncates toward zero and remainder has the dividend’s sign |
| Shifts and rotations | `shl lshr ashr rotl rotr` | Count is an unsigned pattern modulo the width |
| Comparisons | `eq ne ult ule ugt uge slt sle sgt sge` | Return `bool` |
| Selection | `select` | Takes a `bool` condition and two equal-typed alternatives |

## Traps and bit behavior

Division and remainder by zero trap. Signed `MIN / -1` and `MIN % -1` also trap.

`ashr` sign-extends the top bit even when its integer type is unsigned. Rotations wrap within the declared width.

`eq` and `ne` compare equal-typed bit patterns and also accept `bool`. Unsigned ordering comparisons require unsigned integers; signed ordering comparisons require signed integers.

## Evaluation order

Arguments are evaluated left-to-right and eagerly. Both alternatives to `select` are evaluated, so an unchosen expression can still trap.

`not` has one operand, `select` has three, and every other operation has two.
