# Multi-Provider LLM Support Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Ollama (local) and native OpenAI API as first-class LLM providers alongside OpenRouter, controlled by a single `[llm]` config section and surfaced through the setup wizard UI.

**Architecture:** Replace the `[openrouter]` TOML section with a unified `[llm]` section that has a `provider` field (`"openrouter"` | `"ollama"` | `"openai"`). All three providers use the same OpenAI-compatible `/chat/completions` endpoint via the existing `LlmClient`; the only behavioural difference is whether the `Authorization` header is sent (skipped when `api_key` is empty). The setup wizard gains provider tabs and a live Ollama model-discovery endpoint.

**Tech Stack:** Rust 2021, `serde`/`toml`, `reqwest`, `axum` (setup binary only), vanilla JS in `setup/index.html`

---

## Task 1: Update `src/config.rs` — Add `LlmProvider` and `LlmConfig`

**Files:**
- Modify: `src/config.rs`

**Context:**
`Config` currently has `pub openrouter: OpenRouterConfig`. We replace that field with `pub llm: LlmConfig`. We also keep `OpenRouterConfig` around only as a private parse helper in `setup.rs` (not in the main config). Legacy `[openrouter]` fallback is handled in `Config::load()`.

**Step 1: Add the new types**

Add these types *above* the existing `OpenRouterConfig` struct:

```rust
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Openrouter,
    Ollama,
    Openai,
}

impl Default for LlmProvider {
    fn default() -> Self {
        LlmProvider::Openrouter
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    #[serde(default)]
    pub provider: LlmProvider,
    pub model: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}
```

**Step 2: Update `Config` struct**

Replace:
```rust
pub openrouter: OpenRouterConfig,
```
with:
```rust
pub llm: LlmConfig,
```

**Step 3: Add legacy fallback in `Config::load()`**

The existing `toml::from_str` will fail if the file has `[openrouter]` but no `[llm]`. Handle this by first trying a "raw" parse, then mapping legacy fields:

Add a private raw struct and fallback at the top of `Config::load()`:

```rust
pub fn load(path: &Path) -> Result<Self> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    // Try parsing as-is; if [llm] is missing but [openrouter] exists, migrate.
    let config: Config = match toml::from_str(&content) {
        Ok(c) => c,
        Err(_) => {
            // Attempt legacy migration: parse a loose struct that accepts [openrouter]
            #[derive(serde::Deserialize)]
            struct LegacyConfig {
                telegram: TelegramConfig,
                openrouter: Option<LegacyOpenRouter>,
                sandbox: SandboxConfig,
                #[serde(default)]
                mcp_servers: Vec<McpServerConfig>,
                #[serde(default = "default_memory_config")]
                memory: MemoryConfig,
                #[serde(default = "default_skills_config")]
                skills: SkillsConfig,
                #[serde(default)]
                general: Option<GeneralConfig>,
                #[serde(default = "default_agent_config")]
                agent: AgentConfig,
                embedding: Option<EmbeddingApiConfig>,
            }
            #[derive(serde::Deserialize, Default)]
            struct LegacyOpenRouter {
                #[serde(default)]
                api_key: String,
                #[serde(default = "default_model")]
                model: String,
                #[serde(default = "default_base_url")]
                base_url: String,
                #[serde(default = "default_max_tokens")]
                max_tokens: u32,
                #[serde(default = "default_system_prompt")]
                system_prompt: String,
            }
            let legacy: LegacyConfig = toml::from_str(&content)
                .context("Failed to parse config file (legacy and new format both failed)")?;
            let or = legacy.openrouter.unwrap_or_default();
            Config {
                telegram: legacy.telegram,
                llm: LlmConfig {
                    provider: LlmProvider::Openrouter,
                    model: or.model,
                    base_url: or.base_url,
                    api_key: or.api_key,
                    max_tokens: or.max_tokens,
                    system_prompt: or.system_prompt,
                },
                sandbox: legacy.sandbox,
                mcp_servers: legacy.mcp_servers,
                memory: legacy.memory,
                skills: legacy.skills,
                general: legacy.general,
                agent: legacy.agent,
                embedding: legacy.embedding,
            }
        }
    };

    if !config.sandbox.allowed_directory.exists() {
        std::fs::create_dir_all(&config.sandbox.allowed_directory).with_context(|| {
            format!(
                "Failed to create sandbox directory: {}",
                config.sandbox.allowed_directory.display()
            )
        })?;
    }

    Ok(config)
}
```

**Step 4: Update default functions**

Replace `default_base_url()` (was OpenRouter-specific) with a note that `LlmConfig` fills `base_url` from the provider when empty. The wizard always writes an explicit `base_url`, but for hand-edited configs we want a sensible fallback. Add to `config.rs`:

```rust
impl LlmConfig {
    /// Returns the effective base_url: if the stored value is empty,
    /// fall back to the canonical URL for the configured provider.
    pub fn effective_base_url(&self) -> &str {
        if !self.base_url.is_empty() {
            return &self.base_url;
        }
        match self.provider {
            LlmProvider::Openrouter => "https://openrouter.ai/api/v1",
            LlmProvider::Ollama => "http://localhost:11434/v1",
            LlmProvider::Openai => "https://api.openai.com/v1",
        }
    }
}
```

**Step 5: Remove `OpenRouterConfig` from `src/config.rs`**

Delete the entire `OpenRouterConfig` struct and its related `default_model()` / `default_base_url()` functions. (They're no longer used in `config.rs`; they'll be redefined locally in `setup.rs`.)

**Step 6: Check it compiles**

```bash
cargo check 2>&1 | head -40
```

Expected: Errors referencing `config.openrouter` in `src/main.rs` and `src/agent.rs` — we fix those next.

**Step 7: Commit**

```bash
git add src/config.rs
git commit -m "feat: add LlmProvider enum and LlmConfig, replace OpenRouterConfig"
```

---

## Task 2: Update `src/llm.rs` — Use `LlmConfig`, Conditional Auth

**Files:**
- Modify: `src/llm.rs`

**Context:**
`LlmClient` currently takes `OpenRouterConfig`. We switch it to `LlmConfig` and make the `Authorization` header conditional on `api_key` being non-empty.

**Step 1: Update the import and struct**

Replace:
```rust
use crate::config::OpenRouterConfig;
```
with:
```rust
use crate::config::LlmConfig;
```

Replace:
```rust
pub struct LlmClient {
    client: reqwest::Client,
    config: OpenRouterConfig,
}
```
with:
```rust
pub struct LlmClient {
    client: reqwest::Client,
    config: LlmConfig,
}
```

**Step 2: Update `new()`**

Replace:
```rust
pub fn new(config: OpenRouterConfig) -> Self {
```
with:
```rust
pub fn new(config: LlmConfig) -> Self {
```

**Step 3: Conditional auth header and base_url**

Replace the request-building section in `chat()`:
```rust
let url = format!("{}/chat/completions", self.config.base_url);

debug!("Sending request to OpenRouter: {}", url);

let response = self
    .client
    .post(&url)
    .header("Authorization", format!("Bearer {}", self.config.api_key))
    .header("Content-Type", "application/json")
    .json(&request)
    .send()
    .await
    .context("Failed to send request to OpenRouter")?;
```
with:
```rust
let url = format!("{}/chat/completions", self.config.effective_base_url());

debug!("Sending LLM request to: {}", url);

let mut req = self
    .client
    .post(&url)
    .header("Content-Type", "application/json");

if !self.config.api_key.is_empty() {
    req = req.header("Authorization", format!("Bearer {}", self.config.api_key));
}

let response = req
    .json(&request)
    .send()
    .await
    .context("Failed to send request to LLM provider")?;
```

**Step 4: Update error messages**

Replace `"No response from OpenRouter"` with `"No choices in LLM response"`.
Replace `"Failed to parse OpenRouter response"` with `"Failed to parse LLM response"`.
Replace the bail message `"OpenRouter API error ({}):"` with `"LLM API error ({}):"`.

**Step 5: Check it compiles**

```bash
cargo check 2>&1 | head -30
```

Expected: Still errors in `agent.rs` and `main.rs` referencing `config.openrouter`. Fix those next.

**Step 6: Commit**

```bash
git add src/llm.rs
git commit -m "feat: update LlmClient to use LlmConfig with conditional auth header"
```

---

## Task 3: Update `src/agent.rs` and `src/main.rs` — Wire Up New Config Field

**Files:**
- Modify: `src/agent.rs`
- Modify: `src/main.rs`

**Context:**
`agent.rs` references `config.openrouter` in two places; `main.rs` references it in one log line.

**Step 1: Fix `src/agent.rs` — `LlmClient::new` call**

Find line ~57:
```rust
let llm = LlmClient::new(config.openrouter.clone());
```
Replace with:
```rust
let llm = LlmClient::new(config.llm.clone());
```

**Step 2: Fix `src/agent.rs` — system prompt**

Find line ~74:
```rust
let mut prompt = self.config.openrouter.system_prompt.clone();
```
Replace with:
```rust
let mut prompt = self.config.llm.system_prompt.clone();
```

**Step 3: Fix `src/main.rs` — log line**

Find line ~48:
```rust
info!("  Model: {}", config.openrouter.model);
```
Replace with:
```rust
info!("  Provider: {:?}", config.llm.provider);
info!("  Model: {}", config.llm.model);
```

**Step 4: Full compile check**

```bash
cargo check 2>&1
```

Expected: **No errors.** If there are any remaining `openrouter` references, find them:
```bash
grep -rn "openrouter\b" src/ --include="*.rs"
```
Fix each remaining reference.

**Step 5: Run tests**

```bash
cargo test
```

Expected: All tests pass.

**Step 6: Commit**

```bash
git add src/agent.rs src/main.rs
git commit -m "feat: wire LlmConfig through agent and main, remove openrouter references"
```

---

## Task 4: Update `src/bin/setup.rs` — New Endpoint + Config Migration

**Files:**
- Modify: `src/bin/setup.rs`

This task has multiple sub-steps. Take them one at a time.

### 4a: Add `OllamaModelsResponse` types and the `/api/ollama-models` endpoint

**Step 1: Add response types after the existing `ExistingMcpServer` struct**

```rust
#[derive(Serialize)]
struct OllamaModelsResponse {
    ok: bool,
    models: Vec<String>,
}
```

**Step 2: Add the handler function after `load_config`**

```rust
async fn list_ollama_models() -> Json<OllamaModelsResponse> {
    #[derive(Deserialize)]
    struct TagsResponse {
        models: Vec<OllamaModel>,
    }
    #[derive(Deserialize)]
    struct OllamaModel {
        name: String,
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    match client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<TagsResponse>().await {
                Ok(tags) => Json(OllamaModelsResponse {
                    ok: true,
                    models: tags.models.into_iter().map(|m| m.name).collect(),
                }),
                Err(_) => Json(OllamaModelsResponse { ok: false, models: vec![] }),
            }
        }
        _ => Json(OllamaModelsResponse { ok: false, models: vec![] }),
    }
}
```

**Step 3: Register the route in `main()`**

Find the router construction and add the new route:
```rust
let app = Router::new()
    .route("/", get(serve_index))
    .route("/api/load-config", get(load_config))
    .route("/api/save-config", post(save_config))
    .route("/api/ollama-models", get(list_ollama_models))  // ← add this line
    .with_state(state);
```

**Step 4: Check it compiles**

```bash
cargo check --bin setup 2>&1 | head -30
```

### 4b: Update `ExistingConfig` and `RawConfig`

**Step 1: Update `ExistingConfig`**

Replace the existing struct:
```rust
#[derive(Serialize, Default)]
struct ExistingConfig {
    exists: bool,
    telegram_token: String,
    allowed_user_ids: String,
    openrouter_key: String,
    model: String,
    max_tokens: u32,
    system_prompt: String,
    location: String,
    sandbox_dir: String,
    db_path: String,
    mcp_servers: Vec<ExistingMcpServer>,
}
```
with:
```rust
#[derive(Serialize, Default)]
struct ExistingConfig {
    exists: bool,
    telegram_token: String,
    allowed_user_ids: String,
    provider: String,        // "openrouter" | "ollama" | "openai"
    llm_key: String,         // api_key (empty for ollama)
    model: String,
    base_url: String,
    max_tokens: u32,
    system_prompt: String,
    location: String,
    sandbox_dir: String,
    db_path: String,
    mcp_servers: Vec<ExistingMcpServer>,
}
```

**Step 2: Update `RawConfig` to read both `[llm]` and legacy `[openrouter]`**

Add a `RawLlm` struct and update `RawConfig`:

```rust
#[derive(Deserialize, Default)]
struct RawLlm {
    provider: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
    max_tokens: Option<u32>,
    system_prompt: Option<String>,
}

// Add to RawConfig:
#[derive(Deserialize, Default)]
struct RawConfig {
    telegram: Option<RawTelegram>,
    llm: Option<RawLlm>,          // ← new
    openrouter: Option<RawOpenRouter>, // ← keep for legacy
    sandbox: Option<RawSandbox>,
    memory: Option<RawMemory>,
    general: Option<RawGeneral>,
    #[serde(default)]
    mcp_servers: Vec<RawMcpServer>,
}
```

**Step 3: Update `parse_existing_config()`**

Replace the openrouter parsing block:
```rust
let openrouter = raw.openrouter.unwrap_or_default();
// ...
openrouter_key: openrouter.api_key.unwrap_or_default(),
model: openrouter.model.unwrap_or_default(),
max_tokens: openrouter.max_tokens.unwrap_or(0),
system_prompt: openrouter.system_prompt.unwrap_or_default(),
```
with logic that prefers `[llm]` over `[openrouter]`:
```rust
// Prefer [llm]; fall back to [openrouter] for legacy configs.
let (provider, llm_key, model, base_url, max_tokens, system_prompt) =
    if let Some(llm) = raw.llm {
        (
            llm.provider.unwrap_or_else(|| "openrouter".to_string()),
            llm.api_key.unwrap_or_default(),
            llm.model.unwrap_or_default(),
            llm.base_url.unwrap_or_default(),
            llm.max_tokens.unwrap_or(0),
            llm.system_prompt.unwrap_or_default(),
        )
    } else if let Some(or) = raw.openrouter {
        (
            "openrouter".to_string(),
            or.api_key.unwrap_or_default(),
            or.model.unwrap_or_default(),
            or.base_url.unwrap_or_default(),
            or.max_tokens.unwrap_or(0),
            or.system_prompt.unwrap_or_default(),
        )
    } else {
        ("openrouter".to_string(), String::new(), String::new(), String::new(), 0, String::new())
    };
```

Then update the `ExistingConfig` construction:
```rust
ExistingConfig {
    exists: true,
    telegram_token: tg.bot_token.unwrap_or_default(),
    allowed_user_ids,
    provider,
    llm_key,
    model,
    base_url,
    max_tokens,
    system_prompt,
    location: ...,
    sandbox_dir: ...,
    db_path: ...,
    mcp_servers,
}
```

### 4c: Update `format_config()` and `ConfigParams`

**Step 1: Update `ConfigParams`**

Replace:
```rust
struct ConfigParams<'a> {
    tg_token: &'a str,
    user_ids: &'a str,
    or_key: &'a str,
    model: &'a str,
    max_tokens: u32,
    sandbox: &'a str,
    db_path: &'a str,
    location: &'a str,
}
```
with:
```rust
struct ConfigParams<'a> {
    tg_token: &'a str,
    user_ids: &'a str,
    provider: &'a str,
    llm_key: &'a str,
    model: &'a str,
    base_url: &'a str,
    max_tokens: u32,
    system_prompt: &'a str,
    sandbox: &'a str,
    db_path: &'a str,
    location: &'a str,
}
```

**Step 2: Update `format_config()`**

Replace the `[openrouter]` block in the format string:

Old:
```rust
'[openrouter]\n' +
'api_key = "{or_key}"\n' +
'model = "{model}"\n' +
'base_url = "https://openrouter.ai/api/v1"\n' +
'max_tokens = {max_tokens}\n' +
'system_prompt = """..."""\n'
```

New (Rust format string approach):
```rust
fn format_config(p: &ConfigParams<'_>) -> String {
    let ids: Vec<&str> = p
        .user_ids
        .split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let ids_str = ids.join(", ");

    let loc_line = if p.location.is_empty() {
        "# location = \"Your City, Country\"".to_owned()
    } else {
        format!("location = \"{}\"", p.location)
    };

    let tg_token = p.tg_token;
    let provider = p.provider;
    let llm_key = p.llm_key;
    let model = p.model;
    let base_url = p.base_url;
    let max_tokens = p.max_tokens;
    let system_prompt = p.system_prompt;
    let sandbox = p.sandbox;
    let db_path = p.db_path;

    format!(
        r#"[telegram]
bot_token = "{tg_token}"
allowed_user_ids = [{ids_str}]

[llm]
provider = "{provider}"
model = "{model}"
base_url = "{base_url}"
api_key = "{llm_key}"
max_tokens = {max_tokens}
system_prompt = """{system_prompt}"""

[sandbox]
allowed_directory = "{sandbox}"

[memory]
database_path = "{db_path}"

[skills]
directory = "skills"

[general]
{loc_line}
"#
    )
}
```

### 4d: Update CLI wizard

**Step 1: Update `run_cli()`**

Replace the OpenRouter prompt block:
```rust
let or_key = read_line("OpenRouter API key: ")?;
let model = or_default(
    read_line("Model [moonshotai/kimi-k2.5]: ")?,
    "moonshotai/kimi-k2.5",
);
```
with:
```rust
let provider = or_default(
    read_line("Provider [openrouter/ollama/openai] (default: openrouter): ")?,
    "openrouter",
);
let (llm_key, model, base_url) = match provider.as_str() {
    "ollama" => {
        let base = or_default(
            read_line("Ollama base URL [http://localhost:11434/v1]: ")?,
            "http://localhost:11434/v1",
        );
        let model = or_default(
            read_line("Model [qwen2.5:14b]: ")?,
            "qwen2.5:14b",
        );
        (String::new(), model, base)
    }
    "openai" => {
        let key = read_line("OpenAI API key (sk-...): ")?;
        let model = or_default(
            read_line("Model [gpt-4o]: ")?,
            "gpt-4o",
        );
        (key, model, "https://api.openai.com/v1".to_string())
    }
    _ => {
        let key = read_line("OpenRouter API key: ")?;
        let model = or_default(
            read_line("Model [moonshotai/kimi-k2.5]: ")?,
            "moonshotai/kimi-k2.5",
        );
        (key, model, "https://openrouter.ai/api/v1".to_string())
    }
};
```

Update the `format_config` call in `run_cli()`:
```rust
let config = format_config(&ConfigParams {
    tg_token: &tg_token,
    user_ids: &user_ids,
    provider: &provider,
    llm_key: &llm_key,
    model: &model,
    base_url: &base_url,
    max_tokens: 4096,
    system_prompt: "You are a helpful AI assistant with access to tools. \
Use the available tools to help the user with their tasks. \
When using file or terminal tools, operate only within the allowed sandbox directory. \
Be concise and helpful.",
    sandbox: &sandbox,
    db_path: &db_path,
    location: &location,
});
```

### 4e: Update existing unit tests in `setup.rs`

The tests call `parse_existing_config` and `format_config` — update them to use the new field names:

- In `test_parse_full_config`: replace `cfg.openrouter_key` → `cfg.llm_key`, add `assert_eq!(cfg.provider, "openrouter")`; update the TOML input to use `[llm]` section
- In `test_openrouter_section_present`: rename to `test_llm_section_present`, update assertions to check for `[llm]` and `provider =`
- Fix `ConfigParams` usages in `cfg()` helper to pass the new fields

**Step: Run tests**

```bash
cargo test --bin setup 2>&1
```

Expected: All tests pass.

**Step: Full build check**

```bash
cargo check 2>&1
```

Expected: No errors.

**Step: Commit**

```bash
git add src/bin/setup.rs
git commit -m "feat: add ollama-models endpoint, update setup wizard backend for multi-provider"
```

---

## Task 5: Update `setup/index.html` — Provider Tab UI

**Files:**
- Modify: `setup/index.html`

This is the largest single-file change. Work section by section.

### 5a: Update JS state and constants

**Step 1: Update the `state` object**

Replace:
```js
const state = {
  ...
  openrouter_key: '',
  model: 'moonshotai/kimi-k2.5',
  ...
};
```
with:
```js
const state = {
  telegram_token: '',
  allowed_user_ids: '',
  provider: 'openrouter',          // 'openrouter' | 'ollama' | 'openai'
  llm_key: '',                      // api_key (empty for ollama)
  model: 'moonshotai/kimi-k2.5',
  base_url: '',                     // empty = use provider default
  max_tokens: '4096',
  system_prompt: 'You are a helpful AI assistant with access to tools. Use the available tools to help the user with their tasks. When using file or terminal tools, operate only within the allowed sandbox directory. Be concise and helpful.',
  location: 'Hong Kong',
  sandbox_dir: '/tmp/rustfox-sandbox',
  db_path: 'rustfox.db',
  mcp_selections: {},
  custom_mcp_servers: [],
  _loaded: false,
  _ollama_models: [],              // cached from /api/ollama-models
};
```

**Step 2: Add provider default URLs constant**

```js
const PROVIDER_DEFAULTS = {
  openrouter: { base_url: 'https://openrouter.ai/api/v1', model: 'moonshotai/kimi-k2.5' },
  ollama:     { base_url: 'http://localhost:11434',        model: '' },
  openai:     { base_url: 'https://api.openai.com/v1',     model: 'gpt-4o' },
};

const OPENAI_MODELS = [
  'gpt-4o', 'gpt-4o-mini', 'o3-mini', 'o4-mini', 'gpt-4-turbo',
];
```

### 5b: Add CSS for provider tabs

Add inside the `<style>` block (after the last rule, before `</style>`):

```css
/* Provider tabs */
.provider-tabs { display: flex; gap: 0; margin-bottom: 1.5rem; border-radius: 8px; overflow: hidden; border: 1px solid #2d3748; }
.provider-tab { flex: 1; padding: 0.55rem 0; text-align: center; font-size: 0.85rem; font-weight: 600; cursor: pointer; background: #0f1117; color: #718096; border: none; transition: background 0.15s, color 0.15s; }
.provider-tab.active { background: #f6851b; color: #fff; }
.provider-tab:hover:not(.active) { background: #1a1f2e; color: #cbd5e0; }
.provider-panel { display: none; }
.provider-panel.active { display: block; }
/* Ollama model row */
.ollama-model-row { display: flex; gap: 0.5rem; align-items: flex-start; }
.ollama-model-row select { flex: 1; background: #0f1117; border: 1px solid #2d3748; border-radius: 8px; color: #e2e8f0; padding: 0.6rem 0.875rem; font-size: 0.9rem; }
.ollama-fetch-btn { padding: 0.6rem 1rem; background: #2d3748; color: #e2e8f0; border: none; border-radius: 8px; font-size: 0.85rem; font-weight: 600; cursor: pointer; white-space: nowrap; }
.ollama-fetch-btn:hover { background: #4a5568; }
.ollama-status { font-size: 0.78rem; margin-top: 0.3rem; }
.ollama-status.ok { color: #68d391; }
.ollama-status.err { color: #fc8181; }
/* OpenAI custom model */
.openai-custom-wrap { margin-top: 0.5rem; display: none; }
.openai-custom-wrap.visible { display: block; }
```

### 5c: Update `validateStep()` and `collectStep()`

**Step 1: Update `validateStep()`**

Replace the step-3 validation:
```js
if (n === 3) {
  ok = requireField('f-openrouter-key') & ok;
  ok = requireField('f-model') & ok;
}
```
with:
```js
if (n === 3) {
  const p = state.provider;
  if (p === 'openrouter') {
    ok = requireField('f-or-key') & ok;
    ok = requireField('f-or-model') & ok;
  } else if (p === 'ollama') {
    ok = requireField('f-ollama-model') & ok;
  } else if (p === 'openai') {
    ok = requireField('f-oa-key') & ok;
    const sel = document.getElementById('f-oa-model-select');
    if (sel && sel.value === 'custom') {
      ok = requireField('f-oa-model-custom') & ok;
    }
  }
}
```

**Step 2: Update `collectStep()` for step 3**

Replace the step-3 collect block:
```js
if (n === 3) {
  state.openrouter_key = document.getElementById('f-openrouter-key').value.trim();
  state.model = document.getElementById('f-model').value.trim();
  state.max_tokens = document.getElementById('f-max-tokens').value.trim() || '4096';
  state.system_prompt = document.getElementById('f-system-prompt').value;
  state.location = document.getElementById('f-location').value.trim();
}
```
with:
```js
if (n === 3) {
  const p = state.provider;
  if (p === 'openrouter') {
    state.llm_key  = document.getElementById('f-or-key').value.trim();
    state.model    = document.getElementById('f-or-model').value.trim();
    state.base_url = PROVIDER_DEFAULTS.openrouter.base_url;
  } else if (p === 'ollama') {
    state.llm_key  = '';
    state.model    = document.getElementById('f-ollama-model').value.trim();
    state.base_url = (document.getElementById('f-ollama-url').value.trim() || PROVIDER_DEFAULTS.ollama.base_url) + '/v1';
    // strip duplicate /v1/v1
    state.base_url = state.base_url.replace(/\/v1\/v1$/, '/v1');
  } else if (p === 'openai') {
    state.llm_key  = document.getElementById('f-oa-key').value.trim();
    const sel = document.getElementById('f-oa-model-select');
    state.model = sel.value === 'custom'
      ? document.getElementById('f-oa-model-custom').value.trim()
      : sel.value;
    state.base_url = PROVIDER_DEFAULTS.openai.base_url;
  }
  state.max_tokens   = document.getElementById('f-max-tokens-' + p).value.trim() || '4096';
  state.system_prompt = document.getElementById('f-system-prompt-' + p).value;
  state.location     = document.getElementById('f-location-' + p).value.trim();
}
```

### 5d: Update `generateToml()`

Replace the `[openrouter]` block:
```js
'[openrouter]\n' +
'api_key = "' + esc(state.openrouter_key) + '"\n' +
'model = "' + esc(state.model) + '"\n' +
'base_url = "https://openrouter.ai/api/v1"\n' +
'max_tokens = ' + (parseInt(state.max_tokens, 10) || 4096) + '\n' +
'system_prompt = """' + sysprompt + '"""\n' +
```
with:
```js
'[llm]\n' +
'provider = "' + esc(state.provider) + '"\n' +
'model = "' + esc(state.model) + '"\n' +
'base_url = "' + esc(state.base_url || PROVIDER_DEFAULTS[state.provider]?.base_url || '') + '"\n' +
'api_key = "' + esc(state.llm_key) + '"\n' +
'max_tokens = ' + (parseInt(state.max_tokens, 10) || 4096) + '\n' +
'system_prompt = """' + sysprompt + '"""\n' +
```

### 5e: Add Ollama fetch helper

Add before `buildSteps()`:

```js
async function fetchOllamaModels() {
  const btn = document.getElementById('ollama-fetch-btn');
  const status = document.getElementById('ollama-status');
  const sel = document.getElementById('f-ollama-model');
  if (btn) btn.disabled = true;
  if (status) { status.textContent = 'Fetching…'; status.className = 'ollama-status'; }
  try {
    const res = await fetch('/api/ollama-models');
    const data = await res.json();
    if (data.ok && data.models.length > 0) {
      state._ollama_models = data.models;
      if (sel) {
        sel.innerHTML = data.models
          .map(m => `<option value="${escHtml(m)}"${m === state.model ? ' selected' : ''}>${escHtml(m)}</option>`)
          .join('');
        sel.style.display = '';
      }
      if (status) { status.textContent = `${data.models.length} model(s) found`; status.className = 'ollama-status ok'; }
    } else {
      if (status) { status.textContent = 'Ollama not reachable — type model name manually'; status.className = 'ollama-status err'; }
      if (sel) sel.style.display = 'none';
    }
  } catch (_) {
    if (status) { status.textContent = 'Fetch failed — type model name manually'; status.className = 'ollama-status err'; }
    if (sel) sel.style.display = 'none';
  }
  if (btn) btn.disabled = false;
}

function switchProvider(p) {
  state.provider = p;
  document.querySelectorAll('.provider-tab').forEach(t => t.classList.toggle('active', t.dataset.p === p));
  document.querySelectorAll('.provider-panel').forEach(panel => panel.classList.toggle('active', panel.dataset.p === p));
}

function toggleOpenAiCustom() {
  const sel = document.getElementById('f-oa-model-select');
  const wrap = document.getElementById('oa-custom-wrap');
  if (!sel || !wrap) return;
  wrap.classList.toggle('visible', sel.value === 'custom');
}
```

### 5f: Replace Step 3 HTML in `buildSteps()`

Replace the entire Step 3 block:
```js
c.innerHTML += `<div class="step" id="step-3">
  <h2>OpenRouter</h2>
  ...
</div>`;
```

with:

```js
// Shared tail fields (max_tokens, system_prompt, location) — rendered per panel
function sharedFields(p) {
  return `
  <div class="field">
    <label>Max Tokens</label>
    <input type="text" id="f-max-tokens-${p}" value="${escHtml(state.max_tokens || '4096')}">
  </div>
  <div class="field">
    <label>System Prompt</label>
    <textarea id="f-system-prompt-${p}">${escHtml(state.system_prompt)}</textarea>
  </div>
  <div class="field">
    <label>Location <span class="hint">— optional, gives the AI your timezone/region</span></label>
    <input type="text" id="f-location-${p}" placeholder="Hong Kong" value="${escHtml(state.location)}">
  </div>`;
}

c.innerHTML += `<div class="step" id="step-3">
  <h2>AI Provider</h2>

  <div class="provider-tabs">
    <button class="provider-tab ${state.provider==='openrouter'?'active':''}" data-p="openrouter" onclick="switchProvider('openrouter')">OpenRouter</button>
    <button class="provider-tab ${state.provider==='ollama'?'active':''}" data-p="ollama" onclick="switchProvider('ollama')">Ollama (local)</button>
    <button class="provider-tab ${state.provider==='openai'?'active':''}" data-p="openai" onclick="switchProvider('openai')">OpenAI</button>
  </div>

  <!-- OpenRouter panel -->
  <div class="provider-panel ${state.provider==='openrouter'?'active':''}" data-p="openrouter">
    <div class="field">
      <label>API Key <span class="hint">— openrouter.ai/keys</span></label>
      <input type="password" id="f-or-key" placeholder="sk-or-..." value="${escHtml(state.provider==='openrouter'?state.llm_key:'')}">
      <div class="error-msg" id="f-or-key-err">API key is required.</div>
    </div>
    <div class="field">
      <label>Model <span class="hint">— openrouter.ai/models</span></label>
      <input type="text" id="f-or-model" value="${escHtml(state.provider==='openrouter'?state.model:PROVIDER_DEFAULTS.openrouter.model)}">
      <div class="error-msg" id="f-or-model-err">Model is required.</div>
    </div>
    ${sharedFields('openrouter')}
  </div>

  <!-- Ollama panel -->
  <div class="provider-panel ${state.provider==='ollama'?'active':''}" data-p="ollama">
    <div class="field">
      <label>Ollama Base URL <span class="hint">— default: http://localhost:11434</span></label>
      <input type="text" id="f-ollama-url" value="${escHtml(state.provider==='ollama'?state.base_url.replace(/\/v1$/,''):'http://localhost:11434')}">
    </div>
    <div class="field">
      <label>Model</label>
      <div class="ollama-model-row">
        <select id="f-ollama-model" style="${state._ollama_models.length===0?'display:none':''}">
          ${state._ollama_models.map(m=>`<option value="${escHtml(m)}"${m===state.model?' selected':''}>${escHtml(m)}</option>`).join('')}
        </select>
        <input type="text" id="f-ollama-model" style="${state._ollama_models.length>0?'display:none':''}" placeholder="e.g. qwen2.5:14b" value="${escHtml(state.provider==='ollama'?state.model:'')}">
        <button class="ollama-fetch-btn" id="ollama-fetch-btn" onclick="fetchOllamaModels()">Fetch models</button>
      </div>
      <div class="ollama-status" id="ollama-status"></div>
      <div class="error-msg" id="f-ollama-model-err">Model is required.</div>
    </div>
    ${sharedFields('ollama')}
  </div>

  <!-- OpenAI panel -->
  <div class="provider-panel ${state.provider==='openai'?'active':''}" data-p="openai">
    <div class="field">
      <label>API Key <span class="hint">— platform.openai.com/api-keys</span></label>
      <input type="password" id="f-oa-key" placeholder="sk-..." value="${escHtml(state.provider==='openai'?state.llm_key:'')}">
      <div class="error-msg" id="f-oa-key-err">API key is required.</div>
    </div>
    <div class="field">
      <label>Model</label>
      <select id="f-oa-model-select" onchange="toggleOpenAiCustom()">
        ${OPENAI_MODELS.map(m=>`<option value="${m}"${(state.provider==='openai'&&state.model===m)?' selected':''}>${m}</option>`).join('')}
        <option value="custom"${(state.provider==='openai'&&!OPENAI_MODELS.includes(state.model))?' selected':''}>Custom…</option>
      </select>
      <div class="openai-custom-wrap${(state.provider==='openai'&&!OPENAI_MODELS.includes(state.model))?' visible':''}" id="oa-custom-wrap">
        <input type="text" id="f-oa-model-custom" placeholder="e.g. gpt-4.1" value="${escHtml(state.provider==='openai'&&!OPENAI_MODELS.includes(state.model)?state.model:'')}">
        <div class="error-msg" id="f-oa-model-custom-err">Model name is required.</div>
      </div>
    </div>
    ${sharedFields('openai')}
  </div>
</div>`;
```

**Note on duplicate `id="f-ollama-model"`:** The select and the text input both need that ID for `requireField()`. Use a single element: render only one depending on whether models were fetched. Simplify to always render the text input, and populate/clear it via `fetchOllamaModels()`:

```html
<!-- Replace the ollama model field with just a text input + fetch populates it -->
<input type="text" id="f-ollama-model" placeholder="e.g. qwen2.5:14b" value="...">
```

And update `fetchOllamaModels()` to set `document.getElementById('f-ollama-model').value` to the first model when models are returned.

### 5g: Update `loadExistingConfig()` JS function

Replace the openrouter field reads:
```js
if (cfg.openrouter_key)   state.openrouter_key   = cfg.openrouter_key;
if (cfg.model)            state.model            = cfg.model;
```
with:
```js
if (cfg.provider)     state.provider  = cfg.provider;
if (cfg.llm_key)      state.llm_key   = cfg.llm_key;
if (cfg.model)        state.model     = cfg.model;
if (cfg.base_url)     state.base_url  = cfg.base_url;
if (cfg.max_tokens)   state.max_tokens = String(cfg.max_tokens);
if (cfg.system_prompt) state.system_prompt = cfg.system_prompt;
if (cfg.location)     state.location  = cfg.location;
```

### 5h: Test the UI manually

**Step 1: Build and launch**

```bash
cargo build --release --bin setup && RUSTFOX_ROOT=. ./target/release/setup
```

**Step 2: Open browser at `http://localhost:8719`**

Verify:
- Step 3 shows three tabs: OpenRouter, Ollama (local), OpenAI
- Clicking each tab switches panels
- OpenRouter panel has API key + model text inputs
- Ollama panel has URL input + model text input + "Fetch models" button
- OpenAI panel has API key + model dropdown with the 5 curated models + "Custom…" option
- Clicking "Fetch models" with no local Ollama shows the error status
- Clicking Save → "Continue" generates a `[llm]` section in the preview

**Step 3: Commit**

```bash
git add setup/index.html
git commit -m "feat: add multi-provider UI to setup wizard (OpenRouter/Ollama/OpenAI tabs)"
```

---

## Task 6: Update `config.example.toml`

**Files:**
- Modify: `config.example.toml`

**Step 1: Replace `[openrouter]` block**

Replace:
```toml
[openrouter]
api_key = "YOUR_OPENROUTER_API_KEY"
model = "moonshotai/kimi-k2.5"
base_url = "https://openrouter.ai/api/v1"
max_tokens = 4096
system_prompt = """..."""
```

with:
```toml
# ── LLM Provider ────────────────────────────────────────────────────────────────
# Choose one provider. Uncomment the block you want; comment out the others.
#
# Option 1 – OpenRouter (cloud, many models)
# Get your API key at https://openrouter.ai/keys
[llm]
provider   = "openrouter"
model      = "moonshotai/kimi-k2.5"
base_url   = "https://openrouter.ai/api/v1"
api_key    = "YOUR_OPENROUTER_API_KEY"
max_tokens = 4096
system_prompt = """You are a helpful AI assistant with access to tools. \
Use the available tools to help the user with their tasks. \
When using file or terminal tools, operate only within the allowed sandbox directory. \
Be concise and helpful."""

# Option 2 – Ollama (local, no API key required)
# Install: https://ollama.com  then: ollama pull qwen2.5:14b
# [llm]
# provider   = "ollama"
# model      = "qwen2.5:14b"
# base_url   = "http://localhost:11434/v1"
# api_key    = ""
# max_tokens = 4096
# system_prompt = """You are a helpful AI assistant..."""

# Option 3 – OpenAI (cloud)
# Get your API key at https://platform.openai.com/api-keys
# [llm]
# provider   = "openai"
# model      = "gpt-4o"
# base_url   = "https://api.openai.com/v1"
# api_key    = "YOUR_OPENAI_API_KEY"
# max_tokens = 4096
# system_prompt = """You are a helpful AI assistant..."""
```

**Step 2: Commit**

```bash
git add config.example.toml
git commit -m "docs: update config.example.toml for multi-provider [llm] section"
```

---

## Task 7: Full Build & Test

**Step 1: Run all checks**

```bash
cargo fmt --all
cargo clippy -- -D warnings
cargo test
cargo build --release
```

Expected: All pass, zero warnings.

**Step 2: Fix any clippy warnings**

Common issues:
- Unused `default_base_url` or `default_model` functions → delete them
- Dead code warnings for removed fields → make sure `OpenRouterConfig` is fully removed from `config.rs`

**Step 3: Smoke-test the setup wizard**

```bash
RUSTFOX_ROOT=. ./target/release/setup
```

Go through the full wizard:
- Test all 3 provider tabs
- Verify generated TOML preview has `[llm]` with correct `provider`, `base_url`, `api_key`
- Save → check `config.toml` on disk

**Step 4: Smoke-test legacy config migration**

Create a file `test-legacy.toml` with:
```toml
[telegram]
bot_token = "test"
allowed_user_ids = [123]
[openrouter]
api_key = "sk-or-test"
model = "gpt-4o"
[sandbox]
allowed_directory = "/tmp"
```

Then:
```bash
cargo run -- test-legacy.toml
```

Expected: Bot starts, logs show `Provider: Openrouter` and `Model: gpt-4o`. No parse error.

Clean up: `rm test-legacy.toml`

**Step 5: Commit**

```bash
git add -u
git commit -m "fix: address clippy warnings and finalize multi-provider LLM support"
```

---

## Task 8: Push to Branch

```bash
git push -u origin claude/add-ollama-support-7ILGU
```

If push fails due to network error, retry with exponential backoff (2s, 4s, 8s, 16s).

---

## Summary of Files Changed

| File | Change |
|---|---|
| `src/config.rs` | Add `LlmProvider`, `LlmConfig`; remove `OpenRouterConfig`; add `effective_base_url()`; legacy `[openrouter]` fallback in `load()` |
| `src/llm.rs` | Use `LlmConfig`; conditional auth header; generic error messages |
| `src/agent.rs` | `config.openrouter` → `config.llm` (2 sites) |
| `src/main.rs` | `config.openrouter.model` → `config.llm` (1 log line) |
| `src/bin/setup.rs` | `GET /api/ollama-models`; `ExistingConfig`/`RawConfig` updated; `format_config` writes `[llm]`; CLI gets provider prompt; tests updated |
| `setup/index.html` | Step 3 rework: provider tabs, Ollama text+fetch, OpenAI curated dropdown, updated state/TOML generation |
| `config.example.toml` | `[openrouter]` → `[llm]` with all 3 provider variants |
