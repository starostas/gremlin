use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub schema_version: u32,
    pub entry: String,
    pub functions: BTreeMap<String, Function>,
}
impl Module {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("unsupported module schema".into());
        }
        if !self.functions.contains_key(&self.entry) {
            return Err("missing module entry".into());
        }
        if self.functions.len() > 256 {
            return Err("module exceeds 256 functions".into());
        }
        for (name, f) in &self.functions {
            if name.is_empty()
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || name.bytes().next().unwrap().is_ascii_digit()
                || name.parse::<Op>().is_ok()
            {
                return Err("invalid or reserved function name".into());
            }
            f.validate()?;
            for (callee, signature) in &f.callees {
                let target = self
                    .functions
                    .get(callee)
                    .ok_or_else(|| format!("missing callee {callee}"))?;
                if target.signature() != *signature {
                    return Err(format!("callee signature mismatch for {callee}"));
                }
            }
        }
        Ok(())
    }
    pub fn signature(&self) -> Result<Signature, String> {
        self.functions
            .get(&self.entry)
            .map(Function::signature)
            .ok_or("missing entry".into())
    }
    pub fn normalized(&self) -> Result<Self, String> {
        self.validate()?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            entry: self.entry.clone(),
            functions: self
                .functions
                .iter()
                .map(|(n, f)| Ok((n.clone(), f.normalized()?)))
                .collect::<Result<_, String>>()?,
        })
    }
}
pub fn parse_module(source: &str) -> Result<Module, String> {
    if source.len() > 1_000_000 {
        return Err("source exceeds 1 MB limit".into());
    }
    let tokens = crate::syntax::lex(source)?;
    let mut pos = 0;
    let mut declarations = vec![];
    let mut signatures = BTreeMap::new();
    while pos < tokens.len() {
        let start = pos;
        if tokens.get(pos).map(String::as_str) != Some("fn") {
            return Err("expected function declaration".into());
        }
        pos += 1;
        let name = tokens.get(pos).ok_or("missing function name")?.clone();
        pos += 1;
        if tokens.get(pos).map(String::as_str) != Some("(") {
            return Err("expected parameter list".into());
        }
        pos += 1;
        let mut arguments = vec![];
        while tokens.get(pos).map(String::as_str) != Some(")") {
            tokens.get(pos).ok_or("unterminated parameter list")?;
            pos += 1;
            if tokens.get(pos).map(String::as_str) != Some(":") {
                return Err("expected parameter type".into());
            }
            pos += 1;
            arguments.push(tokens.get(pos).ok_or("missing parameter type")?.parse()?);
            pos += 1;
            if tokens.get(pos).map(String::as_str) == Some(",") {
                pos += 1;
            } else {
                break;
            }
        }
        if tokens.get(pos).map(String::as_str) != Some(")") {
            return Err("expected ')'".into());
        }
        pos += 1;
        if tokens.get(pos).map(String::as_str) != Some("->") {
            return Err("expected return type".into());
        }
        pos += 1;
        let return_type = tokens.get(pos).ok_or("missing return type")?.parse()?;
        pos += 1;
        if tokens.get(pos).map(String::as_str) != Some("{") {
            return Err("expected function body".into());
        }
        let mut depth = 0;
        loop {
            let token = tokens.get(pos).ok_or("unterminated function body")?;
            if token == "{" {
                depth += 1;
            } else if token == "}" {
                depth -= 1;
            }
            pos += 1;
            if depth == 0 {
                break;
            }
        }
        if signatures
            .insert(
                name.clone(),
                Signature {
                    arguments,
                    return_type,
                },
            )
            .is_some()
        {
            return Err(format!("duplicate function {name}"));
        }
        declarations.push((name, tokens[start..pos].join(" ")));
    }
    let entry = if signatures.contains_key("main") {
        String::from("main")
    } else {
        declarations.first().ok_or("empty module")?.0.clone()
    };
    let functions = declarations
        .into_iter()
        .map(|(name, source)| {
            Ok((
                name,
                crate::syntax::parse_with_signatures(&source, &signatures)?,
            ))
        })
        .collect::<Result<_, String>>()?;
    Module {
        schema_version: SCHEMA_VERSION,
        entry,
        functions,
    }
    .normalized()
}
pub fn print_module(module: &Module) -> Result<String, String> {
    let module = module.normalized()?;
    let mut names = vec![module.entry.clone()];
    names.extend(
        module
            .functions
            .keys()
            .filter(|n| **n != module.entry)
            .cloned(),
    );
    let mut result = String::new();
    for name in names {
        result.push_str(&print_source(&module.functions[&name])?.replacen(
            "fn candidate(",
            &format!("fn {name}("),
            1,
        ));
        result.push('\n');
    }
    Ok(result)
}
struct Frame {
    function: usize,
    block: usize,
    instruction: usize,
    values: Vec<Value>,
    edge_values: Vec<Value>,
    return_to: Option<Id>,
}
pub struct ModuleEvaluator {
    functions: Vec<Function>,
    names: BTreeMap<String, usize>,
    entry: usize,
}
impl ModuleEvaluator {
    pub fn new(module: &Module) -> Result<Self, String> {
        let module = module.normalized()?;
        let names: BTreeMap<_, _> = module
            .functions
            .keys()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        let entry = names[&module.entry];
        Ok(Self {
            functions: module.functions.into_values().collect(),
            names,
            entry,
        })
    }
    fn frame(&self, index: usize, args: &[Value], return_to: Option<Id>) -> Frame {
        let f = &self.functions[index];
        let count = f.parameters.len()
            + f.blocks
                .iter()
                .map(|b| b.parameters.len() + b.instructions.len())
                .sum::<usize>();
        let mut values = vec![Value::new(Type::Bool, 0); count];
        for (p, v) in f.parameters.iter().zip(args) {
            values[p.id as usize] = *v;
        }
        Frame {
            function: index,
            block: f.entry as usize,
            instruction: 0,
            values,
            edge_values: vec![
                Value::new(Type::Bool, 0);
                f.blocks
                    .iter()
                    .map(|b| b.parameters.len())
                    .max()
                    .unwrap_or(0)
            ],
            return_to,
        }
    }
    pub fn execute(&self, args: &[Value], budget: u64, max_call_depth: usize) -> Execution {
        let finish = |outcome, steps| Execution { outcome, steps };
        if let Err(e) = encode_transport(&self.functions[self.entry].signature(), args) {
            return finish(Outcome::Invalid(e), 0);
        }
        if max_call_depth > 1024 {
            return finish(Outcome::Invalid("max_call_depth exceeds 1024".into()), 0);
        }
        if max_call_depth == 0 {
            return finish(Outcome::Timeout("call depth budget exhausted".into()), 0);
        }
        let mut frames = vec![self.frame(self.entry, args, None)];
        let mut steps = 0;
        loop {
            if steps == budget {
                return finish(Outcome::Timeout("step budget exhausted".into()), steps);
            }
            let depth = frames.len();
            let frame = frames.last_mut().unwrap();
            let function = &self.functions[frame.function];
            let block = &function.blocks[frame.block];
            steps += 1;
            if let Some(instruction) = block.instructions.get(frame.instruction) {
                frame.instruction += 1;
                match &instruction.expr {
                    Expr::Const { value } => frame.values[instruction.id as usize] = *value,
                    Expr::Apply { op, args } => {
                        let mut values = [Value::new(Type::Bool, 0); 3];
                        for (i, id) in args.iter().enumerate() {
                            values[i] = frame.values[*id as usize];
                        }
                        match op.eval(&values[..args.len()]) {
                            Ok(v) => frame.values[instruction.id as usize] = v,
                            Err(e) => return finish(Outcome::Trap(e), steps),
                        }
                    }
                    Expr::Call { function, args } => {
                        if depth >= max_call_depth {
                            return finish(
                                Outcome::Timeout("call depth budget exhausted".into()),
                                steps,
                            );
                        }
                        let mut values = [Value::new(Type::Bool, 0); 4];
                        for (i, id) in args.iter().enumerate() {
                            values[i] = frame.values[*id as usize];
                        }
                        let child = self.frame(
                            self.names[function],
                            &values[..args.len()],
                            Some(instruction.id),
                        );
                        frames.push(child);
                    }
                }
                continue;
            }
            let edge = match &block.terminator {
                Terminator::Return { value } => {
                    let result = frame.values[*value as usize];
                    let target = frame.return_to;
                    frames.pop();
                    if let Some(caller) = frames.last_mut() {
                        caller.values[target.unwrap() as usize] = result;
                        continue;
                    }
                    return finish(Outcome::Completed(result), steps);
                }
                Terminator::Jump { edge } => edge,
                Terminator::Branch {
                    condition,
                    if_true,
                    if_false,
                } => {
                    if frame.values[*condition as usize].bits != 0 {
                        if_true
                    } else {
                        if_false
                    }
                }
            };
            for (i, id) in edge.args.iter().enumerate() {
                frame.edge_values[i] = frame.values[*id as usize];
            }
            frame.block = edge.block as usize;
            frame.instruction = 0;
            for (i, p) in function.blocks[frame.block].parameters.iter().enumerate() {
                frame.values[p.id as usize] = frame.edge_values[i];
            }
        }
    }
}
pub fn execute_module(
    module: &Module,
    args: &[Value],
    budget: u64,
    max_call_depth: usize,
) -> Execution {
    match ModuleEvaluator::new(module) {
        Ok(e) => e.execute(args, budget, max_call_depth),
        Err(error) => Execution {
            outcome: Outcome::Invalid(error),
            steps: 0,
        },
    }
}
