# AI Agent Framework Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform RustBot from a Telegram-only AI chatbot into a modular AI agent framework with persistent memory, markdown-based skills, multi-platform support, and proactive background task scheduling.

**Architecture:** Modular monolith with Cargo feature flags. Core agent logic extracted from `bot.rs` into a platform-agnostic `agent` module. Platform adapters (Telegram first, Discord-ready) implement a `Platform` trait. SQLite with FTS5 provides persistent conversations, knowledge base, and learning. Skills are natural-language markdown files loaded at runtime. Tokio-cron-scheduler handles background tasks.

**Tech Stack:** Rust 2021, tokio (async runtime), rusqlite (SQLite + FTS5), teloxide (Telegram), serenity/poise (Discord, future), tokio-cron-scheduler, serde/toml (config), rmcp (MCP), reqwest (HTTP/LLM).

---

## Target Module Structure

```
src/
├── main.rs                # Entry point — initializes all subsystems
├── config.rs              # Extended config (+ discord, memory, scheduler sections)
├── llm.rs                 # LLM client (OpenRouter) — unchanged
├── mcp.rs                 # MCP server manager — unchanged
├── tools.rs               # Built-in sandbox tools — unchanged
├── platform/
│   ├── mod.rs             # Platform trait, IncomingMessage, OutgoingMessage types
│   └── telegram.rs        # Telegram adapter (refactored from bot.rs)
├── memory/
│   ├── mod.rs             # MemoryStore struct, init, migrations
│   ├── conversations.rs   # Save/load/search conversations
│   └── knowledge.rs       # Knowledge base with FTS5 search
├── skills/
│   ├── mod.rs             # SkillRegistry, Skill struct
│   └── loader.rs          # Load .md skill files from directory
├── scheduler/
│   ├── mod.rs             # Scheduler wrapper around tokio-cron-scheduler
│   └── tasks.rs           # Reminder and heartbeat task types
└── agent.rs               # Core agentic loop — platform-agnostic
```

---

## Phase 1: SQLite Memory Foundation

### Task 1: Add SQLite Dependencies

**Files:**
- Modify: `Cargo.toml:6-34`

**Step 1: Add rusqlite dependency**

Add to `[dependencies]` section in `Cargo.toml`:

```toml
# SQLite database with FTS5
rusqlite = { version = "0.34", features = ["bundled", "modern_full"] }

# Chrono for timestamps
chrono = { version = "0.4", features = ["serde"] }

# UUID for message IDs
uuid = { version = "1", features = ["v4"] }
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully (downloads new crates)

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat: add rusqlite, chrono, uuid dependencies for memory system"
```

---

### Task 2: Create Memory Module — Schema & Initialization

**Files:**
- Create: `src/memory/mod.rs`
- Create: `src/memory/conversations.rs`
- Create: `src/memory/knowledge.rs`
- Modify: `src/main.rs:1-5` (add `mod memory;`)

**Step 1: Create `src/memory/mod.rs`**

```rust
pub mod conversations;
pub mod knowledge;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Thread-safe SQLite memory store
#[derive(Clone)]
pub struct MemoryStore {
    conn: Arc<Mutex<Connection>>,
}

impl MemoryStore {
    /// Open or create the SQLite database at the given path
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database: {}", path.display()))?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        store.run_migrations_sync()?;
        info!("Memory store initialized at: {}", path.display());
        Ok(store)
    }

    /// Open an in-memory database (for testing)
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.run_migrations_sync()?;
        Ok(store)
    }

    fn run_migrations_sync(&self) -> Result<()> {
        // We need to block on the mutex here since this is called from sync context
        // This is safe because it's only called during initialization
        let conn = self.conn.blocking_lock();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                platform TEXT NOT NULL,
                user_id TEXT NOT NULL,
                started_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT,
                tool_calls TEXT,
                tool_call_id TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (conversation_id) REFERENCES conversations(id)
            );

            CREATE INDEX IF NOT EXISTS idx_messages_conversation
                ON messages(conversation_id, created_at);

            CREATE INDEX IF NOT EXISTS idx_conversations_user
                ON conversations(platform, user_id, updated_at);

            CREATE TABLE IF NOT EXISTS knowledge (
                id TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                source TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_key
                ON knowledge(category, key);

            -- FTS5 virtual table for full-text search across messages
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                content,
                content=messages,
                content_rowid=rowid
            );

            -- FTS5 virtual table for knowledge base search
            CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
                key,
                value,
                content=knowledge,
                content_rowid=rowid
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS messages_fts_insert AFTER INSERT ON messages
            WHEN NEW.content IS NOT NULL BEGIN
                INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
            END;

            CREATE TRIGGER IF NOT EXISTS messages_fts_delete AFTER DELETE ON messages
            WHEN OLD.content IS NOT NULL BEGIN
                INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', OLD.rowid, OLD.content);
            END;

            CREATE TRIGGER IF NOT EXISTS knowledge_fts_insert AFTER INSERT ON knowledge BEGIN
                INSERT INTO knowledge_fts(rowid, key, value) VALUES (NEW.rowid, NEW.key, NEW.value);
            END;

            CREATE TRIGGER IF NOT EXISTS knowledge_fts_delete AFTER DELETE ON knowledge BEGIN
                INSERT INTO knowledge_fts(knowledge_fts, rowid, key, value) VALUES('delete', OLD.rowid, OLD.key, OLD.value);
            END;

            CREATE TRIGGER IF NOT EXISTS knowledge_fts_update AFTER UPDATE ON knowledge BEGIN
                INSERT INTO knowledge_fts(knowledge_fts, rowid, key, value) VALUES('delete', OLD.rowid, OLD.key, OLD.value);
                INSERT INTO knowledge_fts(rowid, key, value) VALUES (NEW.rowid, NEW.key, NEW.value);
            END;
            ",
        )?;
        Ok(())
    }
}
```

**Step 2: Create `src/memory/conversations.rs`**

```rust
use anyhow::{Context, Result};
use uuid::Uuid;

use super::MemoryStore;
use crate::llm::ChatMessage;

impl MemoryStore {
    /// Get or create a conversation for a platform user
    pub async fn get_or_create_conversation(
        &self,
        platform: &str,
        user_id: &str,
    ) -> Result<String> {
        let conn = self.conn.lock().await;

        // Try to find an existing active conversation
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM conversations
                 WHERE platform = ?1 AND user_id = ?2
                 ORDER BY updated_at DESC LIMIT 1",
                rusqlite::params![platform, user_id],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = existing {
            return Ok(id);
        }

        // Create a new conversation
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO conversations (id, platform, user_id) VALUES (?1, ?2, ?3)",
            rusqlite::params![&id, platform, user_id],
        )
        .context("Failed to create conversation")?;

        Ok(id)
    }

    /// Save a message to a conversation
    pub async fn save_message(
        &self,
        conversation_id: &str,
        message: &ChatMessage,
    ) -> Result<String> {
        let conn = self.conn.lock().await;
        let id = Uuid::new_v4().to_string();

        let tool_calls_json = message
            .tool_calls
            .as_ref()
            .map(|tc| serde_json::to_string(tc).unwrap_or_default());

        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, tool_calls, tool_call_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                &id,
                conversation_id,
                &message.role,
                &message.content,
                &tool_calls_json,
                &message.tool_call_id,
            ],
        )
        .context("Failed to save message")?;

        // Update conversation timestamp
        conn.execute(
            "UPDATE conversations SET updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![conversation_id],
        )?;

        Ok(id)
    }

    /// Load all messages for a conversation
    pub async fn load_messages(&self, conversation_id: &str) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT role, content, tool_calls, tool_call_id
             FROM messages
             WHERE conversation_id = ?1
             ORDER BY created_at ASC",
        )?;

        let messages = stmt
            .query_map(rusqlite::params![conversation_id], |row| {
                let tool_calls_json: Option<String> = row.get(2)?;
                let tool_calls = tool_calls_json.and_then(|json| serde_json::from_str(&json).ok());

                Ok(ChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    tool_calls,
                    tool_call_id: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to load messages")?;

        Ok(messages)
    }

    /// Clear a conversation (delete all its messages)
    pub async fn clear_conversation(&self, platform: &str, user_id: &str) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            "DELETE FROM messages WHERE conversation_id IN (
                SELECT id FROM conversations WHERE platform = ?1 AND user_id = ?2
            )",
            rusqlite::params![platform, user_id],
        )?;

        conn.execute(
            "DELETE FROM conversations WHERE platform = ?1 AND user_id = ?2",
            rusqlite::params![platform, user_id],
        )?;

        Ok(())
    }

    /// Search messages using FTS5 full-text search
    pub async fn search_messages(&self, query: &str, limit: usize) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT m.role, m.content, m.tool_calls, m.tool_call_id
             FROM messages m
             JOIN messages_fts fts ON m.rowid = fts.rowid
             WHERE messages_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let messages = stmt
            .query_map(rusqlite::params![query, limit as i64], |row| {
                let tool_calls_json: Option<String> = row.get(2)?;
                let tool_calls = tool_calls_json.and_then(|json| serde_json::from_str(&json).ok());

                Ok(ChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    tool_calls,
                    tool_call_id: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to search messages")?;

        Ok(messages)
    }
}
```

**Step 3: Create `src/memory/knowledge.rs`**

```rust
use anyhow::{Context, Result};
use uuid::Uuid;

use super::MemoryStore;

/// A knowledge entry the agent has learned
#[derive(Debug, Clone)]
pub struct KnowledgeEntry {
    pub id: String,
    pub category: String,
    pub key: String,
    pub value: String,
    pub source: Option<String>,
}

impl MemoryStore {
    /// Store or update a knowledge entry
    pub async fn remember(
        &self,
        category: &str,
        key: &str,
        value: &str,
        source: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().await;
        let id = Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO knowledge (id, category, key, value, source)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(category, key) DO UPDATE SET
                value = excluded.value,
                source = excluded.source,
                updated_at = datetime('now')",
            rusqlite::params![&id, category, key, value, source],
        )
        .context("Failed to store knowledge")?;

        Ok(())
    }

    /// Recall a specific knowledge entry
    pub async fn recall(&self, category: &str, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().await;
        let result = conn
            .query_row(
                "SELECT value FROM knowledge WHERE category = ?1 AND key = ?2",
                rusqlite::params![category, key],
                |row| row.get(0),
            )
            .ok();

        Ok(result)
    }

    /// Search knowledge using FTS5
    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeEntry>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT k.id, k.category, k.key, k.value, k.source
             FROM knowledge k
             JOIN knowledge_fts fts ON k.rowid = fts.rowid
             WHERE knowledge_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let entries = stmt
            .query_map(rusqlite::params![query, limit as i64], |row| {
                Ok(KnowledgeEntry {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    source: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to search knowledge")?;

        Ok(entries)
    }

    /// List all knowledge in a category
    pub async fn list_knowledge(&self, category: &str) -> Result<Vec<KnowledgeEntry>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, category, key, value, source
             FROM knowledge
             WHERE category = ?1
             ORDER BY key",
        )?;

        let entries = stmt
            .query_map(rusqlite::params![category], |row| {
                Ok(KnowledgeEntry {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    source: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to list knowledge")?;

        Ok(entries)
    }

    /// Forget a knowledge entry
    pub async fn forget(&self, category: &str, key: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        let rows = conn.execute(
            "DELETE FROM knowledge WHERE category = ?1 AND key = ?2",
            rusqlite::params![category, key],
        )?;
        Ok(rows > 0)
    }
}
```

**Step 4: Register module in `src/main.rs`**

Add `mod memory;` to the module declarations at the top of `src/main.rs` (line 1).

**Step 5: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 6: Commit**

```bash
git add src/memory/ src/main.rs
git commit -m "feat: add SQLite memory module with conversations, knowledge, and FTS5 search"
```

---

### Task 3: Extend Config for Memory and Database Path

**Files:**
- Modify: `src/config.rs:6-12` (add memory config)
- Modify: `config.example.toml` (add memory section)

**Step 1: Add MemoryConfig to `src/config.rs`**

Add a new struct and field:

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct MemoryConfig {
    #[serde(default = "default_db_path")]
    pub database_path: PathBuf,
}

fn default_db_path() -> PathBuf {
    PathBuf::from("rustbot.db")
}
```

Add to the `Config` struct:

```rust
#[serde(default = "default_memory_config")]
pub memory: MemoryConfig,
```

Add the default function:

```rust
fn default_memory_config() -> MemoryConfig {
    MemoryConfig {
        database_path: default_db_path(),
    }
}
```

**Step 2: Add `[memory]` section to `config.example.toml`**

```toml
[memory]
# Path to the SQLite database file for persistent memory
database_path = "rustbot.db"
```

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/config.rs config.example.toml
git commit -m "feat: add memory configuration for SQLite database path"
```

---

### Task 4: Wire MemoryStore into AppState and Main

**Files:**
- Modify: `src/main.rs:51-56` (initialize MemoryStore)
- Modify: `src/bot.rs:33-49` (add memory to AppState)

**Step 1: Initialize MemoryStore in `src/main.rs`**

After config loading (around line 49), add:

```rust
use crate::memory::MemoryStore;

// Initialize memory store
let memory = MemoryStore::open(&config.memory.database_path)
    .context("Failed to initialize memory store")?;
info!("  Database: {}", config.memory.database_path.display());
```

Pass it to AppState:

```rust
let state = Arc::new(AppState::new(config, mcp_manager, memory));
```

**Step 2: Add MemoryStore to AppState in `src/bot.rs`**

Add field to `AppState`:

```rust
pub struct AppState {
    llm: LlmClient,
    config: Config,
    mcp: McpManager,
    memory: MemoryStore,
    conversations: Mutex<HashMap<u64, Conversation>>,
}
```

Update constructor:

```rust
pub fn new(config: Config, mcp: McpManager, memory: MemoryStore) -> Self {
    let llm = LlmClient::new(config.openrouter.clone());
    Self {
        llm,
        config,
        mcp,
        memory,
        conversations: Mutex::new(HashMap::new()),
    }
}
```

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/main.rs src/bot.rs
git commit -m "feat: wire MemoryStore into AppState and initialization"
```

---

### Task 5: Persist Conversations to SQLite

**Files:**
- Modify: `src/bot.rs:171-266` (update `process_with_llm` to use MemoryStore)

**Step 1: Refactor `process_with_llm` to persist messages**

Replace the in-memory-only approach. At conversation start, load from SQLite. After each message (user, assistant, tool), save to SQLite. Keep in-memory cache for the active session but back it with persistence.

In `process_with_llm`, change the conversation initialization to:

```rust
// Get or create persistent conversation
let conversation_id = state
    .memory
    .get_or_create_conversation("telegram", &user_id.to_string())
    .await?;
```

After adding user message to in-memory, also persist:

```rust
state.memory.save_message(&conversation_id, &user_msg).await?;
```

Same pattern for assistant messages and tool results — after adding to in-memory conversation, also call `state.memory.save_message(...)`.

For `/clear`, also call:

```rust
state.memory.clear_conversation("telegram", &user_id.to_string()).await?;
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/bot.rs
git commit -m "feat: persist conversation messages to SQLite memory"
```

---

## Phase 2: Platform Abstraction Layer

### Task 6: Define Platform Trait and Message Types

**Files:**
- Create: `src/platform/mod.rs`
- Modify: `src/main.rs:1-5` (add `mod platform;`)

**Step 1: Create `src/platform/mod.rs`**

```rust
pub mod telegram;

use anyhow::Result;
use async_trait::async_trait;

/// A message received from any platform
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// Platform identifier (e.g., "telegram", "discord")
    pub platform: String,
    /// Platform-specific user ID as string
    pub user_id: String,
    /// Platform-specific chat/channel ID as string
    pub chat_id: String,
    /// Display name of the user
    pub user_name: String,
    /// The message text
    pub text: String,
}

/// A message to send back to a platform
#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    /// The chat/channel to send to
    pub chat_id: String,
    /// The message text
    pub text: String,
}

/// Trait that all platform adapters must implement
#[async_trait]
pub trait Platform: Send + Sync {
    /// Platform name identifier
    fn name(&self) -> &str;

    /// Start the platform's event loop. This should block until shutdown.
    async fn run(&self) -> Result<()>;
}
```

**Step 2: Add `async-trait` dependency to `Cargo.toml`**

```toml
# Async trait support
async-trait = "0.1"
```

**Step 3: Register module in `src/main.rs`**

Add `mod platform;` to the module declarations.

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/platform/ src/main.rs Cargo.toml Cargo.lock
git commit -m "feat: add platform abstraction layer with trait and message types"
```

---

### Task 7: Extract Agent Core from bot.rs

**Files:**
- Create: `src/agent.rs`
- Modify: `src/main.rs:1-5` (add `mod agent;`)

**Step 1: Create `src/agent.rs`**

Extract the platform-agnostic agentic loop. This is the brain of the system — receives an `IncomingMessage`, processes it through LLM + tools, returns response text.

```rust
use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info};

use crate::config::Config;
use crate::llm::{ChatMessage, LlmClient, ToolDefinition};
use crate::mcp::McpManager;
use crate::memory::MemoryStore;
use crate::platform::IncomingMessage;
use crate::skills::SkillRegistry;
use crate::tools;

/// The core agent that processes messages through LLM + tools
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

        // Agentic loop
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

                        let tool_result =
                            self.execute_tool(&tool_call.function.name, &arguments).await;

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
        use crate::llm::FunctionDefinition;
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
                    description: "Search through past conversations and knowledge using full-text search.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "Search query" },
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
        // Memory tools
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

                // Search conversations
                if let Ok(msgs) = self.memory.search_messages(query, limit).await {
                    for msg in msgs {
                        if let Some(content) = &msg.content {
                            results.push(format!("[{}]: {}", msg.role, content));
                        }
                    }
                }

                // Search knowledge
                if let Ok(entries) = self.memory.search_knowledge(query, limit).await {
                    for entry in entries {
                        results.push(format!("[knowledge:{}] {} = {}", entry.category, entry.key, entry.value));
                    }
                }

                if results.is_empty() {
                    "No results found.".to_string()
                } else {
                    results.join("\n\n")
                }
            }
            _ if self.mcp.is_mcp_tool(name) => {
                match self.mcp.call_tool(name, arguments).await {
                    Ok(result) => result,
                    Err(e) => format!("MCP tool error: {}", e),
                }
            }
            _ => {
                match tools::execute_builtin_tool(name, arguments, &self.config.sandbox.allowed_directory).await {
                    Ok(result) => result,
                    Err(e) => format!("Tool error: {}", e),
                }
            }
        }
    }
}
```

**Step 2: Register module in `src/main.rs`**

Add `mod agent;` to the module declarations.

**Step 3: Verify it compiles**

Note: This will not compile yet because it references `crate::skills::SkillRegistry` which doesn't exist. We'll create a minimal stub in the next task.

**Step 4: Commit**

```bash
git add src/agent.rs src/main.rs
git commit -m "feat: extract platform-agnostic agent core with memory tools"
```

---

## Phase 3: Skill System

### Task 8: Create Skill Loader and Registry

**Files:**
- Create: `src/skills/mod.rs`
- Create: `src/skills/loader.rs`
- Modify: `src/main.rs:1-5` (add `mod skills;`)

**Step 1: Create `src/skills/mod.rs`**

```rust
pub mod loader;

use std::collections::HashMap;
use tracing::info;

/// A loaded skill from a markdown file
#[derive(Debug, Clone)]
pub struct Skill {
    /// Skill name (derived from filename or frontmatter)
    pub name: String,
    /// Short description
    pub description: String,
    /// Full markdown content (the instructions)
    pub content: String,
    /// Category/tags for organization
    pub tags: Vec<String>,
}

/// Registry of all loaded skills
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    /// Register a skill
    pub fn register(&mut self, skill: Skill) {
        info!("Registered skill: {} — {}", skill.name, skill.description);
        self.skills.insert(skill.name.clone(), skill);
    }

    /// Get a skill by name
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// List all registered skills
    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// Build context string for the system prompt
    /// This gives the LLM awareness of available skills
    pub fn build_context(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let mut context = String::from("You have the following skills available. When relevant, follow these instructions:\n\n");
        for skill in self.skills.values() {
            context.push_str(&format!("## Skill: {}\n", skill.name));
            context.push_str(&format!("{}\n\n", skill.content));
        }
        context
    }

    /// Count of registered skills
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}
```

**Step 2: Create `src/skills/loader.rs`**

```rust
use anyhow::{Context, Result};
use std::path::Path;
use tracing::{info, warn};

use super::{Skill, SkillRegistry};

/// Load all markdown skill files from a directory
///
/// Skill format (markdown with optional YAML frontmatter):
///
/// ```markdown
/// ---
/// name: my-skill
/// description: What this skill does
/// tags: [coding, review]
/// ---
///
/// # Skill Instructions
///
/// The full markdown content is the skill's instructions...
/// ```
///
/// If no frontmatter, the filename (without extension) is the name,
/// and the first heading or first line is the description.
pub async fn load_skills_from_dir(dir: &Path) -> Result<SkillRegistry> {
    let mut registry = SkillRegistry::new();

    if !dir.exists() {
        info!("Skills directory not found: {}, skipping", dir.display());
        return Ok(registry);
    }

    let mut entries = tokio::fs::read_dir(dir)
        .await
        .with_context(|| format!("Failed to read skills directory: {}", dir.display()))?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        // Support .md files and directories containing SKILL.md
        let skill_path = if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.exists() {
                skill_file
            } else {
                continue;
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            path.clone()
        } else {
            continue;
        };

        match load_skill_file(&skill_path).await {
            Ok(skill) => registry.register(skill),
            Err(e) => warn!("Failed to load skill from {}: {}", skill_path.display(), e),
        }
    }

    info!("Loaded {} skills", registry.len());
    Ok(registry)
}

async fn load_skill_file(path: &Path) -> Result<Skill> {
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read skill file: {}", path.display()))?;

    // Try to parse YAML frontmatter
    if content.starts_with("---") {
        if let Some(end) = content[3..].find("---") {
            let frontmatter = &content[3..3 + end].trim();
            let body = content[3 + end + 3..].trim().to_string();

            let name = extract_field(frontmatter, "name");
            let description = extract_field(frontmatter, "description");
            let tags = extract_list_field(frontmatter, "tags");

            let skill_name = name.unwrap_or_else(|| name_from_path(path));

            return Ok(Skill {
                name: skill_name,
                description: description.unwrap_or_else(|| first_line_or_heading(&body)),
                content: body,
                tags,
            });
        }
    }

    // No frontmatter — derive metadata from content
    let name = name_from_path(path);
    let description = first_line_or_heading(&content);

    Ok(Skill {
        name,
        description,
        content: content.to_string(),
        tags: Vec::new(),
    })
}

/// Extract a simple `key: value` from YAML-like frontmatter
fn extract_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&format!("{}:", key)) {
            let value = rest.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Extract a simple `key: [a, b, c]` list from frontmatter
fn extract_list_field(frontmatter: &str, key: &str) -> Vec<String> {
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&format!("{}:", key)) {
            let rest = rest.trim();
            if rest.starts_with('[') && rest.ends_with(']') {
                return rest[1..rest.len() - 1]
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Derive skill name from file path
fn name_from_path(path: &Path) -> String {
    // If it's SKILL.md inside a directory, use the directory name
    if path.file_name().and_then(|f| f.to_str()) == Some("SKILL.md") {
        if let Some(parent) = path.parent() {
            if let Some(dir_name) = parent.file_name().and_then(|f| f.to_str()) {
                return dir_name.to_string();
            }
        }
    }
    // Otherwise use the file stem
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed")
        .to_string()
}

/// Get the first heading or first line as a description
fn first_line_or_heading(content: &str) -> String {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(heading) = line.strip_prefix('#') {
            return heading.trim().trim_start_matches('#').trim().to_string();
        }
        return line.to_string();
    }
    "No description".to_string()
}
```

**Step 3: Register module in `src/main.rs`**

Add `mod skills;` to the module declarations.

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/skills/ src/main.rs
git commit -m "feat: add skill system with markdown loader and registry"
```

---

### Task 9: Add Skills Config and Wire Skills Loading

**Files:**
- Modify: `src/config.rs` (add SkillsConfig)
- Modify: `src/main.rs` (load skills during init)
- Modify: `config.example.toml` (add skills section)

**Step 1: Add SkillsConfig to `src/config.rs`**

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct SkillsConfig {
    #[serde(default = "default_skills_dir")]
    pub directory: PathBuf,
}

fn default_skills_dir() -> PathBuf {
    PathBuf::from("skills")
}

fn default_skills_config() -> SkillsConfig {
    SkillsConfig {
        directory: default_skills_dir(),
    }
}
```

Add to `Config` struct:

```rust
#[serde(default = "default_skills_config")]
pub skills: SkillsConfig,
```

**Step 2: Load skills in `src/main.rs`**

After memory initialization:

```rust
use crate::skills::loader::load_skills_from_dir;

// Load skills
let skills = load_skills_from_dir(&config.skills.directory).await?;
info!("  Skills: {}", skills.len());
```

**Step 3: Add to `config.example.toml`**

```toml
[skills]
# Directory containing skill markdown files
directory = "skills"
```

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/config.rs src/main.rs config.example.toml
git commit -m "feat: add skills configuration and loading at startup"
```

---

## Phase 4: Refactor Telegram to Use Agent + Platform

### Task 10: Refactor Telegram as a Platform Adapter

**Files:**
- Create: `src/platform/telegram.rs`
- Modify: `src/bot.rs` (slim down to delegation)
- Modify: `src/main.rs` (pass Agent to bot)

**Step 1: Create `src/platform/telegram.rs`**

```rust
use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use tracing::{error, info, warn};

use crate::agent::Agent;
use crate::platform::IncomingMessage;

/// Split long messages for Telegram's 4096 char limit
fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = (start + max_len).min(text.len());
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

/// Run the Telegram bot platform
pub async fn run(agent: Arc<Agent>, allowed_user_ids: Vec<u64>, bot_token: &str) -> Result<()> {
    let bot = Bot::new(bot_token);

    info!("Starting Telegram platform...");

    let handler = Update::filter_message()
        .filter_map(move |msg: Message| {
            let user = msg.from.as_ref()?;
            if allowed_user_ids.contains(&user.id.0) {
                Some(msg)
            } else {
                None
            }
        })
        .endpoint(handle_message);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![agent])
        .default_handler(|upd| async move {
            warn!("Unhandled update: {:?}", upd.id);
        })
        .error_handler(LoggingErrorHandler::with_custom_text("telegram"))
        .build()
        .dispatch()
        .await;

    Ok(())
}

async fn handle_message(bot: Bot, msg: Message, agent: Arc<Agent>) -> ResponseResult<()> {
    let user = match msg.from.as_ref() {
        Some(user) => user,
        None => return Ok(()),
    };

    let user_id = user.id.0;
    let text = match msg.text() {
        Some(t) => t.to_string(),
        None => return Ok(()),
    };

    let user_name = user
        .first_name
        .clone();

    info!("Telegram message from {} ({}): {}", user_name, user_id, text);

    // Handle commands
    if text == "/clear" {
        if let Err(e) = agent.clear_conversation("telegram", &user_id.to_string()).await {
            error!("Failed to clear conversation: {}", e);
        }
        bot.send_message(msg.chat.id, "Conversation cleared.").await?;
        return Ok(());
    }

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

    if text == "/tools" {
        let all_tools = agent.all_tool_definitions();
        let mut tool_list = String::from("Available tools:\n\n");
        for tool in &all_tools {
            tool_list.push_str(&format!("  - {}: {}\n", tool.function.name, tool.function.description));
        }
        bot.send_message(msg.chat.id, tool_list).await?;
        return Ok(());
    }

    // Send "typing" indicator
    bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing)
        .await
        .ok();

    // Build platform-agnostic message
    let incoming = IncomingMessage {
        platform: "telegram".to_string(),
        user_id: user_id.to_string(),
        chat_id: msg.chat.id.0.to_string(),
        user_name,
        text,
    };

    // Process through agent
    match agent.process_message(&incoming).await {
        Ok(response) => {
            for chunk in split_message(&response, 4000) {
                bot.send_message(msg.chat.id, chunk).await.ok();
            }
        }
        Err(e) => {
            error!("Error processing message: {:#}", e);
            bot.send_message(msg.chat.id, format!("Error: {}", e)).await?;
        }
    }

    Ok(())
}
```

**Step 2: Update `src/main.rs` to use Agent and Telegram platform**

Replace the AppState creation and bot::run with:

```rust
use crate::agent::Agent;

// Create the agent
let agent = Arc::new(Agent::new(config.clone(), mcp_manager, memory, skills));

// Run Telegram platform
info!("Bot is starting...");
platform::telegram::run(
    agent,
    config.telegram.allowed_user_ids.clone(),
    &config.telegram.bot_token,
).await?;
```

**Step 3: Deprecate `src/bot.rs`**

Remove `mod bot;` from `src/main.rs`. The file `src/bot.rs` can be deleted or kept for reference. All its logic now lives in `src/agent.rs` and `src/platform/telegram.rs`.

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/platform/telegram.rs src/main.rs
git rm src/bot.rs  # or just remove mod bot; and delete later
git commit -m "feat: refactor Telegram as platform adapter using Agent core"
```

---

## Phase 5: Background Task Scheduler

### Task 11: Add Scheduler Dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add tokio-cron-scheduler dependency**

```toml
# Background task scheduler
tokio-cron-scheduler = "0.13"
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat: add tokio-cron-scheduler dependency"
```

---

### Task 12: Create Scheduler Module

**Files:**
- Create: `src/scheduler/mod.rs`
- Create: `src/scheduler/tasks.rs`
- Modify: `src/main.rs` (add `mod scheduler;`)

**Step 1: Create `src/scheduler/mod.rs`**

```rust
pub mod tasks;

use anyhow::{Context, Result};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

/// Wrapper around tokio-cron-scheduler
pub struct Scheduler {
    inner: JobScheduler,
}

impl Scheduler {
    /// Create a new scheduler
    pub async fn new() -> Result<Self> {
        let inner = JobScheduler::new()
            .await
            .context("Failed to create job scheduler")?;
        Ok(Self { inner })
    }

    /// Add a cron job
    pub async fn add_cron_job<F>(&self, cron_expr: &str, name: &str, task: F) -> Result<()>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync + 'static,
    {
        let job_name = name.to_string();
        let job = Job::new_async(cron_expr, move |_uuid, _lock| {
            let name = job_name.clone();
            let fut = task();
            Box::pin(async move {
                info!("Running scheduled task: {}", name);
                fut.await;
            })
        })
        .with_context(|| format!("Failed to create cron job: {}", name))?;

        self.inner
            .add(job)
            .await
            .with_context(|| format!("Failed to add job: {}", name))?;

        info!("Scheduled task '{}' with cron: {}", name, cron_expr);
        Ok(())
    }

    /// Add a one-shot delayed job
    pub async fn add_delayed_job<F>(
        &self,
        delay: std::time::Duration,
        name: &str,
        task: F,
    ) -> Result<()>
    where
        F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync + 'static,
    {
        let job_name = name.to_string();
        let job = Job::new_one_shot_async(delay, move |_uuid, _lock| {
            let name = job_name.clone();
            let fut = task();
            Box::pin(async move {
                info!("Running one-shot task: {}", name);
                fut.await;
            })
        })
        .with_context(|| format!("Failed to create one-shot job: {}", name))?;

        self.inner
            .add(job)
            .await
            .with_context(|| format!("Failed to add one-shot job: {}", name))?;

        info!("Scheduled one-shot task '{}' in {:?}", name, delay);
        Ok(())
    }

    /// Start the scheduler
    pub async fn start(&self) -> Result<()> {
        self.inner
            .start()
            .await
            .context("Failed to start scheduler")?;
        info!("Scheduler started");
        Ok(())
    }

    /// Shutdown the scheduler
    pub async fn shutdown(&mut self) -> Result<()> {
        self.inner
            .shutdown()
            .await
            .context("Failed to shutdown scheduler")?;
        info!("Scheduler stopped");
        Ok(())
    }
}
```

**Step 2: Create `src/scheduler/tasks.rs`**

```rust
use std::sync::Arc;

use tracing::info;

use crate::memory::MemoryStore;
use crate::scheduler::Scheduler;

/// Register built-in background tasks
pub async fn register_builtin_tasks(
    scheduler: &Scheduler,
    memory: MemoryStore,
) -> anyhow::Result<()> {
    // Heartbeat — log that the bot is alive every hour
    scheduler
        .add_cron_job("0 0 * * * *", "heartbeat", || {
            Box::pin(async {
                info!("Heartbeat: bot is alive");
            })
        })
        .await?;

    // Example: periodic memory cleanup could be added here
    // scheduler.add_cron_job("0 0 3 * * *", "memory-cleanup", move || { ... }).await?;

    Ok(())
}
```

**Step 3: Register module in `src/main.rs`**

Add `mod scheduler;`.

**Step 4: Wire scheduler in `src/main.rs`**

After agent creation, before platform start:

```rust
use crate::scheduler::Scheduler;
use crate::scheduler::tasks::register_builtin_tasks;

// Initialize scheduler
let scheduler = Scheduler::new().await?;
register_builtin_tasks(&scheduler, memory.clone()).await?;
scheduler.start().await?;
info!("  Scheduler: active");
```

**Step 5: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 6: Commit**

```bash
git add src/scheduler/ src/main.rs
git commit -m "feat: add background task scheduler with tokio-cron-scheduler"
```

---

## Phase 6: Feature Flags

### Task 13: Add Cargo Feature Flags

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add feature flags to `Cargo.toml`**

```toml
[features]
default = ["telegram", "memory", "skills", "scheduler"]
telegram = ["dep:teloxide"]
# discord = ["dep:serenity", "dep:poise"]  # Future
memory = ["dep:rusqlite", "dep:uuid"]
skills = []
scheduler = ["dep:tokio-cron-scheduler"]
```

Update dependencies to be optional where appropriate:

```toml
teloxide = { version = "0.17", features = ["macros"], optional = true }
rusqlite = { version = "0.34", features = ["bundled", "modern_full"], optional = true }
uuid = { version = "1", features = ["v4"], optional = true }
tokio-cron-scheduler = { version = "0.13", optional = true }
```

**Step 2: Add `#[cfg(feature = "...")]` guards to modules in `src/main.rs`**

```rust
#[cfg(feature = "memory")]
mod memory;
#[cfg(feature = "telegram")]
mod platform;
#[cfg(feature = "skills")]
mod skills;
#[cfg(feature = "scheduler")]
mod scheduler;
```

**Step 3: Verify it compiles with all features**

Run: `cargo check --all-features`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add Cargo.toml src/main.rs
git commit -m "feat: add cargo feature flags for modular compilation"
```

---

## Phase 7: Create Example Agent Skills

### Task 14: Create Bot Agent Skills Directory with Example Skills

**Files:**
- Create: `skills/coding-assistant.md`
- Create: `skills/memory-manager.md`

**Step 1: Create `skills/coding-assistant.md`**

```markdown
---
name: coding-assistant
description: Help users write, review, and debug code
tags: [coding, development]
---

# Coding Assistant

When the user asks for help with code:

1. **Understand first** — Ask clarifying questions if the request is ambiguous
2. **Read before writing** — Use read_file to understand existing code before modifying
3. **Small changes** — Make focused, minimal changes. Don't refactor unrelated code
4. **Explain your reasoning** — Briefly explain what you changed and why
5. **Test awareness** — Suggest how to test the changes if applicable

When reviewing code:
- Point out bugs, security issues, and performance problems
- Suggest improvements but don't over-engineer
- Be specific — reference line numbers and provide fixed code

When debugging:
- Ask for error messages and reproduction steps
- Use execute_command to investigate (check logs, run tests)
- Explain the root cause, not just the fix
```

**Step 2: Create `skills/memory-manager.md`**

```markdown
---
name: memory-manager
description: Proactively remember and recall useful information
tags: [memory, learning]
---

# Memory Manager

You have persistent memory tools: `remember`, `recall`, and `search_memory`.

## When to Remember

Proactively use the `remember` tool when:
- The user tells you their name, preferences, or important context
- You learn something about their project or workflow
- The user corrects you — remember the correction
- You discover useful facts during tool use

Categories to use:
- `user_preference` — User's stated preferences (language, style, etc.)
- `user_info` — Name, role, timezone, etc.
- `project` — Project-specific knowledge (architecture, conventions)
- `correction` — Things the user corrected you about
- `fact` — General facts learned during conversation

## When to Recall

- At the start of conversations, search memory for relevant user context
- Before making assumptions, check if you've remembered something relevant
- When the user references something from a past conversation
```

**Step 3: Commit**

```bash
git add skills/
git commit -m "feat: add example agent skills for coding assistant and memory manager"
```

---

## Phase 8: Final Integration and Cleanup

### Task 15: Update main.rs with Full Initialization Flow

**Files:**
- Modify: `src/main.rs` (complete rewrite of main function)

**Step 1: Write the complete `src/main.rs`**

```rust
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
use crate::scheduler::Scheduler;
use crate::scheduler::tasks::register_builtin_tasks;
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

    // Initialize memory store
    let memory = MemoryStore::open(&config.memory.database_path)
        .context("Failed to initialize memory store")?;
    info!("  Database: {}", config.memory.database_path.display());

    // Initialize MCP connections
    let mut mcp_manager = McpManager::new();
    mcp_manager.connect_all(&config.mcp_servers).await;

    // Load skills
    let skills = load_skills_from_dir(&config.skills.directory).await?;
    info!("  Skills: {}", skills.len());

    // Create the agent
    let agent = Arc::new(Agent::new(config.clone(), mcp_manager, memory.clone(), skills));

    // Initialize scheduler
    let scheduler = Scheduler::new().await?;
    register_builtin_tasks(&scheduler, memory).await?;
    scheduler.start().await?;
    info!("  Scheduler: active");

    // Run the Telegram platform
    info!("Bot is starting...");
    platform::telegram::run(
        agent,
        config.telegram.allowed_user_ids.clone(),
        &config.telegram.bot_token,
    )
    .await?;

    Ok(())
}
```

**Step 2: Delete `src/bot.rs`**

```bash
rm src/bot.rs
```

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/main.rs
git rm src/bot.rs
git commit -m "feat: complete AI agent framework integration — replace bot.rs with modular architecture"
```

---

### Task 16: Verify Full Build and Test

**Step 1: Full build**

Run: `cargo build`
Expected: Builds successfully with no errors

**Step 2: Check for warnings**

Run: `cargo build 2>&1 | grep -i warning`
Expected: Minimal/no warnings

**Step 3: Run clippy**

Run: `cargo clippy -- -W clippy::all`
Expected: No errors (warnings OK for now)

**Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: address clippy warnings and build issues"
```

**Step 5: Push to branch**

```bash
git push -u origin claude/ai-agent-framework-BEBZn
```

---

## Summary of New Module Responsibilities

| Module | Purpose |
|--------|---------|
| `agent.rs` | Core agentic loop — platform-agnostic message processing with LLM + tools |
| `memory/` | SQLite-backed persistent conversations + knowledge base with FTS5 |
| `platform/` | Platform adapters (Telegram now, Discord-ready) via `Platform` trait |
| `skills/` | Load markdown skill files → inject into system prompt |
| `scheduler/` | Background tasks via tokio-cron-scheduler (heartbeat, reminders) |

## Architecture Diagram

```
┌─────────────────────────────────────────────────────┐
│                    main.rs                          │
│  Config → Memory → MCP → Skills → Agent → Platform │
└─────────────────────┬───────────────────────────────┘
                      │
          ┌───────────┼───────────┐
          ▼           ▼           ▼
    ┌──────────┐ ┌─────────┐ ┌──────────┐
    │ Telegram │ │  Agent  │ │Scheduler │
    │ Platform │ │  Core   │ │  Tasks   │
    └────┬─────┘ └────┬────┘ └──────────┘
         │            │
         │     ┌──────┼──────┐
         │     ▼      ▼      ▼
         │  ┌─────┐ ┌────┐ ┌──────┐
         │  │ LLM │ │MCP │ │Tools │
         │  └─────┘ └────┘ └──────┘
         │            │
         │     ┌──────┼──────┐
         │     ▼      ▼      ▼
         │  ┌──────┐ ┌──────────┐ ┌────────┐
         └─▶│Memory│ │Knowledge │ │Skills  │
            │(SQLite)│(FTS5)   │ │(.md)   │
            └──────┘ └──────────┘ └────────┘
```

## Dependencies Added

| Crate | Purpose |
|-------|---------|
| `rusqlite` (bundled, modern_full) | SQLite database with FTS5 |
| `chrono` (serde) | Timestamps |
| `uuid` (v4) | Message/entry IDs |
| `async-trait` | Async trait support for Platform |
| `tokio-cron-scheduler` | Background task scheduling |
