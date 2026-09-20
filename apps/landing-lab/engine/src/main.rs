use gremlin_core::*;
use serde_json::json;
use std::{
    collections::BTreeSet,
    io::{self, Write},
    time::Instant,
};

const STEPS: u64 = 40000;
#[derive(Clone, Copy, Debug)]
struct Policy {
    brake: usize,
    margin: i32,
    steer: usize,
}
fn source(p: Policy) -> String {
    let brakes = [
        "mul(vy,4i32)",
        "mul(vy,8i32)",
        "sdiv(mul(vy,vy),8i32)",
        "sdiv(mul(vy,vy),12i32)",
        "sdiv(mul(vy,vy),16i32)",
        "sdiv(mul(lookahead,lookahead),8i32)",
        "sdiv(mul(lookahead,lookahead),12i32)",
        "sdiv(mul(lookahead,lookahead),16i32)",
    ];
    let steering = [
        "select(slt(x,0i32),2i32,-2i32)",
        "select(slt(vx,0i32),2i32,-2i32)",
        "select(slt(vx,desired),2i32,select(sgt(vx,desired),-2i32,0i32))",
        "select(slt(vx,desired),2i32,select(sgt(vx,desired),-2i32,0i32))",
    ];
    format!(
        r#"fn autopilot(x0:i32, height0:i32, vx0:i32, vy0:i32)->i32 {{
    let mut x:i32=x0;
    let mut height:i32=height0;
    let mut vx:i32=vx0;
    let mut vy:i32=vy0;
    let wind:i32=sub(srem(add(x0,1024i32),3i32),1i32);
    let mut fuel:i32=224i32;
    let mut tick:i32=0i32;
    let mut vertical:i32=0i32;
    let mut horizontal:i32=0i32;
    while slt(tick,256i32) {{
        if sle(height,0i32) {{ break; }}
        let lookahead:i32=add(vy,8i32);
        let stopping:i32=add({brake},{margin}i32);
        let burn:bool=select(sle(height,stopping),sgt(vy,6i32),false);
        let aim:i32=sdiv(sub(0i32,x),{steer}i32);
        let desired:i32=select(slt(aim,-24i32),-24i32,select(sgt(aim,24i32),24i32,aim));
        let side:i32={side};
        let next_vertical:i32=select(select(burn,sgt(fuel,0i32),false),8i32,0i32);
        let next_horizontal:i32=select(sgt(fuel,0i32),side,0i32);
        vx=add(vx,add(horizontal,wind));
        vy=add(vy,sub(2i32,vertical));
        x=add(x,vx);
        height=sub(height,vy);
        fuel=sub(fuel,select(select(sgt(fuel,0i32),select(ne(next_vertical,0i32),true,ne(next_horizontal,0i32)),false),1i32,0i32));
        vertical=next_vertical;
        horizontal=next_horizontal;
        tick=add(tick,1i32);
    }}
    let ax:i32=select(slt(x,0i32),sub(0i32,x),x);
    let avx:i32=select(slt(vx,0i32),sub(0i32,vx),vx);
    let avy:i32=select(slt(vy,0i32),sub(0i32,vy),vy);
    let landed:bool=sle(height,0i32);
    let safe:bool=select(landed,select(sle(ax,48i32),select(sle(avx,6i32),sle(avy,12i32),false),false),false);
    let px:i32=select(sgt(ax,8191i32),8191i32,ax);
    let py:i32=select(sgt(avy,255i32),255i32,avy);
    return or(select(safe,1073741824i32,0i32),or(select(landed,536870912i32,0i32),or(shl(px,16i32),or(shl(py,8i32),fuel))));
}}"#,
        brake = brakes[p.brake],
        margin = p.margin,
        steer = if p.steer == 3 { 16 } else { 8 },
        side = steering[p.steer]
    )
}
fn policies() -> Vec<Policy> {
    let mut all = vec![];
    for brake in 0..8 {
        for margin in [0, 64, 256, 768] {
            for steer in 0..4 {
                all.push(Policy {
                    brake,
                    margin,
                    steer,
                });
            }
        }
    }
    all
}
fn inputs(count: usize, seed: u64) -> Vec<Vec<Value>> {
    let mut rng = Rng::new(seed);
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    while result.len() < count {
        let v = [
            (rng.next_u64() % 1025) as i32 - 512,
            2400 + (rng.next_u64() % 4001) as i32,
            (rng.next_u64() % 33) as i32 - 16,
            8 + (rng.next_u64() % 33) as i32,
        ];
        if seen.insert(v) {
            result.push(
                v.into_iter()
                    .map(|v| Value::new(Type::I32, v as u64))
                    .collect(),
            );
        }
    }
    result
}
fn emit(v: serde_json::Value) {
    println!("{v}");
    io::stdout().flush().unwrap();
}
fn simulation(p: Policy, input: &[Value], trace: bool) -> (u64, Vec<serde_json::Value>) {
    let mut x = input[0].signed() as i32;
    let mut y = input[1].signed() as i32;
    let mut vx = input[2].signed() as i32;
    let mut vy = input[3].signed() as i32;
    let wind = (x + 1024) % 3 - 1;
    let mut fuel = 224;
    let mut vertical = 0;
    let mut horizontal = 0;
    let mut frames = vec![];
    for tick in 0..=256 {
        if trace {
            frames.push(json!([x, y, vx, vy, fuel, vertical, horizontal]));
        }
        if y <= 0 || tick == 256 {
            break;
        }
        let lookahead = vy + 8;
        let stop = match p.brake {
            0 => vy * 4,
            1 => vy * 8,
            2 => vy * vy / 8,
            3 => vy * vy / 12,
            4 => vy * vy / 16,
            5 => lookahead * lookahead / 8,
            6 => lookahead * lookahead / 12,
            _ => lookahead * lookahead / 16,
        } + p.margin;
        let aim = (-x / if p.steer == 3 { 16 } else { 8 }).clamp(-24, 24);
        let side = match p.steer {
            0 => {
                if x < 0 {
                    2
                } else {
                    -2
                }
            }
            1 => {
                if vx < 0 {
                    2
                } else {
                    -2
                }
            }
            _ => {
                if vx < aim {
                    2
                } else if vx > aim {
                    -2
                } else {
                    0
                }
            }
        };
        let next_vertical = if y <= stop && vy > 6 && fuel > 0 {
            8
        } else {
            0
        };
        let next_horizontal = if fuel > 0 { side } else { 0 };
        vx += horizontal + wind;
        vy += 2 - vertical;
        x += vx;
        y -= vy;
        if fuel > 0 && (next_vertical != 0 || next_horizontal != 0) {
            fuel -= 1;
        }
        vertical = next_vertical;
        horizontal = next_horizontal;
    }
    let safe = y <= 0 && x.abs() <= 48 && vx.abs() <= 6 && vy.abs() <= 12;
    let packed = ((safe as u64) << 30)
        | (((y <= 0) as u64) << 29)
        | ((x.unsigned_abs().min(8191) as u64) << 16)
        | ((vy.unsigned_abs().min(255) as u64) << 8)
        | fuel as u64;
    (packed, frames)
}
fn description(p: Policy) -> String {
    let brake = [
        "vy * 4",
        "vy * 8",
        "vy² / 8",
        "vy² / 12",
        "vy² / 16",
        "(vy + 8)² / 8",
        "(vy + 8)² / 12",
        "(vy + 8)² / 16",
    ][p.brake];
    let side = [
        "steer toward pad",
        "oppose sideways velocity",
        "track clamp(-x / 8, -24, 24)",
        "track clamp(-x / 16, -24, 24)",
    ][p.steer];
    format!("brake when height ≤ {brake} + {}\n           and descent speed > 6\nsteering: {side}\nactuator delay: one tick\nfuel limit: 224 burns",p.margin)
}
fn previews(p: Policy, inputs: &[Vec<Value>]) -> Vec<serde_json::Value> {
    inputs
        .iter()
        .take(48)
        .map(|input| {
            let (r, frames) = simulation(p, input, true);
            json!({"frames":frames,"safe":r&(1<<30)!=0})
        })
        .collect()
}
fn verify_native(
    programs: &[Function],
    policies: &[Policy],
    inputs: &[Vec<Value>],
) -> Result<(), String> {
    for (f, p) in programs.iter().zip(policies) {
        let mut e = Evaluator::new(f)?;
        for input in inputs.iter().take(8) {
            let native = simulation(*p, input, false).0;
            let result = e.execute(input, STEPS);
            if result.outcome != Outcome::Completed(Value::new(Type::I32, native)) {
                return Err(format!(
                    "independent simulator disagreement: {p:?} {result:?}"
                ));
            }
        }
    }
    Ok(())
}
fn run(mode: &str, count: usize, seed: u64) -> Result<serde_json::Value, String> {
    let overall = Instant::now();
    let policies = policies();
    let programs: Vec<_> = policies
        .iter()
        .map(|p| parse(&source(*p)))
        .collect::<Result<_, _>>()?;
    let inputs = inputs(count, 0x4c414e444552 ^ seed);
    // All 128 programs are independently checked before either timed backend.
    verify_native(&programs, &policies, &inputs)?;
    emit(
        json!({"kind":"start","mode":mode,"cases":count,"seed":seed,"candidates":programs.len(),"baseline":previews(policies[0],&inputs),"max_ticks":256,"step_budget":STEPS}),
    );
    let clock = Instant::now();
    let mut scored = vec![];
    let mut hashes = vec![];
    let mut best = 0;
    let mut peak = 0;
    let mut kernel = 0.;
    let mut device = "Gremlin CPU interpreter (one thread)".to_string();
    let mut executed_steps = 0u64;
    for (batch, functions) in programs.chunks(16).enumerate() {
        let result = if mode == "gpu" {
            let r = gremlin_cuda::evaluate(&gremlin_cuda::Request {
                functions: functions.to_vec(),
                inputs: inputs.clone(),
                max_steps: STEPS,
                memory_budget: 2 << 30,
            })?;
            peak = peak.max(r.telemetry.allocated_bytes);
            kernel += r.telemetry.kernel_ms;
            device = r.telemetry.device;
            r.executions
        } else {
            functions
                .iter()
                .map(|f| {
                    let mut e = Evaluator::new(f)?;
                    Ok(inputs
                        .iter()
                        .map(|i| e.execute(i, STEPS))
                        .collect::<Vec<_>>())
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        for row in &result {
            let mut safe = 0;
            let mut fuel = 0;
            let mut bytes = Vec::with_capacity(row.len() * 16);
            for r in row {
                match r.outcome {
                    Outcome::Completed(v) => {
                        if v.bits & (1 << 30) != 0 {
                            safe += 1;
                            fuel += v.bits & 255;
                        }
                        executed_steps += r.steps;
                        bytes.extend_from_slice(&v.bits.to_le_bytes());
                        bytes.extend_from_slice(&r.steps.to_le_bytes());
                    }
                    _ => return Err(format!("execution failed: {:?}", r.outcome)),
                }
            }
            scored.push((safe, fuel));
            hashes.push(hash(&bytes));
            let index = scored.len() - 1;
            if scored[index] > scored[best] {
                best = index;
            }
        }
        emit(
            json!({"kind":"progress","mode":mode,"batch":batch+1,"tested":scored.len(),"seconds":clock.elapsed().as_secs_f64(),"safe":scored[best].0,"baseline_safe":scored[0].0,"cases":count,"best":best,"description":description(policies[best]),"source":source(policies[best]),"traces":previews(policies[best],&inputs)}),
        );
    }
    let seconds = clock.elapsed().as_secs_f64();
    // Disjoint holdout, independently simulated and replayed through Gremlin.
    let seen: BTreeSet<_> = inputs
        .iter()
        .map(|i| i.iter().map(|v| v.bits).collect::<Vec<_>>())
        .collect();
    let unseen: Vec<_> = self::inputs(8192, 0x484f4c444f5554 ^ seed)
        .into_iter()
        .filter(|i| !seen.contains(&i.iter().map(|v| v.bits).collect::<Vec<_>>()))
        .take(4096)
        .collect();
    if unseen.len() != 4096 {
        return Err("insufficient disjoint holdout".into());
    }
    let mut evaluator = Evaluator::new(&programs[best])?;
    let mut holdout_safe = 0;
    for input in &unseen {
        let native = simulation(policies[best], input, false).0;
        if evaluator.execute(input, STEPS).outcome
            != Outcome::Completed(Value::new(Type::I32, native))
        {
            return Err("holdout simulator disagreement".into());
        }
        if native & (1 << 30) != 0 {
            holdout_safe += 1;
        }
    }
    let done = json!({"kind":"done","mode":mode,"seconds":seconds,"total_seconds":overall.elapsed().as_secs_f64(),"cases":count,"seed":seed,"programs":programs.len(),"best":best,"safe":scored[best].0,"baseline_safe":scored[0].0,"fuel":scored[best].1,"hash":object_hash(&hashes),"candidate_hash":object_hash(&programs[best]),"peak_bytes":peak,"kernel_ms":kernel,"device":device,"executed_steps":executed_steps,"holdout_safe":holdout_safe,"holdout_cases":4096,"description":description(policies[best]),"source":source(policies[best]),"evidence":"TESTED in this simplified simulator, not a flight-qualified controller"});
    emit(done.clone());
    Ok(done)
}
fn main() {
    let a: Vec<_> = std::env::args().skip(1).collect();
    let result = (|| {
        if a.len() != 4 || !["cpu", "gpu", "both"].contains(&a[0].as_str()) || a[1] != "lander" {
            return Err("usage: landing-lab cpu|gpu|both lander 2048|8192 seed(1..3)".into());
        }
        let count = a[2].parse().map_err(|_| "invalid cases")?;
        let seed = a[3].parse().map_err(|_| "invalid seed")?;
        if ![2048, 8192].contains(&count) || !(1..=3).contains(&seed) {
            return Err("unsupported demo parameters".into());
        }
        if a[0] == "both" {
            let cpu = run("cpu", count, seed)?;
            let gpu = run("gpu", count, seed)?;
            if cpu["hash"] != gpu["hash"] || cpu["candidate_hash"] != gpu["candidate_hash"] {
                return Err("CPU/GPU parity failed".into());
            }
            emit(
                json!({"kind":"comparison","matching":true,"speedup":cpu["seconds"].as_f64().unwrap()/gpu["seconds"].as_f64().unwrap(),"total_speedup":cpu["total_seconds"].as_f64().unwrap()/gpu["total_seconds"].as_f64().unwrap()}),
            );
        } else {
            run(&a[0], count, seed)?;
        }
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        emit(json!({"kind":"error","message":error}));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_replay_matches_every_controller_and_boundary_cases() {
        let all = policies();
        assert_eq!(all.len(), 128);
        let functions: Vec<_> = all.iter().map(|p| parse(&source(*p)).unwrap()).collect();
        let mut cases = inputs(4, 99);
        for values in [
            [0, 0, 0, 0],
            [-512, 1, -16, 40],
            [512, 6400, 16, 8],
            [0, 2400, 0, 40],
        ] {
            cases.push(
                values
                    .iter()
                    .map(|v| Value::new(Type::I32, *v as u64))
                    .collect(),
            );
        }
        verify_native(&functions, &all, &cases).unwrap();
    }
    #[test]
    fn distinct_controller_programs_and_unique_scenarios() {
        let hashes: BTreeSet<_> = policies()
            .iter()
            .map(|p| object_hash(&parse(&source(*p)).unwrap()))
            .collect();
        assert_eq!(hashes.len(), 128);
        let cases = inputs(8192, 123);
        let unique: BTreeSet<_> = cases
            .iter()
            .map(|i| i.iter().map(|v| v.bits).collect::<Vec<_>>())
            .collect();
        assert_eq!(unique.len(), 8192);
    }
}
