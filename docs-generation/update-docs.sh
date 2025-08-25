#!/bin/bash
set -e

if [ ! -f "$PR_FILE" ]; then
    echo "PR file not found, aborting"
    exit 1
fi

PROMPT="Before making any changes, read the 'docs' directory for the project's current
documentation. Then read 'pr-contents.txt' to see the contents of the current PR.\n\n
After reading both the directory and the PR file, update the files in the 'docs' directory 
with new, concise documentation reflecting the proposed changes in the PR. Make new files as appropriate."

timeout 10m echo -e $PROMPT | qchat chat --non-interactive --trust-all-tools
exit $?