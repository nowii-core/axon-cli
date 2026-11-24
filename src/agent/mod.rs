use crate::config::RuntimeConfig;
use crate::error::{AxonError, Result};
use crate::llm::{LlmClient, ModelDecision};
use crate::memory::MemoryStore;
use crate::tools::{execute_tool, ToolContext};
use crate::trace::{RunTrace, TraceStep};
use std::collections::HashSet;
use std::time::Instant;

pub struct AgentRuntime<'a> {
    pub config: &'a RuntimeConfig,
    pub llm: LlmClient,
    pub memory: &'a MemoryStore,
}

impl<'a> AgentRuntime<'a> {
    pub fn new(config: &'a RuntimeConfig, memory: &'a MemoryStore) -> Self {
        let llm = LlmClient::new(
            config.llm_endpoint.clone(),
            config.llm_model.clone(),
            config.api_key.clone(),
        );
        Self {
            config,
            llm,
            memory,
        }
    }

    pub fn execute(&self, goal: &str) -> Result<RunTrace> {
        let run_start = Instant::now();
        let mut trace = RunTrace::new(goal);
        let mut observations: Vec<(String, String)> = Vec::new();
        let mut seen_signatures: HashSet<String> = HashSet::new();

        let tool_ctx = ToolContext {
            workspace: &self.config.workspace,
            memory: self.memory,
            allow_shell: self.config.allow_shell,
            auto_approve: self.config.auto_approve,
        };

        for step in 1..=self.config.max_steps {
            let step_start = Instant::now();
            let decision = self.llm.decide(goal, &observations, step)?;

            match decision {
                ModelDecision::ToolCall {
                    thought,
                    tool,
                    args,
                } => {
                    let signature = format!("{}::{:?}", tool, args);
                    if !seen_signatures.insert(signature.clone()) {
                        trace.status = "aborted_loop".to_string();
                        trace.total_latency_ms = run_start.elapsed().as_millis();
                        trace.save(&self.config.state_dir)?;
                        return Err(AxonError::LoopDetected(format!(
                            "Repeated identical tool call '{}'",
                            tool
                        )));
                    }

                    let (status, output) = match execute_tool(&tool, &args, &tool_ctx) {
                        Ok(out) => ("ok".to_string(), out),
                        Err(err) => ("error".to_string(), err.to_string()),
                    };

                    let latency = step_start.elapsed().as_millis();
                    trace.record(TraceStep {
                        step,
                        phase: "tool".to_string(),
                        action: tool.clone(),
                        input: format!("thought={} | args={:?}", thought, args),
                        output: output.clone(),
                        status,
                        latency_ms: latency,
                    });

                    observations.push((tool, output));
                }
                ModelDecision::FinalAnswer { thought, answer } => {
                    let latency = step_start.elapsed().as_millis();
                    trace.record(TraceStep {
                        step,
                        phase: "final".to_string(),
                        action: "respond".to_string(),
                        input: thought,
                        output: answer.clone(),
                        status: "ok".to_string(),
                        latency_ms: latency,
                    });
                    trace.status = "completed".to_string();
                    trace.final_answer = answer;
                    trace.total_latency_ms = run_start.elapsed().as_millis();
                    trace.save(&self.config.state_dir)?;
                    return Ok(trace);
                }
            }
        }

        trace.status = "max_steps_exceeded".to_string();
        trace.total_latency_ms = run_start.elapsed().as_millis();
        trace.save(&self.config.state_dir)?;
        Err(AxonError::MaxStepsExceeded(self.config.max_steps))
    }
}
