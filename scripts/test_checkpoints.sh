#!/bin/bash
set -e

# Build the CLI
echo "Building q_cli..."
cargo build -p q_cli

# Create test directory
TEST_DIR=$(mktemp -d)
echo "Using test directory: $TEST_DIR"

# Function to run a command and capture the output
run_command() {
  echo "Running: $1"
  echo "$1" | ./target/debug/q_cli chat --trajectory --trajectory-dir "$TEST_DIR" --accept-all
}

# Start with initial conversation
echo "Starting initial conversation..."
run_command "Hello, this is a test of the checkpoint functionality"

# Create first checkpoint
echo "Creating first checkpoint..."
run_command "/trajectory checkpoint create initial_state"

# Continue conversation
echo "Continuing conversation..."
run_command "List the files in the current directory"

# Create second checkpoint
echo "Creating second checkpoint..."
run_command "/trajectory checkpoint create after_listing"

# List checkpoints
echo "Listing checkpoints..."
CHECKPOINT_LIST=$(run_command "/trajectory checkpoint list")
echo "$CHECKPOINT_LIST"

# Extract checkpoint IDs
INITIAL_ID=$(echo "$CHECKPOINT_LIST" | grep -A1 "initial_state" | grep "ID:" | awk '{print $2}')
LISTING_ID=$(echo "$CHECKPOINT_LIST" | grep -A1 "after_listing" | grep "ID:" | awk '{print $2}')

echo "Initial checkpoint ID: $INITIAL_ID"
echo "After listing checkpoint ID: $LISTING_ID"

# Restore to initial state
echo "Restoring to initial state..."
run_command "/trajectory checkpoint restore $INITIAL_ID"

# Verify by asking about the conversation
echo "Verifying restoration..."
VERIFICATION=$(run_command "What was our last conversation about?")
echo "$VERIFICATION"

# Check if verification contains expected text
if echo "$VERIFICATION" | grep -q "initial"; then
  echo "✅ Checkpoint restoration successful!"
else
  echo "❌ Checkpoint restoration may have failed"
fi

# Clean up
echo "Cleaning up test directory..."
rm -rf "$TEST_DIR"

echo "Test completed!"
