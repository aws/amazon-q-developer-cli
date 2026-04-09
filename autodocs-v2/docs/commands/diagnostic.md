---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli diagnostic
  description: Run diagnostic tests and generate system information report for troubleshooting
  keywords: [diagnostic, diagnostics, test, troubleshoot, debug, system]
  related: [feedback, whoami]
---

# kiro-cli diagnostic

Run diagnostic tests and generate system information report for troubleshooting.

## Overview

The diagnostic command collects system diagnostics and generates a report with build details, system information, environment details, and relevant environment variables. Useful for troubleshooting problems or providing information to support.

## Usage

```bash
kiro-cli diagnostic
kiro-cli diagnostics  # alias
```

### With Output Format

```bash
kiro-cli diagnostic --format json
kiro-cli diagnostic --format json-pretty
```

### Force Limited Output

```bash
kiro-cli diagnostic --force
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--format <FORMAT>` | `-f` | Output format: plain (TOML), json, json-pretty (default: plain) |
| `--force` | | Force limited diagnostic output |
| `--help` | `-h` | Print help information |

## Output Information

Diagnostic report includes:
- Build details (version, commit hash, build date)
- System info (OS, chip, cores, memory)
- Environment (cwd, cli-path, install-method, terminal)
- Conditional flags: in-ssh, in-ci, in-wsl, in-codespaces (shown only when true)
- Relevant environment variables

## Examples

### Example 1: Generate Report

```bash
kiro-cli diagnostic
```

**Output** (TOML format):
```toml
[q-details]
version = "1.23.0"
hash = "97d58722cd90f6d3dda465f6462ee4c6dc104b22"
date = "2025-12-18T16:49:27.015389Z (4d ago)"

[system-info]
os = "macOS 15.7.1 (24G231)"
chip = "Apple M1 Pro"
total-cores = 10
memory = "32.00 GB"

[environment]
cwd = "/Users/USER/project"
cli-path = "/Users/USER/.cargo/bin/kiro-cli"
install-method = "cargo"
terminal = "iTerm2 3.5.0"

[env-vars]
PATH = "..."
SHELL = "/bin/zsh"
TERM = "xterm-256color"
```

### Example 2: JSON Output

```bash
kiro-cli diagnostic --format json-pretty
```

**Output**: Same information in JSON format.

### Example 3: Limited Output

```bash
kiro-cli diagnostic --force
```

Generates minimal diagnostic report (faster).

## Troubleshooting

### Issue: Diagnostic Hangs

**Symptom**: Command doesn't complete  
**Cause**: Checking slow system resources  
**Solution**: Use `--force` for faster limited output

### Issue: Permission Errors

**Symptom**: Errors accessing certain paths  
**Cause**: Insufficient permissions  
**Solution**: Run with appropriate permissions or ignore errors

## Related Features

- [/feedback](../slash-commands/feedback.md) - Submit feedback or report issues
- [kiro-cli whoami](whoami.md) - User information

## Technical Details

**Information Collected**:
- Build details (version, hash, date)
- System info (OS, chip, cores, memory)
- Current environment (cwd, cli path, install method, terminal, SSH/CI/WSL/Codespaces detection)
- Filtered environment variables

**Output Formats**: plain (TOML), json, json-pretty

**Spinner**: Shows progress indicator in terminal mode
