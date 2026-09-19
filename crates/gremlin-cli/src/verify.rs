use crate::{input, read_source, Error};
use gremlin_verify::proof::{self, Status};
use std::{collections::BTreeMap, fs::OpenOptions, path::Path};
pub fn flags(args: &[String], allowed: &[&str]) -> Result<BTreeMap<String, String>, Error> {
    if args.len() % 2 != 1 {
        return Err(input("flags require values"));
    }
    let mut flags = BTreeMap::new();
    for pair in args[1..].chunks_exact(2) {
        if !allowed.contains(&pair[0].as_str())
            || flags.insert(pair[0].clone(), pair[1].clone()).is_some()
        {
            return Err(input("unknown or duplicate flag"));
        }
    }
    Ok(flags)
}
pub fn required<'a>(flags: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, Error> {
    flags
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| input(format!("missing {name}")))
}
pub fn command(args: &[String]) -> Result<i32, Error> {
    let flags = flags(
        args,
        &[
            "--config",
            "--candidate",
            "--reference",
            "--solver",
            "--timeout-ms",
            "--output",
        ],
    )?;
    let source = read_source(required(&flags, "--candidate")?)?;
    let solver = Path::new(required(&flags, "--solver")?);
    let timeout = required(&flags, "--timeout-ms")?.parse().map_err(input)?;
    let output = Path::new(required(&flags, "--output")?);
    if output.exists() {
        return Err(input("proof output already exists"));
    }
    let (report, identity) = if args[0] == "verify" {
        if flags.contains_key("--reference") {
            return Err(input("binary verify does not accept --reference"));
        }
        let c = gremlin_search::Config::parse(
            &std::fs::read_to_string(required(&flags, "--config")?).map_err(input)?,
        )
        .map_err(input)?;
        if c.target.kind != "binary" || c.signature() != source.signature() {
            return Err(input(
                "verify requires a binary target with matching signature",
            ));
        }
        let contract = c
            .target
            .binary
            .ok_or_else(|| input("missing binary contract"))?;
        let oracle = gremlin_native::BinaryOracle::new(
            contract.clone(),
            source.signature(),
            &std::env::current_exe().map_err(input)?,
        )
        .map_err(input)?;
        let report = proof::verify_binary(&source, &contract, solver, timeout, |values| {
            oracle.observe(&[values.to_vec()]).map(|v| v[0])
        })
        .map_err(|e| (4, e))?;
        (
            report,
            Some(serde_json::to_value(oracle.identity()).map_err(input)?),
        )
    } else {
        if flags.contains_key("--config") {
            return Err(input(
                "reference verification does not accept a binary config",
            ));
        }
        let reference = read_source(required(&flags, "--reference")?)?;
        (
            proof::verify_reference(&source, &reference, solver, timeout).map_err(|e| (4, e))?,
            None,
        )
    };
    let code = match report.status {
        Status::Equivalent => 0,
        Status::Counterexample => 6,
        Status::Unsupported => 2,
        Status::Timeout | Status::Unknown => 3,
    };
    let mut value = serde_json::to_value(&report).map_err(input)?;
    value["oracle_identity"] = identity.unwrap_or(serde_json::Value::Null);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(input)?;
    serde_json::to_writer_pretty(file, &value).map_err(input)?;
    println!(
        "{}",
        serde_json::json!({"status":report.status,"evidence_level":report.evidence_level,"evidence_scope":report.evidence_scope,"report":output})
    );
    Ok(code)
}
