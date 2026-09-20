use gremlin_core::*;
use gremlin_search::*;
fn rows() -> Vec<Execution> {
    vec![
        Execution {
            outcome: Outcome::Completed(Value::new(Type::U64, 5)),
            steps: 1,
        },
        Execution {
            outcome: Outcome::Completed(Value::new(Type::U64, 0)),
            steps: 2,
        },
        Execution {
            outcome: Outcome::Trap("division by zero".into()),
            steps: 3,
        },
        Execution {
            outcome: Outcome::Timeout("step budget exhausted".into()),
            steps: 4,
        },
        Execution {
            outcome: Outcome::Invalid("invalid program".into()),
            steps: 5,
        },
    ]
}
#[test]
fn compact_scoring_preserves_failures_and_128_bit_custom_costs() {
    let expected = [4, 3, 0, 0, 0].map(|v| Value::new(Type::U64, v));
    for comparator in [
        ComparatorConfig::default(),
        ComparatorConfig::BitErrorFirst {},
        ComparatorConfig::Gremlin {
            source: "fn score(actual:u64,expected:u64)->u64{return 0xffffffffffffffffu64;}".into(),
            max_steps: 8,
        },
    ] {
        let (summary, details) = score_executions(rows(), &expected, 5, &comparator, true).unwrap();
        let (compact, empty) = score_executions(rows(), &expected, 5, &comparator, false).unwrap();
        assert_eq!(summary, compact);
        assert!(empty.is_empty());
        assert_eq!(details.len(), 5);
        assert_eq!(
            (
                summary.noncompleted,
                summary.mismatches,
                summary.bit_error,
                summary.steps
            ),
            (3, 2, 3, 15)
        );
        assert_eq!(
            (
                summary.failures.trap,
                summary.failures.timeout,
                summary.failures.invalid
            ),
            (1, 1, 1)
        );
        if matches!(comparator, ComparatorConfig::Gremlin { .. }) {
            assert_eq!(summary.selection_cost, Some([1, u64::MAX - 1]));
        }
        summary.validate(5, Type::U64, 5, &comparator).unwrap();
        assert_eq!(
            EvaluationSummary::from_words(summary.words()).unwrap(),
            summary
        );
        let mut bad = summary.clone();
        bad.mismatches = 3;
        assert!(bad.validate(5, Type::U64, 5, &comparator).is_err());
        let mut bad = summary.clone();
        bad.steps = 26;
        assert!(bad.validate(5, Type::U64, 5, &comparator).is_err());
        let mut bad = summary.clone();
        bad.failures.invalid = 0;
        assert!(bad.validate(5, Type::U64, 5, &comparator).is_err());
    }
    let failing = ComparatorConfig::Gremlin {
        source: "fn score(actual:u64,expected:u64)->u64{return udiv(actual,0u64);}".into(),
        max_steps: 8,
    };
    assert!(score_executions(rows(), &expected, 5, &failing, false)
        .unwrap_err()
        .contains("comparator execution failed"));
    let mut bad = [0; 10];
    bad[7] = 2;
    assert!(EvaluationSummary::from_words(bad).is_err());
}
#[test]
fn compact_checkpoints_replay_and_reject_forged_totals() {
    let config = Config::parse(include_str!("../../../tests/fixtures/increment_u64.toml")).unwrap();
    let corpus = fixture_corpus("increment_u64", 1, 4).unwrap();
    let engine = Engine::new(config.search, &corpus).unwrap();
    let state = engine.initialize(1).unwrap();
    assert!(state.population.iter().all(|i| i.fitness.cases.is_empty()));
    engine.validate_state(&state).unwrap();
    let mut legacy = state.clone();
    for i in &mut legacy.population {
        i.fitness = engine.evaluate(&i.genome).unwrap();
    }
    legacy.best = legacy.population[0].clone();
    engine.validate_state(&legacy).unwrap();
    let mut forged = state.clone();
    forged.population[0].fitness.summed_bit_error += 1;
    assert!(engine.validate_state(&forged).is_err());
    let mut a = state.clone();
    let mut b: SearchState = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    engine.advance(&mut a).unwrap();
    engine.advance(&mut b).unwrap();
    assert_eq!(a, b);
}
