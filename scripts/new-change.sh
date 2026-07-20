#!/bin/bash
set -e

TYPE=$1
DESC=$2

if [ -z "$TYPE" ] || [ -z "$DESC" ]; then
  echo "Usage: ./scripts/new-change.sh <type> \"<description>\""
  echo "Types: added, changed, deprecated, removed, fixed, security"
  exit 1
fi

if [[ ! "$TYPE" =~ ^(added|changed|deprecated|removed|fixed|security)$ ]]; then
  echo "Error: type must be one of: added, changed, deprecated, removed, fixed, security"
  exit 1
fi

SLUG=$(echo "$DESC" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9-' | cut -c1-30)
TIMESTAMP=$(date +%Y%m%d-%H%M)
FILE=".changes/${TIMESTAMP}-${TYPE}-${SLUG}.json"

mkdir -p .changes

jq -n --arg type "$TYPE" --arg desc "$DESC" '{type: $type, description: $desc}' > "$FILE"

# Validate the new entry
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
if [ -x "$SCRIPT_DIR/lint-changelog.sh" ]; then
  if ! "$SCRIPT_DIR/lint-changelog.sh" "$FILE"; then
    rm -f "$FILE"
    exit 1
  fi
fi

echo "Created: $FILE"
