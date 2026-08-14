#!/bin/sh

set -eu

# Check if typeshare is installed
if ! command -v typeshare > /dev/null 2>&1; then
    echo "typeshare-cli not found"
    echo "Install it with: cargo install typeshare-cli"
    exit 1
fi

# Generate TypeScript types from Rust using typeshare
# E2E test types
typeshare crates/chat-cli-v2 crates/kiro-telemetry-host --lang=typescript --output-file=packages/tui/e2e_tests/types/chat-cli.ts
typeshare crates/agent --lang=typescript --output-file=packages/tui/e2e_tests/types/agent.ts
typeshare crates/kiro-telemetry-schema --lang=typescript --output-file=packages/tui/e2e_tests/types/telemetry.ts

# Shared types for main source
typeshare crates/agent --lang=typescript --output-file=packages/tui/src/types/generated/agent.ts
typeshare crates/chat-cli-v2 crates/kiro-telemetry-host --lang=typescript --output-file=packages/tui/src/types/generated/chat-cli.ts
# Wire types for the hidden `chat _` internal subcommand surface (CliInternalOutput, ErrorCode).
typeshare crates/chat-cli --lang=typescript --output-file=packages/tui/src/types/generated/chat-internal.ts
typeshare crates/kiro-telemetry-schema --lang=typescript --output-file=packages/tui/src/types/generated/telemetry.ts

echo "✓ Generated types at packages/tui/e2e_tests/types/"
echo "✓ Generated types at packages/tui/src/types/generated/"
