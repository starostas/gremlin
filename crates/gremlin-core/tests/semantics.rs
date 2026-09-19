use gremlin_core::*;
const TYPES: [Type; 8] = [
    Type::U8,
    Type::U16,
    Type::U32,
    Type::U64,
    Type::I8,
    Type::I16,
    Type::I32,
    Type::I64,
];
fn op(o: Op, t: Type, a: u64, b: u64) -> Value {
    o.eval(&[Value::new(t, a), Value::new(t, b)]).unwrap()
}
#[test]
fn operator_vectors_all_widths() {
    for t in TYPES {
        for (o, a, b, want) in [
            (Op::Add, 6, 3, 9),
            (Op::Sub, 6, 3, 3),
            (Op::Mul, 6, 3, 18),
            (Op::And, 6, 3, 2),
            (Op::Or, 6, 3, 7),
            (Op::Xor, 6, 3, 5),
            (Op::Eq, 6, 3, 0),
            (Op::Ne, 6, 3, 1),
        ] {
            assert_eq!(op(o, t, a, b).bits, want, "{o} {t}");
        }
        assert_eq!(op(Op::Add, t, t.mask(), 1).bits, 0);
        assert_eq!(op(Op::Sub, t, 0, 1).bits, t.mask());
        assert_eq!(op(Op::Mul, t, t.mask(), 2).bits, t.mask() - 1);
        assert_eq!(
            Op::Not.eval(&[Value::new(t, 6)]).unwrap().bits,
            t.mask() ^ 6
        );
        for truth in [false, true] {
            assert_eq!(
                Op::Select
                    .eval(&[
                        Value::new(Type::Bool, u64::from(truth)),
                        Value::new(t, 17),
                        Value::new(t, 23)
                    ])
                    .unwrap()
                    .bits,
                if truth { 17 } else { 23 }
            );
        }
        let orders = if t.signed() {
            [Op::Slt, Op::Sle, Op::Sgt, Op::Sge]
        } else {
            [Op::Ult, Op::Ule, Op::Ugt, Op::Uge]
        };
        for (o, want) in orders.into_iter().zip([1, 1, 0, 0]) {
            assert_eq!(op(o, t, 3, 6).bits, want);
        }
        let (div, rem) = if t.signed() {
            (Op::Sdiv, Op::Srem)
        } else {
            (Op::Udiv, Op::Urem)
        };
        assert_eq!(op(div, t, 7, 3).bits, 2);
        assert_eq!(op(rem, t, 7, 3).bits, 1);
        for o in [div, rem] {
            assert!(o.eval(&[Value::new(t, 1), Value::new(t, 0)]).is_err());
        }
        if t.signed() {
            let min = 1u64 << (t.width() - 1);
            for o in [div, rem] {
                assert!(o
                    .eval(&[Value::new(t, min), Value::new(t, t.mask())])
                    .is_err());
            }
            assert_eq!(op(div, t, (-7i64) as u64, 3).signed(), -2);
            assert_eq!(op(rem, t, (-7i64) as u64, 3).signed(), -1);
            assert_eq!(op(Op::Slt, t, min, t.mask()).bits, 1);
            assert_eq!(op(Op::Add, t, min - 1, 1).bits, min);
        }
        let a = (1u64 << (t.width() - 1)) | 3;
        for count in [0, t.width() - 1, t.width(), t.width() + 1] {
            let k = count % t.width();
            let wide = a as u128;
            let mask = t.mask() as u128;
            let expected = [
                ((wide << k) & mask) as u64,
                (wide >> k) as u64,
                ((Value::new(t, a).signed() >> k) as u64) & t.mask(),
                (((wide << k) | (wide >> (t.width() - k))) & mask) as u64,
                (((wide >> k) | (wide << (t.width() - k))) & mask) as u64,
            ];
            for (o, want) in [Op::Shl, Op::Lshr, Op::Ashr, Op::Rotl, Op::Rotr]
                .into_iter()
                .zip(expected)
            {
                assert_eq!(op(o, t, a, count as u64).bits, want, "{o} {t} {count}");
            }
        }
    }
    for o in [Op::Eq, Op::Ne] {
        assert_eq!(op(o, Type::Bool, 1, 0).bits, u64::from(o == Op::Ne));
    }
}
#[test]
fn syntax_roundtrip_and_rejection() {
    let f = parse(include_str!("../../../examples/affine.gremlin")).unwrap();
    assert_eq!(f, parse(&print_source(&f).unwrap()).unwrap());
    for x in [0, 1, 5, u64::MAX] {
        assert_eq!(
            execute(&f, &[Value::new(Type::U64, x)], 256).outcome,
            Outcome::Completed(Value::new(
                Type::U64,
                (x ^ 0x12345678).wrapping_mul(7).wrapping_add(3)
            ))
        );
    }
    for literal in [
        "256u8",
        "-1u8",
        "128i8",
        "-129i8",
        "0x100u8",
        "18446744073709551616u64",
        "1",
        "-0x1i8",
        "+1u8",
        "0x+1u8",
        "-0u8",
    ] {
        assert!(Value::parse(literal).is_err(), "{literal}");
    }
    assert_eq!(Value::parse("0xffi8").unwrap().signed(), -1);
    assert_eq!(Value::parse("-128i8").unwrap().signed(), -128);
    for s in [
        "fn f(x: u8) -> u8 { return add(x, 1u16); }",
        "fn f(x: u8) -> u8 { return missing; }",
        "fn f(x: u8) -> u8 { return sdiv(x, 1u8); }",
        "fn f(x: u8) -> u8 { let x: u8 = 1u8; return x; }",
        "fn f() -> bool { return true; }",
        "fn f(x:u8,y:u8,z:u8,a:u8,b:u8)->u8{return x;}",
    ] {
        assert!(parse(s).is_err(), "{s}");
    }
    let f = parse("// eager\nfn f() -> u8 { return select(true, 1u8, udiv(1u8, 0u8)); }").unwrap();
    assert!(matches!(execute(&f, &[], 20).outcome, Outcome::Trap(_)));
}
fn p(id: u32, ty: Type) -> Param {
    Param { id, ty }
}
fn c(id: u32, n: u64) -> Instruction {
    Instruction {
        id,
        ty: Type::U8,
        expr: Expr::Const {
            value: Value::new(Type::U8, n),
        },
    }
}
fn app(id: u32, ty: Type, op: Op, args: Vec<u32>) -> Instruction {
    Instruction {
        id,
        ty,
        expr: Expr::Apply { op, args },
    }
}
fn edge(block: u32, args: Vec<u32>) -> Edge {
    Edge { block, args }
}
fn loop_ir() -> Function {
    Function {
        callees: std::collections::BTreeMap::new(),
        schema_version: 1,
        parameters: vec![p(0, Type::U8)],
        return_type: Type::U8,
        entry: 0,
        blocks: vec![
            Block {
                id: 0,
                parameters: vec![],
                instructions: vec![c(1, 0), c(2, 1)],
                terminator: Terminator::Jump {
                    edge: edge(1, vec![0]),
                },
            },
            Block {
                id: 1,
                parameters: vec![p(3, Type::U8)],
                instructions: vec![app(4, Type::Bool, Op::Eq, vec![3, 1])],
                terminator: Terminator::Branch {
                    condition: 4,
                    if_true: edge(3, vec![3]),
                    if_false: edge(2, vec![3]),
                },
            },
            Block {
                id: 2,
                parameters: vec![p(5, Type::U8)],
                instructions: vec![app(6, Type::U8, Op::Sub, vec![5, 2])],
                terminator: Terminator::Jump {
                    edge: edge(1, vec![6]),
                },
            },
            Block {
                id: 3,
                parameters: vec![p(7, Type::U8)],
                instructions: vec![],
                terminator: Terminator::Return { value: 7 },
            },
        ],
    }
}
#[test]
fn cfg_loop_and_exact_budgets() {
    let f = loop_ir();
    f.validate().unwrap();
    let args = [Value::new(Type::U8, 3)];
    assert_eq!(
        execute(&f, &args, 18),
        Execution {
            outcome: Outcome::Completed(Value::new(Type::U8, 0)),
            steps: 18
        }
    );
    for budget in [0, 1, 17] {
        let e = execute(&f, &args, budget);
        assert!(matches!(e.outcome, Outcome::Timeout(_)));
        assert_eq!(e.steps, budget);
    }
    let mut f = f;
    f.blocks[2].terminator = Terminator::Jump {
        edge: edge(1, vec![5]),
    };
    let e = execute(&f, &args, 99);
    assert!(matches!(e.outcome, Outcome::Timeout(_)));
    assert_eq!(e.steps, 99);
}
#[test]
fn simultaneous_binding() {
    let f = Function {
        callees: std::collections::BTreeMap::new(),
        schema_version: 1,
        parameters: vec![],
        return_type: Type::U8,
        entry: 0,
        blocks: vec![
            Block {
                id: 0,
                parameters: vec![],
                instructions: vec![c(0, 3), c(1, 9), c(2, 1)],
                terminator: Terminator::Jump {
                    edge: edge(1, vec![0, 1, 2]),
                },
            },
            Block {
                id: 1,
                parameters: vec![p(3, Type::U8), p(4, Type::U8), p(5, Type::U8)],
                instructions: vec![c(6, 0), app(7, Type::Bool, Op::Eq, vec![5, 6])],
                terminator: Terminator::Branch {
                    condition: 7,
                    if_true: edge(2, vec![3]),
                    if_false: edge(1, vec![4, 3, 6]),
                },
            },
            Block {
                id: 2,
                parameters: vec![p(8, Type::U8)],
                instructions: vec![],
                terminator: Terminator::Return { value: 8 },
            },
        ],
    };
    assert_eq!(
        execute(&f, &[], 20).outcome,
        Outcome::Completed(Value::new(Type::U8, 9))
    );
}
#[test]
fn validator_rejects_malformed_ir() {
    let good = loop_ir();
    let mut variants = Vec::new();
    let mut f = good.clone();
    f.entry = 9;
    variants.push(f);
    let mut f = good.clone();
    f.blocks.push(f.blocks[0].clone());
    variants.push(f);
    let mut f = good.clone();
    f.blocks[0].instructions[0].id = 0;
    variants.push(f);
    let mut f = good.clone();
    f.blocks[0].instructions[0] = app(1, Type::U8, Op::Add, vec![2, 2]);
    variants.push(f);
    let mut f = good.clone();
    f.blocks[0].terminator = Terminator::Jump {
        edge: edge(1, vec![999]),
    };
    variants.push(f);
    let mut f = good.clone();
    f.blocks[1].instructions[0] = app(4, Type::Bool, Op::Eq, vec![6, 1]);
    variants.push(f);
    let mut f = good.clone();
    f.blocks[2].terminator = Terminator::Jump {
        edge: edge(1, vec![]),
    };
    variants.push(f);
    let mut f = good.clone();
    if let Terminator::Branch { condition, .. } = &mut f.blocks[1].terminator {
        *condition = 3;
    }
    variants.push(f);
    let mut f = good.clone();
    f.blocks[3].terminator = Terminator::Return { value: 4 };
    variants.push(f);
    let mut f = good.clone();
    f.blocks[0].terminator = Terminator::Return { value: 0 };
    variants.push(f);
    let mut f = good.clone();
    f.blocks[0].instructions[0] = app(1, Type::U8, Op::Add, vec![0]);
    variants.push(f);
    let mut f = good.clone();
    f.blocks[0].terminator = Terminator::Jump {
        edge: edge(8, vec![]),
    };
    variants.push(f);
    for f in variants {
        assert!(f.validate().is_err(), "{f:?}");
        assert!(matches!(
            execute(&f, &[Value::new(Type::U8, 1)], 100).outcome,
            Outcome::Invalid(_)
        ));
    }
}
#[test]
fn generated_programs_no_panics() {
    let mut state = 0x123456789u64;
    for t in TYPES {
        for _ in 0..128 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let mut s = format!("fn f(x:{t})->{t}{{");
            let mut name = "x".to_string();
            for i in 0..8 {
                let o = ["add", "sub", "mul", "xor", "shl", "lshr", "ashr", "rotl"]
                    [(state.rotate_left(i) as usize) % 8];
                s.push_str(&format!(
                    "let a{i}:{t}={o}({name},{});",
                    Value::new(t, state.rotate_right(i)).literal()
                ));
                name = format!("a{i}");
            }
            s.push_str(&format!("return {name};}}"));
            let f = parse(&s).unwrap();
            assert_eq!(f, parse(&print_source(&f).unwrap()).unwrap());
            let e = execute(&f, &[Value::new(t, state)], 100);
            assert!(matches!(e.outcome, Outcome::Completed(_)));
        }
    }
}

#[test]
fn normalized_ids_and_boolean_selection() {
    assert!(Value::from_hex(Type::U8, "0x+1").is_err());
    let mut f = parse("fn f(x:u8)->u8 {return add(x,1u8);}").unwrap();
    let expected = f.canonical_bytes().unwrap();
    f.parameters[0].id = 100;
    f.blocks[0].id = 72;
    f.entry = 72;
    for i in &mut f.blocks[0].instructions {
        i.id += 100;
        if let Expr::Apply { args, .. } = &mut i.expr {
            for a in args {
                *a += 100;
            }
        }
    }
    if let Terminator::Return { value } = &mut f.blocks[0].terminator {
        *value += 100;
    }
    assert_eq!(f.canonical_bytes().unwrap(), expected);
    assert_eq!(
        Op::Select
            .eval(&[
                Value::new(Type::Bool, 0),
                Value::new(Type::Bool, 1),
                Value::new(Type::Bool, 0)
            ])
            .unwrap(),
        Value::new(Type::Bool, 0)
    );
    assert!(serde_json::from_str::<Value>(r#"{"ty":"u8","bits":255}"#).is_err());
    assert_eq!(
        serde_json::to_string(&Value::new(Type::U64, u64::MAX)).unwrap(),
        r#"{"ty":"u64","bits":"0xffffffffffffffff"}"#
    );
}
