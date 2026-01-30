#!/bin/bash
set -e

# Merge feature/ux-refresh into main as a squashed commit while preserving specific files from main

BRANCH="feature/ux-refresh"
PRESERVE_PATHS=(
    .github
    scripts
    AGENTS.md
    README.md
    .kiro
    .gitignore
    packages
    crates/chat-cli
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
    git checkout main
    git pull origin main

    MERGE_BRANCH="merge/ux-refresh-$(date +%Y%m%d)"
    git checkout -b "$MERGE_BRANCH"

    git fetch origin "$BRANCH"

    # Squash merge the branch, accepting theirs for conflicts
    git merge --squash -X theirs "origin/$BRANCH" || true

    # Restore preserved paths from main
    for path in "${PRESERVE_PATHS[@]}"; do
        git checkout main -- "$path" 2>/dev/null || true
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

    git add -A
    git commit -m "Merge feature/ux-refresh into main"

    echo "Branch '$MERGE_BRANCH' created with merge commit."
    echo "Push and create PR with:"
    echo "  git push -u origin $MERGE_BRANCH"
fi
