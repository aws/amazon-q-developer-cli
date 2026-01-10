#!/bin/bash
# Setup git hooks for kiro-cli development

HOOK_DIR=".git/hooks"
HUSKY_DIR=".husky"

if [ ! -d "$HOOK_DIR" ]; then
    echo "Error: Not in a git repository"
    exit 1
fi

echo "Installing git hooks..."
cp "$HUSKY_DIR/pre-commit" "$HOOK_DIR/pre-commit"
chmod +x "$HOOK_DIR/pre-commit"

echo "✓ Git hooks installed successfully"
echo "Pre-commit hook will run 'cargo fmt' and 'cargo clippy' before each commit"
