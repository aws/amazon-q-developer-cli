---
doc_meta:
  title: kiro-cli-help
  description: Fetches authoritative help and version output from kiro-cli subcommands
  category: tool
  keywords: [help, version, subcommand, cli, usage, flags]
  related: [introspect, knowledge]
  validated: 2026-05-22
  commit: 3d576d424
  status: validated
  testable_headless: true
---

# kiro_cli_help

Fetches authoritative help and version output from kiro-cli subcommands.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions about Kiro CLI commands or flags naturally, and the assistant will use this tool to provide accurate, up-to-date answers.

The `kiro_cli_help` tool runs `kiro-cli --help`, `kiro-cli <subcommand> --help`, or `kiro-cli --version` and returns the output. It provides the assistant with authoritative, real-time CLI usage information directly from the binary — unlike `introspect` which uses compile-time embedded docs.

This tool is narrowly scoped for security: it only accepts a fixed allowlist of subcommands and rejects arbitrary input, shell metacharacters, and piping. It is NOT a generic shell.

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `subcommand` | string | No | Subcommand to fetch help for (e.g. `chat`, `mcp`). Omit for top-level help. |
| `version` | boolean | No | When `true`, returns `kiro-cli --version` output. Cannot be combined with `subcommand`. |

### Allowed Subcommands

Only these subcommands are accepted:

`agent`, `chat`, `completion`, `config`, `context`, `experiment`, `help`, `hooks`, `issue`, `login`, `logout`, `mcp`, `prompts`, `settings`, `telemetry`, `tools`, `user`, `version`

Any other value is rejected with an error.

## Usage

Ask the assistant about CLI commands or flags:

```
> What flags does `kiro-cli chat` accept?
> What version of kiro-cli am I running?
> How do I use the mcp command?
```

The assistant calls this tool to get the live help text and answers based on it.

## Examples

### Example 1: Top-Level Help

```json
{}
```

Runs `kiro-cli --help` and returns the full usage listing.

### Example 2: Subcommand Help

```json
{
  "subcommand": "chat"
}
```

Runs `kiro-cli chat --help` and returns flags, options, and usage for the `chat` subcommand.

### Example 3: Version Check

```json
{
  "version": true
}
```

Runs `kiro-cli --version` and returns the installed version string.

### Example 4: Invalid Subcommand (Rejected)

```json
{
  "subcommand": "rm -rf /"
}
```

Returns an error: subcommand not in the allowlist.

## How It Differs from Introspect

| Aspect | `kiro_cli_help` | `introspect` |
|--------|-----------------|--------------|
| Source | Live binary output | Compile-time embedded docs |
| Scope | CLI flags and usage text | Feature docs, tool schemas, guides |
| Freshness | Always current | Updated at build time |
| Depth | Terse `--help` output | Rich documentation with examples |

The assistant uses both tools together: `kiro_cli_help` for precise flag listings and `introspect` for conceptual explanations.

## Permissions

This tool is auto-approved (no user confirmation needed). It can only read help text — it cannot execute arbitrary commands, modify files, or access the network.

## Troubleshooting

### "subcommand not in the allowlist"

**Symptom**: Tool returns an error when asking about a subcommand.
**Cause**: The requested subcommand is not in the fixed allowlist.
**Solution**: Use one of the allowed subcommands listed above. If a new subcommand was added to kiro-cli but isn't in the allowlist yet, use `introspect` or ask the assistant to check documentation instead.

### "pass either version or subcommand, not both"

**Symptom**: Tool rejects the request.
**Cause**: Both `version: true` and a `subcommand` were provided.
**Solution**: Use one or the other. For version info, omit `subcommand`. For subcommand help, omit `version`.

### Empty or unexpected output

**Symptom**: Tool returns empty stdout.
**Cause**: `kiro-cli` binary not found in PATH or returned an error.
**Solution**: Ensure kiro-cli is installed and accessible. Check `stderr` in the response for details.

## Related

- [introspect](introspect.md) — Embedded documentation search (semantic + BM25)
- [knowledge](knowledge.md) — Persistent knowledge base with semantic search
- [/help](../slash-commands/help.md) — Help slash command that activates the help agent
