---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli diagnostic
  description: Run diagnostic tests and generate system information report for troubleshooting
  keywords: [diagnostic, test, troubleshoot, debug, system]
  related: [logdump]
---

# kiro-cli diagnostic

Run diagnostic tests and generate system information report for troubleshooting.

## Overview

The diagnostic command runs system diagnostics and generates a report with environment information, configuration status, and potential issues. Useful for troubleshooting problems or providing information to support.

## Usage

### Basic Usage

```bash
kiro-cli diagnostic
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
| `--format <FORMAT>` | `-f` | Output format: plain (markdown), json, json-pretty (default: plain) |
| `--force` | | Force limited diagnostic output |
| `--verbose` | `-v` | Increase logging verbosity (can be repeated) |
| `--help` | `-h` | Print help information |

## Output Information

Diagnostic report includes:
- System information (OS, architecture)
- Kiro CLI version
- Configuration status
- Environment variables
- Installed dependencies
- Potential issues

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
variant = "full"

[system-info]
os = "macOS 15.7.1 (24G231)"
chip = "Apple M1 Pro"
total-cores = 10
memory = "32.00 GB"

[environment]
cwd = "/Users/user/project"
cli-path = "/Users/user/.cargo/bin/kiro-cli"
os = "Mac"
shell-path = "/bin/bash"
shell-version = "5.1.16"
terminal = "iTerm2"
install-method = "cargo"

[env-vars]
PATH = "..."
SHELL = "/bin/zsh"
TERM = "xterm-256color"
...
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

- [/logdump](../slash-commands/logdump.md) - Create log archive
- [kiro-cli whoami](whoami.md) - User information

## Limitations

- Some checks may require network access
- Limited output with --force flag
- May not detect all issues

## Technical Details

**Checks Performed**:
- Version information
- File system paths
- Configuration validity
- Dependency availability
- Environment variables

**Output Formats**: plain (human-readable), json, json-pretty

**Spinner**: Shows progress indicator in terminal mode
