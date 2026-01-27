#!/bin/bash
set -e

VERSION=$1
DATE=$(date +%Y-%m-%d)
FEED_PATH="crates/chat-cli/src/cli/feed.json"

if [ -z "$VERSION" ]; then
  echo "Usage: ./scripts/release-changelog.sh <version>"
  echo "Example: ./scripts/release-changelog.sh 1.25.0"
  exit 1
fi

# Check for fragments
FRAGMENTS=(.changes/unreleased/*.json)
if [ ! -e "${FRAGMENTS[0]}" ]; then
  echo "No changelog fragments found in .changes/unreleased/"
  exit 1
fi

# Build changes array from fragments
CHANGES=$(jq -s '.' .changes/unreleased/*.json)

# Create new release entry
NEW_ENTRY=$(jq -n \
  --arg type "release" \
  --arg date "$DATE" \
  --arg version "$VERSION" \
  --arg title "Version $VERSION" \
  --argjson changes "$CHANGES" \
  '{type: $type, date: $date, version: $version, title: $title, changes: $changes}')

# Insert after the placeholder entry (index 1)
TMPFILE=$(mktemp)
jq --argjson entry "$NEW_ENTRY" '.entries = [.entries[0], $entry] + .entries[1:]' "$FEED_PATH" > "$TMPFILE"
mv "$TMPFILE" "$FEED_PATH"

# Move fragments to released
mkdir -p ".changes/released/v$VERSION"
mv .changes/unreleased/*.json ".changes/released/v$VERSION/"

echo "Released v$VERSION with $(echo "$CHANGES" | jq 'length') changes"
echo "Fragments moved to .changes/released/v$VERSION/"
