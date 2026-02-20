# Setup Wizard: Load Existing Config Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** When `cargo run --bin setup` is re-run, automatically load any existing `config.toml` and pre-populate all wizard fields, so users never have to re-enter credentials.

**Architecture:** A new `GET /api/load-config` endpoint in `setup.rs` reads `config.toml`, parses it with Rust's `toml` crate into a purpose-built `ExistingConfig` struct, and returns JSON. The browser calls this on page load, populates the JS `state` object, then `buildSteps()` renders form inputs with the loaded values. Non-catalog MCP servers are stored in `state.custom_mcp_servers` and displayed in a new "Custom Servers" section in step 5 with add/remove UI.

**Tech Stack:** Rust (`toml`, `serde`, `axum`, `tokio`), vanilla JS (no new dependencies), existing `setup/index.html` SPA.

---

## Key Files

- Modify: `src/bin/setup.rs` — backend endpoint + types
- Modify: `setup/index.html` — frontend load logic, form pre-fill, custom MCP UI
- Modify: `config.example.toml` — fix `location` placement (bug: it's inside `[openrouter]` block but the Rust struct has it top-level)

---

## Task 1: Backend — Add types to setup.rs

**Files:**
- Modify: `src/bin/setup.rs` (add after the existing `SaveResponse` struct, around line 48)

### Step 1: Add `use std::collections::HashMap;` import

In `src/bin/setup.rs`, at the top `use` block, add:
```rust
use std::collections::HashMap;
```

### Step 2: Add response and raw-parse structs

Add these structs after the existing `SaveResponse` struct:

```rust
// ── Load-config response ────────────────────────────────────────────────────

#[derive(Serialize, Default)]
struct ExistingConfig {
    exists: bool,
    telegram_token: String,
    allowed_user_ids: String,   // "123, 456" — ready for the text input
    openrouter_key: String,
    model: String,
    max_tokens: u32,
    system_prompt: String,
    location: String,
    sandbox_dir: String,
    db_path: String,
    mcp_servers: Vec<ExistingMcpServer>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct ExistingMcpServer {
    name: String,
    command: String,
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

// ── Raw TOML parse structs (loose — all fields optional so partial configs load) ──

#[derive(Deserialize, Default)]
struct RawConfig {
    telegram: Option<RawTelegram>,
    openrouter: Option<RawOpenRouter>,
    sandbox: Option<RawSandbox>,
    memory: Option<RawMemory>,
    location: Option<String>,      // top-level field in Config struct
    #[serde(default)]
    mcp_servers: Vec<RawMcpServer>,
}

#[derive(Deserialize, Default)]
struct RawTelegram {
    bot_token: Option<String>,
    allowed_user_ids: Option<Vec<toml::Value>>,
}

#[derive(Deserialize, Default)]
struct RawOpenRouter {
    api_key: Option<String>,
    model: Option<String>,
    max_tokens: Option<u32>,
    system_prompt: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawSandbox {
    allowed_directory: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawMemory {
    database_path: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawMcpServer {
    name: Option<String>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}
```

### Step 3: Verify it compiles

```bash
cargo check 2>&1 | grep -E "^error"
```
Expected: no output (no errors).

---

## Task 2: Backend — `parse_existing_config()` + unit tests

**Files:**
- Modify: `src/bin/setup.rs`

### Step 1: Write the failing tests first

In the `#[cfg(test)] mod tests` block at the bottom of `setup.rs`, add:

```rust
#[test]
fn test_parse_missing_file_returns_not_exists() {
    let cfg = parse_existing_config("this is not valid toml !!!");
    assert!(!cfg.exists);
}

#[test]
fn test_parse_full_config() {
    let toml = r#"
location = "Tokyo, Japan"

[telegram]
bot_token = "mytoken123"
allowed_user_ids = [111, 222]

[openrouter]
api_key = "sk-or-test"
model = "gpt-4o"
max_tokens = 2048
system_prompt = "Be helpful."

[sandbox]
allowed_directory = "/tmp/test"

[memory]
database_path = "test.db"
"#;
    let cfg = parse_existing_config(toml);
    assert!(cfg.exists);
    assert_eq!(cfg.telegram_token, "mytoken123");
    assert_eq!(cfg.allowed_user_ids, "111, 222");
    assert_eq!(cfg.openrouter_key, "sk-or-test");
    assert_eq!(cfg.model, "gpt-4o");
    assert_eq!(cfg.max_tokens, 2048);
    assert_eq!(cfg.system_prompt, "Be helpful.");
    assert_eq!(cfg.location, "Tokyo, Japan");
    assert_eq!(cfg.sandbox_dir, "/tmp/test");
    assert_eq!(cfg.db_path, "test.db");
    assert!(cfg.mcp_servers.is_empty());
}

#[test]
fn test_parse_config_with_mcp_servers() {
    let toml = r#"
[telegram]
bot_token = "t"
allowed_user_ids = [1]

[openrouter]
api_key = "k"

[sandbox]
allowed_directory = "/tmp"

[[mcp_servers]]
name = "git"
command = "uvx"
args = ["mcp-server-git"]

[[mcp_servers]]
name = "brave-search"
command = "npx"
args = ["-y", "@anthropic/mcp-brave-search"]
[mcp_servers.env]
BRAVE_API_KEY = "brave123"
"#;
    let cfg = parse_existing_config(toml);
    assert!(cfg.exists);
    assert_eq!(cfg.mcp_servers.len(), 2);
    assert_eq!(cfg.mcp_servers[0].name, "git");
    assert_eq!(cfg.mcp_servers[0].command, "uvx");
    assert_eq!(cfg.mcp_servers[0].args, vec!["mcp-server-git"]);
    assert_eq!(cfg.mcp_servers[1].name, "brave-search");
    assert_eq!(cfg.mcp_servers[1].env.get("BRAVE_API_KEY").unwrap(), "brave123");
}

#[test]
fn test_parse_partial_config_no_panic() {
    // Only telegram section — all other fields should be defaults
    let toml = r#"
[telegram]
bot_token = "partial"
allowed_user_ids = [42]
"#;
    let cfg = parse_existing_config(toml);
    assert!(cfg.exists);
    assert_eq!(cfg.telegram_token, "partial");
    assert_eq!(cfg.model, "");        // no default injected — that's the wizard's job
    assert_eq!(cfg.sandbox_dir, "");
}
```

### Step 2: Run tests — expect compile error (function doesn't exist yet)

```bash
cargo test 2>&1 | head -20
```
Expected: `error[E0425]: cannot find function 'parse_existing_config'`

### Step 3: Implement `parse_existing_config()`

Add this function above the `#[cfg(test)]` block:

```rust
fn parse_existing_config(content: &str) -> ExistingConfig {
    let raw: RawConfig = match toml::from_str(content) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Could not parse existing config.toml: {e}");
            return ExistingConfig::default();
        }
    };

    let tg = raw.telegram.unwrap_or_default();
    let or = raw.openrouter.unwrap_or_default();
    let sb = raw.sandbox.unwrap_or_default();
    let mem = raw.memory.unwrap_or_default();

    let allowed_user_ids = tg
        .allowed_user_ids
        .unwrap_or_default()
        .iter()
        .map(|v| match v {
            toml::Value::Integer(i) => i.to_string(),
            toml::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");

    let mcp_servers = raw
        .mcp_servers
        .into_iter()
        .filter_map(|s| {
            Some(ExistingMcpServer {
                name: s.name?,
                command: s.command.unwrap_or_default(),
                args: s.args,
                env: s.env,
            })
        })
        .collect();

    ExistingConfig {
        exists: true,
        telegram_token: tg.bot_token.unwrap_or_default(),
        allowed_user_ids,
        openrouter_key: or.api_key.unwrap_or_default(),
        model: or.model.unwrap_or_default(),
        max_tokens: or.max_tokens.unwrap_or(0), // 0 means "use wizard default"
        system_prompt: or.system_prompt.unwrap_or_default(),
        location: raw.location.unwrap_or_default(),
        sandbox_dir: sb.allowed_directory.unwrap_or_default(),
        db_path: mem.database_path.unwrap_or_default(),
        mcp_servers,
    }
}
```

### Step 4: Run tests — expect all pass

```bash
cargo test 2>&1 | tail -15
```
Expected: all new tests pass alongside existing ones.

### Step 5: Commit

```bash
git add src/bin/setup.rs
git commit -m "feat(setup): add parse_existing_config with tests"
```

---

## Task 3: Backend — `load_config` handler + route registration

**Files:**
- Modify: `src/bin/setup.rs`

### Step 1: Add the async handler

Add below the `save_config` handler:

```rust
async fn load_config(State(state): State<AppState>) -> Json<ExistingConfig> {
    match tokio::fs::read_to_string(&state.config_path).await {
        Ok(content) => Json(parse_existing_config(&content)),
        Err(_) => Json(ExistingConfig::default()), // file absent or unreadable
    }
}
```

### Step 2: Register the route

In `main()`, update the `Router::new()` call from:
```rust
let app = Router::new()
    .route("/", get(serve_index))
    .route("/api/save-config", post(save_config))
    .with_state(state);
```
To:
```rust
let app = Router::new()
    .route("/", get(serve_index))
    .route("/api/load-config", get(load_config))
    .route("/api/save-config", post(save_config))
    .with_state(state);
```

### Step 3: Fix the success println in save_config

Around line 66, update:
```rust
println!("   Run the bot with:  cargo run\n");
```
to:
```rust
println!("   Run the bot with:  cargo run --bin rustbot\n");
```

### Step 4: Verify

```bash
cargo check 2>&1 | grep -E "^error"
cargo test 2>&1 | tail -10
```
Expected: no errors, all tests pass.

### Step 5: Commit

```bash
git add src/bin/setup.rs
git commit -m "feat(setup): add GET /api/load-config endpoint"
```

---

## Task 4: Fix `location` in config.example.toml

**Background:** `location` is a top-level field in the `Config` struct, but `config.example.toml` places the commented `location` line _inside_ the `[openrouter]` section. When a user uncomments it there, it's silently ignored. Fix the placement now so the round-trip works correctly.

**Files:**
- Modify: `config.example.toml`

### Step 1: Move the location comment to after `[skills]`

Change `config.example.toml` — move the location lines from inside the `[openrouter]` block to after the `[skills]` block:

**Remove** from the openrouter section:
```toml
# Your location, injected into the system prompt so the AI knows your timezone/region
# location = "Tokyo, Japan"
```

**Add** at the end of the file (after `[skills]` block):
```toml
# Your location, injected into the system prompt so the AI knows your timezone/region
# Uncomment and set to your city/region (e.g. "Tokyo, Japan")
# location = "Tokyo, Japan"
```

### Step 2: Commit

```bash
git add config.example.toml
git commit -m "fix(config): move location field comment to top level (was inside [openrouter])"
```

---

## Task 5: Frontend — Async init + `loadExistingConfig()` + existing-config banner

**Files:**
- Modify: `setup/index.html`

### Step 1: Add `custom_mcp_servers` to state and existing-config flag

In the `const state = { ... }` block, add two fields:
```js
const state = {
  telegram_token: '',
  allowed_user_ids: '',
  openrouter_key: '',
  model: 'qwen/qwen3-235b-a22b',
  max_tokens: '4096',
  system_prompt: 'You are a helpful AI assistant ...',
  location: '',
  sandbox_dir: '/tmp/rustbot-sandbox',
  db_path: 'rustbot.db',
  mcp_selections: {},
  custom_mcp_servers: [],  // NEW
  _loaded: false,          // NEW — true when existing config was found
};
```

### Step 2: Add `loadExistingConfig()` function

Add this function before `buildSteps()`:

```js
async function loadExistingConfig() {
  try {
    const res = await fetch('/api/load-config');
    if (!res.ok) return;
    const cfg = await res.json();
    if (!cfg.exists) return;

    // Populate scalar state fields (non-empty values win over defaults)
    if (cfg.telegram_token)  state.telegram_token  = cfg.telegram_token;
    if (cfg.allowed_user_ids) state.allowed_user_ids = cfg.allowed_user_ids;
    if (cfg.openrouter_key)  state.openrouter_key  = cfg.openrouter_key;
    if (cfg.model)           state.model           = cfg.model;
    if (cfg.max_tokens)      state.max_tokens       = String(cfg.max_tokens);
    if (cfg.system_prompt)   state.system_prompt    = cfg.system_prompt;
    if (cfg.location)        state.location         = cfg.location;
    if (cfg.sandbox_dir)     state.sandbox_dir      = cfg.sandbox_dir;
    if (cfg.db_path)         state.db_path          = cfg.db_path;

    // Split MCP servers: catalog-matched vs custom
    for (const s of (cfg.mcp_servers || [])) {
      const inCatalog = MCP_CATALOG.find(c => c.name === s.name);
      if (inCatalog) {
        state.mcp_selections[s.name] = { selected: true, env: s.env || {} };
      } else {
        state.custom_mcp_servers.push({
          name: s.name,
          command: s.command,
          args: s.args || [],
          env: s.env || {},
        });
      }
    }

    state._loaded = true;
  } catch (_) {
    // Network error or JSON parse error — start with blank wizard
  }
}
```

### Step 3: Replace the bottom two lines with an async init()

Replace:
```js
buildSteps();
showStep(1);
```
With:
```js
async function init() {
  await loadExistingConfig();
  buildSteps();
  if (state._loaded) {
    document.getElementById('existing-banner').style.display = 'block';
  }
  showStep(1);
}
init();
```

### Step 4: Add the existing-config banner to step 1 HTML

In the step-1 template inside `buildSteps()`, add the banner div right before the closing `</div>`:

**Before:**
```js
  c.innerHTML += `<div class="step" id="step-1">
  <h1>Welcome to RustBot</h1>
  <p class="subtitle">
    Your personal AI assistant on Telegram, powered by OpenRouter LLMs and extensible via MCP tools.<br><br>
    This wizard creates your <code>config.toml</code> in about 2 minutes.
    You can always edit it by hand afterwards.
  </p>
</div>`;
```

**After:**
```js
  c.innerHTML += `<div class="step" id="step-1">
  <h1>Welcome to RustBot</h1>
  <p class="subtitle">
    Your personal AI assistant on Telegram, powered by OpenRouter LLMs and extensible via MCP tools.<br><br>
    This wizard creates your <code>config.toml</code> in about 2 minutes.
    You can always edit it by hand afterwards.
  </p>
  <div id="existing-banner" style="display:none;background:#1a2744;border:1px solid #2b4a8a;border-radius:8px;padding:0.75rem 1rem;font-size:0.85rem;color:#90cdf4">
    <strong style="color:#bee3f8">Editing existing configuration</strong> — all fields pre-loaded from <code>config.toml</code>.
  </div>
</div>`;
```

### Step 5: Verify page loads without JS errors

```bash
cargo run --bin setup 2>&1 &
sleep 2
curl -s http://localhost:8719/api/load-config | head -5
# kill the background process
kill %1
```
Expected: JSON like `{"exists":false,"telegram_token":"","..."}` (or `"exists":true` if config.toml present).

### Step 6: Commit

```bash
git add setup/index.html
git commit -m "feat(setup): async init with loadExistingConfig and existing-config banner"
```

---

## Task 6: Frontend — Pre-fill form inputs from state in `buildSteps()`

**Files:**
- Modify: `setup/index.html`

The form inputs in steps 2–4 need `value` attributes driven from `state.*` so they show loaded values when the user navigates to each step.

### Step 1: Pre-fill step 2 (Telegram)

Replace the two `<input>` lines in step 2:
```js
    <input type="password" id="f-telegram-token" placeholder="1234567890:ABC-...">
    ...
    <input type="text" id="f-allowed-ids" placeholder="123456789, 987654321">
```
With (backtick template — state values already populated by `loadExistingConfig` before `buildSteps` runs):
```js
    <input type="password" id="f-telegram-token" placeholder="1234567890:ABC-..." value="${esc(state.telegram_token)}">
    ...
    <input type="text" id="f-allowed-ids" placeholder="123456789, 987654321" value="${esc(state.allowed_user_ids)}">
```

### Step 2: Pre-fill step 3 (OpenRouter)

Replace the relevant inputs in step 3 (model already has a `value`, just update):
```js
    <input type="password" id="f-openrouter-key" placeholder="sk-or-..." value="${esc(state.openrouter_key)}">
    ...
    <input type="text" id="f-model" value="${esc(state.model || 'qwen/qwen3-235b-a22b')}">
    ...
    <input type="text" id="f-max-tokens" value="${esc(state.max_tokens || '4096')}">
    ...
    <textarea id="f-system-prompt">${esc(state.system_prompt)}</textarea>
    ...
    <input type="text" id="f-location" placeholder="Tokyo, Japan" value="${esc(state.location)}">
```

**Note on textarea:** For `<textarea>`, the content goes between the tags — `value` attribute doesn't work. Use `${esc(state.system_prompt)}` between `<textarea ...>` and `</textarea>`. The `esc()` function must also escape `<` and `>` for safe HTML injection — update it:

```js
function esc(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
```
(Previously only escaped `\` and `"` — this was only safe for TOML strings, not HTML attributes.)

### Step 3: Pre-fill step 4 (Sandbox & Memory)

```js
    <input type="text" id="f-sandbox-dir" value="${esc(state.sandbox_dir || '/tmp/rustbot-sandbox')}">
    ...
    <input type="text" id="f-db-path" value="${esc(state.db_path || 'rustbot.db')}">
```

### Step 4: Pre-check catalog MCP checkboxes

In the step-5 catalog card template, update the checkbox line from:
```js
    <input type="checkbox" id="mcp-cb-${tool.name}" onclick="...">
```
To (add `${state.mcp_selections[tool.name]?.selected ? 'checked' : ''}` and conditional classes):
```js
    <input type="checkbox" id="mcp-cb-${tool.name}"
      ${state.mcp_selections[tool.name]?.selected ? 'checked' : ''}
      onclick="event.stopPropagation();toggleTool('${tool.name}',event)">
```

Also, make the wrapper div and env fields reflect loaded state:
```js
<div class="mcp-tool ${state.mcp_selections[tool.name]?.selected ? 'selected' : ''}"
     id="mcp-wrap-${tool.name}" onclick="toggleTool('${tool.name}', event)">
```

And for env var inputs, pre-fill loaded values:
```js
const envHtml = tool.envVars.map(k => {
  const existingVal = state.mcp_selections[tool.name]?.env?.[k] || '';
  return `<div class="env-row">
    <div class="mcp-env-label">${k}</div>
    <input type="text" placeholder="${k}" value="${esc(existingVal)}"
           oninput="setEnv('${tool.name}','${k}',this.value)">
  </div>`;
}).join('');
```

And make the env div visible if the tool is pre-selected:
```js
${tool.envVars.length ? `<div class="mcp-env-fields ${state.mcp_selections[tool.name]?.selected ? 'visible' : ''}" id="mcp-env-${tool.name}">${envHtml}</div>` : ''}
```

### Step 5: Fix location in generateToml() — move to top level

`location` must appear outside any `[section]` block. Change the TOML generation to emit it at the top level. In `generateToml()`:

**Before** (location is emitted inside the `[openrouter]` block):
```js
  let toml =
    '[telegram]\n' + ... +
    '[openrouter]\n' + ... +
    'system_prompt = """' + sysprompt + '"""\n' +
    locLine + '\n' +    // <-- WRONG: inside [openrouter]
    '\n' +
    '[sandbox]\n' + ...
```

**After** (location after `[skills]` at top level):
```js
  let toml =
    '[telegram]\n' + ... +
    '[openrouter]\n' + ... +
    'system_prompt = """' + sysprompt + '"""\n' +
    '\n' +
    '[sandbox]\n' + ... +
    '\n' +
    '[memory]\n' + ... +
    '\n' +
    '[skills]\n' +
    'directory = "skills"\n' +
    '\n' +
    locLine + '\n';   // <-- CORRECT: after last section, top-level
```

### Step 6: Verify compile + quick manual check

```bash
cargo check 2>&1 | grep -E "^error"
```

### Step 7: Commit

```bash
git add setup/index.html
git commit -m "feat(setup): pre-fill form inputs and MCP checkboxes from loaded config"
```

---

## Task 7: Frontend — Custom MCP Server UI (section in step 5)

**Files:**
- Modify: `setup/index.html`

### Step 1: Add CSS for custom server cards

In the `<style>` block, add after the `.mcp-more` rules:

```css
  /* Custom MCP server section */
  .custom-mcp-section { margin-top: 1.75rem; border-top: 1px solid #2d3748; padding-top: 1.25rem; }
  .custom-mcp-section h3 { font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.08em; color: #718096; margin-bottom: 0.75rem; }
  .custom-card {
    border: 1px solid #2d3748;
    border-radius: 8px;
    padding: 0.65rem 1rem;
    margin-bottom: 0.5rem;
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
  }
  .custom-card-body { flex: 1; min-width: 0; }
  .custom-card-name { font-weight: 600; font-size: 0.9rem; margin-bottom: 0.2rem; }
  .custom-card-cmd { font-size: 0.75rem; color: #4a5568; font-family: monospace; }
  .custom-card-env { font-size: 0.72rem; color: #718096; margin-top: 0.15rem; }
  .btn-remove { background: none; border: none; cursor: pointer; color: #718096; font-size: 1rem; padding: 0; line-height: 1; flex-shrink: 0; }
  .btn-remove:hover { color: #fc8181; opacity: 1; }
  .custom-add-form {
    border: 1px dashed #2d3748;
    border-radius: 8px;
    padding: 1rem;
    margin-top: 0.75rem;
  }
  .custom-add-form h4 { font-size: 0.85rem; font-weight: 600; color: #cbd5e0; margin-bottom: 0.75rem; }
  .cmd-row { display: flex; gap: 0.5rem; }
  .cmd-row select {
    background: #0f1117;
    border: 1px solid #2d3748;
    border-radius: 8px;
    color: #e2e8f0;
    padding: 0.6rem 0.5rem;
    font-size: 0.9rem;
    min-width: 80px;
  }
  .env-add-row { display: flex; gap: 0.5rem; margin-bottom: 0.4rem; }
  .env-add-row input { flex: 1; }
```

### Step 2: Add custom server functions

Add these functions **before** `buildSteps()`:

```js
// ── Custom MCP servers ──────────────────────────────────────────────────────

function renderCustomMcpCards() {
  const el = document.getElementById('custom-mcp-cards');
  if (!el) return;
  if (state.custom_mcp_servers.length === 0) {
    el.innerHTML = '';
    return;
  }
  el.innerHTML = state.custom_mcp_servers.map((s, i) => {
    const cmd = esc(s.command) + (s.args.length ? ' ' + s.args.map(esc).join(' ') : '');
    const envKeys = Object.keys(s.env);
    return `<div class="custom-card">
  <div class="custom-card-body">
    <div class="custom-card-name">${esc(s.name)}</div>
    <div class="custom-card-cmd">${cmd}</div>
    ${envKeys.length ? `<div class="custom-card-env">ENV: ${envKeys.map(esc).join(', ')}</div>` : ''}
  </div>
  <button class="btn-remove" onclick="removeCustomServer(${i})" title="Remove">✕</button>
</div>`;
  }).join('');
}

function removeCustomServer(i) {
  state.custom_mcp_servers.splice(i, 1);
  renderCustomMcpCards();
}

function addCustomEnvRow() {
  const container = document.getElementById('custom-env-rows');
  const row = document.createElement('div');
  row.className = 'env-add-row';
  row.innerHTML = '<input type="text" placeholder="KEY" style="flex:1"><input type="text" placeholder="value" style="flex:2">';
  container.appendChild(row);
}

function syncCustomCmd() {
  const preset = document.getElementById('custom-cmd-preset').value;
  const cmdInput = document.getElementById('custom-cmd');
  if (preset !== 'custom') cmdInput.value = preset;
  cmdInput.readOnly = preset !== 'custom';
}

function addCustomServer() {
  const nameInput = document.getElementById('custom-name');
  const name = nameInput.value.trim();
  if (!name) {
    nameInput.classList.add('error');
    return;
  }
  nameInput.classList.remove('error');

  const command = document.getElementById('custom-cmd').value.trim() || 'npx';
  const argsRaw = document.getElementById('custom-args').value.trim();
  const args = argsRaw ? argsRaw.split(/\s+/) : [];

  const env = {};
  document.getElementById('custom-env-rows').querySelectorAll('.env-add-row').forEach(row => {
    const inputs = row.querySelectorAll('input');
    const k = inputs[0]?.value.trim();
    const v = inputs[1]?.value.trim();
    if (k) env[k] = v || '';
  });

  state.custom_mcp_servers.push({ name, command, args, env });
  renderCustomMcpCards();

  // Reset form
  nameInput.value = '';
  document.getElementById('custom-args').value = '';
  document.getElementById('custom-env-rows').innerHTML = '';
}
```

### Step 3: Add the Custom Servers section to step 5 HTML

In `buildSteps()`, the step-5 HTML block ends with:
```js
  html5 += `<div class="mcp-more">...</div>`;
  html5 += `</div>`;
  c.innerHTML += html5;
```

Change it to inject the custom section **before** `</div>`:
```js
  html5 += `<div class="mcp-more">
    Want more tools? Browse the <a href="https://github.com/modelcontextprotocol/servers" target="_blank" rel="noopener">MCP server registry</a>
    or <a href="https://mcp.so/" target="_blank" rel="noopener">mcp.so</a> and add them manually in <code>config.toml</code>.
  </div>
  <div class="custom-mcp-section">
    <h3>Custom Servers</h3>
    <div id="custom-mcp-cards"></div>
    <div class="custom-add-form">
      <h4>Add Custom Server</h4>
      <div class="field">
        <label>Name</label>
        <input type="text" id="custom-name" placeholder="my-server">
      </div>
      <div class="field">
        <label>Command</label>
        <div class="cmd-row">
          <select id="custom-cmd-preset" onchange="syncCustomCmd()">
            <option value="npx">npx</option>
            <option value="uvx">uvx</option>
            <option value="custom">custom...</option>
          </select>
          <input type="text" id="custom-cmd" value="npx" style="flex:1">
        </div>
      </div>
      <div class="field">
        <label>Args <span class="hint">— space-separated</span></label>
        <input type="text" id="custom-args" placeholder="-y my-mcp-package">
      </div>
      <div class="field">
        <label>Env Vars <span class="hint">— optional</span></label>
        <div id="custom-env-rows"></div>
        <button type="button" class="btn-secondary" onclick="addCustomEnvRow()" style="margin-top:0.4rem;padding:0.3rem 0.75rem;font-size:0.8rem">+ Add env var</button>
      </div>
      <button type="button" class="btn-primary" onclick="addCustomServer()">Add Server</button>
    </div>
  </div>`;
  html5 += `</div>`;
  c.innerHTML += html5;
```

### Step 4: Call `renderCustomMcpCards()` in `init()`

In the `init()` function, add after `buildSteps()`:
```js
async function init() {
  await loadExistingConfig();
  buildSteps();
  renderCustomMcpCards();   // NEW — populate pre-loaded custom servers
  if (state._loaded) {
    document.getElementById('existing-banner').style.display = 'block';
  }
  showStep(1);
}
```

### Step 5: Update `generateToml()` to include custom servers

After the catalog MCP loop, add:

```js
  for (const s of state.custom_mcp_servers) {
    const args = s.args.map(a => '"' + esc(a) + '"').join(', ');
    toml += '\n[[mcp_servers]]\n';
    toml += 'name = "' + esc(s.name) + '"\n';
    toml += 'command = "' + esc(s.command) + '"\n';
    toml += 'args = [' + args + ']\n';
    const envEntries = Object.entries(s.env).filter(([k]) => k.trim());
    if (envEntries.length > 0) {
      toml += '[mcp_servers.env]\n';
      for (const [k, v] of envEntries) {
        toml += k + ' = "' + esc(v) + '"\n';
      }
    }
  }
```

### Step 6: Commit

```bash
git add setup/index.html
git commit -m "feat(setup): add custom MCP server UI with add/remove and pre-load support"
```

---

## Task 8: Final verification + push

### Step 1: Clippy

```bash
cargo clippy -- -D warnings 2>&1 | grep -E "^error"
```
Expected: no output.

### Step 2: All tests pass

```bash
cargo test 2>&1 | tail -15
```
Expected: all tests pass including the 4 new `test_parse_*` tests.

### Step 3: Manual smoke test

```bash
# Build and run setup
cargo run --bin setup &
SETUP_PID=$!
sleep 2

# Verify load-config returns exists:false when no config.toml
curl -s http://localhost:8719/api/load-config

# Kill and start fresh with a real config
kill $SETUP_PID
```
Open `http://localhost:8719` in browser, complete the wizard, verify:
- Step 7 "You're all set!" appears after save
- Re-run `cargo run --bin setup`, verify the banner appears and all fields are pre-populated
- Add a custom server in step 5, verify it appears in the config preview on step 6
- Save again, re-run setup, verify the custom server appears as a card in step 5

### Step 4: Push

```bash
git push -u origin claude/setup-script-web-ui-Q76AI
```
