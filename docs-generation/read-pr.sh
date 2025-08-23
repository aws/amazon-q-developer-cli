#!/bin/bash
set -e

PR_NUMBER=$1
PR_FILE="pr-contents.txt"

echo "====== PR Information ======\n" > $PR_FILE
gh pr view $PR_NUMBER --json title,body --jq '"Title: " + .title + "\nDescription: " + .body >> $PR_FILE

echo -e "\n====== Updated files ======\n" >> $PR_FILE
gh pr view $PR_NUMBER --json files --jq ".files[].path" | while read file; do
    if [ -f "$file" ]; then
        echo "---- $file ----" >> $PR_FILE
        cat "$file" >> $PR_FILE
        echo -e "\n" >> $PR_FILE
    fi
done





 