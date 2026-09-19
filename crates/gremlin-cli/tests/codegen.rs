use gremlin_codegen::artifact;
use gremlin_core::*;
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "gremlin-codegen-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn check(source: &str, inputs: Vec<Vec<Value>>, budget: u64) {
    let temp = Temp::new();
    let f = parse(source).unwrap();
    let artifact =
        artifact::compile(&f, budget, Path::new("/usr/bin/clang-18"), &temp.0, 30000).unwrap();
    let observed = artifact::observe(
        &artifact,
        &f.signature(),
        &inputs,
        Path::new(env!("CARGO_BIN_EXE_gremlin")),
    )
    .unwrap();
    let mut cpu = Evaluator::new(&f).unwrap();
    for (input, native) in inputs.iter().zip(observed.outcomes) {
        assert_eq!(
            cpu.execute(input, budget).outcome,
            native,
            "source={source} input={input:?} budget={budget}"
        );
    }
}
#[test]
fn all_widths_and_operators_match_isolated_native_artifacts() {
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
        for op in Op::ALL {
            let expr = if op == Op::Select {
                "select(eq(x,y),x,y)".to_string()
            } else {
                let Ok(result) = op.result(&vec![ty; op.arity()]) else {
                    continue;
                };
                let expr = if op == Op::Not {
                    "not(x)".into()
                } else {
                    format!("{op}(x,y)")
                };
                if result == Type::Bool {
                    format!("select({expr},x,y)")
                } else {
                    expr
                }
            };
            let source = format!("fn f(x:{ty},y:{ty})->{ty}{{return {expr};}}");
            let inputs = [
                (0, 0),
                (1 << (ty.width() - 1), ty.mask()),
                (ty.mask(), 1),
                (1, ty.width() as u64),
                (ty.mask(), ty.width() as u64 + 1),
                (13, 3),
            ]
            .into_iter()
            .map(|(a, b)| vec![Value::new(ty, a), Value::new(ty, b)])
            .collect();
            check(&source, inputs, 64);
        }
    }
}
#[test]
fn cfg_budget_eager_traps_and_simultaneous_edges() {
    check("fn f(x:u8,y:u8)->u8{let mut a:u8=x;let mut b:u8=y;let mut i:u8=0u8;while ult(i,3u8){let old:u8=a;a=b;b=old;i=add(i,1u8);}return a;}",vec![vec![Value::new(Type::U8,17),Value::new(Type::U8,42)]],64);
    check(
        "fn f(x:u8)->u8{loop {if eq(x,0u8){return x;}}}",
        vec![vec![Value::new(Type::U8, 0)], vec![Value::new(Type::U8, 1)]],
        7,
    );
    check(
        "fn f(x:u64)->u64{return select(eq(x,x),x,udiv(x,0u64));}",
        vec![vec![Value::new(Type::U64, 1)]],
        64,
    );
    for budget in [0, 1, 2] {
        check(
            "fn f(x:u64)->u64{return add(x,1u64);}",
            vec![vec![Value::new(Type::U64, u64::MAX)]],
            budget,
        );
    }
}
#[test]
fn explicit_evidence_gate_prevents_compiling_bad_or_rewritten_candidate() {
    use std::process::Command;
    let temp = Temp::new();
    let config = temp.0.join("config.toml");
    fs::write(
        &config,
        include_str!("../../../tests/fixtures/identity_u64.toml"),
    )
    .unwrap();
    let source = temp.0.join("candidate.gremlin");
    fs::write(&source, "fn f(x:u64)->u64{return add(x,1u64);}").unwrap();
    let directory = temp.0.join("output");
    let args = [
        "compile",
        "--config",
        config.to_str().unwrap(),
        "--candidate",
        source.to_str().unwrap(),
        "--compiler",
        "/usr/bin/clang-18",
        "--timeout-ms",
        "30000",
        "--output",
        directory.to_str().unwrap(),
    ];
    let missing = Command::new(env!("CARGO_BIN_EXE_gremlin"))
        .args(args)
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
    assert!(!directory.exists());
    let failed = Command::new(env!("CARGO_BIN_EXE_gremlin"))
        .args(args)
        .args(["--min-evidence", "E2"])
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(6));
    assert!(!directory.join("candidate.ll").exists());
    assert!(!directory.join("candidate.so").exists());
}
#[test]
fn compiler_failures_are_not_artifact_evidence() {
    use std::os::unix::fs::PermissionsExt;
    let temp = Temp::new();
    let f = parse("fn f(x:u64)->u64{return x;}").unwrap();
    assert!(
        artifact::compile(&f, 64, &temp.0.join("missing"), &temp.0, 100)
            .unwrap_err()
            .contains("unavailable")
    );
    let compiler = temp.0.join("clang");
    fs::write(&compiler,"#!/bin/sh\nif [ \"$1\" = '--version' ]; then echo 'clang version 18.1.3'; exit 0; fi\nsleep 10\n").unwrap();
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(artifact::compile(&f, 64, &compiler, &temp.0, 100)
        .unwrap_err()
        .contains("wall timeout"));
    assert!(!temp.0.join("candidate.so").exists());
}
