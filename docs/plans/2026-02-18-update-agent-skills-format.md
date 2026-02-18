# Update Agent Skills Format Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Update all `.claude/skills/` entries to fully conform to Anthropic's official Claude agent skills specification.

**Architecture:** Each skill lives in a dedicated folder (`skill-name/`) containing `SKILL.md` with YAML frontmatter (`name` + `description`) plus optional resource files. The current structure is almost compliant — the main issues are a non-conforming `brainstorming` description and a missing `session-start-hook` skill folder.

**Tech Stack:** Plain markdown, YAML frontmatter, Git

---

## Background: Official Claude Agent Skills Spec

From https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview:

**Required structure:**
```
.claude/skills/
  skill-name/          ← folder named with lowercase letters, numbers, hyphens
    SKILL.md           ← required; YAML frontmatter + body
    supporting-file.*  ← optional resource files
```

**Required SKILL.md frontmatter:**
```yaml
---
name: skill-name          # lowercase letters, numbers, hyphens only; max 64 chars; no reserved words
description: Use when...  # max 1024 chars; no XML tags; describes WHEN to use, not HOW
---
```

**Three-level loading:**
1. **Metadata** — `name` + `description` always in system prompt (~100 tokens each)
2. **Instructions** — SKILL.md body loaded when skill is triggered (<5k tokens)
3. **Resources** — additional files loaded on demand (no practical token limit)

---

## Audit: Current State

| Skill | Folder | SKILL.md | Frontmatter | Description format | Status |
|-------|--------|----------|-------------|-------------------|--------|
| `brainstorming` | ✅ | ✅ | ✅ | ❌ Starts with "You MUST use this..." not "Use when..." | **FIX NEEDED** |
| `dispatching-parallel-agents` | ✅ | ✅ | ✅ | ✅ "Use when facing 2+ independent tasks..." | OK |
| `executing-plans` | ✅ | ✅ | ✅ | needs verify | verify |
| `finishing-a-development-branch` | ✅ | ✅ | ✅ | needs verify | verify |
| `receiving-code-review` | ✅ | ✅ | ✅ | needs verify | verify |
| `requesting-code-review` | ✅ | ✅ | ✅ | needs verify | verify |
| `subagent-driven-development` | ✅ | ✅ | ✅ | needs verify | verify |
| `systematic-debugging` | ✅ | ✅ | ✅ | ✅ "Use when encountering any bug..." | OK |
| `test-driven-development` | ✅ | ✅ | ✅ | ✅ "Use when implementing any feature..." | OK |
| `using-git-worktrees` | ✅ | ✅ | ✅ | needs verify | verify |
| `using-superpowers` | ✅ | ✅ | ✅ | needs verify | verify |
| `verification-before-completion` | ✅ | ✅ | ✅ | ✅ "Use when about to claim work is complete..." | OK |
| `writing-plans` | ✅ | ✅ | ✅ | needs verify | verify |
| `writing-skills` | ✅ | ✅ | ✅ | needs verify | verify |
| `session-start-hook` | ❌ **MISSING** | ❌ | ❌ | — | **CREATE** |

---

## Task 1: Set up git branch

**Files:** None (git operations only)

**Step 1: Verify you're on the correct branch**

```bash
git branch
```
Expected: `* claude/update-agent-skills-format-YAcbJ`

**Step 2: If not on branch, switch to it**

```bash
git checkout claude/update-agent-skills-format-YAcbJ
```

---

## Task 2: Full compliance audit of all SKILL.md frontmatter

Audit every `SKILL.md` against the spec before making changes. Check:
- `name` field: lowercase letters, numbers, hyphens only; max 64 chars
- `description` field: non-empty; max 1024 chars; no XML tags; starts with "Use when..."
- No extra frontmatter fields beyond `name` and `description`

**Files:**
- Read: `.claude/skills/*/SKILL.md` (all 14 skill folders)

**Step 1: Read each remaining unverified SKILL.md**

Read the following files (frontmatter only — first 10 lines):
- `.claude/skills/executing-plans/SKILL.md`
- `.claude/skills/finishing-a-development-branch/SKILL.md`
- `.claude/skills/receiving-code-review/SKILL.md`
- `.claude/skills/requesting-code-review/SKILL.md`
- `.claude/skills/subagent-driven-development/SKILL.md`
- `.claude/skills/using-git-worktrees/SKILL.md`
- `.claude/skills/using-superpowers/SKILL.md`
- `.claude/skills/writing-plans/SKILL.md`
- `.claude/skills/writing-skills/SKILL.md`

**Step 2: Record any violations**

For each file, note:
- Does description start with "Use when..."? (if not, flag it)
- Is description under 1024 chars? (if not, flag it)
- Are there any XML tags in frontmatter? (if yes, flag it)
- Are there extra frontmatter fields beyond `name`/`description`? (if yes, flag it)

**Step 3: Commit audit notes (optional)**

No code changes yet — this is a read-only discovery step.

---

## Task 3: Fix `brainstorming` description

The `brainstorming/SKILL.md` description currently starts with "You MUST use this before any creative work..." which violates the spec (must start with "Use when...") and the internal writing-skills guideline (description must NEVER summarize workflow).

**Files:**
- Modify: `.claude/skills/brainstorming/SKILL.md` (line 3)

**Step 1: Read the current description**

Read `.claude/skills/brainstorming/SKILL.md` lines 1-5.

**Step 2: Replace the description**

Change:
```yaml
description: "You MUST use this before any creative work - creating features, building components, adding functionality, or modifying behavior. Explores user intent, requirements and design before implementation."
```

To (triggering conditions only, no workflow summary, starts with "Use when..."):
```yaml
description: "Use when starting any creative work — creating features, building components, adding functionality, or modifying behavior — before any implementation or planning"
```

This:
- Starts with "Use when..." ✅
- Describes triggering conditions, not workflow ✅
- Under 1024 chars ✅
- No XML tags ✅
- Third person ✅

**Step 3: Verify the edit**

Read `.claude/skills/brainstorming/SKILL.md` lines 1-5 and confirm the change looks correct.

**Step 4: Commit**

```bash
git add .claude/skills/brainstorming/SKILL.md
git commit -m "fix: update brainstorming skill description to comply with Claude skills spec

Description now starts with 'Use when...' and describes triggering
conditions only, not workflow — per Anthropic skills spec and
writing-skills CSO guidelines."
```

---

## Task 4: Fix any additional violations found in Task 2

Apply the same pattern as Task 3 for any other skills flagged during the audit.

For each violation:

**Step 1: Read the current SKILL.md**

**Step 2: Fix the specific violation**

Common fixes:
- Description doesn't start with "Use when...": Rephrase to start with "Use when [specific triggering conditions]"
- Description summarizes workflow: Remove workflow descriptions, keep only triggering conditions
- Extra frontmatter fields: Remove any fields other than `name` and `description`
- Description over 1024 chars: Trim to most important triggering conditions

**Step 3: Verify the edit**

**Step 4: Commit per skill fixed**

```bash
git add .claude/skills/<skill-name>/SKILL.md
git commit -m "fix: update <skill-name> skill description to comply with Claude skills spec"
```

---

## Task 5: Create `session-start-hook` skill folder

The `session-start-hook` skill appears in the system-reminder's available skills list but has no folder in `.claude/skills/`. Create it following the official spec.

**Files:**
- Create: `.claude/skills/session-start-hook/SKILL.md`

**Step 1: Create the folder and SKILL.md**

Create `.claude/skills/session-start-hook/SKILL.md` with content:

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
        "hooks": [
          {
            "type": "command",
            "command": "your-check-command-here"
          }
        ]
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
| Install deps | `npm install` / `cargo fetch` |
| Multiple checks | Chain with `&&` |

## Example: RustBot Session Start Hook

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "cargo check && cargo fmt --all -- --check && cargo clippy -- -D warnings"
          }
        ]
      }
    ]
  }
}
```

## Notes

- Hook failures are surfaced to Claude at session start
- Keep hooks fast (< 30 seconds) to avoid slow session startup
- Use `--check` flags for formatters/linters rather than auto-fixing
```

**Step 2: Verify the file was created**

Read `.claude/skills/session-start-hook/SKILL.md` and confirm content looks correct.

**Step 3: Commit**

```bash
git add .claude/skills/session-start-hook/SKILL.md
git commit -m "feat: add session-start-hook skill folder per Claude agent skills spec

Creates the missing session-start-hook skill with SKILL.md following
the official Claude agent skills format (folder + YAML frontmatter)."
```

---

## Task 6: Verify overall structure compliance

Do a final check that all skills now comply with the official spec.

**Step 1: List all skills and their SKILL.md files**

```bash
find .claude/skills -name "SKILL.md" | sort
```

Expected: One `SKILL.md` per skill folder, including the new `session-start-hook`.

**Step 2: Check all name fields**

```bash
grep -h "^name:" .claude/skills/*/SKILL.md | sort
```

Verify: all names use only lowercase letters, numbers, hyphens; max 64 chars.

**Step 3: Check all description fields start with "Use when" or acceptable variants**

```bash
grep -h "^description:" .claude/skills/*/SKILL.md
```

Review each line.

**Step 4: Confirm no extra frontmatter fields**

```bash
grep -h "^\w" .claude/skills/*/SKILL.md | grep -v "^name:\|^description:\|^---\|^#"
```

Expected: No output (no unexpected top-level YAML keys).

---

## Task 7: Push to remote

**Step 1: Push the branch**

```bash
git push -u origin claude/update-agent-skills-format-YAcbJ
```

If push fails due to network error, retry with exponential backoff (wait 2s, 4s, 8s, 16s between attempts).

**Step 2: Verify push succeeded**

```bash
git log --oneline origin/claude/update-agent-skills-format-YAcbJ..HEAD
```

Expected: Empty output (local and remote are in sync).

---

## Summary of Changes

| Task | Change | Files Affected |
|------|--------|---------------|
| 2 | Audit all 14 skill SKILL.md files | Read-only |
| 3 | Fix `brainstorming` description | `.claude/skills/brainstorming/SKILL.md` |
| 4 | Fix any additional violations found | `.claude/skills/<skill>/SKILL.md` (TBD) |
| 5 | Create `session-start-hook` skill | `.claude/skills/session-start-hook/SKILL.md` (new) |
| 6 | Verify full compliance | Read-only |
| 7 | Push to remote | Git remote |

**No application code is modified.** All changes are in `.claude/skills/` only.
