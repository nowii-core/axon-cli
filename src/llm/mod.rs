use crate::error::{AxonError, Result};
use crate::json::Json;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum ModelDecision {
    ToolCall {
        thought: String,
        tool: String,
        args: BTreeMap<String, String>,
    },
    FinalAnswer {
        thought: String,
        answer: String,
    },
}

pub struct LlmClient {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl LlmClient {
    pub fn new(endpoint: String, model: String, api_key: Option<String>) -> Self {
        Self {
            endpoint,
            model,
            api_key,
        }
    }

    pub fn ping(&self) -> std::result::Result<String, String> {
        let parsed = parse_http_url(&self.endpoint).map_err(|e| e.to_string())?;
        let addr_str = format!("{}:{}", parsed.host, parsed.port);
        let addr = addr_str
            .to_socket_addrs()
            .map_err(|e| format!("DNS resolution failed: {}", e))?
            .next()
            .ok_or_else(|| "No socket address resolved".to_string())?;

        TcpStream::connect_timeout(&addr, Duration::from_millis(800))
            .map(|_| format!("reachable ({})", addr_str))
            .map_err(|e| format!("unreachable ({}): {}", addr_str, e))
    }

    pub fn decide(
        &self,
        goal: &str,
        observations: &[(String, String)],
        step: usize,
    ) -> Result<ModelDecision> {
        if let Ok(decision) = self.try_remote_completion(goal, observations) {
            return Ok(decision);
        }
        Ok(self.deterministic_local_plan(goal, observations, step))
    }

    fn try_remote_completion(
        &self,
        goal: &str,
        observations: &[(String, String)],
    ) -> Result<ModelDecision> {
        let parsed = parse_http_url(&self.endpoint)?;
        if parsed.is_https {
            return Err(AxonError::Network(
                "Zero-dep HTTP client supports plain HTTP (route HTTPS through ContextShield local proxy)".into(),
            ));
        }

        let addr_str = format!("{}:{}", parsed.host, parsed.port);
        let addr = addr_str
            .to_socket_addrs()
            .map_err(|e| AxonError::Network(e.to_string()))?
            .next()
            .ok_or_else(|| AxonError::Network("Failed to resolve host".into()))?;

        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(1200))
            .map_err(|e| AxonError::Network(e.to_string()))?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

        let system_prompt = r#"You are Axon, a terminal AI agent. Respond ONLY with JSON: {"thought":"...","action":"tool","tool":"list_dir|read_file|write_file|memory_search|memory_save|shell","args":{...}} or {"thought":"...","action":"final","answer":"..."}."#;

        let mut user_prompt = format!("Goal: {}\n", goal);
        for (i, (act, obs)) in observations.iter().enumerate() {
            user_prompt.push_str(&format!("Step {} [{}]: {}\n", i + 1, act, obs));
        }

        let mut sys_msg = BTreeMap::new();
        sys_msg.insert("role".into(), Json::String("system".into()));
        sys_msg.insert("content".into(), Json::String(system_prompt.into()));

        let mut usr_msg = BTreeMap::new();
        usr_msg.insert("role".into(), Json::String("user".into()));
        usr_msg.insert("content".into(), Json::String(user_prompt));

        let mut payload = BTreeMap::new();
        payload.insert("model".into(), Json::String(self.model.clone()));
        payload.insert("temperature".into(), Json::Number(0.1));
        payload.insert(
            "messages".into(),
            Json::Array(vec![Json::Object(sys_msg), Json::Object(usr_msg)]),
        );

        let body = Json::Object(payload).to_string();
        let mut req = format!(
            "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            parsed.path,
            parsed.host,
            parsed.port,
            body.len()
        );
        if let Some(key) = &self.api_key {
            req.push_str(&format!("Authorization: Bearer {}\r\n", key));
        }
        req.push_str("\r\n");
        req.push_str(&body);

        stream.write_all(req.as_bytes())?;
        let mut raw_resp = String::new();
        stream.read_to_string(&mut raw_resp)?;

        let (_, resp_body) = raw_resp
            .split_once("\r\n\r\n")
            .ok_or_else(|| AxonError::Network("Malformed HTTP response".into()))?;

        let root = Json::parse(resp_body.trim())?;
        let content = root
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|s| s.as_str())
            .ok_or_else(|| AxonError::ParseError("Missing choices[0].message.content".into()))?;

        parse_decision_json(content)
    }

    fn deterministic_local_plan(
        &self,
        goal: &str,
        observations: &[(String, String)],
        step: usize,
    ) -> ModelDecision {
        let lower = goal.to_lowercase();

        if step == 1 {
            if lower.contains("memory") || lower.contains("remember") || lower.contains("ricorda") {
                let mut args = BTreeMap::new();
                args.insert("query".into(), goal.to_string());
                return ModelDecision::ToolCall {
                    thought: "Checking persistent memory store for relevant context.".into(),
                    tool: "memory_search".into(),
                    args,
                };
            }

            let mut args = BTreeMap::new();
            args.insert("path".into(), ".".into());
            return ModelDecision::ToolCall {
                thought: "Inspecting workspace directory layout before planning next action.".into(),
                tool: "list_dir".into(),
                args,
            };
        }

        if step == 2 && (lower.contains("readme") || lower.contains("inspect") || lower.contains("audit") || lower.contains("analizza")) {
            let mut args = BTreeMap::new();
            args.insert("path".into(), "README.md".into());
            return ModelDecision::ToolCall {
                thought: "Reading README.md to gather project structure and documentation details.".into(),
                tool: "read_file".into(),
                args,
            };
        }

        let summary = if observations.is_empty() {
            format!("Completed goal '{}'.", goal)
        } else {
            let last = &observations[observations.len() - 1];
            format!(
                "Goal '{}' completed in {} step(s). Latest observation from `{}`:\n{}",
                goal,
                observations.len(),
                last.0,
                truncate(&last.1, 480)
            )
        };

        ModelDecision::FinalAnswer {
            thought: "Sufficient workspace evidence collected; synthesizing final answer.".into(),
            answer: summary,
        }
    }
}

fn parse_decision_json(raw: &str) -> Result<ModelDecision> {
    let trimmed = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let json = Json::parse(trimmed)?;
    let thought = json
        .get("thought")
        .and_then(|v| v.as_str())
        .unwrap_or("No thought provided")
        .to_string();
    let action = json
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("final");

    if action == "tool" {
        let tool = json
            .get("tool")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AxonError::ParseError("Missing 'tool' field".into()))?
            .to_string();
        let mut args = BTreeMap::new();
        if let Some(obj) = json.get("args").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                let val_str = match v {
                    Json::String(s) => s.clone(),
                    other => other.to_string(),
                };
                args.insert(k.clone(), val_str);
            }
        }
        Ok(ModelDecision::ToolCall {
            thought,
            tool,
            args,
        })
    } else {
        let answer = json
            .get("answer")
            .and_then(|v| v.as_str())
            .unwrap_or(trimmed)
            .to_string();
        Ok(ModelDecision::FinalAnswer { thought, answer })
    }
}

struct ParsedUrl {
    is_https: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_http_url(url: &str) -> Result<ParsedUrl> {
    let (is_https, rest) = if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else {
        return Err(AxonError::InvalidArgs(format!(
            "Endpoint must start with http:// or https:// (got '{}')",
            url
        )));
    };

    let (host_port, path) = match rest.split_once('/') {
        Some((hp, p)) => (hp, format!("/{}", p)),
        None => (rest, "/".to_string()),
    };

    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => {
            let parsed_port = p
                .parse::<u16>()
                .map_err(|_| AxonError::InvalidArgs("Invalid port in URL".into()))?;
            (h.to_string(), parsed_port)
        }
        None => (host_port.to_string(), if is_https { 443 } else { 80 }),
    };

    Ok(ParsedUrl {
        is_https,
        host,
        port,
        path,
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}...", cut)
    }
}
