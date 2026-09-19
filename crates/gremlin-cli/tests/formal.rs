use gremlin_core::hash;
use serde_json::Value;
use std::{fs, process::Command};
#[test]
fn binary_e4_and_counterexample_replay_through_isolated_oracle() {
    let dir = std::env::temp_dir().join(format!("gremlin-cli-formal-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let binary = dir.join("target.so");
    let assembly = dir.join("target.s");
    fs::write(&assembly,".text\n.global target\n.type target,@function\ntarget:\nlea 1(%rdi,%rdi,2),%rax\nret\n.size target,.-target\n.section .note.GNU-stack,\"\",@progbits\n").unwrap();
    assert!(Command::new("cc")
        .args(["-shared", "-nostdlib", "-o"])
        .arg(&binary)
        .arg(assembly)
        .status()
        .unwrap()
        .success());
    let config = dir.join("target.toml");
    let original = include_str!("../../../tests/fixtures/composed_u64.toml")
        .replace("kind = \"fixture\"", "kind = \"binary\"");
    fs::write(&config,format!("{original}\n[target.binary]\npath = {:?}\nsha256 = {:?}\narchitecture = \"x86_64\"\nformat = \"elf\"\nsymbol = \"target\"\nabi = \"sysv64\"\nenvironment = \"empty\"\nwall_timeout_ms = 1000\ncpu_seconds = 1\nmemory_mb = 256\n",binary.to_str().unwrap(),hash(&fs::read(&binary).unwrap()))).unwrap();
    for (name, source, code, status) in [
        (
            "equivalent",
            "fn f(x:u64)->u64{return add(mul(x,3u64),1u64);}",
            0,
            "Equivalent",
        ),
        ("wrong", "fn f(x:u64)->u64{return x;}", 6, "Counterexample"),
        (
            "trap",
            "fn f(x:u64)->u64{return udiv(x,0u64);}",
            6,
            "Counterexample",
        ),
    ] {
        let candidate = dir.join(format!("{name}.gremlin"));
        fs::write(&candidate, source).unwrap();
        let report = dir.join(format!("{name}.json"));
        let output = Command::new(env!("CARGO_BIN_EXE_gremlin"))
            .args([
                "verify",
                "--config",
                config.to_str().unwrap(),
                "--candidate",
                candidate.to_str().unwrap(),
                "--solver",
                "/usr/bin/z3",
                "--timeout-ms",
                "5000",
                "--output",
                report.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let r: Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
        assert_eq!(r["status"], status);
        assert_eq!(r["evidence_scope"], "binary");
        assert_eq!(
            r["binary_model"]["binary_hash"],
            hash(&fs::read(&binary).unwrap())
        );
        assert!(r["oracle_identity"]["worker_sha256"].is_string());
        if code == 0 {
            assert_eq!(r["evidence_level"], "E4");
        } else {
            assert!(r["input"].is_array());
            assert!(r["target_observation"].is_object());
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
