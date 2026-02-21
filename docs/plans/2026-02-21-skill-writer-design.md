# Design: Skill Writer — Agent Self-Authoring Skills with Hot-Reload

**Date:** 2026-02-21
**Status:** Approved

---

## Goal

Enable the RustFox Telegram bot to:
1. Interactively author new agent skills in the correct format at a user's request
2. Write multi-file skill directories (SKILL.md + supporting reference/template/script files)
3. Hot-reload skills into memory immediately — no bot restart required

---

## Architecture Overview

Three components work together:

```
skills/creating-skills/SKILL.md
  (new instruction file)
  • Teaches agent the SKILL.md format and best practices
  • Guides agent to call write_skill_file() for each file
  • Tells agent to call reload_skills() when done
        │
        │ injected into system prompt
        ▼
Agent (src/agent.rs)
  skills: RwLock<SkillRegistry>   ◄──┐
                                       │
  write_skill_file(skill_name,         │
                   relative_path,      │
                   content)            │
    → creates skills/<name>/<path>     │
                                       │
  reload_skills()                      │
    → reloads dir → writes RwLock ─────┘

  process_message()
    → always rebuilds messages[0]
      from current live registry
      (in-memory only, DB keeps historical system message)
```

**Hot-reload flow:**
1. User asks agent to create a skill
2. Agent calls `write_skill_file` one or more times (building the directory)
3. Agent calls `reload_skills` → registry updated in RwLock
4. `process_message` rebuilds system prompt from live registry before each LLM call
5. Skill is immediately active in the same conversation — no restart needed

---

## Chosen Approach: Approach A (Dedicated Skill Management Tools)

Rejected alternatives:
- **Approach B** (extend `write_file` sandbox): Muddles security model; sandbox designed for one root
- **Approach C** (pure SKILL.md, no Rust changes): `execute_command` is also sandboxed; can't reach `skills/`

---

## Rust Changes — `src/agent.rs` only

### 1. Field type change

```rust
// before
pub skills: SkillRegistry,

// after
pub skills: tokio::sync::RwLock<SkillRegistry>,
```

`Agent::new` wraps: `skills: tokio::sync::RwLock::new(skills)`

### 2. `build_system_prompt` becomes async

```rust
async fn build_system_prompt(&self) -> String {
    let skills = self.skills.read().await;
    let skill_context = skills.build_context();
    // ... same body as before
}
```

All call sites in `process_message` already `await`.

### 3. `process_message` — always-fresh system prompt

```rust
let current_system_prompt = self.build_system_prompt().await;
if messages.is_empty() {
    // First message: save to DB as before
    let system_msg = ChatMessage {
        role: "system".to_string(),
        content: Some(current_system_prompt),
        tool_calls: None,
        tool_call_id: None,
    };
    self.memory.save_message(&conversation_id, &system_msg).await?;
    messages.push(system_msg);
} else {
    // Existing conversation: refresh in-memory only
    // DB retains historical system message; live calls always use fresh one
    messages[0].content = Some(current_system_prompt);
}
```

### 4. Two new tools

Added to a new `skill_tool_definitions()` method, `all_tool_definitions()`, the agentic loop tool list, and `execute_tool()`:

#### `write_skill_file`

| Parameter | Type | Description |
|-----------|------|-------------|
| `skill_name` | string | Skill directory name: lowercase letters, numbers, hyphens only (max 64 chars) |
| `relative_path` | string | Path within skill dir, e.g. `SKILL.md`, `reference.md`, `scripts/helper.py` |
| `content` | string | Full file content to write |

Behaviour:
- Validates `skill_name`: regex `^[a-z0-9-]{1,64}$`
- Validates `relative_path`: no `..` components, forward slashes only
- Creates `config.skills.directory / skill_name / relative_path`
- Creates parent directories automatically
- Returns path written on success

#### `reload_skills`

No parameters. Calls `load_skills_from_dir(&self.config.skills.directory)`, acquires write lock, replaces registry, returns count of loaded skills.

---

## New Skill File — `skills/creating-skills/SKILL.md`

Named `creating-skills` (gerund form per official best practices).

### YAML frontmatter

```yaml
---
name: creating-skills
description: Use when the user asks to create, write, or add a new bot skill, or wants to teach the bot a new behavior or capability.
tags: [skills, meta]
---
```

### Instruction body — what the agent does

1. **Gather** — Ask user: skill name (slug), what it does, when it should trigger, whether supporting reference/template/script files are needed
2. **Design** — Plan the file structure:
   - `SKILL.md` always (main entry point)
   - Optional: `reference.md`, `examples.md`, `templates/`, `scripts/` — one level deep per best practices
3. **Write** — Call `write_skill_file` once per file
4. **Activate** — Call `reload_skills` immediately after
5. **Confirm** — Report to user: skill is live, list files created

### SKILL.md content the agent writes follows official best practices

- YAML frontmatter: `name` + `description` only
- `description`: starts with "Use when...", third person, no workflow summary, max 1024 chars
- `name`: lowercase letters, numbers, hyphens, max 64 chars, no reserved words
- Body under 500 lines; heavy content split into separate reference files
- References kept one level deep from SKILL.md (no nested chains)
- No time-sensitive content
- Consistent terminology throughout

---

## Files Touched

| File | Change |
|------|--------|
| `src/agent.rs` | RwLock wrapping, async build_system_prompt, always-fresh messages[0], two new tools + handlers |
| `skills/creating-skills/SKILL.md` | New skill file (no code changes needed) |

No other files require modification.

---

## Security Notes

- `write_skill_file` validates `skill_name` with strict regex (no traversal possible via name)
- `relative_path` is checked for `..` components before joining
- Skills directory is separate from the user sandbox — no cross-contamination
- `reload_skills` only reads from the configured `skills.directory`

---

## Open Questions (resolved)

- **Single vs multi-file tool**: Chose single `write_skill_file(skill_name, relative_path, content)` — more extensible, handles full directory structures
- **Hot-reload scope**: In-memory only; DB keeps historical system message intact
- **Skill name for this feature**: `creating-skills` (gerund, follows official naming convention)
