mod agent;
mod config;
mod error;
mod json;
mod llm;
mod memory;
mod tools;
mod trace;

use agent::AgentRuntime;
use config::RuntimeConfig;
use error::{AxonError, Result};
use llm::LlmClient;
use memory::MemoryStore;
use std::env;
use std::path::PathBuf;
use std::process;
use trace::RunTrace;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    if let Err(err) = run_cli() {
        eprintln!("\x1b[31merror:\x1b[0m {}", err);
        process::exit(1);
    }
}

fn run_cli() -> Result<()> {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if raw_args.is_empty() {
        print_help();
        return Ok(());
    }

    let workspace = env::current_dir()?;
    let mut cfg = RuntimeConfig::from_env(workspace);

    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw_args.len() {
        match raw_args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-V" | "--version" => {
                println!("axon {}", VERSION);
                return Ok(());
            }
            "--json" => cfg.json_output = true,
            "--allow-shell" => cfg.allow_shell = true,
            "-y" | "--yes" => cfg.auto_approve = true,
            "--max-steps" => {
                i += 1;
                let val = raw_args
                    .get(i)
                    .ok_or_else(|| AxonError::InvalidArgs("--max-steps requires a number".into()))?;
                cfg.max_steps = val
                    .parse()
                    .map_err(|_| AxonError::InvalidArgs("Invalid --max-steps value".into()))?;
            }
            "--endpoint" => {
                i += 1;
                cfg.llm_endpoint = raw_args
                    .get(i)
                    .ok_or_else(|| AxonError::InvalidArgs("--endpoint requires a URL".into()))?
                    .clone();
            }
            "--model" => {
                i += 1;
                cfg.llm_model = raw_args
                    .get(i)
                    .ok_or_else(|| AxonError::InvalidArgs("--model requires a name".into()))?
                    .clone();
            }
            "--workspace" => {
                i += 1;
                let dir = raw_args
                    .get(i)
                    .ok_or_else(|| AxonError::InvalidArgs("--workspace requires a path".into()))?;
                cfg.workspace = PathBuf::from(dir);
            }
            arg => positional.push(arg.to_string()),
        }
        i += 1;
    }

    if positional.is_empty() {
        print_help();
        return Ok(());
    }

    let cmd = positional[0].as_str();
    let memory = MemoryStore::new(&cfg.state_dir, cfg.vectorcore_url.clone())?;

    match cmd {
        "run" => {
            let goal = positional.get(1).ok_or_else(|| {
                AxonError::InvalidArgs("Usage: axon run <goal> [options]".into())
            })?;
            let runtime = AgentRuntime::new(&cfg, &memory);
            let trace = runtime.execute(goal)?;

            if cfg.json_output {
                println!("{}", trace.to_json().to_pretty_string());
            } else {
                println!(
                    "\x1b[32m✓ Run completed\x1b[0m  id=\x1b[1m{}\x1b[0m  steps={}  latency={}ms",
                    trace.run_id,
                    trace.steps.len(),
                    trace.total_latency_ms
                );
                println!("────────────────────────────────────────────────────────");
                for s in &trace.steps {
                    println!(
                        "  [{:>2}] {:<6} {:<14} ({:>3}ms) -> {}",
                        s.step, s.phase, s.action, s.latency_ms, s.status
                    );
                }
                println!("────────────────────────────────────────────────────────");
                println!("{}", trace.final_answer);
            }
        }
        "trace" => match positional.get(1).map(|s| s.as_str()) {
            Some("list") | None => {
                let ids = RunTrace::list_recent(&cfg.state_dir)?;
                if ids.is_empty() {
                    println!("No traces recorded yet. Run `axon run <goal>` first.");
                } else {
                    println!("Recorded runs ({}):", ids.len());
                    for id in ids {
                        println!("  • {}", id);
                    }
                }
            }
            Some(run_id) => {
                let trace = RunTrace::load(&cfg.state_dir, run_id)?;
                if cfg.json_output {
                    println!("{}", trace.to_json().to_pretty_string());
                } else {
                    println!("Run ID   : {}", trace.run_id);
                    println!("Goal     : {}", trace.goal);
                    println!("Status   : {}", trace.status);
                    println!("Latency  : {} ms", trace.total_latency_ms);
                    println!("Steps    :");
                    for s in &trace.steps {
                        println!(
                            "  #{} [{}] {} ({}ms) [{}]\n     in : {}\n     out: {}",
                            s.step,
                            s.phase,
                            s.action,
                            s.latency_ms,
                            s.status,
                            s.input,
                            s.output.replace('\n', "\n          ")
                        );
                    }
                }
            }
        },
        "memory" => match positional.get(1).map(|s| s.as_str()) {
            Some("add") => {
                let text = positional.get(2).ok_or_else(|| {
                    AxonError::InvalidArgs("Usage: axon memory add <note> [tags]".into())
                })?;
                let tags: Vec<String> = positional
                    .get(3)
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default();
                let entry = memory.add(text, tags)?;
                println!("Saved memory: {} ({})", entry.id, entry.content);
            }
            Some("search") => {
                let query = positional.get(2).map(|s| s.as_str()).unwrap_or("");
                let results = memory.search(query, 10)?;
                if results.is_empty() {
                    println!("No memories matched '{}'.", query);
                } else {
                    for r in results {
                        println!(
                            "  • [{}] score={:.2} tags={:?} :: {}",
                            r.id, r.score, r.tags, r.content
                        );
                    }
                }
            }
            _ => {
                return Err(AxonError::InvalidArgs(
                    "Usage: axon memory <add|search> <text>".into(),
                ))
            }
        },
        "doctor" => {
            let llm = LlmClient::new(
                cfg.llm_endpoint.clone(),
                cfg.llm_model.clone(),
                cfg.api_key.clone(),
            );
            let llm_status = match llm.ping() {
                Ok(msg) => format!("\x1b[32mOK\x1b[0m ({})", msg),
                Err(msg) => format!("\x1b[33mOFFLINE FALLBACK\x1b[0m ({})", msg),
            };
            let mem_count = memory.count().unwrap_or(0);
            let trace_count = RunTrace::list_recent(&cfg.state_dir)
                .map(|v| v.len())
                .unwrap_or(0);

            println!("Axon Runtime Doctor (v{})", VERSION);
            println!("────────────────────────────────────────────────────────");
            println!("  Workspace     : {}", cfg.workspace.display());
            println!("  State Dir     : {}", cfg.state_dir.display());
            println!("  LLM Endpoint  : {} -> {}", cfg.llm_endpoint, llm_status);
            println!("  Model         : {}", cfg.llm_model);
            println!(
                "  Memory Store  : {} ({} entries)",
                memory.backend_label(),
                mem_count
            );
            println!("  Saved Traces  : {}", trace_count);
            println!(
                "  Shell Policy  : {}",
                if cfg.allow_shell {
                    "enabled (gated)"
                } else {
                    "disabled (strict sandbox)"
                }
            );
        }
        other => {
            return Err(AxonError::InvalidArgs(format!(
                "Unknown command '{}'. Run `axon --help`.",
                other
            )))
        }
    }

    Ok(())
}

fn print_help() {
    println!(
        r#"axon v{} — Zero-dependency terminal AI agent runtime in Rust

USAGE:
  axon <COMMAND> [ARGS] [OPTIONS]

COMMANDS:
  run <goal>                Execute a bounded Reason -> Tool -> Observe agent loop
  trace [list|<run_id>]     Inspect deterministic execution traces
  memory <add|search> ...   Persist or query local / VectorCore-linked agent memory
  doctor                    Check workspace sandbox, memory store, and LLM/ContextShield gateway

OPTIONS:
  --max-steps <N>           Maximum agent reasoning steps (default: 8)
  --endpoint <URL>          OpenAI-compatible LLM or ContextShield URL
  --model <NAME>            Target model identifier
  --workspace <DIR>         Sandboxed root directory (default: current dir)
  --allow-shell             Enable gated shell tool execution
  -y, --yes                 Auto-approve write/shell sandbox prompts
  --json                    Emit machine-readable JSON output
  -h, --help                Show help information
  -V, --version             Print version"#,
        VERSION
    );
}
