use super::*;
use gremlin_core::{Expr, Op, Outcome, Terminator, Type};
use std::{collections::BTreeMap, ffi::CStr, mem::size_of, os::raw::c_char, time::Instant};
#[repr(C)]
#[derive(Default)]
struct Instruction {
    op: u32,
    dst: u32,
    a: u32,
    b: u32,
    c: u32,
    width: u32,
    result_width: u32,
    pad: u32,
    literal: u64,
}
#[repr(C)]
#[derive(Default)]
struct Block {
    start: u32,
    len: u32,
    params_offset: u32,
    params_len: u32,
    term: u32,
    a: u32,
    yes: u32,
    no: u32,
}
#[repr(C)]
struct Edge {
    target: u32,
    args_offset: u32,
    len: u32,
}
#[repr(C)]
struct Program {
    registers_offset: u64,
    registers: u32,
    scratch: u32,
    entry: u32,
    argument_count: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RawResult {
    bits: u64,
    steps: u64,
    status: u32,
    reason: u32,
}
#[repr(C)]
struct Metrics {
    setup_ms: f64,
    transfer_ms: f64,
    kernel_ms: f64,
    total_ms: f64,
    driver: i32,
    runtime: i32,
    major: i32,
    minor: i32,
    device: [c_char; 256],
}
extern "C" {
    fn gremlin_cuda_evaluate(
        programs: *const Program,
        program_count: usize,
        blocks: *const Block,
        block_count: usize,
        instructions: *const Instruction,
        instruction_count: usize,
        edges: *const Edge,
        edge_count: usize,
        indices: *const u32,
        index_count: usize,
        inputs: *const u64,
        input_count: usize,
        register_count: u64,
        cases: u32,
        budget: u64,
        results: *mut RawResult,
        metrics: *mut Metrics,
        error: *mut c_char,
        error_size: usize,
    ) -> i32;
}
fn index(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "CUDA encoding exceeds 32-bit index limit".into())
}
fn bytes<T>(n: usize) -> Result<u64, String> {
    (n as u64)
        .checked_mul(size_of::<T>() as u64)
        .ok_or("CUDA allocation size overflow".into())
}
pub fn evaluate(request: &Request) -> Result<Evaluation, String> {
    let start = Instant::now();
    let first = request
        .functions
        .first()
        .ok_or("CUDA batch must contain programs")?;
    if request.inputs.is_empty()
        || request.inputs.len() > 2_097_120
        || request.functions.len() > 65_535
    {
        return Err("CUDA batch requires 1..65535 programs and 1..2097120 cases".into());
    }
    let signature = first.signature();
    signature.validate()?;
    for input in &request.inputs {
        if input.len() != signature.arguments.len()
            || input
                .iter()
                .zip(&signature.arguments)
                .any(|(v, t)| v.ty != *t || v.bits & !t.mask() != 0)
        {
            return Err("CUDA input signature or bit pattern mismatch".into());
        }
    }
    let cases = index(request.inputs.len())?;
    let mut programs = Vec::new();
    let mut blocks = Vec::new();
    let mut instructions = Vec::new();
    let mut edges = Vec::new();
    let mut indices = Vec::new();
    let mut register_count = 0u64;
    for function in &request.functions {
        if function.signature() != signature {
            return Err("CUDA batch signatures differ".into());
        }
        if !function.callees.is_empty() {
            return Err("CUDA internal calls are unsupported".into());
        }
        let f = function.normalized()?;
        let block_base = index(blocks.len())?;
        let block_ids: BTreeMap<_, _> = f
            .blocks
            .iter()
            .enumerate()
            .map(|(n, b)| (b.id, block_base + n as u32))
            .collect();
        let mut types = BTreeMap::<u32, Type>::new();
        for p in &f.parameters {
            types.insert(p.id, p.ty);
        }
        for b in &f.blocks {
            for p in &b.parameters {
                types.insert(p.id, p.ty);
            }
            for i in &b.instructions {
                types.insert(i.id, i.ty);
            }
        }
        let registers = index(types.len())?;
        let scratch = index(
            f.blocks
                .iter()
                .map(|b| b.parameters.len())
                .max()
                .unwrap_or(0),
        )?;
        programs.push(Program {
            registers_offset: register_count,
            registers,
            scratch,
            entry: block_ids[&f.entry],
            argument_count: index(f.parameters.len())?,
        });
        register_count = register_count
            .checked_add(
                (u64::from(registers) + u64::from(scratch))
                    .checked_mul(u64::from(cases))
                    .ok_or("CUDA register size overflow")?,
            )
            .ok_or("CUDA register size overflow")?;
        for b in &f.blocks {
            let mut raw = Block {
                start: index(instructions.len())?,
                len: index(b.instructions.len())?,
                params_offset: index(indices.len())?,
                params_len: index(b.parameters.len())?,
                ..Default::default()
            };
            indices.extend(b.parameters.iter().map(|p| p.id));
            for i in &b.instructions {
                let mut ri = Instruction {
                    dst: i.id,
                    result_width: i.ty.width(),
                    ..Default::default()
                };
                match &i.expr {
                    Expr::Const { value } => ri.literal = value.bits,
                    Expr::Call { .. } => return Err("CUDA internal calls are unsupported".into()),
                    Expr::Apply { op, args } => {
                        ri.op = index(
                            Op::ALL
                                .iter()
                                .position(|x| x == op)
                                .ok_or("unknown CUDA opcode")?
                                + 1,
                        )?;
                        ri.width = types[&args[0]].width();
                        ri.a = args[0];
                        ri.b = *args.get(1).unwrap_or(&args[0]);
                        ri.c = *args.get(2).unwrap_or(&args[0]);
                    }
                }
                instructions.push(ri);
            }
            let mut edge = |e: &gremlin_core::Edge| -> Result<u32, String> {
                let n = index(edges.len())?;
                edges.push(Edge {
                    target: block_ids[&e.block],
                    args_offset: index(indices.len())?,
                    len: index(e.args.len())?,
                });
                indices.extend(&e.args);
                Ok(n)
            };
            match &b.terminator {
                Terminator::Return { value } => raw.a = *value,
                Terminator::Jump { edge: e } => {
                    raw.term = 1;
                    raw.yes = edge(e)?;
                }
                Terminator::Branch {
                    condition,
                    if_true,
                    if_false,
                } => {
                    raw.term = 2;
                    raw.a = *condition;
                    raw.yes = edge(if_true)?;
                    raw.no = edge(if_false)?;
                }
            }
            blocks.push(raw);
        }
    }
    let inputs: Vec<u64> = request.inputs.iter().flatten().map(|v| v.bits).collect();
    let result_count = request
        .functions
        .len()
        .checked_mul(request.inputs.len())
        .ok_or("CUDA result size overflow")?;
    let sizes = [
        bytes::<Program>(programs.len())?,
        bytes::<Block>(blocks.len())?,
        bytes::<Instruction>(instructions.len())?,
        bytes::<Edge>(edges.len())?,
        bytes::<u32>(indices.len())?,
        bytes::<u64>(inputs.len())?,
        register_count
            .checked_mul(8)
            .ok_or("CUDA register size overflow")?,
        bytes::<RawResult>(result_count)?,
    ];
    let allocated_bytes = sizes
        .into_iter()
        .try_fold(0u64, |a, b| a.checked_add(b))
        .ok_or("CUDA allocation size overflow")?;
    if allocated_bytes > request.memory_budget {
        return Err(format!(
            "CUDA batch requires {allocated_bytes} bytes, exceeds memory budget {}",
            request.memory_budget
        ));
    }
    let mut results = vec![RawResult::default(); result_count];
    let mut metrics = Metrics {
        setup_ms: 0.,
        transfer_ms: 0.,
        kernel_ms: 0.,
        total_ms: 0.,
        driver: 0,
        runtime: 0,
        major: 0,
        minor: 0,
        device: [0; 256],
    };
    let mut error = [0 as c_char; 1024];
    // All input arrays are validated, dense and live for this synchronous FFI call.
    let status = unsafe {
        gremlin_cuda_evaluate(
            programs.as_ptr(),
            programs.len(),
            blocks.as_ptr(),
            blocks.len(),
            instructions.as_ptr(),
            instructions.len(),
            edges.as_ptr(),
            edges.len(),
            indices.as_ptr(),
            indices.len(),
            inputs.as_ptr(),
            inputs.len(),
            register_count,
            cases,
            request.max_steps,
            results.as_mut_ptr(),
            &mut metrics,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        return Err(format!(
            "CUDA infrastructure failure: {}",
            unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
        ));
    }
    let executions = results
        .chunks(request.inputs.len())
        .map(|chunk| {
            chunk
                .iter()
                .map(|r| {
                    let outcome = match (r.status, r.reason) {
                        (0, _) => Outcome::Completed(Value::new(signature.return_type, r.bits)),
                        (1, _) => Outcome::Timeout("step budget exhausted".into()),
                        (2, 1) => Outcome::Trap("division by zero".into()),
                        (2, 2) => Outcome::Trap("signed division overflow".into()),
                        _ => return Err("CUDA returned invalid interpreter status".into()),
                    };
                    Ok(Execution {
                        outcome,
                        steps: r.steps,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .collect::<Result<Vec<_>, String>>()?;
    let telemetry = Telemetry {
        setup_ms: metrics.setup_ms,
        transfer_ms: metrics.transfer_ms,
        kernel_ms: metrics.kernel_ms,
        total_ms: metrics.total_ms,
        host_total_ms: start.elapsed().as_secs_f64() * 1000.,
        driver: metrics.driver,
        runtime: metrics.runtime,
        compute_capability: format!("{}.{}", metrics.major, metrics.minor),
        device: unsafe { CStr::from_ptr(metrics.device.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
        compiler: env!("GREMLIN_NVCC_VERSION").into(),
        allocated_bytes,
    };
    Ok(Evaluation {
        executions,
        telemetry,
    })
}
