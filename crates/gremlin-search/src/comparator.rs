use gremlin_core::{parse, Evaluator, Function, Signature, Type};
use serde::{Deserialize, Serialize};

/// Selection guidance only. Exact observations still determine correctness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComparatorConfig {
    CorrectnessFirst {},
    BitErrorFirst {},
    Gremlin { source: String, max_steps: u64 },
}

impl Default for ComparatorConfig {
    fn default() -> Self {
        Self::CorrectnessFirst {}
    }
}

impl ComparatorConfig {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::CorrectnessFirst { .. })
    }

    pub fn program(&self) -> Result<Option<Function>, String> {
        let Self::Gremlin { source, max_steps } = self else {
            return Ok(None);
        };
        if source.len() > 65536 || !(1..=10000).contains(max_steps) {
            return Err("comparator requires source <=64 KiB and max_steps 1..10000".into());
        }
        let f = parse(source).map_err(|e| format!("comparator source: {e}"))?;
        if f.signature()
            != (Signature {
                arguments: vec![Type::U64, Type::U64],
                return_type: Type::U64,
            })
        {
            return Err("comparator signature must be (actual:u64, expected:u64)->u64".into());
        }
        if !f.callees.is_empty()
            || f.blocks.len() > 64
            || f.blocks.iter().map(|b| b.instructions.len()).sum::<usize>() > 4096
        {
            return Err("comparator requires no calls, <=64 blocks and <=4096 instructions".into());
        }
        Evaluator::new(&f).map_err(|e| format!("comparator: {e}"))?;
        Ok(Some(f))
    }
}
