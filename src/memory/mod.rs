use crate::error::Result;
use crate::json::Json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub timestamp: u64,
    pub score: f64,
}

pub struct MemoryStore {
    file_path: PathBuf,
    vectorcore_url: Option<String>,
}

impl MemoryStore {
    pub fn new(state_dir: &Path, vectorcore_url: Option<String>) -> Result<Self> {
        fs::create_dir_all(state_dir)?;
        Ok(Self {
            file_path: state_dir.join("memory.json"),
            vectorcore_url,
        })
    }

    pub fn add(&self, content: &str, tags: Vec<String>) -> Result<MemoryEntry> {
        let mut entries = self.load_all()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let id = format!("mem-{}-{:03}", now, entries.len() + 1);
        let entry = MemoryEntry {
            id,
            content: content.to_string(),
            tags,
            timestamp: now,
            score: 1.0,
        };
        entries.push(entry.clone());
        self.save_all(&entries)?;
        Ok(entry)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let entries = self.load_all()?;
        let q_terms: Vec<String> = tokenize(query);
        if q_terms.is_empty() {
            return Ok(entries.into_iter().rev().take(limit).collect());
        }

        let mut scored: Vec<MemoryEntry> = entries
            .into_iter()
            .filter_map(|mut e| {
                let doc_terms = tokenize(&format!("{} {}", e.content, e.tags.join(" ")));
                let overlap = q_terms
                    .iter()
                    .filter(|qt| doc_terms.iter().any(|dt| dt.contains(qt.as_str())))
                    .count();
                if overlap == 0 {
                    None
                } else {
                    e.score = overlap as f64 / q_terms.len() as f64;
                    Some(e)
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
        scored.truncate(limit);
        Ok(scored)
    }

    pub fn count(&self) -> Result<usize> {
        Ok(self.load_all()?.len())
    }

    pub fn backend_label(&self) -> String {
        match &self.vectorcore_url {
            Some(url) => format!("hybrid (local + vectorcore @ {})", url),
            None => "local-json-store".to_string(),
        }
    }

    fn load_all(&self) -> Result<Vec<MemoryEntry>> {
        if !self.file_path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&self.file_path)?;
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        let json = Json::parse(&raw)?;
        let mut out = Vec::new();
        if let Some(arr) = json.as_array() {
            for item in arr {
                let id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let content = item
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let timestamp = item
                    .get("timestamp")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as u64;
                let tags = item
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|t| t.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(MemoryEntry {
                    id,
                    content,
                    tags,
                    timestamp,
                    score: 0.0,
                });
            }
        }
        Ok(out)
    }

    fn save_all(&self, entries: &[MemoryEntry]) -> Result<()> {
        let items: Vec<Json> = entries
            .iter()
            .map(|e| {
                let mut m = BTreeMap::new();
                m.insert("id".into(), Json::String(e.id.clone()));
                m.insert("content".into(), Json::String(e.content.clone()));
                m.insert("timestamp".into(), Json::Number(e.timestamp as f64));
                m.insert(
                    "tags".into(),
                    Json::Array(e.tags.iter().cloned().map(Json::String).collect()),
                );
                Json::Object(m)
            })
            .collect();
        fs::write(&self.file_path, Json::Array(items).to_pretty_string())?;
        Ok(())
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(String::from)
        .collect()
}
