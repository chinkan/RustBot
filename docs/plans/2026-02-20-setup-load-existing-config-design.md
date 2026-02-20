# Design: Load Existing config.toml in Setup Wizard

**Date:** 2026-02-20
**Status:** Approved

## Problem

When `cargo run --bin setup` is re-run to update an existing configuration, the wizard starts blank — forcing users to re-enter every field including tokens and API keys. Custom MCP servers added manually to `config.toml` are also lost.

## Goals

1. Pre-populate all wizard fields from an existing `config.toml` on page load.
2. Recognise catalog MCP servers and pre-check them with their env vars filled.
3. Show non-catalog MCP servers as "Custom" cards, each editable and removable.
4. Provide a form to add new custom MCP servers during setup.

## Approach: Option A — Server-side TOML parsing → structured JSON

The setup binary adds a `GET /api/load-config` endpoint. It reads `config.toml`, parses it with Rust's `toml` crate into a purpose-built `ExistingConfig` struct (all fields `Option<_>`), and returns JSON. The browser receives clean typed data, no JS TOML parsing needed.

## Backend Design (`src/bin/setup.rs`)

### New struct `ExistingConfig`

All fields optional so partial / hand-edited configs still load without error.

```rust
#[derive(Deserialize, Serialize, Default)]
struct ExistingConfig {
    exists: bool,
    telegram_token: Option<String>,
    allowed_user_ids: Option<String>,   // serialised as "id1, id2"
    openrouter_key: Option<String>,
    model: Option<String>,
    max_tokens: Option<u32>,
    system_prompt: Option<String>,
    location: Option<String>,
    sandbox_dir: Option<String>,
    db_path: Option<String>,
    mcp_servers: Vec<ExistingMcpServer>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
struct ExistingMcpServer {
    name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}
```

### Intermediate TOML-parse structs (private, `setup.rs` only)

```rust
#[derive(Deserialize)]
struct RawConfig {
    telegram:    Option<RawTelegram>,
    openrouter:  Option<RawOpenRouter>,
    sandbox:     Option<RawSandbox>,
    memory:      Option<RawMemory>,
    mcp_servers: Option<Vec<RawMcpServer>>,
}
// ... (RawTelegram, RawOpenRouter, etc.)
```

### `GET /api/load-config` handler

- Config file absent → `{ "exists": false, ... defaults }`
- Parse error → log warning, return `{ "exists": false }` (do not crash setup)
- Success → populate `ExistingConfig`, serialize to JSON

### `AppState` addition

Store `config_path` (already present). The handler reads from the same path.

### `save_config` print fix

Update the success `println!` to say `cargo run --bin rustbot` (already done in prior commit but `setup.rs` still has the old string — fix here).

## Frontend Design (`setup/index.html`)

### On page load (before `showStep(1)`)

```js
async function loadExistingConfig() {
  const res = await fetch('/api/load-config');
  const cfg  = await res.json();
  if (!cfg.exists) return;
  // populate state.*
  // pre-check catalog MCPs
  // store custom MCPs in state.custom_mcp_servers
  showExistingBanner();
}
```

### Step 1 — existing config banner

If a config was loaded, show a tinted info banner below the subtitle:

> **Editing existing configuration** — all fields pre-loaded from `config.toml`.

### State additions

```js
state.custom_mcp_servers = [];
// Each entry: { name, command, args, env: {key:val} }
```

### Step 5 (MCP Tools) — Custom Servers section

Added **below** the catalog grid, always visible:

```
─── Custom MCP Servers ───────────────────────
  [card: name / command / args / env / 🗑]
  [card: ...]

  ┌─ Add Custom Server ──────────────────────┐
  │  Name       [____________]               │
  │  Command    [npx▼] [___________________] │
  │  Args       [_____________________________│
  │             (space-separated)            │
  │  Env Vars   KEY [_______] VALUE [_______]│
  │             [+ Add env var]              │
  │  [Add Server]                            │
  └──────────────────────────────────────────┘
```

- Pre-loaded custom servers appear as removable cards (trash icon)
- "Add Server" validates name non-empty, then pushes to `state.custom_mcp_servers` and renders a new card
- Cards show: name, `command args...`, env key list

### TOML generation

After catalog servers, iterate `state.custom_mcp_servers`:

```js
for (const s of state.custom_mcp_servers) {
  toml += '\n[[mcp_servers]]\n';
  toml += `name = "${esc(s.name)}"\n`;
  toml += `command = "${esc(s.command)}"\n`;
  toml += `args = [${s.args.map(a => '"' + esc(a) + '"').join(', ')}]\n`;
  if (Object.keys(s.env).length > 0) {
    toml += '[mcp_servers.env]\n';
    for (const [k,v] of Object.entries(s.env)) {
      toml += `${k} = "${esc(v)}"\n`;
    }
  }
}
```

### Catalog MCP matching logic

A catalog server is "matched" when the loaded server's `name` equals a catalog entry's `name`. When matched:
- Pre-check the checkbox
- Fill in env var inputs from the loaded server's `env` map

Non-matched servers go to `state.custom_mcp_servers`.

## Data Flow

```
cargo run --bin setup
  └─ Axum serves /
       └─ Browser loads page
            └─ fetch('/api/load-config')
                 ├─ config.toml absent → blank wizard
                 └─ config.toml present → pre-populate state
                      └─ showStep(1) with optional banner
                           └─ user edits fields
                                └─ POST /api/save-config → writes config.toml → step 7
```

## Error Handling

| Scenario | Behaviour |
|---|---|
| `config.toml` absent | `exists: false`; wizard starts blank |
| `config.toml` malformed | Log warning; wizard starts blank |
| Missing optional field | Field stays at wizard default |
| Custom MCP with no name | "Add Server" button disabled until name filled |
| Network error on load | Wizard starts blank, no crash |

## Testing

- Unit test: `load_existing_config()` with a full config → all fields returned correctly
- Unit test: missing file → `exists: false`
- Unit test: malformed TOML → `exists: false` (no panic)
- Unit test: custom MCP server round-trip (load → TOML generation includes it)
- Manual: run setup twice, verify second run pre-populates all fields

## Files Changed

| File | Change |
|---|---|
| `src/bin/setup.rs` | Add `ExistingConfig`, raw parse structs, `GET /api/load-config` handler, register route |
| `setup/index.html` | `loadExistingConfig()`, existing banner, `state.custom_mcp_servers`, custom server UI section, updated `generateToml()` |
