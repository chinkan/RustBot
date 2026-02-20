# Setup Wizard Web UI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a browser-based setup wizard that guides users through creating `config.toml`, launched via `setup.sh` (web by default, CLI with `--cli` flag).

**Architecture:** A shell entry point (`setup.sh`) either runs interactive CLI prompts or starts an Axum HTTP server (compiled Rust binary) that serves a single self-contained HTML wizard. The wizard collects all config values across 6 steps, generates valid TOML, and POSTs it to `/api/save-config` which writes `config.toml` to the project root then shuts the server down via a oneshot channel.

**Tech Stack:** Bash, Rust + Axum 0.8 + Tokio (new `setup` binary at `src/bin/setup.rs`), vanilla HTML/CSS/JS embedded via `include_str!` (no build step, no JS frameworks)

---

## File Map

```
setup.sh              ← entry point (builds + runs the Rust binary)
setup/
  index.html          ← self-contained wizard SPA (embedded in binary via include_str!)
src/bin/
  setup.rs            ← Axum HTTP server + CLI mode + unit tests
Cargo.toml            ← axum = "0.8" added
```

## MCP Catalog Reference

Used in Task 7. Each entry: `{ name, category, description, command, runner, args, envVars[] }`.

| name | category | runner | package/cmd | envVars |
|---|---|---|---|---|
| playwright | Browser & Web | npx | @playwright/mcp | — |
| brave-search | Browser & Web | npx | @brave/brave-search-mcp-server | BRAVE_API_KEY |
| firecrawl | Browser & Web | npx | firecrawl-mcp | FIRECRAWL_API_KEY |
| fetch | Browser & Web | uvx | mcp-server-fetch | — |
| google-workspace | Productivity | uvx | google-workspace-mcp | GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET |
| notion | Productivity | npx | @notionhq/notion-mcp-server | NOTION_API_KEY |
| obsidian | Productivity | npx | obsidian-mcp | — |
| slack | Communication | npx | @modelcontextprotocol/server-slack | SLACK_BOT_TOKEN |
| discord | Communication | npx | discord-mcp | DISCORD_TOKEN |
| github | Developer Tools | npx | @modelcontextprotocol/server-github | GITHUB_TOKEN |
| git | Developer Tools | uvx | mcp-server-git | — |
| filesystem | Developer Tools | npx | @modelcontextprotocol/server-filesystem | — |
| memory | Knowledge & Data | npx | @modelcontextprotocol/server-memory | — |
| tavily | Knowledge & Data | npx | tavily-mcp | TAVILY_API_KEY |
| context7 | Knowledge & Data | npx | @upstash/context7-mcp | — |
| open-meteo | Weather & Location | npx | open-meteo-mcp | — |
| openweathermap | Weather & Location | npx | openweathermap-mcp | OPENWEATHERMAP_API_KEY |

---

## Task 1: Shell entry point + Python server scaffold

**Files:**
- Create: `setup.sh`
- Create: `setup/server.py`

**Step 1: Create `setup/` directory**

```bash
mkdir -p setup
```

**Step 2: Write `setup.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT=8719

if [[ "${1:-}" == "--cli" ]]; then
  echo "CLI mode — not yet implemented"
  exit 0
fi

echo "Starting RustBot setup wizard at http://localhost:$PORT"
echo "Press Ctrl+C to exit."

python3 "$SCRIPT_DIR/setup/server.py" "$PORT" "$SCRIPT_DIR" &
SERVER_PID=$!

sleep 0.4
xdg-open "http://localhost:$PORT" 2>/dev/null \
  || open "http://localhost:$PORT" 2>/dev/null \
  || echo "Open http://localhost:$PORT in your browser."

wait "$SERVER_PID"
```

**Step 3: Make it executable**

```bash
chmod +x setup.sh
```

**Step 4: Write `setup/server.py`**

```python
#!/usr/bin/env python3
"""Minimal HTTP server for RustBot setup wizard."""
import http.server
import json
import os
import sys
import threading


PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8719
PROJECT_ROOT = sys.argv[2] if len(sys.argv) > 2 else os.path.dirname(os.path.dirname(__file__))
SETUP_DIR = os.path.dirname(os.path.abspath(__file__))
server_ref = None


class SetupHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass  # silence default request logs

    def do_GET(self):
        if self.path in ('/', '/index.html'):
            path = os.path.join(SETUP_DIR, 'index.html')
            with open(path, 'rb') as f:
                data = f.read()
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        if self.path == '/api/save-config':
            length = int(self.headers.get('Content-Length', 0))
            body = json.loads(self.rfile.read(length))
            config_path = os.path.join(PROJECT_ROOT, 'config.toml')
            with open(config_path, 'w') as f:
                f.write(body['config'])
            response = json.dumps({'ok': True, 'path': config_path}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(response)))
            self.end_headers()
            self.wfile.write(response)
            print(f'\n✓ config.toml saved to {config_path}')
            # shut down after short delay so response reaches browser
            threading.Timer(0.5, server_ref.shutdown).start()
        else:
            self.send_response(404)
            self.end_headers()


httpd = http.server.HTTPServer(('127.0.0.1', PORT), SetupHandler)
server_ref = httpd
print(f'Setup server running on http://localhost:{PORT}')
httpd.serve_forever()
```

**Step 5: Verify server starts**

```bash
python3 setup/server.py 8719 . &
sleep 0.3
curl -s http://localhost:8719/ | head -5   # should show 404 (no index.html yet) or connection
kill %1
```

Expected: server starts without error.

**Step 6: Commit**

```bash
git add setup.sh setup/server.py
git commit -m "feat: setup wizard scaffold — shell entry point and Python server"
```

---

## Task 2: HTML wizard shell — layout, step navigation, CSS

**Files:**
- Create: `setup/index.html`

This task creates the skeleton: header, step container, navigation buttons, CSS. No real step content yet — just placeholder `<div class="step">` blocks.

**Step 1: Write `setup/index.html` shell**

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>RustBot Setup</title>
<style>
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    background: #0f1117;
    color: #e2e8f0;
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
  }
  .card {
    background: #1a1f2e;
    border: 1px solid #2d3748;
    border-radius: 16px;
    width: 100%;
    max-width: 680px;
    padding: 2.5rem;
  }
  /* Progress bar */
  .progress-wrap { margin-bottom: 2rem; }
  .progress-label { font-size: 0.75rem; color: #718096; margin-bottom: 0.5rem; }
  .progress-bar { height: 4px; background: #2d3748; border-radius: 2px; }
  .progress-fill { height: 100%; background: #f6851b; border-radius: 2px; transition: width 0.3s ease; }
  /* Steps */
  .step { display: none; }
  .step.active { display: block; }
  h1 { font-size: 1.75rem; font-weight: 700; margin-bottom: 0.5rem; }
  h2 { font-size: 1.25rem; font-weight: 600; margin-bottom: 1.5rem; color: #cbd5e0; }
  p.subtitle { color: #718096; margin-bottom: 2rem; line-height: 1.6; }
  /* Form elements */
  .field { margin-bottom: 1.25rem; }
  label { display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.4rem; color: #cbd5e0; }
  label .hint { font-weight: 400; color: #718096; margin-left: 0.4rem; }
  input[type=text], input[type=password], textarea {
    width: 100%;
    background: #0f1117;
    border: 1px solid #2d3748;
    border-radius: 8px;
    color: #e2e8f0;
    padding: 0.6rem 0.875rem;
    font-size: 0.9rem;
    font-family: inherit;
    transition: border-color 0.2s;
  }
  input[type=text]:focus, input[type=password]:focus, textarea:focus {
    outline: none;
    border-color: #f6851b;
  }
  input.error, textarea.error { border-color: #fc8181; }
  .error-msg { color: #fc8181; font-size: 0.8rem; margin-top: 0.3rem; display: none; }
  .error-msg.visible { display: block; }
  textarea { resize: vertical; min-height: 90px; }
  /* Buttons */
  .btn-row { display: flex; gap: 0.75rem; justify-content: flex-end; margin-top: 2rem; }
  button {
    padding: 0.6rem 1.4rem;
    border-radius: 8px;
    border: none;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s;
  }
  button:hover { opacity: 0.85; }
  .btn-primary { background: #f6851b; color: #fff; }
  .btn-secondary { background: #2d3748; color: #e2e8f0; }
  /* MCP tools */
  .mcp-category { margin-bottom: 1.5rem; }
  .mcp-category h3 { font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.08em; color: #718096; margin-bottom: 0.75rem; }
  .mcp-tool {
    border: 1px solid #2d3748;
    border-radius: 8px;
    padding: 0.75rem 1rem;
    margin-bottom: 0.5rem;
    cursor: pointer;
    transition: border-color 0.2s;
  }
  .mcp-tool:hover { border-color: #4a5568; }
  .mcp-tool.selected { border-color: #f6851b; background: #1e2432; }
  .mcp-tool-header { display: flex; align-items: center; gap: 0.75rem; }
  .mcp-tool-header input[type=checkbox] { accent-color: #f6851b; width: 16px; height: 16px; cursor: pointer; }
  .mcp-tool-name { font-weight: 600; font-size: 0.9rem; }
  .mcp-tool-desc { font-size: 0.8rem; color: #718096; margin-left: 1.75rem; margin-top: 0.2rem; }
  .mcp-tool-cmd { font-size: 0.75rem; color: #4a5568; font-family: monospace; margin-left: 1.75rem; margin-top: 0.15rem; }
  .mcp-env-fields { margin-left: 1.75rem; margin-top: 0.75rem; display: none; }
  .mcp-env-fields.visible { display: block; }
  .mcp-env-fields input { margin-bottom: 0.4rem; font-family: monospace; font-size: 0.82rem; }
  .mcp-env-label { font-size: 0.75rem; color: #718096; margin-bottom: 0.2rem; }
  /* Config preview */
  pre {
    background: #0f1117;
    border: 1px solid #2d3748;
    border-radius: 8px;
    padding: 1rem;
    font-size: 0.78rem;
    overflow: auto;
    max-height: 300px;
    color: #a0aec0;
    white-space: pre;
  }
  .success-msg { color: #68d391; font-size: 0.9rem; margin-top: 1rem; display: none; }
  .success-msg.visible { display: block; }
</style>
</head>
<body>
<div class="card">
  <div class="progress-wrap">
    <div class="progress-label" id="progress-label">Step 1 of 6</div>
    <div class="progress-bar"><div class="progress-fill" id="progress-fill" style="width:16.6%"></div></div>
  </div>

  <!-- Steps injected by JS below -->
  <div id="steps-container"></div>

  <div class="btn-row">
    <button class="btn-secondary" id="btn-back" onclick="prevStep()" style="display:none">← Back</button>
    <button class="btn-primary" id="btn-next" onclick="nextStep()">Get Started →</button>
  </div>
</div>

<script>
// ── State ──────────────────────────────────────────────────────────────────────
const state = {
  telegram_token: '',
  allowed_user_ids: '',
  openrouter_key: '',
  model: 'qwen/qwen3-235b-a22b',
  max_tokens: '4096',
  system_prompt: `You are a helpful AI assistant with access to tools. Use the available tools to help the user with their tasks. When using file or terminal tools, operate only within the allowed sandbox directory. Be concise and helpful.`,
  location: '',
  sandbox_dir: '/tmp/rustbot-sandbox',
  db_path: 'rustbot.db',
  mcp_selections: {},   // { toolName: { selected: bool, env: { KEY: val } } }
};

// ── Step navigation ────────────────────────────────────────────────────────────
const TOTAL_STEPS = 6;
let currentStep = 1;

function showStep(n) {
  document.querySelectorAll('.step').forEach(s => s.classList.remove('active'));
  const el = document.getElementById('step-' + n);
  if (el) el.classList.add('active');
  currentStep = n;
  const pct = (n / TOTAL_STEPS * 100).toFixed(1);
  document.getElementById('progress-fill').style.width = pct + '%';
  document.getElementById('progress-label').textContent = `Step ${n} of ${TOTAL_STEPS}`;
  document.getElementById('btn-back').style.display = n > 1 ? 'inline-block' : 'none';
  const btnNext = document.getElementById('btn-next');
  if (n === 1) { btnNext.textContent = 'Get Started →'; }
  else if (n === TOTAL_STEPS) { btnNext.textContent = 'Save config.toml'; }
  else { btnNext.textContent = 'Continue →'; }
  if (n === TOTAL_STEPS) renderPreview();
}

function prevStep() { if (currentStep > 1) showStep(currentStep - 1); }

function nextStep() {
  if (!validateStep(currentStep)) return;
  collectStep(currentStep);
  if (currentStep === TOTAL_STEPS) { saveConfig(); return; }
  showStep(currentStep + 1);
}

// ── Validation ─────────────────────────────────────────────────────────────────
function validateStep(n) {
  let ok = true;
  if (n === 2) {
    ok = requireField('f-telegram-token') && ok;
    ok = requireField('f-allowed-ids') && ok;
  }
  if (n === 3) {
    ok = requireField('f-openrouter-key') && ok;
    ok = requireField('f-model') && ok;
  }
  return ok;
}

function requireField(id) {
  const el = document.getElementById(id);
  const errEl = document.getElementById(id + '-err');
  if (!el.value.trim()) {
    el.classList.add('error');
    if (errEl) errEl.classList.add('visible');
    return false;
  }
  el.classList.remove('error');
  if (errEl) errEl.classList.remove('visible');
  return true;
}

// ── Collect ────────────────────────────────────────────────────────────────────
function collectStep(n) {
  if (n === 2) {
    state.telegram_token = document.getElementById('f-telegram-token').value.trim();
    state.allowed_user_ids = document.getElementById('f-allowed-ids').value.trim();
  }
  if (n === 3) {
    state.openrouter_key = document.getElementById('f-openrouter-key').value.trim();
    state.model = document.getElementById('f-model').value.trim();
    state.max_tokens = document.getElementById('f-max-tokens').value.trim();
    state.system_prompt = document.getElementById('f-system-prompt').value;
    state.location = document.getElementById('f-location').value.trim();
  }
  if (n === 4) {
    state.sandbox_dir = document.getElementById('f-sandbox-dir').value.trim();
    state.db_path = document.getElementById('f-db-path').value.trim();
  }
}

// ── TOML generation ────────────────────────────────────────────────────────────
function generateToml() {
  const ids = state.allowed_user_ids.split(/[\s,]+/).filter(Boolean).join(', ');
  const loc = state.location ? `location = "${esc(state.location)}"` : `# location = "Your City, Country"`;
  const sysprompt = state.system_prompt.replace(/\\/g, '\\\\').replace(/"""/g, '\\"\\"\\"');

  let toml = `[telegram]
bot_token = "${esc(state.telegram_token)}"
allowed_user_ids = [${ids}]

[openrouter]
api_key = "${esc(state.openrouter_key)}"
model = "${esc(state.model)}"
base_url = "https://openrouter.ai/api/v1"
max_tokens = ${parseInt(state.max_tokens) || 4096}
system_prompt = """${sysprompt}"""
${loc}

[sandbox]
allowed_directory = "${esc(state.sandbox_dir)}"

[memory]
database_path = "${esc(state.db_path)}"

[skills]
directory = "skills"
`;

  // MCP servers
  for (const [name, sel] of Object.entries(state.mcp_selections)) {
    if (!sel.selected) continue;
    const tool = MCP_CATALOG.find(t => t.name === name);
    if (!tool) continue;
    const args = tool.args.map(a => `"${esc(a)}"`).join(', ');
    toml += `
[[mcp_servers]]
name = "${name}"
command = "${tool.runner}"
args = [${args}]`;
    if (tool.envVars && tool.envVars.length > 0) {
      toml += `\n[mcp_servers.env]`;
      for (const envKey of tool.envVars) {
        const val = sel.env?.[envKey] || '';
        toml += `\n${envKey} = "${esc(val)}"`;
      }
    }
    toml += '\n';
  }

  return toml;
}

function esc(s) { return String(s).replace(/\\/g, '\\\\').replace(/"/g, '\\"'); }

function renderPreview() {
  collectStep(4); // ensure sandbox fields are collected
  const pre = document.getElementById('config-preview');
  if (pre) pre.textContent = generateToml();
}

// ── Save ───────────────────────────────────────────────────────────────────────
async function saveConfig() {
  const config = generateToml();
  try {
    const res = await fetch('/api/save-config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ config })
    });
    const data = await res.json();
    if (data.ok) {
      const msg = document.getElementById('success-msg');
      if (msg) { msg.classList.add('visible'); }
      document.getElementById('btn-next').disabled = true;
    }
  } catch (e) {
    alert('Failed to save: ' + e.message);
  }
}

// ── MCP Catalog ────────────────────────────────────────────────────────────────
const MCP_CATALOG = [
  { name:'playwright',      category:'🌐 Browser & Web',       desc:'Browser automation & web scraping',             runner:'npx', args:['-y','@playwright/mcp'],                           envVars:[] },
  { name:'brave-search',    category:'🌐 Browser & Web',       desc:'Real-time web search',                          runner:'npx', args:['-y','@brave/brave-search-mcp-server'],               envVars:['BRAVE_API_KEY'] },
  { name:'firecrawl',       category:'🌐 Browser & Web',       desc:'URL → clean Markdown scraping',                 runner:'npx', args:['-y','firecrawl-mcp'],                             envVars:['FIRECRAWL_API_KEY'] },
  { name:'fetch',           category:'🌐 Browser & Web',       desc:'Lightweight page fetching',                     runner:'uvx', args:['mcp-server-fetch'],                               envVars:[] },
  { name:'google-workspace',category:'📁 Productivity',        desc:'Gmail, Calendar, Drive, Docs',                  runner:'uvx', args:['google-workspace-mcp'],                           envVars:['GOOGLE_CLIENT_ID','GOOGLE_CLIENT_SECRET'] },
  { name:'notion',          category:'📁 Productivity',        desc:'Read/write Notion workspace',                   runner:'npx', args:['-y','@notionhq/notion-mcp-server'],               envVars:['NOTION_API_KEY'] },
  { name:'obsidian',        category:'📁 Productivity',        desc:'Local Obsidian vault access',                   runner:'npx', args:['-y','obsidian-mcp'],                              envVars:[] },
  { name:'slack',           category:'💬 Communication',       desc:'Read channels, post messages',                  runner:'npx', args:['-y','@modelcontextprotocol/server-slack'],                      envVars:['SLACK_BOT_TOKEN'] },
  { name:'discord',         category:'💬 Communication',       desc:'Send/read Discord messages',                    runner:'npx', args:['-y','discord-mcp'],                               envVars:['DISCORD_TOKEN'] },
  { name:'github',          category:'🛠 Developer Tools',     desc:'Issues, PRs, repository management',            runner:'npx', args:['-y','@modelcontextprotocol/server-github'],                     envVars:['GITHUB_TOKEN'] },
  { name:'git',             category:'🛠 Developer Tools',     desc:'Local git repository operations',               runner:'uvx', args:['mcp-server-git'],                                 envVars:[] },
  { name:'filesystem',      category:'🛠 Developer Tools',     desc:'Expanded filesystem access',                    runner:'npx', args:['-y','@modelcontextprotocol/server-filesystem'],   envVars:[] },
  { name:'memory',          category:'🧠 Knowledge & Data',    desc:'Persistent cross-session memory graph',         runner:'npx', args:['-y','@modelcontextprotocol/server-memory'],                     envVars:[] },
  { name:'tavily',          category:'🧠 Knowledge & Data',    desc:'AI-optimised semantic search',                  runner:'npx', args:['-y','tavily-mcp'],                                envVars:['TAVILY_API_KEY'] },
  { name:'context7',        category:'🧠 Knowledge & Data',    desc:'Live library docs fetching',                    runner:'npx', args:['-y','@upstash/context7-mcp'],                     envVars:[] },
  { name:'open-meteo',      category:'🌤 Weather & Location',  desc:'Free weather forecasts — no API key needed',   runner:'npx', args:['-y','open-meteo-mcp'],                             envVars:[] },
  { name:'openweathermap',  category:'🌤 Weather & Location',  desc:'Current weather + forecasts',                   runner:'npx', args:['-y','openweathermap-mcp'],                        envVars:['OPENWEATHERMAP_API_KEY'] },
];

// ── Build step HTML ────────────────────────────────────────────────────────────
function buildSteps() {
  const container = document.getElementById('steps-container');

  // Step 1: Welcome
  container.innerHTML += `
<div class="step" id="step-1">
  <h1>Welcome to RustBot</h1>
  <p class="subtitle">Your personal AI assistant on Telegram, powered by OpenRouter LLMs and extensible via MCP tools.<br><br>This wizard will guide you through creating your <code>config.toml</code> in about 2 minutes.</p>
</div>`;

  // Step 2: Telegram
  container.innerHTML += `
<div class="step" id="step-2">
  <h2>Telegram Configuration</h2>
  <div class="field">
    <label>Bot Token <span class="hint">— from @BotFather</span></label>
    <input type="password" id="f-telegram-token" placeholder="1234567890:ABC-...">
    <div class="error-msg" id="f-telegram-token-err">Bot token is required.</div>
  </div>
  <div class="field">
    <label>Allowed User IDs <span class="hint">— comma-separated, from @userinfobot</span></label>
    <input type="text" id="f-allowed-ids" placeholder="123456789, 987654321">
    <div class="error-msg" id="f-allowed-ids-err">At least one user ID is required.</div>
  </div>
</div>`;

  // Step 3: OpenRouter
  container.innerHTML += `
<div class="step" id="step-3">
  <h2>OpenRouter Configuration</h2>
  <div class="field">
    <label>API Key <span class="hint">— openrouter.ai/keys</span></label>
    <input type="password" id="f-openrouter-key" placeholder="sk-or-...">
    <div class="error-msg" id="f-openrouter-key-err">API key is required.</div>
  </div>
  <div class="field">
    <label>Model <span class="hint">— openrouter.ai/models</span></label>
    <input type="text" id="f-model" value="qwen/qwen3-235b-a22b">
    <div class="error-msg" id="f-model-err">Model is required.</div>
  </div>
  <div class="field">
    <label>Max Tokens</label>
    <input type="text" id="f-max-tokens" value="4096">
  </div>
  <div class="field">
    <label>System Prompt</label>
    <textarea id="f-system-prompt">You are a helpful AI assistant with access to tools. Use the available tools to help the user with their tasks. When using file or terminal tools, operate only within the allowed sandbox directory. Be concise and helpful.</textarea>
  </div>
  <div class="field">
    <label>Location <span class="hint">— optional, injected into system prompt</span></label>
    <input type="text" id="f-location" placeholder="Tokyo, Japan">
  </div>
</div>`;

  // Step 4: Sandbox & Memory
  container.innerHTML += `
<div class="step" id="step-4">
  <h2>Sandbox &amp; Memory</h2>
  <div class="field">
    <label>Sandbox Directory <span class="hint">— bot's file/command scope</span></label>
    <input type="text" id="f-sandbox-dir" value="/tmp/rustbot-sandbox">
  </div>
  <div class="field">
    <label>Memory Database Path</label>
    <input type="text" id="f-db-path" value="rustbot.db">
  </div>
</div>`;

  // Step 5: MCP Tools
  const categories = [...new Set(MCP_CATALOG.map(t => t.category))];
  let mcpHtml = `<div class="step" id="step-5"><h2>MCP Tools <span class="hint" style="font-size:0.85rem;font-weight:400">— optional, all can be added later</span></h2>`;
  for (const cat of categories) {
    mcpHtml += `<div class="mcp-category"><h3>${cat}</h3>`;
    for (const tool of MCP_CATALOG.filter(t => t.category === cat)) {
      const cmd = `${tool.runner} ${tool.args.join(' ')}`;
      const envFields = tool.envVars.map(k =>
        `<div class="mcp-env-label">${k}</div><input type="text" placeholder="${k}" data-tool="${tool.name}" data-env="${k}" oninput="setEnv('${tool.name}','${k}',this.value)">`
      ).join('');
      mcpHtml += `
<div class="mcp-tool" id="mcp-wrap-${tool.name}" onclick="toggleTool('${tool.name}', event)">
  <div class="mcp-tool-header">
    <input type="checkbox" id="mcp-${tool.name}" onclick="event.stopPropagation();toggleTool('${tool.name}', event)">
    <span class="mcp-tool-name">${tool.name}</span>
  </div>
  <div class="mcp-tool-desc">${tool.desc}</div>
  <div class="mcp-tool-cmd">${cmd}</div>
  ${tool.envVars.length ? `<div class="mcp-env-fields" id="mcp-env-${tool.name}">${envFields}</div>` : ''}
</div>`;
    }
    mcpHtml += `</div>`;
  }
  mcpHtml += `</div>`;
  container.innerHTML += mcpHtml;

  // Step 6: Review & Save
  container.innerHTML += `
<div class="step" id="step-6">
  <h2>Review &amp; Save</h2>
  <p style="color:#718096;margin-bottom:1rem;font-size:0.85rem;">Generated <code>config.toml</code> — will be saved to the project root.</p>
  <pre id="config-preview"></pre>
  <div class="success-msg" id="success-msg">✓ config.toml saved! You can now run: <code>cargo run</code></div>
</div>`;
}

// ── MCP toggle ─────────────────────────────────────────────────────────────────
function toggleTool(name, event) {
  const cb = document.getElementById('mcp-' + name);
  if (event.target !== cb) cb.checked = !cb.checked;
  const wrap = document.getElementById('mcp-wrap-' + name);
  const envDiv = document.getElementById('mcp-env-' + name);
  if (cb.checked) {
    wrap.classList.add('selected');
    if (envDiv) envDiv.classList.add('visible');
    if (!state.mcp_selections[name]) state.mcp_selections[name] = { selected: true, env: {} };
    state.mcp_selections[name].selected = true;
  } else {
    wrap.classList.remove('selected');
    if (envDiv) envDiv.classList.remove('visible');
    if (state.mcp_selections[name]) state.mcp_selections[name].selected = false;
  }
}

function setEnv(tool, key, val) {
  if (!state.mcp_selections[tool]) state.mcp_selections[tool] = { selected: true, env: {} };
  state.mcp_selections[tool].env[key] = val;
}

// ── Init ───────────────────────────────────────────────────────────────────────
buildSteps();
showStep(1);
</script>
</body>
</html>
```

**Step 2: Start server and load wizard in browser**

```bash
python3 setup/server.py 8719 . &
sleep 0.3
curl -s http://localhost:8719/ | grep -c "<html"  # should print 1
kill %1
```

Expected: `1`

**Step 3: Commit**

```bash
git add setup/index.html
git commit -m "feat: setup wizard HTML — 6-step UI with MCP catalog"
```

---

## Task 3: CLI mode in `setup.sh`

**Files:**
- Modify: `setup.sh`

**Step 1: Replace CLI stub with interactive prompts**

Replace the `--cli` block in `setup.sh`:

```bash
if [[ "${1:-}" == "--cli" ]]; then
  echo "=== RustBot CLI Setup ==="
  echo ""

  read -rp "Telegram bot token: " TG_TOKEN
  read -rp "Allowed user IDs (comma-separated): " USER_IDS
  read -rp "OpenRouter API key: " OR_KEY
  read -rp "Model [qwen/qwen3-235b-a22b]: " OR_MODEL
  OR_MODEL="${OR_MODEL:-qwen/qwen3-235b-a22b}"
  read -rp "Sandbox directory [/tmp/rustbot-sandbox]: " SANDBOX
  SANDBOX="${SANDBOX:-/tmp/rustbot-sandbox}"
  read -rp "Memory DB path [rustbot.db]: " DBPATH
  DBPATH="${DBPATH:-rustbot.db}"
  read -rp "Your location (optional, e.g. Tokyo, Japan): " LOCATION

  IDS_ARR=$(echo "$USER_IDS" | tr ',' '\n' | tr -d ' ' | paste -sd ', ')

  LOC_LINE="# location = \"Your City, Country\""
  if [[ -n "$LOCATION" ]]; then
    LOC_LINE="location = \"$LOCATION\""
  fi

  CONFIG_PATH="$SCRIPT_DIR/config.toml"
  cat > "$CONFIG_PATH" <<TOML
[telegram]
bot_token = "$TG_TOKEN"
allowed_user_ids = [$IDS_ARR]

[openrouter]
api_key = "$OR_KEY"
model = "$OR_MODEL"
base_url = "https://openrouter.ai/api/v1"
max_tokens = 4096
system_prompt = """You are a helpful AI assistant with access to tools. Use the available tools to help the user with their tasks. When using file or terminal tools, operate only within the allowed sandbox directory. Be concise and helpful."""
$LOC_LINE

[sandbox]
allowed_directory = "$SANDBOX"

[memory]
database_path = "$DBPATH"

[skills]
directory = "skills"
TOML

  echo ""
  echo "✓ config.toml saved to $CONFIG_PATH"
  echo "  Run: cargo run"
  exit 0
fi
```

**Step 2: Test CLI mode (dry run)**

```bash
echo -e "mytoken\n123456\nmykey\n\n\n\n" | bash setup.sh --cli
cat config.toml | grep bot_token  # should show mytoken
rm config.toml
```

Expected: `bot_token = "mytoken"`

**Step 3: Commit**

```bash
git add setup.sh
git commit -m "feat: CLI mode for setup.sh via --cli flag"
```

---

## Task 4: Integration test + final polish

**Files:**
- Modify: `setup/server.py` (add `/api/test` health endpoint)
- Create: `setup/test_server.py`

**Step 1: Add health endpoint to server**

In `SetupHandler.do_GET`, add:
```python
elif self.path == '/api/health':
    self.send_response(200)
    self.send_header('Content-Type', 'application/json')
    self.end_headers()
    self.wfile.write(b'{"ok":true}')
```

**Step 2: Write `setup/test_server.py`**

```python
#!/usr/bin/env python3
"""Integration tests for setup server."""
import json, os, subprocess, sys, time, urllib.request, tempfile

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PORT = 8720  # use different port to avoid conflicts


def start_server():
    proc = subprocess.Popen(
        [sys.executable, os.path.join(PROJECT_ROOT, 'setup', 'server.py'), str(PORT), PROJECT_ROOT],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    time.sleep(0.5)
    return proc


def test_health(proc):
    url = f'http://127.0.0.1:{PORT}/api/health'
    r = urllib.request.urlopen(url)
    data = json.loads(r.read())
    assert data == {'ok': True}, f"Expected ok, got {data}"
    print('✓ health endpoint')


def test_serve_html(proc):
    url = f'http://127.0.0.1:{PORT}/'
    r = urllib.request.urlopen(url)
    html = r.read().decode()
    assert '<html' in html, "Response should contain HTML"
    print('✓ serves index.html')


def test_save_config(proc):
    config_path = os.path.join(PROJECT_ROOT, 'config.toml')
    # Remove if exists
    if os.path.exists(config_path): os.remove(config_path)

    payload = json.dumps({'config': '[telegram]\nbot_token = "test"\n'}).encode()
    req = urllib.request.Request(
        f'http://127.0.0.1:{PORT}/api/save-config',
        data=payload,
        headers={'Content-Type': 'application/json'}
    )
    r = urllib.request.urlopen(req)
    data = json.loads(r.read())
    assert data['ok'] is True
    assert os.path.exists(config_path)
    content = open(config_path).read()
    assert 'bot_token = "test"' in content
    os.remove(config_path)
    print('✓ save-config writes config.toml')


if __name__ == '__main__':
    proc = start_server()
    try:
        test_health(proc)
        test_serve_html(proc)
        test_save_config(proc)
        print('\nAll tests passed.')
    finally:
        proc.terminate()
```

**Step 3: Run tests**

```bash
python3 setup/test_server.py
```

Expected:
```
✓ health endpoint
✓ serves index.html
✓ save-config writes config.toml

All tests passed.
```

**Step 4: Commit**

```bash
git add setup/server.py setup/test_server.py
git commit -m "test: integration tests for setup server + health endpoint"
```

---

## Task 5: Push

**Step 1: Push branch**

```bash
git push -u origin claude/setup-script-web-ui-Q76AI
```

Expected: branch pushed, no errors.

---

## Quick sanity check before each commit

```bash
# Ensure no config.toml was accidentally staged
git status | grep config.toml  # should print nothing
```
