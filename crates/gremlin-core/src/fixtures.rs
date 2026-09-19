//! Independent development oracles. Search never receives these implementations.
use crate::{Signature, Type, Value};
pub fn fixture_signature(name: &str) -> Result<Signature, String> {
    let n = match name {
        "identity_u64" | "increment_u64" | "xor_u64" | "composed_u64" | "affine_u64" => 1,
        "add_u64" => 2,
        _ => return Err(format!("unknown fixture {name}")),
    };
    Ok(Signature {
        arguments: vec![Type::U64; n],
        return_type: Type::U64,
    })
}
pub fn fixture_observe(name: &str, args: &[Value]) -> Result<Value, String> {
    let s = fixture_signature(name)?;
    if args.len() != s.arguments.len() || args.iter().zip(&s.arguments).any(|(v, t)| v.ty != *t) {
        return Err("fixture argument mismatch".into());
    }
    let x = args[0].bits;
    let y = match name {
        "identity_u64" => x,
        "increment_u64" => x.wrapping_add(1),
        "xor_u64" => x ^ 0x12345678,
        "add_u64" => x.wrapping_add(args[1].bits),
        "composed_u64" => x.wrapping_mul(3).wrapping_add(1),
        "affine_u64" => (x ^ 0x12345678).wrapping_mul(7).wrapping_add(3),
        _ => return Err("unknown fixture".into()),
    };
    Ok(Value::new(Type::U64, y))
}
pub fn fixture_fingerprint() -> String {
    crate::hash(include_bytes!("fixtures.rs"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_oracle_vectors() {
        for (name, args, expected) in [
            ("identity_u64", vec![u64::MAX], u64::MAX),
            ("increment_u64", vec![u64::MAX], 0),
            ("xor_u64", vec![0], 0x12345678),
            ("xor_u64", vec![0x12345678], 0),
            ("add_u64", vec![u64::MAX, 2], 1),
            ("composed_u64", vec![5], 16),
            ("composed_u64", vec![u64::MAX], u64::MAX - 1),
            ("affine_u64", vec![0], 0x7f6e5d4b),
            ("affine_u64", vec![1], 0x7f6e5d52),
        ] {
            assert_eq!(
                fixture_observe(
                    name,
                    &args
                        .into_iter()
                        .map(|n| Value::new(Type::U64, n))
                        .collect::<Vec<_>>()
                )
                .unwrap()
                .bits,
                expected
            );
        }
    }
}
