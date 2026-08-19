#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/../../.." && pwd)
WORKFLOW="$REPO_ROOT/.github/workflows/kiro-bot-release.yml"
AGENT_CONFIG="$REPO_ROOT/crates/kiro-bot/agents/kiro-help.json"

if grep -Eq '^  eval:|kiro-bot-eval|bedrock:Retrieve|/kb-id' "$WORKFLOW"; then
    echo "release workflow must not run the retired live RAG evaluation" >&2
    exit 1
fi

PUBLISH_JOB=$(sed -n '/^  publish-image:/,$p' "$WORKFLOW")
if grep -Eq '^    needs: .*eval' <<<"$PUBLISH_JOB"; then
    echo "carrier publication must not depend on the retired RAG evaluation" >&2
    exit 1
fi

BULK_SEARCH_TOOL=$(jq -er '
    def exposes_bulk_search:
        . as $tool
        | ["grep", "glob", "code"] as $withdrawn
        | if ($tool == "*" or $tool == "@builtin" or $tool == "@builtin/" or $tool == "@builtin/*") then
            true
          elif ($tool | startswith("@builtin/")) then
            ($tool | ltrimstr("@builtin/")) as $name
            | (($name | contains("*")) or (($withdrawn | index($name)) != null))
          elif (($tool | startswith("@")) or ($tool | startswith("#")) or ($tool | startswith("subagent/"))) then
            false
          else
            (($tool | contains("*")) or (($withdrawn | index($tool)) != null))
          end;
    def guard_is_sound:
        (
            ["grep", "glob", "code", "*", "@builtin", "@builtin/", "@builtin/*", "@builtin/grep",
             "@builtin/glob", "@builtin/code", "@builtin/g*", "g*", "gre*", "*p", "*e*", "glo*"]
            | all(.[]; exposes_bulk_search)
        )
        and (
            ["read", "introspect", "@builtin/read", "@kiro-mcp/*",
             "@kiro-mcp/Taskei___list_tasks", "#agent", "#agent_*", "subagent/researcher"]
            | all(.[]; (exposes_bulk_search | not))
        );
    if guard_is_sound then
        ([.tools[]? | select(exposes_bulk_search)] | first // "")
    else
        error("bulk-search guard fixtures failed")
    end
' "$AGENT_CONFIG")
if [[ -n "$BULK_SEARCH_TOOL" ]]; then
    echo "kiro-help must not expose bulk search through $BULK_SEARCH_TOOL" >&2
    exit 1
fi

for compatibility_contract in \
    '-p kiro-knowledge-mcp' \
    'cp target/${{ env.TARGET }}/release/kiro-knowledge-mcp artifacts/' \
    'COPY artifacts/kiro-knowledge-mcp    /usr/local/bin/kiro-knowledge-mcp'
do
    if ! grep -Fq -- "$compatibility_contract" "$WORKFLOW"; then
        echo "release workflow is missing compatibility contract: $compatibility_contract" >&2
        exit 1
    fi
done

echo "kiro-bot no-RAG release contract tests passed"
