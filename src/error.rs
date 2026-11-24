use std::fmt;
use std::io;

#[derive(Debug)]
pub enum AxonError {
    Io(io::Error),
    InvalidArgs(String),
    SandboxViolation(String),
    PolicyDenied(String),
    LoopDetected(String),
    MaxStepsExceeded(usize),
    ParseError(String),
    Network(String),
    NotFound(String),
}

impl fmt::Display for AxonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AxonError::Io(e) => write!(f, "I/O error: {}", e),
            AxonError::InvalidArgs(msg) => write!(f, "Invalid arguments: {}", msg),
            AxonError::SandboxViolation(msg) => write!(f, "Sandbox violation: {}", msg),
            AxonError::PolicyDenied(msg) => write!(f, "Policy denied: {}", msg),
            AxonError::LoopDetected(msg) => write!(f, "Loop guard triggered: {}", msg),
            AxonError::MaxStepsExceeded(n) => write!(f, "Max step budget ({}) exceeded", n),
            AxonError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            AxonError::Network(msg) => write!(f, "Network error: {}", msg),
            AxonError::NotFound(msg) => write!(f, "Not found: {}", msg),
        }
    }
}

impl std::error::Error for AxonError {}

impl From<io::Error> for AxonError {
    fn from(err: io::Error) -> Self {
        AxonError::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, AxonError>;
