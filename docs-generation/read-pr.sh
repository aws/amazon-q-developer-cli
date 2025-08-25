#!/bin/bash
set -e

PR_NUMBER=$1

# Add PR information
echo "====== PR Information ======\n" > $PR_FILE
gh pr view $PR_NUMBER --json title,body --jq '"Title: " + .title + "\nDescription: " + .body' >> $PR_FILE

# Include updated files
echo -e "\n====== Updated files ======\n" >> $PR_FILE
gh pr view $PR_NUMBER --json files --jq ".files[].path" | while read file; do
    case "$file" in
        *.lock|*-lock.*|*.min.*|dist/*|build/*|target/*)
            continue
                ;;
    esac
    if [ -f "$file" ]; then
        echo "---- $file ----" >> $PR_FILE
        git diff main -- "$file" >> $PR_FILE
        echo -e "\n" >> $PR_FILE
    fi
done





 