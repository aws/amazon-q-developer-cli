# Filing issues (dedup-first)

Reference for the kiro-help bot. Load this when the user asks to file an issue or comment on one. Do not preemptively volunteer to file.

## When to load this reference

Phrases that should trigger this flow:
- "can you file this?" / "open an issue for X" / "report this"
- "comment on issue #123 with..." / "add a note to that issue"

If the user is just *asking about* a bug (not asking to file one), follow the bug-investigation workflow instead — see `references/bug-investigation.md`.

## Filing a new issue

1. **Search existing issues first.** Call `search_github_issues` with a focused query built from the user's symptom — the error string, the failing command, the unexpected behavior. Run 1-2 queries with different phrasings if the first returns nothing.
2. **For bug reports, also confirm in source.** A "bug" that's actually documented behavior shouldn't become an issue — open the relevant file with `read` to verify the symptom is real before proposing an issue. If the source shows this is by-design, surface that to the user instead of filing.
3. **Present matches to the user.** Reply with up to 3 of the closest matches as a numbered list with title + issue number + 1-line state (open/closed, last activity if available). Ask: *"Is this what you're hitting, or is this a different bug worth filing?"*
   - If you found 0 matches, say so explicitly and propose a draft title + 2-3 sentence body for the user to confirm.
4. **Wait for user confirmation.** Do not proceed without an unambiguous "yes file it" / "this is new" / "go ahead". A vague reply is a stop signal — ask again.
5. **Request the write-tool approval reaction.** When `create_github_issue` is invoked, Slack will gate it via the reaction-approval flow. Wait for the approval signal in this turn before claiming the issue was filed.
6. **Confirm with the issue link.** Once the tool returns, reply with the issue number/URL. Cite it in the `Sources:` line for any follow-up.

## Commenting on an existing issue

Same gate: confirm the user wants a comment, draft the comment for them to review, wait for the Slack reaction-approval signal, then post via `comment_on_existing`. Never modify or close an issue silently.

## Hard rules

- Never file an issue without doing the dedup search and getting explicit user confirmation first.
- Never claim to have filed/commented unless you just received the Slack reaction-approval signal in this turn.
- Never echo user-pasted secrets, configs, or stack traces into the issue body without sanitizing — strip tokens, internal hostnames, and customer identifiers.
