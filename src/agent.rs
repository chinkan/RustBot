use anyhow::Result;
use tracing::info;

use crate::config::Config;
use crate::llm::{ChatMessage, FunctionDefinition, LlmClient, ToolDefinition};
use crate::mcp::McpManager;
use crate::memory::MemoryStore;
use crate::platform::IncomingMessage;
use crate::skills::SkillRegistry;
use crate::tools;

/// The core agent that processes messages through LLM + tools.
/// Platform-agnostic — receives IncomingMessage, returns response text.
pub struct Agent {
    pub llm: LlmClient,
    pub config: Config,
    pub mcp: McpManager,
    pub memory: MemoryStore,
    pub skills: SkillRegistry,
}

impl Agent {
    pub fn new(
        config: Config,
        mcp: McpManager,
        memory: MemoryStore,
        skills: SkillRegistry,
    ) -> Self {
        let llm = LlmClient::new(config.openrouter.clone());
        Self {
            llm,
            config,
            mcp,
            memory,
            skills,
        }
    }

    /// Build the system prompt, incorporating loaded skills
    fn build_system_prompt(&self) -> String {
        let mut prompt = self.config.openrouter.system_prompt.clone();

        let skill_context = self.skills.build_context();
        if !skill_context.is_empty() {
            prompt.push_str("\n\n# Available Skills\n\n");
            prompt.push_str(&skill_context);
        }

        prompt
    }

    /// Process an incoming message and return the response text
    pub async fn process_message(&self, incoming: &IncomingMessage) -> Result<String> {
        let platform = &incoming.platform;
        let user_id = &incoming.user_id;

        // Get or create persistent conversation
        let conversation_id = self
            .memory
            .get_or_create_conversation(platform, user_id)
            .await?;

        // Load existing messages from memory
        let mut messages = self.memory.load_messages(&conversation_id).await?;

        // If no messages yet, add system prompt
        if messages.is_empty() {
            let system_msg = ChatMessage {
                role: "system".to_string(),
                content: Some(self.build_system_prompt()),
                tool_calls: None,
                tool_call_id: None,
            };
            self.memory
                .save_message(&conversation_id, &system_msg)
                .await?;
            messages.push(system_msg);
        }

        // Add user message
        let user_msg = ChatMessage {
            role: "user".to_string(),
            content: Some(incoming.text.clone()),
            tool_calls: None,
            tool_call_id: None,
        };
        self.memory
            .save_message(&conversation_id, &user_msg)
            .await?;
        messages.push(user_msg);

        // Gather all tool definitions
        let mut all_tools: Vec<ToolDefinition> = tools::builtin_tool_definitions();
        all_tools.extend(self.mcp.tool_definitions());
        all_tools.extend(self.memory_tool_definitions());

        // Agentic loop — keep calling LLM until we get a non-tool response
        let max_iterations = 10;
        for iteration in 0..max_iterations {
            let response = self.llm.chat(&messages, &all_tools).await?;

            if let Some(tool_calls) = &response.tool_calls {
                if !tool_calls.is_empty() {
                    info!(
                        "LLM requested {} tool call(s) (iteration {})",
                        tool_calls.len(),
                        iteration
                    );

                    // Save assistant message with tool calls
                    self.memory
                        .save_message(&conversation_id, &response)
                        .await?;
                    messages.push(response.clone());

                    // Execute each tool call
                    for tool_call in tool_calls {
                        let arguments: serde_json::Value =
                            serde_json::from_str(&tool_call.function.arguments)
                                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                        let tool_result = self
                            .execute_tool(&tool_call.function.name, &arguments)
                            .await;

                        info!(
                            "Tool '{}' result length: {} chars",
                            tool_call.function.name,
                            tool_result.len()
                        );

                        let tool_msg = ChatMessage {
                            role: "tool".to_string(),
                            content: Some(tool_result),
                            tool_calls: None,
                            tool_call_id: Some(tool_call.id.clone()),
                        };
                        self.memory
                            .save_message(&conversation_id, &tool_msg)
                            .await?;
                        messages.push(tool_msg);
                    }

                    continue;
                }
            }

            // Final response — no tool calls
            let content = response.content.clone().unwrap_or_default();
            self.memory
                .save_message(&conversation_id, &response)
                .await?;

            return Ok(content);
        }

        Ok("I've reached the maximum number of tool call iterations. Please try rephrasing your request.".to_string())
    }

    /// Clear conversation history for a user
    pub async fn clear_conversation(&self, platform: &str, user_id: &str) -> Result<()> {
        self.memory.clear_conversation(platform, user_id).await
    }

    /// Get all tool definitions for display
    pub fn all_tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut all_tools = tools::builtin_tool_definitions();
        all_tools.extend(self.mcp.tool_definitions());
        all_tools.extend(self.memory_tool_definitions());
        all_tools
    }

    /// Memory-related tool definitions exposed to the LLM
    fn memory_tool_definitions(&self) -> Vec<ToolDefinition> {
        use serde_json::json;

        vec![
            ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "remember".to_string(),
                    description: "Store a piece of knowledge for long-term memory. Use this to remember user preferences, facts, or anything useful.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "category": { "type": "string", "description": "Category (e.g., 'user_preference', 'fact', 'project')" },
                            "key": { "type": "string", "description": "Short identifier for this knowledge" },
                            "value": { "type": "string", "description": "The knowledge to remember" }
                        },
                        "required": ["category", "key", "value"]
                    }),
                },
            },
            ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "recall".to_string(),
                    description: "Retrieve a specific piece of remembered knowledge.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "category": { "type": "string", "description": "Category to search in" },
                            "key": { "type": "string", "description": "The key to look up" }
                        },
                        "required": ["category", "key"]
                    }),
                },
            },
            ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "search_memory".to_string(),
                    description: "Search through past conversations and knowledge using hybrid vector + full-text search. Finds semantically similar content even with different wording.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "Search query (natural language)" },
                            "limit": { "type": "integer", "description": "Max results (default 5)" }
                        },
                        "required": ["query"]
                    }),
                },
            },
        ]
    }

    /// Execute a tool call by routing to the right handler
    async fn execute_tool(&self, name: &str, arguments: &serde_json::Value) -> String {
        match name {
            "remember" => {
                let category = arguments["category"].as_str().unwrap_or("general");
                let key = arguments["key"].as_str().unwrap_or("");
                let value = arguments["value"].as_str().unwrap_or("");
                match self.memory.remember(category, key, value, None).await {
                    Ok(()) => format!("Remembered: [{}] {} = {}", category, key, value),
                    Err(e) => format!("Failed to remember: {}", e),
                }
            }
            "recall" => {
                let category = arguments["category"].as_str().unwrap_or("general");
                let key = arguments["key"].as_str().unwrap_or("");
                match self.memory.recall(category, key).await {
                    Ok(Some(value)) => value,
                    Ok(None) => format!("No knowledge found for [{}] {}", category, key),
                    Err(e) => format!("Failed to recall: {}", e),
                }
            }
            "search_memory" => {
                let query = arguments["query"].as_str().unwrap_or("");
                let limit = arguments["limit"].as_u64().unwrap_or(5) as usize;

                let mut results = Vec::new();

                // Search conversations (hybrid vector + FTS5)
                if let Ok(msgs) = self.memory.search_messages(query, limit).await {
                    for msg in msgs {
                        if let Some(content) = &msg.content {
                            results.push(format!("[{}]: {}", msg.role, content));
                        }
                    }
                }

                // Search knowledge (hybrid vector + FTS5)
                if let Ok(entries) = self.memory.search_knowledge(query, limit).await {
                    for entry in entries {
                        results.push(format!(
                            "[knowledge:{}] {} = {}",
                            entry.category, entry.key, entry.value
                        ));
                    }
                }

                if results.is_empty() {
                    "No results found.".to_string()
                } else {
                    results.join("\n\n")
                }
            }
            _ if self.mcp.is_mcp_tool(name) => match self.mcp.call_tool(name, arguments).await {
                Ok(result) => result,
                Err(e) => format!("MCP tool error: {}", e),
            },
            _ => {
                match tools::execute_builtin_tool(
                    name,
                    arguments,
                    &self.config.sandbox.allowed_directory,
                )
                .await
                {
                    Ok(result) => result,
                    Err(e) => format!("Tool error: {}", e),
                }
            }
        }
    }
}
