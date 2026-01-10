## Testing

- Run single test: `cargo test -p chat_cli --bin chat_cli cli::chat::cli::persist::tests::test_save_and_load_file` or - `cargo test -p agent --lib test_mcp_server_config_stdio_deser`
- Run all tests in a module: `cargo test -p chat_cli --bin chat_cli persist::tests` (all tests in a module)

## Setup

After cloning the repository, run the setup script to install git hooks:

```bash
./scripts/setup-hooks.sh
```

This will install pre-commit hooks that run `cargo fmt` and `cargo clippy` checks before each commit.

## Common Commands

```bash
# Linting
cargo clippy --locked --workspace --color always -- -D warnings

# Formatting
cargo +nightly fmt
cargo +nightly fmt --check -- --color always

# Running
cargo run --bin chat_cli --
```

## Log Files

**macOS/Linux**: `$TMPDIR/kiro-log/kiro-chat.log` (or `$XDG_RUNTIME_DIR/kiro-log/kiro-chat.log`)
**Windows**: `%TEMP%/kiro-log/logs/kiro-chat.log`

MCP logs: Same directory, `mcp.log`

