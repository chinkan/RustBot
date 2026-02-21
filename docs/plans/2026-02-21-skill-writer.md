# Skill Writer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let the RustFox agent write multi-file skill directories and hot-reload them instantly — no bot restart required.

**Architecture:** Wrap `SkillRegistry` in `tokio::sync::RwLock` so `reload_skills` can swap it at runtime. Add `write_skill_file` + `reload_skills` as built-in tools in `agent.rs`. Rebuild the system prompt from the live registry on every `process_message` call so new skills take effect immediately in the same conversation.

**Tech Stack:** Rust 2021, Tokio async runtime, `anyhow`, `serde_json`, `tracing`

---

## Task 1: Add validation helpers + tests (TDD — RED then GREEN)

**Files:**
- Modify: `src/agent.rs`

### Step 1: Write failing tests

At the bottom of `src/agent.rs`, inside the existing `#[cfg(test)] mod tests` block, add:

```rust
    #[test]
    fn test_validate_skill_name_valid() {
        assert!(validate_skill_name("creating-skills").is_ok());
        assert!(validate_skill_name("my-skill-123").is_ok());
        assert!(validate_skill_name("a").is_ok());
    }

    #[test]
    fn test_validate_skill_name_empty() {
        assert!(validate_skill_name("").is_err());
    }

    #[test]
    fn test_validate_skill_name_too_long() {
        let long = "a".repeat(65);
        assert!(validate_skill_name(&long).is_err());
    }

    #[test]
    fn test_validate_skill_name_invalid_chars() {
        assert!(validate_skill_name("My-Skill").is_err());  // uppercase
        assert!(validate_skill_name("my skill").is_err()); // space
        assert!(validate_skill_name("my_skill").is_err()); // underscore
        assert!(validate_skill_name("my/skill").is_err()); // slash
    }

    #[test]
    fn test_validate_skill_path_valid() {
        assert!(validate_skill_path("SKILL.md").is_ok());
        assert!(validate_skill_path("reference.md").is_ok());
        assert!(validate_skill_path("scripts/helper.py").is_ok());
        assert!(validate_skill_path("scripts/sub/tool.sh").is_ok());
    }

    #[test]
    fn test_validate_skill_path_traversal() {
        assert!(validate_skill_path("../other-skill/SKILL.md").is_err());
        assert!(validate_skill_path("scripts/../../../etc/passwd").is_err());
        assert!(validate_skill_path("..").is_err());
    }

    #[test]
    fn test_validate_skill_path_empty() {
        assert!(validate_skill_path("").is_err());
    }
```

### Step 2: Run tests — expect compile failure (functions don't exist yet)

```bash
cargo test -p rustfox 2>&1 | head -30
```

Expected: `error[E0425]: cannot find function 'validate_skill_name'`

### Step 3: Add the validation functions

Just above the `#[cfg(test)]` block (at module level, after the `split_response_chunks` function), add:

```rust
/// Validate skill directory name: lowercase letters, numbers, hyphens, 1–64 chars.
fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Skill name must not be empty".to_string());
    }
    if name.len() > 64 {
        return Err(format!("Skill name too long ({} chars, max 64)", name.len()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(
            "Skill name must contain only lowercase letters, numbers, and hyphens".to_string(),
        );
    }
    Ok(())
}

/// Validate a relative path within a skill directory: no '..' components, non-empty.
fn validate_skill_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("Relative path must not be empty".to_string());
    }
    if path.split('/').any(|c| c == "..") {
        return Err("Path traversal ('..') is not allowed".to_string());
    }
    Ok(())
}
```

### Step 4: Run tests — expect GREEN

```bash
cargo test -p rustfox 2>&1 | tail -20
```

Expected: all new tests pass.

### Step 5: Commit

```bash
git add src/agent.rs
git commit -m "feat: add skill_name and skill_path validation helpers with tests"
```

---

## Task 2: Wrap SkillRegistry in RwLock and make build_system_prompt async

**Files:**
- Modify: `src/agent.rs`

### Step 1: Change the `skills` field type

In the `Agent` struct (around line 33), change:

```rust
// BEFORE
pub skills: SkillRegistry,

// AFTER
pub skills: tokio::sync::RwLock<SkillRegistry>,
```

### Step 2: Update `Agent::new` to wrap in RwLock

In `Agent::new`, in the `Self { ... }` block (around line 58), change:

```rust
// BEFORE
            skills,

// AFTER
            skills: tokio::sync::RwLock::new(skills),
```

### Step 3: Make `build_system_prompt` async and acquire read lock

Replace the entire `build_system_prompt` function (currently around line 73–92):

```rust
/// Build the system prompt, incorporating loaded skills
async fn build_system_prompt(&self) -> String {
    let mut prompt = self.config.openrouter.system_prompt.clone();

    let skills = self.skills.read().await;
    let skill_context = skills.build_context();
    if !skill_context.is_empty() {
        prompt.push_str("\n\n# Available Skills\n\n");
        prompt.push_str(&skill_context);
    }
    drop(skills); // release read lock before further work

    // Append current timestamp and optional location
    let now = chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();
    prompt.push_str(&format!("\n\nCurrent date and time: {}", now));
    if let Some(loc) = self.config.user_location() {
        prompt.push_str(&format!("\nUser location: {}", loc));
    }

    prompt
}
```

### Step 4: Verify the project compiles

```bash
cargo check 2>&1
```

Expected: no errors. (The call site in `process_message` will be updated in the next step.)

### Step 5: Commit

```bash
git add src/agent.rs
git commit -m "feat: wrap SkillRegistry in RwLock, make build_system_prompt async"
```

---

## Task 3: Always-fresh system prompt in `process_message`

**Files:**
- Modify: `src/agent.rs`

### Step 1: Replace the system-prompt block in `process_message`

Find this block (around line 110–121):

```rust
        // If no messages yet, add system prompt
        if messages.is_empty() {
            let system_msg = ChatMessage {
                role: "system".to_string(),
                content: Some(self.build_system_prompt()),
                tool_calls: None,
                tool_call_id: None,
            };
            self.memory
                .save_message(&conversation_id, &system_msg)
                .await?;
            messages.push(system_msg);
        }
```

Replace it with:

```rust
        // Always build the system prompt from the live registry.
        // For new conversations: save to DB and push.
        // For existing conversations: refresh messages[0] in-memory only
        //   (DB keeps the historical system message intact).
        let current_system_prompt = self.build_system_prompt().await;
        if messages.is_empty() {
            let system_msg = ChatMessage {
                role: "system".to_string(),
                content: Some(current_system_prompt),
                tool_calls: None,
                tool_call_id: None,
            };
            self.memory
                .save_message(&conversation_id, &system_msg)
                .await?;
            messages.push(system_msg);
        } else {
            // Refresh in-memory: new skills loaded by reload_skills take effect
            // on the very next message without restarting the bot.
            messages[0].content = Some(current_system_prompt);
        }
```

### Step 2: Verify compilation

```bash
cargo check 2>&1
```

Expected: no errors.

### Step 3: Run existing tests

```bash
cargo test 2>&1
```

Expected: all existing tests pass.

### Step 4: Commit

```bash
git add src/agent.rs
git commit -m "feat: always rebuild system prompt from live registry on each process_message"
```

---

## Task 4: Add skill tool definitions

**Files:**
- Modify: `src/agent.rs`

### Step 1: Add `skill_tool_definitions()` method

Add this method to the `impl Agent` block, right after `scheduling_tool_definitions()` (before `execute_tool`):

```rust
    /// Skill management tool definitions exposed to the LLM
    fn skill_tool_definitions(&self) -> Vec<ToolDefinition> {
        use serde_json::json;

        vec![
            ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "write_skill_file".to_string(),
                    description: concat!(
                        "Write a file into a skill directory under the configured skills folder. ",
                        "Use this to create SKILL.md and any supporting files (reference docs, templates, scripts). ",
                        "Call reload_skills after ALL files for the skill are written."
                    ).to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "skill_name": {
                                "type": "string",
                                "description": "Skill directory name: lowercase letters, numbers, hyphens only, max 64 chars (e.g. 'creating-reports')"
                            },
                            "relative_path": {
                                "type": "string",
                                "description": "Path within the skill directory, e.g. 'SKILL.md', 'reference.md', 'scripts/helper.py'"
                            },
                            "content": {
                                "type": "string",
                                "description": "Full file content to write"
                            }
                        },
                        "required": ["skill_name", "relative_path", "content"]
                    }),
                },
            },
            ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "reload_skills".to_string(),
                    description: concat!(
                        "Reload all skills from the skills directory into memory. ",
                        "Call this after writing skill files to make the new skill immediately active ",
                        "without restarting the bot."
                    ).to_string(),
                    parameters: json!({ "type": "object", "properties": {} }),
                },
            },
        ]
    }
```

### Step 2: Add to `all_tool_definitions()`

In `all_tool_definitions()` (around line 324), add:

```rust
// BEFORE (last line of the method):
        all_tools
// AFTER:
        all_tools.extend(self.skill_tool_definitions());
        all_tools
```

### Step 3: Add to the agentic loop's tool list in `process_message`

In `process_message`, find the block that builds `all_tools` (around line 136–139):

```rust
        // Gather all tool definitions
        let mut all_tools: Vec<ToolDefinition> = tools::builtin_tool_definitions();
        all_tools.extend(self.mcp.tool_definitions());
        all_tools.extend(self.memory_tool_definitions());
        all_tools.extend(self.scheduling_tool_definitions());
```

Add one line after the last `extend`:

```rust
        all_tools.extend(self.skill_tool_definitions());
```

### Step 4: Verify compilation

```bash
cargo check 2>&1
```

Expected: no errors.

### Step 5: Commit

```bash
git add src/agent.rs
git commit -m "feat: add write_skill_file and reload_skills tool definitions"
```

---

## Task 5: Handle write_skill_file and reload_skills in execute_tool

**Files:**
- Modify: `src/agent.rs`

### Step 1: Add handlers in `execute_tool`

In `execute_tool`, find the arm:

```rust
            _ if self.mcp.is_mcp_tool(name) => match self.mcp.call_tool(name, arguments).await {
```

Insert the two new arms **immediately before** that line:

```rust
            "write_skill_file" => {
                let skill_name = match arguments["skill_name"].as_str() {
                    Some(n) => n.to_string(),
                    None => return "Missing skill_name".to_string(),
                };
                let relative_path = match arguments["relative_path"].as_str() {
                    Some(p) => p.to_string(),
                    None => return "Missing relative_path".to_string(),
                };
                let content = arguments["content"].as_str().unwrap_or("").to_string();

                if let Err(e) = validate_skill_name(&skill_name) {
                    return format!("Invalid skill_name: {}", e);
                }
                if let Err(e) = validate_skill_path(&relative_path) {
                    return format!("Invalid relative_path: {}", e);
                }

                let target = self
                    .config
                    .skills
                    .directory
                    .join(&skill_name)
                    .join(&relative_path);

                if let Some(parent) = target.parent() {
                    if let Err(e) = tokio::fs::create_dir_all(parent).await {
                        return format!("Failed to create directories: {}", e);
                    }
                }

                match tokio::fs::write(&target, &content).await {
                    Ok(()) => {
                        info!("Skill file written: {}", target.display());
                        format!("Written: {}", target.display())
                    }
                    Err(e) => format!("Failed to write skill file: {}", e),
                }
            }
            "reload_skills" => {
                use crate::skills::loader::load_skills_from_dir;
                match load_skills_from_dir(&self.config.skills.directory).await {
                    Ok(new_registry) => {
                        let count = new_registry.len();
                        let mut skills = self.skills.write().await;
                        *skills = new_registry;
                        info!("Skills reloaded: {} skill(s) active", count);
                        format!("Skills reloaded. {} skill(s) now active.", count)
                    }
                    Err(e) => format!("Failed to reload skills: {}", e),
                }
            }
```

### Step 2: Verify compilation

```bash
cargo check 2>&1
```

Expected: no errors.

### Step 3: Run all tests

```bash
cargo test 2>&1
```

Expected: all tests pass (including the validation tests from Task 1).

### Step 4: Commit

```bash
git add src/agent.rs
git commit -m "feat: handle write_skill_file and reload_skills in execute_tool"
```

---

## Task 6: Create skills/creating-skills/SKILL.md

**Files:**
- Create: `skills/creating-skills/SKILL.md`

### Step 1: Create the directory

```bash
mkdir -p skills/creating-skills
```

### Step 2: Create the skill file

Create `skills/creating-skills/SKILL.md` with this exact content:

````markdown
---
name: creating-skills
description: Use when the user asks to create, add, or write a new bot skill, or wants to teach the bot a new behavior, capability, or workflow.
tags: [skills, meta]
---

# Creating Skills

Writes new bot skills as properly-formatted multi-file directories in `skills/` and activates them immediately without restarting the bot.

## When to Use

- "Create a skill for X"
- "Teach the bot to Y"
- "Add a skill that does Z"
- "Write a skill for [topic]"

## Process

### 1. Gather Requirements

Ask the user (one question at a time if unclear):
- **Name**: Slug for the skill directory — lowercase letters, numbers, hyphens, e.g. `processing-reports`
- **Trigger**: When should this activate? (becomes the `description` field's "Use when...")
- **Behavior**: What should the agent do step-by-step?
- **Files**: Does it need supporting files (reference docs, templates, scripts)?

### 2. Design the Structure

Plan the directory before writing:

```
skills/<name>/
├── SKILL.md           # Always required — main entry point
├── reference.md       # Optional: heavy reference content (100+ lines)
├── examples.md        # Optional: input/output examples
└── scripts/           # Optional: utility scripts
    └── helper.py
```

Rules from official best practices:
- References must be **one level deep** from SKILL.md — no chained references
- Split into separate files only when SKILL.md would exceed ~500 lines
- SKILL.md body should be concise — it loads into context on every trigger

### 3. Write SKILL.md

SKILL.md must follow this format:

```markdown
---
name: skill-name-with-hyphens
description: Use when [specific triggering conditions — third person, no workflow summary]
tags: [optional, tags]
---

# Skill Title

Brief overview (1-2 sentences).

## When to Use

- Trigger condition 1
- Trigger condition 2

## [Core Instructions]

[Clear, action-oriented, imperative instructions]

## Supporting Files (if any)

**Topic A**: See [reference.md](reference.md)
**Topic B**: See [examples.md](examples.md)
```

**Frontmatter rules:**
- `name`: lowercase letters, numbers, hyphens only; max 64 chars; avoid "anthropic" / "claude"
- `description`: starts with "Use when..."; third person; no workflow summary; max 1024 chars
- `tags`: optional list

**Body rules:**
- Under 500 lines total
- Action-oriented and imperative
- Consistent terminology throughout
- No time-sensitive information

### 4. Write Files

Call `write_skill_file` once per file. Always write `SKILL.md` first:

```
write_skill_file(skill_name="my-skill", relative_path="SKILL.md", content="---\nname: ...")
write_skill_file(skill_name="my-skill", relative_path="reference.md", content="# Reference\n...")
```

### 5. Activate

Call `reload_skills` immediately after all files are written.

Tell the user:
- The skill is now live (no restart needed)
- Which files were created
- What trigger phrase activates it

## Description Writing Guide

```yaml
# ✅ Good — triggering conditions only, third person
description: Use when the user asks to generate weekly reports, export data summaries, or create formatted output from raw data.

# ✅ Good — specific triggers
description: Use when analyzing code for bugs, reviewing pull requests, or the user asks for a code review.

# ❌ Bad — summarizes workflow (causes Claude to skip reading the skill body)
description: Use when creating reports — reads data, formats it, writes to file.

# ❌ Bad — first person
description: I help users create reports from their data.
```
````

### Step 3: Verify the skill loads

```bash
cargo check 2>&1
```

The skill is a plain file — no compilation needed. The bot will load it at startup.

### Step 4: Commit

```bash
git add skills/creating-skills/SKILL.md
git commit -m "feat: add creating-skills skill for agent self-authoring"
```

---

## Task 7: Lint, format, and push

**Files:** none (verification only)

### Step 1: Format

```bash
cargo fmt
```

Expected: no output (or only whitespace changes).

### Step 2: Clippy

```bash
cargo clippy -- -D warnings 2>&1
```

Expected: no warnings. If any warnings appear, fix them before continuing.

Common clippy issues to watch for:
- `dead_code` on `validate_skill_name` / `validate_skill_path` (they're used in `execute_tool`, so should be fine)
- If `validate_skill_name` is only used in tests + execute_tool, clippy may flag it — suppress with `#[allow(dead_code)]` only if needed

### Step 3: Run all tests

```bash
cargo test 2>&1
```

Expected: all tests pass.

### Step 4: Commit any fmt changes

```bash
git add -u
git diff --cached --quiet || git commit -m "style: cargo fmt"
```

### Step 5: Push branch

```bash
git push -u origin claude/agent-skill-writer-SJS5d
```

Expected: push succeeds. If network failure, retry with exponential backoff (2s, 4s, 8s, 16s).

---

## Verification Checklist

After all tasks complete:
- [ ] `cargo check` passes with no errors
- [ ] `cargo clippy -- -D warnings` passes with no warnings
- [ ] `cargo fmt --check` passes
- [ ] `cargo test` passes (all tests including the new validation tests)
- [ ] `skills/creating-skills/SKILL.md` exists and has valid YAML frontmatter
- [ ] `src/agent.rs` has `skills: tokio::sync::RwLock<SkillRegistry>`
- [ ] `src/agent.rs` has `build_system_prompt` as `async fn`
- [ ] `process_message` refreshes `messages[0]` for existing conversations
- [ ] `write_skill_file` and `reload_skills` are handled in `execute_tool`
- [ ] Both tools appear in `all_tool_definitions()` and the agentic loop
- [ ] Branch pushed to `claude/agent-skill-writer-SJS5d`
