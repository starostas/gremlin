use crate::{input, oracle::Oracle, Error};
use gremlin_core::*;
use gremlin_search::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};
fn infra(e: impl ToString) -> Error {
    (4, e.to_string())
}
fn bytes<T: Serialize>(v: &T) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(v).map_err(infra)
}
fn read<T: for<'a> Deserialize<'a>>(p: &Path) -> Result<T, Error> {
    serde_json::from_slice(&fs::read(p).map_err(input)?).map_err(input)
}
fn atomic(p: &Path, data: &[u8]) -> Result<(), Error> {
    let temp = p.with_extension("tmp");
    let mut f = File::create(&temp).map_err(infra)?;
    f.write_all(data).map_err(infra)?;
    f.sync_all().map_err(infra)?;
    fs::rename(temp, p).map_err(infra)?;
    File::open(p.parent().unwrap())
        .and_then(|d| d.sync_all())
        .map_err(infra)
}
fn write_json<T: Serialize>(dir: &Path, name: &str, v: &T) -> Result<(), Error> {
    atomic(&dir.join(name), &bytes(v)?)
}
fn file_hash(p: &Path) -> Result<String, Error> {
    Ok(hash(&fs::read(p).map_err(infra)?))
}
fn build_identity() -> Result<String, Error> {
    file_hash(&std::env::current_exe().map_err(infra)?)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema_version: u32,
    semantics_version: String,
    prng_version: String,
    build_identity: String,
    config_hash: String,
    target_hash: String,
    corpus_hash: String,
    file_hashes: BTreeMap<String, String>,
    config: Config,
    state: SearchState,
    runtime_seconds: f64,
    corpus: Corpus,
    provenance: Vec<Provenance>,
    refinement_rounds: usize,
    counterexamples: Vec<Json>,
    initial_candidate_hash: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload_hash: String,
    checkpoint: Checkpoint,
}
fn snapshot_names(cp: &Checkpoint) -> [String; 2] {
    [
        format!("corpus.{}.json", cp.corpus_hash),
        format!("provenance.{}.json", object_hash(&cp.provenance)),
    ]
}
fn persist(dir: &Path, cp: &mut Checkpoint) -> Result<(), Error> {
    cp.corpus_hash = cp.corpus.content_hash().map_err(infra)?;
    cp.provenance = cp.corpus.provenance.clone();
    let [corpus_name, provenance_name] = snapshot_names(cp);
    write_json(dir, &corpus_name, &cp.corpus)?;
    write_json(dir, &provenance_name, &cp.provenance)?;
    cp.file_hashes = ["config.json".into(), corpus_name, provenance_name]
        .into_iter()
        .map(|name| Ok((name.clone(), file_hash(&dir.join(name))?)))
        .collect::<Result<_, Error>>()?;
    write_json(
        dir,
        "checkpoint.json",
        &Envelope {
            payload_hash: object_hash(cp),
            checkpoint: cp.clone(),
        },
    )?;
    write_json(dir, "corpus.json", &cp.corpus)?;
    write_json(dir, "provenance.json", &cp.provenance)
}
struct RunLock {
    path: PathBuf,
}
impl RunLock {
    fn acquire(dir: &Path) -> Result<Self, Error> {
        let path = dir.join(".running");
        let mut file=OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e|input(format!("cannot acquire run lock {}: {e}; remove only a confirmed stale lock after a crashed process",path.display())))?;
        writeln!(file, "{}", std::process::id()).map_err(infra)?;
        Ok(Self { path })
    }
}
impl Drop for RunLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
pub fn synthesize(c: Config) -> Result<i32, Error> {
    synthesize_with_candidate(c, None)
}
pub fn synthesize_with_candidate(c: Config, candidate: Option<Function>) -> Result<i32, Error> {
    c.validate().map_err(input)?;
    if let Some(f) = &candidate {
        f.validate().map_err(input)?;
        if f.signature() != c.signature()
            || f.blocks.len() > c.search.max_blocks
            || !f.callees.is_empty()
        {
            return Err(input(
                "initial candidate requires target signature, configured block limit, and no internal calls",
            ));
        }
    }
    let oracle = Oracle::new(&c).map_err(input)?;
    let dir = PathBuf::from(&c.output.directory);
    if dir.exists() {
        return Err(input("run directory already exists; use resume"));
    }
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent).map_err(infra)?;
    }
    fs::create_dir(&dir).map_err(infra)?;
    let _lock = RunLock::acquire(&dir)?;
    let result = (|| {
        let started = Instant::now();
        let corpus = oracle.corpus(&c).map_err(infra)?;
        write_json(&dir, "config.json", &c)?;
        let engine = crate::gpu::engine(&c, &corpus).map_err(input)?;
        let mut state = engine.initialize(c.seed).map_err(infra)?;
        let initial_candidate_hash = if let Some(f) = candidate {
            let f = f.normalized().map_err(input)?;
            let genome = if f.blocks.len() == 1 {
                if let Terminator::Return { value } = f.blocks[0].terminator {
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
                } else {
                    Genome {
                        cfg: Some(f.clone()),
                        genes: vec![],
                        output: 0,
                    }
                }
            } else {
                Genome {
                    cfg: Some(f.clone()),
                    genes: vec![],
                    output: 0,
                }
            };
            if !genome.valid(&c.signature(), &c.search) {
                return Err(input("initial candidate violates configured limits"));
            }
            atomic(
                &dir.join("initial.gremlin"),
                print_source(&f).map_err(input)?.as_bytes(),
            )?;
            let mut fitness = engine.evaluate(&genome).map_err(input)?;
            fitness.cases.clear();
            state.population[0] = Individual { genome, fitness };
            state.population.sort_by(|a, b| a.fitness.cmp(&b.fitness));
            state.best = state.population[0].clone();
            Some(hash(&f.canonical_bytes().map_err(input)?))
        } else {
            None
        };
        let mut cp = Checkpoint {
            schema_version: 2,
            semantics_version: SEMANTICS_VERSION.into(),
            prng_version: "splitmix64-v1".into(),
            build_identity: build_identity()?,
            config_hash: object_hash(&c),
            target_hash: object_hash(&corpus.target),
            corpus_hash: corpus.content_hash().map_err(infra)?,
            file_hashes: BTreeMap::new(),
            config: c.clone(),
            state,
            runtime_seconds: started.elapsed().as_secs_f64(),
            provenance: corpus.provenance.clone(),
            corpus,
            refinement_rounds: 0,
            counterexamples: vec![],
            initial_candidate_hash,
        };
        persist(&dir, &mut cp)?;
        evolve(&dir, cp, engine, oracle)
    })();
    if let Err((_, error)) = &result {
        error_report(&dir, &c, error);
    }
    result
}
pub fn resume(path: &Path) -> Result<i32, Error> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let envelope: Envelope = read(path)?;
    let mut cp = envelope.checkpoint;
    if envelope.payload_hash != object_hash(&cp) {
        return Err(input("checkpoint integrity mismatch"));
    }
    if cp.schema_version != 2
        || cp.semantics_version != SEMANTICS_VERSION
        || cp.prng_version != "splitmix64-v1"
        || cp.build_identity != build_identity()?
    {
        return Err(input(
            "incompatible checkpoint schema, semantics, PRNG, or executable build",
        ));
    }
    cp.config.validate().map_err(input)?;
    let names = snapshot_names(&cp);
    let allowed = [
        "config.json".to_string(),
        names[0].clone(),
        names[1].clone(),
    ];
    if cp.file_hashes.len() != 3 {
        return Err(input("checkpoint missing persisted file hashes"));
    }
    for (name, h) in &cp.file_hashes {
        if !allowed.contains(name) || file_hash(&dir.join(name))? != *h {
            return Err(input(format!("persisted file integrity mismatch: {name}")));
        }
    }
    let config: Config = read(&dir.join("config.json"))?;
    let corpus: Corpus = read(&dir.join(&names[0]))?;
    let provenance: Vec<Provenance> = read(&dir.join(&names[1]))?;
    if corpus != cp.corpus
        || provenance != cp.provenance
        || provenance.len() != corpus.cases.len()
        || provenance
            .iter()
            .zip(&corpus.cases)
            .any(|(p, c)| p.input != c.input || p.records.is_empty())
    {
        return Err(input("checkpoint corpus/provenance mismatch"));
    }
    // Human-readable projections may be from the previous committed snapshot after a crash.
    for (name, expected) in [
        ("corpus.json", bytes(&corpus)?),
        ("provenance.json", bytes(&provenance)?),
    ] {
        if dir.join(name).exists() {
            let actual = fs::read(dir.join(name)).map_err(input)?;
            if actual != expected {
                let old = if name == "corpus.json" {
                    let c: Corpus = serde_json::from_slice(&actual).map_err(input)?;
                    format!("corpus.{}.json", c.content_hash().map_err(input)?)
                } else {
                    let p: Vec<Provenance> = serde_json::from_slice(&actual).map_err(input)?;
                    format!("provenance.{}.json", object_hash(&p))
                };
                if fs::read(dir.join(old)).map_err(input)? != actual {
                    return Err(input("modified corpus/provenance projection"));
                }
            }
        }
    }
    let oracle = Oracle::new(&config).map_err(input)?;
    let target = oracle.identity(&config.target.name).map_err(input)?;
    if config != cp.config
        || cp.config_hash != object_hash(&config)
        || cp.target_hash != object_hash(&corpus.target)
        || cp.corpus_hash != corpus.content_hash().map_err(input)?
        || corpus.target != target
    {
        return Err(input("checkpoint target/config/corpus identity mismatch"));
    }
    if fs::canonicalize(&config.output.directory).map_err(input)?
        != fs::canonicalize(dir).map_err(input)?
    {
        return Err(input("checkpoint output directory mismatch"));
    }
    cp.corpus.provenance = provenance;
    let engine = crate::gpu::engine(&config, &cp.corpus).map_err(input)?;
    engine.validate_state(&cp.state).map_err(input)?;
    let _lock = RunLock::acquire(dir)?;
    persist(dir, &mut cp)?;
    let result = evolve(dir, cp, engine, oracle);
    if let Err((_, e)) = &result {
        error_report(dir, &config, e);
    }
    result
}
struct Validation {
    seed: u64,
    requested: usize,
    domain_exhausted: bool,
    records: Vec<Json>,
    counterexamples: Vec<Case>,
}
fn validate(cp: &Checkpoint, oracle: &Oracle) -> Result<Validation, Error> {
    let seed = cp.config.seed
        ^ 0xd1b54a32d192ed03
        ^ (cp.refinement_rounds as u64).wrapping_mul(0x9e3779b97f4a7c15);
    let requested = cp
        .config
        .refinement
        .as_ref()
        .map_or(cp.config.corpus.holdout_cases, |r| {
            r.differential_cases.max(cp.config.corpus.holdout_cases)
        });
    let mut inputs = holdout_inputs(&cp.config.signature(), &cp.corpus, seed, requested);
    if cp.config.refinement.is_some() && requested > 0 {
        let ones: Vec<_> = cp
            .config
            .signature()
            .arguments
            .iter()
            .map(|t| Value::new(*t, 1))
            .collect();
        let key: Vec<_> = ones.iter().map(|v| v.hex()).collect();
        if !cp.corpus.cases.iter().any(|c| c.input == key) {
            inputs.inputs.retain(|a| *a != ones);
            inputs.inputs.insert(0, ones);
            inputs.inputs.truncate(requested);
        }
    }
    let f = cp.state.best.genome.lower(&cp.config.signature());
    let mut evaluator = Evaluator::new(&f).map_err(infra)?;
    let expected = oracle.observe(&inputs.inputs).map_err(infra)?;
    let mut result = Validation {
        seed,
        requested,
        domain_exhausted: inputs.domain_exhausted,
        records: vec![],
        counterexamples: vec![],
    };
    for (args, expected) in inputs.inputs.iter().zip(expected) {
        let actual = evaluator.execute(args, cp.config.search.max_steps);
        let matches = actual.outcome == Outcome::Completed(expected);
        let input: Vec<_> = args.iter().map(|v| v.hex()).collect();
        result.records.push(
            json!({"input":input,"expected":expected.hex(),"execution":actual,"matches":matches}),
        );
        if !matches {
            result.counterexamples.push(Case {
                input,
                expected: expected.hex(),
            });
        }
    }
    Ok(result)
}
fn evolve(
    dir: &Path,
    mut cp: Checkpoint,
    mut engine: Engine,
    oracle: Oracle,
) -> Result<i32, Error> {
    let start = Instant::now();
    let prior = cp.runtime_seconds;
    let (validation, code, reason) = loop {
        while !cp.state.best.fitness.matches() && cp.state.generation < cp.config.search.generations
        {
            engine.advance(&mut cp.state).map_err(infra)?;
            cp.runtime_seconds = prior + start.elapsed().as_secs_f64();
            persist(dir, &mut cp)?;
            if cp.state.generation.is_multiple_of(10) {
                eprintln!(
                    "generation {}: {} incomplete, {} mismatches, {} bit errors",
                    cp.state.generation,
                    cp.state.best.fitness.noncompleted_case_count,
                    cp.state.best.fitness.mismatching_completed_case_count,
                    cp.state.best.fitness.summed_bit_error
                );
            }
        }
        if !cp.state.best.fitness.matches() {
            break (None, 3, "generation_budget_exhausted");
        }
        let validation = validate(&cp, &oracle)?;
        if validation.counterexamples.is_empty() {
            break (Some(validation), 0, "corpus_and_requested_validation_match");
        }
        let Some(refinement) = cp.config.refinement.clone() else {
            break (Some(validation), 6, "holdout_counterexample");
        };
        let rejected_hash = hash(&cp.state.best.fitness.canonical_program_bytes);
        let replay_inputs = validation
            .counterexamples
            .iter()
            .map(|c| decode_input(&cp.config.signature(), &c.input))
            .collect::<Result<Vec<_>, _>>()
            .map_err(infra)?;
        let replay = oracle.observe(&replay_inputs).map_err(infra)?;
        for (case, value) in validation.counterexamples.iter().zip(replay) {
            if case.expected != value.hex() {
                return Err(infra("counterexample replay disagreement"));
            }
            cp.corpus
                .add(
                    case.input.clone(),
                    case.expected.clone(),
                    format!(
                        "cegis:round:{}:seed:{}:candidate:{rejected_hash}",
                        cp.refinement_rounds + 1,
                        validation.seed
                    ),
                )
                .map_err(infra)?;
            cp.counterexamples.push(json!({"candidate_hash":rejected_hash,"round":cp.refinement_rounds+1,"case":case,"replayed":true}));
        }
        cp.refinement_rounds += 1;
        engine = crate::gpu::engine(&cp.config, &cp.corpus).map_err(infra)?;
        engine.regrade(&mut cp.state).map_err(infra)?;
        cp.runtime_seconds = prior + start.elapsed().as_secs_f64();
        persist(dir, &mut cp)?;
        eprintln!(
            "refinement {} retained {} counterexamples; corpus now {} cases",
            cp.refinement_rounds,
            validation.counterexamples.len(),
            cp.corpus.cases.len()
        );
        if cp.refinement_rounds >= refinement.max_rounds {
            break (None, 3, "refinement_budget_exhausted");
        }
    };
    cp.runtime_seconds = prior + start.elapsed().as_secs_f64();
    persist(dir, &mut cp)?;
    finish(dir, &cp, &oracle, validation, code, reason)
}
fn finish(
    dir: &Path,
    cp: &Checkpoint,
    oracle: &Oracle,
    validation: Option<Validation>,
    code: i32,
    reason: &str,
) -> Result<i32, Error> {
    let f = cp
        .state
        .best
        .genome
        .lower(&cp.config.signature())
        .normalized()
        .map_err(infra)?;
    atomic(
        &dir.join("best.gremlin"),
        print_source(&f).map_err(infra)?.as_bytes(),
    )?;
    write_json(dir, "best.ir.json", &f)?;
    let matched = cp.state.best.fitness.matches();
    let level = if matched {
        Some(
            if code == 0 && validation.as_ref().is_some_and(|v| !v.records.is_empty()) {
                "E2"
            } else {
                "E1"
            },
        )
    } else {
        None
    };
    let status = match code {
        0 => "completed",
        6 => "counterexample_found",
        _ => "budget_exhausted",
    };
    let scope = oracle.scope();
    let candidate_hash = hash(&f.canonical_bytes().map_err(infra)?);
    let holdout = match &validation {
        Some(v) => {
            json!({"attempted":true,"seed":v.seed,"requested_count":v.requested,"actual_count":v.records.len(),"domain_exhausted":v.domain_exhausted,"mismatch_count":v.counterexamples.len(),"results":v.records})
        }
        None => json!({"attempted":false,"actual_count":null,"mismatch_count":null,"results":[]}),
    };
    let mut evidence = vec![];
    if matched {
        evidence.push(json!({"level":"E1","scope":scope,"candidate_hash":candidate_hash,"corpus_hash":cp.corpus_hash,"case_count":cp.corpus.cases.len(),"max_steps":cp.config.search.max_steps,"assumptions":["configured deterministic pure target contract","bounded gremlin interpreter"]}));
    }
    if level == Some("E2") {
        evidence.push(json!({"level":"E2","scope":scope,"candidate_hash":candidate_hash,"holdout":holdout,"assumptions":["sampled differential evidence, not proof"]}));
    }
    // Detailed diagnostics are materialized once, not stored for every population member.
    let best_fitness = Engine::new(cp.config.search.clone(), &cp.corpus)
        .map_err(infra)?
        .evaluate(&cp.state.best.genome)
        .map_err(infra)?;
    let mut compact = best_fitness.clone();
    compact.cases.clear();
    let mut stored = cp.state.best.fitness.clone();
    stored.cases.clear();
    if compact != stored {
        return Err(infra("best fitness differs from CPU replay"));
    }
    let report = json!({"schema_version":2,"semantics_version":SEMANTICS_VERSION,"run_identity":object_hash(&(cp.config_hash.clone(),cp.corpus_hash.clone(),cp.build_identity.clone())),"build_identity":cp.build_identity,"target":cp.corpus.target,"target_hash":cp.target_hash,"oracle":oracle.details(),"seed":cp.config.seed,"configuration":cp.config,"candidate_hash":candidate_hash,"initial_candidate_hash":cp.initial_candidate_hash,"corpus_hash":cp.corpus_hash,"corpus_count":cp.corpus.cases.len(),"backend":if cp.config.search.cuda.is_some(){"cuda"}else{"cpu-reference"},"evaluation_count":cp.state.evaluation_count,"evaluation_count_unit":"candidate-case search executions including corpus regrading","generations":cp.state.generation,"runtime_seconds":cp.runtime_seconds,"execution_failure_counts":cp.state.failures,"best_fitness":best_fitness,"holdout":holdout,"refinement_rounds":cp.refinement_rounds,"counterexamples":cp.counterexamples,"stop_reason":reason,"run_status":status,"evidence_level":level,"evidence_scope":scope,"label":level.map(|_|"TESTED"),"successful_replacement":code==0,"evidence":evidence,"cuda_telemetry_file":cp.config.search.cuda.as_ref().map(|_|"cuda-batches.jsonl"),"unattempted_stages":["formal verification","LLVM/native compilation","external fuzzing"]});
    write_json(dir, "report.json", &report)?;
    let mut hashes = cp.file_hashes.clone();
    for name in [
        "config.json",
        "corpus.json",
        "provenance.json",
        "best.gremlin",
        "best.ir.json",
        "checkpoint.json",
        "report.json",
    ] {
        hashes.insert(name.into(), file_hash(&dir.join(name))?);
    }
    for name in ["cuda-batches.jsonl", "initial.gremlin"] {
        if dir.join(name).exists() {
            hashes.insert(name.into(), file_hash(&dir.join(name))?);
        }
    }
    write_json(dir, "integrity.json", &hashes)?;
    println!(
        "{}",
        json!({"run_status":status,"evidence_level":level,"evidence_scope":scope,"stop_reason":reason,"report":dir.join("report.json"),"candidate":dir.join("best.gremlin")})
    );
    Ok(code)
}
fn error_report(dir: &Path, c: &Config, error: &str) {
    let _ = write_json(
        dir,
        "report.json",
        &json!({"schema_version":2,"run_status":"error","evidence_level":Json::Null,"evidence_scope":if c.target.kind=="binary"{"binary"}else{"fixture"},"configuration":c,"error":error}),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_timing_preserves_hash_after_json_round_trip() {
        for ticks in 1..10_000_u64 {
            let timing = ticks as f64 / 1_000_000_000.0;
            let encoded = serde_json::to_vec(&timing).unwrap();
            let decoded: f64 = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(timing.to_bits(), decoded.to_bits());
            assert_eq!(object_hash(&timing), object_hash(&decoded));
        }
    }
    #[test]
    fn failed_holdout_retains_e1_but_is_not_success() {
        let dir = std::env::temp_dir().join(format!("gremlin-holdout-test-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let mut config =
            Config::parse(include_str!("../../../tests/fixtures/increment_u64.toml")).unwrap();
        config.output.directory = dir.to_string_lossy().into_owned();
        config.search.population = 16;
        config.search.elite = 2;
        config.corpus.holdout_cases = 8;
        let mut corpus = Corpus::new(TargetIdentity::fixture("increment_u64").unwrap());
        corpus
            .add(
                vec!["0x0000000000000000".into()],
                "0x0000000000000001".into(),
                "deliberately sparse test corpus".into(),
            )
            .unwrap();
        let engine = Engine::new(config.search.clone(), &corpus).unwrap();
        let state = engine.initialize(1).unwrap();
        assert!(state.best.fitness.matches());
        write_json(&dir, "config.json", &config).unwrap();
        write_json(&dir, "corpus.json", &corpus).unwrap();
        write_json(&dir, "provenance.json", &corpus.provenance).unwrap();
        let checkpoint = Checkpoint {
            schema_version: 2,
            semantics_version: SEMANTICS_VERSION.into(),
            prng_version: "splitmix64-v1".into(),
            build_identity: build_identity().unwrap(),
            config_hash: object_hash(&config),
            target_hash: object_hash(&corpus.target),
            corpus_hash: corpus.content_hash().unwrap(),
            file_hashes: BTreeMap::new(),
            corpus: corpus.clone(),
            provenance: corpus.provenance.clone(),
            refinement_rounds: 0,
            counterexamples: vec![],
            initial_candidate_hash: None,
            config,
            state,
            runtime_seconds: 0.0,
        };
        assert_eq!(
            evolve(
                &dir,
                checkpoint,
                engine,
                Oracle::Fixture("increment_u64".into())
            )
            .unwrap(),
            6
        );
        let report: Json = read(&dir.join("report.json")).unwrap();
        assert_eq!(report["run_status"], "counterexample_found");
        assert_eq!(report["evidence_level"], "E1");
        assert_eq!(report["successful_replacement"], false);
        assert!(report["holdout"]["mismatch_count"].as_u64().unwrap() > 0);
        fs::remove_dir_all(dir).unwrap();
    }
}
