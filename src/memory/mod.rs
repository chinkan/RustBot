pub mod conversations;
pub mod embeddings;
pub mod knowledge;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::memory::embeddings::{EmbeddingConfig, EmbeddingEngine};

/// Thread-safe SQLite memory store with hybrid vector+FTS5 search
#[derive(Clone)]
pub struct MemoryStore {
    conn: Arc<Mutex<Connection>>,
    pub embeddings: Arc<EmbeddingEngine>,
}

impl MemoryStore {
    /// Open or create the SQLite database at the given path.
    /// If `embedding_config` is provided, vector search is enabled alongside FTS5.
    /// If None, falls back to FTS5-only search.
    pub fn open(path: &Path, embedding_config: Option<EmbeddingConfig>) -> Result<Self> {
        // Register sqlite-vec extension before opening any connection
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database: {}", path.display()))?;

        // Enable WAL mode for better concurrent read performance
        // journal_mode PRAGMA always returns the resulting mode, so use query_row
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        let embeddings = EmbeddingEngine::new(embedding_config);

        // Run migrations on the raw connection before wrapping in Mutex.
        // This avoids blocking_lock() panic when called from async context.
        Self::run_migrations(&conn, embeddings.dimensions())?;

        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            embeddings: Arc::new(embeddings),
        };

        info!("Memory store initialized at: {}", path.display());
        Ok(store)
    }

    /// Open an in-memory database (for testing)
    pub fn open_in_memory() -> Result<Self> {
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        let embeddings = EmbeddingEngine::new(None);

        Self::run_migrations(&conn, embeddings.dimensions())?;

        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            embeddings: Arc::new(embeddings),
        };
        Ok(store)
    }

    fn run_migrations(conn: &Connection, dims: usize) -> Result<()> {

        conn.execute_batch(
            "
            -- Conversations table
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                platform TEXT NOT NULL,
                user_id TEXT NOT NULL,
                started_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Messages table
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

            -- Knowledge table
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

            -- FTS5 virtual tables for full-text search
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                content,
                content=messages,
                content_rowid=rowid
            );

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
                INSERT INTO messages_fts(messages_fts, rowid, content)
                    VALUES('delete', OLD.rowid, OLD.content);
            END;

            CREATE TRIGGER IF NOT EXISTS knowledge_fts_insert AFTER INSERT ON knowledge BEGIN
                INSERT INTO knowledge_fts(rowid, key, value)
                    VALUES (NEW.rowid, NEW.key, NEW.value);
            END;

            CREATE TRIGGER IF NOT EXISTS knowledge_fts_delete AFTER DELETE ON knowledge BEGIN
                INSERT INTO knowledge_fts(knowledge_fts, rowid, key, value)
                    VALUES('delete', OLD.rowid, OLD.key, OLD.value);
            END;

            CREATE TRIGGER IF NOT EXISTS knowledge_fts_update AFTER UPDATE ON knowledge BEGIN
                INSERT INTO knowledge_fts(knowledge_fts, rowid, key, value)
                    VALUES('delete', OLD.rowid, OLD.key, OLD.value);
                INSERT INTO knowledge_fts(rowid, key, value)
                    VALUES (NEW.rowid, NEW.key, NEW.value);
            END;
            ",
        )?;

        // Create vec0 virtual tables for vector search (sqlite-vec)
        // vec0 doesn't support IF NOT EXISTS, so check first
        let create_vec_table = |conn: &Connection, name: &str, dims: usize| -> Result<()> {
            let exists: bool = conn
                .query_row(
                    &format!(
                        "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name='{}'",
                        name
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            if !exists {
                conn.execute_batch(&format!(
                    "CREATE VIRTUAL TABLE {} USING vec0(embedding float[{}]);",
                    name, dims
                ))?;
            }
            Ok(())
        };

        create_vec_table(conn, "message_embeddings", dims)?;
        create_vec_table(conn, "knowledge_embeddings", dims)?;

        Ok(())
    }
}
