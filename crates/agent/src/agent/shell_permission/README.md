# Shell Permission System

The shell permission system provides multi-layer security evaluation for shell commands, protecting against command injection, environment poisoning, and other shell-based attacks.

## Architecture

The system uses a 3-layer evaluation approach:

```
┌─────────────────────────────────────────────────────────────┐
│                    Command Input                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 1: Parse                                    [PR1 ✓]  │
│  - Parse with tree-sitter (bash grammar)                    │
│  - Split chained commands (&&, ||, ;, |)                    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 2: Detect (for each command)                [PR2 ✓]  │
│  - Dangerous patterns ($(), ``, >, find -exec, etc.)        │
│  - Readonly command check                                   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: Decide                                   [Future] │
│  - Apply policy rules (allowedCommands, deniedCommands)     │
│  - Apply user settings (denyByDefault, autoAllowReadonly)   │
│  - Aggregate results (most restrictive wins)                │
└─────────────────────────────────────────────────────────────┘
```

## Module Structure

```
crates/agent/src/agent/shell_permission/
├── mod.rs              # Public API, core types
├── parser.rs           # Tree-sitter command parsing [PR1 ✓]
├── detector.rs         # Layer 2: Detection [PR2 ✓]
└── README.md           # This file
```

## Layer 1: Parse (Implemented)

Uses tree-sitter with bash grammar to accurately parse shell commands.

### Features
- Splits chained commands (`&&`, `||`, `;`, `|`)
- Detects redirections (`>`, `>>`, `<`)
- Identifies command substitution (`$()`, backticks)
- Handles quoted strings correctly
- Detects heredocs and process substitution

## Layer 2: Detect (Implemented)

Analyzes parsed commands to determine danger level and readonly status.

### Features
- Detects dangerous command options (`find -exec`, `grep -P`, `sed /e`)
- Detects environment manipulation (`export PAGER=...`)
- Identifies readonly commands for auto-allow
- Detects pipe-to-shell patterns (`curl | bash`)
- Configuration-driven via `detector_config.json`

## Layer 3: Decide (Future)

*To be implemented in future PR*

Will apply:
- Policy rules from agent configuration
- User settings (`denyByDefault`, `autoAllowReadonly`)
- Result aggregation for chained commands

## Appendix: 

### Changes from Kiro CLI 1.0
Kiro CLI 2.0 enhances the shell permission system compared to Kiro CLI v1. The new implementation addresses gaps discovered through CVE research and incorporates best practices from industry tools.

#### 1. Tree-sitter Parsing
**v1**: Used regex to split commands by `|` character.
**v2**: Uses tree-sitter with bash grammar for accurate AST-based parsing.

Benefits:
- Correctly handles quoted strings: `echo "hello | world"` is one command
- Detects heredocs and doesn't flag content as dangerous
- Identifies subshells and command substitutions structurally
- Handles escape sequences properly

#### 2. Environment Manipulation Detection
**v1**: No detection of environment poisoning attacks.
**v2**: Detects and blocks shell built-ins that manipulate environment.

Addresses CVE-2026-22708 (Cursor IDE):
- Blocks `export PAGER="malicious"` → DENY
- 50+ dangerous environment variables auto-denied
- Safe env manipulation requires user approval

#### 3. Enhanced Dangerous Pattern Detection
**v1**: Limited to basic dangerous patterns via checks on raw command text.
**v2**: Incorporates patterns from CVE-2025-66032 (Claude Code) research. Detection based on flags set by parser.

New detections like:
- `sed` with `e` flag (execute)
- Bash `@P` prompt expansion

#### 4 Suset of Git subcommands to read only list
**v1**: `git` commands always required approval.
**v2**: `git` read operations are auto-allowed.

New safe options:
- `git status`, `git log`, `git diff`, `git show`

Dangerous optinos options still need approval:
- `--upload-pack`, `--receive-pack`, `--exec`

## References
- [Shell Permission Research](../../../docs/research/shell-permission-research.md)
