# Changelog Guidelines

## Format

Each entry is a JSON file in `.changes/` with two fields:

```json
{
  "type": "added|changed|fixed|removed|deprecated|security",
  "description": "Concise, customer-facing description"
}
```

Create entries with: `./scripts/new-change.sh <type> "<description>"`

## When to create an entry

See `.changes/VISIBILITY.md` for rules on what counts as user-facing vs internal.

## Rules

1. Do NOT start the description with the type verb (Added, Fixed, Changed, etc.) — the type is shown as a section header
2. Start with a capital letter (unless it begins with a code reference like `/command` or `setting.name`)
3. One change per entry — don't join multiple changes with "and"; create separate files instead
4. Keep it concise — one sentence, under 100 characters when possible
5. Focus on what users will notice, not internal details
6. Mention the specific feature or command affected
7. No trailing period
8. Wrap code references in backticks: slash commands (`/settings`), tool names (`fs_write`), settings (`chat.showThinking`), env vars (`KIRO_HOME`), flags (`--resume`)
9. Prefix with `[V3]` only if the change exclusively affects V3 (KAS agent engine) features — e.g. specs, workflows, cloud sessions. Do NOT use `[V3]` for shared TUI changes that affect both V2 (Rust agent) and V3

## Good examples

- **added**: `/rewind` to jump back to an earlier prompt in a conversation and continue in a new session
- **added**: Support per-model default settings in cli.json that apply across all new sessions
- **added**: Configurable keybindings for V2 TUI cancel, close menu, and quit actions
- **changed**: Reduced workspace initialization time by 88% (652ms → 76ms) by moving file watching setup to background
- **changed**: Show actionable remediation steps when MCP is disabled due to a profile API failure
- **changed**: Shell escape (`!`) commands now use the user's default shell from `$SHELL` instead of hardcoded bash
- **fixed**: Hardened pattern search and rewrite against tree-sitter parser panics for unsupported languages
- **fixed**: Reset context manager on `/clear` so loaded skills revert to frontmatter-only
- **fixed**: Prioritize built-in commands over skills in slash command autocomplete
- **security**: Enforce MCP governance (Kiro console MCP toggle) in V2 TUI mode for enterprise and API key users

## Bad examples

- ❌ `"Fixed MCP server env variables being overridden"` — starts with type verb
- ❌ `"Added /settings terminal to enable Shift+Enter"` — starts with type verb
- ❌ `"MCP env vars overridden, and fixed infinite retry loop"` — multiple changes; split into two entries
- ❌ `"changed shell escape to use default shell"` — not capitalized
