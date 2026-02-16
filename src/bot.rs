use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::llm::{ChatMessage, LlmClient, ToolDefinition};
use crate::mcp::McpManager;
use crate::tools;

/// Per-user conversation state
struct Conversation {
    messages: Vec<ChatMessage>,
}

impl Conversation {
    fn new(system_prompt: &str) -> Self {
        Self {
            messages: vec![ChatMessage {
                role: "system".to_string(),
                content: Some(system_prompt.to_string()),
                tool_calls: None,
                tool_call_id: None,
            }],
        }
    }
}

/// Shared application state
pub struct AppState {
    llm: LlmClient,
    config: Config,
    mcp: McpManager,
    conversations: Mutex<HashMap<u64, Conversation>>,
}

impl AppState {
    pub fn new(config: Config, mcp: McpManager) -> Self {
        let llm = LlmClient::new(config.openrouter.clone());
        Self {
            llm,
            config,
            mcp,
            conversations: Mutex::new(HashMap::new()),
        }
    }
}

/// Start the Telegram bot
pub async fn run(state: Arc<AppState>) -> Result<()> {
    let bot = Bot::new(&state.config.telegram.bot_token);

    info!("Starting Telegram bot...");

    let allowed_users = state.config.telegram.allowed_user_ids.clone();

    let handler = Update::filter_message()
        .filter_map(move |msg: Message| {
            let user = msg.from.as_ref()?;
            if allowed_users.contains(&user.id.0) {
                Some(msg)
            } else {
                None
            }
        })
        .endpoint(handle_message);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .default_handler(|upd| async move {
            warn!("Unhandled update: {:?}", upd.id);
        })
        .error_handler(LoggingErrorHandler::with_custom_text("bot"))
        .build()
        .dispatch()
        .await;

    Ok(())
}

async fn handle_message(bot: Bot, msg: Message, state: Arc<AppState>) -> ResponseResult<()> {
    let user_id = match msg.from.as_ref() {
        Some(user) => user.id.0,
        None => return Ok(()),
    };

    let text = match msg.text() {
        Some(t) => t.to_string(),
        None => return Ok(()),
    };

    info!("Message from user {}: {}", user_id, text);

    // Handle /clear command to reset conversation
    if text == "/clear" {
        let mut conversations = state.conversations.lock().await;
        conversations.remove(&user_id);
        bot.send_message(msg.chat.id, "Conversation cleared.")
            .await?;
        return Ok(());
    }

    // Handle /start command
    if text == "/start" {
        bot.send_message(
            msg.chat.id,
            "Hello! I'm your AI assistant. Send me a message and I'll help you.\n\n\
             Commands:\n\
             /clear - Clear conversation history\n\
             /tools - List available tools",
        )
        .await?;
        return Ok(());
    }

    // Handle /tools command
    if text == "/tools" {
        let builtin = tools::builtin_tool_definitions();
        let mcp_tools = state.mcp.tool_definitions();

        let mut tool_list = String::from("Available tools:\n\n");
        tool_list.push_str("Built-in tools:\n");
        for tool in &builtin {
            tool_list.push_str(&format!(
                "  - {}: {}\n",
                tool.function.name, tool.function.description
            ));
        }

        if !mcp_tools.is_empty() {
            tool_list.push_str("\nMCP tools:\n");
            for tool in &mcp_tools {
                tool_list.push_str(&format!(
                    "  - {}: {}\n",
                    tool.function.name, tool.function.description
                ));
            }
        }

        bot.send_message(msg.chat.id, tool_list).await?;
        return Ok(());
    }

    // Send "typing" indicator
    bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing)
        .await
        .ok();

    // Process the message with the LLM
    match process_with_llm(&state, user_id, &text).await {
        Ok(response) => {
            // Split long messages (Telegram has a 4096 char limit)
            for chunk in split_message(&response, 4000) {
                // Try sending, ignore errors for individual chunks
                bot.send_message(msg.chat.id, chunk).await.ok();
            }
        }
        Err(e) => {
            error!("Error processing message: {:#}", e);
            bot.send_message(msg.chat.id, format!("Error: {}", e))
                .await?;
        }
    }

    Ok(())
}

async fn process_with_llm(state: &AppState, user_id: u64, text: &str) -> Result<String> {
    // Get or create conversation
    {
        let mut conversations = state.conversations.lock().await;
        conversations
            .entry(user_id)
            .or_insert_with(|| Conversation::new(&state.config.openrouter.system_prompt));

        // Add user message
        let conv = conversations.get_mut(&user_id).unwrap();
        conv.messages.push(ChatMessage {
            role: "user".to_string(),
            content: Some(text.to_string()),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    // Gather all tool definitions
    let mut all_tools: Vec<ToolDefinition> = tools::builtin_tool_definitions();
    all_tools.extend(state.mcp.tool_definitions());

    // Agentic loop - keep calling LLM until we get a non-tool response
    let max_iterations = 10;
    for iteration in 0..max_iterations {
        let messages = {
            let conversations = state.conversations.lock().await;
            conversations[&user_id].messages.clone()
        };

        let response = state.llm.chat(&messages, &all_tools).await?;

        // Check if the LLM wants to call tools
        if let Some(tool_calls) = &response.tool_calls {
            if !tool_calls.is_empty() {
                info!(
                    "LLM requested {} tool call(s) (iteration {})",
                    tool_calls.len(),
                    iteration
                );

                // Add assistant message with tool calls to conversation
                {
                    let mut conversations = state.conversations.lock().await;
                    let conv = conversations.get_mut(&user_id).unwrap();
                    conv.messages.push(response.clone());
                }

                // Execute each tool call
                for tool_call in tool_calls {
                    let arguments: serde_json::Value =
                        serde_json::from_str(&tool_call.function.arguments)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                    let tool_result =
                        execute_tool(state, &tool_call.function.name, &arguments).await;

                    info!(
                        "Tool '{}' result length: {} chars",
                        tool_call.function.name,
                        tool_result.len()
                    );

                    // Add tool result to conversation
                    {
                        let mut conversations = state.conversations.lock().await;
                        let conv = conversations.get_mut(&user_id).unwrap();
                        conv.messages.push(ChatMessage {
                            role: "tool".to_string(),
                            content: Some(tool_result),
                            tool_calls: None,
                            tool_call_id: Some(tool_call.id.clone()),
                        });
                    }
                }

                // Continue the loop to let the LLM process tool results
                continue;
            }
        }

        // No tool calls - we have a final response
        let content = response.content.clone().unwrap_or_default();

        // Add assistant response to conversation
        {
            let mut conversations = state.conversations.lock().await;
            let conv = conversations.get_mut(&user_id).unwrap();
            conv.messages.push(response);
        }

        return Ok(content);
    }

    Ok("I've reached the maximum number of tool call iterations. Please try rephrasing your request.".to_string())
}

async fn execute_tool(state: &AppState, name: &str, arguments: &serde_json::Value) -> String {
    if state.mcp.is_mcp_tool(name) {
        match state.mcp.call_tool(name, arguments).await {
            Ok(result) => result,
            Err(e) => format!("MCP tool error: {}", e),
        }
    } else {
        match tools::execute_builtin_tool(name, arguments, &state.config.sandbox.allowed_directory)
            .await
        {
            Ok(result) => result,
            Err(e) => format!("Tool error: {}", e),
        }
    }
}

fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = (start + max_len).min(text.len());

        // Try to split at a newline or space
        let actual_end = if end < text.len() {
            text[start..end]
                .rfind('\n')
                .or_else(|| text[start..end].rfind(' '))
                .map(|pos| start + pos + 1)
                .unwrap_or(end)
        } else {
            end
        };

        chunks.push(text[start..actual_end].to_string());
        start = actual_end;
    }

    chunks
}
