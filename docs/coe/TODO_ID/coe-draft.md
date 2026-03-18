# COE: Kiro CLI Context Window Overflow Validation Errors and Infinite Compaction Loops

**Date:** 2026-02-05
**Severity:** SEV-2
**Duration:** ~13 days (2026-02-05 to 2026-02-18)
**Related Ticket:** [P382946346](https://t.corp.amazon.com/P382946346)

---

## Summary

On 2026-02-05, a downstream behavior change in Claude Opus 4.6 (via Bedrock) began returning a new `prompt is too long` error when the user's message alone exceeded the context window. Maestro did not map this to the expected `ContextWindowOverflow` code, returning `ValidationException` to the CLI instead.

The CLI team created P382946346 (SEV-2) on Feb 13. An initial Maestro fix that day resolved the `ValidationException` but exposed a bug with the CLI's compaction logic. The CLI assumed the user's current message could never itself cause overflow, leading to infinite compaction loops.

CLI v1.26.2 with the compaction logic fix was released on Feb 18. The Maestro backend fix adding `prompt is too long` as a `ContextWindowOverflow` error code was gated on CLI version >= 1.26.2, which removed auto-compaction for all older CLI versions.

---

## Background

The Kiro CLI chat flow involves three layers:

1. **CLI Client** — collects user input, conversation history, and tool results into a `ConversationState` payload
2. **Maestro/RTS** (Consolas Runtime Service) — receives the payload, applies prompt templates, and forwards to the model via MPS (Model Proxy Service)
3. **Bedrock (Claude)** — performs inference and streams the response back

When the total payload exceeds the model's context window, Bedrock returns a `ValidationException` with a message describing the specific overflow condition. The Maestro Python sidecar intercepts this and checks the error message against a list of known "input too long" strings. If matched, it returns `ContextWindowOverflowException` to the CLI; if not matched, it returns a generic `"Request input fails validation"` as a `ValidationException`.

Pre-incident, the sidecar recognized three Bedrock error messages as context window overflow:

1. `"Input is too long for requested model"`
2. `"input length and 'max_tokens' exceed context limit"`
3. `"too many total text bytes"`

When Opus 4.6 began returning a fourth message — `"prompt is too long: 209421 tokens > 200000 maximum"` — it did not match any of the three known strings. The sidecar returned the generic `ValidationException` instead of the expected `ContextWindowOverflowException`.

The CLI handles `ContextWindowOverflowException` by "compacting" — using an LLM call to summarize older conversation history into a shorter form, then retrying the original request with the compacted history. But `ValidationException` bypassed this handling entirely, surfacing as an unrecoverable error to the user.

The post-incident fix (CR-254678454) added MESSAGE_4 (`"prompt is too long"`) to the recognized list, but gated it on CLI version >= 1.26.2 via user-agent header parsing. For older CLI versions, the sidecar intentionally does **not** match MESSAGE_4 — this prevents the infinite compaction loop that would occur on pre-fix CLI versions, at the cost of disabling auto-compaction for those users.

> **TODO:** Add architecture diagram showing CLI → Maestro/RTS → Bedrock flow with compaction retry loop.

The critical assumption in the compaction logic was that overflow was always caused by accumulated history — never by the user's current message alone. This assumption held until the Opus 4.6 behavior change.

---

## Customer Impact

- **Duration:** ~13-day incident window (Feb 5–18, 2026). Low-volume impact Feb 5–11 (error present but not visible in daily aggregates); acute impact concentrated in the final 6 days (Feb 12–18) after error volume spiked and the initial Maestro fix exposed the compaction loop.
- **ValidationException rate:** Jumped from baseline ~5,600/day to peak 81,456 on Feb 13 (1.36% of all requests)
- **ContextWindowOverflow rate:** Spiked to 297,187 on Feb 17 (4.68% of all requests) during Maestro rollback/redeploy
- **Total incident-period ValidationException events:** ~376,000 (Feb 7–18, though the first detectable spike was Feb 12)
- **Total incident-period ContextWindowOverflow events:** ~707,000 (Feb 7–18)
- **Regions Impacted:** IAD, FRA (all Maestro prod regions)
- **User Experience:** Users hitting context limits experienced either unrecoverable errors (ValidationException) or infinite compaction loops that consumed resources without resolution. Users had to manually switch models or restart conversations. No proactive customer communication or workaround guidance was issued during the incident (see Action Items: customer communication mechanism).

> **Note:** The above error counts are likely inflated by the infinite compaction loop — a single affected user session could generate hundreds of repeated error events as the CLI retried the same failing request in a loop. Unique affected user/account counts are needed to assess true blast radius.

> **TODO:** Pull Kibana data to determine unique affected user/account count and breakdown by model. CloudWatch `QCLIMessageResponseError` dimension does not include user-level granularity. As a rough estimate: with ~4.8M daily requests and assuming ~50–100 requests per active user per day, the user base is approximately 48K–96K daily active users. At a peak 1.36% ValidationException rate, this suggests roughly 650–1,300 unique users may have been affected on the worst day (Feb 13). This estimate should be validated against Kibana data before approval.

**Blast Radius Reduction:** Users with smaller context usage or those not using Opus 4.6 were unaffected initially. After the Maestro fix on Feb 13, the infinite compaction loop affected all Claude models when `next_user_message` alone exceeded the context window.

**Post-Fix Recovery:** ValidationException rates remained elevated post-fix (0.36–0.54% on Feb 19–20 vs 0.04–0.18% baseline). This is expected: the CLI-side fix shipped in v1.26.2, but users on older CLI versions continued to hit the bug until they updated. Full recovery depends on CLI auto-update adoption across the user base.

> **TODO:** Add metric graphs:
> - Figure 1: ValidationException daily rate (%) Feb 1–25, showing baseline → spike → recovery
> - Figure 2: ContextWindowOverflow daily rate (%) Feb 1–25, showing Feb 17 spike during Maestro rollback
>
> Source data is in the Appendix: Daily Metrics Summary table.

---

## Incident Analysis

### Detection

**How was the event detected?**
The behavior change began on Feb 5 at 19:59 UTC (confirmed via MPS logs), but no CLI-team alarms fired. The CLI team became aware on Feb 13 when the issue was discovered through dashboard review (the ticket description notes "A fix is currently in progress," indicating investigation had already begun). The first user report was via Slack on Feb 5 at ~18:29 UTC ([#kiro-cli-contributors thread](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770316194619669)), with subsequent reports on Feb 9, 10, 11, and 12 before the team began investigating.

**How could detection time be improved?**
The `QCLI-SuccessRateDown` alarm classifies `ContextWindowOverflow` as a user error, excluding it from the success rate calculation. This meant the alarm did not fire despite the dramatic spike in overflow errors caused by a system-side behavior change. An anomaly detection alarm on `ContextWindowOverflow` rate (firing when rate exceeds 2x the 7-day rolling baseline for >30 minutes) would have alerted the CLI team on Feb 5 — reducing detection time from ~8 days to under 1 hour. The baseline ContextWindowOverflow rate of 1–2% means the alarm threshold must be tuned carefully to avoid false positives; the 2x multiplier and 30-minute sustained window are designed to filter normal daily variance while catching genuine spikes like the Feb 17 event (4.68%, a 2.5x+ increase). For ValidationException specifically: baseline was 0.04–0.18% of requests; Feb 12 hit 0.56% (3x baseline), and Feb 13 hit 1.36% (7.5x baseline) — both would have triggered a 2x anomaly alarm within 30 minutes of sustained elevation.

### Diagnosis & Mitigation

**How did you identify the root cause?**
Investigation started from the Slack user report and dashboard review. Analysis of the Maestro deployment pipeline diff revealed a branch change in `QDeveloperMaestroPythonSidecar` that reintroduced the "input is too long" check. Direct testing through Maestro confirmed that Opus 4.6 (and later all Claude Sonnet models) now returned `ContextWindowOverflow` immediately when `next_user_message` alone exceeded the context window — a behavior change from the previous model version.

**How did you mitigate?**
1. Maestro rollback on Feb 17 to stop the `ValidationException` spike
2. New Maestro fix (CR-254678454) deployed to FRA Feb 18, IAD Feb 18 — introduced a CLI-specific context window overflow message
3. CLI v1.26.2 released Feb 18 with fixed compaction logic that handles `next_user_message` overflow (PR #853)

**How could mitigation time be improved?**
The fix required coordinated changes across two codebases (Maestro backend + CLI client) owned by different teams. Cross-team coordination added approximately 5 days to mitigation (Feb 13 root cause identified → Feb 18 fix deployed). The specific blocker was that the CLI team did not have permission to override Maestro pipeline approvals — per P385355150: "I don't have permission to override approvals, we need assistance from Maestro oncall to push these changes to Maestro Prod." Concrete improvements: (1) propose and negotiate scoped deployment permissions for the CLI team to deploy Maestro changes affecting CLI error handling, enabling same-day deployment without waiting for Maestro oncall; (2) establish a shared staging environment where CLI + Maestro changes can be validated together before prod deployment. A pre-agreed fast-path process targeting <24h for cross-team SEV-2 fixes would have significantly reduced mitigation time — removing the ~5-day cross-team deployment blocker would have left only the diagnosis and fix development time (~1–2 days), reducing total incident duration from ~13 days to ~2–3 days.

> **Why not rollback?** Rolling back the CLI to a pre-compaction version was not viable — the compaction feature was not new, and the bug was triggered by a downstream model behavior change, not a CLI deployment. Rolling back Maestro was eventually done on Feb 17 as a stopgap, but this only addressed the post-Feb-13 compaction loop phase, not the original ValidationException issue.

> **Why wasn't a workaround communicated?** A viable user workaround existed throughout the incident: switching to a non-Opus model or starting a new conversation. No proactive communication was issued to users during the 13-day window — the team had no established SOP for incident communication (see Action Items: interim customer communication SOP).

### Contributing Factors

**Was this triggered by a change?**
Yes — a downstream Bedrock/Claude behavior change with Opus 4.6 changed how context window overflow errors were returned. This was compounded by a Maestro deployment that reintroduced an "input is too long" check.

**Did an existing backlog item address this risk?**
No. The CLI compaction logic had a fundamental assumption that `next_user_message` alone could never cause a context window overflow. This assumption was never validated or tested.

**When was the last ORR performed?**
No ORR has been performed for the CLI compaction/retry code path. The compaction feature was added without a dedicated operational readiness review. This is a contributing factor — an ORR would likely have identified the missing test coverage for overflow-after-compaction scenarios and the implicit assumption about `next_user_message` size.

**Did you have a test for this failure mode?**
No. There were no integration tests for the scenario where `next_user_message` alone exceeds the context window, nor was there a test harness for simulating backend overflow responses. Compaction tests only covered the happy path of history-based overflow. This gap is addressed by the HIGH-priority action item to build a backend error simulation harness and add overflow-after-compaction tests.

**How did you confirm recovery?**
Recovery was confirmed through: (1) gamma validation with CLI v1.26.3-nightly.2 on Feb 18, verifying that `ContextWindowOverflow` on `next_user_message` now returns a user-facing error instead of looping; (2) prod deployment of Maestro fix CR-254678454 to both IAD and FRA regions on Feb 18; (3) CLI v1.26.2 released to stable on Feb 18; (4) post-fix monitoring through Feb 27 confirming ValidationException rates returning toward baseline as users updated. Ticket was closed on Feb 27 after sustained recovery was confirmed.

---

## Timeline

All times in UTC.

| Time (UTC) | Event |
|------------|-------|
| Feb 5, ~18:29 UTC | First user report in [#kiro-cli-contributors](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770316194619669) — ValidationException with Opus 4.6 1M context. (This specific request could not be confirmed as "prompt is too long" in MPS; may be related to [COE-387009](https://www.coe.a2z.com/coe/387009/content).) |
| Feb 5, 19:59 UTC | **START OF CUSTOMER IMPACT** — First confirmed `"prompt is too long"` error in MPS logs. Model: `us.anthropic.claude-opus-4-6-v1`, 811,381 tokens > 200,000 maximum. Zero occurrences in MPS logs before this (verified Feb 1–5 19:59 UTC, 200B+ records scanned). |
| Feb 9, 16:25 UTC | [Second user report](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770654766815339) — specific to Opus 4.6. Confirmed as "prompt is too long" via MPS logs. |
| Feb 10–12 | Additional user reports: [Feb 10](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770752481595229), [Feb 11](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770819301090339) (Opus 4.5 now affected), [Feb 12 AM](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770911556920339), [Feb 12 PM](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770932313882719) (Opus 4.5 and 4.6 failing to auto-compact) |
| Feb 12, ~17:28 UTC | [Thread announcing the issue](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770946114722479) in #kiro-cli-contributors |
| Feb 13, 01:21 | CLI team discovers issue; P382946346 created (ticket description notes "A fix is currently in progress"). Maestro fix (CR-253681011) was already in flight. **Note: No CLI-team alarms fired between Feb 5 and Feb 13 — detection was via manual dashboard review.** |
| Feb 13, 01:25 | CR-253681011 merged — initial Maestro fix for "prompt is too long" message (CR was in flight before ticket creation) |
| Feb 13, ~02:00 | Maestro fix reaching gamma |
| Feb 14, ~07:00 | Maestro fix deployed to prod — resolves ValidationException but exposes infinite compaction loop |
| Feb 14, ~08:00 | [User reports infinite compaction loops in Slack](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1771028379163469) — ticket reopened |
| Feb 14, ~09:00 | CLI team re-engages investigation; begins analyzing compaction retry behavior in response to user reports |
| Feb 15–16 | Investigation continued with reduced team availability (weekend) — analyzing compaction retry behavior and identifying that `next_user_message` overflow is the root cause |
| Feb 17, ~05:00 | Root cause analysis posted: `next_user_message` itself causing overflow |
| Feb 17, ~06:00 | ContextWindowOverflow spike confirmed on [Weekly Dashboard](https://w.amazon.com/bin/view/AWS/KiroCLI/Operations/Dashboards/Weekly) — 297,187 events (4.68% of requests) |
| Feb 17, ~08:30 | Maestro rollback completed — overflow spike subsides |
| Feb 17, ~12:00 | CR-254678454 merged — new CLI-specific overflow message in Maestro |
| Feb 18, ~05:00 | Maestro deployed to Prod FRA |
| Feb 18, ~07:54 | Bedrock Anthropic 503 alarm fires (unrelated Opus 4.6 availability drop, [P385427665](https://t.corp.amazon.com/P385427665)) |
| Feb 18, ~14:00 | CLI PR #853 merged — compaction fix |
| Feb 18, ~18:00 | CLI v1.26.2 verified in gamma; v1.26.1 confirmed broken |
| Feb 18, ~22:00 | Maestro IAD deployment completed |
| Feb 18, ~22:30 | **END OF CUSTOMER IMPACT** — CLI v1.26.2 released to stable |
| | **POST-INCIDENT** |
| Feb 21 | Follow-up: SemVer prerelease version matching issue found and fixed (CR-255195219) |
| Feb 27 | Validated nightly build in Prod, ticket closed |

---

## 5 Whys

### Root Cause Analysis

**1. Why did users experience ValidationException errors and infinite compaction loops?**
Because the CLI's context window overflow handling could not process the new error format returned by Opus 4.6, and the compaction retry logic had a fundamental flaw.

**2. Why couldn't the CLI handle the new error format?**
Because Opus 4.6 returned `prompt is too long` instead of the expected `Input is too long.` message. Maestro did not map this new message to the `ContextWindowOverflow` error code the CLI expected.
→ **ACTION:** Maestro should normalize all model-specific overflow messages to a single, stable error code for clients (CR-254678454 — completed).

**3. Why did fixing the error mapping expose infinite compaction loops?**
Because the CLI compaction logic assumed that `next_user_message` alone could never cause a `ContextWindowOverflow`. When the model started returning overflow errors for the message itself (not just history), the CLI would compact history, retry with the same oversized message, get the same error, and loop forever.
→ **ACTION:** CLI compaction logic updated to detect when `next_user_message` itself causes overflow and truncate or fail gracefully (PR #853 — completed).

**4. Why wasn't this caught before reaching users?**
Because there were no integration tests for the scenario where `next_user_message` alone exceeds the context window. The assumption that this could never happen was implicit in the code but never documented or validated — no design document or architectural decision record existed for the compaction feature that would have surfaced this assumption for review. There was no test infrastructure for simulating backend overflow responses — compaction tests only covered the happy path of history-based overflow. Building such a harness was never prioritized because the overflow-on-user-message scenario was assumed impossible.
→ **ACTION:** Add integration tests for overflow-after-compaction scenarios, including `next_user_message` overflow. Establish a test harness that can simulate backend error responses for compaction retry paths. Document compaction design assumptions and invariants.

**5. Why didn't our alarms detect this sooner?**
Because `ContextWindowOverflow` is classified as a "user error" in our alarm pipeline, excluding it from the `QCLI-SuccessRateDown` alarm. This classification was made during initial alarm setup without a review process for reclassification when error patterns change. A system-side behavior change that dramatically increases user errors is invisible to our current alerting.
→ **ACTION:** Add spike-detection alarm for `ContextWindowOverflow` rate that fires on abnormal increases, regardless of user/system classification. Additionally, establish a quarterly review of error classifications in the alarm pipeline to catch misclassifications before they mask incidents.

### Duration Analysis

**1. Why did mitigation take 11 days?**
Because the fix required coordinated changes across Maestro (backend) and CLI (client), owned by different teams with different deployment pipelines.

**2. Why did cross-team coordination take so long?**
Because the CLI team did not have permissions to deploy Maestro changes directly, requiring Maestro oncall assistance ([P385355150](https://t.corp.amazon.com/P385355150)). Additionally, the initial fix (CR-253681011) was incomplete — it resolved ValidationException but exposed the deeper compaction bug, requiring a second round of fixes.

**3. Why was the initial fix (CR-253681011) incomplete?**
Because it was scoped narrowly to map the new "prompt is too long" error message to the existing `ContextWindowOverflow` code. The compaction loop behavior that this would trigger was not tested — the fix was validated only against the ValidationException symptom, not the full retry flow. The CR was reviewed and merged under time pressure during an active SEV-2, and the review did not include end-to-end testing of the compaction retry path.
→ **ACTION:** Produce a proposal for a fast-path cross-team deployment process for SEV-2 incidents and get stakeholder agreement from CLI and Consolas RTS teams.

---

## What Went Well

- **Routine dashboard review caught the issue.** The CLI team's weekly dashboard review identified the anomaly on Feb 13 — without this practice, detection could have been delayed even further since no CLI-team alarms fired.
- **Root cause analysis identified both the surface bug and the deeper flaw.** The initial Maestro fix (CR-253681011) resolved the immediate ValidationException, and when that exposed the compaction loop, the team correctly identified the deeper architectural assumption rather than applying another surface-level patch.
- **Cross-team coordination produced a comprehensive fix.** The CLI and Consolas RTS teams collaborated to deliver both a backend fix (CR-254678454) and a client-side fix (PR #853), addressing the problem at both layers.
- **The initial Maestro fix was already in flight before the ticket was created.** CR-253681011 was merged at 01:25 UTC, just 4 minutes after P382946346 was filed at 01:21 — indicating the team was already investigating before formal tracking began.

---

## Lessons Learned

- **[LL1]** Client-side error handling should never assume specific error message formats from downstream models. Error codes should be stable contracts between backend and client.
- **[LL2]** Compaction/retry logic must handle the case where the retried content itself is the problem, not just the history. Infinite retry loops are a critical failure mode.
- **[LL3]** Classifying errors as "user errors" in alarm pipelines can mask system-caused spikes. Anomaly detection on error rates is needed regardless of classification.
- **[LL4]** Cross-team deployment dependencies during incidents significantly extend mitigation time. Pre-established fast-paths are essential.
- **[LL5]** The CLI team had no alarms that would have detected this incident. Detection relied on manual weekly dashboard review — a practice that worked here but is not documented in oncall runbooks and does not scale.

---

## Action Items

> **TODO:** Assign owners and due dates after team discussion.

| Priority | Action | Owner | Due Date | Status |
|----------|--------|-------|----------|--------|
| HIGH | Deploy anomaly detection alarm that fires when `ContextWindowOverflow` rate exceeds 2x the 7-day rolling baseline for >30 minutes. Exit: alarm deployed and verified against historical Feb 17 data (4.68% rate, 2.5x+ baseline) to confirm it would have fired | TODO | TODO | Open |
| HIGH | Add integration tests for overflow-after-compaction scenarios: (1) `next_user_message` alone causes overflow, (2) overflow after successful compaction retry. Build a test harness for simulating backend error responses (ContextWindowOverflow, ValidationException). Exit: test harness built and tests passing in CI | TODO | TODO | Open |
| HIGH | Document the [Weekly Dashboard](https://w.amazon.com/bin/view/AWS/KiroCLI/Operations/Dashboards/Weekly) in metrics_and_telemetry.md with usage instructions. Exit: dashboard documented in runbook | TODO | TODO | Open |
| MEDIUM | Add a secondary alarm on `ValidationException` rate using anomaly detection (separate from the existing `QCLIErrorCount` threshold alarm). Exit: alarm deployed, fires on >3x baseline | TODO | TODO | Open |
| MEDIUM | Establish quarterly review of error classifications in the alarm pipeline (user vs. system) to catch misclassifications before they mask incidents. Exit: first review completed, recurring calendar event created, review checklist added to oncall runbook | TODO | TODO | Open |
| HIGH | Produce a proposal for fast-path cross-team deployment process for SEV-2 incidents and schedule a review meeting with CLI and Consolas RTS teams. Cross-team deployment friction accounted for 5 of 13 days of incident duration. Exit: proposal document written and review meeting scheduled within 30 days | TODO | TODO | Open |
| MEDIUM | Add runbook section for investigating infinite compaction loops: symptoms, CloudWatch queries, and mitigation steps. Exit: section added to metrics_and_telemetry.md | TODO | TODO | Open |
| MEDIUM | Add example CloudWatch CLI queries to metrics runbook for querying error spikes by `failureReason` dimension. Exit: at least 3 example queries documented | TODO | TODO | Open |
| LOW | Investigate Kibana CLI/API access (e.g., OpenSearch REST API via curl) for oncall engineers. Exit: documented alternative to web UI, or determination that none is viable | TODO | TODO | Open |
| LOW | Produce a design proposal for in-CLI incident notification banner (display mechanism, activation SOP, message format). Exit: design doc written and reviewed by team | TODO | TODO | Open |
| HIGH | Document an interim customer communication SOP for active incidents using Slack announcements and/or email, to be used until the in-CLI banner is built. No proactive communication was issued during this 13-day incident. Exit: SOP added to oncall runbook | TODO | TODO | Open |
| MEDIUM | Add SEV-2 fix validation checklist requiring end-to-end testing of affected code paths before merge, not just the immediate symptom. Exit: checklist added to oncall runbook and linked from CR template | TODO | TODO | Open |
| MEDIUM | Schedule a dedicated ORR for the CLI compaction/retry code path. Exit: ORR completed, findings documented, and any resulting action items tracked in the team backlog | TODO | TODO | Open |
| MEDIUM | Document compaction feature design assumptions and invariants (e.g., "compaction assumes overflow is caused by accumulated history, not the current user message"). Exit: design doc written and reviewed by team, covering all implicit assumptions in the compaction/retry code path | TODO | TODO | Open |
| — | **COMPLETED:** Maestro backend fix to return `ContextWindowOverflow` for oversized `next_user_message` (CR-254678454) | — | Feb 18 | Done |
| — | **COMPLETED:** CLI compaction logic updated to detect and handle `next_user_message` overflow (PR #853, CLI v1.26.2) | — | Feb 18 | Done |
| — | **COMPLETED:** SemVer prerelease version matching fix (CR-255195219) | — | Feb 21 | Done |

---

## Related Documents

- **Primary Ticket:** [P382946346](https://t.corp.amazon.com/P382946346)
- **Maestro Deployment Ticket:** [P385355150](https://t.corp.amazon.com/P385355150)
- **CLI Release Ticket:** [P385360723](https://t.corp.amazon.com/P385360723)
- **Bedrock Alarm (concurrent):** [P385427665](https://t.corp.amazon.com/P385427665)
- **Code Reviews:** [CR-253681011](https://code.amazon.com/reviews/CR-253681011), [CR-254678454](https://code.amazon.com/reviews/CR-254678454), [CR-255195219](https://code.amazon.com/reviews/CR-255195219)
- **CLI PR:** [kiro-cli PR #853](https://github.com/kiro-team/kiro-cli/pull/853)
- **Pipeline:** [QDeveloperMaestro](https://pipelines.amazon.com/pipelines/QDeveloperMaestro/)
- **Dashboard:** [Weekly Dashboard](https://w.amazon.com/bin/view/AWS/KiroCLI/Operations/Dashboards/Weekly)
- **Slack Threads:** [#dae-ops incident thread](https://amzn-aws.slack.com/archives/C08RACJKED9/p1771270944.151269), [User report](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1771028379163469)
- **Runbook:** [Kiro CLI Runbook](https://github.com/kiro-team/kiro-cli/blob/main/docs/oncall/runbook.md)

---

## Appendix: CloudWatch Metric Data

### Example Queries

Query `ContextWindowOverflow` daily counts:
```bash
env $(ada cred print --account 421629052180 --role ReadOnly --format env) \
  aws cloudwatch get-metric-statistics \
  --namespace "Toolkit" \
  --metric-name "QCLIMessageResponseError" \
  --dimensions Name=product,Value=CodewhispererForTerminal Name=failureReason,Value=ContextWindowOverflow \
  --start-time "2026-02-01T00:00:00Z" \
  --end-time "2026-02-25T00:00:00Z" \
  --period 86400 \
  --statistics Sum \
  --region us-east-1
```

Query `ValidationException` daily counts:
```bash
env $(ada cred print --account 421629052180 --role ReadOnly --format env) \
  aws cloudwatch get-metric-statistics \
  --namespace "Toolkit" \
  --metric-name "QCLIMessageResponseError" \
  --dimensions Name=product,Value=CodewhispererForTerminal Name=failureReason,Value=ValidationException \
  --start-time "2026-02-01T00:00:00Z" \
  --end-time "2026-02-25T00:00:00Z" \
  --period 86400 \
  --statistics Sum \
  --region us-east-1
```

Query total message count (for rate calculation):
```bash
env $(ada cred print --account 421629052180 --role ReadOnly --format env) \
  aws cloudwatch get-metric-statistics \
  --namespace "Toolkit" \
  --metric-name "QCLIMessageCount" \
  --dimensions Name=product,Value=CodewhispererForTerminal \
  --start-time "2026-02-01T00:00:00Z" \
  --end-time "2026-02-25T00:00:00Z" \
  --period 86400 \
  --statistics Sum \
  --region us-east-1
```

### Daily Metrics Summary

| Date | ContextWindowOverflow | ValidationException | Total Messages | Overflow % | Validation % |
|------|----------------------:|--------------------:|---------------:|-----------:|-------------:|
| Feb 1 (baseline) | 21,953 | 848 | 1,939,383 | 1.13% | 0.04% |
| Feb 2 (baseline) | 57,397 | 3,431 | 4,786,798 | 1.20% | 0.07% |
| Feb 3 (baseline) | 93,658 | 3,195 | 5,505,411 | 1.70% | 0.06% |
| Feb 4 (baseline) | 113,735 | 7,789 | 5,620,453 | 2.02% | 0.14% |
| Feb 5 (baseline) | 117,888 | 8,952 | 5,749,764 | 2.05% | 0.16% |
| Feb 6 (baseline) | 94,932 | 9,464 | 5,370,135 | 1.77% | 0.18% |
| **Feb 7** | 24,608 | **8,208** | 2,159,643 | 1.14% | **0.38%** |
| Feb 8 | 18,555 | 3,972 | 2,193,195 | 0.85% | 0.18% |
| Feb 9 | 58,185 | 10,357 | 5,489,587 | 1.06% | 0.19% |
| Feb 10 | 74,841 | 9,970 | 6,271,423 | 1.19% | 0.16% |
| Feb 11 | 102,972 | 8,979 | 6,286,435 | 1.64% | 0.14% |
| Feb 12 | 92,224 | **36,046** | 6,447,828 | 1.43% | **0.56%** |
| **Feb 13** | 5,694 | **81,456** | 5,995,360 | 0.09% | **1.36%** |
| Feb 14 | 16,333 | **26,894** | 2,585,311 | 0.63% | **1.04%** |
| Feb 15 | 715 | **21,351** | 2,301,022 | 0.03% | **0.93%** |
| Feb 16 | 2,017 | **53,996** | 5,316,728 | 0.04% | **1.02%** |
| **Feb 17** | **297,187** | **47,896** | 6,355,400 | **4.68%** | **0.75%** |
| **Feb 18** | 13,924 | **66,873** | 6,555,254 | 0.21% | **1.02%** |
| Feb 19 (post-fix) | 25,580 | 37,213 | 6,872,838 | 0.37% | 0.54% |
| Feb 20 (post-fix) | 37,596 | 25,032 | 6,942,461 | 0.54% | 0.36% |

**Period Averages:**
- Pre-incident (Feb 1–6): Overflow ~83,260/day, Validation ~5,613/day
- Incident (Feb 7–18): Overflow ~58,938/day, Validation ~31,333/day (5.6x increase)
- Post-fix (Feb 19–24): Overflow ~58,629/day, Validation ~21,552/day
