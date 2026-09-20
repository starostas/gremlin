use gremlin_core::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    io::{self, Read, Write},
    time::Instant,
};
const Q: i64 = 1 << 28;
const PI: i64 = 843314857;
const STEPS: u64 = 5000;
const POP: usize = 64;
const GENS: usize = 32;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Genome {
    seed: u8,
    first: u8,
    later: u8,
    switch: u8,
    iterations: u8,
    stop: u8,
    guard: u8,
    cap: u8,
}
fn random(r: &mut Rng) -> Genome {
    Genome {
        seed: r.index(5) as u8,
        first: r.index(4) as u8,
        later: r.index(4) as u8,
        switch: r.index(4) as u8,
        iterations: (2 + r.index(7)) as u8,
        stop: r.index(4) as u8,
        guard: r.index(2) as u8,
        cap: r.index(3) as u8,
    }
}
fn mutate(g: Genome, r: &mut Rng) -> Genome {
    let mut a = g;
    let b = random(r);
    for _ in 0..1 + r.index(3) {
        match r.index(8) {
            0 => a.seed = b.seed,
            1 => a.first = b.first,
            2 => a.later = b.later,
            3 => a.switch = b.switch,
            4 => a.iterations = b.iterations,
            5 => a.stop = b.stop,
            6 => a.guard = b.guard,
            _ => a.cap = b.cap,
        }
    }
    a
}
fn mul(a: i64, b: i64) -> i64 {
    (a * b) >> 28
}
fn coeffs(sin: bool) -> Vec<i64> {
    let f = if sin {
        vec![
            1.,
            -1. / 6.,
            1. / 120.,
            -1. / 5040.,
            1. / 362880.,
            -1. / 39916800.,
        ]
    } else {
        vec![
            1.,
            -1. / 2.,
            1. / 24.,
            -1. / 720.,
            1. / 40320.,
            -1. / 3628800.,
        ]
    };
    f.iter().map(|x| (x * Q as f64).round() as i64).collect()
}
fn trig(x: i64) -> (i64, i64) {
    let y = if x > PI / 2 { PI - x } else { x };
    let z = mul(y, y);
    let eval = |c: Vec<i64>| {
        let mut p = *c.last().unwrap();
        for v in c[..c.len() - 1].iter().rev() {
            p = v + mul(p, z);
        }
        p
    };
    (
        mul(y, eval(coeffs(true))),
        eval(coeffs(false)) * if x > PI / 2 { -1 } else { 1 },
    )
}
fn initial(g: Genome, m: i64, e: i64) -> i64 {
    match g.seed {
        0 => m,
        1 => PI / 2,
        2 => m + e / 2,
        3 => m + e,
        _ => {
            if m < Q {
                m + e
            } else {
                m + e / 4
            }
        }
    }
    .clamp(0, PI)
}
fn native(g: Genome, m: i64, e: i64) -> i64 {
    let mut x = initial(g, m, e);
    let (mut lo, mut hi) = (0, PI);
    for tick in 0..g.iterations {
        let (s, c) = trig(x);
        let f = x - m - mul(e, s);
        if f.abs() < [0, 16, 256, 4096][g.stop as usize] {
            break;
        }
        if f > 0 {
            hi = x
        } else {
            lo = x
        }
        let d = Q - mul(e, c);
        let op = if tick < g.switch { g.first } else { g.later };
        let mut step = match op {
            0 => f * Q / d,
            1 => {
                let n = f * Q / d;
                let h = d - mul(mul(e, s), n) / 2;
                if h > Q / 10000 {
                    f * Q / h
                } else {
                    n
                }
            }
            2 => (f * Q / d) / 2,
            _ => f,
        };
        if g.cap > 0 {
            let cap = if g.cap == 1 { Q } else { Q / 2 };
            step = step.clamp(-cap, cap)
        }
        let next = x - step;
        x = if g.guard != 0 && (next < lo || next > hi) {
            (lo + hi) / 2
        } else {
            next.clamp(0, PI)
        };
    }
    x
}
fn polynomial(sin: bool) -> String {
    let c = coeffs(sin);
    let mut p = format!("{}i64", c[c.len() - 1]);
    for v in c[..c.len() - 1].iter().rev() {
        p = format!("add({v}i64,ashr(mul({p},z),28i64))");
    }
    p
}
fn update(op: u8) -> String {
    match op{0=>"step=sdiv(mul(f,268435456i64),d);".into(),1=>"let n:i64=sdiv(mul(f,268435456i64),d); let h:i64=sub(d,sdiv(ashr(mul(es,n),28i64),2i64)); if sgt(h,26843i64) { step=sdiv(mul(f,268435456i64),h); } else { step=n; }".into(),2=>"step=sdiv(sdiv(mul(f,268435456i64),d),2i64);".into(),_=>"step=f;".into()}
}
fn source(g: Genome) -> String {
    let init = match g.seed {
        0 => "m",
        1 => "421657428i64",
        2 => "add(m,sdiv(e,2i64))",
        3 => "add(m,e)",
        _ => "select(slt(m,268435456i64),add(m,e),add(m,sdiv(e,4i64)))",
    };
    let stop = [0, 16, 256, 4096][g.stop as usize];
    let guard = if g.guard != 0 {
        "if select(slt(next,lo),true,sgt(next,hi)) { next=sdiv(add(lo,hi),2i64); }"
    } else {
        ""
    };
    let cap = if g.cap == 0 {
        String::new()
    } else {
        let v = if g.cap == 1 { Q } else { Q / 2 };
        format!("step=select(slt(step,-{v}i64),-{v}i64,select(sgt(step,{v}i64),{v}i64,step));")
    };
    format!(
        r#"fn solve(m:i64,e:i64)->i64 {{
 let mut x:i64={init}; x=select(sgt(x,{PI}i64),{PI}i64,x);
 let mut lo:i64=0i64; let mut hi:i64={PI}i64; let mut tick:i64=0i64;
 while slt(tick,{iterations}i64) {{
  let y:i64=select(sgt(x,421657428i64),sub({PI}i64,x),x);
  let z:i64=ashr(mul(y,y),28i64);
  let sn:i64=ashr(mul(y,{sin}),28i64);
  let cp:i64={cos}; let cs:i64=select(sgt(x,421657428i64),sub(0i64,cp),cp);
  let es:i64=ashr(mul(e,sn),28i64); let f:i64=sub(sub(x,m),es);
  let af:i64=select(slt(f,0i64),sub(0i64,f),f);
  if slt(af,{stop}i64) {{ break; }}
  if sgt(f,0i64) {{ hi=x; }} else {{ lo=x; }}
  let d:i64=sub(268435456i64,ashr(mul(e,cs),28i64)); let mut step:i64=0i64;
  if slt(tick,{switch}i64) {{ {first} }} else {{ {later} }}
  {cap}
  let mut next:i64=sub(x,step); {guard}
  x=select(slt(next,0i64),0i64,select(sgt(next,{PI}i64),{PI}i64,next));
  tick=add(tick,1i64);
 }}
 return x;
}}"#,
        iterations = g.iterations,
        sin = polynomial(true),
        cos = polynomial(false),
        switch = g.switch,
        first = update(g.first),
        later = update(g.later)
    )
}
fn oracle(m: i64, e: i64) -> f64 {
    let m = m as f64 / Q as f64;
    let e = e as f64 / Q as f64;
    let (mut lo, mut hi) = (0., std::f64::consts::PI + 1e-8);
    for _ in 0..60 {
        let x = (lo + hi) * 0.5;
        if x - e * x.sin() > m {
            hi = x
        } else {
            lo = x
        }
    }
    (lo + hi) * 0.5
}
fn inputs(n: usize, seed: u64) -> Vec<Vec<Value>> {
    let mut r = Rng::new(seed);
    let mut out = vec![];
    for k in 0..n {
        let (m, e) = if k < 48 {
            (
                [0, 1, 100, 10000, Q / 100, Q / 10, Q, PI / 2, PI - 100, PI][k % 10],
                [0, Q / 2, Q * 9 / 10, Q * 95 / 100][k % 4],
            )
        } else {
            let u = r.next_u64() % (PI as u64 + 1);
            let m = if k % 2 == 0 {
                ((u as u128 * u as u128) / PI as u128) as u64
            } else {
                u
            };
            (
                m as i64,
                (r.next_u64() % ((Q * 95 / 100) as u64 + 1)) as i64,
            )
        };
        out.push(vec![
            Value::new(Type::I64, m as u64),
            Value::new(Type::I64, e as u64),
        ]);
    }
    out
}
fn emit(v: serde_json::Value) {
    println!("{v}");
    io::stdout().flush().unwrap();
}
#[derive(Clone)]
struct Score {
    bad: usize,
    max: f64,
    steps: u64,
    g: Genome,
}
fn order(a: &Score, b: &Score) -> std::cmp::Ordering {
    a.bad
        .cmp(&b.bad)
        .then_with(|| {
            if a.bad == 0 {
                a.steps.cmp(&b.steps)
            } else {
                a.max.total_cmp(&b.max)
            }
        })
        .then_with(|| a.max.total_cmp(&b.max))
}
fn run(seed: u64, tolerance: f64) -> Result<(), String> {
    let clock = Instant::now();
    let mut rng = Rng::new(seed);
    let mut training = inputs(512, seed ^ 0x43415345);
    let mut expected: Vec<_> = training
        .iter()
        .map(|i| oracle(i[0].signed() as i64, i[1].signed() as i64))
        .collect();
    let challenge: Vec<_> = (0..256)
        .flat_map(|ei| (0..256).map(move |mi| (PI * mi / 255, Q * 95 * ei / (100 * 255))))
        .map(|(m, e)| (m, e, oracle(m, e)))
        .collect();
    let mut counterexamples = 0;
    let mut population: Vec<_> = (0..POP).map(|_| random(&mut rng)).collect();
    let mut best: Option<Score> = None;
    let mut history = vec![];
    let mut kernel = 0.;
    let mut peak = 0;
    let mut device = String::new();
    let mut evaluations = 0usize;
    emit(
        json!({"kind":"start","mode":"gpu","seed":seed,"tolerance":tolerance,"population":POP,"generations":GENS,"cases":training.len()}),
    );
    for generation in 0..GENS {
        let functions: Vec<_> = population
            .iter()
            .map(|g| parse(&source(*g)))
            .collect::<Result<_, _>>()?;
        let r = gremlin_cuda::evaluate(&gremlin_cuda::Request {
            functions: functions.clone(),
            inputs: training.clone(),
            max_steps: STEPS,
            memory_budget: 1 << 30,
        })?;
        kernel += r.telemetry.kernel_ms;
        peak = peak.max(r.telemetry.allocated_bytes);
        device = r.telemetry.device;
        evaluations += POP * training.len();
        let mut ranked = vec![];
        for ((g, f), row) in population.iter().zip(&functions).zip(r.executions) {
            let mut score = Score {
                bad: 0,
                max: 0.,
                steps: 0,
                g: *g,
            };
            let mut interpreter = Evaluator::new(f)?;
            for (n, (ex, target)) in row.iter().zip(&expected).enumerate() {
                let value = match ex.outcome {
                    Outcome::Completed(v) => v.signed() as i64,
                    _ => {
                        score.bad += 1;
                        score.max = f64::INFINITY;
                        continue;
                    }
                };
                if value
                    != native(
                        *g,
                        training[n][0].signed() as i64,
                        training[n][1].signed() as i64,
                    )
                {
                    return Err("GPU/native candidate mismatch".into());
                }
                if n < 4 && interpreter.execute(&training[n], STEPS) != *ex {
                    return Err("GPU/interpreter mismatch".into());
                }
                let error = (value as f64 / Q as f64 - target).abs();
                score.max = score.max.max(error);
                score.bad += usize::from(error > tolerance * 0.1);
                score.steps += ex.steps;
            }
            ranked.push(score);
        }
        ranked.sort_by(order);
        if best.as_ref().is_none_or(|b| order(&ranked[0], b).is_lt()) {
            best = Some(ranked[0].clone());
        }
        let b = best.as_ref().unwrap();
        history.push(json!({"generation":generation+1,"bad":b.bad,"max_error":b.max,"mean_steps":b.steps as f64/training.len()as f64}));
        emit(
            json!({"kind":"progress","generation":generation+1,"generations":GENS,"bad":b.bad,"max_error":b.max,"mean_steps":b.steps as f64/training.len()as f64,"seconds":clock.elapsed().as_secs_f64(),"evaluations":evaluations,"genome":b.g}),
        );
        if generation % 4 == 3 && generation + 1 < GENS {
            let mut missed: Vec<_> = challenge
                .iter()
                .filter_map(|&(m, e, target)| {
                    let error = (native(b.g, m, e) as f64 / Q as f64 - target).abs();
                    (error > tolerance * 0.1).then_some((error, m, e, target))
                })
                .collect();
            missed.sort_by(|a, b| b.0.total_cmp(&a.0));
            let before = training.len();
            for (_, m, e, target) in missed.into_iter().take(32) {
                let i = vec![
                    Value::new(Type::I64, m as u64),
                    Value::new(Type::I64, e as u64),
                ];
                if !training.contains(&i) {
                    training.push(i);
                    expected.push(target);
                }
            }
            counterexamples += training.len() - before;
            if training.len() > before {
                best = None;
                emit(
                    json!({"kind":"stage","message":format!("Found {} difficult orbit cases; feeding them back into the GPU search",training.len()-before)}),
                );
            }
        }
        population = ranked.iter().take(8).map(|s| s.g).collect();
        while population.len() < POP {
            let g = if rng.index(8) == 0 {
                random(&mut rng)
            } else {
                mutate(ranked[rng.index(16)].g, &mut rng)
            };
            population.push(g)
        }
    }
    let b = best.unwrap();
    let text = source(b.g);
    let f = parse(&text)?;
    let mut evaluator = Evaluator::new(&f)?;
    emit(
        json!({"kind":"stage","message":"Checking the full 256 × 256 orbit grid and 4,096 unseen inputs"}),
    );
    let mut grid = vec![];
    let mut max_error = 0f64;
    let mut failures = 0usize;
    let mut worst = json!(null);
    let mut total_steps = 0u64;
    let mut grid_hash = 14695981039346656037u64;
    let mut cases = vec![];
    for ei in 0..256 {
        for mi in 0..256 {
            cases.push(vec![
                Value::new(Type::I64, (PI * mi / 255) as u64),
                Value::new(Type::I64, (Q * 95 * ei / (100 * 255)) as u64),
            ])
        }
    }
    let mut unseen = inputs(4144, seed ^ 0x484f4c44);
    unseen.drain(..48);
    cases.extend(unseen);
    for (n, input) in cases.iter().enumerate() {
        let ex = evaluator.execute(input, STEPS);
        let value = match ex.outcome {
            Outcome::Completed(v) => v.signed() as i64,
            _ => return Err("winner did not complete".into()),
        };
        if value != native(b.g, input[0].signed() as i64, input[1].signed() as i64) {
            return Err("winner interpreter/native mismatch".into());
        }
        let reference = oracle(input[0].signed() as i64, input[1].signed() as i64);
        let error = (value as f64 / Q as f64 - reference).abs();
        if error > max_error {
            max_error = error;
            worst = json!({"m":input[0].bits as f64/Q as f64,"e":input[1].bits as f64/Q as f64,"error":error})
        }
        failures += usize::from(error > tolerance);
        total_steps += ex.steps;
        if n < 65536 {
            for byte in value.to_le_bytes() {
                grid_hash = (grid_hash ^ byte as u64).wrapping_mul(1099511628211);
            }
            grid.push(error);
        }
    }
    // Independent final GPU parity check, using every grid/holdout input in bounded batches.
    for batch in cases.chunks(4096) {
        let r = gremlin_cuda::evaluate(&gremlin_cuda::Request {
            functions: vec![f.clone()],
            inputs: batch.to_vec(),
            max_steps: STEPS,
            memory_budget: 1 << 30,
        })?;
        for (ex, input) in r.executions[0].iter().zip(batch) {
            if ex.outcome
                != Outcome::Completed(Value::new(
                    Type::I64,
                    native(b.g, input[0].signed() as i64, input[1].signed() as i64) as u64,
                ))
            {
                return Err("final GPU parity mismatch".into());
            }
        }
    }
    emit(
        json!({"kind":"done","mode":"gpu","seed":seed,"tolerance":tolerance,"passed":failures==0,"failures":failures,"checked":cases.len(),"grid_cases":65536,"holdout_cases":4096,"max_error":max_error,"worst":worst,"mean_steps":total_steps as f64/cases.len()as f64,"seconds":clock.elapsed().as_secs_f64(),"kernel_ms":kernel,"peak_bytes":peak,"device":device,"evaluations":evaluations,"counterexamples":counterexamples,"training_cases":training.len(),"genome":b.g,"source":text,"llvm":gremlin_codegen::lower(&f,STEPS)?,"history":history,"grid":grid,"grid_hash":format!("{grid_hash:016x}"),"scale":Q,"pi":PI}),
    );
    Ok(())
}
fn main() {
    let result = (|| {
        let mut s = String::new();
        io::stdin()
            .take(4096)
            .read_to_string(&mut s)
            .map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        let seed = v["seed"].as_u64().ok_or("invalid seed")?;
        let tolerance = v["tolerance"].as_f64().ok_or("invalid tolerance")?;
        if v.as_object().map(|x| x.len()) != Some(2)
            || !(1..=3).contains(&seed)
            || ![0.001, 0.0001, 0.00001].contains(&tolerance)
        {
            return Err("unsupported settings".into());
        }
        run(seed, tolerance)
    })();
    if let Err(message) = result {
        emit(json!({"kind":"error","message":message}));
        std::process::exit(1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_control_flow_matches_native() {
        let mut r = Rng::new(7);
        let cases = inputs(64, 8);
        for _ in 0..80 {
            let g = random(&mut r);
            let f = parse(&source(g)).unwrap();
            let mut e = Evaluator::new(&f).unwrap();
            for i in &cases {
                assert_eq!(
                    e.execute(i, STEPS).outcome,
                    Outcome::Completed(Value::new(
                        Type::I64,
                        native(g, i[0].signed() as i64, i[1].signed() as i64) as u64
                    ))
                );
            }
        }
    }
    #[cfg(feature = "cuda")]
    #[test]
    #[ignore = "timed same-program CPU/GPU comparison"]
    fn compare_interpreter_backends() {
        let mut rng = Rng::new(1);
        let cases = inputs(512, 1 ^ 0x43415345);
        let batches: Vec<Vec<_>> = (0..32)
            .map(|_| {
                (0..64)
                    .map(|_| parse(&source(random(&mut rng))).unwrap())
                    .collect()
            })
            .collect();
        let start = Instant::now();
        let cpu: Vec<Vec<Vec<_>>> = batches
            .iter()
            .map(|functions| {
                functions
                    .iter()
                    .map(|f| {
                        let mut e = Evaluator::new(f).unwrap();
                        cases.iter().map(|i| e.execute(i, STEPS)).collect()
                    })
                    .collect()
            })
            .collect();
        let cpu_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let mut kernel_ms = 0.;
        let mut device = String::new();
        for (functions, expected) in batches.into_iter().zip(cpu) {
            let gpu = gremlin_cuda::evaluate(&gremlin_cuda::Request {
                functions,
                inputs: cases.clone(),
                max_steps: STEPS,
                memory_budget: 1 << 30,
            })
            .unwrap();
            assert_eq!(expected, gpu.executions);
            kernel_ms += gpu.telemetry.kernel_ms;
            device = gpu.telemetry.device;
        }
        let gpu_seconds = start.elapsed().as_secs_f64();
        println!(
            "{}",
            json!({"cpu_seconds":cpu_seconds,"gpu_seconds":gpu_seconds,"kernel_ms":kernel_ms,"matching_executions":1048576,"device":device,"scope":"32 distinct random population batches, identical Gremlin programs and inputs on both interpreters; includes cold GPU setup and result transfer, excludes common program construction. Not total application speed."})
        );
    }
    #[test]
    fn holdout_inputs_are_disjoint() {
        for seed in 1..=3 {
            let mut seen: std::collections::BTreeSet<_> = inputs(512, seed ^ 0x43415345)
                .iter()
                .map(|i| (i[0].bits, i[1].bits))
                .collect();
            for ei in 0..256 {
                for mi in 0..256 {
                    seen.insert(((PI * mi / 255) as u64, (Q * 95 * ei / (100 * 255)) as u64));
                }
            }
            for i in inputs(4144, seed ^ 0x484f4c44).into_iter().skip(48) {
                assert!(seen.insert((i[0].bits, i[1].bits)));
            }
        }
    }
    #[test]
    fn reference_residual() {
        for i in inputs(128, 9) {
            let m = i[0].bits as f64 / Q as f64;
            let e = i[1].bits as f64 / Q as f64;
            let x = oracle(i[0].bits as i64, i[1].bits as i64);
            assert!((x - e * x.sin() - m).abs() < 1e-12);
        }
    }
}
