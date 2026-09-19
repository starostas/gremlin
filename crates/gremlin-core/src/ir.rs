use crate::{Op, Signature, Type, Value, SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
pub type Id = u32;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Param {
    pub id: Id,
    pub ty: Type,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expr {
    Call { function: String, args: Vec<Id> },
    Const { value: Value },
    Apply { op: Op, args: Vec<Id> },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instruction {
    pub id: Id,
    pub ty: Type,
    pub expr: Expr,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub block: Id,
    pub args: Vec<Id>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Terminator {
    Return {
        value: Id,
    },
    Jump {
        edge: Edge,
    },
    Branch {
        condition: Id,
        if_true: Edge,
        if_false: Edge,
    },
}
impl Terminator {
    pub fn edges(&self) -> Vec<&Edge> {
        match self {
            Self::Return { .. } => vec![],
            Self::Jump { edge } => vec![edge],
            Self::Branch {
                if_true, if_false, ..
            } => vec![if_true, if_false],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: Id,
    pub parameters: Vec<Param>,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub callees: BTreeMap<String, Signature>,
    pub schema_version: u32,
    pub parameters: Vec<Param>,
    pub return_type: Type,
    pub entry: Id,
    pub blocks: Vec<Block>,
}
impl Function {
    pub fn signature(&self) -> Signature {
        Signature {
            arguments: self.parameters.iter().map(|p| p.ty).collect(),
            return_type: self.return_type,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("unsupported IR schema".into());
        }
        self.signature().validate()?;
        let mut blocks = BTreeMap::new();
        for b in &self.blocks {
            if blocks.insert(b.id, b).is_some() {
                return Err(format!("duplicate block ID {}", b.id));
            }
        }
        let entry = blocks.get(&self.entry).ok_or("missing entry block")?;
        if !entry.parameters.is_empty() {
            return Err("entry block cannot have block parameters".into());
        }
        let mut reachable = BTreeSet::new();
        let mut work = vec![self.entry];
        let mut preds: BTreeMap<Id, BTreeSet<Id>> =
            blocks.keys().map(|id| (*id, BTreeSet::new())).collect();
        while let Some(id) = work.pop() {
            if !reachable.insert(id) {
                continue;
            }
            for e in blocks[&id].terminator.edges() {
                if !blocks.contains_key(&e.block) {
                    return Err(format!("missing block {}", e.block));
                }
                preds.get_mut(&e.block).unwrap().insert(id);
                work.push(e.block);
            }
        }
        if reachable.len() != blocks.len() {
            return Err("unreachable block".into());
        }
        let mut dom: BTreeMap<Id, BTreeSet<Id>> = blocks
            .keys()
            .map(|id| {
                (
                    *id,
                    if *id == self.entry {
                        BTreeSet::from([*id])
                    } else {
                        reachable.clone()
                    },
                )
            })
            .collect();
        loop {
            let mut changed = false;
            for id in blocks.keys().filter(|id| **id != self.entry) {
                let mut d = reachable.clone();
                for p in &preds[id] {
                    d = d.intersection(&dom[p]).copied().collect();
                }
                d.insert(*id);
                if d != dom[id] {
                    dom.insert(*id, d);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // Definition location: None for function args; -1 for block parameters.
        let mut defs: BTreeMap<Id, (Type, Option<(Id, isize)>)> = BTreeMap::new();
        let mut define = |id, ty, loc| {
            if defs.insert(id, (ty, loc)).is_some() {
                Err(format!("duplicate value ID {id}"))
            } else {
                Ok(())
            }
        };
        for p in &self.parameters {
            define(p.id, p.ty, None)?;
        }
        for b in &self.blocks {
            for p in &b.parameters {
                define(p.id, p.ty, Some((b.id, -1)))?;
            }
            for (i, x) in b.instructions.iter().enumerate() {
                define(x.id, x.ty, Some((b.id, i as isize)))?;
            }
        }
        let use_value = |id: Id, b: Id, pos: isize| -> Result<Type, String> {
            let (ty, loc) = defs.get(&id).ok_or_else(|| format!("missing value {id}"))?;
            if let Some((db, dp)) = loc {
                if *db == b {
                    if *dp >= pos {
                        return Err(format!("use-before-definition of {id}"));
                    }
                } else if !dom[&b].contains(db) {
                    return Err(format!("non-dominating use of {id}"));
                }
            }
            Ok(*ty)
        };
        for b in &self.blocks {
            for (i, x) in b.instructions.iter().enumerate() {
                let actual = match &x.expr {
                    Expr::Call { function, args } => {
                        let signature = self
                            .callees
                            .get(function)
                            .ok_or_else(|| format!("undeclared callee {function}"))?;
                        signature.validate()?;
                        let types = args
                            .iter()
                            .map(|id| use_value(*id, b.id, i as isize))
                            .collect::<Result<Vec<_>, _>>()?;
                        if types != signature.arguments {
                            return Err(format!("call signature mismatch for {function}"));
                        }
                        signature.return_type
                    }
                    Expr::Const { value } => {
                        if value.bits > value.ty.mask() {
                            return Err("constant bit pattern exceeds width".into());
                        }
                        value.ty
                    }
                    Expr::Apply { op, args } => op.result(
                        &args
                            .iter()
                            .map(|id| use_value(*id, b.id, i as isize))
                            .collect::<Result<Vec<_>, _>>()?,
                    )?,
                };
                if actual != x.ty {
                    return Err(format!("instruction {} type mismatch", x.id));
                }
            }
            let pos = b.instructions.len() as isize;
            match &b.terminator {
                Terminator::Return { value } => {
                    if use_value(*value, b.id, pos)? != self.return_type {
                        return Err("return type mismatch".into());
                    }
                }
                Terminator::Branch { condition, .. } => {
                    if use_value(*condition, b.id, pos)? != Type::Bool {
                        return Err("branch condition must be bool".into());
                    }
                }
                _ => {}
            }
            for e in b.terminator.edges() {
                let p = &blocks[&e.block].parameters;
                if e.args.len() != p.len() {
                    return Err("edge argument count mismatch".into());
                }
                for (v, p) in e.args.iter().zip(p) {
                    if use_value(*v, b.id, pos)? != p.ty {
                        return Err("edge argument type mismatch".into());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn normalized(&self) -> Result<Self, String> {
        self.validate()?;
        let mut order = Vec::new();
        let mut queue = VecDeque::from([self.entry]);
        let mut seen = BTreeSet::new();
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            let b = self.blocks.iter().find(|b| b.id == id).unwrap();
            order.push(b);
            for e in b.terminator.edges() {
                queue.push_back(e.block);
            }
        }
        let bm: BTreeMap<_, _> = order
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id, i as Id))
            .collect();
        let mut vm = BTreeMap::new();
        for p in &self.parameters {
            vm.insert(p.id, vm.len() as Id);
        }
        for b in &order {
            for p in &b.parameters {
                vm.insert(p.id, vm.len() as Id);
            }
            for x in &b.instructions {
                vm.insert(x.id, vm.len() as Id);
            }
        }
        let param = |p: &Param| Param {
            id: vm[&p.id],
            ty: p.ty,
        };
        let edge = |e: &Edge| Edge {
            block: bm[&e.block],
            args: e.args.iter().map(|v| vm[v]).collect(),
        };
        Ok(Self {
            callees: self.callees.clone(),
            schema_version: SCHEMA_VERSION,
            parameters: self.parameters.iter().map(param).collect(),
            return_type: self.return_type,
            entry: 0,
            blocks: order
                .iter()
                .map(|b| Block {
                    id: bm[&b.id],
                    parameters: b.parameters.iter().map(param).collect(),
                    instructions: b
                        .instructions
                        .iter()
                        .map(|x| Instruction {
                            id: vm[&x.id],
                            ty: x.ty,
                            expr: match &x.expr {
                                Expr::Call { function, args } => Expr::Call {
                                    function: function.clone(),
                                    args: args.iter().map(|v| vm[v]).collect(),
                                },
                                Expr::Const { value } => Expr::Const { value: *value },
                                Expr::Apply { op, args } => Expr::Apply {
                                    op: *op,
                                    args: args.iter().map(|v| vm[v]).collect(),
                                },
                            },
                        })
                        .collect(),
                    terminator: match &b.terminator {
                        Terminator::Return { value } => Terminator::Return { value: vm[value] },
                        Terminator::Jump { edge: e } => Terminator::Jump { edge: edge(e) },
                        Terminator::Branch {
                            condition,
                            if_true,
                            if_false,
                        } => Terminator::Branch {
                            condition: vm[condition],
                            if_true: edge(if_true),
                            if_false: edge(if_false),
                        },
                    },
                })
                .collect(),
        })
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(&self.normalized()?).map_err(|e| e.to_string())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "detail", rename_all = "snake_case")]
pub enum Outcome {
    Completed(Value),
    Timeout(String),
    Invalid(String),
    Trap(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Execution {
    pub outcome: Outcome,
    pub steps: u64,
}
/// Owns dense validated IR and reusable register/edge buffers. No per-case allocation on success.
pub struct Evaluator {
    function: Function,
    values: Vec<Value>,
    edge_values: Vec<Value>,
}
impl Evaluator {
    pub fn new(f: &Function) -> Result<Self, String> {
        if f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.expr, Expr::Call { .. }))
        {
            return Err("internal calls require a validated module evaluator".into());
        }
        let function = f.normalized()?;
        let count = function.parameters.len()
            + function
                .blocks
                .iter()
                .map(|b| b.parameters.len() + b.instructions.len())
                .sum::<usize>();
        let max_edge = function
            .blocks
            .iter()
            .map(|b| b.parameters.len())
            .max()
            .unwrap_or(0);
        Ok(Self {
            function,
            values: vec![Value::new(Type::Bool, 0); count],
            edge_values: vec![Value::new(Type::Bool, 0); max_edge],
        })
    }
    pub fn execute(&mut self, args: &[Value], budget: u64) -> Execution {
        let mut steps = 0;
        let finish = |outcome, steps| Execution { outcome, steps };
        if args.len() != self.function.parameters.len()
            || args
                .iter()
                .zip(&self.function.parameters)
                .any(|(v, p)| v.ty != p.ty || v.bits > v.ty.mask())
        {
            return finish(
                Outcome::Invalid("argument count, type, or width mismatch".into()),
                0,
            );
        }
        for (p, v) in self.function.parameters.iter().zip(args) {
            self.values[p.id as usize] = *v;
        }
        let mut block = self.function.entry as usize;
        loop {
            let b = &self.function.blocks[block];
            for x in &b.instructions {
                if steps == budget {
                    return finish(Outcome::Timeout("step budget exhausted".into()), steps);
                }
                steps += 1;
                let result = match &x.expr {
                    Expr::Call { .. } => {
                        unreachable!("calls rejected during evaluator preparation")
                    }
                    Expr::Const { value } => Ok(*value),
                    Expr::Apply { op, args } => {
                        let mut v = [Value::new(Type::Bool, 0); 3];
                        for (i, id) in args.iter().enumerate() {
                            v[i] = self.values[*id as usize];
                        }
                        op.eval(&v[..args.len()])
                    }
                };
                match result {
                    Ok(v) => self.values[x.id as usize] = v,
                    Err(e) => return finish(Outcome::Trap(e), steps),
                }
            }
            if steps == budget {
                return finish(Outcome::Timeout("step budget exhausted".into()), steps);
            }
            steps += 1;
            let edge = match &b.terminator {
                Terminator::Return { value } => {
                    return finish(Outcome::Completed(self.values[*value as usize]), steps)
                }
                Terminator::Jump { edge } => edge,
                Terminator::Branch {
                    condition,
                    if_true,
                    if_false,
                } => {
                    if self.values[*condition as usize].bits != 0 {
                        if_true
                    } else {
                        if_false
                    }
                }
            };
            for (i, id) in edge.args.iter().enumerate() {
                self.edge_values[i] = self.values[*id as usize];
            }
            block = edge.block as usize;
            for (i, p) in self.function.blocks[block].parameters.iter().enumerate() {
                self.values[p.id as usize] = self.edge_values[i];
            }
        }
    }
}
pub fn execute(f: &Function, args: &[Value], budget: u64) -> Execution {
    match Evaluator::new(f) {
        Ok(mut e) => e.execute(args, budget),
        Err(e) => Execution {
            outcome: Outcome::Invalid(e),
            steps: 0,
        },
    }
}
