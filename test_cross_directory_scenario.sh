#!/bin/bash
set -e

echo "=== Testing Cross-Directory Agent allowedPaths Fix ==="
echo ""

# Build the CLI first
echo "Building chat CLI..."
cargo build --release -p chat_cli
CLI_PATH="./target/release/chat_cli"

if [ ! -f "$CLI_PATH" ]; then
    echo "Error: CLI binary not found at $CLI_PATH"
    exit 1
fi

# Create temporary directories
TEST_DIR=$(mktemp -d)
AGENT_DIR="$TEST_DIR/agent_location"
TARGET_DIR="$TEST_DIR/target_location"

echo "Test directory: $TEST_DIR"
echo "Agent directory: $AGENT_DIR"
echo "Target directory: $TARGET_DIR"
echo ""

# Create directory structure
mkdir -p "$AGENT_DIR/.amazonq/cli-agents"
mkdir -p "$TARGET_DIR/src"

# Create a test agent config with absolute path in allowedPaths
cat > "$AGENT_DIR/.amazonq/cli-agents/test-agent.json" << EOF
{
  "\$schema": "https://example.com/agent-schema.json",
  "name": "test-agent",
  "description": "Test agent for cross-directory scenario",
  "tools": ["fs_write", "fs_read"],
  "allowedTools": ["fs_write", "fs_read"],
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": [
        "$TARGET_DIR/**"
      ]
    },
    "fs_read": {
      "allowedPaths": [
        "$TARGET_DIR/**"
      ]
    }
  }
}
EOF

echo "Created agent config at: $AGENT_DIR/.amazonq/cli-agents/test-agent.json"
echo "Agent config contents:"
cat "$AGENT_DIR/.amazonq/cli-agents/test-agent.json"
echo ""

# Test 1: Write to target directory with absolute path in allowedPaths
echo "=== Test 1: Write file with absolute path in allowedPaths ==="
cd "$AGENT_DIR"
TARGET_FILE="$TARGET_DIR/src/test.txt"

echo "Running: q chat --agent test-agent --no-interactive 'Write hello world to $TARGET_FILE'"
$CLI_PATH chat --agent test-agent --no-interactive "Write the text 'hello world' to the file $TARGET_FILE" 2>&1 | tee "$TEST_DIR/test1_output.txt"

# Check if the command succeeded
if grep -q "Tool approval required" "$TEST_DIR/test1_output.txt"; then
    echo "❌ FAILED: Tool approval was required (bug not fixed)"
    RESULT_1="FAILED"
else
    echo "✅ PASSED: No tool approval required"
    RESULT_1="PASSED"
fi
echo ""

# Test 2: Test with glob patterns
echo "=== Test 2: Write file with glob pattern in allowedPaths ==="
cat > "$AGENT_DIR/.amazonq/cli-agents/test-agent-glob.json" << EOF
{
  "\$schema": "https://example.com/agent-schema.json",
  "name": "test-agent-glob",
  "description": "Test agent with glob patterns",
  "tools": ["fs_write"],
  "allowedTools": ["fs_write"],
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": [
        "$TARGET_DIR/**/*.txt"
      ]
    }
  }
}
EOF

TARGET_FILE_2="$TARGET_DIR/src/test2.txt"
echo "Running: q chat --agent test-agent-glob --no-interactive 'Write test to $TARGET_FILE_2'"
$CLI_PATH chat --agent test-agent-glob --no-interactive "Write the text 'test' to the file $TARGET_FILE_2" 2>&1 | tee "$TEST_DIR/test2_output.txt"

if grep -q "Tool approval required" "$TEST_DIR/test2_output.txt"; then
    echo "❌ FAILED: Tool approval was required with glob pattern"
    RESULT_2="FAILED"
else
    echo "✅ PASSED: Glob pattern worked correctly"
    RESULT_2="PASSED"
fi
echo ""

# Test 3: Read from target directory
echo "=== Test 3: Read file with absolute path in allowedPaths ==="
echo "test content" > "$TARGET_DIR/src/read_test.txt"

echo "Running: q chat --agent test-agent --no-interactive 'Read $TARGET_DIR/src/read_test.txt'"
$CLI_PATH chat --agent test-agent --no-interactive "Read the file $TARGET_DIR/src/read_test.txt" 2>&1 | tee "$TEST_DIR/test3_output.txt"

if grep -q "Tool approval required" "$TEST_DIR/test3_output.txt"; then
    echo "❌ FAILED: Tool approval was required for read"
    RESULT_3="FAILED"
else
    echo "✅ PASSED: Read operation worked correctly"
    RESULT_3="PASSED"
fi
echo ""

# Summary
echo "=== Test Summary ==="
echo "Test 1 (Write with absolute path): $RESULT_1"
echo "Test 2 (Write with glob pattern): $RESULT_2"
echo "Test 3 (Read with absolute path): $RESULT_3"
echo ""

# Cleanup
echo "Cleaning up test directory: $TEST_DIR"
rm -rf "$TEST_DIR"

# Exit with failure if any test failed
if [ "$RESULT_1" = "FAILED" ] || [ "$RESULT_2" = "FAILED" ] || [ "$RESULT_3" = "FAILED" ]; then
    echo "❌ Some tests failed"
    exit 1
else
    echo "✅ All tests passed"
    exit 0
fi
