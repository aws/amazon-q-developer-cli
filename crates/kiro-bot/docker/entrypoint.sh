#!/bin/bash
set -euo pipefail
umask 077

CONFIG_SOURCE_DIR="${KIRO_BOT_CONFIG_SOURCE_DIR:-/etc/kiro-help}"
INSTALL_DIR="${KIRO_BOT_INSTALL_DIR:-$HOME/.kiro/bots/kiro-help}"
SETTINGS_DIR="$HOME/.kiro/settings"
SESSIONS_DIR="$HOME/.kiro/sessions"
DATA_DIR="$HOME/.local/share/kiro-cli"
WORKTREE_DIR="${KIRO_BOT_WORKTREE_DIR:-/var/lib/kiro-cli}"

if [[ -n "${KIRO_BOT_RUNTIME_UID:-}" || -n "${KIRO_BOT_RUNTIME_GID:-}" ]]; then
    RUNTIME_UID="${KIRO_BOT_RUNTIME_UID:?KIRO_BOT_RUNTIME_UID is required}"
    RUNTIME_GID="${KIRO_BOT_RUNTIME_GID:?KIRO_BOT_RUNTIME_GID is required}"
    [[ "$RUNTIME_UID" =~ ^[1-9][0-9]*$ ]] || { echo "invalid runtime UID" >&2; exit 1; }
    [[ "$RUNTIME_GID" =~ ^[1-9][0-9]*$ ]] || { echo "invalid runtime GID" >&2; exit 1; }

    if [[ "$(id -u)" == 0 ]]; then
        chmod 0700 /tmp
        chown "$RUNTIME_UID:$RUNTIME_GID" \
            "$HOME/.kiro/bots" \
            "$SETTINGS_DIR" \
            "$SESSIONS_DIR" \
            "$DATA_DIR" \
            "$WORKTREE_DIR" \
            /tmp
        exec setpriv \
            --reuid="$RUNTIME_UID" \
            --regid="$RUNTIME_GID" \
            --clear-groups \
            --no-new-privs \
            -- /usr/bin/tini -- "$0" "$@"
    fi

    test "$(id -u)" = "$RUNTIME_UID"
    test "$(id -g)" = "$RUNTIME_GID"
fi

mkdir -p "$INSTALL_DIR/policies"
mkdir -p "$SETTINGS_DIR"
mkdir -p "$SESSIONS_DIR"
mkdir -p "$DATA_DIR"
test -w "$SETTINGS_DIR"
test -w "$SESSIONS_DIR"
test -w "$DATA_DIR"

BOT_MEMBER_ID="${BOT_MEMBER_ID:?BOT_MEMBER_ID is required}"
ALLOWED_CHANNEL_ID="${ALLOWED_CHANNEL_ID:?ALLOWED_CHANNEL_ID is required}"
if [[ ! "$BOT_MEMBER_ID" =~ ^U[A-Z0-9]+$ ]]; then
    echo "invalid BOT_MEMBER_ID" >&2
    exit 1
fi
if [[ ! "$ALLOWED_CHANNEL_ID" =~ ^C[A-Z0-9]+$ ]]; then
    echo "invalid ALLOWED_CHANNEL_ID" >&2
    exit 1
fi

grep -Fq '${BOT_MEMBER_ID}' "$CONFIG_SOURCE_DIR/config.toml" || {
    echo "config.toml missing BOT_MEMBER_ID placeholder" >&2
    exit 1
}
grep -Fq '${ALLOWED_CHANNEL_ID}' "$CONFIG_SOURCE_DIR/policies/agents.cedar" || {
    echo "agents.cedar missing ALLOWED_CHANNEL_ID placeholder" >&2
    exit 1
}
sed "s|\${BOT_MEMBER_ID}|${BOT_MEMBER_ID}|g" \
    "$CONFIG_SOURCE_DIR/config.toml" > "$INSTALL_DIR/config.toml"
sed "s|\${ALLOWED_CHANNEL_ID}|${ALLOWED_CHANNEL_ID}|g" \
    "$CONFIG_SOURCE_DIR/policies/agents.cedar" > "$INSTALL_DIR/policies/agents.cedar"
if grep -Fq '${BOT_MEMBER_ID}' "$INSTALL_DIR/config.toml"; then
    echo "config.toml BOT_MEMBER_ID substitution did not complete" >&2
    exit 1
fi
if grep -Fq '${ALLOWED_CHANNEL_ID}' "$INSTALL_DIR/policies/agents.cedar"; then
    echo "agents.cedar ALLOWED_CHANNEL_ID substitution did not complete" >&2
    exit 1
fi

: "${SLACK_BOT_TOKEN:?SLACK_BOT_TOKEN is required}"
: "${SLACK_APP_TOKEN:?SLACK_APP_TOKEN is required}"
# These get interpolated into secrets.toml unquoted-escaped, so a value holding a
# quote or newline yields malformed TOML — or an injected key — that the bot only
# discovers when it parses the file, several seconds into startup. A trailing
# newline on a hand-pasted secret is the realistic way this happens. Check the
# character set rather than the xoxb-/xapp- prefixes: this is about keeping the
# generated TOML well-formed, and Slack's token shapes are not ours to pin.
for token_var in SLACK_BOT_TOKEN SLACK_APP_TOKEN; do
    [[ "${!token_var}" =~ ^[A-Za-z0-9-]+$ ]] || { echo "invalid $token_var" >&2; exit 1; }
done
cat > "$INSTALL_DIR/secrets.toml" <<EOF
[slack]
bot_token = "${SLACK_BOT_TOKEN}"
app_token = "${SLACK_APP_TOKEN}"
EOF
unset SLACK_BOT_TOKEN SLACK_APP_TOKEN

SOURCE_SHA="$(tr -d '\n' < "$CONFIG_SOURCE_DIR/source-sha")"
if [[ ! "$SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
    echo "invalid embedded source SHA: $SOURCE_SHA" >&2
    exit 1
fi

GIT_REPO="${KIRO_BOT_GIT_REPO:-github.com/kiro-team/kiro-cli.git}"
GIT_URL="${KIRO_BOT_GIT_URL:-https://${GIT_REPO}}"
mkdir -p "$WORKTREE_DIR"
git -C "$WORKTREE_DIR" init --quiet
git -C "$WORKTREE_DIR" remote remove origin 2>/dev/null || true
git -C "$WORKTREE_DIR" remote add origin "$GIT_URL"

if [[ -n "${GH_PAT:-}" ]]; then
    ASKPASS="$(mktemp /tmp/kiro-git-askpass.XXXXXX)"
    trap 'rm -f "${ASKPASS:-}"' EXIT
    cat > "$ASKPASS" <<'EOF'
#!/bin/sh
case "$1" in
    *Username*) printf '%s\n' x-access-token ;;
    *Password*) printf '%s\n' "${GH_PAT:?}" ;;
esac
EOF
    chmod 0700 "$ASKPASS"
    export GIT_ASKPASS="$ASKPASS"
    export GIT_TERMINAL_PROMPT=0
fi

git -C "$WORKTREE_DIR" fetch --quiet --depth 1 origin "$SOURCE_SHA"
git -C "$WORKTREE_DIR" checkout --quiet --detach FETCH_HEAD
test "$(git -C "$WORKTREE_DIR" rev-parse HEAD)" = "$SOURCE_SHA"

if [[ -n "${ASKPASS:-}" ]]; then
    rm -f "$ASKPASS"
    unset ASKPASS GH_PAT GIT_ASKPASS GIT_TERMINAL_PROMPT
    trap - EXIT
fi

exec kiro-bot start kiro-help --foreground
