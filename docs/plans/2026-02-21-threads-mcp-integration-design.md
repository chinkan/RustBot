# Threads MCP Integration — Design

**Date:** 2026-02-21
**Status:** Approved

## Goal

Add Meta Threads publishing support to RustFox via the `baguskto/threads-mcp` MCP server. The integration must match the existing Google Workspace MCP pattern: catalog entry in the setup wizard, a guideline popup modal for token setup, a commented example in `config.example.toml`, and a README update.

## Chosen MCP

**`baguskto/threads-mcp`** — npm package `threads-mcp-server`

- Runner: `npx -y threads-mcp-server`
- Single environment variable: `THREADS_ACCESS_TOKEN`
- Capabilities: publish posts, read replies, delete posts, analytics, search
- Fits existing `npx` launcher model used throughout the catalog
- Reference: https://github.com/baguskto/threads-mcp

Why not the alternatives:
- **ThreadsMcpNet**: requires .NET 8 + Redis; not compatible with the npx/uvx launcher model
- **Apify scrapers**: read-only, no publishing capability

## Files Changed

### 1. `setup/index.html`

**MCP Catalog entry** — new object added to `MCP_CATALOG` array under a new `"Social Media"` category:

```js
{
  name: 'threads',
  category: 'Social Media',
  desc: 'Publish and manage Meta Threads posts',
  runner: 'npx',
  args: ['-y', 'threads-mcp-server'],
  envVars: ['THREADS_ACCESS_TOKEN'],
  setupGuide: '__THREADS_GUIDE_BUTTON__',
  link: 'https://github.com/baguskto/threads-mcp'
}
```

**New modal** — `#threads-modal` following the exact same HTML/CSS structure as `#oauth-modal`:

- Title: "Meta Threads — Access Token Setup"
- Subtitle: "Follow these 4 steps to get your access token. You only need to do this once."
- 4 numbered steps:
  1. Create a Meta Developer App at developers.facebook.com
  2. Add the Threads API product to the app
  3. Configure permissions: `threads_basic`, `threads_content_publish`, `threads_manage_replies`, `threads_read_replies`
  4. Generate a long-lived access token and paste into `THREADS_ACCESS_TOKEN`
- Warning box: note about token expiry (long-lived tokens last ~60 days) and need to refresh

**JS functions:**
- `openThreadsModal()` — adds `.open` class to `#threads-modal`, sets `overflow:hidden`
- `closeThreadsModal(event)` — removes `.open` class, restores overflow
- Escape key handler extended to also close `#threads-modal`

**Catalog rendering** — add `else if` branch for `__THREADS_GUIDE_BUTTON__` in the `guideHtml` block

### 2. `config.example.toml`

New commented block after the Google Workspace section:

```toml
# Example: Meta Threads MCP server (publish posts, read replies, analytics)
#
# Token setup:
#   1. Go to https://developers.facebook.com → My Apps → Create App
#      → select "Business" type → add "Threads API" product
#   2. Under Threads API → Permissions → add:
#         threads_basic, threads_content_publish,
#         threads_manage_replies, threads_read_replies
#   3. Under Threads API → Access Tokens → Generate Token
#      → copy the long-lived token (valid ~60 days; refresh before expiry)
#
# [[mcp_servers]]
# name = "threads"
# command = "npx"
# args = ["-y", "threads-mcp-server"]
# [mcp_servers.env]
# THREADS_ACCESS_TOKEN = "your-long-lived-access-token"
```

### 3. `README.md`

**Popular MCP Servers table** — new row:

| [Threads](https://github.com/baguskto/threads-mcp) | `threads-mcp-server` | `npx` | Publish & manage Meta Threads posts (needs access token) |

**Threads example block** — added to the Examples section:

```toml
# Meta Threads — publish posts and read replies
[[mcp_servers]]
name    = "threads"
command = "npx"
args    = ["-y", "threads-mcp-server"]
[mcp_servers.env]
THREADS_ACCESS_TOKEN = "your-long-lived-access-token"
```

## CSS / Styling

No new CSS is needed. The existing `.modal-overlay`, `.modal-box`, `.oauth-steps`, `.oauth-step`, `.oauth-step-num`, `.oauth-step-body`, `.oauth-step-title`, `.oauth-step-desc`, `.scope-list`, `.oauth-warning`, and `.btn-guide` classes are reused as-is for the Threads modal.

## No Rust / Backend Changes

All changes are in static files (`setup/index.html`, `config.example.toml`, `README.md`). No Rust source code changes are required because RustFox's MCP manager (`mcp.rs`) already handles any stdio-based MCP server generically.

## Testing Notes

- Open `setup/index.html` directly in a browser (or via `./setup.sh`)
- Navigate to Step 5 (MCP Tools) — verify "Social Media" category appears with a "threads" card
- Check the card when selected: `THREADS_ACCESS_TOKEN` field appears, "Threads Setup Guide" button opens the modal
- Verify modal closes via X button, Escape key, and clicking outside
- Verify config preview in Step 6 includes the threads block with correct env var
