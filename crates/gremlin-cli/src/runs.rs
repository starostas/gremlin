use crate::{input, Error};
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
fn bytes<T: Serialize>(x: &T) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(x).map_err(infra)
}
fn read<T: for<'a> Deserialize<'a>>(p: &Path) -> Result<T, Error> {
    serde_json::from_slice(&fs::read(p).map_err(input)?).map_err(input)
}
fn atomic(path: &Path, data: &[u8]) -> Result<(), Error> {
    let temp = path.with_extension("tmp");
    let mut f = File::create(&temp).map_err(infra)?;
    f.write_all(data).map_err(infra)?;
    f.sync_all().map_err(infra)?;
    fs::rename(temp, path).map_err(infra)?;
    File::open(path.parent().unwrap())
        .and_then(|d| d.sync_all())
        .map_err(infra)
}
fn write_json<T: Serialize>(dir: &Path, name: &str, value: &T) -> Result<(), Error> {
    atomic(&dir.join(name), &bytes(value)?)
}
fn file_hash(path: &Path) -> Result<String, Error> {
    Ok(hash(&fs::read(path).map_err(infra)?))
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
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload_hash: String,
    checkpoint: Checkpoint,
}
fn persist(dir: &Path, checkpoint: &Checkpoint) -> Result<(), Error> {
    let envelope = Envelope {
        payload_hash: object_hash(checkpoint),
        checkpoint: checkpoint.clone(),
    };
    write_json(dir, "checkpoint.json", &envelope)
}
struct RunLock {
    path: PathBuf,
}
impl RunLock {
    fn acquire(dir: &Path) -> Result<Self, Error> {
        let path = dir.join(".running");
        let mut f=OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e|input(format!("cannot acquire run lock {}: {e}; if a previous process crashed, remove its stale .running file before resuming",path.display())))?;
        writeln!(f, "{}", std::process::id()).map_err(infra)?;
        Ok(Self { path })
    }
}
impl Drop for RunLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
pub fn synthesize(c: Config) -> Result<i32, Error> {
    c.validate().map_err(input)?;
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
        let corpus =
            fixture_corpus(&c.target.name, c.seed, c.corpus.random_cases).map_err(infra)?;
        write_json(&dir, "config.json", &c)?;
        write_json(&dir, "corpus.json", &corpus)?;
        write_json(&dir, "provenance.json", &corpus.provenance)?;
        let file_hashes = ["config.json", "corpus.json", "provenance.json"]
            .into_iter()
            .map(|name| Ok((name.into(), file_hash(&dir.join(name))?)))
            .collect::<Result<_, Error>>()?;
        let engine = Engine::new(c.search.clone(), &corpus).map_err(input)?;
        let started = Instant::now();
        let state = engine.initialize(c.seed).map_err(infra)?;
        let checkpoint = Checkpoint {
            schema_version: SCHEMA_VERSION,
            semantics_version: SEMANTICS_VERSION.into(),
            prng_version: "splitmix64-v1".into(),
            build_identity: build_identity()?,
            config_hash: object_hash(&c),
            target_hash: object_hash(&corpus.target),
            corpus_hash: corpus.content_hash().map_err(infra)?,
            file_hashes,
            config: c.clone(),
            state,
            runtime_seconds: started.elapsed().as_secs_f64(),
        };
        persist(&dir, &checkpoint)?;
        evolve(&dir, checkpoint, corpus, engine)
    })();
    if let Err((_, e)) = &result {
        error_report(&dir, &c, e);
    }
    result
}
pub fn resume(path: &Path) -> Result<i32, Error> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let envelope: Envelope = read(path)?;
    let cp = envelope.checkpoint;
    if envelope.payload_hash != object_hash(&cp) {
        return Err(input("checkpoint integrity mismatch"));
    }
    if cp.schema_version != SCHEMA_VERSION
        || cp.semantics_version != SEMANTICS_VERSION
        || cp.prng_version != "splitmix64-v1"
        || cp.build_identity != build_identity()?
    {
        return Err(input(
            "incompatible checkpoint schema, semantics, PRNG, or executable build",
        ));
    }
    cp.config.validate().map_err(input)?;
    for (name, h) in &cp.file_hashes {
        if !["config.json", "corpus.json", "provenance.json"].contains(&name.as_str())
            || file_hash(&dir.join(name))? != *h
        {
            return Err(input(format!("persisted file integrity mismatch: {name}")));
        }
    }
    if cp.file_hashes.len() != 3 {
        return Err(input("checkpoint missing persisted file hashes"));
    }
    let config: Config = read(&dir.join("config.json"))?;
    let corpus: Corpus = read(&dir.join("corpus.json"))?;
    let provenance: Vec<Provenance> = read(&dir.join("provenance.json"))?;
    if provenance.len() != corpus.cases.len()
        || provenance
            .iter()
            .zip(&corpus.cases)
            .any(|(p, c)| p.input != c.input || p.records.is_empty())
    {
        return Err(input("provenance/corpus mismatch"));
    }
    let expected_target = TargetIdentity::fixture(&config.target.name).map_err(input)?;
    if config != cp.config
        || cp.config_hash != object_hash(&config)
        || cp.target_hash != object_hash(&corpus.target)
        || cp.corpus_hash != corpus.content_hash().map_err(input)?
        || corpus.target != expected_target
    {
        return Err(input("checkpoint target/config/corpus identity mismatch"));
    }
    if fs::canonicalize(&config.output.directory).map_err(input)?
        != fs::canonicalize(dir).map_err(input)?
    {
        return Err(input("checkpoint output directory mismatch"));
    }
    let engine = Engine::new(config.search.clone(), &corpus).map_err(input)?;
    engine.validate_state(&cp.state).map_err(input)?;
    let _lock = RunLock::acquire(dir)?;
    let result = evolve(dir, cp, corpus, engine);
    if let Err((_, e)) = &result {
        error_report(dir, &config, e);
    }
    result
}
fn evolve(dir: &Path, mut cp: Checkpoint, corpus: Corpus, engine: Engine) -> Result<i32, Error> {
    let start = Instant::now();
    let prior = cp.runtime_seconds;
    while !cp.state.best.fitness.matches() && cp.state.generation < cp.config.search.generations {
        engine.advance(&mut cp.state).map_err(infra)?;
        cp.runtime_seconds = prior + start.elapsed().as_secs_f64();
        persist(dir, &cp)?;
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
    let holdout_seed = cp.config.seed ^ 0xd1b54a32d192ed03;
    let mut holdout_records = vec![];
    let mut exhausted = None;
    if matched {
        let inputs = holdout_inputs(
            &cp.config.signature(),
            &corpus,
            holdout_seed,
            cp.config.corpus.holdout_cases,
        );
        exhausted = Some(inputs.domain_exhausted);
        let mut evaluator = Evaluator::new(&f).map_err(infra)?;
        for args in &inputs.inputs {
            let expected = fixture_observe(&cp.config.target.name, args).map_err(infra)?;
            let actual = evaluator.execute(args, cp.config.search.max_steps);
            let matches = actual.outcome == Outcome::Completed(expected);
            holdout_records.push(json!({"input":args.iter().map(|v|v.hex()).collect::<Vec<_>>(),"expected":expected.hex(),"execution":actual,"matches":matches}));
        }
    }
    let mismatches = holdout_records
        .iter()
        .filter(|v| v["matches"] == false)
        .count();
    let (status, level, reason, code) = if !matched {
        ("budget_exhausted", None, "generation_budget_exhausted", 3)
    } else if mismatches > 0 {
        (
            "counterexample_found",
            Some("E1"),
            "holdout_counterexample",
            6,
        )
    } else if cp.config.corpus.holdout_cases == 0 || holdout_records.is_empty() {
        (
            "completed",
            Some("E1"),
            "corpus_match_without_holdout_observations",
            0,
        )
    } else {
        ("completed", Some("E2"), "corpus_and_holdout_match", 0)
    };
    cp.runtime_seconds = prior + start.elapsed().as_secs_f64();
    persist(dir, &cp)?;
    let candidate_hash = hash(&f.canonical_bytes().map_err(infra)?);
    let mut evidence = vec![];
    if matched {
        evidence.push(json!({"level":"E1","scope":"fixture","candidate_hash":candidate_hash,"corpus_hash":cp.corpus_hash,"case_count":corpus.cases.len(),"assumptions":["checked-in pure Rust development oracle","bounded CPU interpreter"]}));
    }
    if level == Some("E2") {
        evidence.push(json!({"level":"E2","scope":"fixture","seed":holdout_seed,"case_count":holdout_records.len(),"assumptions":["sampled holdout only; not exhaustive proof"]}));
    }
    let report = json!({"schema_version":SCHEMA_VERSION,"semantics_version":SEMANTICS_VERSION,"run_identity":object_hash(&(cp.config_hash.clone(),cp.corpus_hash.clone(),cp.build_identity.clone())),"build_identity":cp.build_identity,"target":corpus.target,"target_hash":cp.target_hash,"seed":cp.config.seed,"configuration":cp.config,"candidate_hash":candidate_hash,"corpus_hash":cp.corpus_hash,"corpus_count":corpus.cases.len(),"corpus_generation":{"seed":cp.config.seed,"requested_random_cases":cp.config.corpus.random_cases,"boundary_scheme":"zero/extrema/powers/adjacent/alternating/argument-and-interaction-v1","actual_unique_cases":corpus.cases.len()},"backend":"cpu-reference","evaluation_count":cp.state.evaluation_count,"evaluation_count_unit":"candidate-case executions during search","generations":cp.state.generation,"runtime_seconds":cp.runtime_seconds,"execution_failure_counts":cp.state.failures,"best_fitness":cp.state.best.fitness,"holdout":{"attempted":matched,"seed":holdout_seed,"requested_count":cp.config.corpus.holdout_cases,"actual_count":if matched{Some(holdout_records.len())}else{None},"domain_exhausted":exhausted,"mismatch_count":if matched{Some(mismatches)}else{None},"results":holdout_records},"stop_reason":reason,"run_status":status,"evidence_level":level,"evidence_scope":"fixture","label":level.map(|_|"TESTED"),"successful_replacement":code==0,"evidence":evidence,"unsupported_unattempted_stages":["binary execution","CEGIS refinement","CUDA","formal verification","LLVM/native compilation","external fuzzing"]});
    write_json(dir, "report.json", &report)?;
    let hashes = [
        "config.json",
        "corpus.json",
        "provenance.json",
        "best.gremlin",
        "best.ir.json",
        "checkpoint.json",
        "report.json",
    ]
    .into_iter()
    .map(|name| Ok((name, file_hash(&dir.join(name))?)))
    .collect::<Result<BTreeMap<_, _>, Error>>()?;
    write_json(dir, "integrity.json", &hashes)?;
    println!(
        "{}",
        json!({"run_status":status,"evidence_level":level,"evidence_scope":"fixture","stop_reason":reason,"report":dir.join("report.json"),"candidate":dir.join("best.gremlin")})
    );
    Ok(code)
}
fn error_report(dir: &Path, c: &Config, error: &str) {
    let _ = write_json(
        dir,
        "report.json",
        &json!({"schema_version":1,"run_status":"error","evidence_level":Json::Null,"evidence_scope":"fixture","configuration":c,"error":error}),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
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
            schema_version: 1,
            semantics_version: SEMANTICS_VERSION.into(),
            prng_version: "splitmix64-v1".into(),
            build_identity: build_identity().unwrap(),
            config_hash: object_hash(&config),
            target_hash: object_hash(&corpus.target),
            corpus_hash: corpus.content_hash().unwrap(),
            file_hashes: BTreeMap::new(),
            config,
            state,
            runtime_seconds: 0.0,
        };
        assert_eq!(evolve(&dir, checkpoint, corpus, engine).unwrap(), 6);
        let report: Json = read(&dir.join("report.json")).unwrap();
        assert_eq!(report["run_status"], "counterexample_found");
        assert_eq!(report["evidence_level"], "E1");
        assert_eq!(report["successful_replacement"], false);
        assert!(report["holdout"]["mismatch_count"].as_u64().unwrap() > 0);
        fs::remove_dir_all(dir).unwrap();
    }
}
