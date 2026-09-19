use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    Bool,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}
impl Type {
    pub fn width(self) -> u32 {
        match self {
            Self::Bool => 1,
            Self::U8 | Self::I8 => 8,
            Self::U16 | Self::I16 => 16,
            Self::U32 | Self::I32 => 32,
            Self::U64 | Self::I64 => 64,
        }
    }
    pub fn signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }
    pub fn mask(self) -> u64 {
        u64::MAX >> (64 - self.width())
    }
    pub fn integer(self) -> bool {
        self != Self::Bool
    }
}
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("{self:?}").to_lowercase())
    }
}
impl FromStr for Type {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "bool" => Ok(Self::Bool),
            "u8" => Ok(Self::U8),
            "u16" => Ok(Self::U16),
            "u32" => Ok(Self::U32),
            "u64" => Ok(Self::U64),
            "i8" => Ok(Self::I8),
            "i16" => Ok(Self::I16),
            "i32" => Ok(Self::I32),
            "i64" => Ok(Self::I64),
            _ => Err(format!("unknown type {s}")),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Value {
    pub ty: Type,
    pub bits: u64,
}
impl Value {
    pub fn new(ty: Type, bits: u64) -> Self {
        Self {
            ty,
            bits: bits & ty.mask(),
        }
    }
    pub fn signed(self) -> i128 {
        let n = self.bits as i128;
        if self.bits & (1u64 << (self.ty.width() - 1)) != 0 {
            n - (1i128 << self.ty.width())
        } else {
            n
        }
    }
    pub fn hex(self) -> String {
        format!(
            "0x{:0width$x}",
            self.bits,
            width = self.ty.width().div_ceil(4) as usize
        )
    }
    pub fn from_hex(ty: Type, s: &str) -> Result<Self, String> {
        if !ty.integer()
            || !s.starts_with("0x")
            || s.len() != 2 + (ty.width() / 4) as usize
            || !s.as_bytes()[2..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(format!("expected fixed-width hexadecimal {ty}, got {s}"));
        }
        let bits =
            u64::from_str_radix(&s[2..], 16).map_err(|_| format!("invalid hexadecimal {s}"))?;
        Ok(Self { ty, bits })
    }
    pub fn literal(self) -> String {
        if self.ty == Type::Bool {
            (self.bits != 0).to_string()
        } else {
            format!("{}{ty}", self.hex(), ty = self.ty)
        }
    }
    pub fn parse(s: &str) -> Result<Self, String> {
        if s == "true" || s == "false" {
            return Ok(Self::new(Type::Bool, u64::from(s == "true")));
        }
        let suffix = ["u16", "u32", "u64", "i16", "i32", "i64", "u8", "i8"]
            .into_iter()
            .find(|t| s.ends_with(t))
            .ok_or_else(|| format!("literal requires integer type suffix: {s}"))?;
        let ty: Type = suffix.parse()?;
        let digits = &s[..s.len() - suffix.len()];
        if let Some(hex) = digits.strip_prefix("0x") {
            if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("invalid literal {s}"));
            }
            let bits = u64::from_str_radix(hex, 16).map_err(|_| format!("invalid literal {s}"))?;
            if bits > ty.mask() {
                return Err(format!("out-of-range literal {s}"));
            }
            return Ok(Self { ty, bits });
        }
        let decimal = digits.strip_prefix('-').unwrap_or(digits);
        if decimal.is_empty()
            || !decimal.bytes().all(|b| b.is_ascii_digit())
            || (digits.starts_with('-') && !ty.signed())
        {
            return Err(format!("invalid literal {s}"));
        }
        let n = digits
            .parse::<i128>()
            .map_err(|_| format!("invalid literal {s}"))?;
        let (lo, hi) = if ty.signed() {
            (
                -(1i128 << (ty.width() - 1)),
                (1i128 << (ty.width() - 1)) - 1,
            )
        } else {
            (0, ty.mask() as i128)
        };
        if n < lo || n > hi {
            return Err(format!("out-of-range literal {s}"));
        }
        Ok(Self::new(ty, n as u64))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Add,
    Sub,
    Mul,
    Udiv,
    Sdiv,
    Urem,
    Srem,
    And,
    Or,
    Xor,
    Not,
    Shl,
    Lshr,
    Ashr,
    Rotl,
    Rotr,
    Eq,
    Ne,
    Ult,
    Ule,
    Ugt,
    Uge,
    Slt,
    Sle,
    Sgt,
    Sge,
    Select,
}
impl Op {
    pub const ALL: [Self; 27] = [
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Udiv,
        Self::Sdiv,
        Self::Urem,
        Self::Srem,
        Self::And,
        Self::Or,
        Self::Xor,
        Self::Not,
        Self::Shl,
        Self::Lshr,
        Self::Ashr,
        Self::Rotl,
        Self::Rotr,
        Self::Eq,
        Self::Ne,
        Self::Ult,
        Self::Ule,
        Self::Ugt,
        Self::Uge,
        Self::Slt,
        Self::Sle,
        Self::Sgt,
        Self::Sge,
        Self::Select,
    ];
    pub fn arity(self) -> usize {
        match self {
            Self::Not => 1,
            Self::Select => 3,
            _ => 2,
        }
    }
    pub fn result(self, types: &[Type]) -> Result<Type, String> {
        if types.len() != self.arity() {
            return Err(format!("{self} expects {} operands", self.arity()));
        }
        if self == Self::Select {
            return if types[0] == Type::Bool && types[1] == types[2] {
                Ok(types[1])
            } else {
                Err("select requires bool and identical alternatives".into())
            };
        }
        let t = types[0];
        if types.iter().any(|x| *x != t) {
            return Err(format!("{self} requires identical operand types"));
        }
        if matches!(self, Self::Eq | Self::Ne) {
            return Ok(Type::Bool);
        }
        if !t.integer() {
            return Err(format!("{self} requires integers"));
        }
        if matches!(
            self,
            Self::Udiv | Self::Urem | Self::Ult | Self::Ule | Self::Ugt | Self::Uge
        ) && t.signed()
        {
            return Err(format!("{self} requires unsigned type"));
        }
        if matches!(
            self,
            Self::Sdiv | Self::Srem | Self::Slt | Self::Sle | Self::Sgt | Self::Sge
        ) && !t.signed()
        {
            return Err(format!("{self} requires signed type"));
        }
        Ok(
            if matches!(
                self,
                Self::Ult
                    | Self::Ule
                    | Self::Ugt
                    | Self::Uge
                    | Self::Slt
                    | Self::Sle
                    | Self::Sgt
                    | Self::Sge
            ) {
                Type::Bool
            } else {
                t
            },
        )
    }
    pub fn eval(self, v: &[Value]) -> Result<Value, String> {
        if v.len() > 3 {
            return Err("too many operands".into());
        }
        let mut types = [Type::Bool; 3];
        for (i, x) in v.iter().enumerate() {
            types[i] = x.ty;
        }
        let ty = self.result(&types[..v.len()])?;
        let a = v[0];
        let b = v.get(1).copied().unwrap_or(a);
        let w = a.ty.width();
        let k = (b.bits % u64::from(w)) as u32;
        let bits = match self {
            Self::Add => a.bits.wrapping_add(b.bits),
            Self::Sub => a.bits.wrapping_sub(b.bits),
            Self::Mul => a.bits.wrapping_mul(b.bits),
            Self::Udiv | Self::Urem => {
                if b.bits == 0 {
                    return Err("division by zero".into());
                }
                if self == Self::Udiv {
                    a.bits / b.bits
                } else {
                    a.bits % b.bits
                }
            }
            Self::Sdiv | Self::Srem => {
                let x = a.signed();
                let y = b.signed();
                if y == 0 {
                    return Err("division by zero".into());
                }
                if x == -(1i128 << (w - 1)) && y == -1 {
                    return Err("signed division overflow".into());
                }
                if self == Self::Sdiv {
                    (x / y) as u64
                } else {
                    (x % y) as u64
                }
            }
            Self::And => a.bits & b.bits,
            Self::Or => a.bits | b.bits,
            Self::Xor => a.bits ^ b.bits,
            Self::Not => !a.bits,
            Self::Shl => a.bits.wrapping_shl(k),
            Self::Lshr => a.bits >> k,
            Self::Ashr => (a.signed() >> k) as u64,
            Self::Rotl => {
                if k == 0 {
                    a.bits
                } else {
                    a.bits.wrapping_shl(k) | (a.bits >> (w - k))
                }
            }
            Self::Rotr => {
                if k == 0 {
                    a.bits
                } else {
                    (a.bits >> k) | a.bits.wrapping_shl(w - k)
                }
            }
            Self::Eq => u64::from(a.bits == b.bits),
            Self::Ne => u64::from(a.bits != b.bits),
            Self::Ult => u64::from(a.bits < b.bits),
            Self::Ule => u64::from(a.bits <= b.bits),
            Self::Ugt => u64::from(a.bits > b.bits),
            Self::Uge => u64::from(a.bits >= b.bits),
            Self::Slt => u64::from(a.signed() < b.signed()),
            Self::Sle => u64::from(a.signed() <= b.signed()),
            Self::Sgt => u64::from(a.signed() > b.signed()),
            Self::Sge => u64::from(a.signed() >= b.signed()),
            Self::Select => {
                if a.bits != 0 {
                    v[1].bits
                } else {
                    v[2].bits
                }
            }
        };
        Ok(Value::new(ty, bits))
    }
}
impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("{self:?}").to_lowercase())
    }
}
impl FromStr for Op {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|o| o.to_string() == s)
            .ok_or_else(|| format!("unknown operator {s}"))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub arguments: Vec<Type>,
    pub return_type: Type,
}
impl Signature {
    pub fn validate(&self) -> Result<(), String> {
        if self.arguments.len() > 4 {
            return Err("signatures support at most four arguments".into());
        }
        if !self.return_type.integer() || self.arguments.iter().any(|t| !t.integer()) {
            return Err("signature arguments and return must be integers".into());
        }
        Ok(())
    }
}

impl Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Value", 2)?;
        s.serialize_field("ty", &self.ty)?;
        s.serialize_field("bits", &self.hex())?;
        s.end()
    }
}
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Encoded {
            ty: Type,
            bits: String,
        }
        let e = Encoded::deserialize(deserializer)?;
        if e.ty == Type::Bool {
            return match e.bits.as_str() {
                "0x0" => Ok(Value::new(Type::Bool, 0)),
                "0x1" => Ok(Value::new(Type::Bool, 1)),
                _ => Err(serde::de::Error::custom("invalid bool pattern")),
            };
        }
        Value::from_hex(e.ty, &e.bits).map_err(serde::de::Error::custom)
    }
}
