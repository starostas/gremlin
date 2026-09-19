//! Optional CUDA interpreter. Invalid programs and infrastructure failures are errors.
use gremlin_core::{Execution, Function, Value};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub functions: Vec<Function>,
    pub inputs: Vec<Vec<Value>>,
    pub max_steps: u64,
    pub memory_budget: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evaluation {
    pub executions: Vec<Vec<Execution>>,
    pub telemetry: Telemetry,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Telemetry {
    pub setup_ms: f64,
    pub transfer_ms: f64,
    pub kernel_ms: f64,
    pub total_ms: f64,
    pub host_total_ms: f64,
    pub driver: i32,
    pub runtime: i32,
    pub compute_capability: String,
    pub device: String,
    pub compiler: String,
    pub allocated_bytes: u64,
}
#[cfg(not(feature = "cuda"))]
pub fn evaluate(_: &Request) -> Result<Evaluation, String> {
    Err("CUDA backend unavailable: rebuild with --features cuda on a CUDA toolkit machine".into())
}
#[cfg(feature = "cuda")]
pub use backend::evaluate;
#[cfg(feature = "cuda")]
mod backend;
