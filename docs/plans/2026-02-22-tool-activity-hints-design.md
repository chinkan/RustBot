# Tool Activity Hints — Design

**Date:** 2026-02-22
**Status:** Approved
**Branch:** `claude/debug-slow-agent-pKknh`

## Problem

Users see a typing indicator while the agent runs, but have no visibility into what the LLM is actually doing during multi-step tool calls. Long agentic loops feel opaque.

## Goal

Display a live "⚙️ Calling: {tool_name}" status message in Telegram that updates in-place for each tool the LLM executes, then disappears when the final answer arrives.

## UX Behaviour

1. LLM requests tool calls → status message sent: `⚙️ Calling: search_memory`
2. LLM requests another tool → same message edited: `⚙️ Calling: run_command`
3. Final answer ready → status message deleted, answer sent in chunks
4. If no tools were called → no status message is ever sent

## Chosen Approach: mpsc Channel (Approach B)

Agent emits `AgentEvent` values into an optional channel. The Telegram handler owns the receiver and manages all Telegram API calls. The agent stays platform-agnostic.

## Architecture

```
Telegram handler                    Agent (process_message)
────────────────                    ───────────────────────
mpsc::unbounded_channel()
  tx ──────────────────────────────► Option<&EventSender>
  rx → status task (spawned)
                                    before join_all tool batch:
                                      tx.send(ToolStarted { name })
                                      (one send per tool in batch)

status task loop:
  first event  → send_message()      stores MessageId
  later events → edit_message_text() same MessageId

await process_message()
drop(tx)                            channel closes → task exits

status_handle.await → Option<MessageId>
delete_message(msg_id)
send final answer chunks
```

## New Types (agent.rs)

```rust
pub enum AgentEvent {
    ToolStarted { name: String },
}

pub type EventSender = tokio::sync::mpsc::UnboundedSender<AgentEvent>;
```

## API Change (agent.rs)

```rust
pub async fn process_message(
    &self,
    incoming: &IncomingMessage,
    events: Option<&EventSender>,   // ← new optional parameter
) -> Result<String>
```

Callers that pass `None` (scheduled jobs, tests) are unaffected.

## Telegram Handler Change (platform/telegram.rs)

- Create `(event_tx, event_rx)` channel before calling `process_message`
- Spawn `status_handle` task that reads `event_rx`, manages a single status `MessageId`
- Pass `Some(&event_tx)` to `process_message`
- After `process_message` returns: `drop(event_tx)` → task exits → `await status_handle` → delete message if `Some(id)`
- Existing `typing_handle` is unchanged

## Error Handling

| Failure | Behaviour |
|---------|-----------|
| `tx.send()` fails (receiver dropped) | `let _ =` — agent loop continues |
| `send_message` / `edit_message_text` fails | `let _ =` — no crash |
| `delete_message` fails | `let _ =` — stale message may linger; acceptable |
| `status_handle` panics | `status_handle.await` returns `Err`; ignored with `ok()` |

## Files Changed

| File | Change |
|------|--------|
| `src/agent.rs` | Add `AgentEvent` enum, `EventSender` alias; add `events` param to `process_message`; send events before tool `join_all` |
| `src/platform/telegram.rs` | Create channel, spawn status task, pass `events` to agent, delete status message on completion |
| Scheduled job callsites (`scheduler/reminders.rs` or similar) | Pass `None` for `events` |

## Out of Scope

- Arguments in the status message (tool name only)
- Persistent status log after answer
- Other platforms (the `events` param makes future additions trivial)
