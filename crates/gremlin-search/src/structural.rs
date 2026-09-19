use crate::{Genome, SearchConfig};
use gremlin_core::*;
use std::collections::BTreeSet;
fn values(f: &Function) -> Vec<(Id, Type)> {
    f.parameters
        .iter()
        .map(|p| (p.id, p.ty))
        .chain(f.blocks.iter().flat_map(|b| {
            b.parameters
                .iter()
                .map(|p| (p.id, p.ty))
                .chain(b.instructions.iter().map(|i| (i.id, i.ty)))
        }))
        .collect()
}
fn instruction(id: Id, ty: Type, expr: Expr) -> Instruction {
    Instruction { id, ty, expr }
}
fn constant(id: Id, ty: Type, bits: u64) -> Instruction {
    instruction(
        id,
        ty,
        Expr::Const {
            value: Value::new(ty, bits),
        },
    )
}
fn application(id: Id, ty: Type, op: Op, args: Vec<Id>) -> Instruction {
    instruction(id, ty, Expr::Apply { op, args })
}
fn trim(f: &mut Function) {
    let mut seen = BTreeSet::new();
    let mut work = vec![f.entry];
    while let Some(id) = work.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(b) = f.blocks.iter().find(|b| b.id == id) {
            for e in b.terminator.edges() {
                work.push(e.block);
            }
        }
    }
    f.blocks.retain(|b| seen.contains(&b.id));
}
pub fn mutate_cfg(parent: &Genome, s: &Signature, c: &SearchConfig, rng: &mut Rng) -> Genome {
    for _ in 0..8 {
        let Ok(mut f) = parent.lower(s).normalized() else {
            return parent.clone();
        };
        let vals = values(&f);
        let next = vals.iter().map(|(id, _)| id + 1).max().unwrap_or(0);
        let which = rng.index(f.blocks.len());
        let mut changed = false;
        match rng.index(6) {
            0 if f.blocks.len() + 2 <= c.max_blocks => {
                if let Terminator::Return { value } = f.blocks[which].terminator {
                    let (a, ty) = vals[rng.index(vals.len())];
                    let bs: Vec<_> = vals.iter().filter(|(_, t)| *t == ty).collect();
                    let b = bs[rng.index(bs.len())].0;
                    let ops: Vec<_> = c
                        .operators
                        .iter()
                        .filter(|o| o.result(&[ty, ty]) == Ok(Type::Bool))
                        .collect();
                    let alternatives: Vec<_> =
                        vals.iter().filter(|(_, t)| *t == s.return_type).collect();
                    if !ops.is_empty() && !alternatives.is_empty() {
                        let op = *ops[rng.index(ops.len())];
                        let other = alternatives[rng.index(alternatives.len())].0;
                        let yes = f.blocks.len() as Id;
                        let no = yes + 1;
                        f.blocks[which].instructions.push(application(
                            next,
                            Type::Bool,
                            op,
                            vec![a, b],
                        ));
                        f.blocks[which].terminator = Terminator::Branch {
                            condition: next,
                            if_true: Edge {
                                block: yes,
                                args: vec![],
                            },
                            if_false: Edge {
                                block: no,
                                args: vec![],
                            },
                        };
                        for (id, result) in [(yes, value), (no, other)] {
                            f.blocks.push(Block {
                                id,
                                parameters: vec![],
                                instructions: vec![],
                                terminator: Terminator::Return { value: result },
                            });
                        }
                        changed = true;
                    }
                }
            }
            1 if c.loop_bound > 0
                && f.blocks.len() + 4 <= c.max_blocks
                && c.operators.contains(&Op::Add)
                && c.operators.contains(&Op::Ult) =>
            {
                let counters: Vec<_> = f
                    .parameters
                    .iter()
                    .filter(|p| !p.ty.signed() && p.ty.integer())
                    .collect();
                let operations: Vec<_> = c
                    .operators
                    .iter()
                    .filter(|op| op.result(&[s.return_type, s.return_type]) == Ok(s.return_type))
                    .collect();
                let operands: Vec<_> = vals.iter().filter(|(_, ty)| *ty == s.return_type).collect();
                if let Terminator::Return { value } = f.blocks[which].terminator {
                    if !counters.is_empty() && !operations.is_empty() && !operands.is_empty() {
                        let counter = counters[rng.index(counters.len())];
                        let ty = counter.ty;
                        let count_parameter = counter.id;
                        let op = *operations[rng.index(operations.len())];
                        let operand = operands[rng.index(operands.len())].0;
                        let header = f.blocks.len() as Id;
                        let cap = header + 1;
                        let body = header + 2;
                        let exit = header + 3;
                        f.blocks[which].instructions.extend([
                            constant(next, ty, 0),
                            constant(next + 1, ty, 1),
                            constant(next + 2, ty, c.loop_bound),
                        ]);
                        f.blocks[which].terminator = Terminator::Jump {
                            edge: Edge {
                                block: header,
                                args: vec![next, value],
                            },
                        };
                        f.blocks.push(Block {
                            id: header,
                            parameters: vec![
                                Param { id: next + 3, ty },
                                Param {
                                    id: next + 4,
                                    ty: s.return_type,
                                },
                            ],
                            instructions: vec![application(
                                next + 5,
                                Type::Bool,
                                Op::Ult,
                                vec![next + 3, count_parameter],
                            )],
                            terminator: Terminator::Branch {
                                condition: next + 5,
                                if_true: Edge {
                                    block: cap,
                                    args: vec![],
                                },
                                if_false: Edge {
                                    block: exit,
                                    args: vec![next + 4],
                                },
                            },
                        });
                        f.blocks.push(Block {
                            id: cap,
                            parameters: vec![],
                            instructions: vec![application(
                                next + 6,
                                Type::Bool,
                                Op::Ult,
                                vec![next + 3, next + 2],
                            )],
                            terminator: Terminator::Branch {
                                condition: next + 6,
                                if_true: Edge {
                                    block: body,
                                    args: vec![],
                                },
                                if_false: Edge {
                                    block: exit,
                                    args: vec![next + 4],
                                },
                            },
                        });
                        f.blocks.push(Block {
                            id: body,
                            parameters: vec![],
                            instructions: vec![
                                application(next + 7, ty, Op::Add, vec![next + 3, next + 1]),
                                application(next + 8, s.return_type, op, vec![next + 4, operand]),
                            ],
                            terminator: Terminator::Jump {
                                edge: Edge {
                                    block: header,
                                    args: vec![next + 7, next + 8],
                                },
                            },
                        });
                        f.blocks.push(Block {
                            id: exit,
                            parameters: vec![Param {
                                id: next + 9,
                                ty: s.return_type,
                            }],
                            instructions: vec![],
                            terminator: Terminator::Return { value: next + 9 },
                        });
                        changed = true;
                    }
                }
            }
            2 => {
                if let Terminator::Branch {
                    if_true, if_false, ..
                } = &mut f.blocks[which].terminator
                {
                    std::mem::swap(if_true, if_false);
                    changed = true;
                }
            }
            3 => {
                if let Terminator::Branch {
                    if_true, if_false, ..
                } = &f.blocks[which].terminator
                {
                    let edge = if rng.index(2) == 0 {
                        if_true.clone()
                    } else {
                        if_false.clone()
                    };
                    f.blocks[which].terminator = Terminator::Jump { edge };
                    trim(&mut f);
                    changed = true;
                }
            }
            4 => {
                let options: Vec<_> = vals.iter().filter(|(_, ty)| *ty == s.return_type).collect();
                if matches!(f.blocks[which].terminator, Terminator::Return { .. })
                    && !options.is_empty()
                {
                    f.blocks[which].terminator = Terminator::Return {
                        value: options[rng.index(options.len())].0,
                    };
                    changed = true;
                }
            }
            _ if !f.blocks[which].instructions.is_empty() => {
                let index = rng.index(f.blocks[which].instructions.len());
                let i = &mut f.blocks[which].instructions[index];
                match &mut i.expr {
                    Expr::Call { .. } => {}
                    Expr::Const { value } => {
                        let options: Vec<_> = c
                            .constants
                            .iter()
                            .filter_map(|s| Value::parse(s).ok())
                            .filter(|v| v.ty == i.ty)
                            .collect();
                        if !options.is_empty() {
                            *value = options[rng.index(options.len())];
                            changed = true;
                        }
                    }
                    Expr::Apply { op, args } => {
                        if rng.index(2) == 0 {
                            let ts: Vec<_> = args
                                .iter()
                                .map(|a| vals.iter().find(|(id, _)| id == a).unwrap().1)
                                .collect();
                            let ops: Vec<_> = c
                                .operators
                                .iter()
                                .filter(|o| o.result(&ts) == Ok(i.ty))
                                .collect();
                            if !ops.is_empty() {
                                *op = *ops[rng.index(ops.len())];
                                changed = true;
                            }
                        } else {
                            let index = rng.index(args.len());
                            let ty = vals.iter().find(|(id, _)| *id == args[index]).unwrap().1;
                            let options: Vec<_> = vals.iter().filter(|(_, t)| *t == ty).collect();
                            args[index] = options[rng.index(options.len())].0;
                            changed = true;
                        }
                    }
                }
            }
            _ => {}
        }
        if changed {
            if let Ok(f) = f.normalized() {
                let g = Genome {
                    cfg: Some(f),
                    genes: vec![],
                    output: 0,
                };
                if g.valid(s, c) {
                    return g;
                }
            }
        }
    }
    parent.clone()
}
