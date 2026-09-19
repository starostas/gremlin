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
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    pub kind: String,
    pub name: String,
    pub arguments: Vec<Type>,
    pub return_type: Type,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchConfig {
    pub population: usize,
    pub generations: usize,
    pub max_instructions: usize,
    pub max_steps: u64,
    pub elite: usize,
    pub tournament_size: usize,
    pub operators: Vec<Op>,
    pub constants: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusConfig {
    pub random_cases: usize,
    pub holdout_cases: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    pub directory: String,
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
        if self.target.kind != "fixture" {
            return Err("only target kind 'fixture' is supported".into());
        }
        self.signature().validate()?;
        if gremlin_core::fixture_signature(&self.target.name)? != self.signature() {
            return Err("fixture signature mismatch".into());
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
