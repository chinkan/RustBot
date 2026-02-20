//! RustBot setup wizard.
//!
//! Without flags: starts a local Axum HTTP server on port 8719 and opens the
//! browser-based setup wizard.  On form submission the wizard POSTs the
//! generated config to `/api/save-config`, which writes `config.toml` to the
//! project root and then shuts the server down.
//!
//! With `--cli`: runs an interactive terminal wizard instead.

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{oneshot, Mutex};

// The wizard SPA is embedded at compile time — no runtime file needed.
const INDEX_HTML: &str = include_str!("../../setup/index.html");

// ── Shared state ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    config_path: PathBuf,
    /// Consumed once when the browser POSTs a saved config.
    shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

// ── Request / response types ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct SaveRequest {
    config: String,
}

#[derive(Serialize)]
struct SaveResponse {
    ok: bool,
    path: String,
}

// ── Load-config response ────────────────────────────────────────────────────

#[derive(Serialize, Default)]
struct ExistingConfig {
    exists: bool,
    telegram_token: String,
    allowed_user_ids: String,   // "123, 456" — ready for the text input
    openrouter_key: String,
    model: String,
    max_tokens: u32,        // 0 = not set; frontend treats falsy as "use wizard default (4096)"
    system_prompt: String,
    location: String,
    sandbox_dir: String,
    db_path: String,
    mcp_servers: Vec<ExistingMcpServer>,
}

#[derive(Serialize, Default, Clone)]
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

// ── Handlers ───────────────────────────────────────────────────────────────────

async fn serve_index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn save_config(
    State(state): State<AppState>,
    Json(body): Json<SaveRequest>,
) -> Result<Json<SaveResponse>, StatusCode> {
    tokio::fs::write(&state.config_path, &body.config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let path = state.config_path.to_string_lossy().to_string();
    println!("\n✓  config.toml saved to {path}");
    println!("   Run the bot with:  cargo run\n");

    // Signal main to shut down after the response has been sent.
    let tx = state.shutdown_tx.lock().await.take();
    if let Some(tx) = tx {
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
            let _ = tx.send(());
        });
    }

    Ok(Json(SaveResponse { ok: true, path }))
}

// ── Config formatting ──────────────────────────────────────────────────────────

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

/// Produces a valid config.toml string. Extracted so it can be unit-tested.
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
    let or_key = p.or_key;
    let model = p.model;
    let max_tokens = p.max_tokens;
    let sandbox = p.sandbox;
    let db_path = p.db_path;

    format!(
        r#"[telegram]
bot_token = "{tg_token}"
allowed_user_ids = [{ids_str}]

[openrouter]
api_key = "{or_key}"
model = "{model}"
base_url = "https://openrouter.ai/api/v1"
max_tokens = {max_tokens}
system_prompt = """You are a helpful AI assistant with access to tools. \
Use the available tools to help the user with their tasks. \
When using file or terminal tools, operate only within the allowed sandbox directory. \
Be concise and helpful."""
{loc_line}

[sandbox]
allowed_directory = "{sandbox}"

[memory]
database_path = "{db_path}"

[skills]
directory = "skills"
"#
    )
}

// ── CLI mode ───────────────────────────────────────────────────────────────────

fn run_cli(project_root: &Path) -> Result<()> {
    use std::io::{self, Write};

    println!("=== RustBot CLI Setup ===\n");

    let read_line = |prompt: &str| -> Result<String> {
        print!("{prompt}");
        io::stdout().flush()?;
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        Ok(buf.trim().to_owned())
    };

    let or_default = |s: String, default: &str| {
        if s.is_empty() {
            default.to_owned()
        } else {
            s
        }
    };

    let tg_token = read_line("Telegram bot token: ")?;
    let user_ids = read_line("Allowed user IDs (comma-separated): ")?;
    let or_key = read_line("OpenRouter API key: ")?;
    let model = or_default(
        read_line("Model [qwen/qwen3-235b-a22b]: ")?,
        "qwen/qwen3-235b-a22b",
    );
    let sandbox = or_default(
        read_line("Sandbox directory [/tmp/rustbot-sandbox]: ")?,
        "/tmp/rustbot-sandbox",
    );
    let db_path = or_default(read_line("Memory DB path [rustbot.db]: ")?, "rustbot.db");
    let location = read_line("Your location (optional, e.g. Tokyo, Japan): ")?;

    let config = format_config(&ConfigParams {
        tg_token: &tg_token,
        user_ids: &user_ids,
        or_key: &or_key,
        model: &model,
        max_tokens: 4096,
        sandbox: &sandbox,
        db_path: &db_path,
        location: &location,
    });

    let config_path = project_root.join("config.toml");
    std::fs::write(&config_path, &config)
        .with_context(|| format!("Could not write {}", config_path.display()))?;

    println!("\n✓  config.toml saved to {}", config_path.display());
    println!("   Run the bot with:  cargo run");
    Ok(())
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Resolve project root: prefer RUSTBOT_ROOT env, fall back to cwd.
    let project_root =
        PathBuf::from(std::env::var("RUSTBOT_ROOT").unwrap_or_else(|_| ".".to_string()));

    if args.iter().any(|a| a == "--cli") {
        return run_cli(&project_root);
    }

    // ── Web mode ──────────────────────────────────────────────────────────────
    let port: u16 = 8719;
    let config_path = project_root.join("config.toml");

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let state = AppState {
        config_path,
        shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
    };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/save-config", post(save_config))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    println!("RustBot setup wizard → http://localhost:{port}");
    println!("Press Ctrl-C to exit without saving.\n");

    // Open the browser after a short delay.
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
        let url = format!("http://localhost:{port}");
        // Try xdg-open (Linux), then open (macOS) — ignore errors.
        let _ = std::process::Command::new("xdg-open").arg(&url).status();
        let _ = std::process::Command::new("open").arg(&url).status();
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        })
        .await
        .context("Server error")?;

    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(
        tg_token: &str,
        user_ids: &str,
        or_key: &str,
        model: &str,
        sandbox: &str,
        db_path: &str,
        location: &str,
    ) -> String {
        format_config(&ConfigParams {
            tg_token,
            user_ids,
            or_key,
            model,
            max_tokens: 4096,
            sandbox,
            db_path,
            location,
        })
    }

    #[test]
    fn test_telegram_section_present() {
        let out = cfg("mytoken", "123456", "key", "gpt-4o", "/tmp", "db.db", "");
        assert!(out.contains("[telegram]"));
        assert!(out.contains(r#"bot_token = "mytoken""#));
        assert!(out.contains("allowed_user_ids = [123456]"));
    }

    #[test]
    fn test_openrouter_section_present() {
        let out = cfg("t", "1", "sk-or-abc", "gpt-4o", "/tmp", "db.db", "");
        assert!(out.contains("[openrouter]"));
        assert!(out.contains(r#"api_key = "sk-or-abc""#));
        assert!(out.contains(r#"model = "gpt-4o""#));
        assert!(out.contains("max_tokens = 4096"));
    }

    #[test]
    fn test_location_included_when_set() {
        let out = cfg("t", "1", "k", "m", "/tmp", "db.db", "Tokyo, Japan");
        assert!(out.contains(r#"location = "Tokyo, Japan""#));
    }

    #[test]
    fn test_location_commented_when_empty() {
        let out = cfg("t", "1", "k", "m", "/tmp", "db.db", "");
        assert!(out.contains("# location ="));
        assert!(!out.contains("\nlocation = "));
    }

    #[test]
    fn test_multiple_user_ids_comma_separated() {
        let out = cfg("t", "111, 222, 333", "k", "m", "/tmp", "db.db", "");
        assert!(out.contains("allowed_user_ids = [111, 222, 333]"));
    }

    #[test]
    fn test_sandbox_and_memory_sections() {
        let out = cfg("t", "1", "k", "m", "/my/sandbox", "mem.db", "");
        assert!(out.contains("[sandbox]"));
        assert!(out.contains(r#"allowed_directory = "/my/sandbox""#));
        assert!(out.contains("[memory]"));
        assert!(out.contains(r#"database_path = "mem.db""#));
    }

    #[test]
    fn test_skills_section_present() {
        let out = cfg("t", "1", "k", "m", "/tmp", "db.db", "");
        assert!(out.contains("[skills]"));
        assert!(out.contains(r#"directory = "skills""#));
    }
}
