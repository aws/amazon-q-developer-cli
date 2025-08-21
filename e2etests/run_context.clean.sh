#!/bin/bash

# Clean context-only test runner - runs only context test files
# Usage: ./run_context_clean.sh [path_to_q_binary]

Q_BINARY="q"

if [ $# -gt 0 ]; then
    Q_BINARY="$1"
    export Q_CLI_PATH="$Q_BINARY"
fi

echo "🚀 Running context Commands Tests"
echo "============================="
echo ""

# Run only the specific context test files
echo "🔄 Running context tests..."
cargo test --test --features "context" -- --nocapture --test-threads=1

exit_code=$?

echo ""
if [ $exit_code -eq 0 ]; then
    echo "🎉 All context tests passed!"
else
    echo "💥 Some context tests failed!"
fi

exit $exit_code
