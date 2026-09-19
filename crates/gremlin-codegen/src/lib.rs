use gremlin_core::*;
use std::collections::BTreeMap;
struct Writer {
    out: String,
    next: usize,
    types: BTreeMap<Id, Type>,
}
impl Writer {
    fn line(&mut self, s: impl AsRef<str>) {
        self.out.push_str(s.as_ref());
        self.out.push('\n');
    }
    fn value(&mut self, s: impl AsRef<str>) -> String {
        let id = format!("%t{}", self.next);
        self.next += 1;
        self.line(format!("  {id} = {}", s.as_ref()));
        id
    }
    fn load(&mut self, id: Id) -> String {
        self.value(format!("load i{}, ptr %s{id}", self.types[&id].width()))
    }
    fn guard(&mut self, condition: &str, failure: &str) {
        let label = format!("next{}", self.next);
        self.next += 1;
        self.line(format!(
            "  br i1 {condition}, label %{failure}, label %{label}"
        ));
        self.line(format!("{label}:"));
    }
    fn step(&mut self, budget: u64) {
        let old = self.value("load i64, ptr %steps");
        let exhausted = self.value(format!("icmp eq i64 {old}, {budget}"));
        self.guard(&exhausted, "timeout");
        let new = self.value(format!("add i64 {old}, 1"));
        self.line(format!("  store i64 {new}, ptr %steps"));
    }
    fn expression(&mut self, expr: &Expr) -> Result<String, String> {
        match expr {
            Expr::Const { value } => Ok(value.bits.to_string()),
            Expr::Call { .. } => Err("LLVM internal calls unsupported".into()),
            Expr::Apply { op, args } => {
                let w = self.types[&args[0]].width();
                let a = self.load(args[0]);
                let b = if args.len() > 1 {
                    self.load(args[1])
                } else {
                    a.clone()
                };
                let value = match op {
                    Op::Add | Op::Sub | Op::Mul | Op::And | Op::Or | Op::Xor => {
                        self.value(format!("{op} i{w} {a}, {b}"))
                    }
                    Op::Not => self.value(format!("xor i{w} {a}, -1")),
                    Op::Udiv | Op::Sdiv | Op::Urem | Op::Srem => {
                        let zero = self.value(format!("icmp eq i{w} {b}, 0"));
                        self.guard(&zero, "division_zero");
                        if matches!(op, Op::Sdiv | Op::Srem) {
                            let min = self.value(format!("icmp eq i{w} {a}, {}", 1u64 << (w - 1)));
                            let minus_one = self.value(format!("icmp eq i{w} {b}, -1"));
                            let overflow = self.value(format!("and i1 {min}, {minus_one}"));
                            self.guard(&overflow, "division_overflow");
                        }
                        self.value(format!("{op} i{w} {a}, {b}"))
                    }
                    Op::Shl | Op::Lshr | Op::Ashr | Op::Rotl | Op::Rotr => {
                        let k = self.value(format!("urem i{w} {b}, {w}"));
                        if matches!(op, Op::Rotl | Op::Rotr) {
                            let complement = self.value(format!("sub i{w} {w}, {k}"));
                            let other = self.value(format!("urem i{w} {complement}, {w}"));
                            let (left, right) = if *op == Op::Rotl {
                                ("shl", "lshr")
                            } else {
                                ("lshr", "shl")
                            };
                            let left = self.value(format!("{left} i{w} {a}, {k}"));
                            let right = self.value(format!("{right} i{w} {a}, {other}"));
                            self.value(format!("or i{w} {left}, {right}"))
                        } else {
                            self.value(format!("{op} i{w} {a}, {k}"))
                        }
                    }
                    Op::Eq
                    | Op::Ne
                    | Op::Ult
                    | Op::Ule
                    | Op::Ugt
                    | Op::Uge
                    | Op::Slt
                    | Op::Sle
                    | Op::Sgt
                    | Op::Sge => self.value(format!("icmp {op} i{w} {a}, {b}")),
                    Op::Select => {
                        let c = self.load(args[2]);
                        self.value(format!(
                            "select i1 {a}, i{} {b}, i{} {c}",
                            self.types[&args[1]].width(),
                            self.types[&args[2]].width()
                        ))
                    }
                };
                Ok(value)
            }
        }
    }
    fn edge(&mut self, f: &Function, e: &Edge, label: &str) {
        self.line(format!("{label}:"));
        let args: Vec<_> = e.args.iter().map(|id| self.load(*id)).collect();
        let block = f.blocks.iter().find(|b| b.id == e.block).unwrap();
        for (p, v) in block.parameters.iter().zip(args) {
            self.line(format!("  store i{} {v}, ptr %s{}", p.ty.width(), p.id));
        }
        self.line(format!("  br label %b{}", e.block));
    }
}
pub fn lower(function: &Function, max_steps: u64) -> Result<String, String> {
    let f = function.normalized()?;
    if !f.callees.is_empty()
        || f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.expr, Expr::Call { .. }))
    {
        return Err("LLVM internal calls unsupported".into());
    }
    let count = f
        .blocks
        .iter()
        .map(|b| b.instructions.len() + b.parameters.len())
        .sum::<usize>();
    if count > 65536 || f.blocks.len() > 4096 {
        return Err("LLVM function exceeds supported size".into());
    }
    let mut types = BTreeMap::new();
    for p in &f.parameters {
        types.insert(p.id, p.ty);
    }
    for b in &f.blocks {
        for p in &b.parameters {
            types.insert(p.id, p.ty);
        }
        for i in &b.instructions {
            types.insert(i.id, i.ty);
        }
    }
    let mut writer = Writer {
        out: String::new(),
        next: 0,
        types,
    };
    let w = f.return_type.width();
    let result = format!("{{i{w}, i8}}");
    let args = f
        .parameters
        .iter()
        .map(|p| format!("i{} %arg{}", p.ty.width(), p.id))
        .collect::<Vec<_>>()
        .join(", ");
    writer.line("target triple = \"x86_64-unknown-linux-gnu\"");
    writer.line("declare void @llvm.trap() cold noreturn nounwind");
    writer.line(format!(
        "define internal {result} @gremlin_impl({args}) {{\nentry:"
    ));
    for (id, ty) in writer.types.clone() {
        writer.line(format!("  %s{id} = alloca i{}", ty.width()));
    }
    writer.line("  %steps = alloca i64\n  store i64 0, ptr %steps");
    for p in &f.parameters {
        writer.line(format!(
            "  store i{} %arg{}, ptr %s{}",
            p.ty.width(),
            p.id,
            p.id
        ));
    }
    writer.line(format!("  br label %b{}", f.entry));
    let mut edges = Vec::new();
    for b in &f.blocks {
        writer.line(format!("b{}:", b.id));
        for i in &b.instructions {
            writer.step(max_steps);
            let value = writer.expression(&i.expr)?;
            writer.line(format!("  store i{} {value}, ptr %s{}", i.ty.width(), i.id));
        }
        writer.step(max_steps);
        match &b.terminator {
            Terminator::Return { value } => {
                let value = writer.load(*value);
                let r = writer.value(format!(
                    "insertvalue {result} zeroinitializer, i{w} {value}, 0"
                ));
                writer.line(format!("  ret {result} {r}"));
            }
            Terminator::Jump { edge } => {
                let label = format!("edge{}", edges.len());
                writer.line(format!("  br label %{label}"));
                edges.push((label, edge));
            }
            Terminator::Branch {
                condition,
                if_true,
                if_false,
            } => {
                let condition = writer.load(*condition);
                let yes = format!("edge{}", edges.len());
                let no = format!("edge{}", edges.len() + 1);
                writer.line(format!("  br i1 {condition}, label %{yes}, label %{no}"));
                edges.push((yes, if_true));
                edges.push((no, if_false));
            }
        }
    }
    for (label, edge) in edges {
        writer.edge(&f, edge, &label);
    }
    for (label, status) in [
        ("division_zero", 1),
        ("division_overflow", 2),
        ("timeout", 3),
    ] {
        writer.line(format!("{label}:\n  ret {result} {{i{w} 0, i8 {status}}}"));
    }
    writer.line("}");
    writer.line(format!("define i{w} @gremlin_target({args}) {{\nentry:\n  %r = call {result} @gremlin_impl({args})\n  %status = extractvalue {result} %r, 1\n  %ok = icmp eq i8 %status, 0\n  br i1 %ok, label %done, label %fail\ndone:\n  %value = extractvalue {result} %r, 0\n  ret i{w} %value\nfail:\n  call void @llvm.trap()\n  unreachable\n}}"));
    writer.line(format!("define i8 @gremlin_status({args}) {{\nentry:\n  %r = call {result} @gremlin_impl({args})\n  %status = extractvalue {result} %r, 1\n  ret i8 %status\n}}"));
    Ok(writer.out)
}
pub mod artifact;
