use crate::{
    genome::*, score_executions, summarize_batch, ComparatorConfig, EvaluationSummary, SearchConfig,
};
use gremlin_core::*;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseResult {
    pub execution: Execution,
    pub bit_error: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_cost: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fitness {
    /// Unsigned 128-bit sum as [high, low] words; lower is preferred.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_cost: Option<[u64; 2]>,
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
        self.noncompleted_case_count
            .cmp(&other.noncompleted_case_count)
            // A custom score cannot outrank an exact solution or reward failure.
            .then_with(|| other.matches().cmp(&self.matches()))
            .then_with(|| self.selection_cost.cmp(&other.selection_cost))
            .then_with(|| self.rank().cmp(&other.rank()))
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
    pub enumeration_cursor: u64,
    pub population: Vec<Individual>,
    pub best: Individual,
    pub evaluation_count: u64,
    pub failures: Failures,
}
pub trait Backend {
    fn evaluate(
        &self,
        functions: &[Function],
        inputs: &[Vec<Value>],
        max_steps: u64,
    ) -> Result<Vec<Vec<Execution>>, String>;
    fn summarize(
        &self,
        functions: &[Function],
        inputs: &[Vec<Value>],
        expected: &[Value],
        max_steps: u64,
        comparator: &ComparatorConfig,
    ) -> Result<Vec<EvaluationSummary>, String> {
        summarize_batch(
            self.evaluate(functions, inputs, max_steps)?,
            expected,
            max_steps,
            comparator,
        )
    }
}
pub struct CpuBackend;
impl Backend for CpuBackend {
    fn evaluate(
        &self,
        functions: &[Function],
        inputs: &[Vec<Value>],
        max_steps: u64,
    ) -> Result<Vec<Vec<Execution>>, String> {
        functions
            .iter()
            .map(|f| {
                let mut evaluator = Evaluator::new(f)?;
                Ok(inputs
                    .iter()
                    .map(|input| evaluator.execute(input, max_steps))
                    .collect())
            })
            .collect()
    }
}
pub struct Engine {
    backend: Box<dyn Backend>,
    signature: Signature,
    config: SearchConfig,
    corpus_hash: String,
    cases: Vec<(Vec<Value>, Value)>,
}
impl Engine {
    pub fn new(config: SearchConfig, corpus: &Corpus) -> Result<Self, String> {
        corpus.validate()?;
        config.comparator.program()?;
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
            backend: Box::new(CpuBackend),
            signature,
            config,
            corpus_hash: corpus.content_hash()?,
            cases,
        })
    }
    pub fn with_backend(mut self, backend: Box<dyn Backend>) -> Self {
        self.backend = backend;
        self
    }
    pub fn evaluate(&self, g: &Genome) -> Result<Fitness, String> {
        if !g.valid(&self.signature, &self.config) {
            return Err("genome violates signature or structural limits".into());
        }
        let f = g.lower(&self.signature);
        let bytes = f.canonical_bytes()?;
        let mut evaluator = Evaluator::new(&f)?;
        let executions = self
            .cases
            .iter()
            .map(|(input, _)| evaluator.execute(input, self.config.max_steps))
            .collect();
        self.grade(g, bytes, executions)
    }
    fn grade(
        &self,
        g: &Genome,
        bytes: Vec<u8>,
        executions: Vec<Execution>,
    ) -> Result<Fitness, String> {
        let expected = self.cases.iter().map(|(_, v)| *v).collect::<Vec<_>>();
        let (summary, cases) = score_executions(
            executions,
            &expected,
            self.config.max_steps,
            &self.config.comparator,
            true,
        )?;
        let mut fitness = self.fitness(g, bytes, summary);
        fitness.cases = cases;
        Ok(fitness)
    }
    fn fitness(&self, g: &Genome, bytes: Vec<u8>, summary: EvaluationSummary) -> Fitness {
        Fitness {
            selection_cost: summary.selection_cost,
            corpus_hash: self.corpus_hash.clone(),
            noncompleted_case_count: summary.noncompleted,
            mismatching_completed_case_count: summary.mismatches,
            summed_bit_error: summary.bit_error,
            instruction_count: g.instruction_count(),
            total_executed_steps: summary.steps,
            canonical_program_bytes: bytes,
            cases: Vec::new(),
        }
    }
    fn individuals(
        &self,
        genomes: Vec<Genome>,
        count: &mut u64,
        failures: &mut Failures,
    ) -> Result<Vec<Individual>, String> {
        if genomes.is_empty() {
            return Ok(vec![]);
        }
        let functions = genomes
            .iter()
            .map(|g| {
                if !g.valid(&self.signature, &self.config) {
                    return Err("genome violates signature or structural limits".into());
                }
                Ok(g.lower(&self.signature))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let inputs = self
            .cases
            .iter()
            .map(|(input, _)| input.clone())
            .collect::<Vec<_>>();
        let expected = self.cases.iter().map(|(_, v)| *v).collect::<Vec<_>>();
        let results = self.backend.summarize(
            &functions,
            &inputs,
            &expected,
            self.config.max_steps,
            &self.config.comparator,
        )?;
        if results.len() != genomes.len() {
            return Err("backend returned wrong program count".into());
        }
        let mut individuals = Vec::with_capacity(genomes.len());
        for ((g, f), summary) in genomes.into_iter().zip(functions).zip(results) {
            summary.validate(
                self.cases.len(),
                self.signature.return_type,
                self.config.max_steps,
                &self.config.comparator,
            )?;
            *count += self.cases.len() as u64;
            failures.trap += summary.failures.trap;
            failures.timeout += summary.failures.timeout;
            failures.invalid += summary.failures.invalid;
            let fitness = self.fitness(&g, f.canonical_bytes()?, summary);
            individuals.push(Individual { genome: g, fitness });
        }
        Ok(individuals)
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
            population.push(g);
        }
        let mut population = self.individuals(population, &mut count, &mut failures)?;
        population.sort_by(|a, b| a.fitness.cmp(&b.fitness));
        Ok(SearchState {
            generation: 1,
            rng,
            enumeration_cursor: 0,
            best: population[0].clone(),
            population,
            evaluation_count: count,
            failures,
        })
    }
    pub fn advance(&self, state: &mut SearchState) -> Result<(), String> {
        let mut next = state.population[..self.config.elite].to_vec();
        let mut genomes = Vec::new();
        for _ in 0..self.config.enumeration_proposals {
            let proposal = chain_proposal(&self.signature, &self.config, state.enumeration_cursor);
            state.enumeration_cursor = state
                .enumeration_cursor
                .checked_add(1)
                .ok_or("enumeration cursor exhausted")?;
            if let Some(genome) = proposal {
                genomes.push(genome);
            }
        }
        while next.len() + genomes.len() < self.config.population {
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
            genomes.push(child);
        }
        next.extend(self.individuals(genomes, &mut state.evaluation_count, &mut state.failures)?);
        next.sort_by(|a, b| a.fitness.cmp(&b.fitness));
        state.population = next;
        state.generation += 1;
        state.best = state.population[0].clone();
        Ok(())
    }
    pub fn regrade(&self, state: &mut SearchState) -> Result<(), String> {
        state.population = self.individuals(
            state.population.iter().map(|i| i.genome.clone()).collect(),
            &mut state.evaluation_count,
            &mut state.failures,
        )?;
        state.population.sort_by(|a, b| a.fitness.cmp(&b.fitness));
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
            let mut recomputed = self.evaluate(&i.genome)?;
            if i.fitness.cases.is_empty() {
                recomputed.cases.clear();
            }
            if !i.genome.valid(&self.signature, &self.config) || recomputed != i.fitness {
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
