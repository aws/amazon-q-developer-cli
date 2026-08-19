# Bug investigation

Load this reference for errors and bug reports.

1. Open the code named by the stack trace, error text, command, or symbol. Use `source-tree.md` when ownership is unclear.
2. Reconstruct the current path from caller to failure. Separate observed behavior from a proposed cause.
3. Check V1/V2 parity when an equivalent path exists. Ask which surface the user runs if that changes the diagnosis.
4. Search GitHub issues when the user asks whether the issue is known or the
   evidence suggests a regression. Use sanitized symptom terms that omit
   secrets, customer identifiers, internal hostnames, and unrelated context.
5. Confirm release claims in `crates/chat-cli/src/cli/feed.json`; do not infer shipment from a commit or issue alone.

Common places to check:

- TUI state mismatch: `packages/tui/src/stores/`
- Terminal input or visual width: `packages/twinki/`
- V2 ACP/session behavior: `crates/chat-cli-v2/src/agent/acp/`
- Agent tools, MCP, or execution: `crates/agent/src/agent/`
- Classic parity: `crates/chat-cli/src/cli/chat/`

Do not guess at history when the shallow clone lacks it, and do not file an issue unless the user asks. Issue writes follow `issue-filing.md`.
