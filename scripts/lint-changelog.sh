#!/usr/bin/env bash
# Lint changelog fragments in .changes/unreleased/ for format compliance.
# Usage: ./scripts/lint-changelog.sh [file1.json file2.json ...]
# If no files given, lints all files in .changes/unreleased/
set -e

VALID_TYPES="added changed deprecated removed fixed security"

errors=0

get_verb_pattern() {
  case "$1" in
    added)      echo "^[Aa]dd(ed)?\b" ;;
    fixed)      echo "^[Ff]ix(ed)?\b" ;;
    changed)    echo "^[Cc]hange[d]?\b" ;;
    deprecated) echo "^[Dd]eprecate[d]?\b" ;;
    removed)    echo "^[Rr]emove[d]?\b" ;;
    *)          echo "" ;;
  esac
}

lint_file() {
  local file="$1"
  local basename
  basename=$(basename "$file")

  # Check valid JSON
  if ! jq empty "$file" 2>/dev/null; then
    echo "❌ $basename: invalid JSON"
    errors=$((errors + 1))
    return
  fi

  # Check required fields
  local type desc extra_keys
  type=$(jq -r '.type // empty' "$file")
  desc=$(jq -r '.description // empty' "$file")
  extra_keys=$(jq -r 'keys | map(select(. != "type" and . != "description")) | .[]' "$file")

  if [ -z "$type" ]; then
    echo "❌ $basename: missing 'type' field"
    errors=$((errors + 1))
    return
  fi

  if [ -z "$desc" ]; then
    echo "❌ $basename: missing 'description' field"
    errors=$((errors + 1))
    return
  fi

  if [ -n "$extra_keys" ]; then
    echo "❌ $basename: unexpected fields: $extra_keys"
    errors=$((errors + 1))
  fi

  # Check type is valid
  local valid=false
  for t in $VALID_TYPES; do
    if [ "$type" = "$t" ]; then valid=true; break; fi
  done
  if [ "$valid" = false ]; then
    echo "❌ $basename: invalid type '$type'. Must be one of: $VALID_TYPES"
    errors=$((errors + 1))
  fi

  # Check description doesn't start with the type verb
  local pattern
  pattern=$(get_verb_pattern "$type")
  if [ -n "$pattern" ] && echo "$desc" | grep -qE "$pattern"; then
    echo "❌ $basename: description should not start with '$type' verb."
    echo "   The type is already shown as a section header."
    echo "   Got: \"$desc\""
    echo ""
    echo "   Good examples:"
    case "$type" in
      added)
        echo "     - \`/rewind\` to jump back to an earlier prompt in a conversation"
        echo "     - Support per-model default settings in cli.json"
        echo "     - Configurable keybindings for V2 TUI cancel, close menu, and quit actions"
        ;;
      fixed)
        echo "     - Hardened pattern search against tree-sitter parser panics"
        echo "     - Reset context manager on \`/clear\` so loaded skills revert to frontmatter-only"
        echo "     - Prioritize built-in commands over skills in slash command autocomplete"
        ;;
      changed)
        echo "     - Reduced workspace initialization time by 88%"
        echo "     - Show actionable remediation steps when MCP is disabled"
        echo "     - Shell escape commands now use the user's default shell from \$SHELL"
        ;;
      *)
        echo "     - Write the description without the leading verb"
        ;;
    esac
    echo ""
    errors=$((errors + 1))
  fi

  # Check minimum length
  if [ ${#desc} -lt 10 ]; then
    echo "❌ $basename: description too short (${#desc} chars, min 10)"
    errors=$((errors + 1))
  fi

  # Check first letter is capitalized (skip if starts with backtick, /, $, --, or ")
  local first="${desc:0:1}"
  if echo "$first" | grep -qE '[a-z]'; then
    echo "❌ $basename: description should start with a capital letter."
    echo "   Got: \"$desc\""
    echo ""
    errors=$((errors + 1))
  fi

  # Check for 'and' joining multiple distinct changes — should be separate entries.
  # Only flag when 'and' follows a verb pattern suggesting a second fix/change.
  if echo "$desc" | grep -qiE ",\s+and\s+(fixed|added|changed|removed|deprecated|also|additionally)"; then
    echo "⚠️  $basename: description appears to join multiple changes with 'and'."
    echo "   Consider splitting into separate changelog entries — one change per file."
    echo "   Got: \"$desc\""
    echo ""
    errors=$((errors + 1))
  fi

  # Check that code references are wrapped in backticks
  # Match: /command, --flag, UPPER_ENV_VAR, tool_name patterns without backticks
  if echo "$desc" | grep -qE '(^|[^`])/[a-z]' && ! echo "$desc" | grep -qE '`/[a-z]'; then
    echo "⚠️  $basename: slash commands should be wrapped in backticks (e.g. \`/settings\`)"
    echo "   Got: \"$desc\""
    echo ""
    errors=$((errors + 1))
  fi
  if echo "$desc" | grep -qE '(^|[^`])--[a-z]' && ! echo "$desc" | grep -qE '`--[a-z]'; then
    echo "⚠️  $basename: flags should be wrapped in backticks (e.g. \`--resume\`)"
    echo "   Got: \"$desc\""
    echo ""
    errors=$((errors + 1))
  fi
}

# Determine files to lint
if [ $# -gt 0 ]; then
  files=("$@")
else
  shopt -s nullglob
  files=(.changes/unreleased/*.json)
  shopt -u nullglob
fi

if [ ${#files[@]} -eq 0 ]; then
  echo "No changelog fragments to lint."
  exit 0
fi

for file in "${files[@]}"; do
  lint_file "$file"
done

if [ $errors -gt 0 ]; then
  echo ""
  echo "Found $errors error(s). See .changes/GUIDELINES.md for format rules and examples."
  exit 1
else
  echo "✅ All ${#files[@]} changelog fragment(s) pass lint."
fi
