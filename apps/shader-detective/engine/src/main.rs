use gremlin_core::*;
use gremlin_search::*;
use std::{
    collections::BTreeSet,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Instant,
};

const MASK: u32 = 0x0055aa33;
const BIAS: u32 = 0x00102030;
const WIDTH: usize = 128;
const HEIGHT: usize = 88;
fn oracle(x: u32, preset: &str) -> u32 {
    let mixed = x.rotate_left(8) ^ MASK;
    if preset == "afterglow" {
        mixed.wrapping_add(BIAS)
    } else {
        mixed
    }
}
fn picture() -> Vec<u32> {
    (0..WIDTH * HEIGHT)
        .map(|i| {
            let x = (i % WIDTH) as f64 / WIDTH as f64;
            let y = (i / WIDTH) as f64 / HEIGHT as f64;
            let (r, g, b) = if (x - 0.69).powi(2) + (y - 0.30).powi(2) < 0.12f64.powi(2) {
                (255, 224, 126)
            } else if y > 0.58 + 0.11 * (x * 11.).sin() + 0.055 * (x * 29.).cos() {
                if y > 0.78 {
                    (
                        (35. + x * 54.) as u32,
                        (36. + y * 58.) as u32,
                        (85. + x * 68.) as u32,
                    )
                } else {
                    (33, 48 + (x * 38.) as u32, 81 + (y * 45.) as u32)
                }
            } else {
                (
                    (229. - y * 110.) as u32,
                    (97. + x * 54. + y * 40.) as u32,
                    (111. + y * 110.) as u32,
                )
            };
            (r << 16) | (g << 8) | b
        })
        .collect()
}
#[derive(Default)]
struct Metrics {
    batches: u64,
    host_ms: f64,
    kernel_ms: f64,
    bytes: u64,
    device: String,
}
struct Gpu {
    metrics: Arc<Mutex<Metrics>>,
}
impl Backend for Gpu {
    fn evaluate(
        &self,
        functions: &[Function],
        inputs: &[Vec<Value>],
        max_steps: u64,
    ) -> Result<Vec<Vec<Execution>>, String> {
        let evaluated = gremlin_cuda::evaluate(&gremlin_cuda::Request {
            functions: functions.to_vec(),
            inputs: inputs.to_vec(),
            max_steps,
            memory_budget: 2 * 1024 * 1024 * 1024,
        })?;
        let mut m = self.metrics.lock().map_err(|_| "metrics lock poisoned")?;
        m.batches += 1;
        m.host_ms += evaluated.telemetry.host_total_ms;
        m.kernel_ms += evaluated.telemetry.kernel_ms;
        m.bytes = m.bytes.max(evaluated.telemetry.allocated_bytes);
        m.device = evaluated.telemetry.device;
        Ok(evaluated.executions)
    }
}
fn emit(value: serde_json::Value) {
    println!("{value}");
    io::stdout().flush().unwrap();
}
fn execute_image(f: &Function, pixels: &[u32]) -> Result<Vec<u32>, String> {
    let mut e = Evaluator::new(f)?;
    pixels
        .iter()
        .map(
            |x| match e.execute(&[Value::new(Type::U32, *x as u64)], 64).outcome {
                Outcome::Completed(v) => Ok(v.bits as u32 & 0xffffff),
                other => Err(format!("preview execution: {other:?}")),
            },
        )
        .collect()
}
fn run(mode: &str, preset: &str, count: usize, seed: u64) -> Result<(), String> {
    let pixels = picture();
    let target: Vec<_> = pixels
        .iter()
        .map(|x| oracle(*x, preset) & 0xffffff)
        .collect();
    let mut rng = Rng::new(0x534841444552 ^ seed);
    let mut training = BTreeSet::from([0u32, 1, u32::MAX, 0x80000000]);
    while training.len() < count {
        training.insert(rng.next_u64() as u32);
    }
    let mut corpus = Corpus::new(TargetIdentity {
        name: format!("shader-{preset}"),
        signature: Signature {
            arguments: vec![Type::U32],
            return_type: Type::U32,
        },
        contract: "pure packed-color transformation; demo oracle".into(),
        implementation_fingerprint: "shader-detective-v1".into(),
    });
    // Sorted insertion avoids quadratic shifts while retaining canonical provenance.
    for x in &training {
        corpus.add(
            vec![format!("0x{x:08x}")],
            format!("0x{:08x}", oracle(*x, preset)),
            "seeded training color".into(),
        )?;
    }
    let config = SearchConfig {
        comparator: ComparatorConfig::default(),
        cuda: None,
        population: 256,
        generations: 80,
        max_instructions: 12,
        max_steps: 64,
        elite: 8,
        tournament_size: 4,
        operators: vec![Op::Rotl, Op::Xor, Op::Add],
        constants: vec![
            "8u32".into(),
            format!("0x{MASK:08x}u32"),
            format!("0x{BIAS:08x}u32"),
        ],
        enumeration_depth: 3,
        enumeration_proposals: 192,
        max_blocks: 1,
        structural_mutation_percent: 0,
        loop_bound: 0,
    };
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    let mut engine = Engine::new(config.clone(), &corpus)?;
    if mode == "gpu" {
        engine = engine.with_backend(Box::new(Gpu {
            metrics: metrics.clone(),
        }));
    }
    emit(
        serde_json::json!({"kind":"start","mode":mode,"preset":preset,"seed":seed,"cases":count,"population":256,"generation_limit":80,"width":WIDTH,"height":HEIGHT,"input":pixels,"target":target,"corpus_hash":corpus.content_hash()?}),
    );
    let clock = Instant::now();
    let mut state = engine.initialize(seed)?;
    let mut last_hash = String::new();
    loop {
        let f = state.best.genome.lower(&corpus.target.signature);
        let hash = object_hash(&f);
        let changed = hash != last_hash;
        emit(
            serde_json::json!({"kind":"progress","mode":mode,"generation":state.generation,"cases":count,"mismatches":state.best.fitness.mismatching_completed_case_count,"bit_errors":state.best.fitness.summed_bit_error,"elapsed_seconds":clock.elapsed().as_secs_f64(),"evaluations":state.evaluation_count,"program":if changed {Some(print_source(&f)?)} else {None},"preview":if changed {Some(execute_image(&f,&pixels)?)} else {None}}),
        );
        last_hash = hash;
        if state.best.fitness.matches() || state.generation >= config.generations {
            break;
        }
        engine.advance(&mut state)?;
    }
    let search_seconds = clock.elapsed().as_secs_f64();
    let f = state.best.genome.lower(&corpus.target.signature);
    let mut evaluator = Evaluator::new(&f)?;
    let mut unseen = BTreeSet::new();
    let mut holdout = Rng::new(0x484f4c444f5554 ^ seed);
    while unseen.len() < 4096 {
        let x = holdout.next_u64() as u32;
        if !training.contains(&x) {
            unseen.insert(x);
        }
    }
    let mut mismatches = 0;
    for x in unseen {
        if evaluator
            .execute(&[Value::new(Type::U32, x as u64)], 64)
            .outcome
            != Outcome::Completed(Value::new(Type::U32, oracle(x, preset) as u64))
        {
            mismatches += 1;
        }
    }
    let preview = execute_image(&f, &pixels)?;
    let image_matches = preview == target;
    let m = metrics.lock().map_err(|_| "metrics lock poisoned")?;
    let success = state.best.fitness.matches() && mismatches == 0 && image_matches;
    emit(
        serde_json::json!({"kind":"done","mode":mode,"success":success,"generation":state.generation,"search_seconds":search_seconds,"holdout_cases":4096,"holdout_mismatches":mismatches,"image_matches":image_matches,"program":print_source(&f)?,"candidate_hash":hash(&f.canonical_bytes()?),"search_state_hash":object_hash(&state),"preview":preview,"evaluations":state.evaluation_count,"device":if mode=="gpu" {m.device.clone()}else{"CPU reference interpreter".into()},"gpu_batches":m.batches,"gpu_host_ms":m.host_ms,"gpu_kernel_ms":m.kernel_ms,"peak_device_bytes":m.bytes,"evidence":"TESTED on training + independent holdout; not a formal proof"}),
    );
    if !success {
        return Err("search budget exhausted or validation mismatch".into());
    }
    Ok(())
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = (|| {
        if args.len() != 4 {
            return Err(
                "usage: shader-detective cpu|gpu|both aurora|afterglow 8192|32768 seed(1..3)"
                    .into(),
            );
        }
        let mode = &args[0];
        let preset = &args[1];
        let cases = args[2].parse::<usize>().map_err(|_| "invalid cases")?;
        let seed = args[3].parse::<u64>().map_err(|_| "invalid seed")?;
        if !["cpu", "gpu", "both"].contains(&mode.as_str())
            || !["aurora", "afterglow"].contains(&preset.as_str())
            || ![8192, 32768].contains(&cases)
            || !(1..=3).contains(&seed)
        {
            return Err("unsupported demo parameters".into());
        }
        if mode == "both" {
            run("cpu", preset, cases, seed)?;
            run("gpu", preset, cases, seed)
        } else {
            run(mode, preset, cases, seed)
        }
    })();
    if let Err(error) = result {
        emit(serde_json::json!({"kind":"error","message":error}));
        std::process::exit(1);
    }
}
