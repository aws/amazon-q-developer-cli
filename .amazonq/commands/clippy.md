# Clippy

Run clippy tests used in CI pipeline

## Instructions

Run the following clippy, tests, format, and fix related errors

1. Step 1: `cargo clippy --locked --workspace --color always -- -D warnings`
2. Step 2: `cargo clippy --locked -p chat_cli --color always -- -D warnings`
3. Step 3: `cargo test --locked -p chat_cli`
4. Step 4: `cargo test --locked --workspace --lib --bins --test '*' --exclude fig_desktop-fuzz`
5. Step 5: `cargo +nightly fmt --check -- --color always`

## Context

All clippy, tests, and format commands must pass without errors after each
step. ask questions for any changes that have multiple optins to fix and do not
assume one way to fix. When different error handling or implementation patterns
are found, review project rules and design documents in context or knowledge to identify the
correct pattern.

