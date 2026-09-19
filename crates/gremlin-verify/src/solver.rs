use gremlin_core::hash;
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize)]
pub struct SolverIdentity {
    pub path: String,
    pub sha256: String,
    pub version: String,
    pub timeout_ms: u64,
}
#[derive(Debug)]
pub enum Answer {
    Unsat,
    Sat(Vec<u64>),
    Timeout,
    Unknown(String),
}
fn invoke(
    path: &Path,
    args: &[&str],
    payload: Vec<u8>,
    timeout_ms: u64,
) -> Result<Option<String>, String> {
    let mut child = Command::new(path)
        .args(args)
        .env_clear()
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("solver unavailable: {e}"))?;
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let writer = thread::spawn(move || stdin.write_all(&payload));
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(e) => break Err(e.to_string()),
            _ => {}
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            timed_out = true;
            break Err("timeout".into());
        }
        thread::sleep(Duration::from_millis(2));
    };
    if status.is_err() {
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.wait();
    }
    let write = writer.join().map_err(|_| "solver writer panicked")?;
    let bytes = reader
        .join()
        .map_err(|_| "solver reader panicked")?
        .map_err(|e| e.to_string())?;
    if timed_out {
        return Ok(None);
    }
    let status = status?;
    if !status.success() {
        return Err(format!(
            "solver exited {status}: {}",
            String::from_utf8_lossy(&bytes)
        ));
    }
    write.map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("solver output exceeds protocol limit".into());
    }
    Ok(Some(String::from_utf8(bytes).map_err(|e| e.to_string())?))
}
pub fn identity(path: &Path, timeout_ms: u64) -> Result<SolverIdentity, String> {
    if timeout_ms == 0 || timeout_ms > 600000 {
        return Err("solver timeout must be 1..600000 ms".into());
    }
    let version = invoke(path, &["-version"], vec![], 5000)?.ok_or("solver identity timeout")?;
    Ok(SolverIdentity {
        path: path.to_string_lossy().into_owned(),
        sha256: hash(&fs::read(path).map_err(|e| e.to_string())?),
        version: version.trim().into(),
        timeout_ms,
    })
}
pub fn solve(identity: &SolverIdentity, query: &str, arguments: usize) -> Result<Answer, String> {
    let path = Path::new(&identity.path);
    if hash(&fs::read(path).map_err(|e| e.to_string())?) != identity.sha256 {
        return Err("solver executable changed".into());
    }
    let payload = format!(
        "(set-option :timeout {})\n{query}\n(check-sat)\n",
        identity.timeout_ms
    );
    let Some(output) = invoke(
        path,
        &["-in", "-smt2"],
        payload.as_bytes().to_vec(),
        identity.timeout_ms,
    )?
    else {
        return Ok(Answer::Timeout);
    };
    match output.trim() {
        "unsat" => return Ok(Answer::Unsat),
        "unknown" => {
            return Ok(Answer::Unknown(
                "solver returned unknown (possibly internal resource limit)".into(),
            ))
        }
        "sat" => {}
        _ => return Err(format!("unexpected solver response: {output}")),
    }
    if arguments == 0 {
        return Ok(Answer::Sat(vec![]));
    }
    let payload = format!(
        "{payload}(get-value ({}))\n",
        (0..arguments)
            .map(|i| format!("x{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let Some(output) = invoke(
        path,
        &["-in", "-smt2"],
        payload.into_bytes(),
        identity.timeout_ms,
    )?
    else {
        return Ok(Answer::Timeout);
    };
    let tokens: Vec<_> = output
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .filter(|s| !s.is_empty())
        .collect();
    if tokens.len() != 1 + 2 * arguments || tokens[0] != "sat" {
        return Err(format!("invalid solver model response: {output}"));
    }
    let mut values = Vec::new();
    for n in 0..arguments {
        if tokens[1 + n * 2] != format!("x{n}") {
            return Err("solver model argument order mismatch".into());
        }
        let text = tokens[2 + n * 2];
        let value = if let Some(hex) = text.strip_prefix("#x") {
            u64::from_str_radix(hex, 16)
        } else if let Some(bits) = text.strip_prefix("#b") {
            u64::from_str_radix(bits, 2)
        } else {
            return Err("unsupported solver value encoding".into());
        };
        values.push(value.map_err(|e| e.to_string())?);
    }
    Ok(Answer::Sat(values))
}
