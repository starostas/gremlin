mod runs;
use gremlin_core::*;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match command(&args) {
        Ok(code) => code,
        Err((code, e)) => {
            println!("{}", serde_json::json!({"error":e}));
            code
        }
    };
    std::process::exit(code);
}
type Error = (i32, String);
fn input(e: impl ToString) -> Error {
    (2, e.to_string())
}
fn command(args: &[String]) -> Result<i32, Error> {
    if args.is_empty() || matches!(args[0].as_str(), "--help" | "-h" | "help") {
        println!("gremlin: CPU program synthesis\nCommands: check <source> | run <source> --args <hex,...> --max-steps <n> | synthesize --config <toml> | resume <checkpoint.json>");
        return Ok(0);
    }
    match args[0].as_str() {
        "check" if args.len() == 2 => {
            let f = read_source(&args[1])?;
            println!(
                "{}",
                serde_json::json!({"status":"valid","signature":f.signature(),"candidate_hash":hash(&f.canonical_bytes().map_err(input)?)})
            );
            Ok(0)
        }
        "run" if args.len() >= 2 => {
            let f = read_source(&args[1])?;
            let mut raw = None;
            let mut budget = 256;
            let mut budget_seen = false;
            let mut i = 2;
            while i < args.len() {
                let val = args.get(i + 1).ok_or_else(|| input("missing flag value"))?;
                match args[i].as_str() {
                    "--args" if raw.is_none() => raw = Some(val.as_str()),
                    "--max-steps" if !budget_seen => {
                        budget = val.parse().map_err(input)?;
                        budget_seen = true;
                    }
                    _ => return Err(input("unknown or duplicate run flag")),
                }
                i += 2;
            }
            let parts: Vec<_> = match raw {
                None | Some("") => vec![],
                Some(s) => s.split(',').collect(),
            };
            if parts.len() != f.parameters.len() {
                return Err(input("argument count mismatch"));
            }
            let values = parts
                .iter()
                .zip(&f.parameters)
                .map(|(s, p)| Value::from_hex(p.ty, s))
                .collect::<Result<Vec<_>, _>>()
                .map_err(input)?;
            let e = execute(&f, &values, budget);
            let code = match &e.outcome {
                Outcome::Completed(_) => 0,
                Outcome::Invalid(_) => 2,
                _ => 5,
            };
            let outcome = match &e.outcome {
                Outcome::Completed(v) => {
                    serde_json::json!({"status":"completed","type":v.ty,"value":v.hex(),"steps":e.steps})
                }
                _ => serde_json::to_value(&e).map_err(|e| (4, e.to_string()))?,
            };
            println!("{outcome}");
            Ok(code)
        }
        "synthesize" if args.len() == 3 && args[1] == "--config" => {
            let s = std::fs::read_to_string(&args[2]).map_err(input)?;
            let c = gremlin_search::Config::parse(&s).map_err(input)?;
            runs::synthesize(c)
        }
        "resume" if args.len() == 2 => runs::resume(std::path::Path::new(&args[1])),
        _ => Err(input("unsupported command or arguments; use --help")),
    }
}
fn read_source(path: &str) -> Result<Function, Error> {
    parse(&std::fs::read_to_string(path).map_err(input)?).map_err(input)
}
