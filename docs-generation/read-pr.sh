#!/bin/bash
set -e

PR_NUMBER=$1

# Add PR information
echo "====== PR Information ======\n" > $PR_FILE
gh pr view $PR_NUMBER --json title,body --jq '"Title: " + .title + "\nDescription: " + .body' >> $PR_FILE

# Include PR diffs
echo -e "\n====== PR Diffs ======\n" >> $PR_FILE
gh pr diff $PR_NUMBER >> $PR_FILE





 