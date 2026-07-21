#!/usr/bin/env bash
# Revert a bad release for INTERNAL toolbox users.
#
# Runs `toolbox-vendor-ops recall` to mark the bad version as recalled on the
# stable channel and point auto-updates at a safe version instead. Installs
# toolbox-vendor-ops on demand if it's not already on PATH.
#
# Effect:
#   - `toolbox install kiro-cli --channel stable` future runs get <recommended>
#   - Existing installs of <bad_version> stay on it until users run
#     `toolbox update` — there is no push mechanism.
#
# For the external CDN revert (install script + auto-updaters), see
# revert-external-release.sh — they're independent surfaces, run both if the
# bad version reached both.
#
# Usage:
#   revert-internal-release.sh --version <bad> --recommended <safe>
#   revert-internal-release.sh --version <bad> --recommended <safe> --live

set -euo pipefail

usage() {
  cat <<'EOF'
Revert a bad release for internal (Amazon toolbox) users.

Runs the toolbox recall via toolbox-vendor-ops. Installs the tool on demand
if it's not present, refreshes credentials for the toolbox account, then runs
the recall command (in --dryrun mode by default).

Usage:
  revert-internal-release.sh --version <bad> --recommended <safe>       # dry-run (default)
  revert-internal-release.sh --version <bad> --recommended <safe> --live

Options:
  --version <ver>       required — the bad version to recall (e.g. 2.11.0)
  --recommended <ver>   required — safe version toolbox auto-updates land on (e.g. 2.10.0)
  --channel <name>      channel to recall from (default: stable)
  --live                actually execute the recall (default is --dryrun)
  --dry-run             explicit form of the default; passes --dryrun to toolbox-vendor-ops
  --help, -h            show this help

Prereqs:
  - `toolbox` CLI on PATH (Amazon Builder Toolbox). If toolbox-vendor-ops is
    missing, this script installs it via `toolbox install toolbox-ops`.
  - Isengard access to the toolbox-vendor-ops account (211125606403, Admin).
    Creds are refreshed automatically via `ada credentials update`.
EOF
}

# --- Args --------------------------------------------------------------------

VERSION=""
RECOMMENDED=""
CHANNEL="stable"
MODE="dry-run"

while [ $# -gt 0 ]; do
  case "$1" in
    --version)      VERSION="${2:?--version requires a value}"; shift 2 ;;
    --recommended)  RECOMMENDED="${2:?--recommended requires a value}"; shift 2 ;;
    --channel)      CHANNEL="${2:?--channel requires a value}"; shift 2 ;;
    --live)         MODE="live"; shift ;;
    --dry-run)      MODE="dry-run"; shift ;;
    --help|-h)      usage; exit 0 ;;
    *)              echo "unknown arg: $1 (try --help)" >&2; exit 2 ;;
  esac
done

[ -z "$VERSION" ]     && { echo "error: --version is required" >&2; exit 2; }
[ -z "$RECOMMENDED" ] && { echo "error: --recommended is required" >&2; exit 2; }
[ "$VERSION" = "$RECOMMENDED" ] && {
  echo "error: --recommended ($RECOMMENDED) must differ from --version ($VERSION)" >&2
  echo "       recalling a version and recommending that same version is a no-op." >&2
  exit 2
}

# --- Config ------------------------------------------------------------------

TOOLBOX_ACCOUNT="211125606403"
TOOLBOX_ROLE="Admin"
TOOLBOX_PROFILE="kiro-toolbox"
TOOLBOX_REPO="s3://buildertoolbox-kiro-cli-us-west-2"
REGISTRY_URI="s3://buildertoolbox-registry-toolbox-ops-us-west-2/tools.json"

# --- 1. Ensure toolbox-vendor-ops is installed ------------------------------

if ! command -v toolbox-vendor-ops >/dev/null; then
  echo "[1/3] toolbox-vendor-ops not on PATH — installing"
  if ! command -v toolbox >/dev/null; then
    echo "error: 'toolbox' CLI not on PATH." >&2
    echo "       Install Amazon Builder Toolbox first:" >&2
    echo "       https://docs.hub.amazon.dev/builder-toolbox/user-guide/install/" >&2
    exit 2
  fi

  toolbox registry add "$REGISTRY_URI"
  toolbox install toolbox-ops

  if ! command -v toolbox-vendor-ops >/dev/null; then
    echo "error: toolbox-vendor-ops still not on PATH after install." >&2
    echo "       Ensure ~/.toolbox/bin (or wherever toolbox installs shims) is on PATH." >&2
    exit 2
  fi
  echo "      installed ✓"
else
  echo "[1/3] toolbox-vendor-ops already installed: $(command -v toolbox-vendor-ops)"
fi

# --- 2. Refresh credentials for the toolbox account -------------------------

echo "[2/3] refreshing $TOOLBOX_PROFILE creds (account $TOOLBOX_ACCOUNT, role $TOOLBOX_ROLE)"
if ! command -v ada >/dev/null; then
  echo "error: 'ada' not on PATH — needed to refresh Isengard creds." >&2
  exit 2
fi
ada credentials update \
  --account "$TOOLBOX_ACCOUNT" \
  --provider isengard \
  --role "$TOOLBOX_ROLE" \
  --profile "$TOOLBOX_PROFILE" \
  --once

# --- 3. Run the recall -------------------------------------------------------

# Build one command array — conditionally include --dryrun — so the two modes
# can't drift. Array is seeded non-empty (with the binary), so "${cmd[@]}"
# expansion is safe under `set -u` on all bash versions including macOS 3.2.
cmd=(toolbox-vendor-ops
     --credentials-file ~/.aws/credentials
     --profile "$TOOLBOX_PROFILE"
     --repo "$TOOLBOX_REPO")
if [ "$MODE" = "dry-run" ]; then
  cmd+=(--dryrun)
  echo "[3/3] recalling $VERSION on $CHANNEL channel; recommend $RECOMMENDED  (DRY RUN)"
else
  echo "[3/3] recalling $VERSION on $CHANNEL channel; recommend $RECOMMENDED"
fi
cmd+=(recall --version "$VERSION" --channel "$CHANNEL" --recommended "$RECOMMENDED")

"${cmd[@]}"

if [ "$MODE" = "dry-run" ]; then
  echo ""
  echo "DRY RUN complete. Review the output above; if it looks right, re-run with --live:"
  echo "  $0 --version $VERSION --recommended $RECOMMENDED --live"
else
  echo ""
  echo "OK: toolbox recall complete."
  echo ""
  echo "Verify with a fresh install:"
  echo "  toolbox install kiro-cli --channel stable --force"
  echo "  kiro-cli --version   # should NOT be $VERSION"
  echo ""
  echo "Note: users already on $VERSION stay on it until they run 'toolbox update'."
fi
