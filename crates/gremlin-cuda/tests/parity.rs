#[cfg(not(feature = "cuda"))]
#[test]
fn unavailable_is_explicit() {
    assert!(gremlin_cuda::evaluate(&gremlin_cuda::Request {
        functions: vec![],
        inputs: vec![],
        max_steps: 1,
        memory_budget: 1
    })
    .unwrap_err()
    .contains("unavailable"));
}
#[cfg(feature = "cuda")]
#[test]
fn ten_thousand_program_cpu_gpu_parity() {
    use gremlin_core::{parse, Evaluator, Op, Rng, Type, Value};
    use gremlin_cuda::{evaluate, Request};
    let mut rng = Rng::new(0xD3);
    let mut program_count = 0;
    let mut comparisons = 0;
    let mut measured = Vec::new();
    for ty in [
        Type::U8,
        Type::U16,
        Type::U32,
        Type::U64,
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
    ] {
        let compare = if ty.signed() { "slt" } else { "ult" };
        let valid: Vec<_> = Op::ALL
            .into_iter()
            .filter(|op| *op == Op::Select || op.result(&vec![ty; op.arity()]).is_ok())
            .collect();
        let functions:Vec<_>=(0..1250).map(|n| {
            let v=Value::new(ty,rng.next_u64());
            let literal=format!("{}{}",if ty.signed(){v.signed()}else{v.bits as i128},ty);
            let op=valid[n%valid.len()];
            let expr=match op {Op::Not=>"not(x)".into(),Op::Select=>format!("select({compare}(x,y),x,y)"),_=>{
                let e=format!("{op}(x,y)");
                if op.result(&[ty,ty]).unwrap()==Type::Bool {format!("select({e},x,y)")}else{e}
            }};
            let body=match n%5 {
                0=>format!("return xor({expr},{literal});"),
                1=>format!("if {compare}(x,y) {{ return {expr}; }} else {{ return {literal}; }}"),
                2=>format!("let mut a:{ty}=x; let mut i:{ty}=0{ty}; while {compare}(i,y) {{ if eq(i,8{ty}) {{ break; }} a=add(a,{expr}); i=add(i,1{ty}); }} return a;"),
                3=>format!("let mut a:{ty}=x; loop {{ a=xor(a,{literal}); if eq(a,y) {{break;}} }} return a;"),
                _=>format!("return add({expr},{literal});")
            };
            parse(&format!("fn f(x:{ty},y:{ty})->{ty}{{{body}}}")).unwrap()
        }).collect();
        program_count += functions.len();
        for cases in [1, 31, 32, 33, 65] {
            let inputs: Vec<_> = (0..cases)
                .map(|n| {
                    let (a, b) = match n {
                        0 => (0, 0),
                        1 => (1, 0),
                        2 => (1 << (ty.width() - 1), ty.mask()),
                        3 => (ty.mask(), 1),
                        4 => (1, ty.width() as u64),
                        5 => (1, ty.width() as u64 + 1),
                        _ => (rng.next_u64(), rng.next_u64()),
                    };
                    vec![Value::new(ty, a), Value::new(ty, b)]
                })
                .collect();
            for max_steps in [0, 1, 64] {
                let result = evaluate(&Request {
                    functions: functions.clone(),
                    inputs: inputs.clone(),
                    max_steps,
                    memory_budget: 1 << 30,
                })
                .unwrap();
                let cpu_started = std::time::Instant::now();
                for (n, (function, executions)) in
                    functions.iter().zip(&result.executions).enumerate()
                {
                    let mut cpu = Evaluator::new(function).unwrap();
                    for (input, gpu) in inputs.iter().zip(executions) {
                        assert_eq!(&cpu.execute(input,max_steps),gpu,"type={ty} program={n} cases={cases} budget={max_steps} input={input:?}");
                        comparisons += 1;
                    }
                }
                measured.push(serde_json::json!({"type":ty,"cases":cases,"budget":max_steps,"programs":functions.len(),"cpu_comparison_ms":cpu_started.elapsed().as_secs_f64()*1000.,"telemetry":result.telemetry}));
            }
        }
    }
    assert_eq!(program_count, 10000);
    let report =
        serde_json::json!({"programs":program_count,"comparisons":comparisons,"batches":measured});
    if let Ok(path) = std::env::var("GREMLIN_CUDA_PARITY_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!("CUDA parity passed: {program_count} seeded programs, {comparisons} outcome/step comparisons");
}
