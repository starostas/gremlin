use gremlin_core::*;
use gremlin_native::BinaryOracle;
use gremlin_search::Config;
pub enum Oracle {
    Fixture(String),
    Binary(Box<BinaryOracle>),
}
impl Oracle {
    pub fn new(c: &Config) -> Result<Self, String> {
        match c.target.kind.as_str() {
            "fixture" => Ok(Self::Fixture(c.target.name.clone())),
            "binary" => Ok(Self::Binary(Box::new(BinaryOracle::new(
                c.target.binary.clone().ok_or("missing binary contract")?,
                c.signature(),
                &std::env::current_exe().map_err(|e| e.to_string())?,
            )?))),
            _ => Err("unsupported oracle".into()),
        }
    }
    pub fn identity(&self, name: &str) -> Result<TargetIdentity, String> {
        match self {
            Self::Fixture(n) => TargetIdentity::fixture(n),
            Self::Binary(o) => Ok(o.target_identity(name)),
        }
    }
    pub fn scope(&self) -> &'static str {
        match self {
            Self::Fixture(_) => "fixture",
            Self::Binary(_) => "binary",
        }
    }
    pub fn details(&self) -> serde_json::Value {
        match self {
            Self::Fixture(n) => serde_json::json!({"kind":"fixture","name":n}),
            Self::Binary(o) => serde_json::to_value(o.identity()).unwrap(),
        }
    }
    pub fn observe(&self, args: &[Vec<Value>]) -> Result<Vec<Value>, String> {
        match self {
            Self::Fixture(n) => args.iter().map(|a| fixture_observe(n, a)).collect(),
            Self::Binary(o) => o.observe(args),
        }
    }
    pub fn corpus(&self, c: &Config) -> Result<Corpus, String> {
        let mut corpus = Corpus::new(self.identity(&c.target.name)?);
        let inputs = if let Some(r) = &c.refinement {
            if !r.initial_inputs.is_empty() {
                r.initial_inputs
                    .iter()
                    .map(|input| {
                        Ok((
                            decode_input(&c.signature(), input)?,
                            "explicit initial input".into(),
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?
            } else {
                seed_inputs(&c.signature(), c.seed, c.corpus.random_cases)
            }
        } else {
            seed_inputs(&c.signature(), c.seed, c.corpus.random_cases)
        };
        let args: Vec<_> = inputs.iter().map(|(a, _)| a.clone()).collect();
        let expected = self.observe(&args)?;
        for ((args, provenance), value) in inputs.into_iter().zip(expected) {
            corpus.add(
                args.iter().map(|v| v.hex()).collect(),
                value.hex(),
                provenance,
            )?;
        }
        if let Some(path) = &c.corpus.initial_corpus {
            crate::import::merge_stored(self, &mut corpus, std::path::Path::new(path))?;
        }
        corpus.validate()?;
        Ok(corpus)
    }
}
