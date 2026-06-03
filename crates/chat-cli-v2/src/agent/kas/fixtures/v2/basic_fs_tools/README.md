# v2/basic_fs_tools

V2 session captured by running this prompt through `kiro-cli chat
--agent-engine v2 --no-interactive --trust-all-tools` (kiro-cli 2.4.2):

> Do these six steps in order. Do NOT use the todo_list or task tool. Do NOT
> use a subagent. Use only file write, file read, shell execute, grep, and
> glob tools.
>
> 1. Write a file notes.md with the content "kiro fixture notes".
> 2. Read notes.md back.
> 3. Write a file greet.sh that echoes "hello fixture".
> 4. Run greet.sh with the shell tool.
> 5. Search for the word "fixture" across all files in this directory with
>    grep.
> 6. List all .sh files in this directory with glob.

Bucket: basic-FS / shell tools. Exercises `write`, `read`, `shell`, `grep`,
`glob` in a single linear session with no subagent split and no todo_list
usage. This is the smallest fixture covering the most-common V2 builtin
tools.

## Layout

- `session.json` - V2 `SessionData` (renamed from `<id>.json`)
- `messages.jsonl` - V2 `LogEntry` stream (renamed from `<id>.jsonl`)

The original session id was `8e247382-c779-440f-b1db-ed9bad1afaee` and lives
inside `session.json` as `session_id`. Tests should treat the id as opaque -
substitute at load time if needed.

## Contents

14 LogEntries:
- 1 `Prompt` (the user prompt above)
- 7 `AssistantMessage` entries (mix of text + tool calls)
- 6 `ToolResults` entries

Tool names exercised: `write` (x2), `read`, `shell`, `grep`, `glob`.

`parent_session_id` is `null` - this is a single-session capture, no subagent
fork.

## Refresh

```sh
PROMPT='Do these six steps in order. Do NOT use the todo_list or task tool. Do NOT use a subagent. Use only file write, file read, shell execute, grep, and glob tools.

1. Write a file notes.md with the content "kiro fixture notes".
2. Read notes.md back.
3. Write a file greet.sh that echoes "hello fixture".
4. Run greet.sh with the shell tool.
5. Search for the word "fixture" across all files in this directory with grep.
6. List all .sh files in this directory with glob.'

WORKDIR=$(mktemp -d)
SESSDIR=$(mktemp -d)
cd "$WORKDIR"
KIRO_TEST_SESSIONS_DIR="$SESSDIR/" \
  kiro-cli chat --agent-engine v2 --no-interactive --trust-all-tools "$PROMPT"

# Single-session capture - no subagent fork to filter for
SID=$(ls "$SESSDIR" | grep '\.json$' | head -1 | sed 's/\.json$//')
cp "$SESSDIR/$SID.json"  session.json
cp "$SESSDIR/$SID.jsonl" messages.jsonl
```
