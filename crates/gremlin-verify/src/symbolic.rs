use gremlin_core::*;
use std::collections::BTreeMap;
pub struct Symbolic {
    pub definitions: String,
    pub value: String,
    pub completed: String,
    pub trap: String,
}
pub fn bv(value: u64, width: u32) -> String {
    format!("(_ bv{} {width})", value & (u64::MAX >> (64 - width)))
}
pub fn candidate(f: &Function, prefix: &str) -> Result<Symbolic, String> {
    f.validate()?;
    if f.blocks.len() != 1 || !f.callees.is_empty() {
        return Err("formal candidate subset requires one block and no calls".into());
    }
    let mut values = BTreeMap::new();
    let mut types = BTreeMap::new();
    let mut definitions = String::new();
    let mut traps = Vec::new();
    for (n, p) in f.parameters.iter().enumerate() {
        values.insert(p.id, format!("x{n}"));
        types.insert(p.id, p.ty);
    }
    for i in &f.blocks[0].instructions {
        let value = match &i.expr {
            Expr::Const { value } => bv(value.bits, value.ty.width()),
            Expr::Call { .. } => return Err("formal internal calls unsupported".into()),
            Expr::Apply { op, args } => {
                let a = &values[&args[0]];
                let b = values.get(args.get(1).unwrap_or(&args[0])).unwrap();
                let w = types[&args[0]].width();
                let zero = bv(0, w);
                let k = format!("(bvurem {b} {})", bv(w as u64, w));
                let binary = |name: &str| format!("({name} {a} {b})");
                let comparison = |name: &str| format!("(ite {} #b1 #b0)", binary(name));
                match op {
                    Op::Add => binary("bvadd"),
                    Op::Sub => binary("bvsub"),
                    Op::Mul => binary("bvmul"),
                    Op::And => binary("bvand"),
                    Op::Or => binary("bvor"),
                    Op::Xor => binary("bvxor"),
                    Op::Not => format!("(bvnot {a})"),
                    Op::Udiv | Op::Sdiv | Op::Urem | Op::Srem => {
                        traps.push((format!("(= {b} {zero})"), "#b01"));
                        if matches!(op, Op::Sdiv | Op::Srem) {
                            traps.push((
                                format!(
                                    "(and (= {a} {}) (= {b} {}))",
                                    bv(1 << (w - 1), w),
                                    bv(u64::MAX, w)
                                ),
                                "#b10",
                            ));
                        }
                        binary(match op {
                            Op::Udiv => "bvudiv",
                            Op::Sdiv => "bvsdiv",
                            Op::Urem => "bvurem",
                            _ => "bvsrem",
                        })
                    }
                    Op::Shl => format!("(bvshl {a} {k})"),
                    Op::Lshr => format!("(bvlshr {a} {k})"),
                    Op::Ashr => format!("(bvashr {a} {k})"),
                    Op::Rotl => format!(
                        "(bvor (bvshl {a} {k}) (bvlshr {a} (bvsub {} {k})))",
                        bv(w as u64, w)
                    ),
                    Op::Rotr => format!(
                        "(bvor (bvlshr {a} {k}) (bvshl {a} (bvsub {} {k})))",
                        bv(w as u64, w)
                    ),
                    Op::Eq => comparison("="),
                    Op::Ne => format!("(ite (= {a} {b}) #b0 #b1)"),
                    Op::Ult => comparison("bvult"),
                    Op::Ule => comparison("bvule"),
                    Op::Ugt => comparison("bvugt"),
                    Op::Uge => comparison("bvuge"),
                    Op::Slt => comparison("bvslt"),
                    Op::Sle => comparison("bvsle"),
                    Op::Sgt => comparison("bvsgt"),
                    Op::Sge => comparison("bvsge"),
                    Op::Select => format!("(ite (= {a} #b1) {b} {})", values[&args[2]]),
                }
            }
        };
        let name = format!("{prefix}{}", i.id);
        definitions.push_str(&format!(
            "(define-fun {name} () (_ BitVec {}) {value})\n",
            i.ty.width()
        ));
        values.insert(i.id, name);
        types.insert(i.id, i.ty);
    }
    let Terminator::Return { value } = f.blocks[0].terminator else {
        return Err("formal candidate requires a return".into());
    };
    let trap = traps
        .into_iter()
        .rev()
        .fold("#b00".to_string(), |rest, (condition, reason)| {
            format!("(ite {condition} {reason} {rest})")
        });
    let completed = format!("(= {trap} #b00)");
    Ok(Symbolic {
        definitions,
        value: values[&value].clone(),
        completed,
        trap,
    })
}
