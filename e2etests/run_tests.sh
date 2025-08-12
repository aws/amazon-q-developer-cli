#!/bin/bash

# Q CLI E2E Test Runner
# Usage: ./run_tests.sh [path_to_q_binary] [test_name]

Q_BINARY=${1:-"q"}
TEST_NAME=${2:-""}

echo "🚀 Running Q CLI E2E tests with binary: $Q_BINARY"

if [ "$Q_BINARY" != "q" ]; then
    if [ ! -f "$Q_BINARY" ]; then
        echo "❌ Error: Q CLI binary not found at: $Q_BINARY"
        exit 1
    fi
    echo "📍 Using custom Q CLI binary: $Q_BINARY"
    export Q_CLI_PATH="$Q_BINARY"
else
    echo "📍 Using default system Q CLI binary"
    export Q_CLI_PATH="$Q_BINARY"
fi

if [ -n "$TEST_NAME" ]; then
    echo "🧪 Running specific test: $TEST_NAME"
    cargo test --test "$TEST_NAME" -- --nocapture
else
    echo "🧪 Running all E2E tests"
    cargo test --test test_help_command --test test_tools_command --test test_ai_prompt --test test_clear_command --test test_quit_command --test test_save_help_command --test test_usage_command --test test_usage_help_command --test test_compact_command --test test_compact_help_command --test test_hooks_help_command --test test_mcp_help_command --test test_mcp_loading_command --test test_model_command --test test_model_help_command --test test_subscribe_command --test test_subscribe_help_command --test test_editor_help_command -- --nocapture
fi
