---
doc_meta:
  title: chat.disableInheritingDefaultResources
  description: Disable automatic inheritance of default resources in custom agents
  category: setting
  keywords: [setting, agent, resources, inheritance, steering, skills, AGENTS.md, custom agent]
  related: [agent-configuration, default-agent]
  validated: 2026-06-25
  commit: e856295ea
  status: validated
  testable_headless: true
---

# chat.disableInheritingDefaultResources

Disable automatic inheritance of default resources in custom (user-defined) agents.

## Overview

By default, custom agents inherit a set of default resources — global and workspace steering files, skills, and project marker files like `AGENTS.md` and `README.md`. These resources are appended to whatever resources the agent explicitly declares.

When `chat.disableInheritingDefaultResources` is set to `true`, custom agents only use resources they explicitly declare in their configuration. Built-in agents (e.g., `kiro_default`) always inherit default resources regardless of this setting.

## Default

`false` (custom agents inherit default resources)

**Type**: Boolean  
**Scope**: Global  
**Key**: `chat.disableInheritingDefaultResources`

## Usage

### Enable via CLI

```bash
kiro-cli settings chat.disableInheritingDefaultResources true
```

### Enable via in-chat command

```
/settings set chat.disableInheritingDefaultResources true
```

### Check current value

```bash
kiro-cli settings chat.disableInheritingDefaultResources
```

### Disable (restore default behavior)

```bash
kiro-cli settings chat.disableInheritingDefaultResources false
```

Or delete the setting entirely:

```bash
kiro-cli settings --delete chat.disableInheritingDefaultResources
```

## Inherited Default Resources

When inheritance is enabled (the default), custom agents receive these resources automatically:

| Resource | Description |
|----------|-------------|
| `file://AGENTS.md` | Project agent instructions |
| `file://README.md` | Project readme |
| `skill://.kiro/skills/*/SKILL.md` | Workspace skills |
| `skill://~/.kiro/skills/*/SKILL.md` | Global skills |
| `file://~/.kiro/steering/**/*.md` | Global steering files (if directory exists) |
| `file://.kiro/steering/**/*.md` | Workspace steering files (if directory exists) |
| `file://AmazonQ.md` | Legacy project instructions (if file exists) |
| `file://.amazonq/rules/**/*.md` | Legacy rules (if `.amazonq` exists but `.kiro` doesn't) |

## Examples

### Example 1: Minimal Agent Without Inherited Context

You want an agent that only sees specific files and nothing else:

```bash
kiro-cli settings chat.disableInheritingDefaultResources true
```

Then define a focused agent in `.kiro/agents/minimal.json`:

```json
{
  "name": "minimal",
  "description": "Only sees test files",
  "tools": ["read", "write", "shell"],
  "resources": ["file://tests/**/*.rs"]
}
```

This agent loads only `tests/**/*.rs` — no `AGENTS.md`, no steering files, no skills.

### Example 2: Re-enable Inheritance

```bash
kiro-cli settings chat.disableInheritingDefaultResources false
```

The same `minimal` agent now also receives `AGENTS.md`, `README.md`, skills, and steering files.

### Example 3: Built-in Agents Are Unaffected

Even with the setting enabled, built-in agents like `kiro_default` always load the full set of default resources. This setting only affects user-created agents in `.kiro/agents/` or `~/.kiro/agents/`.

## When to Use

- **Large projects with many steering files**: When inherited context is too large for a focused agent's purpose
- **Specialized agents**: When you want precise control over what context an agent sees
- **Testing agent configs**: When verifying an agent works with only its declared resources

## Troubleshooting

### Issue: Custom Agent Missing Expected Context

**Symptom**: Agent doesn't know about project conventions or skills  
**Cause**: `chat.disableInheritingDefaultResources` is set to `true`  
**Solution**: Set to `false` or explicitly add needed resources to the agent config

### Issue: Agent Has Too Much Context

**Symptom**: Agent loads steering/skills you don't want for a specialized task  
**Cause**: Default resource inheritance is active  
**Solution**: Set `chat.disableInheritingDefaultResources` to `true` and declare only what the agent needs

### Issue: Setting Doesn't Take Effect

**Symptom**: Agents still inherit resources after enabling the setting  
**Cause**: Agent configs are loaded at startup  
**Solution**: Start a new session after changing the setting. In-session agent reloads (e.g., after `/agent create`) will pick up the new value.

## Related

- [Agent Configuration](../features/agent-configuration.md) — Full agent config format and resource declarations
- [chat.defaultAgent](default-agent.md) — Set which agent loads by default
