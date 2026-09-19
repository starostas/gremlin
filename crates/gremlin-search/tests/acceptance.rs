use gremlin_core::*;
use gremlin_search::*;
fn config(name: &str) -> Config {
    Config::parse(&std::fs::read_to_string(format!("../../tests/fixtures/{name}.toml")).unwrap())
        .unwrap()
}
#[test]
fn mandatory_synthesis_all_seeds() {
    for name in [
        "identity_u64",
        "increment_u64",
        "xor_u64",
        "add_u64",
        "composed_u64",
    ] {
        for seed in [1, 2, 3] {
            let mut c = config(name);
            c.seed = seed;
            let corpus = fixture_corpus(name, seed, c.corpus.random_cases).unwrap();
            let engine = Engine::new(c.search.clone(), &corpus).unwrap();
            let mut state = engine.initialize(seed).unwrap();
            while !state.best.fitness.matches() && state.generation < c.search.generations {
                engine.advance(&mut state).unwrap();
            }
            assert!(
                state.best.fitness.matches(),
                "{name} seed {seed} exhausted at {}: {:?}",
                state.generation,
                state.best.fitness
            );
            let holdout = holdout_inputs(
                &c.signature(),
                &corpus,
                seed ^ 0xd1b54a32d192ed03,
                c.corpus.holdout_cases,
            );
            let mut eval = Evaluator::new(&state.best.genome.lower(&c.signature())).unwrap();
            for args in &holdout.inputs {
                assert_eq!(
                    eval.execute(args, c.search.max_steps).outcome,
                    Outcome::Completed(fixture_observe(name, args).unwrap()),
                    "{name} seed {seed}"
                );
            }
            eprintln!(
                "{name}, seed {seed}: generation {}, {} corpus cases, {} holdout cases",
                state.generation,
                corpus.cases.len(),
                holdout.inputs.len()
            );
        }
    }
}
#[test]
fn deterministic_checkpoint_resume() {
    let mut c = config("composed_u64");
    c.search.population = 32;
    c.search.elite = 2;
    let corpus = fixture_corpus(&c.target.name, 1, 4).unwrap();
    let engine = Engine::new(c.search.clone(), &corpus).unwrap();
    let mut uninterrupted = engine.initialize(2).unwrap();
    for _ in 0..3 {
        engine.advance(&mut uninterrupted).unwrap();
    }
    let serialized = serde_json::to_vec(&uninterrupted).unwrap();
    let mut resumed: SearchState = serde_json::from_slice(&serialized).unwrap();
    engine.validate_state(&resumed).unwrap();
    for _ in 0..8 {
        engine.advance(&mut uninterrupted).unwrap();
        engine.advance(&mut resumed).unwrap();
        assert_eq!(uninterrupted, resumed);
    }
}
fn genome(source: &str) -> Genome {
    let f = parse(source).unwrap();
    let Terminator::Return { value } = f.blocks[0].terminator else {
        unreachable!()
    };
    Genome {
        genes: f.blocks[0]
            .instructions
            .iter()
            .map(|i| Gene {
                ty: i.ty,
                expr: i.expr.clone(),
            })
            .collect(),
        output: value,
    }
}
#[test]
fn correctness_beats_size_and_failures_never_match() {
    let c = config("increment_u64");
    let corpus = fixture_corpus(&c.target.name, 1, 2).unwrap();
    let engine = Engine::new(c.search.clone(), &corpus).unwrap();
    let bad = genome("fn f(x:u64)->u64{return x;}");
    let good = genome("fn f(x:u64)->u64{return add(x,1u64);}");
    assert!(engine.evaluate(&good).unwrap() < engine.evaluate(&bad).unwrap());
    let trap = genome("fn f(x:u64)->u64{return udiv(x,0u64);}");
    let fit = engine.evaluate(&trap).unwrap();
    assert!(!fit.matches());
    assert_eq!(fit.noncompleted_case_count, corpus.cases.len() as u64);
    assert!(fit.cases.iter().all(|r| r.bit_error.is_none()));
    let mut limit = c.search;
    limit.max_steps = 1;
    assert!(!Engine::new(limit, &corpus)
        .unwrap()
        .evaluate(&good)
        .unwrap()
        .matches());
}
#[test]
fn deterministic_deletion_repair() {
    let s = Signature {
        arguments: vec![Type::U64],
        return_type: Type::U64,
    };
    let g = genome("fn f(x:u64)->u64{let a:u64=1u64;let b:u64=add(x,a);return b;}");
    let d = delete(&g, &s, 0).unwrap();
    assert_eq!(
        d.genes[0].expr,
        Expr::Apply {
            op: Op::Add,
            args: vec![0, 0]
        }
    );
    d.lower(&s).validate().unwrap();
    let g = genome("fn f()->u64{return 1u64;}");
    assert!(delete(
        &g,
        &Signature {
            arguments: vec![],
            return_type: Type::U64
        },
        0
    )
    .is_none());
}
#[test]
fn mutation_preserves_types() {
    let c = config("composed_u64");
    let mut rng = Rng::new(7);
    let s = c.signature();
    let mut g = seeds(&s, &c.search)[0].clone();
    for _ in 0..2000 {
        g = mutate(&g, &s, &c.search, &mut rng);
        g.lower(&s).validate().unwrap();
        assert!(g.genes.len() <= c.search.max_instructions);
    }
}

#[test]
fn mixed_type_mutations_remain_valid() {
    let mut config = config("composed_u64").search;
    config.operators = Op::ALL.to_vec();
    config.constants = vec![
        "0i8".into(),
        "1i8".into(),
        "3u16".into(),
        "true".into(),
        "false".into(),
    ];
    let signature = Signature {
        arguments: vec![Type::I8, Type::U16],
        return_type: Type::I8,
    };
    let mut g = Genome {
        genes: vec![],
        output: 0,
    };
    let mut rng = Rng::new(19);
    for _ in 0..1000 {
        g = mutate(&g, &signature, &config, &mut rng);
        g.lower(&signature).validate().unwrap();
    }
}
