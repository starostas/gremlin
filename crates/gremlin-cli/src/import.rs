use crate::{
    input,
    oracle::Oracle,
    verify::{flags, required},
    Error,
};
use gremlin_core::*;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedCase {
    pub input: Vec<String>,
    #[serde(default)]
    pub expected: Option<String>,
    pub provenance: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportDocument {
    pub schema_version: u32,
    pub signature: Signature,
    pub cases: Vec<ImportedCase>,
}
pub fn stored(path: &Path) -> Result<(Corpus, Vec<Provenance>), String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    if data.len() > 16 * 1024 * 1024 {
        return Err("corpus file exceeds 16 MiB".into());
    }
    let corpus: Corpus = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    corpus.validate()?;
    if corpus.cases.len() > 65536 {
        return Err("corpus exceeds case limit".into());
    }
    let provenance_path = path.with_file_name("provenance.json");
    let provenance = if provenance_path.exists() {
        let provenance: Vec<Provenance> =
            serde_json::from_slice(&fs::read(provenance_path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if provenance.len() != corpus.cases.len()
            || provenance
                .iter()
                .zip(&corpus.cases)
                .any(|(p, c)| p.input != c.input || p.records.is_empty())
        {
            return Err("stored corpus provenance mismatch".into());
        }
        provenance
    } else {
        vec![]
    };
    Ok((corpus, provenance))
}
pub fn merge_stored(oracle: &Oracle, corpus: &mut Corpus, path: &Path) -> Result<(), String> {
    let (old, provenance) = stored(path)?;
    if old.target.signature != corpus.target.signature {
        return Err("initial corpus signature mismatch".into());
    }
    let origin = hash(&fs::read(path).map_err(|e| e.to_string())?);
    let inputs = old
        .cases
        .iter()
        .map(|c| decode_input(&corpus.target.signature, &c.input))
        .collect::<Result<Vec<_>, _>>()?;
    let observed = oracle.observe(&inputs)?;
    for (n, (case, value)) in old.cases.iter().zip(observed).enumerate() {
        if value.hex() != case.expected {
            return Err("stored corpus label disagrees with fresh oracle replay".into());
        }
        corpus.add(
            case.input.clone(),
            value.hex(),
            format!("replayed stored corpus {origin}"),
        )?;
        if let Some(p) = provenance.get(n) {
            for record in &p.records {
                corpus.add(case.input.clone(), value.hex(), record.clone())?;
            }
        }
    }
    Ok(())
}
pub fn replay(
    oracle: &Oracle,
    corpus: &mut Corpus,
    document: &ImportDocument,
    origin: &str,
) -> Result<(), String> {
    if document.schema_version != 1
        || document.signature != corpus.target.signature
        || document.cases.is_empty()
        || document.cases.len() > 65536
    {
        return Err("import schema, signature or case count unsupported".into());
    }
    let inputs = document
        .cases
        .iter()
        .map(|c| {
            if c.provenance.is_empty()
                || c.provenance.len() > 32
                || c.provenance.iter().any(|p| p.is_empty() || p.len() > 1024)
            {
                return Err(
                    "import provenance must contain 1..32 nonempty records up to 1024 bytes".into(),
                );
            }
            if let Some(e) = &c.expected {
                Value::from_hex(document.signature.return_type, e)?;
            }
            decode_input(&document.signature, &c.input)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let values = oracle.observe(&inputs)?;
    for (case, value) in document.cases.iter().zip(values) {
        if let Some(expected) = &case.expected {
            if Value::from_hex(document.signature.return_type, expected)? != value {
                return Err("import expected output disagrees with oracle replay".into());
            }
        }
        corpus.add(
            case.input.clone(),
            value.hex(),
            format!("replayed import v1 {origin}"),
        )?;
        for record in &case.provenance {
            corpus.add(case.input.clone(), value.hex(), record.clone())?;
        }
    }
    Ok(())
}
pub fn command(args: &[String]) -> Result<i32, Error> {
    let flags = flags(args, &["--config", "--input", "--corpus", "--output"])?;
    let config = gremlin_search::Config::parse(
        &fs::read_to_string(required(&flags, "--config")?).map_err(input)?,
    )
    .map_err(input)?;
    let path = Path::new(required(&flags, "--input")?);
    let data = fs::read(path).map_err(input)?;
    if data.len() > 16 * 1024 * 1024 {
        return Err(input("import exceeds 16 MiB"));
    }
    let document: ImportDocument = serde_json::from_slice(&data).map_err(input)?;
    let directory = Path::new(required(&flags, "--output")?);
    if directory.exists() {
        return Err(input("import output already exists"));
    }
    let oracle = Oracle::new(&config).map_err(|e| (4, e))?;
    let mut corpus = Corpus::new(oracle.identity(&config.target.name).map_err(input)?);
    if let Some(path) = flags.get("--corpus") {
        merge_stored(&oracle, &mut corpus, Path::new(path)).map_err(input)?;
    }
    let before = corpus.cases.len();
    replay(&oracle, &mut corpus, &document, &hash(&data)).map_err(input)?;
    corpus.validate().map_err(input)?;
    if let Some(parent) = directory.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(input)?;
    }
    let temporary = directory.with_extension(format!("importtmp-{}", std::process::id()));
    fs::create_dir(&temporary).map_err(input)?;
    for (name, value) in [
        ("corpus.json", serde_json::to_value(&corpus).map_err(input)?),
        (
            "provenance.json",
            serde_json::to_value(&corpus.provenance).map_err(input)?,
        ),
        (
            "report.json",
            serde_json::json!({"schema_version":1,"status":"replayed","import_hash":hash(&data),"corpus_hash":corpus.content_hash().map_err(input)?,"supplied_cases":document.cases.len(),"new_cases":corpus.cases.len()-before,"total_cases":corpus.cases.len(),"oracle":oracle.details(),"evidence_level":null}),
        ),
    ] {
        fs::write(
            temporary.join(name),
            serde_json::to_vec_pretty(&value).map_err(input)?,
        )
        .map_err(input)?;
    }
    if directory.exists() {
        return Err(input("import output appeared during replay"));
    }
    fs::rename(&temporary, directory).map_err(input)?;
    println!(
        "{}",
        serde_json::json!({"status":"replayed","cases":corpus.cases.len(),"corpus":directory.join("corpus.json")})
    );
    Ok(0)
}
