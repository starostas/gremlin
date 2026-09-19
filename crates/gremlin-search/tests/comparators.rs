use gremlin_core::*;
use gremlin_search::*;

fn config() -> SearchConfig {
    Config::parse(include_str!("../../../tests/fixtures/composed_u64.toml"))
        .unwrap()
        .search
}
fn corpus() -> Corpus {
    let mut corpus = Corpus::new(TargetIdentity {
        name: "comparator-test".into(),
        signature: Signature {
            arguments: vec![Type::U64],
            return_type: Type::U64,
        },
        contract: "test observations".into(),
        implementation_fingerprint: "test-v1".into(),
    });
    for (input, expected) in [(0, 0), (1, 255)] {
        corpus
            .add(
                vec![format!("0x{input:016x}")],
                format!("0x{expected:016x}"),
                "test".into(),
            )
            .unwrap();
    }
    corpus
}
fn genome(source: &str) -> Genome {
    let f = parse(source).unwrap();
    let Terminator::Return { value } = f.blocks[0].terminator else {
        panic!()
    };
    Genome {
        cfg: None,
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
fn custom(source: &str) -> ComparatorConfig {
    ComparatorConfig::Gremlin {
        source: source.into(),
        max_steps: 256,
    }
}
#[test]
fn bit_error_first_changes_ranking_without_changing_correctness() {
    let exact_one = genome("fn f(x:u64)->u64{return 0u64;}");
    let close_both = genome("fn f(x:u64)->u64{return xor(mul(x,255u64),1u64);}");
    let c = config();
    let engine = Engine::new(c.clone(), &corpus()).unwrap();
    assert!(engine.evaluate(&exact_one).unwrap() < engine.evaluate(&close_both).unwrap());
    let mut c = c;
    c.comparator = ComparatorConfig::BitErrorFirst {};
    let engine = Engine::new(c, &corpus()).unwrap();
    let a = engine.evaluate(&exact_one).unwrap();
    let b = engine.evaluate(&close_both).unwrap();
    assert_eq!(
        (a.mismatching_completed_case_count, a.summed_bit_error),
        (1, 8)
    );
    assert_eq!(
        (b.mismatching_completed_case_count, b.summed_bit_error),
        (2, 2)
    );
    assert!(b < a);
    assert!(!b.matches());
}
#[test]
fn custom_scores_cannot_fake_success_or_reward_execution_failures() {
    let mut c = config();
    c.comparator = custom("fn score(actual:u64,expected:u64)->u64 { if eq(actual,expected) { return 0xffffffffffffffffu64; } else { return 0u64; } }");
    let engine = Engine::new(c, &corpus()).unwrap();
    let good = engine
        .evaluate(&genome("fn f(x:u64)->u64{return mul(x,255u64);}"))
        .unwrap();
    let bad = engine
        .evaluate(&genome("fn f(x:u64)->u64{return xor(mul(x,255u64),1u64);}"))
        .unwrap();
    let trap = engine
        .evaluate(&genome("fn f(x:u64)->u64{return udiv(x,0u64);}"))
        .unwrap();
    assert_eq!(good.selection_cost, Some([1, u64::MAX - 1]));
    assert_eq!(bad.selection_cost, Some([0, 0]));
    assert!(good < bad && bad < trap);
    assert!(good.matches());
    assert!(!bad.matches() && !trap.matches());
    assert_eq!(
        serde_json::from_slice::<Fitness>(&serde_json::to_vec(&good).unwrap()).unwrap(),
        good
    );
}
#[test]
fn comparator_errors_abort_instead_of_becoming_fitness() {
    for source in [
        "fn score(actual:u64,expected:u64)->u64{return udiv(actual,0u64);}",
        "fn score(actual:u64,expected:u64)->u64{loop{}}",
    ] {
        let mut c = config();
        c.comparator = custom(source);
        let engine = Engine::new(c, &corpus()).unwrap();
        assert!(engine
            .evaluate(&genome("fn f(x:u64)->u64{return x;}"))
            .unwrap_err()
            .contains("comparator execution failed"));
    }
    for comparator in [
        custom("fn score(x:u8,y:u8)->u8{return x;}"),
        custom("fn score(x:u64,y:u64)->u64{return missing(x);}"),
        ComparatorConfig::Gremlin {
            source: "fn score(x:u64,y:u64)->u64{return x;}".into(),
            max_steps: 0,
        },
    ] {
        assert!(comparator.program().is_err());
    }
    let source = include_str!("../../../tests/fixtures/composed_u64.toml");
    assert!(Config::parse(&format!("{source}\n[search.comparator]\nkind='unknown'\n")).is_err());
    assert!(Config::parse(&format!(
        "{source}\n[search.comparator]\nkind='bit_error_first'\nmax_steps=3\n"
    ))
    .is_err());
}
#[test]
fn custom_scoring_survives_resume_and_corpus_regrading() {
    let mut c = config();
    c.population = 16;
    c.elite = 2;
    c.comparator = custom("fn score(actual:u64,expected:u64)->u64{return xor(actual,expected);}");
    let engine = Engine::new(c.clone(), &corpus()).unwrap();
    let mut state = engine.initialize(7).unwrap();
    for _ in 0..3 {
        engine.advance(&mut state).unwrap();
    }
    let mut resumed: SearchState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    engine.validate_state(&resumed).unwrap();
    for _ in 0..8 {
        engine.advance(&mut state).unwrap();
        engine.advance(&mut resumed).unwrap();
        assert_eq!(state, resumed);
    }
    resumed.population[0].fitness.selection_cost = Some([9, 9]);
    assert!(engine.validate_state(&resumed).is_err());
    let mut changed = corpus();
    changed
        .add(
            vec!["0x0000000000000002".into()],
            "0x00000000000001fe".into(),
            "new counterexample".into(),
        )
        .unwrap();
    let regraded = Engine::new(c.clone(), &changed).unwrap();
    regraded.regrade(&mut state).unwrap();
    regraded.validate_state(&state).unwrap();
    c.comparator = custom("fn score(actual:u64,expected:u64)->u64{return 7u64;}");
    assert!(Engine::new(c, &changed)
        .unwrap()
        .validate_state(&state)
        .is_err());
}
#[test]
fn narrow_signed_values_are_zero_extended_bit_patterns() {
    let mut cases = corpus();
    cases.target.signature = Signature {
        arguments: vec![Type::I8],
        return_type: Type::I8,
    };
    cases.cases.clear();
    cases.provenance.clear();
    cases
        .add(vec!["0xff".into()], "0xff".into(), "signed -1".into())
        .unwrap();
    let mut c = config();
    c.comparator = custom("fn score(actual:u64,expected:u64)->u64{return actual;}");
    let fit = Engine::new(c, &cases)
        .unwrap()
        .evaluate(&genome("fn f(x:i8)->i8{return x;}"))
        .unwrap();
    assert_eq!(fit.selection_cost, Some([0, 255]));
    assert!(fit.matches());
}

#[test]
fn custom_hamming_matches_builtin_selection() {
    let mut c = config();
    c.population = 16;
    c.elite = 2;
    c.comparator = ComparatorConfig::BitErrorFirst {};
    let corpus = fixture_corpus("composed_u64", 1, 8).unwrap();
    let builtin = Engine::new(c.clone(), &corpus).unwrap();
    c.comparator = custom(include_str!(
        "../../../examples/comparators/hamming.gremlin"
    ));
    let custom = Engine::new(c, &corpus).unwrap();
    let mut a = builtin.initialize(4).unwrap();
    let mut b = custom.initialize(4).unwrap();
    for _ in 0..10 {
        for (x, y) in a.population.iter().zip(&b.population) {
            assert_eq!(x.genome, y.genome);
            assert_eq!(x.fitness.selection_cost, y.fitness.selection_cost);
            for case in &y.fitness.cases {
                assert_eq!(case.custom_cost, case.bit_error.map(u64::from));
            }
        }
        builtin.advance(&mut a).unwrap();
        custom.advance(&mut b).unwrap();
    }
}
