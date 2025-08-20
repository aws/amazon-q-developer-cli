#!/bin/bash

# Clean tools command test runner
# Usage: ./run_tools_clean.sh [path_to_q_binary]

Q_BINARY="q"

if [ $# -gt 0 ]; then
    Q_BINARY="$1"
    export Q_CLI_PATH="$Q_BINARY"
fi

echo "🚀 Running Tools Command Test"
echo "============================="
echo ""

# Run only the /tools command test
echo "🔄 Running /tools test..."
cargo test --test test_tools_command --features "core_session" -- --nocapture --test-threads=1

exit_code=$?

echo ""
if [ $exit_code -eq 0 ]; then
    echo "🎉 Tools test passed!"
else
    echo "💥 Tools test failed!"
fi

exit $exit_code
