#!/bin/bash
# publish-smoke-report.sh — Copy smoke test report to the shared report server
#
# Usage: ./publish-smoke-report.sh [report-dir]
#
# If no report-dir given, finds the latest smoke-* directory in test-outputs.

set -euo pipefail

REPORT_SERVER_DIR="$HOME/workplace/kiro_reviewer/reports"
TEST_OUTPUTS="$(dirname "$0")/../test-outputs"

# Find report dir
if [ -n "${1:-}" ]; then
  REPORT_DIR="$1"
else
  REPORT_DIR=$(ls -dt "$TEST_OUTPUTS"/smoke-* 2>/dev/null | head -1)
  if [ -z "$REPORT_DIR" ]; then
    echo "❌ No smoke test reports found in $TEST_OUTPUTS"
    exit 1
  fi
fi

if [ ! -f "$REPORT_DIR/index.html" ]; then
  echo "❌ No index.html in $REPORT_DIR"
  exit 1
fi

# Create destination
TIMESTAMP=$(basename "$REPORT_DIR")
DEST="$REPORT_SERVER_DIR/$TIMESTAMP"
mkdir -p "$DEST"

# Copy all report files
cp -r "$REPORT_DIR"/* "$DEST/"

echo "✅ Published to $DEST"
echo "🔗 http://$(hostname -f):3002/$TIMESTAMP/index.html"

# Update the reports index if it exists
INDEX="$REPORT_SERVER_DIR/index.html"
if [ -f "$INDEX" ]; then
  # Read results.json for summary
  if [ -f "$DEST/results.json" ]; then
    PASSED=$(python3 -c "import json; d=json.load(open('$DEST/results.json')); print(sum(1 for r in d['results'] if r['status']=='pass'))")
    TOTAL=$(python3 -c "import json; d=json.load(open('$DEST/results.json')); print(len(d['results']))")
    SUMMARY="$PASSED/$TOTAL passed"
  else
    SUMMARY="report available"
  fi

  # Prepend to index (insert after <tbody>)
  ROW="<tr><td>🔥</td><td><a href=\"$TIMESTAMP/index.html\">Smoke Test — $TIMESTAMP</a></td><td>$SUMMARY</td><td>$(date -u +%Y-%m-%dT%H:%M:%SZ)</td></tr>"
  sed -i "/<tbody>/a\\$ROW" "$INDEX" 2>/dev/null || true
fi
