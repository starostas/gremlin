//! Native execution boundary. Targets are never loaded by the host process.
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
compile_error!("gremlin-native currently supports Linux x86-64 only");
mod sandbox;
use gremlin_core::*;
use object::{Object, ObjectSymbol};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub const MAX_BATCH: usize = 4096;
const RUNTIME_FILES: [&str; 4] = [
    "/lib64/ld-linux-x86-64.so.2",
    "/lib/x86_64-linux-gnu/libc.so.6",
    "/lib/x86_64-linux-gnu/libm.so.6",
    "/lib/x86_64-linux-gnu/libgcc_s.so.1",
];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BinaryIdentity {
    pub contract: BinaryContract,
    pub binary_sha256: String,
    pub worker_sha256: String,
    pub sandbox_version: String,
    pub sandbox_sha256: String,
    pub runtime_files: BTreeMap<String, String>,
    pub kernel: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub contract: BinaryContract,
    pub signature: Signature,
    pub inputs: Vec<Vec<Value>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub values: Vec<Value>,
    pub error: Option<String>,
}
pub struct BinaryOracle {
    contract: BinaryContract,
    signature: Signature,
    worker: PathBuf,
    identity: BinaryIdentity,
}
impl BinaryOracle {
    pub fn new(
        contract: BinaryContract,
        signature: Signature,
        worker: &Path,
    ) -> Result<Self, String> {
        contract.validate()?;
        signature.validate()?;
        let data = fs::read(&contract.path).map_err(|e| format!("read binary: {e}"))?;
        let digest = hash(&data);
        if digest != contract.sha256.to_lowercase() {
            return Err("binary SHA-256 mismatch".into());
        }
        if data.len() < 20
            || &data[..4] != b"\x7fELF"
            || data[4] != 2
            || data[5] != 1
            || u16::from_le_bytes([data[16], data[17]]) != 3
            || u16::from_le_bytes([data[18], data[19]]) != 62
        {
            return Err("expected ELF64 little-endian x86-64 shared object (ET_DYN)".into());
        }
        let file =
            object::File::parse(data.as_slice()).map_err(|e| format!("ELF metadata: {e}"))?;
        if !file
            .dynamic_symbols()
            .any(|s| s.name().ok() == Some(contract.symbol.as_str()) && !s.is_undefined())
        {
            return Err(format!("missing exported symbol {}", contract.symbol));
        }
        let version = Command::new("/usr/bin/bwrap")
            .arg("--version")
            .output()
            .map_err(|e| format!("bubblewrap required: {e}"))?;
        if !version.status.success() {
            return Err("bubblewrap version query failed".into());
        }
        let files = RUNTIME_FILES
            .iter()
            .map(|p| {
                Ok((
                    p.to_string(),
                    hash(&fs::read(p).map_err(|e| format!("runtime dependency {p}: {e}"))?),
                ))
            })
            .collect::<Result<_, String>>()?;
        let kernel = Command::new("uname")
            .args(["-srvm"])
            .output()
            .map_err(|e| e.to_string())?;
        let identity = BinaryIdentity {
            contract: contract.clone(),
            binary_sha256: digest,
            worker_sha256: hash(&fs::read(worker).map_err(|e| e.to_string())?),
            sandbox_version: String::from_utf8_lossy(&version.stdout).trim().into(),
            sandbox_sha256: hash(&fs::read("/usr/bin/bwrap").map_err(|e| e.to_string())?),
            runtime_files: files,
            kernel: String::from_utf8_lossy(&kernel.stdout).trim().into(),
        };
        Ok(Self {
            contract,
            signature,
            worker: worker.to_owned(),
            identity,
        })
    }
    pub fn identity(&self) -> &BinaryIdentity {
        &self.identity
    }
    pub fn target_identity(&self, name: &str) -> TargetIdentity {
        TargetIdentity{name:name.into(),signature:self.signature.clone(),contract:"pure deterministic total integer return; ELF x86-64 sysv64; isolated empty environment; no observable syscalls".into(),implementation_fingerprint:object_hash(&self.identity)}
    }
    pub fn observe(&self, inputs: &[Vec<Value>]) -> Result<Vec<Value>, String> {
        let mut values = vec![];
        for batch in inputs.chunks(MAX_BATCH) {
            let a = self.once(batch)?;
            let b = self.once(batch)?;
            if a != b {
                return Err("oracle nondeterminism across fresh workers".into());
            }
            values.extend(a);
        }
        Ok(values)
    }
    fn once(&self, inputs: &[Vec<Value>]) -> Result<Vec<Value>, String> {
        if hash(&fs::read(&self.contract.path).map_err(|e| e.to_string())?)
            != self.identity.binary_sha256
        {
            return Err("binary changed since identity was established".into());
        }
        if hash(&fs::read(&self.worker).map_err(|e| e.to_string())?) != self.identity.worker_sha256
            || hash(&fs::read("/usr/bin/bwrap").map_err(|e| e.to_string())?)
                != self.identity.sandbox_sha256
        {
            return Err("worker or sandbox executable changed during run".into());
        }
        for (path, expected) in &self.identity.runtime_files {
            if hash(&fs::read(path).map_err(|e| e.to_string())?) != *expected {
                return Err(format!("runtime dependency changed: {path}"));
            }
        }
        for input in inputs {
            encode_transport(&self.signature, input)?;
        }
        let mut request = Request {
            contract: self.contract.clone(),
            signature: self.signature.clone(),
            inputs: inputs.to_vec(),
        };
        request.contract.path = "/target.so".into();
        let mut command = Command::new("/usr/bin/bwrap");
        command
            .args([
                "--unshare-all",
                "--die-with-parent",
                "--new-session",
                "--cap-drop",
                "ALL",
                "--clearenv",
                "--ro-bind",
            ])
            .arg(&self.worker)
            .arg("/worker")
            .arg("--ro-bind")
            .arg(fs::canonicalize(&self.contract.path).map_err(|e| e.to_string())?)
            .arg("/target.so");
        for p in RUNTIME_FILES {
            command.args(["--ro-bind", p, p]);
        }
        command
            .args(["--chdir", "/", "/worker", "--oracle-worker"])
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = command.spawn().map_err(|e| format!("worker spawn: {e}"))?;
        let pid = child.id();
        let start = Instant::now();
        let data = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        let mut stdin = child.stdin.take().unwrap();
        let writer = std::thread::spawn(move || stdin.write_all(&data));
        let mut stdout = child.stdout.take().unwrap();
        let reader = std::thread::spawn(move || {
            let mut data = vec![];
            stdout
                .by_ref()
                .take(2_000_001)
                .read_to_end(&mut data)
                .map(|_| data)
        });
        let mut stderr = child.stderr.take().unwrap();
        let errors = std::thread::spawn(move || {
            let mut data = vec![];
            stderr
                .by_ref()
                .take(65537)
                .read_to_end(&mut data)
                .map(|_| data)
        });
        let (status, timed_out) = loop {
            match child.try_wait() {
                Ok(Some(status)) => break (status, false),
                Ok(None) => {}
                Err(e) => {
                    unsafe {
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                    let _ = child.wait();
                    return Err(format!("worker wait failed: {e}"));
                }
            }
            if start.elapsed() >= Duration::from_millis(self.contract.wall_timeout_ms) {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
                let status = child.wait().map_err(|e| e.to_string())?;
                break (status, true);
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        let write_result = writer
            .join()
            .map_err(|_| "worker request writer panicked")?;
        let output = reader
            .join()
            .map_err(|_| "worker reader panicked")?
            .map_err(|e| e.to_string())?;
        let errors = errors
            .join()
            .map_err(|_| "worker error reader panicked")?
            .map_err(|e| e.to_string())?;
        if timed_out {
            return Err("oracle wall-clock timeout".into());
        }
        if !status.success() {
            return Err(format!(
                "oracle worker failure ({status}): {}",
                String::from_utf8_lossy(&errors)
            ));
        }
        write_result.map_err(|e| format!("worker request failed: {e}"))?;
        if output.len() > 2_000_000 {
            return Err("worker protocol output exceeded limit".into());
        }
        let response: Response =
            serde_json::from_slice(&output).map_err(|e| format!("invalid worker protocol: {e}"))?;
        if let Some(e) = response.error {
            return Err(e);
        }
        if response.values.len() != inputs.len()
            || response
                .values
                .iter()
                .any(|v| v.ty != self.signature.return_type)
        {
            return Err("worker result count/type mismatch".into());
        }
        Ok(response.values)
    }
}
pub fn worker_main() -> i32 {
    let result = (|| {
        let mut data = vec![];
        std::io::stdin()
            .take(2_000_001)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() > 2_000_000 {
            return Err("worker request too large".into());
        }
        let request: Request = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        sandbox::execute(&request)
    })();
    let response = match result {
        Ok(values) => Response {
            values,
            error: None,
        },
        Err(error) => Response {
            values: vec![],
            error: Some(error),
        },
    };
    match serde_json::to_writer(std::io::stdout(), &response) {
        Ok(()) => 0,
        Err(_) => 4,
    }
}
