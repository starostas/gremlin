use crate::*;
use std::collections::BTreeMap;
fn lex(source: &str) -> Result<Vec<String>, String> {
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"->") {
            out.push("->".into());
            i += 2;
            continue;
        }
        if b"(),:{};=".contains(&c) {
            out.push((c as char).to_string());
            i += 1;
            continue;
        }
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push(source[start..i].into());
            continue;
        }
        return Err(format!("unexpected character at byte {i}"));
    }
    Ok(out)
}
struct Parser {
    tokens: Vec<String>,
    pos: usize,
    names: BTreeMap<String, (Id, Type)>,
    instructions: Vec<Instruction>,
    next: Id,
}
impl Parser {
    fn take(&mut self) -> Result<String, String> {
        let t = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or("unexpected end of source")?;
        self.pos += 1;
        Ok(t)
    }
    fn peek(&self) -> &str {
        self.tokens.get(self.pos).map(String::as_str).unwrap_or("")
    }
    fn expect(&mut self, s: &str) -> Result<(), String> {
        let t = self.take()?;
        if t != s {
            Err(format!("expected '{s}', got '{t}' at token {}", self.pos))
        } else {
            Ok(())
        }
    }
    fn ident(&mut self) -> Result<String, String> {
        let s = self.take()?;
        if !s
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || matches!(s.as_str(), "fn" | "let" | "return" | "true" | "false")
        {
            return Err(format!("expected identifier, got {s}"));
        }
        Ok(s)
    }
    fn emit(&mut self, ty: Type, expr: Expr) -> (Id, Type) {
        let id = self.next;
        self.next += 1;
        self.instructions.push(Instruction { id, ty, expr });
        (id, ty)
    }
    fn expr(&mut self, depth: usize) -> Result<(Id, Type), String> {
        if depth > 128 {
            return Err("expression nesting exceeds 128".into());
        }
        let token = self.take()?;
        if self.peek() == "(" {
            let op: Op = token.parse()?;
            self.expect("(")?;
            let mut args = Vec::new();
            let mut types = Vec::new();
            if self.peek() != ")" {
                loop {
                    let (id, t) = self.expr(depth + 1)?;
                    args.push(id);
                    types.push(t);
                    if self.peek() != "," {
                        break;
                    }
                    self.expect(",")?;
                }
            }
            self.expect(")")?;
            let ty = op.result(&types)?;
            Ok(self.emit(ty, Expr::Apply { op, args }))
        } else if let Some(v) = self.names.get(&token) {
            Ok(*v)
        } else if token == "true"
            || token == "false"
            || token.starts_with(|c: char| c.is_ascii_digit() || c == '-')
        {
            let value = Value::parse(&token)?;
            Ok(self.emit(value.ty, Expr::Const { value }))
        } else {
            Err(format!("unknown value {token}"))
        }
    }
}
pub fn parse(source: &str) -> Result<Function, String> {
    if source.len() > 1_000_000 {
        return Err("source exceeds 1 MB limit".into());
    }
    let mut p = Parser {
        tokens: lex(source)?,
        pos: 0,
        names: BTreeMap::new(),
        instructions: Vec::new(),
        next: 0,
    };
    p.expect("fn")?;
    p.ident()?;
    p.expect("(")?;
    let mut parameters = Vec::new();
    if p.peek() != ")" {
        loop {
            let name = p.ident()?;
            p.expect(":")?;
            let ty: Type = p.take()?.parse()?;
            let id = p.next;
            p.next += 1;
            if p.names.insert(name.clone(), (id, ty)).is_some() {
                return Err(format!("duplicate binding {name}"));
            }
            parameters.push(Param { id, ty });
            if p.peek() != "," {
                break;
            }
            p.expect(",")?;
        }
    }
    p.expect(")")?;
    p.expect("->")?;
    let return_type = p.take()?.parse()?;
    p.expect("{")?;
    while p.peek() == "let" {
        p.expect("let")?;
        let name = p.ident()?;
        p.expect(":")?;
        let ty: Type = p.take()?.parse()?;
        p.expect("=")?;
        let (id, actual) = p.expr(0)?;
        if ty != actual {
            return Err(format!("binding {name} type mismatch"));
        }
        p.expect(";")?;
        if p.names.insert(name.clone(), (id, ty)).is_some() {
            return Err(format!("duplicate binding {name}"));
        }
    }
    p.expect("return")?;
    let (value, ty) = p.expr(0)?;
    if ty != return_type {
        return Err("return type mismatch".into());
    }
    p.expect(";")?;
    p.expect("}")?;
    if p.pos != p.tokens.len() {
        return Err("expected exactly one function".into());
    }
    let f = Function {
        schema_version: SCHEMA_VERSION,
        parameters,
        return_type,
        entry: 0,
        blocks: vec![Block {
            id: 0,
            parameters: vec![],
            instructions: p.instructions,
            terminator: Terminator::Return { value },
        }],
    };
    f.normalized()
}
pub fn print_source(f: &Function) -> Result<String, String> {
    let f = f.normalized()?;
    if f.blocks.len() != 1 || !f.blocks[0].parameters.is_empty() {
        return Err("source printer supports straight-line functions only".into());
    }
    let mut s = format!(
        "fn candidate({}) -> {} {{\n",
        f.parameters
            .iter()
            .map(|p| format!("v{}: {}", p.id, p.ty))
            .collect::<Vec<_>>()
            .join(", "),
        f.return_type
    );
    for x in &f.blocks[0].instructions {
        let rhs = match &x.expr {
            Expr::Const { value } => value.literal(),
            Expr::Apply { op, args } => format!(
                "{op}({})",
                args.iter()
                    .map(|v| format!("v{v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        s.push_str(&format!("    let v{}: {} = {};\n", x.id, x.ty, rhs));
    }
    if let Terminator::Return { value } = f.blocks[0].terminator {
        s.push_str(&format!("    return v{value};\n}}\n"));
    } else {
        return Err("source printer requires return terminator".into());
    }
    Ok(s)
}
