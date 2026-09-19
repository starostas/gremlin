#![cfg(feature = "cuda")]
use serde_json::Value;
use std::{fs, process::Command, time::Instant};
#[test]
fn synthesis_parity_watchdog_memory_and_missing_device() {
    let root = std::env::temp_dir().join(format!("gremlin-cuda-synthesis-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let original = include_str!("../../../tests/fixtures/composed_u64.toml");
    let mut states = Vec::new();
    let mut measurements = Vec::new();
    for mode in ["cpu", "cuda", "timeout", "memory", "missing"] {
        let directory = root.join(mode);
        let path = root.join(format!("{mode}.toml"));
        let mut config = original.replace("runs/composed_u64", directory.to_str().unwrap());
        if mode != "cpu" {
            config.push_str(&format!(
                "\n[search.cuda]\nmemory_budget_mb = {}\nwall_timeout_ms = {}\n",
                if mode == "memory" { 1 } else { 1024 },
                if mode == "timeout" { 1 } else { 30000 }
            ));
        }
        if mode == "memory" {
            config = config.replace("random_cases = 64", "random_cases = 1024");
        }
        fs::write(&path, config).unwrap();
        let start = Instant::now();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_gremlin"));
        cmd.args(["synthesize", "--config", path.to_str().unwrap()]);
        if mode == "missing" {
            cmd.env("CUDA_VISIBLE_DEVICES", "");
        }
        let output = cmd.output().unwrap();
        let seconds = start.elapsed().as_secs_f64();
        if mode == "cpu" || mode == "cuda" {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            let checkpoint: Value =
                serde_json::from_slice(&fs::read(directory.join("checkpoint.json")).unwrap())
                    .unwrap();
            states.push(checkpoint["checkpoint"]["state"].clone());
            let report: Value =
                serde_json::from_slice(&fs::read(directory.join("report.json")).unwrap()).unwrap();
            assert_eq!(report["evidence_level"], "E2");
            measurements
                .push(serde_json::json!({"mode":mode,"wall_seconds":seconds,"report":report}));
            let resumed = Command::new(env!("CARGO_BIN_EXE_gremlin"))
                .args([
                    "resume",
                    directory.join("checkpoint.json").to_str().unwrap(),
                ])
                .output()
                .unwrap();
            assert!(
                resumed.status.success(),
                "{}",
                String::from_utf8_lossy(&resumed.stdout)
            );
        } else {
            assert_eq!(
                output.status.code(),
                Some(4),
                "mode={mode} output={}",
                String::from_utf8_lossy(&output.stdout)
            );
            let expected = match mode {
                "timeout" => "wall timeout",
                "memory" => "memory budget",
                _ => "CUDA infrastructure failure",
            };
            assert!(
                String::from_utf8_lossy(&output.stdout).contains(expected),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            let report: Value =
                serde_json::from_slice(&fs::read(directory.join("report.json")).unwrap()).unwrap();
            assert!(report["evidence_level"].is_null());
        }
    }
    assert_eq!(
        states[0], states[1],
        "CPU/CUDA search and PRNG states must agree"
    );
    if let Ok(path) = std::env::var("GREMLIN_CUDA_SYNTH_REPORT") {
        fs::write(path, serde_json::to_vec_pretty(&measurements).unwrap()).unwrap();
    }
    println!("CPU/CUDA synthesis states and E2 agree; watchdog, memory and missing-device failures are explicit");
    fs::remove_dir_all(root).unwrap();
}
