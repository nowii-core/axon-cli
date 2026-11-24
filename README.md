# axon-cli

Zero-dependency terminal AI agent runtime in Rust with sandboxed tool execution, persistent memory, and deterministic trace logs.

Designed as the execution layer for the local AI infrastructure stack alongside [`vectorcore`](https://github.com/nowii-core/vectorcore) (hybrid vector & BM25 search) and [`contextshield`](https://github.com/nowii-core/contextshield) (AI security gateway & secret redaction).

## Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│                         axon-cli (Rust)                         │
│                                                                 │
│  CLI (`run` · `trace` · `memory` · `doctor`)                    │
│       │                                                         │
│       ▼                                                         │
│  Bounded Agent Loop (Reason ─► Tool ─► Observe)                 │
│   ├── Loop Guard & Step Budget                                  │
│   ├── Workspace Path Sandbox & Interactive Approval Gate        │
│   ├── Persistent Memory Store (Local / VectorCore HTTP bridge)  │
│   └── Deterministic JSON Execution Traces (`.axon/traces/`)     │
└───────────────┬─────────────────────────────────┬───────────────┘
                │ OpenAI-compatible HTTP          │ Hybrid Retrieval
                ▼                                 ▼
         ContextShield / LLM                 VectorCore
```

## Key Features

- **Zero External Dependencies:** Built purely on the Rust standard library (`std`) — compiles in seconds into a single stripped native binary.
- **Sandboxed Tool Execution:** Path traversal protection (`..` and symlink escape blocking), tool allowlisting, disabled-by-default shell execution (`--allow-shell`), and interactive operator approval prompts.
- **Deterministic Trace Logs:** Every run records step-by-step thoughts, tool inputs, outputs, status, and millisecond latency into `.axon/traces/<run_id>.json`.
- **Loop & Budget Guards:** Prevents infinite tool loops via call signature deduplication and strict `--max-steps` enforcement.
- **Offline & Gateway Ready:** Connects seamlessly to [`contextshield`](https://github.com/nowii-core/contextshield) or any OpenAI-compatible endpoint (`--endpoint`), with a built-in deterministic offline planner fallback.

## Quick Start

```bash
# Build release binary
cargo build --release

# Verify workspace sandbox, memory store, and gateway connectivity
./target/release/axon-cli doctor

# Run a bounded agent task
./target/release/axon-cli run "Inspect workspace files and summarize README"

# Inspect recorded execution traces
./target/release/axon-cli trace list
./target/release/axon-cli trace <run_id>

# Save and search persistent agent memory
./target/release/axon-cli memory add "ContextShield listens on 127.0.0.1:8080" "infra,gateway"
./target/release/axon-cli memory search "gateway"
```

## Example Output

```text
$ axon-cli run "Inspect workspace files and summarize README"
✓ Run completed  id=run-1795514400-01a4  steps=3  latency=2ms
────────────────────────────────────────────────────────
  [ 1] tool   list_dir       (  0ms) -> ok
  [ 2] tool   read_file      (  0ms) -> ok
  [ 3] final  respond        (  0ms) -> ok
────────────────────────────────────────────────────────
Goal 'Inspect workspace files and summarize README' completed in 2 step(s).
```

## Configuration

| Environment Variable | Default | Description |
| :--- | :--- | :--- |
| `AXON_LLM_ENDPOINT` | `http://127.0.0.1:8080/v1/chat/completions` | Target LLM or ContextShield gateway URL |
| `AXON_MODEL` | `local-default` | Model identifier sent in chat completions |
| `AXON_API_KEY` | *(unset)* | Optional Bearer token |
| `AXON_VECTORCORE_URL` | *(unset)* | Optional VectorCore HTTP API endpoint |
| `AXON_STATE_DIR` | `./.axon` | Directory for traces and persistent memory |

## License

MIT
