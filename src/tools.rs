use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tracing::info;

use crate::llm::{FunctionDefinition, ToolDefinition};

/// Validates that a path is within the allowed sandbox directory.
/// Returns the canonicalized path if valid.
fn validate_sandbox_path(sandbox_dir: &Path, requested: &str) -> Result<PathBuf> {
    let sandbox_canonical = sandbox_dir
        .canonicalize()
        .with_context(|| format!("Sandbox directory not found: {}", sandbox_dir.display()))?;

    let requested_path = if Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        sandbox_dir.join(requested)
    };

    // For paths that don't exist yet (write_file), check the parent
    let check_path = if requested_path.exists() {
        requested_path
            .canonicalize()
            .context("Failed to canonicalize path")?
    } else {
        let parent = requested_path
            .parent()
            .context("Path has no parent directory")?;
        let parent_canonical = parent
            .canonicalize()
            .with_context(|| format!("Parent directory not found: {}", parent.display()))?;
        parent_canonical.join(requested_path.file_name().context("Path has no filename")?)
    };

    if !check_path.starts_with(&sandbox_canonical) {
        anyhow::bail!(
            "Access denied: path '{}' is outside the sandbox directory '{}'",
            requested,
            sandbox_dir.display()
        );
    }

    Ok(check_path)
}

pub fn builtin_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "read_file".to_string(),
                description: "Read the contents of a file within the sandbox directory"
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The file path (relative to sandbox or absolute within sandbox)"
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "write_file".to_string(),
                description: "Write content to a file within the sandbox directory. Creates parent directories if needed.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The file path (relative to sandbox or absolute within sandbox)"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "list_files".to_string(),
                description: "List files and directories within a path in the sandbox directory"
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The directory path (relative to sandbox or absolute within sandbox). Defaults to sandbox root."
                        }
                    },
                    "required": []
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "execute_command".to_string(),
                description:
                    "Execute a shell command within the sandbox directory. The working directory is set to the sandbox."
                        .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        }
                    },
                    "required": ["command"]
                }),
            },
        },
    ]
}

pub async fn execute_builtin_tool(
    tool_name: &str,
    arguments: &Value,
    sandbox_dir: &Path,
) -> Result<String> {
    match tool_name {
        "read_file" => {
            let path = arguments["path"]
                .as_str()
                .context("Missing 'path' argument")?;
            let full_path = validate_sandbox_path(sandbox_dir, path)?;
            info!("Reading file: {}", full_path.display());
            let content = tokio::fs::read_to_string(&full_path)
                .await
                .with_context(|| format!("Failed to read file: {}", full_path.display()))?;
            Ok(content)
        }
        "write_file" => {
            let path = arguments["path"]
                .as_str()
                .context("Missing 'path' argument")?;
            let content = arguments["content"]
                .as_str()
                .context("Missing 'content' argument")?;
            let full_path = validate_sandbox_path(sandbox_dir, path)?;

            // Create parent directories if they don't exist
            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await.with_context(|| {
                    format!("Failed to create directories: {}", parent.display())
                })?;
            }

            info!("Writing file: {}", full_path.display());
            tokio::fs::write(&full_path, content)
                .await
                .with_context(|| format!("Failed to write file: {}", full_path.display()))?;
            Ok(format!(
                "File written successfully: {}",
                full_path.display()
            ))
        }
        "list_files" => {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let full_path = validate_sandbox_path(sandbox_dir, path)?;
            info!("Listing files: {}", full_path.display());

            let mut entries = Vec::new();
            let mut read_dir = tokio::fs::read_dir(&full_path)
                .await
                .with_context(|| format!("Failed to read directory: {}", full_path.display()))?;

            while let Some(entry) = read_dir.next_entry().await? {
                let file_type = entry.file_type().await?;
                let prefix = if file_type.is_dir() {
                    "[DIR]"
                } else {
                    "[FILE]"
                };
                entries.push(format!(
                    "{} {}",
                    prefix,
                    entry.file_name().to_string_lossy()
                ));
            }

            entries.sort();
            if entries.is_empty() {
                Ok("Directory is empty".to_string())
            } else {
                Ok(entries.join("\n"))
            }
        }
        "execute_command" => {
            let command = arguments["command"]
                .as_str()
                .context("Missing 'command' argument")?;

            info!("Executing command in sandbox: {}", command);

            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(sandbox_dir)
                .output()
                .await
                .with_context(|| format!("Failed to execute command: {}", command))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut result = String::new();
            if !stdout.is_empty() {
                result.push_str(&format!("STDOUT:\n{}\n", stdout));
            }
            if !stderr.is_empty() {
                result.push_str(&format!("STDERR:\n{}\n", stderr));
            }
            result.push_str(&format!(
                "Exit code: {}",
                output.status.code().unwrap_or(-1)
            ));
            Ok(result)
        }
        _ => anyhow::bail!("Unknown built-in tool: {}", tool_name),
    }
}
