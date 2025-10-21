#!/bin/bash
set -e

echo "🔍 Running CLI regression tests..."

# Build the CLI
cargo build --bin code-agent-cli

# Test help command
echo "Testing --help..."
cargo run --bin code-agent-cli -- --help > /dev/null

# Test find-symbol
echo "Testing find-symbol..."
cargo run --bin code-agent-cli -- find-symbol greet --file tests/samples/test.ts > /dev/null

# Test find-references
echo "Testing find-references..."
cargo run --bin code-agent-cli -- find-references --file tests/samples/test.ts --line 6 --column 20 > /dev/null

# Test goto-definition
echo "Testing goto-definition..."
cargo run --bin code-agent-cli -- goto-definition tests/samples/test.ts 6 20 > /dev/null

# Test format-code
echo "Testing format-code..."
echo "function test(){return 42;}" > temp_format_test.ts
cargo run --bin code-agent-cli -- format-code temp_format_test.ts > /dev/null
rm -f temp_format_test.ts

# Test rename-symbol dry-run
echo "Testing rename-symbol (dry-run)..."
cargo run --bin code-agent-cli -- rename-symbol tests/samples/test.ts 1 9 newGreet --dry-run > /dev/null

echo "✅ All CLI regression tests passed!"
