use gremlin_core::*;
use gremlin_native::{BinaryIdentity, BinaryOracle};
use serde::Serialize;
use std::{
    fs,
    io::Read,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[derive(Debug, Serialize)]
pub struct Artifact {
    pub candidate_hash: String,
    pub llvm_hash: String,
    pub artifact_hash: String,
    pub path: String,
    pub compiler_path: String,
    pub compiler_hash: String,
    pub compiler_version: String,
    pub flags: Vec<String>,
    pub compile_seconds: f64,
    pub artifact_bytes: u64,
    pub max_steps: u64,
}
pub fn run_compiler(path: &Path, args: &[String], timeout_ms: u64) -> Result<String, String> {
    let mut command = Command::new(path);
    command
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A concurrent fork can briefly retain a just-written executable descriptor.
    let launch = Instant::now();
    let mut child = loop {
        match command.spawn() {
            Ok(child) => break child,
            Err(e)
                if e.raw_os_error() == Some(libc::ETXTBSY)
                    && launch.elapsed() < Duration::from_millis(100) =>
            {
                thread::sleep(Duration::from_millis(2))
            }
            Err(e) => return Err(format!("compiler unavailable: {e}")),
        }
    };
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let read = |mut stream: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            stream
                .by_ref()
                .take(1_048_577)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
    };
    let stdout = read(Box::new(out));
    let stderr = read(Box::new(err));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(e) => break Err(e.to_string()),
            _ => {}
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            break Err("compiler wall timeout".into());
        }
        thread::sleep(Duration::from_millis(2));
    };
    if status.is_err() {
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.wait();
    }
    let stdout = stdout
        .join()
        .map_err(|_| "compiler reader panicked")?
        .map_err(|e| e.to_string())?;
    let stderr = stderr
        .join()
        .map_err(|_| "compiler reader panicked")?
        .map_err(|e| e.to_string())?;
    let status = status?;
    if stdout.len() > 1_048_576 || stderr.len() > 1_048_576 {
        return Err("compiler output exceeds limit".into());
    }
    if !status.success() {
        return Err(format!(
            "compiler failed {status}: {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}
pub fn compile(
    f: &Function,
    max_steps: u64,
    compiler: &Path,
    directory: &Path,
    timeout_ms: u64,
) -> Result<Artifact, String> {
    if timeout_ms == 0 || timeout_ms > 600000 {
        return Err("compiler timeout must be 1..600000 ms".into());
    }
    let start = Instant::now();
    let llvm = crate::lower(f, max_steps)?;
    let ir = directory.join("candidate.ll");
    let output = directory.join("candidate.so");
    if ir.exists() || output.exists() {
        return Err("LLVM/artifact output already exists".into());
    }
    let compiler_hash =
        hash(&fs::read(compiler).map_err(|e| format!("compiler unavailable: {e}"))?);
    let version = run_compiler(compiler, &["--version".into()], timeout_ms)?;
    if !version.contains("clang version 18.") {
        return Err("LLVM backend requires Clang 18".into());
    }
    fs::write(&ir, &llvm).map_err(|e| e.to_string())?;
    let flags: Vec<String> = [
        "-x",
        "ir",
        "-O2",
        "-shared",
        "-nostdlib",
        "-fPIC",
        "-Wl,-z,noexecstack",
        "-target",
        "x86_64-unknown-linux-gnu",
        "-o",
    ]
    .into_iter()
    .map(String::from)
    .chain([
        output.to_string_lossy().into_owned(),
        ir.to_string_lossy().into_owned(),
    ])
    .collect();
    run_compiler(compiler, &flags, timeout_ms)?;
    if hash(&fs::read(compiler).map_err(|e| e.to_string())?) != compiler_hash {
        return Err("compiler changed during compilation".into());
    }
    let binary = fs::read(&output).map_err(|e| e.to_string())?;
    Ok(Artifact {
        candidate_hash: hash(&f.canonical_bytes()?),
        llvm_hash: hash(llvm.as_bytes()),
        artifact_hash: hash(&binary),
        path: output.to_string_lossy().into_owned(),
        compiler_path: compiler.to_string_lossy().into_owned(),
        compiler_hash,
        compiler_version: version.trim().into(),
        flags,
        compile_seconds: start.elapsed().as_secs_f64(),
        artifact_bytes: binary.len() as u64,
        max_steps,
    })
}
#[derive(Debug, Serialize)]
pub struct NativeValidation {
    pub outcomes: Vec<Outcome>,
    pub value_identity: BinaryIdentity,
    pub status_identity: BinaryIdentity,
    pub wall_seconds: f64,
}
pub fn observe(
    artifact: &Artifact,
    signature: &Signature,
    inputs: &[Vec<Value>],
    worker: &Path,
) -> Result<NativeValidation, String> {
    let start = Instant::now();
    let contract = BinaryContract {
        path: artifact.path.clone(),
        sha256: artifact.artifact_hash.clone(),
        architecture: "x86_64".into(),
        format: "elf".into(),
        symbol: "gremlin_target".into(),
        abi: "sysv64".into(),
        environment: "empty".into(),
        wall_timeout_ms: 5000,
        cpu_seconds: 2,
        memory_mb: 256,
    };
    let value = BinaryOracle::new(contract.clone(), signature.clone(), worker)?;
    let mut status_contract = contract;
    status_contract.symbol = "gremlin_status".into();
    let mut status_signature = signature.clone();
    status_signature.return_type = Type::U8;
    let status = BinaryOracle::new(status_contract, status_signature, worker)?;
    let statuses = status.observe(inputs)?;
    let completed_inputs = inputs
        .iter()
        .zip(&statuses)
        .filter(|(_, v)| v.bits == 0)
        .map(|(input, _)| input.clone())
        .collect::<Vec<_>>();
    let completed = value.observe(&completed_inputs)?;
    let mut completed = completed.into_iter();
    let mut outcomes = Vec::new();
    for (input, status) in inputs.iter().zip(statuses) {
        let outcome = match status.bits {
            0 => Outcome::Completed(completed.next().ok_or("native output count mismatch")?),
            1 => Outcome::Trap("division by zero".into()),
            2 => Outcome::Trap("signed division overflow".into()),
            3 => Outcome::Timeout("step budget exhausted".into()),
            _ => return Err("native artifact returned unknown status".into()),
        };
        if status.bits != 0 {
            match value.observe(std::slice::from_ref(input)) {
                Err(e) if e.starts_with("oracle terminated by signal 4 ") => {}
                Err(e) => return Err(format!("native trap validation failed: {e}")),
                Ok(_) => {
                    return Err("native status reports noncompletion but target returned".into())
                }
            }
        }
        outcomes.push(outcome);
    }
    Ok(NativeValidation {
        outcomes,
        value_identity: value.identity().clone(),
        status_identity: status.identity().clone(),
        wall_seconds: start.elapsed().as_secs_f64(),
    })
}
