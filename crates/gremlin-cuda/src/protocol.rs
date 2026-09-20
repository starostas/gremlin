//! Versioned little-endian worker responses. Each execution occupies 18 bytes.
//! Requests remain JSON; responses (including errors and telemetry) are binary.
use crate::{Evaluation, Telemetry};
use gremlin_core::{Execution, Outcome, Type, Value};

pub const RESPONSE_LIMIT: usize = 256 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"GRCUDA01";
const RECORD_BYTES: usize = 18;
const STRING_LIMIT: usize = 64 * 1024;
const TYPES: [Type; 8] = [
    Type::U8,
    Type::U16,
    Type::U32,
    Type::U64,
    Type::I8,
    Type::I16,
    Type::I32,
    Type::I64,
];

fn string(out: &mut Vec<u8>, value: &str) -> Result<(), String> {
    if value.len() > STRING_LIMIT {
        return Err("CUDA protocol string exceeds limit".into());
    }
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

pub fn encode_response(result: &Result<Evaluation, String>) -> Result<Vec<u8>, String> {
    let mut out = MAGIC.to_vec();
    let evaluation = match result {
        Err(error) => {
            out.push(0);
            string(&mut out, error)?;
            return Ok(out);
        }
        Ok(evaluation) => evaluation,
    };
    out.push(1);
    write_telemetry(&mut out, &evaluation.telemetry)?;
    let programs = evaluation.executions.len();
    let cases = evaluation.executions.first().map_or(0, Vec::len);
    if programs == 0 || cases == 0 || programs > u32::MAX as usize || cases > u32::MAX as usize {
        return Err("invalid CUDA response dimensions".into());
    }
    out.extend_from_slice(&(programs as u32).to_le_bytes());
    out.extend_from_slice(&(cases as u32).to_le_bytes());
    let size = programs
        .checked_mul(cases)
        .and_then(|n| n.checked_mul(RECORD_BYTES))
        .and_then(|n| n.checked_add(out.len()))
        .ok_or("CUDA response size overflow")?;
    if size > RESPONSE_LIMIT {
        return Err("CUDA response exceeds protocol limit".into());
    }
    out.reserve(size - out.len());
    for row in &evaluation.executions {
        if row.len() != cases {
            return Err("ragged CUDA response".into());
        }
        for execution in row {
            let (status, ty, bits) = match &execution.outcome {
                Outcome::Completed(value) => {
                    let ty = TYPES
                        .iter()
                        .position(|t| *t == value.ty)
                        .ok_or("invalid CUDA result type")?;
                    if value.bits & !value.ty.mask() != 0 {
                        return Err("invalid CUDA result bits".into());
                    }
                    (0, ty as u8 + 1, value.bits)
                }
                Outcome::Timeout(reason) if reason == "step budget exhausted" => (1, 0, 0),
                Outcome::Trap(reason) if reason == "division by zero" => (2, 0, 0),
                Outcome::Trap(reason) if reason == "signed division overflow" => (3, 0, 0),
                _ => return Err("unsupported CUDA outcome in response".into()),
            };
            out.extend_from_slice(&[status, ty]);
            out.extend_from_slice(&bits.to_le_bytes());
            out.extend_from_slice(&execution.steps.to_le_bytes());
        }
    }
    Ok(out)
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if n > self.0.len() {
            return Err("truncated CUDA response".into());
        }
        let (part, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(part)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn timing(&mut self) -> Result<f64, String> {
        let value = f64::from_bits(self.u64()?);
        if !value.is_finite() || value < 0. {
            return Err("invalid CUDA timing".into());
        }
        Ok(value)
    }
    fn string(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        if n > STRING_LIMIT {
            return Err("CUDA protocol string exceeds limit".into());
        }
        String::from_utf8(self.take(n)?.to_vec())
            .map_err(|_| "invalid UTF-8 in CUDA response".into())
    }
}

pub fn decode_response(bytes: &[u8]) -> Result<Evaluation, String> {
    if bytes.len() > RESPONSE_LIMIT {
        return Err("CUDA response exceeds protocol limit".into());
    }
    let mut r = Reader(bytes);
    if r.take(8)? != MAGIC {
        return Err("unsupported CUDA response version".into());
    }
    match r.take(1)?[0] {
        0 => {
            let error = r.string()?;
            if !r.0.is_empty() {
                return Err("trailing CUDA error response bytes".into());
            }
            return Err(error);
        }
        1 => {}
        _ => return Err("invalid CUDA response tag".into()),
    }
    let telemetry = read_telemetry(&mut r)?;
    let programs = r.u32()? as usize;
    let cases = r.u32()? as usize;
    let size = programs
        .checked_mul(cases)
        .and_then(|n| n.checked_mul(RECORD_BYTES))
        .ok_or("CUDA response size overflow")?;
    // Validate the complete length before allocating any result rows.
    if programs == 0 || cases == 0 || size != r.0.len() {
        return Err("CUDA response dimensions or length mismatch".into());
    }
    let mut executions = Vec::with_capacity(programs);
    for _ in 0..programs {
        let mut row = Vec::with_capacity(cases);
        for _ in 0..cases {
            let b = r.take(RECORD_BYTES)?;
            let bits = u64::from_le_bytes(b[2..10].try_into().unwrap());
            let steps = u64::from_le_bytes(b[10..18].try_into().unwrap());
            let outcome = if b[0] == 0 {
                let ty = *b[1]
                    .checked_sub(1)
                    .and_then(|i| TYPES.get(i as usize))
                    .ok_or("invalid CUDA result type")?;
                if bits & !ty.mask() != 0 {
                    return Err("invalid CUDA result bits".into());
                }
                Outcome::Completed(Value::new(ty, bits))
            } else {
                if b[1] != 0 || bits != 0 {
                    return Err("invalid CUDA noncompleted payload".into());
                }
                match b[0] {
                    1 => Outcome::Timeout("step budget exhausted".into()),
                    2 => Outcome::Trap("division by zero".into()),
                    3 => Outcome::Trap("signed division overflow".into()),
                    _ => return Err("invalid CUDA outcome tag".into()),
                }
            };
            row.push(Execution { outcome, steps });
        }
        executions.push(row);
    }
    Ok(Evaluation {
        executions,
        telemetry,
    })
}

fn write_telemetry(out: &mut Vec<u8>, t: &Telemetry) -> Result<(), String> {
    for n in [
        t.setup_ms,
        t.transfer_ms,
        t.kernel_ms,
        t.total_ms,
        t.host_total_ms,
    ] {
        if !n.is_finite() || n < 0. {
            return Err("invalid CUDA timing".into());
        }
        out.extend_from_slice(&n.to_le_bytes());
    }
    out.extend_from_slice(&t.driver.to_le_bytes());
    out.extend_from_slice(&t.runtime.to_le_bytes());
    out.extend_from_slice(&t.allocated_bytes.to_le_bytes());
    for s in [&t.compute_capability, &t.device, &t.compiler] {
        string(out, s)?;
    }
    Ok(())
}

fn read_telemetry(r: &mut Reader<'_>) -> Result<Telemetry, String> {
    Ok(Telemetry {
        setup_ms: r.timing()?,
        transfer_ms: r.timing()?,
        kernel_ms: r.timing()?,
        total_ms: r.timing()?,
        host_total_ms: r.timing()?,
        driver: r.u32()? as i32,
        runtime: r.u32()? as i32,
        allocated_bytes: r.u64()?,
        compute_capability: r.string()?,
        device: r.string()?,
        compiler: r.string()?,
    })
}

/// Search totals, independent of case count. Field meanings are owned by gremlin-search.
#[derive(Clone, Debug)]
pub struct SummaryEvaluation {
    pub summaries: Vec<[u64; 10]>,
    pub telemetry: Telemetry,
}
pub fn encode_summary_response(
    result: &Result<SummaryEvaluation, String>,
) -> Result<Vec<u8>, String> {
    let evaluation = match result {
        Ok(e) => e,
        Err(e) => return encode_response(&Err(e.clone())),
    };
    let mut out = MAGIC.to_vec();
    out.push(2);
    write_telemetry(&mut out, &evaluation.telemetry)?;
    let n = evaluation.summaries.len();
    if n == 0 || n > u32::MAX as usize {
        return Err("invalid summary count".into());
    }
    out.extend_from_slice(&(n as u32).to_le_bytes());
    let size = n
        .checked_mul(80)
        .and_then(|n| n.checked_add(out.len()))
        .ok_or("summary size overflow")?;
    if size > RESPONSE_LIMIT {
        return Err("CUDA response exceeds protocol limit".into());
    }
    out.reserve(size - out.len());
    for summary in &evaluation.summaries {
        for word in summary {
            out.extend_from_slice(&word.to_le_bytes());
        }
    }
    Ok(out)
}
pub fn decode_summary_response(bytes: &[u8]) -> Result<SummaryEvaluation, String> {
    if bytes.len() > RESPONSE_LIMIT {
        return Err("CUDA response exceeds protocol limit".into());
    }
    let mut r = Reader(bytes);
    if r.take(8)? != MAGIC {
        return Err("unsupported CUDA response version".into());
    }
    match r.take(1)?[0] {
        0 => return Err(decode_response(bytes).unwrap_err()),
        2 => {}
        _ => return Err("expected CUDA summary response".into()),
    }
    let telemetry = read_telemetry(&mut r)?;
    let count = r.u32()? as usize;
    if count == 0 || count.checked_mul(80) != Some(r.0.len()) {
        return Err("summary dimensions or length mismatch".into());
    }
    let mut summaries = Vec::with_capacity(count);
    for _ in 0..count {
        let mut words = [0; 10];
        for word in &mut words {
            *word = r.u64()?;
        }
        summaries.push(words);
    }
    Ok(SummaryEvaluation {
        summaries,
        telemetry,
    })
}
