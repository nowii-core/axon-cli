use crate::error::{AxonError, Result};
use crate::memory::MemoryStore;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub struct ToolContext<'a> {
    pub workspace: &'a Path,
    pub memory: &'a MemoryStore,
    pub allow_shell: bool,
    pub auto_approve: bool,
}

pub fn execute_tool(
    name: &str,
    args: &BTreeMap<String, String>,
    ctx: &ToolContext<'_>,
) -> Result<String> {
    match name {
        "list_dir" => {
            let rel = args.get("path").map(|s| s.as_str()).unwrap_or(".");
            let target = resolve_sandboxed(ctx.workspace, rel)?;
            if !target.is_dir() {
                return Err(AxonError::InvalidArgs(format!(
                    "'{}' is not a directory",
                    rel
                )));
            }
            let mut entries = Vec::new();
            for entry in fs::read_dir(&target)? {
                let entry = entry?;
                let file_name = entry.file_name().to_string_lossy().to_string();
                if file_name == ".git" || file_name == "target" {
                    continue;
                }
                let kind = if entry.file_type()?.is_dir() {
                    "dir "
                } else {
                    "file"
                };
                entries.push(format!("[{}] {}", kind, file_name));
            }
            entries.sort();
            Ok(if entries.is_empty() {
                "(empty directory)".to_string()
            } else {
                entries.join("\n")
            })
        }
        "read_file" => {
            let rel = args
                .get("path")
                .ok_or_else(|| AxonError::InvalidArgs("read_file requires 'path'".into()))?;
            let target = resolve_sandboxed(ctx.workspace, rel)?;
            if !target.is_file() {
                return Err(AxonError::NotFound(format!("File '{}' not found", rel)));
            }
            let content = fs::read_to_string(&target)?;
            let capped: String = content.chars().take(4000).collect();
            Ok(capped)
        }
        "write_file" => {
            let rel = args
                .get("path")
                .ok_or_else(|| AxonError::InvalidArgs("write_file requires 'path'".into()))?;
            let content = args
                .get("content")
                .ok_or_else(|| AxonError::InvalidArgs("write_file requires 'content'".into()))?;
            let target = resolve_sandboxed(ctx.workspace, rel)?;
            confirm_action(
                ctx.auto_approve,
                &format!("Write {} bytes to '{}'", content.len(), rel),
            )?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, content)?;
            Ok(format!("Wrote {} bytes to {}", content.len(), rel))
        }
        "memory_search" => {
            let query = args.get("query").map(|s| s.as_str()).unwrap_or("");
            let hits = ctx.memory.search(query, 5)?;
            if hits.is_empty() {
                Ok("No matching memories found.".to_string())
            } else {
                let lines: Vec<String> = hits
                    .into_iter()
                    .map(|m| format!("- [{}] (score {:.2}): {}", m.id, m.score, m.content))
                    .collect();
                Ok(lines.join("\n"))
            }
        }
        "memory_save" => {
            let content = args
                .get("content")
                .ok_or_else(|| AxonError::InvalidArgs("memory_save requires 'content'".into()))?;
            let tags: Vec<String> = args
                .get("tags")
                .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();
            let entry = ctx.memory.add(content, tags)?;
            Ok(format!("Saved memory entry {}", entry.id))
        }
        "shell" => {
            if !ctx.allow_shell {
                return Err(AxonError::PolicyDenied(
                    "Shell tool is disabled (pass --allow-shell to enable)".into(),
                ));
            }
            let cmd = args
                .get("cmd")
                .ok_or_else(|| AxonError::InvalidArgs("shell requires 'cmd'".into()))?;
            confirm_action(ctx.auto_approve, &format!("Execute shell command: `{}`", cmd))?;

            let output = Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(ctx.workspace)
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(format!(
                "exit={}\nstdout:\n{}\nstderr:\n{}",
                output.status.code().unwrap_or(-1),
                stdout.trim(),
                stderr.trim()
            ))
        }
        unknown => Err(AxonError::PolicyDenied(format!(
            "Tool '{}' is not in allowlist",
            unknown
        ))),
    }
}

/// Ensures every path stays strictly inside the workspace root (prevents `..` traversal).
pub fn resolve_sandboxed(workspace: &Path, relative: &str) -> Result<PathBuf> {
    let rel_path = Path::new(relative);
    if rel_path.is_absolute() {
        return Err(AxonError::SandboxViolation(format!(
            "Absolute path '{}' is forbidden inside workspace sandbox",
            relative
        )));
    }

    let mut clean = PathBuf::new();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(c) => clean.push(c),
            Component::CurDir => {}
            Component::ParentDir => {
                if !clean.pop() {
                    return Err(AxonError::SandboxViolation(format!(
                        "Path '{}' escapes workspace root",
                        relative
                    )));
                }
            }
            _ => {
                return Err(AxonError::SandboxViolation(format!(
                    "Unsupported path component in '{}'",
                    relative
                )))
            }
        }
    }

    let root = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let joined = root.join(&clean);
    if joined.exists() {
        let canon = joined.canonicalize()?;
        if !canon.starts_with(&root) {
            return Err(AxonError::SandboxViolation(format!(
                "Symlink escape detected for '{}'",
                relative
            )));
        }
        Ok(canon)
    } else {
        Ok(joined)
    }
}

fn confirm_action(auto_approve: bool, prompt: &str) -> Result<()> {
    if auto_approve {
        return Ok(());
    }
    eprint!("[sandbox gate] {} — approve? [y/N]: ", prompt);
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        Err(AxonError::PolicyDenied("Action rejected by operator".into()))
    }
}
