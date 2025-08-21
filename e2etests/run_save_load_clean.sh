#!/bin/bash

# Clean save-load-only test runner - runs only save and load test files
# Usage: ./run_context_save_load.sh [path_to_q_binary]

Q_BINARY="q"

if [ $# -gt 0 ]; then
    Q_BINARY="$1"
    export Q_CLI_PATH="$Q_BINARY"
fi

echo "🚀 Running save and load Commands Tests"
echo "============================="
echo ""

# Run only the specific save and load test files
echo "🔄 Running save and load tests..."
cargo test --test --features "save_load" -- --nocapture --test-threads=1

exit_code=$?

echo ""
if [ $exit_code -eq 0 ]; then
    echo "🎉 All save and load tests passed!"
else
    echo "💥 Some save and load tests failed!"
fi

exit $exit_code
