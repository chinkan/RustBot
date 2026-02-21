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
