use gremlin_core::*;
use gremlin_search::*;
fn config(name: &str) -> Config {
    Config::parse(&std::fs::read_to_string(format!("../../tests/fixtures/{name}.toml")).unwrap())
        .unwrap()
}
#[test]
fn structural_mutations_preserve_ir_and_budget_failures() {
    let c = config("bounded_sum_u8");
    let s = c.signature();
    let mut g = seeds(&s, &c.search)[0].clone();
    let mut rng = Rng::new(3);
    let mut saw_branch = false;
    let mut saw_loop = false;
    for _ in 0..3000 {
        g = mutate(&g, &s, &c.search, &mut rng);
        assert!(g.valid(&s, &c.search));
        let f = g.lower(&s);
        saw_branch |= f
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Branch { .. }));
        saw_loop |= f
            .blocks
            .iter()
            .any(|b| b.terminator.edges().iter().any(|e| e.block <= b.id));
        assert_eq!(
            parse(&print_source(&f).unwrap()).unwrap(),
            f.normalized().unwrap()
        );
        let e = execute(&f, &[Value::new(Type::U8, 15), Value::new(Type::U8, 3)], 64);
        assert!(e.steps <= 64);
    }
    assert!(saw_branch);
    assert!(saw_loop);
}
#[test]
fn structural_convergence() {
    for name in ["min_u8", "bounded_sum_u8"] {
        for seed in [1, 2, 3] {
            let c = config(name);
            let corpus = fixture_corpus(name, seed, 16).unwrap();
            let engine = Engine::new(c.search.clone(), &corpus).unwrap();
            let mut state = engine.initialize(seed).unwrap();
            while !state.best.fitness.matches() && state.generation < c.search.generations {
                engine.advance(&mut state).unwrap();
            }
            assert!(
                state.best.fitness.matches(),
                "{name} seed {seed} generation {} errors {}",
                state.generation,
                state.best.fitness.summed_bit_error
            );
            let holdout = holdout_inputs(&c.signature(), &corpus, seed ^ 0x76543210, 256);
            let f = state.best.genome.lower(&c.signature());
            assert!(f.blocks.len() > 1);
            let mut evaluator = Evaluator::new(&f).unwrap();
            for args in &holdout.inputs {
                assert_eq!(
                    evaluator.execute(args, c.search.max_steps).outcome,
                    Outcome::Completed(fixture_observe(name, args).unwrap())
                );
            }
            eprintln!(
                "{name} seed {seed} generation {} blocks {}",
                state.generation,
                f.blocks.len()
            );
        }
    }
}
