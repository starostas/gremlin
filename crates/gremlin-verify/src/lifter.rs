use crate::symbolic::{bv, Symbolic};
use gremlin_core::*;
use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};
use object::{Object, ObjectSection, ObjectSymbol};
use serde::Serialize;
use std::collections::BTreeMap;
pub const LIFTER_VERSION: &str = "gremlin-x86-register-v2/iced-x86-1.21.0";
#[derive(Clone, Debug, Serialize)]
enum Operand {
    Reg(u8, u32),
    Imm(u64),
}
#[derive(Clone, Debug, Serialize)]
enum Operation {
    Move(Operand),
    Binary(String, Operand, Operand),
    Unary(String, Operand),
    Lea(Option<u8>, Option<u8>, u32, u64),
}
#[derive(Clone, Debug, Serialize)]
struct Assignment {
    dst: u8,
    width: u32,
    operation: Operation,
}
#[derive(Clone, Debug, Serialize)]
pub struct BinaryModel {
    pub binary_hash: String,
    pub symbol: String,
    pub entry: u64,
    pub size: u64,
    pub code_hash: String,
    pub lifter: String,
    signature: Signature,
    instructions: Vec<Assignment>,
}
fn register(r: Register) -> Result<(u8, u32), String> {
    let width = (r.size() * 8) as u32;
    if width != 32 && width != 64 {
        return Err(format!("unsupported register {r:?}"));
    }
    let id = match r.full_register() {
        Register::RAX => 0,
        Register::RCX => 1,
        Register::RDX => 2,
        Register::RSI => 3,
        Register::RDI => 4,
        Register::R8 => 5,
        Register::R9 => 6,
        Register::R10 => 7,
        Register::R11 => 8,
        _ => return Err(format!("unsupported register {r:?}")),
    };
    Ok((id, width))
}
fn operand(i: &Instruction, n: u32) -> Result<Operand, String> {
    match i.op_kind(n) {
        OpKind::Register => {
            let (r, w) = register(i.op_register(n))?;
            Ok(Operand::Reg(r, w))
        }
        OpKind::Immediate8
        | OpKind::Immediate16
        | OpKind::Immediate32
        | OpKind::Immediate64
        | OpKind::Immediate8to16
        | OpKind::Immediate8to32
        | OpKind::Immediate8to64
        | OpKind::Immediate32to64 => Ok(Operand::Imm(i.immediate(n))),
        _ => Err("memory and non-register operands unsupported".into()),
    }
}
fn slice(data: &[u8], offset: u64, size: u64) -> Result<&[u8], String> {
    let end = offset.checked_add(size).ok_or("ELF range overflow")?;
    data.get(
        usize::try_from(offset).map_err(|_| "ELF offset overflow")?
            ..usize::try_from(end).map_err(|_| "ELF offset overflow")?,
    )
    .ok_or("ELF range outside file".into())
}
fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b.try_into().unwrap())
}
fn static_elf(data: &[u8]) -> Result<(), String> {
    if data.len() < 64 || &data[..6] != b"\x7fELF\x02\x01" || data[16..20] != [3, 0, 62, 0] {
        return Err("formal target requires ELF64 little-endian x86-64 ET_DYN".into());
    }
    let offset = u64le(&data[32..40]);
    let size = u16::from_le_bytes(data[54..56].try_into().unwrap()) as u64;
    let count = u16::from_le_bytes(data[56..58].try_into().unwrap()) as u64;
    if size != 56 || count == 0 || count == 65535 {
        return Err("unsupported ELF program header layout".into());
    }
    for n in 0..count {
        let p = slice(
            data,
            offset.checked_add(n * size).ok_or("ELF header overflow")?,
            size,
        )?;
        let kind = u32::from_le_bytes(p[..4].try_into().unwrap());
        let flags = u32::from_le_bytes(p[4..8].try_into().unwrap());
        if kind == 1 && flags & 3 == 3 {
            return Err("writable executable segments unsupported".into());
        }
        if kind == 2 {
            let dynamic = slice(data, u64le(&p[8..16]), u64le(&p[32..40]))?;
            for row in dynamic.chunks_exact(16) {
                let tag = u64le(&row[..8]);
                if tag == 0 {
                    break;
                }
                if !matches!(tag, 4 | 5 | 6 | 10 | 11 | 14 | 0x6fff_fef5) {
                    return Err(
                        "ELF dependencies, initializers, finalizers and relocations unsupported"
                            .into(),
                    );
                }
            }
        }
    }
    Ok(())
}
pub fn lift(contract: &BinaryContract, signature: &Signature) -> Result<BinaryModel, String> {
    contract.validate()?;
    signature.validate()?;
    let data = std::fs::read(&contract.path).map_err(|e| e.to_string())?;
    if hash(&data) != contract.sha256.to_lowercase() {
        return Err("binary hash mismatch".into());
    }
    static_elf(&data)?;
    let file = object::File::parse(data.as_slice()).map_err(|e| e.to_string())?;
    let mut symbols = file
        .dynamic_symbols()
        .filter(|s| s.name().ok() == Some(contract.symbol.as_str()) && !s.is_undefined());
    let symbol = symbols.next().ok_or("missing exported function")?;
    if symbols.next().is_some() {
        return Err("ambiguous exported symbol versions unsupported".into());
    }
    if !matches!(symbol.flags(), object::SymbolFlags::Elf { st_info, .. } if st_info & 15 == 2) {
        return Err("only ordinary STT_FUNC symbols are supported, not resolvers".into());
    }
    if symbol.size() == 0 || symbol.size() > 4096 || symbol.kind() != object::SymbolKind::Text {
        return Err("formal symbol must be a nonempty function of at most 4096 bytes".into());
    }
    crate::elf_binding::validate(&data, &contract.symbol, symbol.address(), symbol.size())?;
    let section = file
        .section_by_index(symbol.section_index().ok_or("symbol has no section")?)
        .map_err(|e| e.to_string())?;
    let offset = symbol
        .address()
        .checked_sub(section.address())
        .ok_or("symbol outside section")?;
    let code = slice(
        section.data().map_err(|e| e.to_string())?,
        offset,
        symbol.size(),
    )?;
    let phoff = u64le(&data[32..40]);
    let phnum = u16::from_le_bytes(data[56..58].try_into().unwrap()) as u64;
    let end = symbol
        .address()
        .checked_add(symbol.size())
        .ok_or("symbol address overflow")?;
    let mut mapped = false;
    for n in 0..phnum {
        let p = slice(&data, phoff + n * 56, 56)?;
        if u32::from_le_bytes(p[..4].try_into().unwrap()) != 1 {
            continue;
        }
        let address = u64le(&p[16..24]);
        let memory_end = address
            .checked_add(u64le(&p[40..48]))
            .ok_or("segment address overflow")?;
        if address & !4095 >= end.checked_add(4095).ok_or("symbol page overflow")? & !4095
            || memory_end <= symbol.address() & !4095
        {
            continue;
        }
        if mapped
            || address > symbol.address()
            || address
                .checked_add(u64le(&p[32..40]))
                .ok_or("segment size overflow")?
                < end
            || u32::from_le_bytes(p[4..8].try_into().unwrap()) & 3 != 1
        {
            return Err("symbol has ambiguous, non-executable or writable load mapping".into());
        }
        let offset = u64le(&p[8..16])
            .checked_add(symbol.address() - address)
            .ok_or("mapped offset overflow")?;
        if slice(&data, offset, symbol.size())? != code {
            return Err("ELF symbol bytes differ from loaded bytes".into());
        }
        mapped = true;
    }
    if !mapped {
        return Err("symbol is outside executable load mapping".into());
    }
    let mut decoder = Decoder::with_ip(64, code, symbol.address(), DecoderOptions::NONE);
    let mut instructions = Vec::new();
    let mut returned = false;
    while decoder.can_decode() {
        let position = decoder.position();
        let i = decoder.decode();
        for byte in &code[position..decoder.position()] {
            if *byte == 0x67 {
                return Err("address-size override unsupported".into());
            }
            if !matches!(
                *byte,
                0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65 | 0x66 | 0xf0 | 0xf2 | 0xf3 | 0x40..=0x4f
            ) {
                break;
            }
        }
        if i.is_invalid()
            || i.has_lock_prefix()
            || i.has_rep_prefix()
            || i.has_repne_prefix()
            || i.segment_prefix() != Register::None
        {
            return Err("invalid or prefixed instruction unsupported".into());
        }
        if i.mnemonic() == Mnemonic::Ret {
            if i.len() != 1
                || i.code() != iced_x86::Code::Retnq
                || i.op_count() != 0
                || decoder.can_decode()
            {
                return Err("only one final near return is supported".into());
            }
            returned = true;
            break;
        }
        if instructions.len() >= 256 {
            return Err("formal binary instruction limit is 256".into());
        }
        if i.op_count() == 0 || i.op0_kind() != OpKind::Register {
            return Err(format!("unsupported instruction {:?}", i.mnemonic()));
        }
        let (dst, width) = register(i.op0_register())?;
        let operation = match i.mnemonic() {
            Mnemonic::Mov if i.op_count() == 2 => Operation::Move(operand(&i, 1)?),
            Mnemonic::Xor
                if i.op_count() == 2
                    && i.op1_kind() == OpKind::Register
                    && i.op0_register() == i.op1_register() =>
            {
                Operation::Move(Operand::Imm(0))
            }
            Mnemonic::Add | Mnemonic::Sub | Mnemonic::Xor | Mnemonic::And | Mnemonic::Or
                if i.op_count() == 2 =>
            {
                Operation::Binary(
                    format!("{:?}", i.mnemonic()).to_lowercase(),
                    operand(&i, 0)?,
                    operand(&i, 1)?,
                )
            }
            Mnemonic::Imul if i.op_count() == 2 || i.op_count() == 3 => {
                let a = if i.op_count() == 2 { 0 } else { 1 };
                Operation::Binary("mul".into(), operand(&i, a)?, operand(&i, a + 1)?)
            }
            Mnemonic::Not | Mnemonic::Neg if i.op_count() == 1 => Operation::Unary(
                format!("{:?}", i.mnemonic()).to_lowercase(),
                operand(&i, 0)?,
            ),
            Mnemonic::Lea if i.op_count() == 2 && i.op1_kind() == OpKind::Memory => {
                let get = |r: Register| -> Result<Option<u8>, String> {
                    if r == Register::None {
                        return Ok(None);
                    }
                    let (id, w) = register(r)?;
                    if w != 64 {
                        return Err("only 64-bit LEA addressing is supported".into());
                    }
                    Ok(Some(id))
                };
                Operation::Lea(
                    get(i.memory_base())?,
                    get(i.memory_index())?,
                    i.memory_index_scale(),
                    i.memory_displacement64(),
                )
            }
            _ => return Err(format!("unsupported instruction {:?}", i.mnemonic())),
        };
        instructions.push(Assignment {
            dst,
            width,
            operation,
        });
    }
    if !returned {
        return Err("function does not end in a supported return".into());
    }
    let model = BinaryModel {
        binary_hash: hash(&data),
        symbol: contract.symbol.clone(),
        entry: symbol.address(),
        size: symbol.size(),
        code_hash: hash(code),
        lifter: LIFTER_VERSION.into(),
        signature: signature.clone(),
        instructions,
    };
    model.symbolic()?;
    Ok(model)
}
impl BinaryModel {
    pub fn symbolic(&self) -> Result<Symbolic, String> {
        let mut registers = BTreeMap::<u8, String>::new();
        let mut definitions = String::new();
        for (n, ty) in self.signature.arguments.iter().enumerate() {
            let value = if ty.width() == 64 {
                format!("x{n}")
            } else {
                format!(
                    "((_ {} {}) x{n})",
                    if ty.signed() {
                        "sign_extend"
                    } else {
                        "zero_extend"
                    },
                    64 - ty.width()
                )
            };
            registers.insert([4, 3, 2, 1][n], value);
        }
        for (n, i) in self.instructions.iter().enumerate() {
            let read = |op: &Operand| -> Result<String, String> {
                match op {
                    Operand::Imm(v) => Ok(bv(*v, i.width)),
                    Operand::Reg(r, w) => {
                        if *w != i.width {
                            return Err("mixed register widths unsupported".into());
                        }
                        let v = registers.get(r).ok_or("read of undefined register")?;
                        Ok(if *w == 64 {
                            v.clone()
                        } else {
                            format!("((_ extract 31 0) {v})")
                        })
                    }
                }
            };
            let value = match &i.operation {
                Operation::Move(a) => read(a)?,
                Operation::Unary(op, a) => format!("(bv{op} {})", read(a)?),
                Operation::Binary(op, a, b) => format!("(bv{op} {} {})", read(a)?, read(b)?),
                Operation::Lea(base, index, scale, disp) => {
                    let get = |r: &Option<u8>| -> Result<String, String> {
                        match r {
                            None => Ok(bv(0, 64)),
                            Some(r) => registers
                                .get(r)
                                .cloned()
                                .ok_or("LEA reads undefined register".into()),
                        }
                    };
                    let e = format!(
                        "(bvadd (bvadd {} (bvmul {} {})) {})",
                        get(base)?,
                        get(index)?,
                        bv(*scale as u64, 64),
                        bv(*disp, 64)
                    );
                    if i.width == 32 {
                        format!("((_ extract 31 0) {e})")
                    } else {
                        e
                    }
                }
            };
            let value = if i.width == 32 {
                format!("((_ zero_extend 32) {value})")
            } else {
                value
            };
            let name = format!("b{n}");
            definitions.push_str(&format!("(define-fun {name} () (_ BitVec 64) {value})\n"));
            registers.insert(i.dst, name);
        }
        let value = registers.get(&0).ok_or("return register is undefined")?;
        let w = self.signature.return_type.width();
        Ok(Symbolic {
            definitions,
            value: if w == 64 {
                value.clone()
            } else {
                format!("((_ extract {} 0) {value})", w - 1)
            },
            completed: "true".into(),
            trap: "#b00".into(),
        })
    }
    pub fn execute(&self, inputs: &[Value]) -> Result<Value, String> {
        if inputs.len() != self.signature.arguments.len()
            || inputs
                .iter()
                .zip(&self.signature.arguments)
                .any(|(v, t)| v.ty != *t)
        {
            return Err("binary model input signature mismatch".into());
        }
        let mut registers = BTreeMap::<u8, u64>::new();
        for (n, v) in inputs.iter().enumerate() {
            registers.insert(
                [4, 3, 2, 1][n],
                if v.ty.signed() {
                    v.signed() as u64
                } else {
                    v.bits
                },
            );
        }
        for i in &self.instructions {
            let read = |o: &Operand| -> Result<u64, String> {
                match o {
                    Operand::Imm(v) => Ok(*v),
                    Operand::Reg(r, _) => {
                        registers.get(r).copied().ok_or("undefined register".into())
                    }
                }
            };
            let v = match &i.operation {
                Operation::Move(a) => read(a)?,
                Operation::Unary(op, a) => {
                    let a = read(a)?;
                    if op == "not" {
                        !a
                    } else {
                        a.wrapping_neg()
                    }
                }
                Operation::Binary(op, a, b) => {
                    let a = read(a)?;
                    let b = read(b)?;
                    match op.as_str() {
                        "add" => a.wrapping_add(b),
                        "sub" => a.wrapping_sub(b),
                        "mul" => a.wrapping_mul(b),
                        "and" => a & b,
                        "or" => a | b,
                        "xor" => a ^ b,
                        _ => return Err("invalid model opcode".into()),
                    }
                }
                Operation::Lea(base, index, scale, disp) => {
                    let get = |r: &Option<u8>| -> Result<u64, String> {
                        match r {
                            None => Ok(0),
                            Some(r) => registers
                                .get(r)
                                .copied()
                                .ok_or("undefined LEA register".into()),
                        }
                    };
                    get(base)?
                        .wrapping_add(get(index)?.wrapping_mul(*scale as u64))
                        .wrapping_add(*disp)
                }
            };
            registers.insert(i.dst, v & (u64::MAX >> (64 - i.width)));
        }
        Ok(Value::new(
            self.signature.return_type,
            *registers.get(&0).ok_or("undefined return register")?,
        ))
    }
}
