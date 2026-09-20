//! Shared scoring for local evaluation and isolated accelerator workers.
use crate::{CaseResult, ComparatorConfig, Failures};
use gremlin_core::{Evaluator, Execution, Outcome, Type, Value};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationSummary {
    pub noncompleted: u64,
    pub mismatches: u64,
    pub bit_error: u64,
    pub steps: u64,
    pub failures: Failures,
    pub selection_cost: Option<[u64; 2]>,
}
impl EvaluationSummary {
    pub fn words(&self) -> [u64; 10] {
        let cost = self.selection_cost.unwrap_or([0, 0]);
        [
            self.noncompleted,
            self.mismatches,
            self.bit_error,
            self.steps,
            self.failures.trap,
            self.failures.timeout,
            self.failures.invalid,
            u64::from(self.selection_cost.is_some()),
            cost[0],
            cost[1],
        ]
    }
    pub fn from_words(w: [u64; 10]) -> Result<Self, String> {
        let selection_cost = match w[7] {
            0 if w[8] == 0 && w[9] == 0 => None,
            1 => Some([w[8], w[9]]),
            _ => return Err("invalid summary selection cost".into()),
        };
        Ok(Self {
            noncompleted: w[0],
            mismatches: w[1],
            bit_error: w[2],
            steps: w[3],
            failures: Failures {
                trap: w[4],
                timeout: w[5],
                invalid: w[6],
            },
            selection_cost,
        })
    }
    pub fn validate(
        &self,
        cases: usize,
        ty: Type,
        max_steps: u64,
        comparator: &ComparatorConfig,
    ) -> Result<(), String> {
        let cases = cases as u128;
        let completed = cases
            .checked_sub(self.noncompleted as u128)
            .ok_or("summary failure count exceeds cases")?;
        if self.mismatches as u128 > completed
            || self.bit_error < self.mismatches
            || self.bit_error as u128 > self.mismatches as u128 * ty.width() as u128
            || self.steps as u128 > cases * max_steps as u128
            || self.failures.trap as u128
                + self.failures.timeout as u128
                + self.failures.invalid as u128
                != self.noncompleted as u128
        {
            return Err("invalid evaluation summary totals".into());
        }
        match comparator {
            ComparatorConfig::CorrectnessFirst {} if self.selection_cost.is_none() => {}
            ComparatorConfig::BitErrorFirst {}
                if self.selection_cost == Some([0, self.bit_error]) => {}
            ComparatorConfig::Gremlin { .. }
                if self.selection_cost.is_some_and(|v| {
                    ((v[0] as u128) << 64 | v[1] as u128) <= completed * u64::MAX as u128
                }) => {}
            _ => return Err("evaluation summary comparator mismatch".into()),
        }
        Ok(())
    }
}

pub fn score_executions(
    executions: Vec<Execution>,
    expected: &[Value],
    max_steps: u64,
    comparator: &ComparatorConfig,
    details: bool,
) -> Result<(EvaluationSummary, Vec<CaseResult>), String> {
    if executions.len() != expected.len() {
        return Err("backend returned wrong case count".into());
    }
    let program = comparator.program()?;
    score_with_program(
        executions,
        expected,
        max_steps,
        comparator,
        program.as_ref(),
        details,
    )
}

fn score_with_program(
    executions: Vec<Execution>,
    expected: &[Value],
    max_steps: u64,
    comparator: &ComparatorConfig,
    program: Option<&gremlin_core::Function>,
    details: bool,
) -> Result<(EvaluationSummary, Vec<CaseResult>), String> {
    if executions.len() != expected.len() {
        return Err("backend returned wrong case count".into());
    }
    let mut scorer = program.map(Evaluator::new).transpose()?;
    let mut summary = EvaluationSummary {
        noncompleted: 0,
        mismatches: 0,
        bit_error: 0,
        steps: 0,
        failures: Failures::default(),
        selection_cost: None,
    };
    let mut cases = if details {
        Vec::with_capacity(expected.len())
    } else {
        Vec::new()
    };
    let mut custom_sum = 0u128;
    for (expected, e) in expected.iter().zip(executions) {
        if expected.bits & !expected.ty.mask() != 0 || !expected.ty.integer() {
            return Err("invalid expected value".into());
        }
        if e.steps > max_steps {
            return Err("backend exceeded step budget".into());
        }
        summary.steps = summary
            .steps
            .checked_add(e.steps)
            .ok_or("execution step sum overflow")?;
        let (bit_error, custom_cost) = match &e.outcome {
            Outcome::Completed(actual) => {
                if actual.ty != expected.ty || actual.bits & !actual.ty.mask() != 0 {
                    return Err("backend returned wrong type or bit pattern".into());
                }
                let error = (actual.bits ^ expected.bits).count_ones();
                summary.mismatches += u64::from(error != 0);
                summary.bit_error += u64::from(error);
                let cost = if let (Some(scorer), ComparatorConfig::Gremlin { max_steps, .. }) =
                    (&mut scorer, comparator)
                {
                    match scorer
                        .execute(
                            &[
                                Value::new(Type::U64, actual.bits),
                                Value::new(Type::U64, expected.bits),
                            ],
                            *max_steps,
                        )
                        .outcome
                    {
                        Outcome::Completed(cost) => {
                            custom_sum = custom_sum
                                .checked_add(cost.bits as u128)
                                .ok_or("comparator cost sum overflow")?;
                            Some(cost.bits)
                        }
                        outcome => return Err(format!("comparator execution failed: {outcome:?}")),
                    }
                } else {
                    None
                };
                (Some(error), cost)
            }
            outcome => {
                summary.noncompleted += 1;
                match outcome {
                    Outcome::Trap(_) => summary.failures.trap += 1,
                    Outcome::Timeout(_) => summary.failures.timeout += 1,
                    Outcome::Invalid(_) => summary.failures.invalid += 1,
                    _ => unreachable!(),
                }
                (None, None)
            }
        };
        if details {
            cases.push(CaseResult {
                execution: e,
                bit_error,
                custom_cost,
            });
        }
    }
    summary.selection_cost = match comparator {
        ComparatorConfig::CorrectnessFirst {} => None,
        ComparatorConfig::BitErrorFirst {} => Some([0, summary.bit_error]),
        ComparatorConfig::Gremlin { .. } => Some([(custom_sum >> 64) as u64, custom_sum as u64]),
    };
    Ok((summary, cases))
}

pub fn summarize_batch(
    results: Vec<Vec<Execution>>,
    expected: &[Value],
    max_steps: u64,
    comparator: &ComparatorConfig,
) -> Result<Vec<EvaluationSummary>, String> {
    let program = comparator.program()?;
    results
        .into_iter()
        .map(|executions| {
            score_with_program(
                executions,
                expected,
                max_steps,
                comparator,
                program.as_ref(),
                false,
            )
            .map(|(s, _)| s)
        })
        .collect()
}
