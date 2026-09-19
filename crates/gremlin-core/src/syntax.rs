use crate::*;
use std::collections::BTreeMap;
pub(crate) fn lex(source: &str) -> Result<Vec<String>, String> {
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
    callees: BTreeMap<String, Signature>,
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
            let op: Option<Op> = token.parse().ok();
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
            if let Some(op) = op {
                let ty = op.result(&types)?;
                Ok(self.emit(ty, Expr::Apply { op, args }))
            } else {
                let signature = self
                    .callees
                    .get(&token)
                    .ok_or_else(|| format!("unknown operator or function {token}"))?;
                if types != signature.arguments {
                    return Err(format!("call signature mismatch for {token}"));
                }
                let ty = signature.return_type;
                Ok(self.emit(
                    ty,
                    Expr::Call {
                        function: token,
                        args,
                    },
                ))
            }
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
    parse_with_signatures(source, &BTreeMap::new())
}
pub(crate) fn parse_with_signatures(
    source: &str,
    callees: &BTreeMap<String, Signature>,
) -> Result<Function, String> {
    if source.len() > 1_000_000 {
        return Err("source exceeds 1 MB limit".into());
    }
    let tokens = lex(source)?;
    if tokens
        .iter()
        .any(|t| ["mut", "if", "else", "while", "loop", "break", "continue"].contains(&t.as_str()))
    {
        return crate::structured::parse_structured(tokens, callees);
    }
    let mut p = Parser {
        callees: callees.clone(),
        tokens,
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
    if p.peek() == "block" {
        return parse_cfg(p, parameters, return_type);
    }
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
        callees: used_callees(&p.instructions, callees),
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
    if f.blocks.len() != 1 || !matches!(f.blocks[0].terminator, Terminator::Return { .. }) {
        return print_cfg(&f);
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
            Expr::Call { function, args } => format!(
                "{function}({})",
                args.iter()
                    .map(|v| format!("v{v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
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

fn block_id(name: &str) -> Result<Id, String> {
    name.strip_prefix('b')
        .ok_or("block IDs must be b followed by an integer")?
        .parse()
        .map_err(|_| "invalid block ID".into())
}
fn cfg_edge(p: &mut Parser) -> Result<Edge, String> {
    let block = block_id(&p.take()?)?;
    p.expect("(")?;
    let mut args = vec![];
    if p.peek() != ")" {
        loop {
            args.push(p.expr(0)?.0);
            if p.peek() != "," {
                break;
            }
            p.take()?;
        }
    }
    p.expect(")")?;
    Ok(Edge { block, args })
}
fn parse_cfg(mut p: Parser, parameters: Vec<Param>, return_type: Type) -> Result<Function, String> {
    let mut blocks = vec![];
    while p.peek() == "block" {
        p.take()?;
        let id = block_id(&p.take()?)?;
        p.expect("(")?;
        let mut block_parameters = vec![];
        if p.peek() != ")" {
            loop {
                let name = p.ident()?;
                p.expect(":")?;
                let ty = p.take()?.parse()?;
                let id = p.next;
                p.next += 1;
                if p.names.insert(name.clone(), (id, ty)).is_some() {
                    return Err(format!("duplicate value {name}"));
                }
                block_parameters.push(Param { id, ty });
                if p.peek() != "," {
                    break;
                }
                p.take()?;
            }
        }
        p.expect(")")?;
        p.expect("{")?;
        while p.peek() == "let" {
            p.take()?;
            let name = p.ident()?;
            p.expect(":")?;
            let ty: Type = p.take()?.parse()?;
            p.expect("=")?;
            let (id, actual) = p.expr(0)?;
            if ty != actual {
                return Err("CFG binding type mismatch".into());
            }
            p.expect(";")?;
            if p.names.insert(name.clone(), (id, ty)).is_some() {
                return Err(format!("duplicate value {name}"));
            }
        }
        let terminator = match p.take()?.as_str() {
            "return" => {
                let (value, ty) = p.expr(0)?;
                if ty != return_type {
                    return Err("return type mismatch".into());
                }
                Terminator::Return { value }
            }
            "jump" => Terminator::Jump {
                edge: cfg_edge(&mut p)?,
            },
            "branch" => {
                let (condition, ty) = p.expr(0)?;
                if ty != Type::Bool {
                    return Err("branch condition must be bool".into());
                }
                p.expect(",")?;
                let if_true = cfg_edge(&mut p)?;
                p.expect(",")?;
                let if_false = cfg_edge(&mut p)?;
                Terminator::Branch {
                    condition,
                    if_true,
                    if_false,
                }
            }
            other => return Err(format!("expected CFG terminator, got {other}")),
        };
        p.expect(";")?;
        p.expect("}")?;
        blocks.push(Block {
            id,
            parameters: block_parameters,
            instructions: std::mem::take(&mut p.instructions),
            terminator,
        });
    }
    p.expect("}")?;
    if p.pos != p.tokens.len() {
        return Err("expected exactly one function".into());
    }
    let entry = blocks.first().ok_or("CFG requires a block")?.id;
    Function {
        callees: used_callees(
            &blocks
                .iter()
                .flat_map(|b| b.instructions.iter().cloned())
                .collect::<Vec<_>>(),
            &p.callees,
        ),
        schema_version: SCHEMA_VERSION,
        parameters,
        return_type,
        entry,
        blocks,
    }
    .normalized()
}
fn print_cfg(f: &Function) -> Result<String, String> {
    let params = |ps: &[Param]| {
        ps.iter()
            .map(|p| format!("v{}: {}", p.id, p.ty))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let edge = |e: &Edge| {
        format!(
            "b{}({})",
            e.block,
            e.args
                .iter()
                .map(|v| format!("v{v}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let mut source = format!(
        "fn candidate({}) -> {} {{\n",
        params(&f.parameters),
        f.return_type
    );
    for block in &f.blocks {
        source.push_str(&format!(
            "    block b{}({}) {{\n",
            block.id,
            params(&block.parameters)
        ));
        for i in &block.instructions {
            let expression = match &i.expr {
                Expr::Call { function, args } => format!(
                    "{function}({})",
                    args.iter()
                        .map(|v| format!("v{v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Expr::Const { value } => value.literal(),
                Expr::Apply { op, args } => format!(
                    "{op}({})",
                    args.iter()
                        .map(|v| format!("v{v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            source.push_str(&format!(
                "        let v{}: {} = {};\n",
                i.id, i.ty, expression
            ));
        }
        let terminator = match &block.terminator {
            Terminator::Return { value } => format!("return v{value}"),
            Terminator::Jump { edge: e } => format!("jump {}", edge(e)),
            Terminator::Branch {
                condition,
                if_true,
                if_false,
            } => format!("branch v{condition}, {}, {}", edge(if_true), edge(if_false)),
        };
        source.push_str(&format!("        {terminator};\n    }}\n"));
    }
    source.push_str("}\n");
    Ok(source)
}

pub(crate) fn used_callees(
    instructions: &[Instruction],
    signatures: &BTreeMap<String, Signature>,
) -> BTreeMap<String, Signature> {
    instructions
        .iter()
        .filter_map(|i| {
            if let Expr::Call { function, .. } = &i.expr {
                signatures
                    .get(function)
                    .map(|s| (function.clone(), s.clone()))
            } else {
                None
            }
        })
        .collect()
}
