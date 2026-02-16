mod bot;
mod config;
mod llm;
mod mcp;
mod tools;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::bot::AppState;
use crate::config::Config;
use crate::mcp::McpManager;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rustbot=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"));

    info!("Loading configuration from: {}", config_path.display());
    let config = Config::load(&config_path)
        .with_context(|| format!("Failed to load config from {}", config_path.display()))?;

    info!("Configuration loaded successfully");
    info!("  Model: {}", config.openrouter.model);
    info!("  Sandbox: {}", config.sandbox.allowed_directory.display());
    info!("  Allowed users: {:?}", config.telegram.allowed_user_ids);
    info!("  MCP servers: {}", config.mcp_servers.len());

    // Initialize MCP connections
    let mut mcp_manager = McpManager::new();
    mcp_manager.connect_all(&config.mcp_servers).await;

    // Create shared state
    let state = Arc::new(AppState::new(config, mcp_manager));

    // Run the Telegram bot
    info!("Bot is starting...");
    bot::run(state).await?;

    Ok(())
}
