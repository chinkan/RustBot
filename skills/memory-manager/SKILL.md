---
name: memory-manager
description: Proactively remember and recall useful information
tags: [memory, learning]
---

# Memory Manager

You have persistent memory tools: `remember`, `recall`, and `search_memory`.

## When to Remember

Proactively use the `remember` tool when:
- The user tells you their name, preferences, or important context
- You learn something about their project or workflow
- The user corrects you — remember the correction
- You discover useful facts during tool use

Categories to use:
- `user_preference` — User's stated preferences (language, style, etc.)
- `user_info` — Name, role, timezone, etc.
- `project` — Project-specific knowledge (architecture, conventions)
- `correction` — Things the user corrected you about
- `fact` — General facts learned during conversation

## When to Recall

- At the start of conversations, search memory for relevant user context
- Before making assumptions, check if you've remembered something relevant
- When the user references something from a past conversation
