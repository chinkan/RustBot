# RustBot - Telegram AI Assistant

A Rust-based AI assistant that connects to Telegram and uses OpenRouter LLM (default: Qwen 3) with built-in tools and MCP server integration.

## Features

- **Telegram Bot** - Responds only to configured user IDs
- **OpenRouter LLM** - Configurable model (default: `qwen/qwen3-235b-a22b`)
- **Built-in Tools** - File read/write, directory listing, and command execution (sandboxed)
- **MCP Integration** - Connect any MCP-compatible server to extend capabilities
- **Agentic Loop** - Automatic multi-step tool calling until task completion
- **Per-user Conversations** - Independent conversation history per user

## Quick Start

### 1. Build

```bash
cargo build --release
```

### 2. Configure

Copy the example config and fill in your credentials:

```bash
cp config.example.toml config.toml
```

Edit `config.toml`:
- Set your Telegram bot token (from [@BotFather](https://t.me/BotFather))
- Set your OpenRouter API key (from [openrouter.ai/keys](https://openrouter.ai/keys))
- Add your Telegram user ID to `allowed_user_ids`
- Set the sandbox directory for file/command operations
- Optionally configure MCP servers

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
| `openrouter.model` | LLM model ID (e.g., `qwen/qwen3-235b-a22b`) |
| `sandbox.allowed_directory` | Directory for file/command operations |
| `embedding` (optional) | Vector search for memory |
| `mcp_servers` | List of MCP servers to connect |

### MCP Server Configuration

```toml
[[mcp_servers]]
name = "git"
command = "uvx"
args = ["mcp-server-git"]

[[mcp_servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/dir"]
```

## Built-in Tools

| Tool | Description |
|------|-------------|
| `read_file` | Read file contents within sandbox |
| `write_file` | Write/create files within sandbox |
| `list_files` | List directory contents within sandbox |
| `execute_command` | Run shell commands within sandbox directory |

## Bot Commands

| Command | Description |
|---------|-------------|
| `/start` | Show welcome message |
| `/clear` | Clear conversation history |
| `/tools` | List all available tools |

## Architecture

```
src/
├── main.rs      # Entry point, config loading, initialization
├── config.rs    # TOML configuration parsing
├── llm.rs       # OpenRouter API client with tool calling
├── tools.rs     # Built-in tools (file I/O, command execution)
├── mcp.rs       # MCP client manager for external tool servers
└── bot.rs       # Telegram bot handler with agentic loop
```

## Dependencies

- [teloxide](https://github.com/teloxide/teloxide) - Telegram bot framework
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) - Official MCP Rust SDK
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP client for OpenRouter
- [tokio](https://tokio.rs/) - Async runtime
