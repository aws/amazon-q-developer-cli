#!/bin/bash
# Setup git hooks for kiro-cli development

if [ ! -d ".git" ]; then
    echo "Error: Not in a git repository"
    exit 1
fi

echo "Installing git hooks..."
git config core.hooksPath .githooks

echo "✓ Git hooks installed successfully"
echo ""
echo "Enabled hooks:"
echo "  • pre-commit: runs cargo fmt, cargo clippy, TUI lint, and changelog fragment lint"
echo "  • pre-push: reminds to update documentation"
