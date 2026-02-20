# Design: README Update — Models, Schedule Tools, Roadmap, License, Sponsorship

**Date:** 2026-02-19
**Branch:** `claude/update-readme-models-JMkVu`

---

## Overview

Update the README and related config defaults to reflect:

1. New default inference model: `moonshotai/kimi-k2.5`
2. New default embedding model: `qwen/qwen3-embedding-8b`
3. Documentation of scheduling tools added in PR #3
4. A Roadmap (TODO) section with done/planned items
5. MIT license + contributing invitation
6. Sponsorship links (Buy Me a Coffee + GitHub Sponsors)

---

## Changes

### 1. `README.md`

- **Header description**: Update to reference `moonshotai/kimi-k2.5` as default model
- **Features section**: Add scheduling tools and memory/embedding feature line
- **Configuration table**: Update model default values shown
- **Built-in Tools**: Add scheduling subsection (`schedule_task`, `list_scheduled_tasks`, `cancel_scheduled_task`)
- **Roadmap section (new)**: Checkboxes for done ✅ and planned 🔲 items
- **Contributing section (new)**: MIT license badge, "feel free to open a PR" invitation
- **Sponsorship section (new)**: Buy Me a Coffee (buymeacoffee.com/chinkan.ai) + GitHub Sponsors

#### Roadmap — Done ✅
- Telegram bot with user allowlist
- OpenRouter LLM integration with tool calling
- Built-in sandboxed tools (file read/write, directory listing, command execution)
- MCP server integration for extensible tooling
- Agentic loop (up to 10 iterations)
- Per-user conversation history
- Scheduling tools: `schedule_task`, `list_scheduled_tasks`, `cancel_scheduled_task`
- Persistent memory with SQLite
- Bot skills (folder-based, auto-loaded)
- Vector embedding search for memory

#### Roadmap — Planned 🔲
- Image upload support
- Google integration tools (Calendar, Email, Drive)
- Event trigger framework (e.g., on email receive)
- Web portal for setup and configuration
- WhatsApp support
- Webhook mode (in addition to polling)
- And more…

---

### 2. `src/config.rs`

| Function | Old value | New value |
|----------|-----------|-----------|
| `default_model()` | `qwen/qwen3-235b-a22b` | `moonshotai/kimi-k2.5` |
| `default_embedding_model()` | `openai/text-embedding-3-small` | `qwen/qwen3-embedding-8b` |

---

### 3. `config.example.toml`

Update any inline comments or example values that reference the old model IDs to the new defaults.

---

### 4. `LICENSE`

Create an MIT `LICENSE` file with:
- Copyright holder: chinkan.ai
- Year: 2026

---

## Out of Scope

- No feature implementation — roadmap items remain aspirational
- No changes to business logic, only docs + config defaults
