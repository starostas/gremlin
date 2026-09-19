use crate::{
    lifter::{lift, BinaryModel},
    solver::{self, Answer, SolverIdentity},
    symbolic::{self, Symbolic},
};
use gremlin_core::*;
use serde::Serialize;
use std::{path::Path, time::Instant};
#[derive(Debug, Serialize, PartialEq, Eq)]
pub enum Status {
    Equivalent,
    Counterexample,
    Timeout,
    Unsupported,
    Unknown,
}
#[derive(Debug, Serialize)]
pub struct ProofReport {
    pub schema_version: u32,
    pub semantics_version: String,
    pub status: Status,
    pub evidence_level: Option<String>,
    pub evidence_scope: String,
    pub candidate_hash: String,
    pub reference_hash: Option<String>,
    pub binary_model: Option<BinaryModel>,
    pub solver: Option<SolverIdentity>,
    pub query_hash: Option<String>,
    pub query: Option<String>,
    pub reason: Option<String>,
    pub input: Option<Vec<Value>>,
    pub candidate_observation: Option<Execution>,
    pub target_observation: Option<Value>,
    pub assumptions: Vec<String>,
    pub runtime_seconds: f64,
}
fn report(f: &Function, scope: &str) -> Result<ProofReport, String> {
    Ok(ProofReport {
        schema_version: 1,
        semantics_version: SEMANTICS_VERSION.into(),
        status: Status::Unsupported,
        evidence_level: None,
        evidence_scope: scope.into(),
        candidate_hash: hash(&f.canonical_bytes()?),
        reference_hash: None,
        binary_model: None,
        solver: None,
        query_hash: None,
        query: None,
        reason: None,
        input: None,
        candidate_observation: None,
        target_observation: None,
        assumptions: vec![
            "fixed-width gremlin semantics".into(),
            "complete straight-line execution; no loop bound".into(),
        ],
        runtime_seconds: 0.,
    })
}
fn query(f: &Function, a: &Symbolic, b: &Symbolic) -> String {
    let mut q = String::from("(set-logic QF_BV)\n");
    for (n, p) in f.parameters.iter().enumerate() {
        q.push_str(&format!(
            "(declare-fun x{n} () (_ BitVec {}))\n",
            p.ty.width()
        ));
    }
    q.push_str(&a.definitions);
    q.push_str(&b.definitions);
    q.push_str(&format!(
        "(assert (or (not (= {} {})) (and {} {} (not (= {} {})))))\n",
        a.trap, b.trap, a.completed, b.completed, a.value, b.value
    ));
    q
}
fn solve(
    f: &Function,
    a: &Symbolic,
    b: &Symbolic,
    path: &Path,
    timeout_ms: u64,
    r: &mut ProofReport,
) -> Result<Option<Vec<Value>>, String> {
    let identity = solver::identity(path, timeout_ms)?;
    let query = query(f, a, b);
    let answer = solver::solve(&identity, &query, f.parameters.len())?;
    r.query_hash = Some(hash(query.as_bytes()));
    r.query = Some(query);
    r.solver = Some(identity);
    match answer {
        Answer::Unsat => {
            r.status = Status::Equivalent;
            r.evidence_level = Some("E4".into());
            Ok(None)
        }
        Answer::Timeout => {
            r.status = Status::Timeout;
            Ok(None)
        }
        Answer::Unknown(reason) => {
            r.status = Status::Unknown;
            r.reason = Some(reason);
            Ok(None)
        }
        Answer::Sat(bits) => {
            let values = bits
                .into_iter()
                .zip(&f.parameters)
                .map(|(bits, p)| {
                    if bits & !p.ty.mask() != 0 {
                        return Err("solver value exceeds input width".into());
                    }
                    Ok(Value::new(p.ty, bits))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Some(values))
        }
    }
}
pub fn verify_binary(
    f: &Function,
    contract: &BinaryContract,
    solver: &Path,
    timeout_ms: u64,
    mut oracle: impl FnMut(&[Value]) -> Result<Value, String>,
) -> Result<ProofReport, String> {
    let start = Instant::now();
    let mut r = report(f, "binary")?;
    let candidate = match symbolic::candidate(f, "c") {
        Ok(c) => c,
        Err(e) => {
            r.reason = Some(e);
            return Ok(r);
        }
    };
    let model = match lift(contract, &f.signature()) {
        Ok(m) => m,
        Err(e) => {
            r.reason = Some(e);
            return Ok(r);
        }
    };
    r.assumptions.extend(["explicit SysV integer ABI with valid caller return address".into(),"initializer/dependency/relocation-free ELF; immutable executable code".into(),"only documented caller-saved register instructions; flags unused, no memory or uncovered paths".into(),"lifter and solver soundness are trusted; differential checks are not a soundness proof".into()]);
    let target = model.symbolic()?;
    if let Some(input) = solve(f, &candidate, &target, solver, timeout_ms, &mut r)? {
        let model_value = model.execute(&input)?;
        let observed = oracle(&input)?;
        let concrete = execute(f, &input, f.blocks[0].instructions.len() as u64 + 1);
        if model_value != observed {
            return Err(
                "modeling error: lifted target disagrees with original isolated oracle".into(),
            );
        }
        if concrete.outcome == Outcome::Completed(observed) {
            return Err("modeling error: solver counterexample does not replay concretely".into());
        }
        r.status = Status::Counterexample;
        r.input = Some(input);
        r.candidate_observation = Some(concrete);
        r.target_observation = Some(observed);
    }
    r.binary_model = Some(model);
    r.runtime_seconds = start.elapsed().as_secs_f64();
    Ok(r)
}
pub fn verify_reference(
    f: &Function,
    reference: &Function,
    solver: &Path,
    timeout_ms: u64,
) -> Result<ProofReport, String> {
    let start = Instant::now();
    let mut r = report(f, "reference_model")?;
    r.reference_hash = Some(hash(&reference.canonical_bytes()?));
    if f.signature() != reference.signature() {
        return Err("reference/candidate signature mismatch".into());
    }
    let a = match symbolic::candidate(f, "c") {
        Ok(a) => a,
        Err(e) => {
            r.reason = Some(e);
            return Ok(r);
        }
    };
    let b = match symbolic::candidate(reference, "r") {
        Ok(b) => b,
        Err(e) => {
            r.reason = Some(e);
            return Ok(r);
        }
    };
    if let Some(input) = solve(f, &a, &b, solver, timeout_ms, &mut r)? {
        let candidate = execute(f, &input, f.blocks[0].instructions.len() as u64 + 1);
        let target = execute(
            reference,
            &input,
            reference.blocks[0].instructions.len() as u64 + 1,
        );
        if candidate.outcome == target.outcome {
            return Err("modeling error: reference counterexample does not replay".into());
        }
        r.status = Status::Counterexample;
        r.input = Some(input);
        r.candidate_observation = Some(candidate);
        if let Outcome::Completed(v) = target.outcome {
            r.target_observation = Some(v);
        }
    }
    r.assumptions
        .push("reference source has no independently established relationship to a binary".into());
    r.runtime_seconds = start.elapsed().as_secs_f64();
    Ok(r)
}
