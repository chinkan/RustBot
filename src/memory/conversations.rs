use anyhow::{Context, Result};
use uuid::Uuid;

use super::MemoryStore;
use crate::llm::ChatMessage;

/// Cast a &[f32] to &[u8] for SQLite blob storage
pub(crate) fn f32_slice_to_bytes(floats: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(floats.as_ptr() as *const u8, floats.len() * 4) }
}

/// Cast Vec<f32> to Vec<u8> for SQLite blob storage
pub(crate) fn f32_vec_to_bytes(floats: &[f32]) -> Vec<u8> {
    f32_slice_to_bytes(floats).to_vec()
}

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

    /// Save a message to a conversation, with optional vector embedding.
    /// Embedding generation happens in the background so this returns quickly.
    pub async fn save_message(
        &self,
        conversation_id: &str,
        message: &ChatMessage,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let tool_calls_json = message
            .tool_calls
            .as_ref()
            .map(|tc| serde_json::to_string(tc).unwrap_or_default());

        // Determine whether this message should be embedded
        let should_embed = self.embeddings.is_available()
            && message.content.as_ref().is_some_and(|c| !c.is_empty())
            && message.role != "tool";

        let conn = self.conn.lock().await;

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

        let rowid = conn.last_insert_rowid();

        // Update conversation timestamp
        conn.execute(
            "UPDATE conversations SET updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![conversation_id],
        )?;

        // Release DB lock before spawning background embedding task
        drop(conn);

        // Generate and store the vector embedding in the background so the
        // caller isn't blocked waiting for the embedding API roundtrip.
        if should_embed {
            if let Some(content) = message.content.clone() {
                let conn_bg = std::sync::Arc::clone(&self.conn);
                let embeddings_bg = std::sync::Arc::clone(&self.embeddings);
                tokio::spawn(async move {
                    if let Some(emb) = embeddings_bg.try_embed_one(&content).await {
                        let conn = conn_bg.lock().await;
                        let embedding_bytes = f32_slice_to_bytes(&emb);
                        let _ = conn.execute(
                            "INSERT INTO message_embeddings (rowid, embedding) VALUES (?1, ?2)",
                            rusqlite::params![rowid, embedding_bytes],
                        );
                    }
                });
            }
        }

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

    /// Clear a conversation (delete all its messages and embeddings)
    pub async fn clear_conversation(&self, platform: &str, user_id: &str) -> Result<()> {
        let conn = self.conn.lock().await;

        // Delete embeddings for messages in this conversation
        conn.execute(
            "DELETE FROM message_embeddings WHERE rowid IN (
                SELECT m.rowid FROM messages m
                JOIN conversations c ON m.conversation_id = c.id
                WHERE c.platform = ?1 AND c.user_id = ?2
            )",
            rusqlite::params![platform, user_id],
        )?;

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

    /// Hybrid search across messages using Reciprocal Rank Fusion (vector + FTS5).
    /// Pass `query_embedding = None` to fall back to FTS5-only search.
    /// Use a pre-computed embedding to avoid redundant API calls when the
    /// caller already has one (e.g. sharing it with `search_knowledge_with_embedding`).
    pub async fn search_messages_with_embedding(
        &self,
        query: &str,
        query_embedding: Option<Vec<f32>>,
        limit: usize,
    ) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().await;

        if let Some(ref qe) = query_embedding {
            // Hybrid search with Reciprocal Rank Fusion
            let query_bytes = f32_vec_to_bytes(qe);
            let sql = "
                WITH vec_matches AS (
                    SELECT rowid, distance,
                           row_number() OVER (ORDER BY distance) as rank_number
                    FROM message_embeddings
                    WHERE embedding MATCH ?1
                    ORDER BY distance
                    LIMIT ?2
                ),
                fts_matches AS (
                    SELECT rowid,
                           row_number() OVER (ORDER BY rank) as rank_number
                    FROM messages_fts
                    WHERE messages_fts MATCH ?3
                    LIMIT ?2
                )
                SELECT m.role, m.content, m.tool_calls, m.tool_call_id,
                       coalesce(1.0 / (60 + fts.rank_number), 0.0) * 0.5
                       + coalesce(1.0 / (60 + vec.rank_number), 0.0) * 0.5 as combined_rank
                FROM messages m
                LEFT JOIN vec_matches vec ON m.rowid = vec.rowid
                LEFT JOIN fts_matches fts ON m.rowid = fts.rowid
                WHERE vec.rowid IS NOT NULL OR fts.rowid IS NOT NULL
                ORDER BY combined_rank DESC
                LIMIT ?2
            ";

            let search_limit = (limit * 3) as i64;
            let mut stmt = conn.prepare(sql)?;
            let messages = stmt
                .query_map(rusqlite::params![query_bytes, search_limit, query], |row| {
                    parse_message_row(row)
                })?
                .collect::<Result<Vec<_>, _>>()
                .context("Failed to hybrid-search messages")?;

            Ok(messages.into_iter().take(limit).collect())
        } else {
            // FTS5-only fallback
            let sql = "
                SELECT m.role, m.content, m.tool_calls, m.tool_call_id
                FROM messages m
                JOIN messages_fts fts ON m.rowid = fts.rowid
                WHERE messages_fts MATCH ?1
                ORDER BY fts.rank
                LIMIT ?2
            ";
            let mut stmt = conn.prepare(sql)?;
            let messages = stmt
                .query_map(rusqlite::params![query, limit as i64], |row| {
                    parse_message_row(row)
                })?
                .collect::<Result<Vec<_>, _>>()
                .context("Failed to FTS-search messages")?;

            Ok(messages)
        }
    }
}

fn parse_message_row(row: &rusqlite::Row) -> rusqlite::Result<ChatMessage> {
    let tool_calls_json: Option<String> = row.get(2)?;
    let tool_calls = tool_calls_json.and_then(|json| serde_json::from_str(&json).ok());

    Ok(ChatMessage {
        role: row.get(0)?,
        content: row.get(1)?,
        tool_calls,
        tool_call_id: row.get(3)?,
    })
}
