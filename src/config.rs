use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    #[default]
    Openrouter,
    Ollama,
    Openai,
}

impl std::fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmProvider::Openrouter => write!(f, "openrouter"),
            LlmProvider::Ollama => write!(f, "ollama"),
            LlmProvider::Openai => write!(f, "openai"),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    #[serde(default)]
    pub provider: LlmProvider,
    pub model: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

impl LlmConfig {
    /// Returns the effective base_url: if the stored value is empty,
    /// fall back to the canonical URL for the configured provider.
    pub fn effective_base_url(&self) -> &str {
        if !self.base_url.is_empty() {
            return &self.base_url;
        }
        match self.provider {
            LlmProvider::Openrouter => "https://openrouter.ai/api/v1",
            LlmProvider::Ollama => "http://localhost:11434/v1",
            LlmProvider::Openai => "https://api.openai.com/v1",
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub telegram: TelegramConfig,
    pub llm: LlmConfig,
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default = "default_memory_config")]
    pub memory: MemoryConfig,
    #[serde(default = "default_skills_config")]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub general: Option<GeneralConfig>,
    #[serde(default = "default_agent_config")]
    pub agent: AgentConfig,
    pub embedding: Option<EmbeddingApiConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EmbeddingApiConfig {
    pub api_key: String,
    #[serde(default = "default_embedding_base_url")]
    pub base_url: String,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_embedding_dimensions")]
    pub dimensions: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub allowed_user_ids: Vec<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SandboxConfig {
    pub allowed_directory: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MemoryConfig {
    #[serde(default = "default_db_path")]
    pub database_path: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SkillsConfig {
    #[serde(default = "default_skills_dir")]
    pub directory: PathBuf,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GeneralConfig {
    /// Optional location string injected into the system prompt (e.g. "Tokyo, Japan")
    #[serde(default)]
    pub location: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_system_prompt() -> String {
    "You are a helpful AI assistant with access to tools. \
     Use the available tools to help the user with their tasks. \
     When using file or terminal tools, operate only within the allowed sandbox directory."
        .to_string()
}

fn default_db_path() -> PathBuf {
    PathBuf::from("rustfox.db")
}

fn default_skills_dir() -> PathBuf {
    PathBuf::from("skills")
}

fn default_embedding_base_url() -> String {
    "https://openrouter.ai/api/v1".to_string()
}

fn default_embedding_model() -> String {
    "qwen/qwen3-embedding-8b".to_string()
}

fn default_embedding_dimensions() -> usize {
    1536
}

fn default_memory_config() -> MemoryConfig {
    MemoryConfig {
        database_path: default_db_path(),
    }
}

fn default_skills_config() -> SkillsConfig {
    SkillsConfig {
        directory: default_skills_dir(),
    }
}

fn default_max_iterations() -> u32 {
    25
}

fn default_agent_config() -> AgentConfig {
    AgentConfig {
        max_iterations: default_max_iterations(),
    }
}

impl Config {
    /// Location string from [general], injected into the system prompt.
    pub fn user_location(&self) -> Option<&str> {
        self.general.as_ref().and_then(|g| g.location.as_deref())
    }

    /// Maximum agent loop iterations (from [agent] max_iterations, default 25).
    pub fn max_iterations(&self) -> u32 {
        self.agent.max_iterations
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        // Try parsing as-is; if [llm] is missing but [openrouter] exists, migrate.
        let config: Config = match toml::from_str(&content) {
            Ok(c) => c,
            Err(primary_err) => {
                // Attempt legacy migration: parse a loose struct that accepts [openrouter]
                #[derive(serde::Deserialize)]
                struct LegacyConfig {
                    telegram: TelegramConfig,
                    openrouter: Option<LegacyOpenRouter>,
                    sandbox: SandboxConfig,
                    #[serde(default)]
                    mcp_servers: Vec<McpServerConfig>,
                    #[serde(default = "default_memory_config")]
                    memory: MemoryConfig,
                    #[serde(default = "default_skills_config")]
                    skills: SkillsConfig,
                    #[serde(default)]
                    general: Option<GeneralConfig>,
                    #[serde(default = "default_agent_config")]
                    agent: AgentConfig,
                    embedding: Option<EmbeddingApiConfig>,
                }
                #[derive(serde::Deserialize, Default)]
                struct LegacyOpenRouter {
                    #[serde(default)]
                    api_key: String,
                    #[serde(default)]
                    model: String,
                    #[serde(default)]
                    base_url: String,
                    #[serde(default = "default_max_tokens")]
                    max_tokens: u32,
                    #[serde(default = "default_system_prompt")]
                    system_prompt: String,
                }
                let legacy: LegacyConfig = toml::from_str(&content)
                    .with_context(|| format!(
                        "Failed to parse config file (new format error: {}; legacy format also failed)",
                        primary_err
                    ))?;
                let or = legacy.openrouter.unwrap_or_default();
                let legacy_model = if or.model.is_empty() {
                    "moonshotai/kimi-k2.5".to_string()
                } else {
                    or.model
                };
                let legacy_base_url = if or.base_url.is_empty() {
                    "https://openrouter.ai/api/v1".to_string()
                } else {
                    or.base_url
                };
                Config {
                    telegram: legacy.telegram,
                    llm: LlmConfig {
                        provider: LlmProvider::Openrouter,
                        model: legacy_model,
                        base_url: legacy_base_url,
                        api_key: or.api_key,
                        max_tokens: or.max_tokens,
                        system_prompt: or.system_prompt,
                    },
                    sandbox: legacy.sandbox,
                    mcp_servers: legacy.mcp_servers,
                    memory: legacy.memory,
                    skills: legacy.skills,
                    general: legacy.general,
                    agent: legacy.agent,
                    embedding: legacy.embedding,
                }
            }
        };

        if !config.sandbox.allowed_directory.exists() {
            std::fs::create_dir_all(&config.sandbox.allowed_directory).with_context(|| {
                format!(
                    "Failed to create sandbox directory: {}",
                    config.sandbox.allowed_directory.display()
                )
            })?;
        }

        Ok(config)
    }
}
