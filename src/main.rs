mod agent;
mod config;
mod llm;
mod mcp;
mod memory;
mod platform;
mod scheduler;
mod skills;
mod tools;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::agent::Agent;
use crate::config::Config;
use crate::mcp::McpManager;
use crate::memory::MemoryStore;
use crate::scheduler::tasks::register_builtin_tasks;
use crate::scheduler::Scheduler;
use crate::skills::loader::load_skills_from_dir;

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

    // Build embedding config if configured
    let embedding_config =
        config
            .embedding
            .as_ref()
            .map(|cfg| crate::memory::embeddings::EmbeddingConfig {
                api_key: cfg.api_key.clone(),
                base_url: cfg.base_url.clone(),
                model: cfg.model.clone(),
                dimensions: cfg.dimensions,
            });

    // Initialize memory store (SQLite + vector embeddings)
    let memory = MemoryStore::open(&config.memory.database_path, embedding_config)
        .context("Failed to initialize memory store")?;
    info!("  Database: {}", config.memory.database_path.display());

    // Initialize MCP connections
    let mut mcp_manager = McpManager::new();
    mcp_manager.connect_all(&config.mcp_servers).await;

    // Load skills from markdown files
    let skills = load_skills_from_dir(&config.skills.directory).await?;
    info!("  Skills: {}", skills.len());

    // Create the agent
    let agent = Arc::new(Agent::new(
        config.clone(),
        mcp_manager,
        memory.clone(),
        skills,
    ));

    // Initialize background task scheduler
    let scheduler = Scheduler::new().await?;
    register_builtin_tasks(&scheduler, memory).await?;
    scheduler.start().await?;
    info!("  Scheduler: active");

    // Run the Telegram platform
    info!("Bot is starting...");
    let bot = Arc::new(teloxide::Bot::new(&config.telegram.bot_token));
    platform::telegram::run(
        agent,
        config.telegram.allowed_user_ids.clone(),
        Arc::clone(&bot),
    )
    .await?;

    Ok(())
}
