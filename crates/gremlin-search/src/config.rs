use gremlin_core::{Op, Signature, Type, Value, SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub seed: u64,
    pub target: TargetConfig,
    pub search: SearchConfig,
    pub corpus: CorpusConfig,
    pub output: OutputConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refinement: Option<RefinementConfig>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    pub kind: String,
    pub name: String,
    pub arguments: Vec<Type>,
    pub return_type: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<gremlin_core::BinaryContract>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefinementConfig {
    pub max_rounds: usize,
    pub differential_cases: usize,
    #[serde(default)]
    pub initial_inputs: Vec<Vec<String>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchConfig {
    #[serde(default)]
    pub cuda: Option<CudaConfig>,
    pub population: usize,
    pub generations: usize,
    pub max_instructions: usize,
    pub max_steps: u64,
    pub elite: usize,
    pub tournament_size: usize,
    pub operators: Vec<Op>,
    pub constants: Vec<String>,
    #[serde(default)]
    pub enumeration_depth: u32,
    #[serde(default)]
    pub enumeration_proposals: usize,
    #[serde(default = "one_block")]
    pub max_blocks: usize,
    #[serde(default)]
    pub structural_mutation_percent: usize,
    #[serde(default)]
    pub loop_bound: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CudaConfig {
    pub memory_budget_mb: u64,
    pub wall_timeout_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusConfig {
    #[serde(default)]
    pub initial_corpus: Option<String>,
    pub random_cases: usize,
    pub holdout_cases: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    pub directory: String,
}
fn one_block() -> usize {
    1
}
impl Config {
    pub fn parse(s: &str) -> Result<Self, String> {
        let c: Self = toml::from_str(s).map_err(|e| format!("configuration: {e}"))?;
        c.validate()?;
        Ok(c)
    }
    pub fn signature(&self) -> Signature {
        Signature {
            arguments: self.target.arguments.clone(),
            return_type: self.target.return_type,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("unsupported configuration schema_version".into());
        }
        self.signature().validate()?;
        if let Some(cuda) = &self.search.cuda {
            if cuda.memory_budget_mb == 0
                || cuda.memory_budget_mb > 65536
                || cuda.wall_timeout_ms == 0
                || cuda.wall_timeout_ms > 600000
            {
                return Err(
                    "CUDA memory budget must be 1..65536 MiB and wall timeout 1..600000 ms".into(),
                );
            }
        }
        match self.target.kind.as_str() {
            "fixture" => {
                if self.target.binary.is_some() {
                    return Err("fixture target cannot configure binary options".into());
                }
                if gremlin_core::fixture_signature(&self.target.name)? != self.signature() {
                    return Err("fixture signature mismatch".into());
                }
            }
            "binary" => self
                .target
                .binary
                .as_ref()
                .ok_or("binary target requires [target.binary]")?
                .validate()?,
            _ => return Err("target kind must be fixture or binary".into()),
        }
        if let Some(r) = &self.refinement {
            if r.max_rounds == 0 || r.differential_cases == 0 || r.differential_cases > 1_000_000 {
                return Err(
                    "refinement requires positive max_rounds and 1..1000000 differential_cases"
                        .into(),
                );
            }
            for input in &r.initial_inputs {
                gremlin_core::decode_input(&self.signature(), input)?;
            }
        }
        let s = &self.search;
        if s.population < 2
            || s.generations == 0
            || s.max_instructions == 0
            || s.max_steps == 0
            || s.elite == 0
            || s.elite >= s.population
            || s.tournament_size == 0
            || s.tournament_size > s.population
        {
            return Err("invalid search limits: positive limits, 1 <= elite < population, 1 <= tournament_size <= population required".into());
        }
        if s.max_blocks == 0
            || s.max_blocks > 64
            || s.structural_mutation_percent > 100
            || s.loop_bound > 255
            || (s.structural_mutation_percent > 0 && s.max_blocks < 3)
        {
            return Err("invalid structural limits: 1..64 blocks, 0..100 mutation percent, loop_bound <=255".into());
        }
        if s.enumeration_depth > 4
            || (s.enumeration_depth == 0) != (s.enumeration_proposals == 0)
            || s.enumeration_proposals > s.population - s.elite
        {
            return Err(
                "enumeration requires depth 1..4 and proposals 1..population-elite, or both zero"
                    .into(),
            );
        }
        if s.constants.len() > 256 || s.operators.len() > Op::ALL.len() {
            return Err("operator/constant pool exceeds supported limits".into());
        }
        if s.population > 100_000
            || s.max_instructions > 4096
            || self.corpus.random_cases > 1_000_000
            || self.corpus.holdout_cases > 1_000_000
        {
            return Err("configuration exceeds supported resource limits".into());
        }
        if self.output.directory.is_empty() {
            return Err("output directory cannot be empty".into());
        }
        for c in &s.constants {
            Value::parse(c)?;
        }
        if self.target.arguments.is_empty()
            && s.constants
                .iter()
                .all(|c| Value::parse(c).unwrap().ty != self.target.return_type)
        {
            return Err("zero-argument search requires a return-typed constant".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_configuration() {
        let s = include_str!("../../../examples/affine.toml");
        assert!(Config::parse(s).is_ok());
        assert!(Config::parse(&format!("unknown=1\n{s}")).is_err());
        assert!(Config::parse(&s.replace("population = 256", "population = 0")).is_err());
        assert!(Config::parse(&s.replace("kind = \"fixture\"", "kind = \"binary\"")).is_err());
        assert!(
            Config::parse(&s.replace("arguments = [\"u64\"]", "arguments = [\"u8\"]")).is_err()
        );
    }
}
