# Update Agent Skills Format Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Update **two** skill directories to fully conform to Anthropic's official Claude agent skills specification:

1. **`skills/`** — RustFox's own bot skills (loaded into the Telegram bot's LLM system prompt by `src/skills/loader.rs`). Currently uses flat `.md` files → convert to `skill-name/SKILL.md` folder format.
2. **`.claude/skills/`** — Claude Code development skills (used by Claude when working on this project). Already uses folder format but has minor compliance issues to fix.

**Tech Stack:** Plain markdown, YAML frontmatter, Rust (no code changes needed — loader already supports folder format), Git

---

## Background: Official Claude Agent Skills Spec

From https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview:

**Required structure:**
```
skills/
  skill-name/          ← folder named with lowercase letters, numbers, hyphens
    SKILL.md           ← required; YAML frontmatter + body
    supporting-file.*  ← optional resource files
```

**Required SKILL.md frontmatter:**
```yaml
---
name: skill-name          # lowercase letters, numbers, hyphens only; max 64 chars
description: Use when...  # max 1024 chars; no XML tags; triggering conditions only
---
```

---

## Part A: RustFox Bot Skills (`skills/`)

### How RustFox Skills Work

`src/skills/loader.rs` loads skills from the directory configured in `config.toml` (`[skills] directory = "skills"`). `src/skills/mod.rs` builds a context string from them, injected into the LLM system prompt as:

```
## Skill: <name>
<full SKILL.md body>
```

The loader **already supports both formats**:
- `skills/my-skill.md` — flat file (current, to be replaced)
- `skills/my-skill/SKILL.md` — folder format (target)

RustFox skills also support an optional `tags` field in frontmatter (RustFox extension, not in Claude spec).

### Current State Audit

| Skill file | Format | Issue |
|-----------|--------|-------|
| `skills/coding-assistant.md` | ❌ flat `.md` | Must convert to folder |
| `skills/memory-manager.md` | ❌ flat `.md` | Must convert to folder |

---

### Task A1: Convert `coding-assistant` to folder format

**Files:**
- Create: `skills/coding-assistant/SKILL.md`
- Delete: `skills/coding-assistant.md`

**Step 1: Read current file**

Read `skills/coding-assistant.md` to confirm full content.

**Step 2: Create folder and SKILL.md**

Create `skills/coding-assistant/SKILL.md` with identical content to `skills/coding-assistant.md`.

**Step 3: Delete the flat file**

```bash
rm skills/coding-assistant.md
```

**Step 4: Verify**

```bash
ls skills/coding-assistant/
cat skills/coding-assistant/SKILL.md
```

Expected: folder exists, SKILL.md has same frontmatter and body as the original file.

**Step 5: Commit**

```bash
git add skills/coding-assistant/SKILL.md
git rm skills/coding-assistant.md
git commit -m "refactor: convert coding-assistant skill to folder format

Moves skills/coding-assistant.md to skills/coding-assistant/SKILL.md
to match Claude agent skills spec (folder + SKILL.md structure)."
```

---

### Task A2: Convert `memory-manager` to folder format

**Files:**
- Create: `skills/memory-manager/SKILL.md`
- Delete: `skills/memory-manager.md`

**Step 1: Read current file**

Read `skills/memory-manager.md` to confirm full content.

**Step 2: Create folder and SKILL.md**

Create `skills/memory-manager/SKILL.md` with identical content to `skills/memory-manager.md`.

**Step 3: Delete the flat file**

```bash
rm skills/memory-manager.md
```

**Step 4: Verify**

```bash
ls skills/memory-manager/
cat skills/memory-manager/SKILL.md
```

Expected: folder exists, SKILL.md has same frontmatter and body as the original file.

**Step 5: Commit**

```bash
git add skills/memory-manager/SKILL.md
git rm skills/memory-manager.md
git commit -m "refactor: convert memory-manager skill to folder format

Moves skills/memory-manager.md to skills/memory-manager/SKILL.md
to match Claude agent skills spec (folder + SKILL.md structure)."
```

---

### Task A3: Update CLAUDE.md — document new skills format

The `CLAUDE.md` section on skills must reflect the folder-based format so future contributors know the correct structure.

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Find the skills section in CLAUDE.md**

Search for the skills-related documentation (look for "skill" mentions).

**Step 2: Update or add skills directory documentation**

Under the Architecture section or a dedicated Skills section, document:

```markdown
## Bot Skills (`skills/`)

Skills are natural-language instruction files loaded at startup and injected into the
LLM's system prompt. Each skill lives in its own folder:

```
skills/
  skill-name/
    SKILL.md           # Required: YAML frontmatter + instruction body
    supporting-file.*  # Optional: templates, examples, reference docs
```

**SKILL.md frontmatter:**
```yaml
---
name: skill-name     # lowercase letters, numbers, hyphens only
description: Brief description of what this skill does
tags: [tag1, tag2]   # optional: for organization
---
```

To add a new skill: create a `skills/<skill-name>/` folder with `SKILL.md`.
```

**Step 3: Verify the edit looks correct**

Read the updated section in CLAUDE.md.

**Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md to document folder-based skills format"
```

---

## Part B: Claude Code Skills (`.claude/skills/`)

These are used by Claude Code when developing RustFox, not by the bot itself.

### Current State Audit

| Skill | Folder | SKILL.md | Description format | Status |
|-------|--------|----------|--------------------|--------|
| `brainstorming` | ✅ | ✅ | ❌ Starts with "You MUST use this..." | **FIX** |
| `dispatching-parallel-agents` | ✅ | ✅ | ✅ "Use when facing 2+..." | OK |
| `executing-plans` | ✅ | ✅ | needs verify | verify |
| `finishing-a-development-branch` | ✅ | ✅ | needs verify | verify |
| `receiving-code-review` | ✅ | ✅ | needs verify | verify |
| `requesting-code-review` | ✅ | ✅ | needs verify | verify |
| `subagent-driven-development` | ✅ | ✅ | needs verify | verify |
| `systematic-debugging` | ✅ | ✅ | ✅ "Use when encountering any bug..." | OK |
| `test-driven-development` | ✅ | ✅ | ✅ "Use when implementing any feature..." | OK |
| `using-git-worktrees` | ✅ | ✅ | needs verify | verify |
| `using-superpowers` | ✅ | ✅ | needs verify | verify |
| `verification-before-completion` | ✅ | ✅ | ✅ "Use when about to claim work..." | OK |
| `writing-plans` | ✅ | ✅ | needs verify | verify |
| `writing-skills` | ✅ | ✅ | needs verify | verify |
| `session-start-hook` | ❌ **MISSING** | ❌ | — | **CREATE** |

---

### Task B1: Audit remaining `.claude/skills/` SKILL.md files

Read frontmatter of all unverified skills (first 5 lines each). Flag any that:
- Description doesn't start with "Use when..."
- Description exceeds 1024 chars
- Description contains XML tags
- Frontmatter has fields other than `name` and `description`

**Files to read:**
- `.claude/skills/executing-plans/SKILL.md`
- `.claude/skills/finishing-a-development-branch/SKILL.md`
- `.claude/skills/receiving-code-review/SKILL.md`
- `.claude/skills/requesting-code-review/SKILL.md`
- `.claude/skills/subagent-driven-development/SKILL.md`
- `.claude/skills/using-git-worktrees/SKILL.md`
- `.claude/skills/using-superpowers/SKILL.md`
- `.claude/skills/writing-plans/SKILL.md`
- `.claude/skills/writing-skills/SKILL.md`

---

### Task B2: Fix `brainstorming` description

**Files:** `.claude/skills/brainstorming/SKILL.md` (line 3)

Current (violates spec — summarizes workflow, doesn't start with "Use when..."):
```yaml
description: "You MUST use this before any creative work - creating features, building components, adding functionality, or modifying behavior. Explores user intent, requirements and design before implementation."
```

Replace with (triggering conditions only):
```yaml
description: "Use when starting any creative work — creating features, building components, adding functionality, or modifying behavior — before any implementation or planning"
```

**Commit:**
```bash
git add .claude/skills/brainstorming/SKILL.md
git commit -m "fix: update brainstorming skill description to comply with Claude skills spec"
```

---

### Task B3: Fix any additional violations found in Task B1

For each flagged skill, apply same fix as B2: rephrase description to "Use when [triggering conditions]", remove extra frontmatter fields.

Commit each fix separately:
```bash
git add .claude/skills/<skill-name>/SKILL.md
git commit -m "fix: update <skill-name> skill description to comply with Claude skills spec"
```

---

### Task B4: Create missing `session-start-hook` skill folder

**Files:** Create `.claude/skills/session-start-hook/SKILL.md`

```markdown
---
name: session-start-hook
description: Use when the user wants to set up a repository for Claude Code on the web, or create a SessionStart hook to ensure their project can run tests and linters during web sessions
---

# Session Start Hook

## Overview

A SessionStart hook runs automatically when Claude Code starts a new session. Use it to ensure the development environment is ready: tests pass, linters are clean, dependencies are installed.

## When to Use

- User wants to set up their repo for Claude Code on the web
- User wants tests or linters to run automatically at session start
- User needs environment validation before coding begins

## Quick Start

Add a `hooks` section to `.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "",
        "hooks": [{"type": "command", "command": "your-check-command-here"}]
      }
    ]
  }
}
```

## Common Patterns

| Goal | Command |
|------|---------|
| Run tests | `cargo test` / `npm test` / `pytest` |
| Lint check | `cargo clippy` / `eslint .` |
| Format check | `cargo fmt --check` / `prettier --check .` |
| Multiple checks | Chain with `&&` |

## Example: RustFox

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "",
        "hooks": [{
          "type": "command",
          "command": "cargo check && cargo fmt --all -- --check && cargo clippy -- -D warnings"
        }]
      }
    ]
  }
}
```

## Notes

- Hook failures are surfaced to Claude at session start
- Keep hooks fast (< 30 seconds)
- Use `--check` flags for linters/formatters, not auto-fix
```

**Commit:**
```bash
git add .claude/skills/session-start-hook/SKILL.md
git commit -m "feat: add session-start-hook skill per Claude agent skills spec"
```

---

### Task B5: Final compliance verification

```bash
# All skill folders have SKILL.md
find .claude/skills -name "SKILL.md" | sort

# All names are valid
grep -h "^name:" .claude/skills/*/SKILL.md | sort

# All descriptions
grep -h "^description:" .claude/skills/*/SKILL.md
```

---

## Task C: Push to remote

```bash
git push -u origin claude/update-agent-skills-format-YAcbJ
```

Retry up to 4 times with exponential backoff (2s, 4s, 8s, 16s) on network failure.

---

## Summary of All Changes

| Task | Change | Files Affected |
|------|--------|---------------|
| A1 | `coding-assistant.md` → `coding-assistant/SKILL.md` | `skills/coding-assistant/SKILL.md` (new), `skills/coding-assistant.md` (deleted) |
| A2 | `memory-manager.md` → `memory-manager/SKILL.md` | `skills/memory-manager/SKILL.md` (new), `skills/memory-manager.md` (deleted) |
| A3 | Document skills format in CLAUDE.md | `CLAUDE.md` |
| B1 | Audit `.claude/skills/` frontmatter | Read-only |
| B2 | Fix `brainstorming` description | `.claude/skills/brainstorming/SKILL.md` |
| B3 | Fix additional violations (TBD) | `.claude/skills/<skill>/SKILL.md` |
| B4 | Create `session-start-hook` skill | `.claude/skills/session-start-hook/SKILL.md` (new) |
| B5 | Final verification | Read-only |
| C | Push to remote | Git remote |

**No Rust source code is modified.** The loader already supports folder format.
