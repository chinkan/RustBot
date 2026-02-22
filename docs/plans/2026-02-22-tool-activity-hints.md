# Tool Activity Hints Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Emit `AgentEvent::ToolStarted` events from the agent loop and display them in Telegram as a single in-place-edited status message that is deleted when the final answer arrives.

**Architecture:** A `tokio::sync::mpsc` channel is created per interactive Telegram message. The agent's `process_message` accepts an optional sender and fires one event per tool call batch. The Telegram handler owns the receiver, managing a single ephemeral status message via send/edit/delete. Scheduled jobs pass `None` and are unaffected.

**Tech Stack:** Rust 2021, Tokio async, `teloxide` Telegram bot framework, `futures::future::join_all` (already in use).

---

### Task 1: Add `AgentEvent` enum and `EventSender` alias to `agent.rs`

**Files:**
- Modify: `src/agent.rs` (top of file, after existing `use` imports)

**Step 1: Add the new types**

In `src/agent.rs`, after the last `use` statement (line ~16, before `pub struct ScheduledJobRequest`), insert:

```rust
/// Events emitted by the agent loop so callers can show live progress.
pub enum AgentEvent {
    ToolStarted { name: String },
}

/// Convenience alias — the sending half of an agent event channel.
pub type EventSender = tokio::sync::mpsc::UnboundedSender<AgentEvent>;
```

**Step 2: Verify it compiles**

```bash
cargo check 2>&1
```
Expected: `Finished` with no errors.

**Step 3: Commit**

```bash
git add src/agent.rs
git commit -m "feat: add AgentEvent enum and EventSender type alias"
```

---

### Task 2: Thread `events` parameter through `process_message`

**Files:**
- Modify: `src/agent.rs:97` — `process_message` signature

**Step 1: Write the failing check**

Before changing the signature, verify both existing callsites compile now:
```bash
cargo check 2>&1 | grep "process_message"
```
Expected: no errors (baseline).

**Step 2: Update the signature**

Change `src/agent.rs:97`:

```rust
// Before
pub async fn process_message(&self, incoming: &IncomingMessage) -> Result<String> {

// After
pub async fn process_message(
    &self,
    incoming: &IncomingMessage,
    events: Option<&EventSender>,
) -> Result<String> {
```

**Step 3: Fix the two broken callsites**

`src/main.rs:116` — scheduled job runner (no UI, always `None`):
```rust
// Before
let response = match agent.process_message(&req.incoming).await {

// After
let response = match agent.process_message(&req.incoming, None).await {
```

`src/platform/telegram.rs:178` — interactive message (will wire up properly in Task 4; use `None` for now):
```rust
// Before
let result = agent.process_message(&incoming).await;

// After
let result = agent.process_message(&incoming, None).await;
```

**Step 4: Verify**

```bash
cargo check 2>&1
```
Expected: `Finished` with no errors.

**Step 5: Commit**

```bash
git add src/agent.rs src/main.rs src/platform/telegram.rs
git commit -m "feat: thread events param through process_message (None everywhere for now)"
```

---

### Task 3: Send `ToolStarted` events in the agentic loop

**Files:**
- Modify: `src/agent.rs` — inside the `join_all` tool execution block (~line 176)

**Step 1: Locate the insertion point**

The block to modify is in `process_message`. It starts at:
```rust
// Execute tool calls in parallel — each tool is independent
let tool_futures = tool_calls.iter().map(|tc| {
```

**Step 2: Insert event sends before `join_all`**

Add the following immediately after saving the assistant message with tool calls (`messages.push(response.clone());`) and before the `tool_futures` map:

```rust
// Notify listener about all tools about to run in this batch
if let Some(tx) = events {
    for tc in tool_calls.iter() {
        let _ = tx.send(AgentEvent::ToolStarted {
            name: tc.function.name.clone(),
        });
    }
}
```

The full block should now look like:

```rust
// Save assistant message with tool calls
self.memory
    .save_message(&conversation_id, &response)
    .await?;
messages.push(response.clone());

// Notify listener about all tools about to run in this batch
if let Some(tx) = events {
    for tc in tool_calls.iter() {
        let _ = tx.send(AgentEvent::ToolStarted {
            name: tc.function.name.clone(),
        });
    }
}

// Execute tool calls in parallel — each tool is independent
let tool_futures = tool_calls.iter().map(|tc| {
    ...
```

**Step 3: Write a unit test**

In `src/agent.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[tokio::test]
async fn test_agent_event_tool_started_is_sent() {
    // Verify the channel receives one event per tool name sent
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

    // Simulate what the agentic loop does
    let names = vec!["search_memory", "run_command"];
    for name in &names {
        let _ = tx.send(AgentEvent::ToolStarted {
            name: name.to_string(),
        });
    }
    drop(tx);

    let mut received = Vec::new();
    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::ToolStarted { name } => received.push(name),
        }
    }
    assert_eq!(received, vec!["search_memory", "run_command"]);
}
```

**Step 4: Run the test**

```bash
cargo test test_agent_event_tool_started_is_sent 2>&1
```
Expected: `test agent::tests::test_agent_event_tool_started_is_sent ... ok`

**Step 5: Full check**

```bash
cargo check 2>&1
```
Expected: `Finished` with no errors.

**Step 6: Commit**

```bash
git add src/agent.rs
git commit -m "feat: send ToolStarted events before parallel tool execution"
```

---

### Task 4: Wire the Telegram status task and channel

**Files:**
- Modify: `src/platform/telegram.rs` — inside `handle_message`, replacing the placeholder `None` with a live channel

**Step 1: Read the current handler**

The section to modify is in `handle_message` (around line 145–180):

```rust
// Send "typing" indicator and keep refreshing it...
bot.send_chat_action(...).await.ok();
let typing_bot = bot.clone();
...
let typing_handle = tokio::spawn(async move { ... });

// Build platform-agnostic message
let incoming = IncomingMessage { ... };

// Process through agent
let result = agent.process_message(&incoming, None).await;
typing_handle.abort();
match result { ... }
```

**Step 2: Replace that section with the channel-based version**

Replace from `// Build platform-agnostic message` to the end of `match result { ... }` with:

```rust
// Channel for real-time tool activity hints
let (event_tx, mut event_rx) =
    tokio::sync::mpsc::unbounded_channel::<crate::agent::AgentEvent>();

// Spawn status task: receives ToolStarted events, manages one Telegram message
let status_bot = bot.clone();
let status_chat_id = msg.chat.id;
let status_handle: tokio::task::JoinHandle<Option<teloxide::types::MessageId>> =
    tokio::spawn(async move {
        let mut status_msg_id: Option<teloxide::types::MessageId> = None;
        while let Some(event) = event_rx.recv().await {
            match event {
                crate::agent::AgentEvent::ToolStarted { name } => {
                    let text = format!("⚙️ Calling: {}", name);
                    match status_msg_id {
                        None => {
                            // First tool — send new status message
                            if let Ok(m) =
                                status_bot.send_message(status_chat_id, &text).await
                            {
                                status_msg_id = Some(m.id);
                            }
                        }
                        Some(id) => {
                            // Subsequent tools — edit in place
                            let _ = status_bot
                                .edit_message_text(status_chat_id, id, &text)
                                .await;
                        }
                    }
                }
            }
        }
        status_msg_id
    });

// Build platform-agnostic message
let incoming = IncomingMessage {
    platform: "telegram".to_string(),
    user_id: user_id.to_string(),
    chat_id: msg.chat.id.0.to_string(),
    user_name,
    text,
};

// Process through agent — passes the event sender for live tool hints
let result = agent.process_message(&incoming, Some(&event_tx)).await;

// Close the event channel so the status task exits its recv loop
drop(event_tx);
typing_handle.abort();

// Wait for the status task to finish, then delete its message if one was sent
if let Ok(Some(status_msg_id)) = status_handle.await {
    let _ = bot.delete_message(msg.chat.id, status_msg_id).await;
}

match result {
    Ok(response) => {
        for chunk in split_message(&response, 4000) {
            bot.send_message(msg.chat.id, chunk).await.ok();
        }
    }
    Err(e) => {
        error!("Error processing message: {:#}", e);
        bot.send_message(msg.chat.id, format!("Error: {}", e))
            .await?;
    }
}
```

**Step 3: Verify**

```bash
cargo check 2>&1
```
Expected: `Finished` with no errors.

**Step 4: Full lint**

```bash
cargo clippy -- -D warnings 2>&1
```
Expected: `Finished` with no warnings.

**Step 5: Run all tests**

```bash
cargo test 2>&1
```
Expected: all existing tests + new event test pass.

**Step 6: Commit**

```bash
git add src/platform/telegram.rs
git commit -m "feat: wire Telegram status task for live tool activity hints"
```

---

### Task 5: Final polish and push

**Step 1: Format**

```bash
cargo fmt --all 2>&1
```

**Step 2: Full CI check**

```bash
cargo check && cargo clippy -- -D warnings && cargo test 2>&1
```
Expected: all pass, zero warnings.

**Step 3: Push**

```bash
git push -u origin claude/debug-slow-agent-pKknh 2>&1
```

---

## Acceptance Criteria

- `cargo check`, `cargo clippy -- -D warnings`, `cargo test` all pass
- Sending a message that triggers tool use shows `⚙️ Calling: <name>` in Telegram
- The status message updates in-place for each subsequent tool in the same batch
- The status message is deleted when the final answer is sent
- Messages that need no tools show no status message at all
- Scheduled jobs (fired from `main.rs`) are unaffected — they pass `None`
