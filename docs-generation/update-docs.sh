#!/bin/bash
set -e

BRANCH_NAME="docs-update-for-pr-$PR_NUMBER"

if [ ! -f "$PR_FILE" ]; then
    echo "PR file not found, aborting"
    exit 1
fi

# Create branch before making any changes
git checkout -B "$BRANCH_NAME"
if git ls-remote --exit-code --heads origin $BRANCH_NAME; then
    git pull origin $BRANCH_NAME --force
fi

PROMPT="Before making any changes, read the 'docs' directory for the project's current
documentation. Then read 'pr-contents.txt' to see the contents of the current PR.\n\n
After reading both the directory and the PR file, update the files in the 'docs' directory 
with new, concise documentation reflecting ONLY the proposed changes in the PR. Make new files as appropriate.
Do not document changes or functionalities not related to the PR."

timeout 10m echo -e $PROMPT | qchat chat --non-interactive --trust-all-tools
exit $?