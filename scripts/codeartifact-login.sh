#!/bin/bash
# Authenticate to CodeArtifact for @kiro npm packages.
# Run this before `bun install` when working with KAS (--agent-engine=kas).
# Token expires after 12h — re-run if you get 401/403 errors.

set -euo pipefail

DOMAIN="kiro-agent"
OWNER="273221486557"
REGION="us-west-2"
PROFILE="kiro-agent-source"
REGISTRY="kiro-agent-273221486557.d.codeartifact.us-west-2.amazonaws.com/npm/npm-packages"

# 1. Get AWS credentials
echo "Getting AWS credentials..."
ada credentials update --account "$OWNER" --role npm-dev-access-role --provider isengard --profile "$PROFILE" --once

# 2. Get scoped auth token
echo "Getting CodeArtifact token..."
TOKEN=$(AWS_PROFILE="$PROFILE" aws codeartifact get-authorization-token \
  --domain "$DOMAIN" --domain-owner "$OWNER" \
  --query authorizationToken --output text \
  --region "$REGION")

# 3. Write .npmrc at repo root (bun workspaces resolve .npmrc from root)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_NPMRC="$SCRIPT_DIR/../.npmrc"
cat > "$REPO_NPMRC" <<EOF
@kiro:registry=https://${REGISTRY}/
@amzn:registry=https://${REGISTRY}/
//${REGISTRY}/:always-auth=true
//${REGISTRY}/:_authToken=${TOKEN}
EOF

echo "✓ CodeArtifact token written to .npmrc. Run 'bun install' to fetch @kiro and @amzn packages."
