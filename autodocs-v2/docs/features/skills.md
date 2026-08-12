---
doc_meta:
  validated: 2026-08-12
  commit: e49533c92
  status: validated
  testable_headless: true
  category: feature
  title: Skills
  description: Reusable prompt templates invocable as slash commands from .kiro/skills/
  keywords: [skill, skills, slash command, prompt, template, SKILL.md, frontmatter]
  related: [agent-configuration, prompts, context]
---

# Skills

Reusable prompt templates stored in `.kiro/skills/` that can be invoked as slash commands.

## Overview

Skills are markdown files that define reusable prompts. Unlike regular prompts, skills are loaded on demand and invoked directly as `/skill-name` slash commands. They support YAML frontmatter for metadata and template arguments for dynamic content.

## File Location

Skills are discovered from paths matching `skill://` resources in agent configuration.

**Default locations** (from default agent):
- `.kiro/skills/*/SKILL.md` — workspace skills
- `~/.kiro/skills/*/SKILL.md` — global skills

**Directory structure**:
```
.kiro/
└── skills/
    └── greet/
        └── SKILL.md
```

The skill name defaults to the parent directory name (e.g., `greet`).

## Skill File Format

Skills are markdown files with optional YAML frontmatter:

```markdown
---
name: greet
description: Say hello to someone
---

# Greeting

Say hello to $ARGUMENTS in a friendly way.
```

### Frontmatter Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | No | Skill name (defaults to parent directory name) |
| `description` | No | Description shown in command completion |

### Template Arguments

Skills support the `$ARGUMENTS` placeholder which expands to any text provided after the skill name:

```markdown
---
name: review
description: Review code changes
---

Review the following code for issues:

$ARGUMENTS
```

When invoked as `/review src/main.rs`, the `$ARGUMENTS` placeholder expands to `src/main.rs`.

## Invoking Skills

Skills appear as slash commands and can be invoked in several ways:

### Direct invocation

```
/greet World
```

Invokes the `greet` skill with "World" as the argument.

### Tab completion

```
/gre<Tab>
```

Auto-completes to available skills matching the prefix.

### From prompts menu

```
/prompts
```

Skills appear alongside MCP and file prompts in the selection menu.

## Examples

### Example 1: Simple greeting skill

Create `.kiro/skills/greet/SKILL.md`:

```markdown
---
name: greet
description: Say hello to someone
---

# Greeting

Say hello to $ARGUMENTS in a friendly, enthusiastic way.
```

Invoke:
```
/greet Alice
```

The model receives:
```
# Greeting

Say hello to Alice in a friendly, enthusiastic way.
```

### Example 2: Code review skill

Create `.kiro/skills/review/SKILL.md`:

```markdown
---
name: review
description: Review code for issues and improvements
---

# Code Review

Please review the following for:
- Bugs and logic errors
- Security issues
- Performance concerns
- Code style

$ARGUMENTS
```

Invoke:
```
/review the changes in src/auth.rs
```

### Example 3: Skill without frontmatter

Create `.kiro/skills/explain/SKILL.md`:

```markdown
# Explain

Explain $ARGUMENTS in simple terms that a beginner would understand.
Use analogies and examples where helpful.
```

The skill name is `explain` (from directory name). No description appears in completion.

## Resolution Priority

When a slash command is invoked, the system resolves it in this order:

1. **Local file prompts** — `.kiro/prompts/*.md`
2. **Global file prompts** — `~/.kiro/prompts/*.md`
3. **Skills** — from `skill://` resources
4. **MCP prompts** — from configured MCP servers

If a skill has the same name as a file prompt, the file prompt takes precedence.

## Configuring Skill Resources

Skills are discovered from `skill://` resources in agent configuration. The default agent includes:

```json
{
  "resources": [
    "skill://.kiro/skills/*/SKILL.md"
  ]
}
```

To add custom skill paths, create or edit an agent configuration:

```json
{
  "name": "my-agent",
  "resources": [
    "skill://.kiro/skills/*/SKILL.md",
    "skill://~/shared-skills/**/SKILL.md"
  ]
}
```

## Troubleshooting

### Skill not appearing as slash command

**Cause**: Skill path not matched by `skill://` resource pattern.

**Solution**: Verify the skill file location matches a `skill://` pattern in your agent config. Check with `/context` to see loaded resources.

### Skill name conflicts with prompt

**Cause**: A file prompt in `.kiro/prompts/` has the same name.

**Solution**: File prompts take precedence. Rename either the skill or the prompt.

### Frontmatter appearing in output

**Cause**: Malformed YAML frontmatter.

**Solution**: Ensure frontmatter starts with `---` on line 1 and ends with `---` on its own line.

### $ARGUMENTS not expanding

**Cause**: Placeholder must be exactly `$ARGUMENTS` (case-sensitive).

**Solution**: Check spelling and case. Use `$ARGUMENTS` not `$arguments` or `$ARGS`.

### Skill not found error

**Cause**: Skill name contains invalid characters.

**Solution**: Skill names must contain only alphanumeric characters, hyphens, and underscores. No spaces or special characters.

## Related

- [Agent Configuration](agent-configuration.md) — Configure skill resources
- [/prompts](../slash-commands/prompts.md) — View all available prompts including skills
- [/context](../slash-commands/context.md) — View loaded resources including skills
