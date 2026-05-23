# canonical-kas-session

A real KAS session captured by running this prompt through `kiro-cli chat
--agent-engine=kas --non-interactive --trust-all-tools`:

> Create a directory test-scripts/ here. Inside, write hello.sh that echoes
> Hello World, then read the file back, then run it with the shell tool.
> Then use the subagent tool to create three more files: test-scripts/hello.py
> (Python printing Hello World), test-scripts/hello.js (Node printing Hello
> World), and test-scripts/hello.c (C printing Hello World). Keep each minimal.

The session subdirectory is named with the original session id KAS
generated. Layout matches `~/.kiro/sessions/<workspaceHash>/<id>/` exactly:
`session.json`, `messages.jsonl`, plus a `snapshots/` tree of file-content
checkpoints.

To refresh, run the prompt above against KAS and replace the session
subdir with the new one (rename the directory to its new id). Tests
auto-discover the single `sess_*` subdir and substitute id, workspace
paths, and timestamps at seed time.
