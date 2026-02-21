# Multi-Provider LLM Support Design

**Date:** 2026-02-21
**Status:** Approved

## Overview

Add Ollama (local) and native OpenAI API as first-class LLM providers alongside
OpenRouter. The user picks exactly one provider. All three share the same unified
`[llm]` config section, replacing the existing `[openrouter]` section.

## Goals

- Keep the codebase small and fast (no new heavy dependencies)
- Easy to switch provider by editing one field in `config.toml`
- Setup wizard guides the user through provider selection with live Ollama model
  discovery and a curated OpenAI model dropdown
- Existing OpenRouter users can migrate by rerunning the wizard (legacy `[openrouter]`
  section is read and pre-filled automatically)

## Provider Comparison

| Provider    | `base_url` default                   | `api_key`          | Model selection          |
|-------------|--------------------------------------|--------------------|--------------------------|
| `openrouter`| `https://openrouter.ai/api/v1`       | required           | free-text input          |
| `ollama`    | `http://localhost:11434/v1`          | empty string       | live dropdown from API   |
| `openai`    | `https://api.openai.com/v1`          | required           | curated dropdown + custom|

## Config Schema (`[llm]`)

Replaces `[openrouter]`. All fields except `provider` and `model` have
provider-aware defaults.

```toml
[llm]
provider   = "ollama"                    # "openrouter" | "ollama" | "openai"
model      = "qwen2.5:14b"
base_url   = "http://localhost:11434/v1" # auto-filled per provider in wizard
api_key    = ""                          # required for openrouter/openai, empty for ollama
max_tokens = 4096
system_prompt = """You are a helpful AI assistant..."""
```

`config.example.toml` shows all three variants (two commented out).

## Architecture

### `src/config.rs`

- Add `LlmProvider` enum deriving `Deserialize`, `Serialize`, `Clone`, `Debug`:
  ```rust
  pub enum LlmProvider { Openrouter, Ollama, Openai }
  ```
- Add `LlmConfig` struct with fields: `provider`, `model`, `base_url`, `api_key`
  (optional, `#[serde(default)]`), `max_tokens`, `system_prompt`
- Replace `Config.openrouter: OpenRouterConfig` with `Config.llm: LlmConfig`
- Keep `OpenRouterConfig` only as an internal parse helper for migration
- `Config::load()` reads `[openrouter]` as legacy fallback if `[llm]` is absent

### `src/llm.rs`

- `LlmClient::new(config: LlmConfig)` — rename parameter type only
- Auth header: sent only when `config.api_key` is non-empty:
  ```rust
  if !self.config.api_key.is_empty() {
      req = req.header("Authorization", format!("Bearer {}", self.config.api_key));
  }
  ```
- All other logic unchanged — Ollama and OpenAI both use the same
  `/chat/completions` format

### `src/bin/setup.rs`

**New endpoint:** `GET /api/ollama-models`
- Makes a `GET http://localhost:11434/api/tags` request with 2s timeout
- Returns `{ ok: true, models: ["qwen2.5:14b", ...] }` on success
- Returns `{ ok: false, models: [] }` if Ollama unreachable (no error crash)

**Updated structs:**
- `ExistingConfig`: add `provider: String`, rename `openrouter_key` → `llm_key`
- `RawConfig`: add `llm: Option<RawLlm>`, keep `openrouter: Option<RawOpenRouter>`
  for legacy read

**Updated `parse_existing_config()`:**
- Prefers `[llm]` section; falls back to `[openrouter]` for legacy configs
- Maps legacy `openrouter` → `provider = "openrouter"`

**Updated `format_config()`:**
- Writes `[llm]` section with all five fields
- `api_key` line omitted (written as empty string) when provider is ollama

**Updated CLI wizard (`run_cli`):**
- Prompts: `Provider [openrouter/ollama/openai]:`, then provider-specific prompts

### `setup/index.html`

Step 3 is renamed "AI Provider" and restructured:

**Provider selector** — 3 styled radio buttons (tab-like):
```
[○ OpenRouter]  [○ Ollama]  [○ OpenAI]
```

**OpenRouter panel:**
- API Key (password input)
- Model (free-text input, hint: openrouter.ai/models)
- Max Tokens, System Prompt, Location (unchanged)

**Ollama panel:**
- Base URL (text input, default `http://localhost:11434`)
- "Fetch models" button → calls `/api/ollama-models`
  - Success: populates `<select>` dropdown
  - Failure: shows inline warning, falls back to text input
- Max Tokens, System Prompt, Location

**OpenAI panel:**
- API Key (password input)
- Model: curated `<select>` with options:
  `gpt-4o`, `gpt-4o-mini`, `o3-mini`, `o4-mini`, `gpt-4-turbo` + "Custom…" option
  that reveals a text input
- Max Tokens, System Prompt, Location

**Validation:**
- OpenRouter: api_key required, model required
- Ollama: model required (base_url has a valid default)
- OpenAI: api_key required, model required

**TOML generation (`generateToml`):**
- Writes `[llm]` section
- `base_url` always written (provider-specific default if user left blank)
- `api_key` always written (empty string for Ollama)

**Load existing config:**
- Reads `provider` field from server response
- Sets active radio tab accordingly
- Pre-fills correct panel inputs

## Migration Story

Users with existing `[openrouter]` configs:
1. Rerun `./setup.sh` — wizard detects legacy `[openrouter]` section via
   `parse_existing_config`, maps it to `provider = "openrouter"`, pre-fills all
   fields. User clicks through and saves → new `[llm]` format written.
2. Alternatively, manually rename `[openrouter]` → `[llm]` and add
   `provider = "openrouter"` line.

## Files Changed

| File | Change |
|------|--------|
| `src/config.rs` | Add `LlmProvider`, `LlmConfig`; replace `openrouter` field; legacy fallback in `load()` |
| `src/llm.rs` | Rename config type; conditional auth header |
| `src/bot.rs` | Update `AppState` and `process_with_llm` to use `LlmConfig` |
| `src/main.rs` | Pass `config.llm` instead of `config.openrouter` to `LlmClient` |
| `src/bin/setup.rs` | New Ollama endpoint; updated structs, parser, formatter, CLI |
| `setup/index.html` | Step 3 rework: provider tabs, Ollama dropdown, OpenAI dropdown |
| `config.example.toml` | Replace `[openrouter]` with `[llm]` section + all three variants |

## What Does Not Change

- Telegram integration
- MCP server management
- Memory / SQLite layer
- Sandbox tools
- Skills system
- `setup.sh` script
- CI pipeline

## Non-Goals

- Running multiple providers simultaneously
- Provider fallback/failover at runtime
- Streaming responses
- Fetching OpenAI model list via API (hardcoded curated list is sufficient)
