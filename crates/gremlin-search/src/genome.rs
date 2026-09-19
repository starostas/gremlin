use crate::SearchConfig;
use gremlin_core::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gene {
    pub ty: Type,
    pub expr: Expr,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Genome {
    pub genes: Vec<Gene>,
    pub output: Id,
}
impl Genome {
    pub fn lower(&self, s: &Signature) -> Function {
        Function {
            schema_version: SCHEMA_VERSION,
            parameters: s
                .arguments
                .iter()
                .enumerate()
                .map(|(i, t)| Param {
                    id: i as Id,
                    ty: *t,
                })
                .collect(),
            return_type: s.return_type,
            entry: 0,
            blocks: vec![Block {
                id: 0,
                parameters: vec![],
                instructions: self
                    .genes
                    .iter()
                    .enumerate()
                    .map(|(i, g)| Instruction {
                        id: (s.arguments.len() + i) as Id,
                        ty: g.ty,
                        expr: g.expr.clone(),
                    })
                    .collect(),
                terminator: Terminator::Return { value: self.output },
            }],
        }
    }
    fn types(&self, s: &Signature) -> Vec<Type> {
        s.arguments
            .iter()
            .copied()
            .chain(self.genes.iter().map(|g| g.ty))
            .collect()
    }
    fn valid(&self, s: &Signature, c: &SearchConfig) -> bool {
        self.genes.len() <= c.max_instructions && self.lower(s).validate().is_ok()
    }
}
fn matching(types: &[Type], t: Type) -> Vec<Id> {
    types
        .iter()
        .enumerate()
        .filter(|(_, x)| **x == t)
        .map(|(i, _)| i as Id)
        .collect()
}
fn random_gene(
    types: &[Type],
    c: &SearchConfig,
    constants: &[Value],
    rng: &mut Rng,
) -> Option<Gene> {
    if !constants.is_empty() && (c.operators.is_empty() || rng.index(4) == 0) {
        let v = constants[rng.index(constants.len())];
        return Some(Gene {
            ty: v.ty,
            expr: Expr::Const { value: v },
        });
    }
    if c.operators.is_empty() || types.is_empty() {
        return None;
    }
    let op = c.operators[rng.index(c.operators.len())];
    let t = types[rng.index(types.len())];
    let mut args = vec![];
    if op == Op::Select {
        let b = matching(types, Type::Bool);
        if b.is_empty() {
            return None;
        }
        args.push(b[rng.index(b.len())]);
    }
    let ids = matching(types, t);
    for _ in args.len()..op.arity() {
        args.push(ids[rng.index(ids.len())]);
    }
    let arg_types: Vec<_> = args.iter().map(|id| types[*id as usize]).collect();
    let ty = op.result(&arg_types).ok()?;
    Some(Gene {
        ty,
        expr: Expr::Apply { op, args },
    })
}
/// Generic initial seeds: identities, constants, and every legal single operator over params/constants.
pub fn seeds(s: &Signature, c: &SearchConfig) -> Vec<Genome> {
    let constants: Vec<_> = c
        .constants
        .iter()
        .map(|v| Value::parse(v).unwrap())
        .collect();
    let mut out = vec![];
    for (i, t) in s.arguments.iter().enumerate() {
        if *t == s.return_type {
            out.push(Genome {
                genes: vec![],
                output: i as Id,
            });
        }
    }
    for v in &constants {
        if v.ty == s.return_type {
            out.push(Genome {
                genes: vec![Gene {
                    ty: v.ty,
                    expr: Expr::Const { value: *v },
                }],
                output: s.arguments.len() as Id,
            });
        }
    }
    let pool: Vec<_> = s
        .arguments
        .iter()
        .copied()
        .chain(constants.iter().map(|v| v.ty))
        .collect();
    for op in &c.operators {
        let n = pool.len();
        for a in 0..n {
            for b in 0..if op.arity() > 1 { n } else { 1 } {
                for d in 0..if op.arity() > 2 { n } else { 1 } {
                    let picks = [a, b, d];
                    let types: Vec<_> = picks[..op.arity()].iter().map(|i| pool[*i]).collect();
                    if op.result(&types) != Ok(s.return_type) {
                        continue;
                    }
                    let mut g = Genome {
                        genes: vec![],
                        output: 0,
                    };
                    let mut args = vec![];
                    for pick in &picks[..op.arity()] {
                        if *pick < s.arguments.len() {
                            args.push(*pick as Id);
                        } else {
                            let v = constants[*pick - s.arguments.len()];
                            let existing = g
                                .genes
                                .iter()
                                .position(|x| x.expr == Expr::Const { value: v });
                            let i = existing.unwrap_or_else(|| {
                                g.genes.push(Gene {
                                    ty: v.ty,
                                    expr: Expr::Const { value: v },
                                });
                                g.genes.len() - 1
                            });
                            args.push((s.arguments.len() + i) as Id);
                        }
                    }
                    g.output = (s.arguments.len() + g.genes.len()) as Id;
                    g.genes.push(Gene {
                        ty: s.return_type,
                        expr: Expr::Apply { op: *op, args },
                    });
                    if g.valid(s, c) {
                        out.push(g);
                    }
                }
            }
        }
    }
    out
}
pub fn random_genome(s: &Signature, c: &SearchConfig, rng: &mut Rng, fallback: &Genome) -> Genome {
    let constants: Vec<_> = c
        .constants
        .iter()
        .map(|v| Value::parse(v).unwrap())
        .collect();
    let mut g = Genome {
        genes: vec![],
        output: 0,
    };
    let count = 1 + rng.index(c.max_instructions);
    for _ in 0..count {
        if let Some(gene) = random_gene(&g.types(s), c, &constants, rng) {
            g.genes.push(gene);
        }
    }
    let outputs = matching(&g.types(s), s.return_type);
    if outputs.is_empty() {
        fallback.clone()
    } else {
        g.output = outputs[rng.index(outputs.len())];
        g
    }
}
fn remap_insert(g: &mut Genome, at: Id) {
    for gene in &mut g.genes {
        if let Expr::Apply { args, .. } = &mut gene.expr {
            for id in args {
                if *id >= at {
                    *id += 1;
                }
            }
        }
    }
    if g.output >= at {
        g.output += 1;
    }
}
/// Deletion repair is separate from validation. Replace removed references with lowest preceding typed ID.
pub fn delete(g: &Genome, s: &Signature, index: usize) -> Option<Genome> {
    if index >= g.genes.len() {
        return None;
    }
    let mut n = g.clone();
    let id = (s.arguments.len() + index) as Id;
    let ty = n.genes[index].ty;
    let preceding = g.types(s);
    let replacement = matching(&preceding[..id as usize], ty).first().copied();
    n.genes.remove(index);
    for gene in &mut n.genes {
        if let Expr::Apply { args, .. } = &mut gene.expr {
            for v in args {
                if *v == id {
                    *v = replacement?;
                } else if *v > id {
                    *v -= 1;
                }
            }
        }
    }
    if n.output == id {
        n.output = replacement?;
    } else if n.output > id {
        n.output -= 1;
    }
    Some(n)
}
pub fn mutate(parent: &Genome, s: &Signature, c: &SearchConfig, rng: &mut Rng) -> Genome {
    let constants: Vec<_> = c
        .constants
        .iter()
        .map(|v| Value::parse(v).unwrap())
        .collect();
    for _ in 0..8 {
        let mut g = parent.clone();
        let types = g.types(s);
        let mode = rng.index(7);
        let ok = match mode {
            0 => {
                let ids = matching(&types, s.return_type);
                g.output = ids[rng.index(ids.len())];
                true
            }
            1 if !g.genes.is_empty() && !c.operators.is_empty() => {
                let i = rng.index(g.genes.len());
                if let Expr::Apply { args, .. } = &mut g.genes[i].expr {
                    let candidate = c.operators[rng.index(c.operators.len())];
                    let ts: Vec<_> = args.iter().map(|id| types[*id as usize]).collect();
                    if candidate.result(&ts) == Ok(g.genes[i].ty) {
                        if let Expr::Apply { op, .. } = &mut g.genes[i].expr {
                            *op = candidate;
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            2 if !g.genes.is_empty() => {
                let i = rng.index(g.genes.len());
                if let Expr::Apply { args, .. } = &mut g.genes[i].expr {
                    let j = rng.index(args.len());
                    let ids = matching(&types[..s.arguments.len() + i], types[args[j] as usize]);
                    args[j] = ids[rng.index(ids.len())];
                    true
                } else {
                    false
                }
            }
            3 if !g.genes.is_empty() => {
                let i = rng.index(g.genes.len());
                if matches!(g.genes[i].expr, Expr::Const { .. }) {
                    let cs: Vec<_> = constants.iter().filter(|v| v.ty == g.genes[i].ty).collect();
                    if cs.is_empty() {
                        false
                    } else {
                        g.genes[i].expr = Expr::Const {
                            value: *cs[rng.index(cs.len())],
                        };
                        true
                    }
                } else {
                    false
                }
            }
            4 if g.genes.len() < c.max_instructions => {
                let i = rng.index(g.genes.len() + 1);
                let id = (s.arguments.len() + i) as Id;
                if let Some(gene) = random_gene(&types[..id as usize], c, &constants, rng) {
                    let ty = gene.ty;
                    remap_insert(&mut g, id);
                    g.genes.insert(i, gene);
                    if ty == s.return_type && rng.index(2) == 0 {
                        g.output = id;
                    }
                    true
                } else {
                    false
                }
            }
            5 if !g.genes.is_empty() => {
                let i = rng.index(g.genes.len());
                if let Some(n) = delete(&g, s, i) {
                    g = n;
                    true
                } else {
                    false
                }
            }
            6 if !g.genes.is_empty() => {
                let i = rng.index(g.genes.len());
                if let Some(gene) = random_gene(&types[..s.arguments.len() + i], c, &constants, rng)
                {
                    if gene.ty == g.genes[i].ty {
                        g.genes[i] = gene;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => false,
        };
        if ok && g.valid(s, c) {
            return g;
        }
    }
    parent.clone()
}
