---
name: kiro-help-workflow
description: Evidence-first workflow for answering Kiro CLI questions, investigating bugs, and handling GitHub or Taskei requests.
---

# Kiro-help workflow

Follow this workflow for Kiro questions, setup, errors, bugs, and feature requests. Greetings and unrelated conversation need no retrieval. For a meta-question about the bot, answer from its prompt and configuration.

## References

Load only what the request needs:

- Source ownership and current V1/V2 paths: `$KIRO_HOME/agents/references/source-tree.md`
- Error and bug triage: `$KIRO_HOME/agents/references/bug-investigation.md`
- GitHub issue creation or comments: `$KIRO_HOME/agents/references/issue-filing.md`

Agent files live under `$KIRO_HOME/agents/`. Kiro CLI source is the checkout in the working directory.

## Procedure

1. **Classify.** Choose the intent: question, setup, error, bug report, feature request, GitHub write, Taskei query, meta, or off-topic. Keep this classification internal.

2. **Locate evidence.**
   - Query `search_kiro_knowledge` with a focused four-to-eight-word description and up to five results.
   - For errors, bug reports, and feature requests, also query `search_github_issues`.
   - For Taskei status or work items, use `Taskei___list_tasks`, then `Taskei___get_task` for the relevant IDs.
   - If retrieval is empty, use the user's exact symbol, flag, setting, or error text to navigate the source tree.

3. **Verify current behavior.**
   - Open the relevant source with `read`; a focused one-to-three-file investigation is preferred.
   - Start from V2 and TUI paths in `references/source-tree.md`. Read V1 when the user asks about classic behavior or the bug workflow requires a parity check.
   - Use `introspect` for exact names and schemas, then confirm behavior or defaults in source.
   - If the answer needs more than about five files, ask a narrowing question unless the user explicitly requested a broad audit.

4. **Run specialized checks.**
   - For errors and bugs, follow `references/bug-investigation.md`.
   - For a GitHub write, follow `references/issue-filing.md` completely. Do not skip deduplication, user confirmation, or Slack approval.

5. **Compose.**
   - Apply the prompt's response contract: direct answer first, standard Markdown, concise supporting detail, exact `Sources:` line last.
   - Use a short heading only when the answer has multiple real sections.
   - Include commands, configuration, or code in fenced blocks with a language tag where useful.

6. **Validate before sending.**
   - Check the prompt's answer-length, `TL;DR:`, and exact-line citation requirements.
   - Every behavior claim must be supported by source read this turn.
   - TUI claims need evidence from `packages/tui/` or `packages/twinki/`, not only Rust backend code.
   - Current behavior must not rely only on V1 unless the user asked about classic.
   - Every citation must exactly match retrieval or tool output from this turn.
   - Taskei summaries must cite task IDs, or `taskei:<room>(N of M)` for a bounded room summary.
   - If evidence conflicts, report the conflict instead of silently choosing one source.
