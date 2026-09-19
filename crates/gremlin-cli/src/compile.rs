use crate::{
    input,
    oracle::Oracle,
    verify::{flags, required},
    Error,
};
use gremlin_core::*;
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path, time::Instant};
fn write(path: &Path, value: &impl serde::Serialize) -> Result<(), Error> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(crate::input)?,
    )
    .map_err(crate::input)
}
pub fn command(args: &[String]) -> Result<i32, Error> {
    let flags = flags(
        args,
        &[
            "--config",
            "--candidate",
            "--corpus",
            "--min-evidence",
            "--compiler",
            "--solver",
            "--timeout-ms",
            "--output",
        ],
    )?;
    let minimum = required(&flags, "--min-evidence")?;
    if !matches!(minimum, "E1" | "E2" | "E4") {
        return Err(input("minimum evidence must be E1, E2 or E4"));
    }
    let compiler = Path::new(required(&flags, "--compiler")?);
    let timeout: u64 = required(&flags, "--timeout-ms")?
        .parse()
        .map_err(crate::input)?;
    if minimum != "E4" && flags.contains_key("--solver") {
        return Err(input("--solver is only used for E4 compilation"));
    }
    let solver = if minimum == "E4" {
        Some(Path::new(required(&flags, "--solver")?))
    } else {
        None
    };
    let config = gremlin_search::Config::parse(
        &fs::read_to_string(required(&flags, "--config")?).map_err(crate::input)?,
    )
    .map_err(crate::input)?;
    let source_path = required(&flags, "--candidate")?;
    let source = fs::read(source_path).map_err(crate::input)?;
    let function =
        parse(std::str::from_utf8(&source).map_err(crate::input)?).map_err(crate::input)?;
    if config.refinement.is_some() && !flags.contains_key("--corpus") {
        return Err(input(
            "refined candidates require --corpus with their retained counterexamples",
        ));
    }
    if function.signature() != config.signature() {
        return Err(input("candidate/target signatures differ"));
    }
    if !function.callees.is_empty() {
        return Err(input("LLVM internal calls unsupported"));
    }
    if minimum != "E1" && config.corpus.holdout_cases == 0 {
        return Err(input(
            "E2/E4 artifact validation requires positive holdout_cases",
        ));
    }
    let directory = Path::new(required(&flags, "--output")?);
    if directory.exists() {
        return Err(input("compilation output directory already exists"));
    }
    fs::create_dir_all(directory).map_err(crate::input)?;
    let candidate_hash = hash(&function.canonical_bytes().map_err(crate::input)?);
    let start = Instant::now();
    let work = (|| {
        let source_hash = hash(&source);
        fs::write(directory.join("candidate.gremlin"), source).map_err(crate::input)?;
        write(&directory.join("candidate.ir.json"), &function)?;
        write(&directory.join("config.json"), &config)?;
        let oracle = Oracle::new(&config).map_err(|e| (4, e))?;
        let mut corpus = oracle.corpus(&config).map_err(|e| (4, e))?;
        let mut imported = None;
        if let Some(path) = flags.get("--corpus") {
            let data = fs::read(path).map_err(crate::input)?;
            let additional: Corpus = serde_json::from_slice(&data).map_err(crate::input)?;
            additional.validate().map_err(crate::input)?;
            if additional.target.signature != config.signature() || additional.cases.len() > 65536 {
                return Err(input("additional corpus signature or size unsupported"));
            }
            let origin = hash(&data);
            let inputs = additional
                .cases
                .iter()
                .map(|c| decode_input(&config.signature(), &c.input))
                .collect::<Result<Vec<_>, _>>()
                .map_err(crate::input)?;
            let observed = oracle.observe(&inputs).map_err(|e| (4, e))?;
            for (input, value) in inputs.iter().zip(observed) {
                corpus
                    .add(
                        input.iter().map(|v| v.hex()).collect(),
                        value.hex(),
                        format!("compile replay from corpus {origin}"),
                    )
                    .map_err(crate::input)?;
            }
            imported = Some(origin);
        }
        let mut inputs = corpus
            .cases
            .iter()
            .map(|c| decode_input(&config.signature(), &c.input))
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::input)?;
        let corpus_count = inputs.len();
        let holdout = holdout_inputs(
            &config.signature(),
            &corpus,
            config.seed ^ 0xD5D5D5D5D5D5D5D5,
            config.corpus.holdout_cases,
        );
        let holdout_count = holdout.inputs.len();
        inputs.extend(holdout.inputs);
        let expected = oracle.observe(&inputs).map_err(|e| (4, e))?;
        let mut cpu = Evaluator::new(&function).map_err(crate::input)?;
        let executions = inputs
            .iter()
            .map(|a| cpu.execute(a, config.search.max_steps))
            .collect::<Vec<_>>();
        let gate:Vec<_>=inputs.iter().zip(&expected).zip(&executions).enumerate().map(|(n,((args,expected),execution))|json!({"input":args,"target":expected,"cpu":execution,"origin":if n<corpus_count{"corpus"}else{"holdout"},"matches":execution.outcome==Outcome::Completed(*expected)})).collect();
        write(&directory.join("candidate-validation.json"), &gate)?;
        write(&directory.join("corpus.json"), &corpus)?;
        write(&directory.join("provenance.json"), &corpus.provenance)?;
        if executions
            .iter()
            .zip(&expected)
            .any(|(e, v)| e.outcome != Outcome::Completed(*v))
        {
            return Err((
                6,
                "candidate failed requested evidence gate; no compilation performed".into(),
            ));
        }
        let measured_level = if config.corpus.holdout_cases > 0 {
            "E2"
        } else {
            "E1"
        };
        if let Some(solver) = solver {
            let contract = config
                .target
                .binary
                .as_ref()
                .ok_or_else(|| input("E4 compilation requires a binary target"))?;
            let proof =
                gremlin_verify::proof::verify_binary(&function, contract, solver, timeout, |a| {
                    oracle.observe(&[a.to_vec()]).map(|v| v[0])
                })
                .map_err(|e| (4, e))?;
            write(&directory.join("candidate-proof.json"), &proof)?;
            if proof.status != gremlin_verify::proof::Status::Equivalent
                || proof.evidence_scope != "binary"
            {
                return Err((
                    6,
                    "candidate did not achieve requested binary E4; no compilation performed"
                        .into(),
                ));
            }
        }
        let artifact = gremlin_codegen::artifact::compile(
            &function,
            config.search.max_steps,
            compiler,
            directory,
            timeout,
        )
        .map_err(|e| (4, e))?;
        let validation = gremlin_codegen::artifact::observe(
            &artifact,
            &config.signature(),
            &inputs,
            &std::env::current_exe().map_err(crate::input)?,
        )
        .map_err(|e| (4, e))?;
        let records:Vec<_>=inputs.iter().zip(&executions).zip(&expected).zip(&validation.outcomes).map(|(((input,cpu),target),native)|json!({"input":input,"cpu":cpu,"target":target,"native":native,"matches":cpu.outcome==*native && *native==Outcome::Completed(*target)})).collect();
        write(&directory.join("native-validation.json"), &records)?;
        if records.iter().any(|r| r["matches"] != true) {
            return Err((
                6,
                "compiled artifact disagrees with CPU or original target".into(),
            ));
        }
        let report = json!({"schema_version":1,"semantics_version":SEMANTICS_VERSION,"status":"validated","label":"TESTED","requested_minimum_candidate_evidence":minimum,"candidate_evidence_level":if minimum=="E4"{"E4"}else{measured_level},"candidate_evidence_scope":oracle.scope(),"candidate_hash":candidate_hash,"source_hash":source_hash,"artifact_evidence_level":measured_level,"artifact_evidence_scope":"native_artifact_differential","artifact":artifact,"oracle":oracle.details(),"native_value_identity":validation.value_identity,"native_status_identity":validation.status_identity,"native_validation_seconds":validation.wall_seconds,"native_validation_timing_scope":"isolated batch validation including startup and repeated observations; not intrinsic function latency","corpus_count":corpus_count,"holdout_count":holdout_count,"holdout_domain_exhausted":holdout.domain_exhausted,"imported_corpus_hash":imported,"runtime_seconds":start.elapsed().as_secs_f64()});
        write(&directory.join("report.json"), &report)?;
        let mut hashes = BTreeMap::new();
        for entry in fs::read_dir(directory).map_err(crate::input)? {
            let path = entry.map_err(crate::input)?.path();
            if path.is_file() {
                hashes.insert(
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    hash(&fs::read(&path).map_err(crate::input)?),
                );
            }
        }
        write(&directory.join("integrity.json"), &hashes)?;
        println!(
            "{}",
            json!({"status":"validated","artifact_label":"TESTED","artifact_evidence_level":measured_level,"report":directory.join("report.json")})
        );
        Ok(0)
    })();
    if let Err((_, error)) = &work {
        let _ = write(
            &directory.join("report.json"),
            &json!({"schema_version":1,"status":"error","artifact_evidence_level":null,"candidate_hash":candidate_hash,"error":error}),
        );
    }
    work
}
