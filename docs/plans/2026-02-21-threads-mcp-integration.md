# Threads MCP Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Meta Threads publishing support to RustFox via `baguskto/threads-mcp`, with a setup-wizard catalog entry, a guideline popup modal (matching the Google Workspace pattern), a `config.example.toml` example, and a README update.

**Architecture:** Three static files only — `setup/index.html` (catalog + modal + JS), `config.example.toml` (commented example block), `README.md` (table row + TOML snippet). No Rust source changes are needed because `mcp.rs` handles any stdio-based MCP server generically.

**Tech Stack:** Vanilla HTML/CSS/JavaScript (single-file setup wizard), TOML, Markdown.

---

### Task 1: Add Threads entry to MCP_CATALOG in `setup/index.html`

**Files:**
- Modify: `setup/index.html` (the `MCP_CATALOG` array, starting at line 423)

The catalog is a JS array. Each entry follows this shape:
```js
{ name:'...', category:'...', desc:'...', runner:'npx'|'uvx', args:[...], envVars:[...], setupGuide:'...', link:'...' }
```

**Step 1: Open the file and locate the end of MCP_CATALOG**

The array ends at line 441 with:
```js
  { name:'openweathermap', category:'Weather & Location', ... },
];
```

**Step 2: Insert the Threads entry as the last item before `];`**

Add this line immediately before the closing `];` of `MCP_CATALOG`:

```js
  { name:'threads', category:'Social Media', desc:'Publish and manage Meta Threads posts', runner:'npx', args:['-y','threads-mcp-server'], envVars:['THREADS_ACCESS_TOKEN'], setupGuide:'__THREADS_GUIDE_BUTTON__', link:'https://github.com/baguskto/threads-mcp' },
```

**Step 3: Verify the catalog renders correctly**

Open `setup/index.html` in a browser (or run `./setup.sh`), navigate to Step 5 (MCP Tools). A new **"Social Media"** category section should appear at the bottom with a **threads** card.

**Step 4: Commit**

```bash
git add setup/index.html
git commit -m "feat: add threads MCP catalog entry to setup wizard"
```

---

### Task 2: Add Threads modal HTML to `setup/index.html`

**Files:**
- Modify: `setup/index.html` (after the existing `#oauth-modal` div, around line 419)

The existing Google Workspace modal (`#oauth-modal`) ends at line 419:
```html
</div>
</div>

<script>
```

**Step 1: Insert the Threads modal HTML between the closing `</div>` of `#oauth-modal` and `<script>`**

```html
<!-- Threads API Setup Modal -->
<div class="modal-overlay" id="threads-modal" onclick="closeThreadsModal(event)">
  <div class="modal-box" role="dialog" aria-modal="true" aria-labelledby="threads-modal-title">
    <button class="modal-close" onclick="closeThreadsModal()" aria-label="Close">&#x2715;</button>
    <h3 id="threads-modal-title">Meta Threads — Access Token Setup</h3>
    <p class="modal-subtitle">Follow these 4 steps to get your long-lived access token. You only need to do this once.</p>
    <ol class="oauth-steps">
      <li class="oauth-step">
        <div class="oauth-step-num">1</div>
        <div class="oauth-step-body">
          <div class="oauth-step-title">Create a Meta Developer App</div>
          <div class="oauth-step-desc">
            Go to <a href="https://developers.facebook.com/apps" target="_blank" rel="noopener">developers.facebook.com/apps</a>
            and click <strong>Create App</strong>.<br>
            Choose the <strong>Business</strong> app type, give it a name, and complete the creation flow.
          </div>
        </div>
      </li>
      <li class="oauth-step">
        <div class="oauth-step-num">2</div>
        <div class="oauth-step-body">
          <div class="oauth-step-title">Add the Threads API Product</div>
          <div class="oauth-step-desc">
            In your app dashboard, click <strong>Add Product</strong> and find <strong>Threads API</strong>.<br>
            Click <strong>Set Up</strong> to add it to your app.
          </div>
        </div>
      </li>
      <li class="oauth-step">
        <div class="oauth-step-num">3</div>
        <div class="oauth-step-body">
          <div class="oauth-step-title">Configure Permissions</div>
          <div class="oauth-step-desc">
            Under <strong>Threads API → Permissions</strong>, request all four permissions:
            <ul class="scope-list">
              <li>threads_basic</li>
              <li>threads_content_publish</li>
              <li>threads_manage_replies</li>
              <li>threads_read_replies</li>
            </ul>
            Add your Instagram/Threads account as a <strong>Test User</strong> under
            <strong>App Roles → Roles</strong> so you can generate a token before app review.
          </div>
        </div>
      </li>
      <li class="oauth-step">
        <div class="oauth-step-num">4</div>
        <div class="oauth-step-body">
          <div class="oauth-step-title">Generate a Long-Lived Access Token</div>
          <div class="oauth-step-desc">
            Under <strong>Threads API → Access Tokens</strong>, click <strong>Generate Token</strong>
            for your test user account.<br>
            Copy the token — it is valid for approximately <strong>60 days</strong>.<br>
            Paste it into the <code>THREADS_ACCESS_TOKEN</code> field below.<br>
            To refresh it before expiry, use the
            <a href="https://developers.facebook.com/tools/explorer/" target="_blank" rel="noopener">Graph API Explorer</a>
            or call the token-refresh endpoint documented in the Threads API docs.
          </div>
        </div>
      </li>
    </ol>
    <div class="oauth-warning">
      <strong>Missing permissions cause publishing errors.</strong>
      If you see "permissions error" when posting, ensure all four permissions above are approved for your test user and regenerate the token.
    </div>
  </div>
</div>
```

**Step 2: Verify modal HTML is well-formed**

Open the page in a browser. No visual change yet (the modal is hidden). Check the browser console for HTML parse errors — there should be none.

**Step 3: Commit**

```bash
git add setup/index.html
git commit -m "feat: add Threads API setup guide modal HTML"
```

---

### Task 3: Add Threads modal JS and wire up the guide button

**Files:**
- Modify: `setup/index.html` (two JS locations)

**Step 1: Add `openThreadsModal` and `closeThreadsModal` functions**

Find the existing OAuth modal functions (around line 981):
```js
// ── OAuth Modal ─────────────────────────────────────────────────────────────────
function openOAuthModal() {
  document.getElementById('oauth-modal').classList.add('open');
  document.body.style.overflow = 'hidden';
}

function closeOAuthModal(event) {
  if (event && event.target !== document.getElementById('oauth-modal')) return;
  document.getElementById('oauth-modal').classList.remove('open');
  document.body.style.overflow = '';
}
```

Immediately after `closeOAuthModal`, insert:

```js
function openThreadsModal() {
  document.getElementById('threads-modal').classList.add('open');
  document.body.style.overflow = 'hidden';
}

function closeThreadsModal(event) {
  if (event && event.target !== document.getElementById('threads-modal')) return;
  document.getElementById('threads-modal').classList.remove('open');
  document.body.style.overflow = '';
}
```

**Step 2: Extend the Escape key handler to close the Threads modal too**

Find the existing keydown listener (around line 994):
```js
document.addEventListener('keydown', function(e) {
  if (e.key === 'Escape') {
    document.getElementById('oauth-modal').classList.remove('open');
    document.body.style.overflow = '';
  }
});
```

Replace it with:
```js
document.addEventListener('keydown', function(e) {
  if (e.key === 'Escape') {
    document.getElementById('oauth-modal').classList.remove('open');
    document.getElementById('threads-modal').classList.remove('open');
    document.body.style.overflow = '';
  }
});
```

**Step 3: Wire up `__THREADS_GUIDE_BUTTON__` in the catalog renderer**

Find the existing guide-button conditional (around line 885):
```js
      if (tool.setupGuide === '__OAUTH_GUIDE_BUTTON__') {
        guideHtml = `<button class="btn-guide" onclick="event.stopPropagation();openOAuthModal()" type="button">&#9432; OAuth Setup Guide</button>`;
      } else if (tool.setupGuide) {
        guideHtml = `<div class="mcp-setup-guide" onclick="event.stopPropagation()">${tool.setupGuide}</div>`;
      }
```

Replace it with:
```js
      if (tool.setupGuide === '__OAUTH_GUIDE_BUTTON__') {
        guideHtml = `<button class="btn-guide" onclick="event.stopPropagation();openOAuthModal()" type="button">&#9432; OAuth Setup Guide</button>`;
      } else if (tool.setupGuide === '__THREADS_GUIDE_BUTTON__') {
        guideHtml = `<button class="btn-guide" onclick="event.stopPropagation();openThreadsModal()" type="button">&#9432; Threads Setup Guide</button>`;
      } else if (tool.setupGuide) {
        guideHtml = `<div class="mcp-setup-guide" onclick="event.stopPropagation()">${tool.setupGuide}</div>`;
      }
```

**Step 4: Test the full flow**

1. Open `setup/index.html` in a browser (or run `./setup.sh`)
2. Navigate to Step 5 (MCP Tools)
3. Confirm "Social Media" category appears with a **threads** card
4. Click the **threads** card to select it
5. Confirm the `THREADS_ACCESS_TOKEN` input appears and the **"Threads Setup Guide"** button appears
6. Click the button — the modal should open with 4 numbered steps
7. Press Escape — the modal should close
8. Click outside the modal box — the modal should close
9. Click the X button — the modal should close
10. Proceed to Step 6 (Review) and confirm the config preview includes:
    ```toml
    [[mcp_servers]]
    name = "threads"
    command = "npx"
    args = ["-y", "threads-mcp-server"]
    [mcp_servers.env]
    THREADS_ACCESS_TOKEN = ""
    ```

**Step 5: Commit**

```bash
git add setup/index.html
git commit -m "feat: wire Threads setup guide button and modal JS"
```

---

### Task 4: Add commented Threads example to `config.example.toml`

**Files:**
- Modify: `config.example.toml` (after the Google Workspace block, around line 105)

**Step 1: Locate the insertion point**

The Google Workspace block ends around line 104:
```toml
# GOOGLE_WORKSPACE_ENABLED_CAPABILITIES = '["drive","docs","gmail","calendar","sheets","slides"]'
```

Then comes:
```toml

# Example: Web search MCP server with environment variables
```

**Step 2: Insert the Threads block between them**

```toml

# Example: Meta Threads MCP server (publish posts, read replies, analytics)
#
# Token setup:
#   1. Go to https://developers.facebook.com/apps → Create App → Business type
#      → add the "Threads API" product from the dashboard
#   2. Under Threads API → Permissions → request:
#         threads_basic, threads_content_publish,
#         threads_manage_replies, threads_read_replies
#      Add your Threads account as a Test User under App Roles → Roles
#   3. Under Threads API → Access Tokens → Generate Token for your test user
#      → copy the long-lived token (valid ~60 days; regenerate before expiry)
#
# [[mcp_servers]]
# name = "threads"
# command = "npx"
# args = ["-y", "threads-mcp-server"]
# [mcp_servers.env]
# THREADS_ACCESS_TOKEN = "your-long-lived-access-token"
```

**Step 3: Verify the file is still valid TOML**

Since all Threads lines are commented, no TOML parsing is needed — but confirm no accidental uncommented lines were added by scanning the diff.

**Step 4: Commit**

```bash
git add config.example.toml
git commit -m "docs: add Threads MCP commented example to config.example.toml"
```

---

### Task 5: Update `README.md`

**Files:**
- Modify: `README.md`

**Step 1: Add Threads row to the Popular MCP Servers table**

Find the table (around line 111):
```markdown
| Server | Package | Runtime | Notes |
|--------|---------|---------|-------|
| [Git](...) | `mcp-server-git` | `uvx` | Read/search git repos |
...
| [Puppeteer](...) | `@modelcontextprotocol/server-puppeteer` | `npx` | Browser automation |
```

Append a new row after `Puppeteer`:
```markdown
| [Threads](https://github.com/baguskto/threads-mcp) | `threads-mcp-server` | `npx` | Publish & manage Meta Threads posts (needs access token) |
```

**Step 2: Add Threads TOML example to the Examples section**

Find the Examples section (around line 123). After the `Brave Search` example block:
```toml
[mcp_servers.env]
BRAVE_API_KEY = "your-brave-api-key"
```

Add:
```toml

# Meta Threads — publish posts, read replies, and view analytics
[[mcp_servers]]
name    = "threads"
command = "npx"
args    = ["-y", "threads-mcp-server"]
[mcp_servers.env]
THREADS_ACCESS_TOKEN = "your-long-lived-access-token"
```

**Step 3: Verify rendering**

Preview the README (e.g. `gh browse` or open in GitHub web UI). Confirm the table row is aligned and the code block renders correctly.

**Step 4: Commit**

```bash
git add README.md
git commit -m "docs: add Threads MCP to README popular servers table and examples"
```

---

### Task 6: Commit design doc and push branch

**Step 1: Stage the design documents**

```bash
git add docs/plans/2026-02-21-threads-mcp-integration-design.md
git add docs/plans/2026-02-21-threads-mcp-integration.md
git commit -m "docs: add Threads MCP integration design and implementation plan"
```

**Step 2: Push to the feature branch**

```bash
git push -u origin claude/add-threads-mcp-setup-TTvOL
```

Expected: branch pushed, no errors.

---

## Summary of Changes

| File | Change |
|------|--------|
| `setup/index.html` | New catalog entry (Social Media category), new `#threads-modal` HTML, two new JS functions, updated Escape handler, updated `guideHtml` conditional |
| `config.example.toml` | New commented `[[mcp_servers]]` block for Threads |
| `README.md` | New row in popular servers table, new TOML example block |
| `docs/plans/` | Two new design/plan documents |
