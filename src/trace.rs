use crate::error::{AxonError, Result};
use crate::json::Json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct TraceStep {
    pub step: usize,
    pub phase: String,
    pub action: String,
    pub input: String,
    pub output: String,
    pub status: String,
    pub latency_ms: u128,
}

#[derive(Debug, Clone)]
pub struct RunTrace {
    pub run_id: String,
    pub goal: String,
    pub status: String,
    pub started_unix: u64,
    pub total_latency_ms: u128,
    pub steps: Vec<TraceStep>,
    pub final_answer: String,
}

impl RunTrace {
    pub fn new(goal: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let run_id = format!("run-{}-{:04x}", now.as_secs(), now.subsec_millis());
        Self {
            run_id,
            goal: goal.to_string(),
            status: "running".to_string(),
            started_unix: now.as_secs(),
            total_latency_ms: 0,
            steps: Vec::new(),
            final_answer: String::new(),
        }
    }

    pub fn record(&mut self, step: TraceStep) {
        self.steps.push(step);
    }

    pub fn to_json(&self) -> Json {
        let mut root = BTreeMap::new();
        root.insert("run_id".into(), Json::String(self.run_id.clone()));
        root.insert("goal".into(), Json::String(self.goal.clone()));
        root.insert("status".into(), Json::String(self.status.clone()));
        root.insert(
            "started_unix".into(),
            Json::Number(self.started_unix as f64),
        );
        root.insert(
            "total_latency_ms".into(),
            Json::Number(self.total_latency_ms as f64),
        );
        root.insert(
            "final_answer".into(),
            Json::String(self.final_answer.clone()),
        );

        let step_items: Vec<Json> = self
            .steps
            .iter()
            .map(|s| {
                let mut m = BTreeMap::new();
                m.insert("step".into(), Json::Number(s.step as f64));
                m.insert("phase".into(), Json::String(s.phase.clone()));
                m.insert("action".into(), Json::String(s.action.clone()));
                m.insert("input".into(), Json::String(s.input.clone()));
                m.insert("output".into(), Json::String(s.output.clone()));
                m.insert("status".into(), Json::String(s.status.clone()));
                m.insert("latency_ms".into(), Json::Number(s.latency_ms as f64));
                Json::Object(m)
            })
            .collect();

        root.insert("steps".into(), Json::Array(step_items));
        Json::Object(root)
    }

    pub fn save(&self, state_dir: &Path) -> Result<PathBuf> {
        let traces_dir = state_dir.join("traces");
        fs::create_dir_all(&traces_dir)?;
        let path = traces_dir.join(format!("{}.json", self.run_id));
        fs::write(&path, self.to_json().to_pretty_string())?;
        Ok(path)
    }

    pub fn load(state_dir: &Path, run_id: &str) -> Result<Self> {
        let clean_id = run_id.trim_end_matches(".json");
        let path = state_dir.join("traces").join(format!("{}.json", clean_id));
        if !path.exists() {
            return Err(AxonError::NotFound(format!(
                "Trace '{}' not found in {}",
                clean_id,
                path.display()
            )));
        }
        let raw = fs::read_to_string(&path)?;
        let json = Json::parse(&raw)?;

        let goal = json
            .get("goal")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let status = json
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let started_unix = json
            .get("started_unix")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u64;
        let total_latency_ms = json
            .get("total_latency_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u128;
        let final_answer = json
            .get("final_answer")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut steps = Vec::new();
        if let Some(arr) = json.get("steps").and_then(|v| v.as_array()) {
            for item in arr {
                steps.push(TraceStep {
                    step: item.get("step").and_then(|v| v.as_f64()).unwrap_or(0.0) as usize,
                    phase: item
                        .get("phase")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    action: item
                        .get("action")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    input: item
                        .get("input")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    output: item
                        .get("output")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    status: item
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    latency_ms: item
                        .get("latency_ms")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as u128,
                });
            }
        }

        Ok(Self {
            run_id: clean_id.to_string(),
            goal,
            status,
            started_unix,
            total_latency_ms,
            steps,
            final_answer,
        })
    }

    pub fn list_recent(state_dir: &Path) -> Result<Vec<String>> {
        let traces_dir = state_dir.join("traces");
        if !traces_dir.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(traces_dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                if let Some(id) = name.strip_suffix(".json") {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        ids.reverse();
        Ok(ids)
    }
}
