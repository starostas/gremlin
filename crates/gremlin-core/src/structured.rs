use crate::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
enum Expression {
    Name(String),
    Literal(Value),
    Apply(String, Vec<Expression>),
}
#[derive(Clone)]
enum Statement {
    Let(String, Type, bool, Expression),
    Assign(String, Expression),
    Return(Expression),
    If(Expression, Vec<Statement>, Vec<Statement>),
    While(Expression, Vec<Statement>),
    Loop(Vec<Statement>),
    Break,
    Continue,
}
struct Reader {
    tokens: Vec<String>,
    pos: usize,
}
impl Reader {
    fn peek(&self) -> &str {
        self.tokens.get(self.pos).map(String::as_str).unwrap_or("")
    }
    fn take(&mut self) -> Result<String, String> {
        let value = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or("unexpected end of source")?;
        self.pos += 1;
        Ok(value)
    }
    fn expect(&mut self, want: &str) -> Result<(), String> {
        let got = self.take()?;
        if got == want {
            Ok(())
        } else {
            Err(format!("expected '{want}', got '{got}'"))
        }
    }
    fn identifier(&mut self) -> Result<String, String> {
        let name = self.take()?;
        if !name
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
            || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || [
                "fn", "let", "mut", "return", "if", "else", "while", "loop", "break", "continue",
                "true", "false", "block", "jump", "branch",
            ]
            .contains(&name.as_str())
        {
            return Err(format!("expected identifier, got {name}"));
        }
        Ok(name)
    }
    fn expression(&mut self, depth: usize) -> Result<Expression, String> {
        if depth > 128 {
            return Err("expression nesting exceeds 128".into());
        }
        let token = self.take()?;
        if self.peek() == "(" {
            let op = token;
            self.expect("(")?;
            let mut args = vec![];
            if self.peek() != ")" {
                loop {
                    args.push(self.expression(depth + 1)?);
                    if self.peek() != "," {
                        break;
                    }
                    self.expect(",")?;
                }
            }
            self.expect(")")?;
            Ok(Expression::Apply(op, args))
        } else if token == "true"
            || token == "false"
            || token.starts_with(|c: char| c.is_ascii_digit() || c == '-')
        {
            Ok(Expression::Literal(Value::parse(&token)?))
        } else {
            Ok(Expression::Name(token))
        }
    }
    fn body(&mut self, depth: usize) -> Result<Vec<Statement>, String> {
        if depth > 128 {
            return Err("statement nesting exceeds 128".into());
        }
        self.expect("{")?;
        let mut statements = vec![];
        while self.peek() != "}" {
            statements.push(self.statement(depth + 1)?);
        }
        self.expect("}")?;
        Ok(statements)
    }
    fn statement(&mut self, depth: usize) -> Result<Statement, String> {
        match self.peek() {
            "let" => {
                self.take()?;
                let mutable = self.peek() == "mut";
                if mutable {
                    self.take()?;
                }
                let name = self.identifier()?;
                self.expect(":")?;
                let ty = self.take()?.parse()?;
                self.expect("=")?;
                let expression = self.expression(0)?;
                self.expect(";")?;
                Ok(Statement::Let(name, ty, mutable, expression))
            }
            "return" => {
                self.take()?;
                let expression = self.expression(0)?;
                self.expect(";")?;
                Ok(Statement::Return(expression))
            }
            "if" => {
                self.take()?;
                let condition = self.expression(0)?;
                let yes = self.body(depth)?;
                let no = if self.peek() == "else" {
                    self.take()?;
                    if self.peek() == "if" {
                        vec![self.statement(depth + 1)?]
                    } else {
                        self.body(depth)?
                    }
                } else {
                    vec![]
                };
                Ok(Statement::If(condition, yes, no))
            }
            "while" => {
                self.take()?;
                let condition = self.expression(0)?;
                Ok(Statement::While(condition, self.body(depth)?))
            }
            "loop" => {
                self.take()?;
                Ok(Statement::Loop(self.body(depth)?))
            }
            "break" => {
                self.take()?;
                self.expect(";")?;
                Ok(Statement::Break)
            }
            "continue" => {
                self.take()?;
                self.expect(";")?;
                Ok(Statement::Continue)
            }
            _ => {
                let name = self.identifier()?;
                self.expect("=")?;
                let expression = self.expression(0)?;
                self.expect(";")?;
                Ok(Statement::Assign(name, expression))
            }
        }
    }
}
#[derive(Clone)]
struct Binding {
    id: Id,
    ty: Type,
    mutable: bool,
}
type Environment = BTreeMap<String, Binding>;
struct PendingBlock {
    id: Id,
    parameters: Vec<Param>,
    instructions: Vec<Instruction>,
    terminator: Option<Terminator>,
}
struct LoopContext {
    header: Id,
    exit: Id,
    names: Vec<String>,
    has_break: bool,
}
struct Builder {
    callees: BTreeMap<String, Signature>,
    blocks: Vec<PendingBlock>,
    current: Option<Id>,
    next_value: Id,
    return_type: Type,
    loops: Vec<LoopContext>,
}
impl Builder {
    fn block(&mut self, env: &Environment) -> (Id, Environment) {
        let id = self.blocks.len() as Id;
        let mut parameters = vec![];
        let mut mapped = Environment::new();
        for (name, binding) in env {
            let value = self.next_value;
            self.next_value += 1;
            parameters.push(Param {
                id: value,
                ty: binding.ty,
            });
            mapped.insert(
                name.clone(),
                Binding {
                    id: value,
                    ty: binding.ty,
                    mutable: binding.mutable,
                },
            );
        }
        self.blocks.push(PendingBlock {
            id,
            parameters,
            instructions: vec![],
            terminator: None,
        });
        (id, mapped)
    }
    fn emit(&mut self, ty: Type, expr: Expr) -> Result<Binding, String> {
        let block = self.current.ok_or("unreachable expression")?;
        let id = self.next_value;
        self.next_value += 1;
        self.blocks[block as usize]
            .instructions
            .push(Instruction { id, ty, expr });
        Ok(Binding {
            id,
            ty,
            mutable: false,
        })
    }
    fn expression(
        &mut self,
        expression: &Expression,
        env: &Environment,
    ) -> Result<Binding, String> {
        match expression {
            Expression::Name(name) => env
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unknown value {name}")),
            Expression::Literal(value) => self.emit(value.ty, Expr::Const { value: *value }),
            Expression::Apply(op, args) => {
                let values = args
                    .iter()
                    .map(|e| self.expression(e, env))
                    .collect::<Result<Vec<_>, _>>()?;
                let types = values.iter().map(|v| v.ty).collect::<Vec<_>>();
                let args = values.iter().map(|v| v.id).collect();
                if let Ok(operator) = op.parse::<Op>() {
                    let ty = operator.result(&types)?;
                    self.emit(ty, Expr::Apply { op: operator, args })
                } else {
                    let signature = self
                        .callees
                        .get(op)
                        .ok_or_else(|| format!("unknown function {op}"))?;
                    if signature.arguments != types {
                        return Err(format!("call signature mismatch for {op}"));
                    }
                    let ty = signature.return_type;
                    self.emit(
                        ty,
                        Expr::Call {
                            function: op.clone(),
                            args,
                        },
                    )
                }
            }
        }
    }
    fn terminate(&mut self, t: Terminator) -> Result<(), String> {
        let id = self.current.take().ok_or("unreachable statement")?;
        self.blocks[id as usize].terminator = Some(t);
        Ok(())
    }
    fn edge(block: Id, names: &[String], env: &Environment) -> Edge {
        Edge {
            block,
            args: names.iter().map(|n| env[n].id).collect(),
        }
    }
    fn statements(
        &mut self,
        statements: &[Statement],
        env: &mut Environment,
    ) -> Result<(), String> {
        for statement in statements {
            if self.current.is_none() {
                return Err(
                    "unreachable statement after return/break/continue/infinite loop".into(),
                );
            }
            match statement {
                Statement::Let(name, ty, mutable, expr) => {
                    if env.contains_key(name) {
                        return Err(format!("duplicate or shadowed binding {name}"));
                    }
                    let mut value = self.expression(expr, env)?;
                    if value.ty != *ty {
                        return Err(format!("binding {name} type mismatch"));
                    }
                    value.mutable = *mutable;
                    env.insert(name.clone(), value);
                }
                Statement::Assign(name, expr) => {
                    let old = env
                        .get(name)
                        .ok_or_else(|| format!("unknown assignment {name}"))?;
                    if !old.mutable {
                        return Err(format!("cannot assign immutable binding {name}"));
                    }
                    let ty = old.ty;
                    let mut value = self.expression(expr, env)?;
                    if ty != value.ty {
                        return Err(format!("assignment {name} type mismatch"));
                    }
                    value.mutable = true;
                    env.insert(name.clone(), value);
                }
                Statement::Return(expr) => {
                    let value = self.expression(expr, env)?;
                    if value.ty != self.return_type {
                        return Err("return type mismatch".into());
                    }
                    self.terminate(Terminator::Return { value: value.id })?;
                }
                Statement::If(condition, yes, no) => {
                    let value = self.expression(condition, env)?;
                    if value.ty != Type::Bool {
                        return Err("if condition must be bool".into());
                    }
                    let names: Vec<_> = env.keys().cloned().collect();
                    let (yes_id, mut yes_env) = self.block(env);
                    let (no_id, mut no_env) = self.block(env);
                    self.terminate(Terminator::Branch {
                        condition: value.id,
                        if_true: Self::edge(yes_id, &names, env),
                        if_false: Self::edge(no_id, &names, env),
                    })?;
                    self.current = Some(yes_id);
                    self.statements(yes, &mut yes_env)?;
                    let yes_end = self.current.take();
                    self.current = Some(no_id);
                    self.statements(no, &mut no_env)?;
                    let no_end = self.current.take();
                    if yes_end.is_some() || no_end.is_some() {
                        let (join, joined) = self.block(env);
                        for (end, branch_env) in [(yes_end, yes_env), (no_end, no_env)] {
                            if let Some(end) = end {
                                self.blocks[end as usize].terminator = Some(Terminator::Jump {
                                    edge: Self::edge(join, &names, &branch_env),
                                });
                            }
                        }
                        *env = joined;
                        self.current = Some(join);
                    }
                }
                Statement::While(condition, body) => self.loop_body(Some(condition), body, env)?,
                Statement::Loop(body) => self.loop_body(None, body, env)?,
                Statement::Break | Statement::Continue => {
                    let is_break = matches!(statement, Statement::Break);
                    let context = self.loops.last_mut().ok_or("break/continue outside loop")?;
                    if is_break {
                        context.has_break = true;
                    }
                    let target = if is_break {
                        context.exit
                    } else {
                        context.header
                    };
                    let edge = Self::edge(target, &context.names, env);
                    self.terminate(Terminator::Jump { edge })?;
                }
            }
        }
        Ok(())
    }
    fn loop_body(
        &mut self,
        condition: Option<&Expression>,
        body: &[Statement],
        env: &mut Environment,
    ) -> Result<(), String> {
        let names: Vec<_> = env.keys().cloned().collect();
        let (header, mut header_env) = self.block(env);
        let (exit, exit_env) = self.block(env);
        self.terminate(Terminator::Jump {
            edge: Self::edge(header, &names, env),
        })?;
        self.current = Some(header);
        if let Some(condition) = condition {
            let value = self.expression(condition, &header_env)?;
            if value.ty != Type::Bool {
                return Err("while condition must be bool".into());
            }
            let (body_id, body_env) = self.block(&header_env);
            self.terminate(Terminator::Branch {
                condition: value.id,
                if_true: Self::edge(body_id, &names, &header_env),
                if_false: Self::edge(exit, &names, &header_env),
            })?;
            self.current = Some(body_id);
            header_env = body_env;
        }
        self.loops.push(LoopContext {
            header,
            exit,
            names: names.clone(),
            has_break: false,
        });
        self.statements(body, &mut header_env)?;
        if self.current.is_some() {
            self.terminate(Terminator::Jump {
                edge: Self::edge(header, &names, &header_env),
            })?;
        }
        let context = self.loops.pop().unwrap();
        if condition.is_some() || context.has_break {
            self.current = Some(exit);
            *env = exit_env;
        }
        Ok(())
    }
}
pub fn parse_structured(
    tokens: Vec<String>,
    callees: &BTreeMap<String, Signature>,
) -> Result<Function, String> {
    let mut reader = Reader { tokens, pos: 0 };
    reader.expect("fn")?;
    reader.identifier()?;
    reader.expect("(")?;
    let mut parameters = vec![];
    let mut env = Environment::new();
    if reader.peek() != ")" {
        loop {
            let name = reader.identifier()?;
            reader.expect(":")?;
            let ty = reader.take()?.parse()?;
            let id = parameters.len() as Id;
            if env
                .insert(
                    name.clone(),
                    Binding {
                        id,
                        ty,
                        mutable: false,
                    },
                )
                .is_some()
            {
                return Err(format!("duplicate parameter {name}"));
            }
            parameters.push(Param { id, ty });
            if reader.peek() != "," {
                break;
            }
            reader.take()?;
        }
    }
    reader.expect(")")?;
    reader.expect("->")?;
    let return_type = reader.take()?.parse()?;
    let statements = reader.body(0)?;
    if reader.pos != reader.tokens.len() {
        return Err("expected exactly one function".into());
    }
    Signature {
        arguments: parameters.iter().map(|p| p.ty).collect(),
        return_type,
    }
    .validate()?;
    let mut builder = Builder {
        callees: callees.clone(),
        blocks: vec![PendingBlock {
            id: 0,
            parameters: vec![],
            instructions: vec![],
            terminator: None,
        }],
        current: Some(0),
        next_value: parameters.len() as Id,
        return_type,
        loops: vec![],
    };
    builder.statements(&statements, &mut env)?;
    if builder.current.is_some() {
        return Err("function can fall through without returning".into());
    }
    let mut reachable = BTreeSet::new();
    let mut pending = vec![0];
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let t = builder.blocks[id as usize]
            .terminator
            .as_ref()
            .ok_or("unterminated reachable block")?;
        for e in t.edges() {
            pending.push(e.block);
        }
    }
    let blocks: Vec<Block> = builder
        .blocks
        .into_iter()
        .filter(|b| reachable.contains(&b.id))
        .map(|b| Block {
            id: b.id,
            parameters: b.parameters,
            instructions: b.instructions,
            terminator: b.terminator.unwrap(),
        })
        .collect();
    Function {
        callees: crate::syntax::used_callees(
            &blocks
                .iter()
                .flat_map(|b| b.instructions.iter().cloned())
                .collect::<Vec<_>>(),
            callees,
        ),
        schema_version: SCHEMA_VERSION,
        parameters,
        return_type,
        entry: 0,
        blocks,
    }
    .normalized()
}
