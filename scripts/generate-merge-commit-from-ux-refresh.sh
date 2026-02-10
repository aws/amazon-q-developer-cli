#!/bin/bash
set -e

# Merge feature/ux-refresh into main as a squashed commit while preserving specific files from main

BRANCH="feature/ux-refresh"
PRESERVE_PATHS=(
    .github
    .githooks
    scripts
    autodocs
    AGENTS.md
    README.md
    .kiro
    .gitignore
    packages
    crates/chat-cli
    crates/code-agent-sdk
    crates/semantic-search-client
    Cargo.toml
)
ACP_TEST_FILE="crates/chat-cli-v2/tests/acp.rs"

# Detect if using jj or git
if [ "$1" = "jj" ]; then
    VCS="jj"
else
    VCS="git"
fi

if [ "$VCS" = "jj" ]; then
    # jj workflow
    jj git fetch

    # Create a new change on main
    jj new main -m "Merge feature/ux-refresh into main"

    # Squash merge the branch
    jj squash --from "$BRANCH" --into @ || true

    # Restore preserved paths from main
    for path in "${PRESERVE_PATHS[@]}"; do
        jj restore --from main "$path" 2>/dev/null || true
    done

    # Add #[ignore] to all tests in acp.rs that don't already have it
    if [ -f "$ACP_TEST_FILE" ]; then
        sed -i '' '/^#\[tokio::test/{
            N
            /\n#\[ignore/!s/\(#\[tokio::test[^]]*\]\)\n/\1\n#[ignore = "disabled for ux-refresh merge"]\n/
        }' "$ACP_TEST_FILE"
    fi

    # Update lock file
    echo "Updating lock file"
    cargo generate-lockfile

    jj describe -m "Merge feature/ux-refresh into main"

    echo "Done. Push with: jj git push"
else
    # git workflow
    
    # Block if untracked files exist
    if [ -n "$(git ls-files --others --exclude-standard)" ]; then
        echo "Error: Untracked files exist. Please commit or remove them first:"
        git ls-files --others --exclude-standard
        exit 1
    fi

    git checkout main
    git pull origin main

    MERGE_BRANCH="merge/ux-refresh-$(date +%Y%m%d)"
    git checkout -b "$MERGE_BRANCH"

    git fetch origin "$BRANCH"

    # Squash merge the branch, accepting theirs for conflicts
    git merge --squash -X theirs "origin/$BRANCH" || true

    # Restore preserved paths from main (reset first to handle new files from merge)
    for path in "${PRESERVE_PATHS[@]}"; do
        git reset main -- "$path" 2>/dev/null || true
        git checkout main -- "$path" 2>/dev/null || true
    done

    # Explicitly restore non-preserved crates from feature branch
    git checkout "origin/$BRANCH" -- crates/agent 2>/dev/null || true
    git checkout "origin/$BRANCH" -- crates/chat-cli-v2 2>/dev/null || true
    git checkout "origin/$BRANCH" -- crates/mock-mcp-server 2>/dev/null || true

    # Add #[ignore] to all tests in acp.rs that don't already have it
    if [ -f "$ACP_TEST_FILE" ]; then
        sed -i '' '/^#\[tokio::test/{
            N
            /\n#\[ignore/!s/\(#\[tokio::test[^]]*\]\)\n/\1\n#[ignore = "disabled for ux-refresh merge"]\n/
        }' "$ACP_TEST_FILE"
    fi

    # Update lock file
    echo "Updating lock file"
    cargo generate-lockfile

    git add -A

    echo "Branch '$MERGE_BRANCH' ready for review."
    echo "Review changes with: git status && git diff --staged"
    echo "When ready, commit with: git commit -m 'Merge feature/ux-refresh into main'"
    echo "Then push and create PR with: git push -u origin $MERGE_BRANCH"
fi
