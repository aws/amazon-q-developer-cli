#!/bin/bash

# Clean MCP-only test runner - runs only MCP test files
# Usage: ./run_mcp_clean.sh [path_to_q_binary]

Q_BINARY="q"

if [ $# -gt 0 ]; then
    Q_BINARY="$1"
    export Q_CLI_PATH="$Q_BINARY"
fi

echo "🚀 Running MCP Commands Tests"
echo "============================="
echo ""

# Run only the specific MCP test files
echo "🔄 Running MCP tests..."
cargo test --test test_mcp_help_command --test test_mcp_loading_command --features "mcp" -- --nocapture --test-threads=1

exit_code=$?

echo ""
if [ $exit_code -eq 0 ]; then
    echo "🎉 All MCP tests passed!"
else
    echo "💥 Some MCP tests failed!"
fi

exit $exit_code
