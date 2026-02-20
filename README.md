# RustBot — Telegram AI Assistant

[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Buy Me a Coffee](https://img.shields.io/badge/buy%20me%20a%20coffee-%E2%98%95-yellow)](https://buymeacoffee.com/chinkan.ai)

A Rust-based Telegram AI assistant powered by OpenRouter LLM (default: `moonshotai/kimi-k2.5`) with built-in sandboxed tools, scheduling, persistent memory, and MCP server integration.

## Features

- **Telegram Bot** — Responds only to configured user IDs
- **OpenRouter LLM** — Configurable model (default: `moonshotai/kimi-k2.5`)
- **Built-in Tools** — File read/write, directory listing, command execution (sandboxed)
- **Scheduling Tools** — Schedule, list, and cancel recurring or one-shot tasks
- **Persistent Memory** — SQLite-backed conversation history and knowledge base
- **Vector Embedding Search** — Hybrid vector + FTS5 search using `qwen/qwen3-embedding-8b`
- **MCP Integration** — Connect any MCP-compatible server to extend capabilities
- **Bot Skills** — Folder-based natural-language skill instructions auto-loaded at startup
- **Agentic Loop** — Automatic multi-step tool calling until task completion (max 10 iterations)
- **Per-user Conversations** — Independent conversation history per user

## Quick Start

### 1. Build

```bash
cargo build --release
```

### 2. Configure

Run the setup wizard — it guides you through all required fields and writes `config.toml` for you:

```bash
# Browser-based wizard (recommended)
./setup.sh

# Terminal wizard (no browser required)
./setup.sh --cli
```

The wizard will ask for your:
- Telegram bot token (from [@BotFather](https://t.me/BotFather))
- Allowed Telegram user IDs (from [@userinfobot](https://t.me/userinfobot))
- OpenRouter API key (from [openrouter.ai/keys](https://openrouter.ai/keys))
- Sandbox directory, model, and optional MCP tools

> **Manual setup:** Copy `config.example.toml` to `config.toml` and edit it directly if you prefer.

### 3. Run

```bash
cargo run
# or with a custom config path:
cargo run -- /path/to/config.toml
```

## Configuration

See [`config.example.toml`](config.example.toml) for all options.

### Key Settings

| Setting | Description |
|---------|-------------|
| `telegram.bot_token` | Telegram Bot API token |
| `telegram.allowed_user_ids` | List of user IDs allowed to use the bot |
| `openrouter.api_key` | OpenRouter API key |
| `openrouter.model` | LLM model ID (default: `moonshotai/kimi-k2.5`) |
| `sandbox.allowed_directory` | Directory for file/command operations |
| `memory.database_path` | SQLite DB path (default: `rustbot.db`) |
| `embedding` (optional) | Vector search API config (default model: `qwen/qwen3-embedding-8b`) |
| `skills.directory` | Folder of bot skill files (default: `skills/`) |
| `mcp_servers` | List of MCP servers to connect |
| `location` | Your location string, injected into system prompt |

### MCP Server Configuration

RustBot supports the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) — an open standard for connecting AI assistants to external tools and data sources. Any MCP-compatible server can be plugged in via `config.toml`.

#### Prerequisites

MCP servers are usually distributed as Python packages (run via `uvx`) or npm packages (run via `npx`).

| Runtime | Install |
|---------|---------|
| `uvx` (Python) | [Install uv](https://docs.astral.sh/uv/getting-started/installation/) — `curl -LsSf https://astral.sh/uv/install.sh \| sh` |
| `npx` (Node.js) | [Install Node.js](https://nodejs.org/) — comes bundled with npm/npx |

#### Config Syntax

Add one `[[mcp_servers]]` block per server in `config.toml`:

```toml
[[mcp_servers]]
name   = "server-name"   # used to namespace tools: mcp_<name>_<tool>
command = "uvx"          # or "npx", or any executable on PATH
args   = ["package-name", "optional-arg"]

# Optional: pass environment variables to the server process
# [mcp_servers.env]
# API_KEY = "your-key-here"
```

#### Popular MCP Servers

| Server | Package | Runtime | Notes |
|--------|---------|---------|-------|
| [Git](https://github.com/modelcontextprotocol/servers/tree/main/src/git) | `mcp-server-git` | `uvx` | Read/search git repos |
| [Filesystem](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem) | `@modelcontextprotocol/server-filesystem` | `npx` | File access outside the sandbox |
| [Brave Search](https://github.com/modelcontextprotocol/servers/tree/main/src/brave-search) | `@anthropic/mcp-brave-search` | `npx` | Web search (needs [Brave API key](https://brave.com/search/api/)) |
| [GitHub](https://github.com/modelcontextprotocol/servers/tree/main/src/github) | `@modelcontextprotocol/server-github` | `npx` | Issues, PRs, repos |
| [Fetch](https://github.com/modelcontextprotocol/servers/tree/main/src/fetch) | `mcp-server-fetch` | `uvx` | HTTP fetch / web scraping |
| [SQLite](https://github.com/modelcontextprotocol/servers/tree/main/src/sqlite) | `mcp-server-sqlite` | `uvx` | Query local SQLite databases |
| [Puppeteer](https://github.com/modelcontextprotocol/servers/tree/main/src/puppeteer) | `@modelcontextprotocol/server-puppeteer` | `npx` | Browser automation |

> Find more servers at the [MCP server registry](https://github.com/modelcontextprotocol/servers) and [mcp.so](https://mcp.so/).

#### Examples

```toml
# Git — inspect repositories
[[mcp_servers]]
name    = "git"
command = "uvx"
args    = ["mcp-server-git"]

# Filesystem — expose an extra directory to the bot
[[mcp_servers]]
name    = "filesystem"
command = "npx"
args    = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/dir"]

# Brave Search — web search (requires API key)
[[mcp_servers]]
name    = "brave-search"
command = "npx"
args    = ["-y", "@anthropic/mcp-brave-search"]
[mcp_servers.env]
BRAVE_API_KEY = "your-brave-api-key"
```

#### Tool Naming

Tools from MCP servers are automatically namespaced as `mcp_<server-name>_<tool-name>` (e.g. `mcp_git_git_log`). Run `/tools` in the bot to see all registered tools after startup.

## Built-in Tools

### Core Tools

| Tool | Description |
|------|-------------|
| `read_file` | Read file contents within sandbox |
| `write_file` | Write/create files within sandbox |
| `list_files` | List directory contents within sandbox |
| `execute_command` | Run shell commands within sandbox directory |

### Scheduling Tools

| Tool | Description |
|------|-------------|
| `schedule_task` | Schedule a recurring (cron) or one-shot task with a message |
| `list_scheduled_tasks` | List all active scheduled tasks |
| `cancel_scheduled_task` | Cancel a scheduled task by ID |

## Bot Commands

| Command | Description |
|---------|-------------|
| `/start` | Show welcome message |
| `/clear` | Clear conversation history |
| `/tools` | List all available tools |

## Architecture

```
src/
├── main.rs           # Entry point, config loading, initialization
├── config.rs         # TOML configuration parsing
├── llm.rs            # OpenRouter API client with tool calling
├── agent.rs          # Agentic loop, tool dispatch, scheduling tools
├── tools.rs          # Built-in tools (file I/O, command execution)
├── mcp.rs            # MCP client manager for external tool servers
├── memory/           # SQLite persistence, vector embeddings
├── scheduler/        # Cron/one-shot task scheduler with DB persistence
├── skills/           # Skill loader (auto-loads from skills/ directory)
└── platform/         # Telegram bot handler
```

## Roadmap

### Done

- [x] Telegram bot with user allowlist
- [x] OpenRouter LLM integration with tool calling (agentic loop)
- [x] Built-in sandboxed tools (file read/write, directory listing, command execution)
- [x] MCP server integration for extensible tooling
- [x] Per-user conversation history
- [x] Persistent memory with SQLite
- [x] Vector embedding search (`qwen/qwen3-embedding-8b`)
- [x] Scheduling tools (`schedule_task`, `list_scheduled_tasks`, `cancel_scheduled_task`)
- [x] Bot skills (folder-based, auto-loaded at startup)

### Planned

- [ ] Image upload support
- [ ] Google integration tools (Calendar, Email, Drive)
- [ ] Event trigger framework (e.g., on email receive)
- [x] Setup wizard (web UI + CLI) for guided `config.toml` creation
- [ ] WhatsApp support
- [ ] Webhook mode (in addition to polling)
- [ ] And more…

## Contributing

This project is open source under the [MIT License](LICENSE). Contributions are very welcome!

Feel free to:
- Open an issue for bugs or feature requests
- Submit a pull request — all PRs are appreciated

## Support

If you find RustBot useful, consider supporting the project:

[![Buy Me a Coffee](https://img.shields.io/badge/Buy%20Me%20a%20Coffee-%E2%98%95-yellow?style=for-the-badge&logo=buy-me-a-coffee)](https://buymeacoffee.com/chinkan.ai)

[![GitHub Sponsors](https://img.shields.io/badge/GitHub%20Sponsors-%E2%9D%A4-pink?style=for-the-badge&logo=github)](https://github.com/sponsors/chinkan)

## Dependencies

- [teloxide](https://github.com/teloxide/teloxide) — Telegram bot framework
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) — Official MCP Rust SDK
- [reqwest](https://github.com/seanmonstar/reqwest) — HTTP client for OpenRouter
- [tokio](https://tokio.rs/) — Async runtime
- [tokio-cron-scheduler](https://github.com/mvniekerk/tokio-cron-scheduler) — Task scheduling
