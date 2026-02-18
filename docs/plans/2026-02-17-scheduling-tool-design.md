# Scheduling Tool Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add LLM-accessible scheduling tools so users can register one-shot and recurring tasks that trigger a full agentic loop when they fire, surviving bot restarts via SQLite persistence.

**Architecture:** A new `ScheduledTaskStore` wraps the existing SQLite connection and owns CRUD for a `scheduled_tasks` table added to the existing DB migration. The `Agent` struct grows three fields (`task_store`, `scheduler`, `bot`) and gains three new tool handlers; job closures hold a `Weak<Agent>` to break the reference cycle. On startup, `restore_scheduled_tasks()` re-registers every `active` task from the DB into `tokio-cron-scheduler`.

**Tech Stack:** `tokio-cron-scheduler` (already in Cargo.toml), `rusqlite` (already in use), `teloxide::Bot` (already in use), `chrono` (already in use), `uuid` (already in use).

---

## Quick-reference: files touched

| Action | Path |
|--------|------|
| Create | `src/scheduler/reminders.rs` |
| Modify | `src/memory/mod.rs` |
| Modify | `src/scheduler/mod.rs` |
| Modify | `src/scheduler/tasks.rs` |
| Modify | `src/agent.rs` |
| Modify | `src/main.rs` |
| Modify | `src/platform/telegram.rs` |

---

## Task 1: Add `scheduled_tasks` migration + `connection()` accessor

**Files:**
- Modify: `src/memory/mod.rs`

The existing `run_migrations()` call is the right place. `ScheduledTaskStore` will need the raw `Arc<Mutex<Connection>>`, so expose it.

**Step 1: Write the failing test**

Add at the bottom of `src/memory/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduled_tasks_table_exists() {
        let memory = MemoryStore::open_in_memory().unwrap();
        let conn = memory.connection();
        let conn = conn.blocking_lock();
        let exists: bool = conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name='scheduled_tasks'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
    }

    #[test]
    fn test_connection_accessor_returns_working_connection() {
        let memory = MemoryStore::open_in_memory().unwrap();
        let conn = memory.connection();
        let conn = conn.blocking_lock();
        let n: i64 = conn
            .query_row("SELECT 42", [], |row| row.get(0))
            .unwrap();
        assert_eq!(n, 42);
    }
}
```

**Step 2: Run to confirm failure**

```
cargo test test_scheduled_tasks_table_exists -- --nocapture
```
Expected: FAIL — `scheduled_tasks` table does not exist yet.

**Step 3: Add `connection()` method and migration**

In `src/memory/mod.rs`, inside `impl MemoryStore` (after the `open_in_memory` fn):

```rust
/// Expose the underlying connection for modules that share the DB.
pub fn connection(&self) -> Arc<Mutex<Connection>> {
    Arc::clone(&self.conn)
}
```

In `run_migrations()`, append to the `execute_batch` SQL string (before the closing `"`):

```sql
-- Scheduled tasks for user-registered reminders / recurring jobs
CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id               TEXT PRIMARY KEY,
    scheduler_job_id TEXT,
    user_id          TEXT NOT NULL,
    chat_id          TEXT NOT NULL,
    platform         TEXT NOT NULL,
    trigger_type     TEXT NOT NULL,
    trigger_value    TEXT NOT NULL,
    prompt           TEXT NOT NULL,
    description      TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'active',
    created_at       TEXT NOT NULL,
    next_run_at      TEXT
);

CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_user
    ON scheduled_tasks(user_id, status);
```

**Step 4: Run tests to verify they pass**

```
cargo test test_scheduled_tasks -- --nocapture
```
Expected: PASS (both tests).

**Step 5: Commit**

```bash
git add src/memory/mod.rs
git commit -m "feat(memory): add scheduled_tasks migration and connection() accessor"
```

---

## Task 2: `ScheduledTaskStore` — CRUD layer

**Files:**
- Create: `src/scheduler/reminders.rs`
- Modify: `src/scheduler/mod.rs` (add `pub mod reminders;`)

**Step 1: Write the failing tests**

Create `src/scheduler/reminders.rs` with the struct, a stub `impl`, and tests:

```rust
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ScheduledTask {
    pub id: String,
    pub scheduler_job_id: Option<String>,
    pub user_id: String,
    pub chat_id: String,
    pub platform: String,
    pub trigger_type: String,
    pub trigger_value: String,
    pub prompt: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
    pub next_run_at: Option<String>,
}

#[derive(Clone)]
pub struct ScheduledTaskStore {
    conn: Arc<Mutex<Connection>>,
}

impl ScheduledTaskStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub async fn create(&self, task: &ScheduledTask) -> Result<()> {
        todo!()
    }

    pub async fn list_active_for_user(&self, user_id: &str) -> Result<Vec<ScheduledTask>> {
        todo!()
    }

    pub async fn list_all_active(&self) -> Result<Vec<ScheduledTask>> {
        todo!()
    }

    pub async fn set_status(&self, id: &str, status: &str) -> Result<()> {
        todo!()
    }

    pub async fn update_scheduler_job_id(&self, id: &str, job_id: &str) -> Result<()> {
        todo!()
    }

    pub async fn update_next_run_at(&self, id: &str, next_run_at: &str) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryStore;

    fn make_task(id: &str, user_id: &str, trigger_type: &str) -> ScheduledTask {
        ScheduledTask {
            id: id.to_string(),
            scheduler_job_id: None,
            user_id: user_id.to_string(),
            chat_id: "123456".to_string(),
            platform: "telegram".to_string(),
            trigger_type: trigger_type.to_string(),
            trigger_value: "2099-01-01T09:00:00".to_string(),
            prompt: "Say hello!".to_string(),
            description: "Test task".to_string(),
            status: "active".to_string(),
            created_at: "2026-01-01T00:00:00".to_string(),
            next_run_at: Some("2099-01-01T09:00:00".to_string()),
        }
    }

    #[tokio::test]
    async fn test_create_and_list() {
        let memory = MemoryStore::open_in_memory().unwrap();
        let store = ScheduledTaskStore::new(memory.connection());

        let task = make_task("task-1", "user-1", "one_shot");
        store.create(&task).await.unwrap();

        let tasks = store.list_active_for_user("user-1").await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "task-1");
    }

    #[tokio::test]
    async fn test_list_only_returns_active() {
        let memory = MemoryStore::open_in_memory().unwrap();
        let store = ScheduledTaskStore::new(memory.connection());

        store.create(&make_task("task-a", "user-2", "one_shot")).await.unwrap();
        store.create(&make_task("task-b", "user-2", "one_shot")).await.unwrap();
        store.set_status("task-b", "cancelled").await.unwrap();

        let tasks = store.list_active_for_user("user-2").await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "task-a");
    }

    #[tokio::test]
    async fn test_list_all_active_excludes_other_users_cancelled() {
        let memory = MemoryStore::open_in_memory().unwrap();
        let store = ScheduledTaskStore::new(memory.connection());

        store.create(&make_task("t1", "user-a", "recurring")).await.unwrap();
        store.create(&make_task("t2", "user-b", "one_shot")).await.unwrap();
        store.set_status("t2", "completed").await.unwrap();

        let all = store.list_all_active().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "t1");
    }

    #[tokio::test]
    async fn test_update_scheduler_job_id() {
        let memory = MemoryStore::open_in_memory().unwrap();
        let store = ScheduledTaskStore::new(memory.connection());

        store.create(&make_task("task-x", "user-3", "one_shot")).await.unwrap();
        store.update_scheduler_job_id("task-x", "sched-uuid-123").await.unwrap();

        let tasks = store.list_all_active().await.unwrap();
        assert_eq!(tasks[0].scheduler_job_id.as_deref(), Some("sched-uuid-123"));
    }
}
```

Add `pub mod reminders;` at the top of `src/scheduler/mod.rs`.

**Step 2: Run to confirm failure**

```
cargo test scheduler::reminders -- --nocapture
```
Expected: FAIL — `todo!()` panics.

**Step 3: Implement all methods**

Replace the `todo!()` stubs in `src/scheduler/reminders.rs`:

```rust
pub async fn create(&self, task: &ScheduledTask) -> Result<()> {
    let conn = self.conn.lock().await;
    conn.execute(
        "INSERT INTO scheduled_tasks
         (id, scheduler_job_id, user_id, chat_id, platform, trigger_type,
          trigger_value, prompt, description, status, created_at, next_run_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        rusqlite::params![
            task.id, task.scheduler_job_id, task.user_id, task.chat_id,
            task.platform, task.trigger_type, task.trigger_value, task.prompt,
            task.description, task.status, task.created_at, task.next_run_at,
        ],
    )
    .context("Failed to insert scheduled task")?;
    Ok(())
}

pub async fn list_active_for_user(&self, user_id: &str) -> Result<Vec<ScheduledTask>> {
    let conn = self.conn.lock().await;
    self.query_tasks(&conn, "WHERE user_id = ?1 AND status = 'active'", rusqlite::params![user_id])
}

pub async fn list_all_active(&self) -> Result<Vec<ScheduledTask>> {
    let conn = self.conn.lock().await;
    self.query_tasks(&conn, "WHERE status = 'active'", rusqlite::params![])
}

pub async fn set_status(&self, id: &str, status: &str) -> Result<()> {
    let conn = self.conn.lock().await;
    conn.execute(
        "UPDATE scheduled_tasks SET status = ?1 WHERE id = ?2",
        rusqlite::params![status, id],
    )
    .context("Failed to update task status")?;
    Ok(())
}

pub async fn update_scheduler_job_id(&self, id: &str, job_id: &str) -> Result<()> {
    let conn = self.conn.lock().await;
    conn.execute(
        "UPDATE scheduled_tasks SET scheduler_job_id = ?1 WHERE id = ?2",
        rusqlite::params![job_id, id],
    )
    .context("Failed to update scheduler_job_id")?;
    Ok(())
}

pub async fn update_next_run_at(&self, id: &str, next_run_at: &str) -> Result<()> {
    let conn = self.conn.lock().await;
    conn.execute(
        "UPDATE scheduled_tasks SET next_run_at = ?1 WHERE id = ?2",
        rusqlite::params![next_run_at, id],
    )
    .context("Failed to update next_run_at")?;
    Ok(())
}

// Private helper — runs a SELECT with a WHERE clause fragment
fn query_tasks(
    &self,
    conn: &Connection,
    where_clause: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<ScheduledTask>> {
    let sql = format!(
        "SELECT id, scheduler_job_id, user_id, chat_id, platform, trigger_type,
                trigger_value, prompt, description, status, created_at, next_run_at
         FROM scheduled_tasks {}
         ORDER BY created_at ASC",
        where_clause
    );
    let mut stmt = conn.prepare(&sql).context("Failed to prepare query")?;
    let tasks = stmt
        .query_map(params, |row| {
            Ok(ScheduledTask {
                id:               row.get(0)?,
                scheduler_job_id: row.get(1)?,
                user_id:          row.get(2)?,
                chat_id:          row.get(3)?,
                platform:         row.get(4)?,
                trigger_type:     row.get(5)?,
                trigger_value:    row.get(6)?,
                prompt:           row.get(7)?,
                description:      row.get(8)?,
                status:           row.get(9)?,
                created_at:       row.get(10)?,
                next_run_at:      row.get(11)?,
            })
        })
        .context("Failed to map rows")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("Failed to collect rows")?;
    Ok(tasks)
}
```

Note: `query_tasks` takes `&Connection` (already locked) so it can share the lock. The public async methods lock, then call this helper.

**Step 4: Run tests**

```
cargo test scheduler::reminders -- --nocapture
```
Expected: PASS (4 tests).

**Step 5: Commit**

```bash
git add src/scheduler/reminders.rs src/scheduler/mod.rs
git commit -m "feat(scheduler): add ScheduledTaskStore with SQLite CRUD"
```

---

## Task 3: Extend `Scheduler` — return `Uuid`, add `add_one_shot_job`, `remove_job`

**Files:**
- Modify: `src/scheduler/mod.rs`

**Step 1: Write the failing tests**

Add to `src/scheduler/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_add_cron_job_returns_uuid() {
        let scheduler = Scheduler::new().await.unwrap();
        scheduler.start().await.unwrap();
        let id = scheduler
            .add_cron_job("0 * * * * *", "test-cron", || Box::pin(async {}))
            .await
            .unwrap();
        // Uuid is non-zero
        assert_ne!(id.as_u128(), 0);
    }

    #[tokio::test]
    async fn test_add_one_shot_job_returns_uuid() {
        let scheduler = Scheduler::new().await.unwrap();
        scheduler.start().await.unwrap();
        let id = scheduler
            .add_one_shot_job(Duration::from_secs(3600), "test-oneshot", || {
                Box::pin(async {})
            })
            .await
            .unwrap();
        assert_ne!(id.as_u128(), 0);
    }

    #[tokio::test]
    async fn test_remove_job_does_not_error() {
        let scheduler = Scheduler::new().await.unwrap();
        scheduler.start().await.unwrap();
        let id = scheduler
            .add_one_shot_job(Duration::from_secs(3600), "test-remove", || {
                Box::pin(async {})
            })
            .await
            .unwrap();
        // Should not error even if job hasn't fired
        scheduler.remove_job(id).await.unwrap();
    }
}
```

**Step 2: Run to confirm failure**

```
cargo test scheduler::tests -- --nocapture
```
Expected: FAIL — `add_cron_job` returns `()`, `add_one_shot_job` doesn't exist.

**Step 3: Implement the changes**

Replace the contents of `src/scheduler/mod.rs` (keeping `pub mod reminders;` at top, adding the new methods):

```rust
pub mod reminders;
pub mod tasks;

use anyhow::{Context, Result};
use std::time::Duration;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::info;
use uuid::Uuid;

/// Wrapper around tokio-cron-scheduler for background tasks
pub struct Scheduler {
    inner: JobScheduler,
}

impl Scheduler {
    pub async fn new() -> Result<Self> {
        let inner = JobScheduler::new()
            .await
            .context("Failed to create job scheduler")?;
        Ok(Self { inner })
    }

    /// Add a recurring cron job. Returns the job's UUID (for cancellation).
    pub async fn add_cron_job<F>(&self, cron_expr: &str, name: &str, task: F) -> Result<Uuid>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync
            + 'static,
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

        let id = self
            .inner
            .add(job)
            .await
            .with_context(|| format!("Failed to add job: {}", name))?;

        info!("Scheduled task '{}' with cron: {}", name, cron_expr);
        Ok(id)
    }

    /// Add a one-shot job that fires once after `delay`. Returns the job's UUID.
    pub async fn add_one_shot_job<F>(&self, delay: Duration, name: &str, task: F) -> Result<Uuid>
    where
        F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync
            + 'static,
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

        let id = self
            .inner
            .add(job)
            .await
            .with_context(|| format!("Failed to add one-shot job: {}", name))?;

        info!("One-shot task '{}' scheduled in {:?}", name, delay);
        Ok(id)
    }

    /// Remove a job by its UUID.
    pub async fn remove_job(&self, id: Uuid) -> Result<()> {
        self.inner
            .remove(&id)
            .await
            .with_context(|| format!("Failed to remove job: {}", id))?;
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
    #[allow(dead_code)]
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

Fix the existing caller in `src/scheduler/tasks.rs` — `add_cron_job` now returns `Result<Uuid>`, but it's used with `?` so the Uuid is just dropped. No change needed (compiles as-is).

**Step 4: Run tests**

```
cargo test scheduler::tests -- --nocapture
```
Expected: PASS (3 tests).

**Step 5: Commit**

```bash
git add src/scheduler/mod.rs
git commit -m "feat(scheduler): add_one_shot_job + remove_job, return Uuid from add_cron_job"
```

---

## Task 4: Create `Arc<Bot>` in `main.rs` and thread it through

**Files:**
- Modify: `src/main.rs`
- Modify: `src/platform/telegram.rs`

The goal: `Bot` is created once in `main.rs`, passed as `Arc<Bot>` to the Telegram platform runner instead of being created internally. This lets scheduled job closures send messages.

**Step 1: Read `src/platform/telegram.rs` to find the `run` signature and `Bot::new` call**

```
grep -n "Bot::new\|pub async fn run" src/platform/telegram.rs
```

**Step 2: Change `telegram::run` to accept `Arc<Bot>`**

In `src/platform/telegram.rs`, find:

```rust
pub async fn run(
    agent: Arc<Agent>,
    allowed_user_ids: Vec<u64>,
    bot_token: &str,
) -> Result<()> {
    let bot = Bot::new(bot_token);
```

Replace with:

```rust
pub async fn run(
    agent: Arc<Agent>,
    allowed_user_ids: Vec<u64>,
    bot: Arc<teloxide::Bot>,
) -> Result<()> {
```

Remove the `let bot = Bot::new(bot_token);` line that follows (the rest of the function is unchanged).

If the function uses `bot` as `Bot` (not `Arc<Bot>`), check if `teloxide` dispatcher accepts `Arc<Bot>` — teloxide's `Dispatcher::builder` accepts anything that implements `Requester`. `Arc<Bot>` does not automatically implement `Requester`, so we need to dereference. Use `(*bot).clone()` or `bot.as_ref().clone()` to get a plain `Bot` from the `Arc`:

```rust
pub async fn run(
    agent: Arc<Agent>,
    allowed_user_ids: Vec<u64>,
    bot: Arc<teloxide::Bot>,
) -> Result<()> {
    let bot = (*bot).clone();  // teloxide Bot is Clone; unwrap from Arc for dispatcher
```

**Step 3: Update `main.rs`**

In `src/main.rs`, replace:

```rust
platform::telegram::run(
    agent,
    config.telegram.allowed_user_ids.clone(),
    &config.telegram.bot_token,
)
.await?;
```

With:

```rust
let bot = Arc::new(teloxide::Bot::new(&config.telegram.bot_token));

platform::telegram::run(
    agent,
    config.telegram.allowed_user_ids.clone(),
    Arc::clone(&bot),
)
.await?;
```

Add `use teloxide::Bot;` if not already imported at the top of `main.rs`. (Or use the full path `teloxide::Bot::new(...)` inline as shown.)

**Step 4: Verify compilation**

```
cargo check
```
Expected: compiles cleanly.

**Step 5: Commit**

```bash
git add src/main.rs src/platform/telegram.rs
git commit -m "refactor(platform): create Bot in main.rs, pass Arc<Bot> to telegram runner"
```

---

## Task 5: Add scheduling fields to `Agent`, wire `Arc::new_cyclic`

**Files:**
- Modify: `src/agent.rs`
- Modify: `src/main.rs`

**Why `Arc::new_cyclic`:** Job closures need `Arc<Agent>` (to call `process_message`) but `Agent` needs `Arc<Scheduler>` (to register jobs). We break this cycle by storing `Weak<Agent>` in `Agent.self_weak` and using it in closures. `Arc::new_cyclic` lets us create the Weak ref during construction.

**Step 1: Update `Agent` struct**

In `src/agent.rs`, add imports at top:

```rust
use std::sync::{Arc, Weak};
use crate::scheduler::Scheduler;
use crate::scheduler::reminders::ScheduledTaskStore;
use teloxide::Bot;
```

Add fields to `Agent`:

```rust
pub struct Agent {
    pub llm: LlmClient,
    pub config: Config,
    pub mcp: McpManager,
    pub memory: MemoryStore,
    pub skills: SkillRegistry,
    pub task_store: ScheduledTaskStore,
    pub scheduler: Arc<Scheduler>,
    pub bot: Arc<Bot>,
    pub self_weak: Weak<Agent>,
}
```

Update `Agent::new` signature:

```rust
pub fn new(
    config: Config,
    mcp: McpManager,
    memory: MemoryStore,
    skills: SkillRegistry,
    task_store: ScheduledTaskStore,
    scheduler: Arc<Scheduler>,
    bot: Arc<Bot>,
    self_weak: Weak<Agent>,
) -> Self {
    let llm = LlmClient::new(config.openrouter.clone());
    Self {
        llm,
        config,
        mcp,
        memory,
        skills,
        task_store,
        scheduler,
        bot,
        self_weak,
    }
}
```

**Step 2: Update `main.rs` to use `Arc::new_cyclic`**

In `src/main.rs`, replace the agent creation block:

```rust
// Create the agent
let agent = Arc::new(Agent::new(
    config.clone(),
    mcp_manager,
    memory.clone(),
    skills,
));
```

With:

```rust
// Create ScheduledTaskStore sharing the existing SQLite connection
let task_store = crate::scheduler::reminders::ScheduledTaskStore::new(memory.connection());

// Scheduler needs Arc so closures can hold Weak<Agent> without cycle
let scheduler = Arc::new(crate::scheduler::Scheduler::new().await?);

// Arc::new_cyclic so Agent can hold Weak<Self> for job closure captures
let agent = Arc::new_cyclic(|weak| {
    Agent::new(
        config.clone(),
        mcp_manager,
        memory.clone(),
        skills,
        task_store.clone(),
        Arc::clone(&scheduler),
        Arc::clone(&bot),
        weak.clone(),
    )
});
```

Also replace the existing scheduler init block:

```rust
// Initialize background task scheduler
let scheduler = Scheduler::new().await?;
register_builtin_tasks(&scheduler, memory).await?;
scheduler.start().await?;
```

With:

```rust
register_builtin_tasks(&scheduler, memory).await?;
scheduler.start().await?;
```

(Scheduler is now created above, before agent.)

**Step 3: Verify compilation**

```
cargo check
```
Expected: compiles cleanly.

**Step 4: Commit**

```bash
git add src/agent.rs src/main.rs
git commit -m "feat(agent): add task_store, scheduler, bot, self_weak fields for scheduling"
```

---

## Task 6: Add scheduling tool definitions to `Agent`

**Files:**
- Modify: `src/agent.rs`

**Step 1: Add `scheduling_tool_definitions` method and wire it in**

In `src/agent.rs`, inside `impl Agent`, add after `memory_tool_definitions`:

```rust
fn scheduling_tool_definitions(&self) -> Vec<ToolDefinition> {
    use serde_json::json;

    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "schedule_task".to_string(),
                description: concat!(
                    "Schedule a task to run at a future time. The prompt will be executed by the AI agent ",
                    "at the scheduled time (full agentic loop). ",
                    "For one_shot: trigger_value is ISO 8601 datetime e.g. '2026-03-05T12:00:00'. ",
                    "For recurring: trigger_value is a 6-field cron expression ",
                    "(sec min hour day month weekday) e.g. '0 0 9 * * MON' for every Monday at 9am."
                ).to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "trigger_type":  { "type": "string", "enum": ["one_shot", "recurring"] },
                        "trigger_value": { "type": "string", "description": "ISO 8601 datetime (one_shot) or 6-field cron expression (recurring)" },
                        "prompt":        { "type": "string", "description": "The message the agent will process at trigger time" },
                        "description":   { "type": "string", "description": "Human-readable label for this task" }
                    },
                    "required": ["trigger_type", "trigger_value", "prompt", "description"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "list_scheduled_tasks".to_string(),
                description: "List all active scheduled tasks for the current user.".to_string(),
                parameters: json!({ "type": "object", "properties": {} }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "cancel_scheduled_task".to_string(),
                description: "Cancel an active scheduled task by its ID.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string", "description": "The task ID from list_scheduled_tasks" }
                    },
                    "required": ["task_id"]
                }),
            },
        },
    ]
}
```

In the two places that build `all_tools` inside `process_message` and `all_tool_definitions`:

```rust
// existing:
all_tools.extend(self.memory_tool_definitions());
// add after:
all_tools.extend(self.scheduling_tool_definitions());
```

**Step 2: Verify compilation**

```
cargo check
```
Expected: compiles.

**Step 3: Commit**

```bash
git add src/agent.rs
git commit -m "feat(agent): expose schedule_task, list_scheduled_tasks, cancel_scheduled_task tools"
```

---

## Task 7: Implement `schedule_task` tool execution

**Files:**
- Modify: `src/agent.rs`

This is the most complex tool. It needs to: parse the trigger value, compute the delay (one-shot) or validate the cron expr (recurring), persist to DB, register with the scheduler (capturing `Weak<Agent>` in the closure), and update the `scheduler_job_id`.

**Step 1: Add a helper module for trigger parsing**

At the top of the `execute_tool` match arm section (or as a free function at the bottom of `agent.rs`), add:

```rust
/// Parse an ISO 8601 datetime string and return the Duration until it fires.
/// Returns Err if the string is invalid or the time is in the past.
fn parse_one_shot_delay(trigger_value: &str) -> Result<std::time::Duration> {
    use chrono::{DateTime, Local, NaiveDateTime, TimeZone};

    // Try parsing as naive local datetime first (no timezone)
    let dt = NaiveDateTime::parse_from_str(trigger_value, "%Y-%m-%dT%H:%M:%S")
        .map(|naive| Local.from_local_datetime(&naive).single())
        .ok()
        .flatten()
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|| DateTime::parse_from_rfc3339(trigger_value).ok().map(|dt| dt.with_timezone(&chrono::Utc)))
        .ok_or_else(|| anyhow::anyhow!(
            "Invalid datetime '{}'. Use ISO 8601 format e.g. '2026-03-05T12:00:00'",
            trigger_value
        ))?;

    let now = chrono::Utc::now();
    if dt <= now {
        anyhow::bail!("That time has already passed ({}). Please provide a future datetime.", trigger_value);
    }

    let duration = (dt - now).to_std().context("Duration conversion failed")?;
    Ok(duration)
}

/// Validate a 6-field cron expression by attempting to construct a Job.
/// Returns Ok(()) if valid.
fn validate_cron_expr(expr: &str) -> Result<()> {
    // tokio-cron-scheduler will error on construction if expression is invalid.
    // We do a dry-run validation here by checking it parses via the cron crate.
    // Simple heuristic: must have 6 whitespace-separated fields.
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 6 {
        anyhow::bail!(
            "Cron expression must have 6 fields (sec min hour day month weekday), got {}: '{}'",
            fields.len(),
            expr
        );
    }
    Ok(())
}
```

**Step 2: Add unit tests for the helpers**

In `#[cfg(test)] mod tests` at the bottom of `agent.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_one_shot_delay_valid() {
        // A datetime far in the future should parse without error
        let result = parse_one_shot_delay("2099-12-31T23:59:59");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_one_shot_delay_past_returns_err() {
        let result = parse_one_shot_delay("2000-01-01T00:00:00");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already passed"));
    }

    #[test]
    fn test_parse_one_shot_delay_invalid_format() {
        let result = parse_one_shot_delay("next tuesday");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cron_expr_valid() {
        assert!(validate_cron_expr("0 0 9 * * MON").is_ok());
        assert!(validate_cron_expr("0 30 8 * * *").is_ok());
    }

    #[test]
    fn test_validate_cron_expr_wrong_field_count() {
        assert!(validate_cron_expr("0 9 * * *").is_err()); // 5 fields
        assert!(validate_cron_expr("0 0 9 1 * * MON").is_err()); // 7 fields
    }
}
```

**Step 3: Run tests**

```
cargo test agent::tests -- --nocapture
```
Expected: PASS (5 tests).

**Step 4: Implement `schedule_task` in `execute_tool`**

In the `match name` block in `execute_tool`, add before the final `_ =>` arm:

```rust
"schedule_task" => {
    let trigger_type = match arguments["trigger_type"].as_str() {
        Some(t) => t.to_string(),
        None => return "Missing trigger_type".to_string(),
    };
    let trigger_value = match arguments["trigger_value"].as_str() {
        Some(v) => v.to_string(),
        None => return "Missing trigger_value".to_string(),
    };
    let prompt = match arguments["prompt"].as_str() {
        Some(p) => p.to_string(),
        None => return "Missing prompt".to_string(),
    };
    let description = match arguments["description"].as_str() {
        Some(d) => d.to_string(),
        None => return "Missing description".to_string(),
    };

    // Validate trigger before touching the DB
    if trigger_type == "one_shot" {
        if let Err(e) = parse_one_shot_delay(&trigger_value) {
            return format!("Invalid trigger: {}", e);
        }
    } else if trigger_type == "recurring" {
        if let Err(e) = validate_cron_expr(&trigger_value) {
            return format!("Invalid cron expression: {}", e);
        }
    } else {
        return format!("Unknown trigger_type '{}'. Use 'one_shot' or 'recurring'.", trigger_type);
    }

    // We need the user_id and chat_id from the incoming message context.
    // These are not available here directly — see design note below.
    // For now, this tool requires the caller to pass context via IncomingMessage.
    // We access it through a thread-local set at the top of process_message.
    // IMPLEMENTATION NOTE: see Task 7b for the context-passing mechanism.

    format!("schedule_task handler: trigger_type={}, trigger_value={}", trigger_type, trigger_value)
}
```

**Design note — passing `user_id`/`chat_id` to `execute_tool`:**

`execute_tool` currently only receives `name` and `arguments`. The scheduling tools need `user_id` and `chat_id` to persist to DB and configure the job closure. The cleanest approach without touching the LLM loop signature: add `user_id: &str` and `chat_id: &str` parameters to `execute_tool`.

**Step 5: Update `execute_tool` signature**

Change:
```rust
async fn execute_tool(&self, name: &str, arguments: &serde_json::Value) -> String {
```
To:
```rust
async fn execute_tool(
    &self,
    name: &str,
    arguments: &serde_json::Value,
    user_id: &str,
    chat_id: &str,
) -> String {
```

Update all call sites in `process_message` (two lines in the tool call loop):

```rust
// existing:
let tool_result = self.execute_tool(&tool_call.function.name, &arguments).await;

// updated:
let tool_result = self
    .execute_tool(&tool_call.function.name, &arguments, user_id, chat_id)
    .await;
```

(`user_id` is already in scope in `process_message` — it's `&incoming.user_id`.)

For `chat_id`: add `let chat_id = &incoming.chat_id;` at the top of `process_message`. This requires `IncomingMessage` to have a `chat_id` field.

**Step 6: Check `IncomingMessage` for `chat_id`**

```
grep -n "chat_id\|IncomingMessage" src/platform/mod.rs src/platform/telegram.rs
```

If `chat_id` is missing from `IncomingMessage`, add it. In `src/platform/mod.rs`:

```rust
pub struct IncomingMessage {
    pub platform: String,
    pub user_id: String,
    pub chat_id: String,   // ADD THIS if missing
    pub text: String,
}
```

And populate it in `src/platform/telegram.rs` where `IncomingMessage` is constructed:
```rust
IncomingMessage {
    platform: "telegram".to_string(),
    user_id: msg.from.as_ref().map(|u| u.id.0.to_string()).unwrap_or_default(),
    chat_id: msg.chat.id.0.to_string(),   // ADD THIS
    text: text.to_string(),
}
```

**Step 7: Complete the `schedule_task` handler**

Replace the stub in `execute_tool` with the full implementation:

```rust
"schedule_task" => {
    let trigger_type = match arguments["trigger_type"].as_str() {
        Some(t) => t.to_string(),
        None => return "Missing trigger_type".to_string(),
    };
    let trigger_value = match arguments["trigger_value"].as_str() {
        Some(v) => v.to_string(),
        None => return "Missing trigger_value".to_string(),
    };
    let prompt_text = match arguments["prompt"].as_str() {
        Some(p) => p.to_string(),
        None => return "Missing prompt".to_string(),
    };
    let description = match arguments["description"].as_str() {
        Some(d) => d.to_string(),
        None => return "Missing description".to_string(),
    };

    // Validate + compute next_run_at
    let (delay_or_err, next_run_at) = if trigger_type == "one_shot" {
        match parse_one_shot_delay(&trigger_value) {
            Ok(d)  => (Ok(d), trigger_value.clone()),
            Err(e) => return format!("Invalid trigger: {}", e),
        }
    } else if trigger_type == "recurring" {
        if let Err(e) = validate_cron_expr(&trigger_value) {
            return format!("Invalid cron expression: {}", e);
        }
        // For recurring, delay_or_err is a dummy (cron path doesn't use Duration)
        (Ok(std::time::Duration::from_secs(0)), trigger_value.clone())
    } else {
        return format!("Unknown trigger_type '{}'", trigger_type);
    };

    // Persist task to DB
    let task_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let task = crate::scheduler::reminders::ScheduledTask {
        id: task_id.clone(),
        scheduler_job_id: None,
        user_id: user_id.to_string(),
        chat_id: chat_id.to_string(),
        platform: "telegram".to_string(),
        trigger_type: trigger_type.clone(),
        trigger_value: trigger_value.clone(),
        prompt: prompt_text.clone(),
        description: description.clone(),
        status: "active".to_string(),
        created_at: now,
        next_run_at: Some(next_run_at),
    };
    if let Err(e) = self.task_store.create(&task).await {
        return format!("Failed to save task: {}", e);
    }

    // Build closure captures (Weak<Agent> breaks Arc cycle)
    let weak_agent = self.self_weak.clone();
    let bot_clone = Arc::clone(&self.bot);
    let store_clone = self.task_store.clone();
    let tid = task_id.clone();
    let user_id_cap = user_id.to_string();
    let chat_id_cap = chat_id.to_string();
    let prompt_cap = prompt_text.clone();
    let desc_cap = description.clone();
    let is_recurring = trigger_type == "recurring";

    let fire = move || {
        let weak_agent = weak_agent.clone();
        let bot = bot_clone.clone();
        let store = store_clone.clone();
        let tid = tid.clone();
        let uid = user_id_cap.clone();
        let cid = chat_id_cap.clone();
        let prompt = prompt_cap.clone();
        let recurring = is_recurring;
        Box::pin(async move {
            // Mark completed before running (prevents double-fire on crash for one-shot)
            if !recurring {
                let _ = store.set_status(&tid, "completed").await;
            }

            // Upgrade weak ref to Arc<Agent>
            let agent = match weak_agent.upgrade() {
                Some(a) => a,
                None => {
                    tracing::error!("Agent dropped before scheduled task fired: {}", tid);
                    return;
                }
            };

            // Run full agentic loop
            let incoming = crate::platform::IncomingMessage {
                platform: "telegram".to_string(),
                user_id: uid,
                chat_id: cid.clone(),
                text: prompt,
            };
            let response = match agent.process_message(&incoming).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Scheduled task {} failed: {}", tid, e);
                    if !recurring {
                        let _ = store.set_status(&tid, "failed").await;
                    }
                    return;
                }
            };

            // Send response via Telegram
            use teloxide::prelude::*;
            let chat = teloxide::types::ChatId(cid.parse::<i64>().unwrap_or(0));
            for chunk in split_into_chunks(&response, 4000) {
                if let Err(e) = bot.send_message(chat, chunk).await {
                    tracing::error!("Failed to send scheduled message: {}", e);
                }
            }
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
    };

    // Register with scheduler
    let sched_id_result = if trigger_type == "one_shot" {
        self.scheduler
            .add_one_shot_job(delay_or_err.unwrap(), &description, fire)
            .await
    } else {
        self.scheduler
            .add_cron_job(&trigger_value, &description, fire)
            .await
    };

    match sched_id_result {
        Ok(sched_id) => {
            let _ = self.task_store.update_scheduler_job_id(&task_id, &sched_id.to_string()).await;
            format!(
                "Task scheduled! ID: {} — {} ({})",
                task_id, description, trigger_value
            )
        }
        Err(e) => {
            let _ = self.task_store.set_status(&task_id, "failed").await;
            format!("Failed to register task with scheduler: {}", e)
        }
    }
}
```

**Note on `split_into_chunks`:** This function likely already exists somewhere in the codebase (for splitting long Telegram messages). Find it with:
```
grep -rn "split_into_chunks\|split_message\|4000" src/
```
If it's a private function in `platform/telegram.rs`, make it `pub` and import it, or duplicate the logic inline as a small closure.

**Step 8: Verify compilation**

```
cargo check
```
Expected: compiles cleanly.

**Step 9: Commit**

```bash
git add src/agent.rs src/platform/mod.rs src/platform/telegram.rs
git commit -m "feat(agent): implement schedule_task tool with DB persistence and scheduler registration"
```

---

## Task 8: Implement `list_scheduled_tasks` and `cancel_scheduled_task`

**Files:**
- Modify: `src/agent.rs`

**Step 1: Add handlers in `execute_tool`**

```rust
"list_scheduled_tasks" => {
    match self.task_store.list_active_for_user(user_id).await {
        Ok(tasks) if tasks.is_empty() => {
            "You have no active scheduled tasks.".to_string()
        }
        Ok(tasks) => {
            let lines: Vec<String> = tasks
                .iter()
                .map(|t| {
                    format!(
                        "• ID: {}\n  Description: {}\n  Type: {} ({})\n  Next run: {}",
                        t.id,
                        t.description,
                        t.trigger_type,
                        t.trigger_value,
                        t.next_run_at.as_deref().unwrap_or("unknown"),
                    )
                })
                .collect();
            format!("Your active scheduled tasks:\n\n{}", lines.join("\n\n"))
        }
        Err(e) => format!("Failed to list tasks: {}", e),
    }
}

"cancel_scheduled_task" => {
    let task_id = match arguments["task_id"].as_str() {
        Some(id) => id.to_string(),
        None => return "Missing task_id".to_string(),
    };

    // Verify ownership before cancelling
    match self.task_store.list_active_for_user(user_id).await {
        Ok(tasks) => {
            let found = tasks.iter().find(|t| t.id == task_id);
            match found {
                None => return "Task not found or already completed/cancelled.".to_string(),
                Some(task) => {
                    // Remove from scheduler
                    if let Some(sched_id_str) = &task.scheduler_job_id {
                        if let Ok(sched_uuid) = sched_id_str.parse::<uuid::Uuid>() {
                            let _ = self.scheduler.remove_job(sched_uuid).await;
                        }
                    }
                    match self.task_store.set_status(&task_id, "cancelled").await {
                        Ok(()) => format!("Task '{}' cancelled.", task.description),
                        Err(e) => format!("Failed to cancel task: {}", e),
                    }
                }
            }
        }
        Err(e) => format!("Failed to look up tasks: {}", e),
    }
}
```

**Step 2: Verify compilation**

```
cargo check
```
Expected: compiles.

**Step 3: Commit**

```bash
git add src/agent.rs
git commit -m "feat(agent): implement list_scheduled_tasks and cancel_scheduled_task tools"
```

---

## Task 9: `restore_scheduled_tasks` on startup

**Files:**
- Modify: `src/scheduler/tasks.rs`
- Modify: `src/main.rs`

This rehydrates all `active` tasks from the DB into the scheduler after a bot restart.

**Step 1: Add `restore_scheduled_tasks` to `tasks.rs`**

```rust
use std::sync::{Arc, Weak};
use crate::agent::Agent;
use crate::scheduler::reminders::ScheduledTaskStore;
use teloxide::Bot;

pub async fn restore_scheduled_tasks(
    scheduler: &crate::scheduler::Scheduler,
    task_store: &ScheduledTaskStore,
    agent_weak: Weak<Agent>,
    bot: Arc<Bot>,
) -> anyhow::Result<()> {
    use std::time::Duration;

    let tasks = task_store.list_all_active().await?;
    let now = chrono::Utc::now();
    let missed_threshold = chrono::Duration::hours(1);

    info!("Restoring {} active scheduled task(s) from DB", tasks.len());

    for task in tasks {
        let store_clone = task_store.clone();
        let bot_clone = Arc::clone(&bot);
        let weak_clone = agent_weak.clone();
        let tid = task.id.clone();
        let uid = task.user_id.clone();
        let cid = task.chat_id.clone();
        let prompt_cap = task.prompt.clone();
        let is_recurring = task.trigger_type == "recurring";
        let desc = task.description.clone();

        let fire = move || {
            let store = store_clone.clone();
            let bot = bot_clone.clone();
            let weak_agent = weak_clone.clone();
            let tid = tid.clone();
            let uid = uid.clone();
            let cid = cid.clone();
            let prompt = prompt_cap.clone();
            let recurring = is_recurring;
            Box::pin(async move {
                if !recurring {
                    let _ = store.set_status(&tid, "completed").await;
                }
                let agent = match weak_agent.upgrade() {
                    Some(a) => a,
                    None => return,
                };
                let incoming = crate::platform::IncomingMessage {
                    platform: "telegram".to_string(),
                    user_id: uid,
                    chat_id: cid.clone(),
                    text: prompt,
                };
                let response = match agent.process_message(&incoming).await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("Restored task {} failed: {}", tid, e);
                        return;
                    }
                };
                use teloxide::prelude::*;
                let chat = teloxide::types::ChatId(cid.parse::<i64>().unwrap_or(0));
                for chunk in crate::platform::telegram::split_message(&response, 4000) {
                    let _ = bot.send_message(chat, chunk).await;
                }
            }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        };

        if task.trigger_type == "one_shot" {
            let next_run = task.next_run_at.as_deref().unwrap_or(&task.trigger_value);
            match chrono::NaiveDateTime::parse_from_str(next_run, "%Y-%m-%dT%H:%M:%S")
                .map(|n| chrono::Local.from_local_datetime(&n).single())
                .ok()
                .flatten()
                .map(|dt| dt.with_timezone(&chrono::Utc))
            {
                Some(fire_time) => {
                    if fire_time <= now {
                        if now - fire_time < missed_threshold {
                            // Missed by < 1h: fire immediately (1s delay)
                            info!("Task {} missed by <1h, firing immediately", task.id);
                            let id = scheduler
                                .add_one_shot_job(Duration::from_secs(1), &desc, fire)
                                .await?;
                            task_store.update_scheduler_job_id(&task.id, &id.to_string()).await?;
                        } else {
                            // Missed by > 1h: mark completed
                            info!("Task {} missed by >1h, marking completed", task.id);
                            task_store.set_status(&task.id, "completed").await?;
                        }
                    } else {
                        let delay = (fire_time - now).to_std().unwrap_or(Duration::from_secs(1));
                        let id = scheduler.add_one_shot_job(delay, &desc, fire).await?;
                        task_store.update_scheduler_job_id(&task.id, &id.to_string()).await?;
                    }
                }
                None => {
                    tracing::warn!("Could not parse next_run_at for task {}, skipping", task.id);
                }
            }
        } else {
            // Recurring: re-register cron
            let id = scheduler.add_cron_job(&task.trigger_value, &desc, fire).await?;
            task_store.update_scheduler_job_id(&task.id, &id.to_string()).await?;
        }
    }

    Ok(())
}
```

**Step 2: Wire it in `main.rs`**

After `register_builtin_tasks` and before `scheduler.start()`:

```rust
let agent_weak = Arc::downgrade(&agent);
crate::scheduler::tasks::restore_scheduled_tasks(
    &scheduler,
    &task_store,
    agent_weak,
    Arc::clone(&bot),
)
.await?;
```

**Step 3: Verify compilation**

```
cargo check
```
Expected: compiles.

**Step 4: Commit**

```bash
git add src/scheduler/tasks.rs src/main.rs
git commit -m "feat(scheduler): restore active scheduled tasks from DB on startup"
```

---

## Task 10: `cargo fmt`, `cargo clippy`, final checks

**Step 1: Format**

```
cargo fmt
```

**Step 2: Clippy**

```
cargo clippy -- -D warnings
```

Fix any warnings before proceeding. Common issues to expect:
- Unused `delay_or_err.unwrap()` — the `Ok(Duration::from_secs(0))` dummy for recurring. Refactor that branch to avoid the Option/Result carry-through.
- `Arc::clone` vs `.clone()` style — Clippy prefers `Arc::clone(&x)` for clarity.
- Large closures — Clippy may suggest extracting helpers.

**Step 3: Run all tests**

```
cargo test -- --nocapture
```
Expected: all tests PASS.

**Step 4: Final commit**

```bash
git add -u
git commit -m "chore: cargo fmt and clippy fixes"
```

---

## Task 11: Push branch

```bash
git push -u origin claude/rustbot-scheduling-tool-nLhrR
```

---

## Testing the feature manually

Once the bot is running with a valid `config.toml`:

1. **One-shot:** Send `"Remind me to review the PR at 5pm today"` — the LLM should call `schedule_task` with an ISO datetime and confirm. At 5pm the bot sends a message unprompted.

2. **Recurring:** Send `"Every weekday morning at 9am, give me a motivational quote"` — LLM calls `schedule_task` with cron `0 0 9 * * MON-FRI`.

3. **List:** Send `"What tasks do I have scheduled?"` — LLM calls `list_scheduled_tasks`.

4. **Cancel:** Send `"Cancel that reminder"` — LLM calls `list_scheduled_tasks` then `cancel_scheduled_task` with the ID.

5. **Restart test:** Stop and restart the bot. Confirm recurring tasks are still active; one-shot tasks with future times are re-registered.
