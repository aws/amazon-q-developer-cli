# KIRO CLI

## Installation

Please use this command to install kiro. 
``` shell
curl -fsSL https://cli.kiro.dev/install | bash
```

## Development Setup

After cloning the repository, run the setup script to install git hooks:

```bash
./scripts/setup-hooks.sh
```

This enables:
- Pre-commit hook that runs `cargo fmt` and `cargo clippy`
- Pre-push hook that reminds you to update documentation when source files change

To update docs, run: `kiro-cli chat --agent docs`

## Development Workflows

Kiro-cli includes prompt-based workflows in `.kiro/prompts/` to accelerate development:

### Available Workflows

| Command | Purpose | Output |
|---------|---------|--------|
| `@research` | Document codebase, answer questions | `thoughts/shared/research/YYYY-MM-DD-description.md` |
| `@plan` | Create implementation plans interactively | `thoughts/shared/plans/YYYY-MM-DD-description.md` |
| `@implement` | Execute approved plans phase-by-phase | Code changes with verification |
| `@validate` | Verify implementation against plan | Validation report |
| `@commit` | Create git commits with proper formatting | Git commits |

### Typical Workflow

1. **Research** - `@research` to understand existing code
2. **Plan** - `@plan` to create implementation plan
3. **Implement** - `@implement` to execute plan phase-by-phase
4. **Validate** - `@validate` to verify implementation
5. **Commit** - `@commit` to create git commits

### Key Features

- **Code tool emphasis**: Use `code` tool for symbol discovery, `grep` for text patterns
- **Autodocs integration**: Check knowledge base before deep diving
- **Todo list tracking**: Track progress through multi-phase implementations
- **Cargo commands**: All prompts reference correct build/test commands
- **Phase-by-phase**: Implement and verify one phase at a time

### Research Workflow

Use `@research` to understand how features work:
- Searches autodocs knowledge base first
- Uses `code` tool for symbol discovery
- Uses `grep` for text patterns
- Documents findings with file:line references

### Planning Workflow

Use `@plan` to create implementation plans:
- Interactive process with clarifying questions
- Spawns research tasks to understand codebase
- Separates automated vs manual verification
- Outputs detailed plans with success criteria

### Implementation Workflow

Use `@implement` to execute plans:
- Creates todo list to track progress
- Implements phase-by-phase with verification
- Updates checkboxes in plan as sections complete
- Pauses for manual verification between phases

## Security

For security related concerns, see [here](SECURITY.md).

## Licensing

This repo is dual licensed under MIT and Apache 2.0 licenses.

Those licenses can be found [here](LICENSE.MIT) and [here](LICENSE.APACHE).

“Amazon Web Services” and all related marks, including logos, graphic designs, and service names, are trademarks or trade dress of AWS in the U.S. and other countries. AWS’s trademarks and trade dress may not be used in connection with any product or service that is not AWS’s, in any manner that is likely to cause confusion among customers, or in any manner that disparages or discredits AWS.
