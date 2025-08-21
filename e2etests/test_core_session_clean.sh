#!/bin/bash

# Clean core session(help, clean, quit) command test runner
# Usage: ./run_core_session_clean.sh [path_to_q_binary]

Q_BINARY="q"

if [ $# -gt 0 ]; then
    Q_BINARY="$1"
    export Q_CLI_PATH="$Q_BINARY"
fi

echo "🚀 Running Core Session Command Test"
echo "============================="
echo ""

# Run only the core session(help, clean, quit) command test
echo "🔄 Running core session(help, clean, quit) test..."
cargo test --test --features "core_session" -- --nocapture --test-threads=1

exit_code=$?

echo ""
if [ $exit_code -eq 0 ]; then
    echo "🎉 Core Session test passed!"
else
    echo "💥 Core Session test failed!"
fi

exit $exit_code
