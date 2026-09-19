use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "gremlin-integrations-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
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
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn config(temp: &Temp) -> PathBuf {
    let path = temp.0.join("config.toml");
    fs::write(
        &path,
        include_str!("../../../tests/fixtures/identity_u64.toml")
            .replace("runs/identity_u64", temp.0.join("search").to_str().unwrap()),
    )
    .unwrap();
    path
}
fn load(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
#[test]
fn import_replays_deduplicates_preserves_provenance_and_seeds_search() {
    let temp = Temp::new();
    let config = config(&temp);
    let input = temp.0.join("import.json");
    let document = json!({"schema_version":1,"signature":{"arguments":["u64"],"return_type":"u64"},"cases":[{"input":["0x123456789abcdef0"],"expected":"0x123456789abcdef0","provenance":["external campaign A"]},{"input":["0x123456789abcdef0"],"provenance":["external campaign B"]}]});
    fs::write(&input, serde_json::to_vec(&document).unwrap()).unwrap();
    let first = temp.0.join("first");
    let second = temp.0.join("second");
    success(run(&[
        "import",
        "--config",
        config.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
        "--output",
        first.to_str().unwrap(),
    ]));
    success(run(&[
        "import",
        "--config",
        config.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
        "--corpus",
        first.join("corpus.json").to_str().unwrap(),
        "--output",
        second.to_str().unwrap(),
    ]));
    assert_eq!(
        load(&first.join("corpus.json")),
        load(&second.join("corpus.json"))
    );
    assert_eq!(
        load(&second.join("corpus.json"))["cases"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let provenance = load(&second.join("provenance.json"));
    let records = provenance[0]["records"].as_array().unwrap();
    assert!(records.contains(&json!("external campaign A")));
    assert!(records.contains(&json!("external campaign B")));
    let source = fs::read_to_string(&config).unwrap().replace(
        "[corpus]",
        &format!(
            "[corpus]\ninitial_corpus = {:?}",
            second.join("corpus.json").to_str().unwrap()
        ),
    );
    fs::write(&config, source).unwrap();
    success(run(&["synthesize", "--config", config.to_str().unwrap()]));
    let corpus = load(&temp.0.join("search/corpus.json"));
    assert!(corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["input"][0] == "0x123456789abcdef0"));
    for (n, mut bad) in [document.clone(), document.clone(), document]
        .into_iter()
        .enumerate()
    {
        match n {
            0 => bad["schema_version"] = json!(99),
            1 => bad["cases"][0]["input"][0] = json!("bad"),
            _ => bad["cases"][0]["expected"] = json!("0x0000000000000000"),
        };
        fs::write(&input, serde_json::to_vec(&bad).unwrap()).unwrap();
        let out = temp.0.join(format!("bad{n}"));
        assert_eq!(
            run(&[
                "import",
                "--config",
                config.to_str().unwrap(),
                "--input",
                input.to_str().unwrap(),
                "--output",
                out.to_str().unwrap()
            ])
            .status
            .code(),
            Some(2)
        );
        assert!(!out.exists());
    }
}
fn create(config: &Path, directory: &Path, runs: &str) {
    success(run(&[
        "campaign",
        "create",
        "--tool",
        "libfuzzer",
        "--config",
        config.to_str().unwrap(),
        "--compiler",
        "/usr/bin/clang++-18",
        "--seed",
        "1",
        "--runs",
        runs,
        "--seconds",
        "30",
        "--output",
        directory.to_str().unwrap(),
    ]));
}
fn poll(directory: &Path) -> Value {
    success(run(&[
        "campaign",
        "poll",
        "--directory",
        directory.to_str().unwrap(),
    ]))
}
#[test]
fn external_campaign_lifecycle_export_import_and_unavailable_tools() {
    let temp = Temp::new();
    let config = config(&temp);
    let campaign = temp.0.join("campaign");
    create(&config, &campaign, "30");
    success(run(&[
        "campaign",
        "start",
        "--directory",
        campaign.to_str().unwrap(),
    ]));
    let start = Instant::now();
    let state = loop {
        let state = poll(&campaign);
        if state["state"]["status"] != "running" {
            break state;
        }
        assert!(start.elapsed() < Duration::from_secs(20));
        thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(state["state"]["status"], "completed", "{state}");
    assert!(state["observations"].as_u64().unwrap() >= 2);
    assert!(state["coverage_scope"]
        .as_str()
        .unwrap()
        .contains("no target coverage"));
    let export = temp.0.join("export.json");
    success(run(&[
        "campaign",
        "export",
        "--directory",
        campaign.to_str().unwrap(),
        "--output",
        export.to_str().unwrap(),
    ]));
    let imported = temp.0.join("imported");
    success(run(&[
        "import",
        "--config",
        config.to_str().unwrap(),
        "--input",
        export.to_str().unwrap(),
        "--output",
        imported.to_str().unwrap(),
    ]));
    assert_eq!(
        run(&[
            "campaign",
            "start",
            "--directory",
            campaign.to_str().unwrap()
        ])
        .status
        .code(),
        Some(2)
    );
    let stop = temp.0.join("stop");
    create(&config, &stop, "10000");
    success(run(&[
        "campaign",
        "start",
        "--directory",
        stop.to_str().unwrap(),
    ]));
    success(run(&[
        "campaign",
        "stop",
        "--directory",
        stop.to_str().unwrap(),
    ]));
    assert_eq!(poll(&stop)["state"]["status"], "stopped");
    let unavailable = temp.0.join("unavailable");
    assert_eq!(
        run(&[
            "campaign",
            "create",
            "--tool",
            "libfuzzer",
            "--config",
            config.to_str().unwrap(),
            "--compiler",
            "/missing/clang",
            "--seed",
            "1",
            "--runs",
            "10",
            "--seconds",
            "1",
            "--output",
            unavailable.to_str().unwrap()
        ])
        .status
        .code(),
        Some(4)
    );
    assert!(!unavailable.exists());
}
