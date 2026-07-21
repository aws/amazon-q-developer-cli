#!/usr/bin/env bash
# Revert a bad release from the prod stable channel.
#
# Runs the full 4-step revert as a single pipeline:
#   1. download the current stable index.json from S3
#   2. strip the bad version's entry
#   3. STOP HERE if --dry-run (default); print the diff to inspect
#      or continue and upload back to S3
#   4. invalidate CloudFront on both distributions
#
# Usage:
#   revert-external-release.sh --version <ver>            # dry-run: downloads + strips, then stops
#   revert-external-release.sh --version <ver> --live     # actually upload + invalidate
#
# Options:
#   --version <ver>   required, the bad version to remove (e.g. 2.11.0)
#   --live            actually execute upload + invalidate (default is dry-run)
#   --out-dir <dir>   where to write results (default: ./results)
#
# Prereq: AWS creds in the env, e.g.
#   eval "$(ada cred print --account 158872659206 --role McmDeploy --format env)"

set -euo pipefail

usage() {
  cat <<'EOF'
Revert a bad release from the prod stable channel.

Runs the full 4-step revert as a single pipeline:
  1. download the current stable index.json from S3
  2. strip the bad version's entry
  3. STOP HERE if --dry-run (default); print the diff to inspect
     or continue and upload back to S3
  4. invalidate CloudFront on both distributions

Usage:
  revert-external-release.sh --version <ver>            # dry-run: downloads + strips, then stops
  revert-external-release.sh --version <ver> --live     # actually upload + invalidate

Options:
  --version <ver>   required, the bad version to remove (e.g. 2.11.0)
  --live            actually execute upload + invalidate (default is dry-run)
  --out-dir <dir>   where to write results (default: ./results)
  --help, -h        show this help

Prereq: AWS creds in the env, e.g.
  eval "$(ada cred print --account 158872659206 --role McmDeploy --format env)"
EOF
}

# --- Config ------------------------------------------------------------------

S3_URI="s3://kiro-cli-public-download-prod-us-east-1-158872659206/stable/index.json"
MODERN_DIST="E3I6IG70J7OAJ"      # DownloadDistribution — prod.download.cli.kiro.dev
MODERN_PATHS=("/stable/index.json" "/stable/latest/*")
LEGACY_DIST="E1PREM1JKPVIXA"     # FigIoDesktop legacy — desktop-release.q.us-east-1.amazonaws.com
LEGACY_PATHS=("/index.json" "/latest/*")

# --- Args --------------------------------------------------------------------

VERSION=""
MODE="dry-run"
OUT_DIR="./results"

while [ $# -gt 0 ]; do
  case "$1" in
    --version)  VERSION="${2:?--version requires a value}"; shift 2 ;;
    --live)     MODE="live"; shift ;;
    --dry-run)  MODE="dry-run"; shift ;;
    --out-dir)  OUT_DIR="${2:?--out-dir requires a value}"; shift 2 ;;
    --help|-h)  usage; exit 0 ;;
    *)          echo "unknown arg: $1 (try --help)" >&2; exit 2 ;;
  esac
done

if [ -z "$VERSION" ]; then
  echo "error: --version is required" >&2
  exit 2
fi

for tool in aws jq perl shasum; do
  command -v "$tool" >/dev/null || { echo "error: $tool not on PATH" >&2; exit 2; }
done

mkdir -p "$OUT_DIR"
ORIGIN="$OUT_DIR/index.origin.json"
MODIFIED="$OUT_DIR/index.json"

# --- 1. Download -------------------------------------------------------------

echo "[1/4] downloading $S3_URI"
aws s3 cp "$S3_URI" "$ORIGIN" >/dev/null
echo "      -> $ORIGIN"

# --- 2. Strip ----------------------------------------------------------------

BEFORE=$(jq '.versions | length' "$ORIGIN")
echo "[2/4] removing version $VERSION from versions[] (currently $BEFORE entries)"

# jq's default output is 2-space indent + trailing newline; the Lambda writes
# without a trailing newline, so strip it here to match Lambda's byte layout.
jq --arg v "$VERSION" '.versions |= map(select(.version != $v))' "$ORIGIN" \
  | perl -pe 'chomp if eof' > "$MODIFIED"

AFTER=$(jq '.versions | length' "$MODIFIED")
if [ "$BEFORE" = "$AFTER" ]; then
  if [ "$MODE" = "dry-run" ]; then
    # Dry-run keeps the hard error so typos surface before any live action.
    echo "error: version $VERSION not found in versions[]" >&2
    echo "       available (first 5): $(jq -r '.versions[].version' "$ORIGIN" | head -5 | tr '\n' ' ')..." >&2
    exit 1
  else
    # Live mode: version already absent means we're resuming after a partial
    # failure (e.g. previous run stripped and uploaded but crashed before
    # invalidation). Continue so the invalidation step runs and finishes the
    # job — origin is already what we wanted to write, so upload is a no-op.
    echo "      note: version $VERSION already absent (likely a resume after partial failure)"
    echo "      proceeding to re-upload (no-op) and invalidation"
  fi
else
  echo "      $BEFORE -> $AFTER entries"
fi

# --- 3. Dry-run gate ---------------------------------------------------------

if [ "$MODE" = "dry-run" ]; then
  echo ""
  echo "DRY RUN — stopping before upload."
  echo ""
  echo "Files produced:"
  echo "  original: $ORIGIN"
  echo "  modified: $MODIFIED"
  echo ""
  echo "Inspect the diff:"
  echo "  diff <(jq -r '.versions[].version' $ORIGIN) <(jq -r '.versions[].version' $MODIFIED)"
  echo ""
  echo "When ready to push, re-run with --live:"
  echo "  $0 --version $VERSION --live"
  exit 0
fi

# --- 4. Upload + verify ------------------------------------------------------

# Guard against a lost update: another writer (the Revert Step 2 Lambda re-run,
# a scheduled release, a second responder) may have rewritten index.json since
# we downloaded $ORIGIN. Re-fetch and bail if the remote no longer matches the
# snapshot we stripped from, so we never clobber an intervening change.
PRECHECK_TMP=$(mktemp -t revert-precheck.XXXXXX)
REMOTE_TMP=$(mktemp -t revert-verify.XXXXXX)
trap 'rm -f "$PRECHECK_TMP" "$REMOTE_TMP"' EXIT

echo "[3/4] re-checking remote is unchanged since download"
aws s3 cp "$S3_URI" "$PRECHECK_TMP" >/dev/null
ORIGIN_SHA=$(shasum -a 256 "$ORIGIN" | awk '{print $1}')
PRECHECK_SHA=$(shasum -a 256 "$PRECHECK_TMP" | awk '{print $1}')
if [ "$ORIGIN_SHA" != "$PRECHECK_SHA" ]; then
  echo "error: remote index.json changed since it was downloaded — refusing to upload." >&2
  echo "       Something wrote to $S3_URI after this run started (e.g. the Revert" >&2
  echo "       Step 2 Lambda re-run, or a scheduled release). Re-run the dry-run to" >&2
  echo "       pick up the current file, re-inspect the diff, then re-run --live." >&2
  exit 1
fi
echo "      unchanged ($ORIGIN_SHA)"

echo "      uploading $MODIFIED -> $S3_URI"
# content-type + cache-control are the intended metadata for the registry
# object; the SHA256 check below verifies body bytes only, not this metadata.
aws s3 cp "$MODIFIED" "$S3_URI" \
  --content-type application/json \
  --cache-control "public, max-age=300" >/dev/null

echo "      verifying S3 object matches local file (SHA256)"
aws s3 cp "$S3_URI" "$REMOTE_TMP" >/dev/null

LOCAL_SHA=$(shasum -a 256 "$MODIFIED" | awk '{print $1}')
REMOTE_SHA=$(shasum -a 256 "$REMOTE_TMP" | awk '{print $1}')
if [ "$LOCAL_SHA" != "$REMOTE_SHA" ]; then
  echo "error: SHA256 mismatch after upload" >&2
  echo "       local:  $LOCAL_SHA" >&2
  echo "       remote: $REMOTE_SHA" >&2
  exit 1
fi
echo "      SHA256 match: $LOCAL_SHA"

# --- 5. Invalidate both CloudFront distributions -----------------------------

echo "[4/4] invalidating CloudFront"

MODERN_ID=$(aws cloudfront create-invalidation \
  --distribution-id "$MODERN_DIST" \
  --paths "${MODERN_PATHS[@]}" \
  --query 'Invalidation.Id' --output text)
echo "      modern ($MODERN_DIST, prod.download.cli.kiro.dev): $MODERN_ID"

LEGACY_ID=$(aws cloudfront create-invalidation \
  --distribution-id "$LEGACY_DIST" \
  --paths "${LEGACY_PATHS[@]}" \
  --query 'Invalidation.Id' --output text)
echo "      legacy ($LEGACY_DIST, desktop-release.q.us-east-1.amazonaws.com): $LEGACY_ID"

echo ""
echo "OK: revert complete. Invalidations typically finish in 30-60 seconds."
echo "Poll status with:"
echo "  aws cloudfront list-invalidations --distribution-id $MODERN_DIST --max-items 3"
echo "  aws cloudfront list-invalidations --distribution-id $LEGACY_DIST --max-items 3"
