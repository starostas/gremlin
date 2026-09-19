use crate::{
    import::{ImportDocument, ImportedCase},
    input,
    oracle::Oracle,
    verify::{flags, required},
    Error,
};
use gremlin_core::*;
use gremlin_search::Config;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    tool: String,
    license: String,
    coverage_scope: String,
    compiler: String,
    compiler_hash: String,
    compiler_version: String,
    runtime_archive: String,
    runtime_hash: String,
    compile_flags: Vec<String>,
    engine_hash: String,
    harness_hash: String,
    worker: String,
    worker_hash: String,
    config: Config,
    candidate_hash: Option<String>,
    oracle: serde_json::Value,
    seed: u32,
    runs: u64,
    seconds: u64,
    input_bytes: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    status: String,
    worker_pid: Option<u32>,
    worker_start: Option<String>,
    fuzzer_pid: Option<u32>,
    fuzzer_start: Option<String>,
    exit_code: Option<i32>,
    detail: Option<String>,
}
impl State {
    fn created() -> Self {
        Self {
            status: "created".into(),
            worker_pid: None,
            worker_start: None,
            fuzzer_pid: None,
            fuzzer_start: None,
            exit_code: None,
            detail: None,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observed {
    case: ImportedCase,
    mismatch: bool,
    candidate: Option<Execution>,
    oracle_identity_hash: String,
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn write(path: &Path, data: &impl Serialize) -> Result<(), String> {
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}
fn fingerprint(path: &Path) -> Result<String, String> {
    Ok(hash(&fs::read(path).map_err(|e| e.to_string())?))
}
fn manifest(directory: &Path) -> Result<Manifest, String> {
    let path = directory.join("campaign.json");
    if fingerprint(&path)?
        != fs::read_to_string(directory.join("campaign.sha256")).map_err(|e| e.to_string())?
    {
        return Err("campaign manifest integrity mismatch".into());
    }
    let m: Manifest = read(&path)?;
    if m.schema_version != 1 || m.tool != "libfuzzer-18" {
        return Err("unsupported campaign schema/tool".into());
    }
    Ok(m)
}
fn ticks(pid: u32) -> Result<String, String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|e| e.to_string())?;
    stat[stat.rfind(')').ok_or("invalid process stat")? + 1..]
        .split_whitespace()
        .nth(19)
        .map(String::from)
        .ok_or("missing process start time".into())
}
fn same_process(pid: Option<u32>, start: &Option<String>, expected: &str) -> bool {
    let (Some(pid), Some(start)) = (pid, start) else {
        return false;
    };
    ticks(pid).as_ref() == Ok(start)
        && fingerprint(Path::new(&format!("/proc/{pid}/exe"))).as_deref() == Ok(expected)
}
fn observations(directory: &Path) -> Result<Vec<Observed>, String> {
    let mut paths = fs::read_dir(directory.join("observed"))
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    paths.sort();
    if paths.len() > 20000 {
        return Err("campaign observation limit exceeded".into());
    }
    paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| read(&p))
        .collect()
}
fn create(args: &[String]) -> Result<i32, Error> {
    let f = flags(
        args,
        &[
            "--tool",
            "--config",
            "--candidate",
            "--compiler",
            "--seed",
            "--runs",
            "--seconds",
            "--output",
        ],
    )?;
    if required(&f, "--tool")? != "libfuzzer" {
        return Err(input("supported campaign tool: libfuzzer (Clang 18)"));
    }
    let mut config = Config::parse(&fs::read_to_string(required(&f, "--config")?).map_err(input)?)
        .map_err(input)?;
    if let Some(binary) = &mut config.target.binary {
        binary.path = fs::canonicalize(&binary.path)
            .map_err(input)?
            .to_string_lossy()
            .into_owned();
    }
    let seed: u32 = required(&f, "--seed")?.parse().map_err(input)?;
    let runs: u64 = required(&f, "--runs")?.parse().map_err(input)?;
    let seconds: u64 = required(&f, "--seconds")?.parse().map_err(input)?;
    if seed == 0 || runs == 0 || runs > 10000 || seconds == 0 || seconds > 3600 {
        return Err(input(
            "campaign requires seed>0, runs 1..10000 and seconds 1..3600",
        ));
    }
    let compiler = fs::canonicalize(required(&f, "--compiler")?)
        .map_err(|e| (4, format!("fuzzer compiler unavailable: {e}")))?;
    let version = gremlin_codegen::artifact::run_compiler(&compiler, &["--version".into()], 30000)
        .map_err(|e| (4, e))?;
    if !version.contains("clang version 18.") {
        return Err(input("libFuzzer adapter requires Clang 18"));
    }
    let runtime_dir =
        gremlin_codegen::artifact::run_compiler(&compiler, &["--print-runtime-dir".into()], 30000)
            .map_err(|e| (4, e))?;
    let runtime = Path::new(runtime_dir.trim()).join("libclang_rt.fuzzer-x86_64.a");
    let runtime_hash =
        fingerprint(&runtime).map_err(|e| (4, format!("libFuzzer runtime unavailable: {e}")))?;
    let oracle = Oracle::new(&config).map_err(|e| (4, e))?;
    let candidate = if let Some(path) = f.get("--candidate") {
        let source = fs::read_to_string(path).map_err(input)?;
        let function = parse(&source).map_err(input)?;
        if function.signature() != config.signature() || !function.callees.is_empty() {
            return Err(input(
                "fuzz candidate signature mismatch or unsupported calls",
            ));
        }
        Some((source, hash(&function.canonical_bytes().map_err(input)?)))
    } else {
        None
    };
    let directory = Path::new(required(&f, "--output")?);
    if directory.exists() {
        return Err(input("campaign directory already exists"));
    }
    fs::create_dir_all(directory).map_err(input)?;
    let directory = fs::canonicalize(directory).map_err(input)?;
    for name in ["corpus", "observed", "artifacts"] {
        fs::create_dir(directory.join(name)).map_err(input)?;
    }
    let source = directory.join("harness.cc");
    fs::write(&source, include_str!("harness.cc")).map_err(input)?;
    let engine = directory.join("fuzzer");
    let compile_flags: Vec<String> = [
        "-x",
        "c++",
        "-std=c++17",
        "-O1",
        "-g",
        "-fsanitize=fuzzer",
        "-o",
    ]
    .into_iter()
    .map(String::from)
    .chain([
        engine.to_string_lossy().into_owned(),
        source.to_string_lossy().into_owned(),
    ])
    .collect();
    gremlin_codegen::artifact::run_compiler(&compiler, &compile_flags, 30000)
        .map_err(|e| (4, format!("libFuzzer harness unavailable: {e}")))?;
    let input_bytes = config
        .signature()
        .arguments
        .iter()
        .map(|t| (t.width() / 8) as usize)
        .sum();
    fs::write(directory.join("corpus/zero"), vec![0u8; input_bytes]).map_err(input)?;
    fs::write(directory.join("corpus/ones"), vec![255u8; input_bytes]).map_err(input)?;
    let worker = fs::canonicalize(std::env::current_exe().map_err(input)?).map_err(input)?;
    if let Some((source, _)) = &candidate {
        fs::write(directory.join("candidate.gremlin"), source).map_err(input)?;
    }
    let m = Manifest {
        schema_version: 1,
        tool: "libfuzzer-18".into(),
        license: "Apache-2.0 WITH LLVM-exception".into(),
        coverage_scope: "harness only; no target coverage for isolated uninstrumented ELF".into(),
        compiler: compiler.to_string_lossy().into_owned(),
        compiler_hash: fingerprint(&compiler).map_err(input)?,
        compiler_version: version.trim().into(),
        runtime_archive: runtime.to_string_lossy().into_owned(),
        runtime_hash,
        compile_flags,
        engine_hash: fingerprint(&engine).map_err(input)?,
        harness_hash: fingerprint(&source).map_err(input)?,
        worker: worker.to_string_lossy().into_owned(),
        worker_hash: fingerprint(&worker).map_err(input)?,
        config,
        candidate_hash: candidate.map(|(_, h)| h),
        oracle: oracle.details(),
        seed,
        runs,
        seconds,
        input_bytes,
    };
    write(&directory.join("campaign.json"), &m).map_err(input)?;
    fs::write(
        directory.join("campaign.sha256"),
        fingerprint(&directory.join("campaign.json")).map_err(input)?,
    )
    .map_err(input)?;
    write(&directory.join("state.json"), &State::created()).map_err(input)?;
    println!(
        "{}",
        serde_json::json!({"status":"created","campaign":directory,"tool":m.tool,"coverage_scope":m.coverage_scope})
    );
    Ok(0)
}
pub fn command(args: &[String]) -> Result<i32, Error> {
    let action = args
        .get(1)
        .ok_or_else(|| input("campaign requires create/start/stop/poll/export"))?;
    if action == "create" {
        return create(&args[1..]);
    }
    let f = flags(
        &args[1..],
        if action == "export" {
            &["--directory", "--output"]
        } else {
            &["--directory"]
        },
    )?;
    let directory = fs::canonicalize(required(&f, "--directory")?).map_err(input)?;
    let m = manifest(&directory).map_err(input)?;
    match action.as_str() {
        "start" => {
            if fingerprint(Path::new(&m.worker)).map_err(input)? != m.worker_hash
                || fingerprint(&directory.join("fuzzer")).map_err(input)? != m.engine_hash
                || fingerprint(&directory.join("harness.cc")).map_err(input)? != m.harness_hash
            {
                return Err(input("campaign executable identity changed"));
            }
            let _started = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(".started"))
                .map_err(|e| input(format!("campaign is single-use or cannot start: {e}")))?;
            let log = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(directory.join("supervisor.log"))
                .map_err(input)?;
            let mut child = Command::new(&m.worker)
                .args(["--campaign-worker", directory.to_str().unwrap()])
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::from(log.try_clone().map_err(input)?))
                .stderr(Stdio::from(log))
                .spawn()
                .map_err(input)?;
            let started = Instant::now();
            loop {
                let state: State = read(&directory.join("state.json")).map_err(input)?;
                if state.status != "created" {
                    println!("{}", serde_json::to_string(&state).map_err(input)?);
                    return Ok(0);
                }
                if let Some(status) = child.try_wait().map_err(input)? {
                    return Err((
                        4,
                        format!("campaign supervisor failed before start: {status}"),
                    ));
                }
                if started.elapsed() > Duration::from_secs(5) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err((4, "campaign supervisor start timeout".into()));
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
        "poll" => {
            let mut state: State = read(&directory.join("state.json")).map_err(input)?;
            if state.status == "running"
                && !same_process(state.worker_pid, &state.worker_start, &m.worker_hash)
            {
                state = read(&directory.join("state.json")).map_err(input)?;
                if state.status == "running" {
                    state.status = "failed".into();
                    state.detail =
                        Some("supervisor no longer exists with recorded identity".into());
                    write(&directory.join("state.json"), &state).map_err(input)?;
                }
            }
            let observed = observations(&directory).map_err(input)?;
            println!(
                "{}",
                serde_json::json!({"state":state,"observations":observed.len(),"counterexamples":observed.iter().filter(|o|o.mismatch).count(),"coverage_scope":m.coverage_scope})
            );
            Ok(0)
        }
        "stop" => {
            let mut state: State = read(&directory.join("state.json")).map_err(input)?;
            if state.status == "running" {
                if !same_process(state.worker_pid, &state.worker_start, &m.worker_hash) {
                    return Err(input("refusing to signal a changed supervisor identity"));
                }
                let fuzzer_alive =
                    same_process(state.fuzzer_pid, &state.fuzzer_start, &m.engine_hash);
                unsafe {
                    libc::kill(state.worker_pid.unwrap() as i32, libc::SIGTERM);
                    if fuzzer_alive {
                        libc::kill(-(state.fuzzer_pid.unwrap() as i32), libc::SIGKILL);
                    }
                }
                state.status = "stopped".into();
                state.detail = Some("explicit campaign stop".into());
                write(&directory.join("state.json"), &state).map_err(input)?;
            }
            println!("{}", serde_json::to_string(&state).map_err(input)?);
            Ok(0)
        }
        "export" => {
            let output = Path::new(required(&f, "--output")?);
            let observations = observations(&directory).map_err(input)?;
            let document = ImportDocument {
                schema_version: 1,
                signature: m.config.signature(),
                cases: observations.into_iter().map(|o| o.case).collect(),
            };
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)
                .map_err(input)?;
            serde_json::to_writer_pretty(file, &document).map_err(input)?;
            println!(
                "{}",
                serde_json::json!({"status":"exported","cases":document.cases.len(),"output":output})
            );
            Ok(0)
        }
        _ => Err(input("unknown campaign action")),
    }
}
pub fn worker(directory: &Path) -> Result<(), String> {
    let m = manifest(directory)?;
    if fingerprint(&std::env::current_exe().map_err(|e| e.to_string())?)? != m.worker_hash
        || fingerprint(&directory.join("fuzzer"))? != m.engine_hash
    {
        return Err("campaign worker/tool identity mismatch".into());
    }
    let log = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.join("fuzzer.log"))
        .map_err(|e| e.to_string())?;
    let arguments = vec![
        directory.join("corpus").to_string_lossy().into_owned(),
        format!("-runs={}", m.runs),
        format!("-max_total_time={}", m.seconds),
        format!("-seed={}", m.seed),
        format!("-max_len={}", m.input_bytes.max(1)),
        "-rss_limit_mb=512".into(),
        "-timeout=120".into(),
        "-error_exitcode=77".into(),
        format!(
            "-artifact_prefix={}/",
            directory.join("artifacts").display()
        ),
    ];
    write(&directory.join("invocation.json"), &arguments)?;
    let mut child = Command::new(directory.join("fuzzer"))
        .args(&arguments)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GREMLIN_WORKER", &m.worker)
        .env("GREMLIN_CAMPAIGN", directory)
        .env("GREMLIN_INPUT_BYTES", m.input_bytes.to_string())
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut state = State {
        status: "running".into(),
        worker_pid: Some(std::process::id()),
        worker_start: Some(ticks(std::process::id())?),
        fuzzer_pid: Some(child.id()),
        fuzzer_start: Some(ticks(child.id())?),
        exit_code: None,
        detail: None,
    };
    write(&directory.join("state.json"), &state)?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(m.seconds + 10) {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            state.detail = Some("campaign supervisor wall deadline".into());
            break child.wait().map_err(|e| e.to_string())?;
        }
        thread::sleep(Duration::from_millis(20));
    };
    state.exit_code = status.code();
    let has_counterexample = observations(directory)?.iter().any(|o| o.mismatch);
    state.status = if status.success() {
        "completed"
    } else if status.code() == Some(77) && has_counterexample {
        "counterexample"
    } else {
        "failed"
    }
    .into();
    write(&directory.join("state.json"), &state)?;
    Ok(())
}
pub fn observe(directory: &Path, hex: &str) -> Result<i32, String> {
    let m = manifest(directory)?;
    if hex.len() != m.input_bytes * 2 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("fuzzer input width/encoding mismatch".into());
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut offset = 0;
    let mut input = Vec::new();
    for ty in &m.config.signature().arguments {
        let width = (ty.width() / 8) as usize;
        let mut raw = [0u8; 8];
        raw[..width].copy_from_slice(&bytes[offset..offset + width]);
        input.push(Value::new(*ty, u64::from_le_bytes(raw)));
        offset += width;
    }
    if fingerprint(&std::env::current_exe().map_err(|e| e.to_string())?)? != m.worker_hash {
        return Err("campaign observation worker changed".into());
    }
    let oracle = Oracle::new(&m.config)?;
    if oracle.details() != m.oracle {
        return Err("campaign oracle identity changed".into());
    }
    let expected = oracle.observe(&[input.clone()])?[0];
    let candidate = if let Some(expected_hash) = &m.candidate_hash {
        let source =
            fs::read_to_string(directory.join("candidate.gremlin")).map_err(|e| e.to_string())?;
        let f = parse(&source)?;
        if hash(&f.canonical_bytes()?) != *expected_hash {
            return Err("campaign candidate changed".into());
        }
        Some(execute(&f, &input, m.config.search.max_steps))
    } else {
        None
    };
    let mismatch = candidate
        .as_ref()
        .is_some_and(|e| e.outcome != Outcome::Completed(expected));
    let observation = Observed {
        case: ImportedCase {
            input: input.iter().map(|v| v.hex()).collect(),
            expected: Some(expected.hex()),
            provenance: vec![
                format!(
                    "libfuzzer-18 campaign {}",
                    fingerprint(&directory.join("campaign.json"))?
                ),
                format!(
                    "seed {}; target coverage unavailable; candidate mismatch {mismatch}",
                    m.seed
                ),
            ],
        },
        mismatch,
        candidate,
        oracle_identity_hash: object_hash(&m.oracle),
    };
    let filename = format!("{}.json", hash(&bytes));
    let path = directory.join("observed").join(filename);
    if path.exists() {
        let previous: Observed = read(&path)?;
        if previous.case.expected != observation.case.expected {
            return Err("campaign observation nondeterminism".into());
        }
    } else {
        write(&path, &observation)?;
    }
    Ok(if mismatch { 86 } else { 0 })
}
