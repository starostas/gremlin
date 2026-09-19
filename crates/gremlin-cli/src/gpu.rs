use gremlin_core::{Corpus, Execution, Function, Value};
use gremlin_cuda::{Evaluation, Request};
use gremlin_search::{Backend, Config, CudaConfig, Engine};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const PROTOCOL_LIMIT: u64 = 256 * 1024 * 1024;
pub fn worker() -> i32 {
    let result = (|| {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(PROTOCOL_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > PROTOCOL_LIMIT {
            return Err("CUDA request exceeds protocol limit".into());
        }
        let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        gremlin_cuda::evaluate(&request)
    })();
    match serde_json::to_writer(std::io::stdout(), &result) {
        Ok(()) => 0,
        Err(_) => 4,
    }
}
pub fn engine(config: &Config, corpus: &Corpus) -> Result<Engine, String> {
    let engine = Engine::new(config.search.clone(), corpus)?;
    match &config.search.cuda {
        None => Ok(engine),
        Some(cuda) => {
            if !cfg!(feature = "cuda") {
                return Err("CUDA backend unavailable: rebuild with --features cuda".into());
            }
            Ok(engine.with_backend(Box::new(GpuBackend {
                config: cuda.clone(),
                log: PathBuf::from(&config.output.directory).join("cuda-batches.jsonl"),
            })))
        }
    }
}
struct GpuBackend {
    config: CudaConfig,
    log: PathBuf,
}
impl Backend for GpuBackend {
    fn evaluate(
        &self,
        functions: &[Function],
        inputs: &[Vec<Value>],
        max_steps: u64,
    ) -> Result<Vec<Vec<Execution>>, String> {
        let started = Instant::now();
        let request = Request {
            functions: functions.to_vec(),
            inputs: inputs.to_vec(),
            max_steps,
            memory_budget: self.config.memory_budget_mb * 1024 * 1024,
        };
        let payload = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        if payload.len() as u64 > PROTOCOL_LIMIT {
            return Err("CUDA request exceeds protocol limit".into());
        }
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--cuda-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let writer = thread::spawn(move || stdin.write_all(&payload));
        let reader = thread::spawn(move || {
            let mut b = Vec::new();
            stdout
                .take(PROTOCOL_LIMIT + 1)
                .read_to_end(&mut b)
                .map(|_| b)
        });
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(e.to_string());
                }
                _ => {}
            }
            if started.elapsed() >= Duration::from_millis(self.config.wall_timeout_ms) {
                let _ = child.kill();
                let _ = child.wait();
                break Err("CUDA worker wall timeout; batch aborted".into());
            }
            thread::sleep(Duration::from_millis(5));
        };
        let written = writer.join().map_err(|_| "CUDA writer panicked")?;
        let output = reader
            .join()
            .map_err(|_| "CUDA reader panicked")?
            .map_err(|e| e.to_string())?;
        let status = status?;
        if !status.success() {
            return Err(format!("CUDA worker failed: {status}"));
        }
        written.map_err(|e| e.to_string())?;
        if output.len() as u64 > PROTOCOL_LIMIT {
            return Err("CUDA response exceeds protocol limit".into());
        }
        let evaluation: Result<Evaluation, String> =
            serde_json::from_slice(&output).map_err(|e| format!("CUDA worker protocol: {e}"))?;
        let evaluation = evaluation?;
        let record = serde_json::json!({"programs":functions.len(),"cases":inputs.len(),"max_steps":max_steps,"worker_wall_ms":started.elapsed().as_secs_f64()*1000.,"telemetry":evaluation.telemetry});
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut file, &record).map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
        Ok(evaluation.executions)
    }
}
