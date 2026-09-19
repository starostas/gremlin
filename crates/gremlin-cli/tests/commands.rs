use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "gremlin-cli-test-{}-{}",
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
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gremlin"))
        .args(args)
        .output()
        .unwrap()
}
fn json(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout).unwrap()
}
fn config(temp: &Temp, fixture: &str, holdout: usize, steps: usize) -> PathBuf {
    let source = fs::read_to_string(format!("../../tests/fixtures/{fixture}.toml"))
        .unwrap()
        .replace("population = 256", "population = 16")
        .replace("elite = 8", "elite = 2")
        .replace("generations = 1000", "generations = 1")
        .replace("holdout_cases = 256", &format!("holdout_cases = {holdout}"))
        .replace("max_steps = 256", &format!("max_steps = {steps}"))
        .replace(
            &format!("runs/{fixture}"),
            temp.0.join("run").to_str().unwrap(),
        );
    let path = temp.0.join("config.toml");
    fs::write(&path, source).unwrap();
    path
}
#[test]
fn help_input_and_execution_exit_codes() {
    assert!(run(&["--help"]).status.success());
    assert_eq!(run(&["unknown"]).status.code(), Some(2));
    let temp = Temp::new();
    let source = temp.0.join("test.gremlin");
    fs::write(&source, "fn f() -> u8 {return udiv(1u8,0u8);}").unwrap();
    let path = source.to_str().unwrap();
    assert_eq!(run(&["check", path]).status.code(), Some(0));
    assert_eq!(run(&["run", path]).status.code(), Some(5));
    fs::write(&source, "fn f(x:u64)->u64{return x;}").unwrap();
    let output = run(&[
        "run",
        path,
        "--args",
        "0xffffffffffffffff",
        "--max-steps",
        "1",
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json(&output)["value"], "0xffffffffffffffff");
    assert_eq!(
        run(&[
            "run",
            path,
            "--args",
            "0xffffffffffffffff",
            "--max-steps",
            "0"
        ])
        .status
        .code(),
        Some(5)
    );
    assert_eq!(run(&["run", path, "--args", "0xff"]).status.code(), Some(2));
    assert_eq!(
        run(&["run", path, "--args", ",0xffffffffffffffff"])
            .status
            .code(),
        Some(2)
    );
}
#[test]
fn artifacts_resume_and_integrity() {
    let temp = Temp::new();
    let config = config(&temp, "increment_u64", 16, 256);
    let output = run(&["synthesize", "--config", config.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let dir = temp.0.join("run");
    let report: Value =
        serde_json::from_slice(&fs::read(dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["evidence_level"], "E2");
    assert_eq!(report["label"], "TESTED");
    assert_eq!(report["holdout"]["actual_count"], 16);
    let checkpoint = dir.join("checkpoint.json");
    let before: Value = serde_json::from_slice(&fs::read(&checkpoint).unwrap()).unwrap();
    let resumed = run(&["resume", checkpoint.to_str().unwrap()]);
    assert_eq!(
        resumed.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&resumed.stdout),
        String::from_utf8_lossy(&resumed.stderr)
    );
    let after: Value = serde_json::from_slice(&fs::read(&checkpoint).unwrap()).unwrap();
    assert_eq!(before["checkpoint"]["state"], after["checkpoint"]["state"]);
    assert_eq!(
        run(&["synthesize", "--config", config.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    let integrity: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(dir.join("integrity.json")).unwrap()).unwrap();
    for (name, h) in integrity {
        assert_eq!(gremlin_core::hash(&fs::read(dir.join(name)).unwrap()), h);
    }
    fs::write(dir.join("corpus.json"), b"{}").unwrap();
    assert_eq!(
        run(&["resume", checkpoint.to_str().unwrap()]).status.code(),
        Some(2)
    );
}
#[test]
fn exhaustion_disabled_holdout_and_bad_configuration() {
    let temp = Temp::new();
    let c = config(&temp, "increment_u64", 0, 1);
    let output = run(&["synthesize", "--config", c.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(3));
    assert!(json(&output)["evidence_level"].is_null());
    let temp = Temp::new();
    let c = config(&temp, "identity_u64", 0, 256);
    let output = run(&["synthesize", "--config", c.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(json(&output)["evidence_level"], "E1");
    fs::write(&c, "unknown = 1").unwrap();
    assert_eq!(
        run(&["synthesize", "--config", c.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
}
