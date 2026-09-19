mod compile;
mod gpu;
mod oracle;
mod runs;
mod verify;
use gremlin_core::*;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--cuda-worker"] {
        std::process::exit(gpu::worker());
    }
    if args.as_slice() == ["--oracle-worker"] {
        std::process::exit(gremlin_native::worker_main());
    }
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
        println!("gremlin: program synthesis\nNative artifact: compile --config <toml> --candidate <source> --min-evidence E1|E2|E4 --compiler <clang18> --timeout-ms <n> --output <directory> [--solver <z3>] [--corpus <json>]\nCommands: check <source> | run <source> --args <hex,...> --max-steps <n> --max-call-depth <n> | synthesize --config <toml> | resume <checkpoint.json> | refine --config <toml> --candidate <source> | verify --config <toml> --candidate <source> --solver <path> --timeout-ms <n> --output <json> | verify-reference --reference <source> --candidate <source> --solver <path> --timeout-ms <n> --output <json>");
        return Ok(0);
    }
    match args[0].as_str() {
        "verify" | "verify-reference" => verify::command(args),
        "compile" => compile::command(args),
        "check" if args.len() == 2 => {
            let module = read_module(&args[1])?;
            let f = &module.functions[&module.entry];
            println!(
                "{}",
                serde_json::json!({"status":"valid","signature":f.signature(),"candidate_hash":object_hash(&module),"entry":module.entry,"function_count":module.functions.len()})
            );
            Ok(0)
        }
        "run" if args.len() >= 2 => {
            let module = read_module(&args[1])?;
            let f = &module.functions[&module.entry];
            let mut raw = None;
            let mut budget = 256;
            let mut budget_seen = false;
            let mut call_depth = 64;
            let mut call_depth_seen = false;
            let mut i = 2;
            while i < args.len() {
                let val = args.get(i + 1).ok_or_else(|| input("missing flag value"))?;
                match args[i].as_str() {
                    "--args" if raw.is_none() => raw = Some(val.as_str()),
                    "--max-steps" if !budget_seen => {
                        budget = val.parse().map_err(input)?;
                        budget_seen = true;
                    }
                    "--max-call-depth" if !call_depth_seen => {
                        call_depth = val.parse().map_err(input)?;
                        call_depth_seen = true;
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
            let e = execute_module(&module, &values, budget, call_depth);
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
        "refine" if args.len() == 5 && args[1] == "--config" && args[3] == "--candidate" => {
            let c =
                gremlin_search::Config::parse(&std::fs::read_to_string(&args[2]).map_err(input)?)
                    .map_err(input)?;
            if c.refinement.is_none() {
                return Err(input("refine requires [refinement] configuration"));
            }
            runs::synthesize_with_candidate(c, Some(read_source(&args[4])?))
        }
        "resume" if args.len() == 2 => runs::resume(std::path::Path::new(&args[1])),
        _ => Err(input("unsupported command or arguments; use --help")),
    }
}
fn read_source(path: &str) -> Result<Function, Error> {
    parse(&std::fs::read_to_string(path).map_err(input)?).map_err(input)
}

fn read_module(path: &str) -> Result<Module, Error> {
    parse_module(&std::fs::read_to_string(path).map_err(input)?).map_err(input)
}
