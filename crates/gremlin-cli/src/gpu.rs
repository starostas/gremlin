use gremlin_core::{Corpus, Execution, Function, Value};
use gremlin_cuda::{
    protocol::{self, SummaryEvaluation},
    Request, Telemetry,
};
use gremlin_search::{
    summarize_batch, Backend, ComparatorConfig, Config, CudaConfig, Engine, EvaluationSummary,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const PROTOCOL_LIMIT: u64 = 256 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScoredRequest {
    request: Request,
    expected: Vec<Value>,
    comparator: ComparatorConfig,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum WorkerRequest {
    Scored(ScoredRequest),
    Detailed(Request),
}
pub fn worker() -> i32 {
    let response = (|| {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(PROTOCOL_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > PROTOCOL_LIMIT {
            return Err("CUDA request exceeds protocol limit".into());
        }
        let request: WorkerRequest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        match request {
            WorkerRequest::Detailed(request) => {
                protocol::encode_response(&gremlin_cuda::evaluate(&request))
            }
            WorkerRequest::Scored(request) => {
                if request.expected.len() != request.request.inputs.len() {
                    return Err("scoring expected/input count mismatch".into());
                }
                request.comparator.program()?;
                let evaluated = gremlin_cuda::evaluate(&request.request)?;
                let summaries = summarize_batch(
                    evaluated.executions,
                    &request.expected,
                    request.request.max_steps,
                    &request.comparator,
                )?;
                protocol::encode_summary_response(&Ok(SummaryEvaluation {
                    summaries: summaries.iter().map(EvaluationSummary::words).collect(),
                    telemetry: evaluated.telemetry,
                }))
            }
        }
    })();
    let bytes = response.or_else(|error| protocol::encode_response(&Err(error)));
    match bytes.and_then(|bytes| {
        std::io::stdout()
            .write_all(&bytes)
            .map_err(|e| e.to_string())
    }) {
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
impl GpuBackend {
    fn request(&self, functions: &[Function], inputs: &[Vec<Value>], max_steps: u64) -> Request {
        Request {
            functions: functions.to_vec(),
            inputs: inputs.to_vec(),
            max_steps,
            memory_budget: self.config.memory_budget_mb * 1024 * 1024,
        }
    }
    fn exchange(&self, payload: Vec<u8>, started: Instant) -> Result<Vec<u8>, String> {
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
        Ok(output)
    }
    fn log(
        &self,
        request: &Request,
        telemetry: &Telemetry,
        started: Instant,
        kind: &str,
        bytes: usize,
    ) -> Result<(), String> {
        let record = serde_json::json!({"programs":request.functions.len(),"cases":request.inputs.len(),"max_steps":request.max_steps,
            "worker_wall_ms":started.elapsed().as_secs_f64()*1000.,"telemetry":telemetry,"response_kind":kind,"response_bytes":bytes});
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut file, &record).map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
        Ok(())
    }
}
impl Backend for GpuBackend {
    fn evaluate(
        &self,
        functions: &[Function],
        inputs: &[Vec<Value>],
        max_steps: u64,
    ) -> Result<Vec<Vec<Execution>>, String> {
        let started = Instant::now();
        let request = self.request(functions, inputs, max_steps);
        let output = self.exchange(
            serde_json::to_vec(&request).map_err(|e| e.to_string())?,
            started,
        )?;
        let evaluation =
            protocol::decode_response(&output).map_err(|e| format!("CUDA worker protocol: {e}"))?;
        if evaluation.executions.len() != functions.len()
            || evaluation
                .executions
                .iter()
                .any(|row| row.len() != inputs.len())
        {
            return Err("CUDA worker response dimensions differ from request".into());
        }
        for (function, row) in functions.iter().zip(&evaluation.executions) {
            for execution in row {
                if execution.steps > max_steps
                    || matches!(&execution.outcome,gremlin_core::Outcome::Completed(value) if value.ty!=function.return_type)
                {
                    return Err("CUDA worker result differs from request contract".into());
                }
            }
        }
        self.log(
            &request,
            &evaluation.telemetry,
            started,
            "detailed",
            output.len(),
        )?;
        Ok(evaluation.executions)
    }
    fn summarize(
        &self,
        functions: &[Function],
        inputs: &[Vec<Value>],
        expected: &[Value],
        max_steps: u64,
        comparator: &ComparatorConfig,
    ) -> Result<Vec<EvaluationSummary>, String> {
        let started = Instant::now();
        let request = ScoredRequest {
            request: self.request(functions, inputs, max_steps),
            expected: expected.to_vec(),
            comparator: comparator.clone(),
        };
        let output = self.exchange(
            serde_json::to_vec(&request).map_err(|e| e.to_string())?,
            started,
        )?;
        let evaluation = protocol::decode_summary_response(&output)
            .map_err(|e| format!("CUDA worker protocol: {e}"))?;
        if evaluation.summaries.len() != functions.len() {
            return Err("CUDA summary program count mismatch".into());
        }
        let summaries = evaluation
            .summaries
            .into_iter()
            .map(EvaluationSummary::from_words)
            .collect::<Result<Vec<_>, _>>()?;
        for (summary, function) in summaries.iter().zip(functions) {
            summary.validate(inputs.len(), function.return_type, max_steps, comparator)?;
        }
        self.log(
            &request.request,
            &evaluation.telemetry,
            started,
            "summary",
            output.len(),
        )?;
        Ok(summaries)
    }
}
