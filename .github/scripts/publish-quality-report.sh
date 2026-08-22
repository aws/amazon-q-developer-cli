#!/usr/bin/env bash
set -euo pipefail

report_status() {
  local level="$1"
  local message="$2"
  printf '::%s title=Quality metrics publisher::%s\n' "$level" "$message"
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '### Quality metrics publisher\n\n%s\n' "$message" >> "$GITHUB_STEP_SUMMARY"
  fi
}

fail() {
  report_status error "$1"
  exit 1
}

on_error() {
  local status="$?"
  trap - ERR
  report_status error "Publisher command failed with exit code $status."
  exit "$status"
}
trap on_error ERR

if [[ ! "$EXPECTED_HEAD_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  fail "Invalid expected PR head SHA: $EXPECTED_HEAD_SHA"
fi
if [[ ! "$PR_NUMBER" =~ ^[1-9][0-9]*$ ]]; then
  fail "Invalid pull request number: $PR_NUMBER"
fi
if [[ ! "$REPORT_SECTION" =~ ^[a-z0-9-]+$ ]]; then
  fail "Invalid quality report section: $REPORT_SECTION"
fi

publisher_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
work_dir="$(mktemp -d "${RUNNER_TEMP%/}/quality-report.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT
current_report="$work_dir/current.md"
section_report="$work_dir/section.md"
merged_report="$work_dir/merged.md"
: > "$current_report"

current_head_sha="$(
  gh api "repos/${GITHUB_REPOSITORY}/pulls/${PR_NUMBER}" --jq .head.sha
)"
if [[ "$current_head_sha" != "$EXPECTED_HEAD_SHA" ]]; then
  report_status notice \
    "Skipping stale report for $EXPECTED_HEAD_SHA; PR head is $current_head_sha."
  exit 0
fi

comment_ids="$(
  gh api --paginate \
    "repos/${GITHUB_REPOSITORY}/issues/${PR_NUMBER}/comments?per_page=100" \
    --jq '.[] | select(.user.login == "github-actions[bot]") | select(.body | contains("<!-- code-quality-coverage-report -->")) | .id'
)"
comment_id=""
comment_count=0
while IFS= read -r candidate; do
  [[ -z "$candidate" ]] && continue
  if [[ ! "$candidate" =~ ^[1-9][0-9]*$ ]]; then
    fail "Invalid quality report comment id: $candidate"
  fi
  ((comment_count += 1))
  if [[ -z "$comment_id" ]] || ((10#$candidate < 10#$comment_id)); then
    comment_id="$candidate"
  fi
done <<< "$comment_ids"
if ((comment_count > 1)); then
  report_status warning \
    "Found $comment_count quality report comments; updating canonical comment $comment_id."
fi

if [[ -n "$comment_id" ]]; then
  gh api "repos/${GITHUB_REPOSITORY}/issues/comments/${comment_id}" \
    --jq .body > "$current_report"
fi

cp -- "$REPORT_CONTENT" "$section_report"
printf "\n_PR head revision: \`%s\`._\n" \
  "${EXPECTED_HEAD_SHA:0:12}" >> "$section_report"

python3 "$publisher_root/scripts/code-quality/pr-report.py" merge \
  --current "$current_report" \
  --section "$REPORT_SECTION" \
  --content "$section_report" \
  --out "$merged_report"

current_head_sha="$(
  gh api "repos/${GITHUB_REPOSITORY}/pulls/${PR_NUMBER}" --jq .head.sha
)"
if [[ "$current_head_sha" != "$EXPECTED_HEAD_SHA" ]]; then
  report_status notice \
    "Skipping stale report for $EXPECTED_HEAD_SHA; PR head is $current_head_sha."
  exit 0
fi

if [[ -n "$comment_id" ]]; then
  gh api --method PATCH \
    "repos/${GITHUB_REPOSITORY}/issues/comments/${comment_id}" \
    --raw-field body="$(cat "$merged_report")" \
    >/dev/null
else
  gh api --method POST \
    "repos/${GITHUB_REPOSITORY}/issues/${PR_NUMBER}/comments" \
    --raw-field body="$(cat "$merged_report")" \
    >/dev/null
fi
