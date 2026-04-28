---
doc_meta:
  title: steering-files
  description: Markdown files that provide persistent instructions and rules to guide agent behavior across sessions
  category: feature
  keywords: [steering, rules, instructions, context, guidelines, conventions, kiro, configuration]
  related: [agent-configuration, context]
  validated: 2026-04-28
  commit: 5ddb3cf4
  status: validated
  testable_headless: true
---

# Steering Files

Markdown files that provide persistent instructions and rules to guide agent behavior across sessions.

## Overview

Steering files are markdown files placed in `.kiro/steering/` directories that are automatically loaded into the agent's context. They act as persistent rules, conventions, and instructions that shape how the agent behaves — similar to coding guidelines, project conventions, or team-specific instructions that you want the agent to follow every time.

Unlike agent prompts (which define the agent's identity), steering files define the project or workspace rules the agent should follow. They are loaded as `file://` resources, meaning their full content is always present in the agent's context window.

## File Locations

Steering files are loaded from two locations:

| Location | Scope | Example Path |
|----------|-------|-------------|
| **Workspace** (local) | `.kiro/steering/` in current directory | `./my-project/.kiro/steering/coding-standards.md` |
| **Global** (user-wide) | `~/.kiro/steering/` in home directory | `~/.kiro/steering/general-rules.md` |

Both directories are scanned recursively — any `.md` file under `.kiro/steering/` at any depth is included.

**Resolution order**: Both workspace and global steering files are loaded. If the workspace `.kiro/steering/` directory is the same as the global one (e.g., when running from the home directory), duplicates are automatically skipped.

## When Steering Files Are Loaded

Steering files are loaded automatically by the **built-in default agent** (`kiro_default`). This happens when:

- You start `kiro-cli chat` without specifying an agent
- You use the default agent via `chat.defaultAgent` setting
- The `.kiro/steering/` directory exists and contains `.md` files

**Custom agents** do not automatically load steering files. To include them in a custom agent, add them explicitly to the agent's `resources`:

```json
{
  "resources": [
    "file://.kiro/steering/**/*.md"
  ]
}
```

## Creating Steering Files

### Basic Steering File

Create a markdown file in `.kiro/steering/`:

```markdown
# Coding Conventions

- Use snake_case for function names
- All public functions must have doc comments
- Prefer returning Result over panicking
- Maximum function length: 50 lines
```

No frontmatter is required. The file content is loaded directly into the agent's context.

### Steering File with Inclusion Control

Steering files support optional YAML frontmatter with an `inclusion` field that controls when the file is loaded:

```markdown
---
inclusion: always
---

# Always-Loaded Rules

These rules are loaded into every session.
```

**Inclusion modes:**

| Value | Behavior |
|-------|----------|
| `always` | Always loaded (default behavior, same as no frontmatter) |
| `fileMatch` | Only loaded when working with files matching a pattern (excluded from automatic loading) |
| `manual` | Only loaded when explicitly added via `/context add` (excluded from automatic loading) |
| *(no frontmatter)* | Always loaded |
| *(malformed frontmatter)* | Always loaded (fails open) |

Files with `inclusion: fileMatch` or `inclusion: manual` are **excluded** from automatic context loading. They exist in the steering directory but are only activated through other mechanisms.

## Examples

### Example 1: Project-Wide Coding Standards

`.kiro/steering/coding-standards.md`:
```markdown
# Project Coding Standards

## Rust
- Use `thiserror` for error types, not `anyhow` in library crates
- All async functions should be cancellation-safe
- Prefer `&str` over `String` in function parameters

## TypeScript
- Use strict mode
- Prefer `const` over `let`
- No `any` types — use `unknown` and narrow
```

### Example 2: Global Personal Preferences

`~/.kiro/steering/preferences.md`:
```markdown
# My Preferences

- Be concise — skip filler phrases
- When writing code, include comments only for non-obvious logic
- Prefer functional style over imperative when readability is equal
- Always suggest tests when adding new functions
```

### Example 3: Team Conventions

`.kiro/steering/team-conventions.md`:
```markdown
# Team Conventions

- PR titles follow conventional commits: feat:, fix:, chore:, docs:
- All PRs need at least one approval before merge
- Changelog entries required for user-facing changes (use `no-changelog` label to skip)
- Documentation lives in autodocs-v2/ for V2 features
```

### Example 4: Conditional Steering (fileMatch)

`.kiro/steering/rust-safety.md`:
```markdown
---
inclusion: fileMatch
---

# Rust Safety Rules

- No `unsafe` blocks without a safety comment
- No `.unwrap()` in production code — use `?` or `.expect("reason")`
- All `todo!()` must have a tracking issue
```

This file exists in the steering directory but is not automatically loaded. It would need to be activated through file-matching logic or manual inclusion.

## How It Works

1. On agent initialization, Kiro CLI checks if `.kiro/steering/` exists in the workspace and/or global home directory
2. If found, adds `file://<steering_dir>/**/*.md` to the agent's resource list
3. Each `.md` file is read and checked for frontmatter inclusion rules
4. Files with `inclusion: always`, no frontmatter, or malformed frontmatter are included
5. Files with `inclusion: fileMatch` or `inclusion: manual` are excluded
6. Included files are loaded as `file://` resources — their full content is always in the context window

## Viewing Steering Files in Context

Use `/context show` to see which steering files are loaded:

```
/context show
```

Steering files appear under the agent's context files section with their full paths.

## Troubleshooting

### Issue: Steering Files Not Loading

**Symptom**: Rules in steering files are not being followed

**Possible causes**:
- Directory doesn't exist: create `.kiro/steering/` in your project or home directory
- Using a custom agent: custom agents don't load steering files automatically — add `"file://.kiro/steering/**/*.md"` to the agent's `resources`
- File has `inclusion: fileMatch` or `inclusion: manual` frontmatter: these are excluded from automatic loading
- File is not `.md`: only markdown files are loaded

### Issue: Steering File Too Large

**Symptom**: Context window filling up quickly

**Cause**: Steering files are loaded as `file://` resources (always in context, every turn)

**Solution**: Keep steering files concise. Move detailed reference material to `skill://` resources instead, which load on demand.

### Issue: Duplicate Content from Global and Workspace

**Symptom**: Same rules appearing twice in context

**Cause**: Running from home directory where workspace and global steering directories are the same

**Solution**: This is handled automatically — Kiro CLI deduplicates when the workspace and global steering directories resolve to the same canonical path.

## Best Practices

- **Keep files focused**: One topic per file (coding standards, PR conventions, testing rules)
- **Keep files concise**: Every line consumes context window space on every turn
- **Use global for personal preferences**: `~/.kiro/steering/` for rules you want everywhere
- **Use workspace for project rules**: `.kiro/steering/` for project-specific conventions
- **Commit workspace steering files**: They're part of your project configuration — share with the team via version control
- **Don't duplicate agent prompt content**: Steering files complement the agent prompt, not replace it

## Related

- [Agent Configuration](agent-configuration.md) — Define agent behavior, tools, and resources
- [/context](../slash-commands/context.md) — View and manage context files
