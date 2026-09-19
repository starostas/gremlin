use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIdentity {
    pub name: String,
    pub signature: Signature,
    pub contract: String,
    pub implementation_fingerprint: String,
}
impl TargetIdentity {
    pub fn fixture(name: &str) -> Result<Self, String> {
        Ok(Self {
            name: name.into(),
            signature: fixture_signature(name)?,
            contract: "deterministic pure total integer return; fixture".into(),
            implementation_fingerprint: fixture_fingerprint(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub input: Vec<String>,
    pub expected: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub input: Vec<String>,
    pub records: BTreeSet<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub schema_version: u32,
    pub target: TargetIdentity,
    pub cases: Vec<Case>,
    #[serde(skip)]
    pub provenance: Vec<Provenance>,
}
impl Corpus {
    pub fn new(target: TargetIdentity) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            target,
            cases: vec![],
            provenance: vec![],
        }
    }
    pub fn add(
        &mut self,
        input: Vec<String>,
        expected: String,
        record: String,
    ) -> Result<(), String> {
        let args = decode_input(&self.target.signature, &input)?;
        let input: Vec<_> = args.iter().map(|v| v.hex()).collect();
        let expected = Value::from_hex(self.target.signature.return_type, &expected)?.hex();
        match self.cases.binary_search_by(|c| c.input.cmp(&input)) {
            Ok(i) => {
                if self.cases[i].expected != expected {
                    return Err("conflicting outputs for the same input".into());
                }
            }
            Err(i) => self.cases.insert(
                i,
                Case {
                    input: input.clone(),
                    expected,
                },
            ),
        }
        match self.provenance.binary_search_by(|p| p.input.cmp(&input)) {
            Ok(i) => {
                self.provenance[i].records.insert(record);
            }
            Err(i) => self.provenance.insert(
                i,
                Provenance {
                    input,
                    records: BTreeSet::from([record]),
                },
            ),
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("unsupported corpus schema".into());
        }
        self.target.signature.validate()?;
        if self.cases.is_empty() {
            return Err("empty corpus".into());
        }
        let mut previous = None;
        for c in &self.cases {
            let input = decode_input(&self.target.signature, &c.input)?;
            if input.iter().map(|v| v.hex()).collect::<Vec<_>>() != c.input
                || Value::from_hex(self.target.signature.return_type, &c.expected)?.hex()
                    != c.expected
            {
                return Err("corpus must use normalized lowercase hexadecimal".into());
            }
            if previous.is_some_and(|p: &Vec<String>| p >= &c.input) {
                return Err("corpus must have sorted unique inputs".into());
            }
            previous = Some(&c.input);
        }
        Ok(())
    }
    pub fn content_hash(&self) -> Result<String, String> {
        self.validate()?;
        let header =
            serde_json::to_vec(&(self.schema_version, &self.target)).map_err(|e| e.to_string())?;
        let mut data = b"gremlin-corpus-content-v1\0".to_vec();
        data.extend_from_slice(&(header.len() as u64).to_le_bytes());
        data.extend_from_slice(&header);
        data.extend_from_slice(&(self.cases.len() as u64).to_le_bytes());
        for case in &self.cases {
            data.extend_from_slice(&encode_transport(
                &self.target.signature,
                &decode_input(&self.target.signature, &case.input)?,
            )?);
            let result = Value::from_hex(self.target.signature.return_type, &case.expected)?;
            data.extend_from_slice(&encode_result(self.target.signature.return_type, result)?);
        }
        Ok(hash(&data))
    }
}
pub fn decode_input(s: &Signature, input: &[String]) -> Result<Vec<Value>, String> {
    if input.len() != s.arguments.len() {
        return Err("input argument count mismatch".into());
    }
    input
        .iter()
        .zip(&s.arguments)
        .map(|(v, t)| Value::from_hex(*t, v))
        .collect()
}
pub fn encode_transport(s: &Signature, args: &[Value]) -> Result<Vec<u8>, String> {
    s.validate()?;
    if args.len() != s.arguments.len() {
        return Err("transport argument count mismatch".into());
    }
    let mut out = vec![];
    for (v, t) in args.iter().zip(&s.arguments) {
        if v.ty != *t || v.bits > t.mask() {
            return Err("transport type/width mismatch".into());
        }
        out.extend_from_slice(&v.bits.to_le_bytes()[..(t.width() / 8) as usize]);
    }
    Ok(out)
}
pub fn encode_result(ty: Type, value: Value) -> Result<Vec<u8>, String> {
    if !ty.integer() || value.ty != ty || value.bits > ty.mask() {
        return Err("result transport type/width mismatch".into());
    }
    Ok(value.bits.to_le_bytes()[..(ty.width() / 8) as usize].to_vec())
}
pub fn boundary_patterns(t: Type) -> Vec<u64> {
    let mut set = BTreeSet::from([
        0,
        1,
        t.mask(),
        t.mask() - 1,
        1u64 << (t.width() - 1),
        (1u64 << (t.width() - 1)) - 1,
        0xaaaaaaaaaaaaaaaa & t.mask(),
        0x5555555555555555 & t.mask(),
    ]);
    for i in 0..t.width() {
        let p = 1u64 << i;
        set.insert(p);
        set.insert(p - 1);
        set.insert(p.wrapping_add(1) & t.mask());
    }
    set.into_iter().collect()
}
pub fn seed_inputs(s: &Signature, seed: u64, random: usize) -> Vec<(Vec<Value>, String)> {
    let mut result = vec![];
    let zero: Vec<_> = s.arguments.iter().map(|t| Value::new(*t, 0)).collect();
    result.push((zero.clone(), "boundary:zero".into()));
    let patterns: Vec<_> = s.arguments.iter().map(|t| boundary_patterns(*t)).collect();
    for (i, ps) in patterns.iter().enumerate() {
        for n in ps {
            let mut input = zero.clone();
            input[i] = Value::new(s.arguments[i], *n);
            result.push((input, format!("boundary:argument:{i}")));
        }
    }
    let count = patterns.iter().map(Vec::len).max().unwrap_or(0);
    for i in 0..count {
        for stagger in [0, 1] {
            result.push((
                s.arguments
                    .iter()
                    .enumerate()
                    .map(|(j, t)| {
                        Value::new(*t, patterns[j][(i + j * stagger) % patterns[j].len()])
                    })
                    .collect(),
                format!("boundary:interaction:{stagger}"),
            ));
        }
    }
    let mut rng = Rng::new(seed);
    for i in 0..random {
        result.push((
            s.arguments
                .iter()
                .map(|t| Value::new(*t, rng.next_u64()))
                .collect(),
            format!("random:seed:{seed}:case:{i}"),
        ));
    }
    result
}
pub fn fixture_corpus(name: &str, seed: u64, random: usize) -> Result<Corpus, String> {
    let target = TargetIdentity::fixture(name)?;
    let mut c = Corpus::new(target);
    for (args, record) in seed_inputs(&c.target.signature, seed, random) {
        let out = fixture_observe(name, &args)?;
        c.add(args.iter().map(|v| v.hex()).collect(), out.hex(), record)?;
    }
    c.validate()?;
    Ok(c)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldoutInputs {
    pub seed: u64,
    pub requested: usize,
    pub domain_exhausted: bool,
    pub inputs: Vec<Vec<Value>>,
}
pub fn holdout_inputs(
    s: &Signature,
    corpus: &Corpus,
    seed: u64,
    requested: usize,
) -> HoldoutInputs {
    let bits: u32 = s.arguments.iter().map(|t| t.width()).sum();
    let mut seen: BTreeSet<Vec<String>> = corpus.cases.iter().map(|c| c.input.clone()).collect();
    let domain = if bits < 64 { Some(1u64 << bits) } else { None };
    let mut rng = Rng::new(seed);
    let mut inputs = vec![];
    let mut cursor = 0u64;
    let (offset, stride) = if bits <= 16 {
        (rng.next_u64(), rng.next_u64() | 1)
    } else {
        (0, 1)
    };
    while inputs.len() < requested && domain.is_none_or(|n| (seen.len() as u64) < n) {
        let args: Vec<Value> = if bits <= 16 {
            let n = offset.wrapping_add(cursor.wrapping_mul(stride));
            cursor += 1;
            let mut shift = 0;
            s.arguments
                .iter()
                .map(|t| {
                    let v = Value::new(*t, n >> shift);
                    shift += t.width();
                    v
                })
                .collect()
        } else {
            s.arguments
                .iter()
                .map(|t| Value::new(*t, rng.next_u64()))
                .collect()
        };
        let key = args.iter().map(|v| v.hex()).collect();
        if seen.insert(key) {
            inputs.push(args);
        }
    }
    HoldoutInputs {
        seed,
        requested,
        domain_exhausted: domain.is_some_and(|n| seen.len() as u64 == n),
        inputs,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dedup_and_integrity() {
        let mut c = fixture_corpus("identity_u64", 1, 2).unwrap();
        let h = c.content_hash().unwrap();
        c.add(
            vec!["0x0000000000000000".into()],
            "0x0000000000000000".into(),
            "import:extra".into(),
        )
        .unwrap();
        assert_eq!(h, c.content_hash().unwrap());
        assert!(c.provenance[0].records.contains("import:extra"));
        assert!(c
            .add(
                vec!["0x0000000000000000".into()],
                "0x0000000000000001".into(),
                "conflict".into()
            )
            .is_err());
        assert!(c
            .add(
                vec!["0x0".into()],
                "0x0000000000000000".into(),
                "bad".into()
            )
            .is_err());
    }
    #[test]
    fn finite_holdout_excludes_corpus() {
        let s = Signature {
            arguments: vec![Type::U8],
            return_type: Type::U8,
        };
        let mut c = Corpus::new(TargetIdentity {
            name: "test".into(),
            signature: s.clone(),
            contract: "test".into(),
            implementation_fingerprint: "test".into(),
        });
        c.add(vec!["0x00".into()], "0x00".into(), "test".into())
            .unwrap();
        let h = holdout_inputs(&s, &c, 3, 300);
        assert_eq!(h.inputs.len(), 255);
        assert!(h.domain_exhausted);
        assert!(h.inputs.iter().all(|a| a[0].bits != 0));
    }
    #[test]
    fn transport_encoding() {
        let s = Signature {
            arguments: vec![Type::I16, Type::U8],
            return_type: Type::U8,
        };
        assert_eq!(
            encode_transport(
                &s,
                &[Value::new(Type::I16, 0xfffe), Value::new(Type::U8, 3)]
            )
            .unwrap(),
            vec![254, 255, 3]
        );
        assert!(Value::from_hex(Type::U8, "0x0001").is_err());
    }
}
