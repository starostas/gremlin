use gremlin_core::*;
use gremlin_native::*;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    path: PathBuf,
}
impl Fixture {
    fn build(source: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gremlin-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let c = dir.join("oracle.c");
        let path = dir.join("oracle.so");
        fs::write(&c, source).unwrap();
        assert!(Command::new("cc")
            .args(["-shared", "-fPIC", "-O2", "-o"])
            .arg(&path)
            .arg(c)
            .status()
            .unwrap()
            .success());
        Self { dir, path }
    }
    fn oracle(
        &self,
        symbol: &str,
        arguments: Vec<Type>,
        return_type: Type,
    ) -> Result<BinaryOracle, String> {
        let contract = BinaryContract {
            path: self.path.to_string_lossy().into(),
            sha256: hash(&fs::read(&self.path).unwrap()),
            architecture: "x86_64".into(),
            format: "elf".into(),
            symbol: symbol.into(),
            abi: "sysv64".into(),
            environment: "empty".into(),
            wall_timeout_ms: 2000,
            cpu_seconds: 1,
            memory_mb: 128,
        };
        BinaryOracle::new(
            contract,
            Signature {
                arguments,
                return_type,
            },
            std::path::Path::new(env!("CARGO_BIN_EXE_gremlin")),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).unwrap();
    }
}
#[test]
fn all_integer_abi_types_and_arities() {
    let f=Fixture::build("#include <stdint.h>\n#define F(T,N) T N(T a,T b,T c,T d){return a+b+c+d;}\nF(uint8_t,u8) F(uint16_t,u16) F(uint32_t,u32) F(uint64_t,u64) F(int8_t,i8) F(int16_t,i16) F(int32_t,i32) F(int64_t,i64)\nuint64_t zero(void){return 42;} uint64_t one(uint64_t a){return a;} uint64_t two(uint64_t a,uint64_t b){return a+b;} uint64_t three(uint64_t a,uint64_t b,uint64_t c){return a+b+c;}\nint64_t signed8(int8_t x){return x;} int64_t signed16(int16_t x){return x;} int64_t signed32(int32_t x){return x;}");
    for t in [
        Type::U8,
        Type::U16,
        Type::U32,
        Type::U64,
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
    ] {
        let o = f.oracle(&t.to_string(), vec![t; 4], t).unwrap();
        let values = o
            .observe(&[vec![
                Value::new(t, 2),
                Value::new(t, 3),
                Value::new(t, 4),
                Value::new(t, 5),
            ]])
            .unwrap();
        assert_eq!(values, [Value::new(t, 14)]);
    }
    for (n, name) in ["zero", "one", "two", "three"].into_iter().enumerate() {
        let o = f.oracle(name, vec![Type::U64; n], Type::U64).unwrap();
        assert_eq!(
            o.observe(&[vec![Value::new(Type::U64, 7); n]]).unwrap()[0].bits,
            if n == 0 { 42 } else { 7 * n as u64 }
        );
    }
    for (t, sym) in [
        (Type::I8, "signed8"),
        (Type::I16, "signed16"),
        (Type::I32, "signed32"),
    ] {
        assert_eq!(
            f.oracle(sym, vec![t], Type::I64)
                .unwrap()
                .observe(&[vec![Value::new(t, t.mask())]])
                .unwrap()[0]
                .signed(),
            -1
        );
    }
}
#[test]
fn forbidden_syscalls_crash_timeout_missing_symbol_and_restart() {
    let f=Fixture::build("#include <stdint.h>\n#include <unistd.h>\n#include <sys/socket.h>\n#include <fcntl.h>\nuint64_t good(uint64_t x){return x+1;} uint64_t crash(uint64_t x){__builtin_trap();} uint64_t forever(uint64_t x){for(;;)__asm__ volatile(\"\");} uint64_t network(uint64_t x){return socket(2,1,0);} uint64_t file(uint64_t x){return open(\"/etc/passwd\",O_RDONLY);} uint64_t output(uint64_t x){return write(1,\"bad\",3);} uint64_t stateful(uint64_t x){static uint64_t n;return n++;}\n");
    let input = [vec![Value::new(Type::U64, 0)]];
    for sym in ["crash", "forever", "network", "file", "output", "stateful"] {
        let error = f
            .oracle(sym, vec![Type::U64], Type::U64)
            .unwrap()
            .observe(&input)
            .unwrap_err();
        eprintln!("{sym}: {error}");
        assert!(!error.is_empty());
        assert_eq!(
            f.oracle("good", vec![Type::U64], Type::U64)
                .unwrap()
                .observe(&input)
                .unwrap()[0]
                .bits,
            1
        );
    }
    assert!(f.oracle("missing", vec![], Type::U64).is_err());
}
#[test]
fn constructors_run_inside_isolation() {
    let f=Fixture::build("#include <unistd.h>\n__attribute__((constructor)) void init(void){write(1,\"bad\",3);} unsigned long target(unsigned long x){return x;}\n");
    assert!(f
        .oracle("target", vec![Type::U64], Type::U64)
        .unwrap()
        .observe(&[vec![Value::new(Type::U64, 0)]])
        .is_err());
}

#[test]
fn wall_watchdog_and_memory_limits() {
    let f=Fixture::build("#include <stdlib.h>\nunsigned long spin(unsigned long x){for(;;)__asm__ volatile(\"\");} unsigned long memory(unsigned long x){volatile char *p=malloc(1024UL*1024*1024); *p=1; return *p;}\n");
    let base = f.oracle("spin", vec![Type::U64], Type::U64).unwrap();
    let mut contract = base.identity().contract.clone();
    contract.wall_timeout_ms = 100;
    contract.cpu_seconds = 5;
    let oracle = BinaryOracle::new(
        contract,
        Signature {
            arguments: vec![Type::U64],
            return_type: Type::U64,
        },
        std::path::Path::new(env!("CARGO_BIN_EXE_gremlin")),
    )
    .unwrap();
    assert!(oracle
        .observe(&[vec![Value::new(Type::U64, 0)]])
        .unwrap_err()
        .contains("wall-clock timeout"));
    assert!(f
        .oracle("memory", vec![Type::U64], Type::U64)
        .unwrap()
        .observe(&[vec![Value::new(Type::U64, 0)]])
        .is_err());
}

#[test]
fn affine_counterexample_survives_resume() {
    let f = Fixture::build(include_str!("../../../tests/fixtures/affine.c"));
    let source = f.dir.join("wrong.gremlin");
    fs::write(&source, "fn f(x:u64)->u64{return 0x000000007f6e5d4bu64;}").unwrap();
    let target = f.oracle("affine_u64", vec![Type::U64], Type::U64).unwrap();
    let mut config =
        gremlin_search::Config::parse(include_str!("../../../examples/affine.toml")).unwrap();
    config.target.kind = "binary".into();
    config.target.binary = Some(target.identity().contract.clone());
    config.output.directory = f.dir.join("run").to_string_lossy().into();
    config.search.population = 16;
    config.search.elite = 2;
    config.search.generations = 1;
    config.corpus.holdout_cases = 4;
    config.refinement = Some(gremlin_search::RefinementConfig {
        max_rounds: 1,
        differential_cases: 4,
        initial_inputs: vec![vec!["0x0000000000000000".into()]],
    });
    // JSON is valid input to this test helper only; CLI config remains TOML.
    let config_path = f.dir.join("config.toml");
    let mut text = include_str!("../../../examples/affine.toml")
        .replace("kind = \"fixture\"", "kind = \"binary\"")
        .replace("population = 256", "population = 16")
        .replace("elite = 8", "elite = 2")
        .replace("generations = 1000", "generations = 1")
        .replace("holdout_cases = 256", "holdout_cases = 4")
        .replace("runs/affine", &config.output.directory);
    let b = config.target.binary.as_ref().unwrap();
    text.push_str(&format!("\n[target.binary]\npath = {:?}\nsha256 = {:?}\narchitecture = \"x86_64\"\nformat = \"elf\"\nsymbol = \"affine_u64\"\nabi = \"sysv64\"\nenvironment = \"empty\"\nwall_timeout_ms = 2000\ncpu_seconds = 1\nmemory_mb = 128\n[refinement]\nmax_rounds = 1\ndifferential_cases = 4\ninitial_inputs = [[\"0x0000000000000000\"]]\n",b.path,b.sha256));
    fs::write(&config_path, text).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_gremlin"))
        .args(["refine", "--config"])
        .arg(config_path)
        .arg("--candidate")
        .arg(source)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(3),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dir = f.dir.join("run");
    let before: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("checkpoint.json")).unwrap()).unwrap();
    let corpus: Corpus =
        serde_json::from_slice(&fs::read(dir.join("corpus.json")).unwrap()).unwrap();
    assert!(corpus
        .cases
        .iter()
        .any(|c| c.input == ["0x0000000000000001"] && c.expected == "0x000000007f6e5d52"));
    let output = Command::new(env!("CARGO_BIN_EXE_gremlin"))
        .arg("resume")
        .arg(dir.join("checkpoint.json"))
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let after: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("checkpoint.json")).unwrap()).unwrap();
    assert_eq!(
        before["checkpoint"]["corpus"],
        after["checkpoint"]["corpus"]
    );
    assert_eq!(before["checkpoint"]["state"], after["checkpoint"]["state"]);
}

#[test]
fn discover_affine_from_isolated_observations() {
    let fixture = Fixture::build(include_str!("../../../tests/fixtures/affine.c"));
    let oracle = fixture
        .oracle("affine_u64", vec![Type::U64], Type::U64)
        .unwrap();
    let mut config =
        gremlin_search::Config::parse(include_str!("../../../examples/affine.toml")).unwrap();
    config.search.enumeration_depth = 3;
    config.search.enumeration_proposals = 128;
    let seeds = seed_inputs(&config.signature(), 1, 8);
    let inputs: Vec<_> = seeds.iter().map(|(a, _)| a.clone()).collect();
    let outputs = oracle.observe(&inputs).unwrap();
    let mut corpus = Corpus::new(oracle.target_identity("affine_u64"));
    for ((args, record), output) in seeds.into_iter().zip(outputs) {
        corpus
            .add(args.iter().map(|v| v.hex()).collect(), output.hex(), record)
            .unwrap();
    }
    let engine = gremlin_search::Engine::new(config.search.clone(), &corpus).unwrap();
    let mut state = engine.initialize(1).unwrap();
    while !state.best.fitness.matches() && state.generation < config.search.generations {
        engine.advance(&mut state).unwrap();
    }
    assert!(state.best.fitness.matches());
    let holdout = holdout_inputs(&config.signature(), &corpus, 0x76543210, 64);
    let expected = oracle.observe(&holdout.inputs).unwrap();
    let mut evaluator = Evaluator::new(&state.best.genome.lower(&config.signature())).unwrap();
    for (args, value) in holdout.inputs.iter().zip(expected) {
        assert_eq!(
            evaluator.execute(args, 256).outcome,
            Outcome::Completed(value)
        );
    }
}
