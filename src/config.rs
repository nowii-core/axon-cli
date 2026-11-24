use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub workspace: PathBuf,
    pub state_dir: PathBuf,
    pub llm_endpoint: String,
    pub llm_model: String,
    pub api_key: Option<String>,
    pub vectorcore_url: Option<String>,
    pub max_steps: usize,
    pub allow_shell: bool,
    pub auto_approve: bool,
    pub json_output: bool,
}

impl RuntimeConfig {
    pub fn from_env(workspace: PathBuf) -> Self {
        let state_dir = env::var("AXON_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| workspace.join(".axon"));

        let llm_endpoint = env::var("AXON_LLM_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:8080/v1/chat/completions".to_string());

        let llm_model = env::var("AXON_MODEL").unwrap_or_else(|_| "local-default".to_string());
        let api_key = env::var("AXON_API_KEY").ok().filter(|s| !s.is_empty());
        let vectorcore_url = env::var("AXON_VECTORCORE_URL")
            .ok()
            .filter(|s| !s.is_empty());

        Self {
            workspace,
            state_dir,
            llm_endpoint,
            llm_model,
            api_key,
            vectorcore_url,
            max_steps: 8,
            allow_shell: false,
            auto_approve: false,
            json_output: false,
        }
    }
}
