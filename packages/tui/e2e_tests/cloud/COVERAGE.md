# Cloud-sandbox test coverage map

Dark-shipped feature: cloud sessions are enabled ONLY for internal users
(any channel; rollout.rs `remote_sandbox`) or under `KIRO_TEST_MODE=1`.
External builds must show zero cloud UX — that guarantee is pinned by
`crates/chat-cli/tests/cloud_sessions_gating.rs` and `rollout.rs` unit
tests. Everything below tests the feature AS the eligible (internal or
test-mode) user sees it, plus the dark-ship boundary.

Sources: Pippin "Kiro Next - CLI Bugs" doc (bugs #1–#35 + UX list), the
basic user stories, merged fixes (#3552 #3599 #3651 #3652 #3653 #3656 #3657
#3664 #3666 #3690 #3699 #3720 #3797), and open PRs (#3689 #3691 #3693).

Tiers:

- **rust-integ** — `crates/chat-cli/tests/cloud_sessions_gating.rs` (headless
  binary, mock KAS rows, release-profile dark-ship proofs)
- **unit** — bun/vitest unit suites in `packages/tui/src`
- **e2e** — `packages/tui/e2e_tests/cloud/*.test.ts` (real TUI + published
  KAS + mock BFF; hermetic)
- **smoke** — `knight-rider-smoke.sh` (real binary in a PTY, screenshot
  evidence; mock or prod BFF)
- **manual** — needs a real BFF/sandbox or a second client (Kiro Web); listed
  so the gap is explicit, covered by the smoke script's prod mode

## Basic user stories

| Story                                                                                            | Tier(s)          | Where                                                       |
| ------------------------------------------------------------------------------------------------ | ---------------- | ----------------------------------------------------------- |
| Start cloud session (`--cloud`, checklist, ☁ footer)                                             | e2e + smoke      | cloud-sessions.test.ts t1; smoke "cloud boot"               |
| `kiro-cli --cloud` without `chat` (launcher forward)                                             | manual           | separate repo (kiro-cli-autocomplete #679); noted only      |
| Source provider not connected → gate + Kiro Web handoff                                          | e2e              | cloud-sessions.test.ts t2                                   |
| Headless (no browser) provider gate variant                                                      | unit             | cloud-urls / detach-notice suites; UX pending Figma         |
| Repo selection: interactive picker lists provider repos                                          | e2e + smoke      | cloud-sessions.test.ts t3; smoke "/repo picker"             |
| Repo picker: Tab switches panels, Selected panel interactive, checkmark accent (#28, UX6, #3656) | e2e              | cloud-repo-picker.test.ts (new)                             |
| `--repo` binds footer, defers clone                                                              | e2e + smoke      | cloud-sessions.test.ts t4; smoke "--repo"                   |
| List sessions: cloud rows + env/status columns                                                   | rust-integ + e2e | gating tests (feature-on shape); cloud-resume-concurrent t3 |
| Attach/resume with full trajectory replay                                                        | e2e              | cloud-resume-concurrent t1 (batch 2)                        |
| Resume auto-detects cloud (no --cloud flag)                                                      | e2e + unit       | cloud-resume-concurrent t1; resolve-resume-target.test.ts   |
| Resume shows "Resuming…/✓ Cloud session resumed" wording (UX, #3656)                             | unit             | cloud-startup-checklist resumed-state assertion             |
| Disconnect keeps session running; reattach hint                                                  | e2e + smoke      | cloud-sessions.test.ts t5; smoke "disconnect"               |
| /quit keep-running prompt                                                                        | e2e + smoke      | cloud-sessions.test.ts t6; smoke "quit"                     |
| Multiple concurrent sessions listed, one attached                                                | e2e              | cloud-resume-concurrent t3/t4                               |
| Delete sessions (full id + unique prefix, ambiguity)                                             | rust-integ       | cloud_sessions_gating.rs delete tests                       |
| Opt-in: same env without --cloud stays local                                                     | e2e              | cloud-sessions.test.ts t7                                   |
| /autonomous on/off: picker + [current] tag, verified mode switch (#3653 CLI; KAS 0.27.8 relay)   | e2e + smoke      | cloud-autonomous.test.ts; smoke "autonomous"; KR S15        |

## Dark-ship boundary (release safety)

| Guarantee                                                             | Tier                         | Where                                                            |
| --------------------------------------------------------------------- | ---------------------------- | ---------------------------------------------------------------- |
| `remote_sandbox` enabled ONLY for internal users (any channel)        | unit (rust)                  | rollout.rs test_remote_sandbox_enabled_for_all_internal_any_channel |
| Released `--list-sessions`: zero cloud UX even w/ stray env           | rust-integ (release profile) | cloud_sessions_gating.rs                                         |
| Released: injected cloud/remote-control KAS rows hidden (fail-closed) | rust-integ (release profile) | cloud_sessions_gating.rs                                         |
| Released JSON listing carries no cloud members                        | rust-integ (release profile) | cloud_sessions_gating.rs                                         |
| Released: `--cloud`/`--repo` rejected as unknown args                 | rust-integ (release profile) | cloud_sessions_gating.rs                                         |
| Malformed executionTarget fails CLOSED (hidden)                       | unit (rust)                  | persist.rs kas_tests                                             |
| Release-binary injected-row spot check                                | smoke                        | knight-rider-smoke.sh dark-ship section                          |

## Reported bugs → regression scenarios

Legend: ✅ covered by this change · ▶ covered when the named open PR merges ·
⛔ KAS/BFF-blocked (client-side seam pinned where possible) · 📋 manual/smoke

| #           | Bug                                                       | Status upstream                           | Regression tier                                       | Scenario                                                                                                                                         |
| ----------- | --------------------------------------------------------- | ----------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1           | Resumed session missing repo footer                       | KAS-blocked (P474219068)                  | ⛔ e2e seam                                           | resume test asserts the cloud chip today; add a footer-repo assertion when KAS ships repositories in load meta                                   |
| 2           | Completed tools replay as "interrupted"                   | Fixed (KAS + #3664)                       | ✅ e2e                                                | resume-with-history asserts the tool row renders AND the snapshot carries no Cancelled/interrupted label (cloud-resume-concurrent t1)            |
| 3           | "stream ended without done frame" on prompt-after-resume  | Fixed                                     | ⛔ needs live-turn mock; smoke prod-mode 📋           |
| 4/20        | /code error in cloud                                      | KAS remote-slash pending                  | ⛔                                                    | KAS-blocked; per-decision no per-command error test — the global no-error invariant in the KR sweep is the guard                                 |
| 5           | /agent leaks local agents after cloud→local→cloud         | CLI fixed (scope stash)                   | ✅ unit                                               | cloud-scope-stash.test.ts (exists); e2e switch test exercises the path                                                                           |
| 6/24-family | /compact error                                            | KAS remote-slash pending                  | ⛔                                                    | same — global no-error invariant                                                                                                                 |
| 7/21        | /model refused / "No models available"                    | KAS (P474219360)                          | ⛔ e2e seam                                           | /model in cloud shows gate/refusal text, never a raw RPC error                                                                                   |
| 8/22        | /context add/rm/clear errors                              | KAS (P474219380)                          | ✅ e2e gate                                           | add/rm/clear gated; `/context show` still works                                                                                                  |
| 9           | Footer "(+1 others)" plural                               | CLI fixed #3552                           | ✅ unit                                               | cloud-status.test.ts (exists)                                                                                                                    |
| 10          | Steer/queue mid-turn failed                               | Fixed                                     | ▶ e2e (#3691 guards) + 📋 smoke mid-turn              |
| 11          | Tool success reported failed                              | KAS relay                                 | ⛔ + ✅ e2e seam                                      | replay asserts completed status maps completed (t1)                                                                                              |
| 12          | Subagent/tasks view flattened                             | KAS meta strip; CLI ready #3657           | ⛔ seam; unit kas-subagent-routing (exists)           |
| 13          | "Open in browser" on headless                             | CLI fixed #3599                           | ✅ unit                                               | (merged with #3599's tests)                                                                                                                      |
| 14/24       | /knowledge "Session not found" (+ pre-prompt variant #24) | KAS transport                             | ⛔ (invariant)                                        | per-decision no dedicated test; the KR sweep's global no-error check fails on any raw wire error in any scenario                                 |
| 15          | /effort "not available on model"                          | KAS configOptions gap                     | ⛔ e2e seam                                           | /effort output is the friendly message; never raw error                                                                                          |
| 16/25       | /clear "Failed to restore agent"                          | Fixed upstream (KAS ≥0.26.14 accepts relayed set_config_option; #3687 closed unmerged) | ✅ e2e                | /clear completes without error banner (cloud-command-gates)                                                                                      |
| 17/26       | /plan refusal (+ pre-prompt variant #26)                  | KAS transport                             | ⛔ (invariant)                                        | same — global no-error invariant                                                                                                                 |
| 18/27       | /rewind fork "Internal error"                             | KAS no remote fork; ▶ #3689 hides /rewind | ▶ e2e                                                 | /rewind hidden from autocomplete + refuses with message in cloud                                                                                 |
| 19          | /chat save/load local-path error                          | CLI gated #3656                           | ✅ e2e                                                | save/load gate message (real fix parked on xianwp/cloud-chat-save-0720)                                                                          |
| 23          | Steer queue duplication on ctrl+c/esc                     | KAS #1844 + ▶ #3691                       | ▶ unit (steer-buffer guards) + 📋 smoke               |
| 29          | Resume stopped replaying mid-turn stream                  | Fixed #3664; ▶ #3693 boundary             | ✅ e2e resume; ▶ boundary event assertion             |
| 30          | Initial tools labelled "Cancelled"                        | KAS (startup prefetches)                  | ✅ e2e (CLI side) + 📋 smoke prod                     | mock serves no startup tool frames, so the e2e pins only client-synthesized rows; real prefetch path needs prod-mode smoke                       |
| 31          | session/cancel declined by BFF → missed end_turn          | KAS/BFF                                   | ⛔ 📋 smoke prod                                      |
| 32          | First msg after resume replays full history again         | No repro after #3652                      | ✅ e2e (layered-dedup pin)                            | KAS id-dedup masks the original CLI-only path against this mock; the test pins the end-to-end dedup stack; original path needs a live-turn smoke |
| 33          | Web+CLI concurrent view out of sync                       | KAS 0.23+                                 | 📋 manual (needs two clients)                         |
| 34          | Local file attach dead path                               | CLI fixed #3652                           | ✅ unit (cloud-attach.test.ts exists) + e2e send-scan |
| 35          | --resume-id "failed" (env confusion)                      | Docs/NA                                   | ✅ e2e (resume works w/o extra env in test mode)      |
| —           | `!cmd` local bash in cloud                                | CLI blocked (gate)                        | ✅ unit (cloud-shell-escape-gate exists) + e2e gate   |
| —           | /clear leaves viewport residue                            | CLI fixed #3651                           | ✅ e2e                                                | post-/clear snapshot has zero pre-clear text                                                                                                     |
| —           | /chat new premature checklist + residue                   | ▶ #3699                                   | ▶ e2e                                                 | /chat new: wiped scrollback + honest "Creating…" order                                                                                           |
| —           | A→B→A reload doesn't re-replay                            | Fixed: KAS f78f0e590 + #3699 (user rows)  | ✅ e2e                                                | switch A→B→A re-replays A's history incl. user rows (occurrence count ≥2 — scrollback keeps the first replay, so presence alone is vacuous)      |
| —           | /mcp /tools /hooks panel provenance                       | CLI fixed #3690/#3735                     | ✅ e2e                                                | cloud-panels.test.ts: /mcp and /tools show the sandbox notice, never local config                                                                |
| —           | /hooks raw "Internal error" in cloud (08/04 parity sweep) | KIRONEXT-4 (closed): Hooks v2 EP flag on in beta/gamma; prod repro 08/04 pending prod flag | ✅ e2e (client bar) + 📋 prod-smoke | cloud-panels.test.ts /hooks test pins no-raw-error + no local leak at the mock tier; the live sandbox _kiro/hooks/list shape needs prod-mode smoke |
| —           | Unrouted BFF → bare "UnknownError" (07/31 outage)         | CLI fixed #3720 guidance + #3797 classify | ✅ e2e + unit                                         | cloud-version-skew.test.ts + KR S16: MOCK_BFF_UNROUTED boot surfaces the version-skew guidance ('out of sync… Update kiro'), no silent fallback  |
| —           | /sessions current session not listed                      | UX decision pending                       | 📋                                                    |
| —           | ghost /disconnect print after detach                      | UX                                        | ▶ #3699 wipe covers                                   |
| —           | pink/white checkmark mismatch in /repo                    | CLI fixed #3656                           | ✅ e2e repo-picker accent assertion                   |

## Open-PR-gated scenarios (add when each merges)

- #3689 hide+refuse /rewind → un-gate bug 18/27 e2e
- #3691 steer buffer guards → un-gate bug 23 unit+e2e
- #3693 history_replay_complete boundary → tighten resume tests to the
  deterministic boundary event instead of text waits

Un-gated since this map was first written: #3687's e2e (fixed upstream in
KAS, the PR itself closed unmerged), #3690's panels e2e
(cloud-panels.test.ts), and #3700's A→B→A e2e (fix landed via #3699).

The gated tests are written now and marked `it.skip` with the PR number so
they light up by deleting the skip (grep `SKIP-UNTIL-PR`).
