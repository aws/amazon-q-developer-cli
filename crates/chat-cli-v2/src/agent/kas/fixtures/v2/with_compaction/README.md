# v2/with_compaction

V2 session captured by driving `kiro-cli chat --agent-engine v2
--trust-all-tools` (kiro-cli 2.5.0) interactively in a TUI. The
session must be driven interactively because `/compact` is a slash
command and does not exist in `--no-interactive` mode.

Bucket: basic-FS tools plus a `LogEntry::Compaction`. Exercises
`write` and `read` across a multi-turn session, then a `/compact`,
then one more turn that runs AFTER the compaction. This is the
fixture for the V2 `Compaction` -> KAS `tombstone { kind:
'summarization' } + assistant { operationType: 'Summary' }` mapping.

## Layout

- `session.json` - V2 `SessionData` (renamed from `<id>.json`)
- `messages.jsonl` - V2 `LogEntry` stream (renamed from `<id>.jsonl`)
- `make-large-file` - bundled generator for the deterministic
  `bigfile.txt` the session reads. Used only at capture/refresh time.

Original session id `5912f694-10d5-4b04-ad6e-e2cc4816cbc7`, cwd
`/private/tmp/kiro-fixture-compaction-u7iX`. `parent_session_id` is
null. Treat the id as opaque - substitute at load time if needed.

## Contents

45 LogEntries in order:
- 6 `Prompt`
- 22 `AssistantMessage` (mix of text + tool calls)
- 16 `ToolResults`
- 1 `Compaction` at index 42, followed by `Prompt` (43) +
  `AssistantMessage` (44) - a turn that runs after compaction

Tool names exercised: `write` (x9), `read` (x9). No shell / grep /
glob / subagent / todo_list (those live in `basic_fs_tools`).

## Compaction entry shape (index 42)

The `Compaction` `data` has three fields:

- `summary` - 3121-char summary string (the "## OBJECTIVE ..."
  factual record).
- `strategy` - `{ message_pairs_to_exclude: 2,
  context_window_percent_to_exclude: 2, truncate_large_messages:
  false, max_message_length: 25000 }`.
- `messages_snapshot` - a `Vec<Message>` of length 16.

Key finding (answers the design's open `Compaction.messages_snapshot`
decision): `messages_snapshot` does NOT contain the summary. It holds
the RETAINED recent messages (the un-summarized tail - the last two
message pairs per `message_pairs_to_exclude: 2`), starting with a
`user` toolResult and ending with the final `assistant` text of the
pre-compaction turns. The summary text lives only in the separate
`summary` field.

Implication for the V2 -> KAS converter: emit the
`tombstone { kind: 'summarization' }` + `assistant { operationType:
'Summary' }` carrying `summary`, THEN re-emit the 16
`messages_snapshot` messages after the Summary so KAS's effective
context matches V2's post-compaction rolling messages. Dropping the
snapshot would lose the retained tail.

## Refresh

`/compact` requires an interactive TUI, so this fixture cannot be
captured with `--no-interactive`. Drive it in a terminal (or via the
tmux testing skill). The bundled `./make-large-file ... --lorem` is
deterministic, so the two sizes below reproduce the exact
`bigfile.txt` contents the session read.

```sh
WORKDIR=$(mktemp -d)
SESSDIR=$(mktemp -d)
cp ./make-large-file "$WORKDIR/"
cd "$WORKDIR"
./make-large-file bigfile.txt 50000 --lorem    # first read (turn 2)

KIRO_TEST_SESSIONS_DIR="$SESSDIR/" \
  kiro-cli chat --agent-engine v2 --trust-all-tools
```

Then send these turns in order, waiting for idle between each:

1. Create hello world in C, Python, and bash using only the file
   write/read tools (write hello.c, hello.py, hello.sh; read each
   back). No todo_list/task, no subagent.
2. Read bigfile.txt in full and explain/summarize its contents.
3. Update all three programs one at a time to print goodbye world
   instead; read each back after editing.
4. (Outside the TUI) regenerate the large file so its contents
   change: `./make-large-file bigfile.txt 80000 --lorem`. Then ask the
   agent to read bigfile.txt again and describe what changed.
5. Replace all three programs one at a time with a basic calculator
   app (add/subtract/multiply/divide two numbers); read each back.
6. Run the `/compact` slash command.
7. Ask "what is in our conversation history so far?" - this turn runs
   after the compaction.

Then `/quit`, and copy the parent session out:

```sh
SID=$(ls "$SESSDIR" | grep '\.json$' | head -1 | sed 's/\.json$//')
cp "$SESSDIR/$SID.json"  session.json
cp "$SESSDIR/$SID.jsonl" messages.jsonl
```
