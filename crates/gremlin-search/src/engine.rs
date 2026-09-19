use crate::{genome::*, SearchConfig};
use gremlin_core::*;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseResult {
    pub execution: Execution,
    pub bit_error: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fitness {
    pub corpus_hash: String,
    pub noncompleted_case_count: u64,
    pub mismatching_completed_case_count: u64,
    pub summed_bit_error: u64,
    pub instruction_count: usize,
    pub total_executed_steps: u64,
    pub canonical_program_bytes: Vec<u8>,
    pub cases: Vec<CaseResult>,
}
impl Fitness {
    pub fn matches(&self) -> bool {
        self.noncompleted_case_count == 0 && self.mismatching_completed_case_count == 0
    }
    fn rank(&self) -> (u64, u64, u64, usize, u64, &[u8]) {
        (
            self.noncompleted_case_count,
            self.mismatching_completed_case_count,
            self.summed_bit_error,
            self.instruction_count,
            self.total_executed_steps,
            &self.canonical_program_bytes,
        )
    }
}
impl Ord for Fitness {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank().cmp(&other.rank())
    }
}
impl PartialOrd for Fitness {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Individual {
    pub genome: Genome,
    pub fitness: Fitness,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failures {
    pub trap: u64,
    pub timeout: u64,
    pub invalid: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchState {
    pub generation: usize,
    pub rng: Rng,
    pub population: Vec<Individual>,
    pub best: Individual,
    pub evaluation_count: u64,
    pub failures: Failures,
}
pub struct Engine {
    signature: Signature,
    config: SearchConfig,
    corpus_hash: String,
    cases: Vec<(Vec<Value>, Value)>,
}
impl Engine {
    pub fn new(config: SearchConfig, corpus: &Corpus) -> Result<Self, String> {
        corpus.validate()?;
        let signature = corpus.target.signature.clone();
        let cases = corpus
            .cases
            .iter()
            .map(|c| {
                Ok((
                    decode_input(&signature, &c.input)?,
                    Value::from_hex(signature.return_type, &c.expected)?,
                ))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            signature,
            config,
            corpus_hash: corpus.content_hash()?,
            cases,
        })
    }
    pub fn evaluate(&self, g: &Genome) -> Result<Fitness, String> {
        let f = g.lower(&self.signature);
        let bytes = f.canonical_bytes()?;
        let mut evaluator = Evaluator::new(&f)?;
        let mut fitness = Fitness {
            corpus_hash: self.corpus_hash.clone(),
            noncompleted_case_count: 0,
            mismatching_completed_case_count: 0,
            summed_bit_error: 0,
            instruction_count: g.genes.len(),
            total_executed_steps: 0,
            canonical_program_bytes: bytes,
            cases: Vec::with_capacity(self.cases.len()),
        };
        for (input, expected) in &self.cases {
            let e = evaluator.execute(input, self.config.max_steps);
            fitness.total_executed_steps += e.steps;
            let bit_error = if let Outcome::Completed(v) = &e.outcome {
                let error = ((v.bits ^ expected.bits) & expected.ty.mask()).count_ones();
                fitness.mismatching_completed_case_count += u64::from(error != 0);
                fitness.summed_bit_error += u64::from(error);
                Some(error)
            } else {
                fitness.noncompleted_case_count += 1;
                None
            };
            fitness.cases.push(CaseResult {
                execution: e,
                bit_error,
            });
        }
        Ok(fitness)
    }
    fn individual(
        &self,
        g: Genome,
        count: &mut u64,
        failures: &mut Failures,
    ) -> Result<Individual, String> {
        let fitness = self.evaluate(&g)?;
        *count += self.cases.len() as u64;
        for c in &fitness.cases {
            match c.execution.outcome {
                Outcome::Trap(_) => failures.trap += 1,
                Outcome::Timeout(_) => failures.timeout += 1,
                Outcome::Invalid(_) => failures.invalid += 1,
                _ => {}
            }
        }
        Ok(Individual { genome: g, fitness })
    }
    pub fn initialize(&self, seed: u64) -> Result<SearchState, String> {
        let mut rng = Rng::new(seed);
        let seeds = seeds(&self.signature, &self.config);
        let fallback = seeds
            .first()
            .ok_or("search pool cannot construct a valid return")?;
        let mut population = vec![];
        let mut count = 0;
        let mut failures = Failures::default();
        for i in 0..self.config.population {
            let g = if i < seeds.len() {
                seeds[i].clone()
            } else {
                random_genome(&self.signature, &self.config, &mut rng, fallback)
            };
            population.push(self.individual(g, &mut count, &mut failures)?);
        }
        population.sort_by(|a, b| a.fitness.cmp(&b.fitness));
        Ok(SearchState {
            generation: 1,
            rng,
            best: population[0].clone(),
            population,
            evaluation_count: count,
            failures,
        })
    }
    pub fn advance(&self, state: &mut SearchState) -> Result<(), String> {
        let mut next = state.population[..self.config.elite].to_vec();
        while next.len() < self.config.population {
            let mut selected = state.rng.index(state.population.len());
            for _ in 1..self.config.tournament_size {
                let i = state.rng.index(state.population.len());
                if state.population[i].fitness < state.population[selected].fitness {
                    selected = i;
                }
            }
            let child = mutate(
                &state.population[selected].genome,
                &self.signature,
                &self.config,
                &mut state.rng,
            );
            next.push(self.individual(child, &mut state.evaluation_count, &mut state.failures)?);
        }
        next.sort_by(|a, b| a.fitness.cmp(&b.fitness));
        state.population = next;
        state.generation += 1;
        state.best = state.population[0].clone();
        Ok(())
    }
    pub fn validate_state(&self, state: &SearchState) -> Result<(), String> {
        if state.generation == 0
            || state.generation > self.config.generations
            || state.population.len() != self.config.population
        {
            return Err("checkpoint generation or population mismatch".into());
        }
        for i in &state.population {
            if i.genome.genes.len() > self.config.max_instructions
                || self.evaluate(&i.genome)? != i.fitness
            {
                return Err("checkpoint fitness or genome mismatch".into());
            }
        }
        if state
            .population
            .windows(2)
            .any(|p| p[0].fitness > p[1].fitness)
            || state.best != state.population[0]
        {
            return Err("checkpoint population order/best mismatch".into());
        }
        Ok(())
    }
}
