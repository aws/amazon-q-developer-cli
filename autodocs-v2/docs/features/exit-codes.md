---
doc_meta:
  title: Exit Codes
  description: Exit codes returned by kiro-cli commands for scripting and automation
  category: feature
  keywords: [exit, code, status, automation, scripting, CI, headless]
  related: [chat, hooks]
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: true
---

## Overview

Kiro CLI returns standard exit codes that can be used in scripts and CI/CD pipelines.

## Exit Codes

| Code | Meaning | Description |
|------|---------|-------------|
| 0 | Success | Command completed successfully |
| 1 | Failure | Unhandled error, API failure, or runtime error |
| 2 | Argument Error | Invalid CLI arguments (from clap parser) |

## Examples

### Headless mode in CI

```bash
kiro-cli chat --no-interactive --trust-all-tools "Run analysis"
echo "Exit code: $?"
```

### Script with error handling

```bash
if kiro-cli chat --no-interactive "Task"; then
  echo "Success"
else
  echo "Failed with exit code $?"
fi
```

### Using with hooks

```bash
kiro-cli chat --no-interactive --agent my-agent "Run checks"
exit_code=$?
if [ $exit_code -ne 0 ]; then
  echo "Kiro CLI failed with code $exit_code"
fi
```

## Troubleshooting

### Exit code 1

General failure. Check stderr for error details. Common causes: network issues, authentication expired, API errors, internal error.

### Exit code 2

CLI argument parsing error. Check your command syntax. Common causes: unknown flags, missing required arguments, invalid option values.

## Related

- [kiro-cli chat](../commands/chat.md) — Chat command with `--no-interactive` for automation
- [Hooks](hooks.md) — Automated commands triggered during agent lifecycle
