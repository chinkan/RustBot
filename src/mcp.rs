use anyhow::{Context, Result};
use rmcp::{
    model::{CallToolRequestParams, Tool as McpTool},
    service::RunningService,
    transport::{ConfigureCommandExt, TokioChildProcess},
    ServiceExt,
};
use serde_json::Value;
use std::collections::HashMap;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::config::McpServerConfig;
use crate::llm::{FunctionDefinition, ToolDefinition};

/// Represents a connected MCP server with its tools
pub struct McpConnection {
    pub name: String,
    /// Mutex serializes concurrent calls to prevent the stdio MCP server from
    /// receiving interleaved requests it cannot handle.
    pub client: Mutex<RunningService<rmcp::service::RoleClient, ()>>,
    /// Stored config allows reconnecting if the subprocess crashes.
    pub config: McpServerConfig,
    pub tools: Vec<McpTool>,
}

/// Manages multiple MCP server connections
pub struct McpManager {
    connections: HashMap<String, McpConnection>,
}

/// Start a fresh MCP client subprocess for the given config.
async fn start_client(
    config: &McpServerConfig,
) -> Result<RunningService<rmcp::service::RoleClient, ()>> {
    let args = config.args.clone();
    let env = config.env.clone();
    let command_str = config.command.clone();

    let transport = TokioChildProcess::new(Command::new(&command_str).configure(move |cmd| {
        for arg in &args {
            cmd.arg(arg);
        }
        for (key, value) in &env {
            cmd.env(key, value);
        }
    }))
    .with_context(|| format!("Failed to start MCP server process: {}", config.name))?;

    ().serve(transport)
        .await
        .with_context(|| format!("Failed to initialize MCP connection: {}", config.name))
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    /// Connect to an MCP server via stdio child process
    pub async fn connect(&mut self, config: &McpServerConfig) -> Result<()> {
        info!(
            "Connecting to MCP server '{}': {} {:?}",
            config.name, config.command, config.args
        );

        let client = start_client(config).await?;

        let server_info = client.peer_info();
        info!(
            "Connected to MCP server '{}': {:?}",
            config.name, server_info
        );

        let tools = client
            .list_all_tools()
            .await
            .with_context(|| format!("Failed to list tools from MCP server: {}", config.name))?;

        info!(
            "MCP server '{}' provides {} tools",
            config.name,
            tools.len()
        );
        for tool in &tools {
            info!("  - {}: {:?}", tool.name, tool.description);
        }

        self.connections.insert(
            config.name.clone(),
            McpConnection {
                name: config.name.clone(),
                client: Mutex::new(client),
                config: config.clone(),
                tools,
            },
        );

        Ok(())
    }

    /// Connect to all configured MCP servers, logging errors but not failing
    pub async fn connect_all(&mut self, configs: &[McpServerConfig]) {
        for config in configs {
            if let Err(e) = self.connect(config).await {
                error!("Failed to connect to MCP server '{}': {:#}", config.name, e);
            }
        }
    }

    /// Get all MCP tools as OpenRouter-compatible tool definitions
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut definitions = Vec::new();

        for connection in self.connections.values() {
            for tool in &connection.tools {
                let parameters = tool.schema_as_json_value();
                definitions.push(ToolDefinition {
                    tool_type: "function".to_string(),
                    function: FunctionDefinition {
                        name: format!("mcp_{}_{}", connection.name, tool.name),
                        description: tool
                            .description
                            .as_deref()
                            .unwrap_or("MCP tool")
                            .to_string(),
                        parameters,
                    },
                });
            }
        }

        definitions
    }

    /// Find which MCP server owns a tool and call it.
    ///
    /// Concurrent calls to the same server are serialized via a per-connection
    /// Mutex to prevent the stdio-based Python subprocess from receiving
    /// interleaved JSON-RPC requests it cannot reliably handle.
    ///
    /// If the first attempt fails (e.g. because the subprocess crashed), the
    /// connection is transparently reconnected and the call is retried once.
    pub async fn call_tool(&self, prefixed_name: &str, arguments: &Value) -> Result<String> {
        // Tool names are prefixed with "mcp_{server_name}_{tool_name}"
        let without_mcp = prefixed_name
            .strip_prefix("mcp_")
            .context("MCP tool name must start with 'mcp_'")?;

        // Find the matching connection
        for connection in self.connections.values() {
            let prefix = format!("{}_", connection.name);
            if let Some(tool_name) = without_mcp.strip_prefix(&prefix) {
                // Verify this tool exists on this server
                if connection
                    .tools
                    .iter()
                    .any(|t| t.name.as_ref() == tool_name)
                {
                    info!(
                        "Calling MCP tool '{}' on server '{}'",
                        tool_name, connection.name
                    );

                    let make_params = || CallToolRequestParams {
                        meta: None,
                        name: std::borrow::Cow::Owned(tool_name.to_string()),
                        arguments: arguments.as_object().cloned(),
                        task: None,
                    };

                    // Serialize calls to this server to avoid concurrent stdio issues.
                    let mut client_guard = connection.client.lock().await;

                    let call_result = match client_guard.call_tool(make_params()).await {
                        Ok(r) => r,
                        Err(first_err) => {
                            warn!(
                                "MCP tool '{}' on server '{}' failed: {}. Reconnecting...",
                                tool_name, connection.name, first_err
                            );
                            // The subprocess likely crashed; reconnect and retry once.
                            match start_client(&connection.config).await {
                                Ok(new_client) => {
                                    *client_guard = new_client;
                                    client_guard.call_tool(make_params()).await.with_context(
                                        || {
                                            format!(
                                                "Failed to call MCP tool '{}' on server '{}' after reconnect",
                                                tool_name, connection.name
                                            )
                                        },
                                    )?
                                }
                                Err(reconnect_err) => {
                                    anyhow::bail!(
                                        "MCP server '{}' crashed and reconnect failed: {}",
                                        connection.name,
                                        reconnect_err
                                    );
                                }
                            }
                        }
                    };

                    // Extract text content from the result
                    let text_parts: Vec<String> = call_result
                        .content
                        .iter()
                        .filter_map(|c| c.raw.as_text().map(|t| t.text.clone()))
                        .collect();

                    if text_parts.is_empty() {
                        return Ok(format!("{:?}", call_result.content));
                    }
                    return Ok(text_parts.join("\n"));
                }
            }
        }

        anyhow::bail!("MCP tool not found: {}", prefixed_name)
    }

    /// Check if a tool name belongs to an MCP server
    pub fn is_mcp_tool(&self, name: &str) -> bool {
        name.starts_with("mcp_")
    }

    /// Shutdown all MCP connections
    #[allow(dead_code)]
    pub async fn shutdown(&mut self) {
        for (name, connection) in self.connections.drain() {
            info!("Shutting down MCP server: {}", name);
            // into_inner() consumes the Mutex without locking (safe since we have &mut self)
            if let Err(e) = connection.client.into_inner().cancel().await {
                error!("Error shutting down MCP server '{}': {}", name, e);
            }
        }
    }
}
