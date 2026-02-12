#!/bin/bash
# Delete all workflow runs for a given workflow file
# Usage: ./delete-workflow-runs.sh <workflow-file>

WORKFLOW="${1:-mdbook.yml}"

echo "Fetching runs for $WORKFLOW..."
RUN_IDS=$(gh run list --workflow="$WORKFLOW" --json databaseId -q '.[].databaseId' --limit 1000)

if [ -z "$RUN_IDS" ]; then
  echo "No runs found for $WORKFLOW"
  exit 0
fi

COUNT=$(echo "$RUN_IDS" | wc -l | tr -d ' ')
echo "Found $COUNT runs. Deleting..."

echo "$RUN_IDS" | while read -r id; do
  gh run delete "$id" 2>&1
done

echo "Done"
