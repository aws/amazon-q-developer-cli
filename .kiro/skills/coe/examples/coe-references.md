# COE Draft - Data Gathering References

All queries use the telemetry account `421629052180` with `ReadOnly` role via `ada`.

---

## Source References for COE Assertions

Every major claim in the COE draft is traced back to its source below.

### Incident Start Date: "Feb 5, 19:59 UTC" (confirmed via MPS logs)

**Evidence:** MPS (`AWSVectorConsolasModelProxyService-prod-ApplicationLogs`, account `344555145015`) query:
```bash
env $(ada cred print --account 344555145015 --role ReadOnly --format env) \
  aws logs start-query \
  --log-group-name 'AWSVectorConsolasModelProxyService-prod-ApplicationLogs' \
  --start-time $(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-05T19:58:00" +%s) \
  --end-time $(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-05T20:01:00" +%s) \
  --query-string 'fields @timestamp, @message | filter @message like "prompt is too long" | sort @timestamp asc | limit 5' \
  --region us-east-1 --output json
```

**Result:** 6 matches. Earliest: `2026-02-05 19:59:10.820 UTC`
```
modelId: us.anthropic.claude-opus-4-6-v1
prompt is too long: 811381 tokens > 200000 maximum
```

**No earlier occurrences:** Searched Feb 1 00:00 UTC through Feb 5 19:59 UTC — zero matches across 200B+ records scanned.

**Previous estimates (RETRACTED):**
- "~Feb 7 07:00" — from P382946346 ticket comment, based on ContextWindowOverflow dashboard graph. Wrong metric (behavior change manifested as ValidationException, not ContextWindowOverflow).
- "~Feb 12" — from CloudWatch daily ValidationException aggregates. Feb 12 was the first day with a clear spike in daily counts, but MPS logs show the error started 7 days earlier at low volume.

**Strength: STRONG.** Direct log evidence from the service that calls Bedrock, showing the exact error message, model ID, and token count.

### Feb 5 user report (8371585b) — NOT confirmed as "prompt is too long"

The first user report was at Feb 5 ~18:29 UTC (10:29 AM PST), request ID `8371585b-2809-4421-98a3-0adde8147d2a`. Maestro logs confirm this request got `"Request input fails validation"` from the sidecar. However, MPS logs show zero "prompt is too long" errors in a 2-minute window around 18:29 UTC. This request predates the first MPS "prompt is too long" entry by ~90 minutes and may be related to a different issue ([COE-387009](https://www.coe.a2z.com/coe/387009/content)).

### Feb 9 request (65c3fee1) — CONFIRMED as "prompt is too long"

Request `65c3fee1-49db-46ca-a6fa-b99fe94dc175` at Feb 9 16:25 UTC. Maestro logs show `"Got prestream error from Sidecar"` with `"Request input fails validation"`. MPS logs in the same 10-second window show 111 "prompt is too long" errors, all `us.anthropic.claude-opus-4-6-v1`.

```bash
env $(ada cred print --account 344555145015 --role ReadOnly --format env) \
  aws logs start-query \
  --log-group-name 'AWSVectorConsolasModelProxyService-prod-ApplicationLogs' \
  --start-time $(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-09T16:25:10" +%s) \
  --end-time $(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-09T16:25:20" +%s) \
  --query-string 'fields @timestamp, @message | filter @message like "prompt is too long" | sort @timestamp asc | limit 10' \
  --region us-east-1 --output json
```

### Root Cause: Opus 4.6 behavior change

**Source:** P382946346 ticket title and description:
> "Opus 4.6 is returning a new error on context window overflows that is causing validation exceptions."

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-17T08:37:24Z:
> "Seeing infinite compaction loop with `kiro-cli` version 1.26.0, and model `claude-sonnet-4.5` [...] No change in Maestro to update the context window overflow exception, seems to be a downstream behavior change with Bedrock/Claude"

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-17T13:24:04Z:
> "From local testing directly through Maestro - it seems that the issue is now impacting *all* Claude Sonnet models. [...] **New behavior:** Claude returns `prompt is too long` now immediately."

### Maestro branch change reintroduced "input is too long" check

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-17T13:33:54Z:
> "Recent maestro deployment changed branches in `QDeveloperMaestroPythonSidecar` which added back the 'input is too long' check"
> Links: pipeline diff and commit diff provided in comment.

### V2103376290 used to justify Maestro deployment (NOT as detection signal)

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-17T13:35:14Z:
> "Deployment was justified with https://t.corp.amazon.com/V2103376290"

**Note:** This comment says the Maestro *deployment* was justified with V2103376290. It does NOT say V2103376290 was related to the context window overflow issue. V2103376290 is a generic `MaestroFailedCountCanaryTest` alarm — removed from the COE draft as unrelated.

### Infinite compaction loop root cause analysis

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-17T06:07:24Z (the "Analysis" comment):
> "The CLI assumes that a context window overflow *cannot* occur if `next_user_message` itself results in `ContextWindowOverflow`"
> "Therefore, if `next_user_message` itself can result in a context window overflow, then the CLI will perform an *infinite compaction loop*"
> "Opus returns a new error message during `ContextWindowOverflow` - `prompt is too long`. Not handling this in Maestro results in `ValidationException`'s on the client."

### Proposed fix architecture

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-17T11:05:22Z:
> "The problem is that client compaction logic is **fundamentally broken** - infinite compaction loops will occur whenever `next_user_message` needs to be updated"
> "A *breaking change* needs to be made such that: CLI context window overflow has a separate message than other origins; A new CLI version with a fixed compaction implementation correctly handles the new message"

### CR-253681011 (initial Maestro fix)

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-13T01:25:13Z:
> "CR merged - https://code.amazon.com/reviews/CR-253681011"

Ticket created at 2026-02-13T01:21:12Z, CR merged 4 minutes later — confirms CR was already in flight.

### CR-254678454 (final Maestro fix)

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-18T04:43:13Z:
> "Maestro change merged - https://code.amazon.com/reviews/CR-254678454"

### PR #853 (CLI fix)

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-18T06:06:51Z:
> "CLI PR merged, working on creating a release - https://github.com/kiro-team/kiro-cli/pull/853"

### CLI v1.26.2 release

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-18T21:21:16Z:
> "Autocomplete build succeeded. Created Kiro CLI v1.26.2 release ticket - https://t.corp.amazon.com/P385360723/"

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-19T11:34:56Z:
> "CLI v1.26.2 release has completed - https://github.com/kiro-team/kiro-cli-autocomplete/actions/runs/22127916936"

### Maestro FRA deployment

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-18T14:50:54Z:
> "Deployed to Prod FRA `18 Feb 2026 07:13:03 GMT`"

### Maestro IAD deployment

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-19T11:21:34Z:
> "Maestro IAD deployment completed now, verified with v1.26.2 build in the beta toolbox channel."

### P385355150 (cross-team deployment blocker)

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-19T08:15:15Z:
> "https://t.corp.amazon.com/P385355150/"

P385355150 ticket description confirms: "I don't have permission to override approvals, we need assistance from Maestro oncall to push these changes to Maestro Prod."

### Bedrock Opus 4.6 availability drop (unrelated)

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-18T18:14:16Z:
> "Spike seems to match with Opus 4.6 availability drop - https://amzn-aws.slack.com/archives/C08RACJKED9/p1771401116410549"
> "Bedrock ticket - https://t.corp.amazon.com/P385427665/communication"

**Note:** This was a concurrent Bedrock 503 issue, noted as contributing factor during remediation only.

### User report (dcolaizz)

**Source:** P382946346 correspondence, comment by dcolaizz at 2026-02-17T15:51:59Z:
> "Hey team, can we do a COE on this ContextWindowOverflow incident? [...] Kiro is now critical to my dev workflow and I have struggled to work around this for 3 days."

### Slack user reports (chronological)

| Date (PST) | Slack Link | Details |
|------------|-----------|---------|
| Feb 5, 10:29 AM | [p1770316194619669](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770316194619669) | ValidationException with Opus 4.6 1M. Request `8371585b` — NOT confirmed as "prompt is too long" in MPS. Potentially related to [COE-387009](https://www.coe.a2z.com/coe/387009/content). |
| Feb 9, 8:32 AM | [p1770654766815339](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770654766815339) | Specific to Opus 4.6 |
| Feb 10, 11:41 AM | [p1770752481595229](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770752481595229) | |
| Feb 11, 6:15 AM | [p1770819301090339](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770819301090339) | Opus 4.5 now affected |
| Feb 12, 7:52 AM | [p1770911556920339](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770911556920339) | |
| Feb 12, 1:38 PM | [p1770932313882719](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770932313882719) | Opus 4.5 and 4.6 failing to auto-compact |
| Feb 12, 5:28 PM | [p1770946114722479](https://amzn-aws.slack.com/archives/C064EVBE0LR/p1770946114722479) | Thread announcing the issue |

### Gamma validation

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-21T01:30:40Z:
> "Validated change in gamma with: kiro-cli 1.26.3-nightly.2"

### CR-255195219 (SemVer fix)

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-21T18:33:42Z:
> "CR for the fix is merged - https://code.amazon.com/reviews/CR-255195219"

### Prod validation and ticket closure

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-27T10:00:19Z:
> "Validated nightly build in Prod"

**Source:** P382946346 correspondence, comment by bskiser at 2026-02-27T10:00:19Z:
> "Fix deployed to IAD" (resolution comment, ticket closed)

### Weekly Dashboard reference

**Source:** P382946346 correspondence, multiple comments reference:
> https://w.amazon.com/bin/view/AWS/KiroCLI/Operations/Dashboards/Weekly

### Detection: routine weekly dashboard review on Feb 13

**Source:** P382946346 ticket created at 2026-02-13T01:21:12Z by bskiser. The ticket description says "A fix is currently in progress" — indicating the issue was already being investigated when the ticket was filed. The "Feb 7 07:00" spike observation was posted later (Feb 17) based on dashboard review.

**Strength: MODERATE.** We know the ticket was created Feb 13 and the description says a fix was already in progress. The claim that this was a "routine weekly dashboard review" is inferred — the ticket doesn't explicitly say what triggered the discovery.

---

## CloudWatch Queries

### ContextWindowOverflow daily counts (incident window)

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

### ValidationException daily counts (incident window)

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

### Total message count (for error rate calculation)

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

### CompactHistoryFailure counts (checked — returned empty)

```bash
env $(ada cred print --account 421629052180 --role ReadOnly --format env) \
  aws cloudwatch get-metric-statistics \
  --namespace "Toolkit" \
  --metric-name "QCLIMessageResponseError" \
  --dimensions Name=product,Value=CodewhispererForTerminal Name=failureReason,Value=CompactHistoryFailure \
  --start-time "2026-02-01T00:00:00Z" \
  --end-time "2026-02-25T00:00:00Z" \
  --period 86400 \
  --statistics Sum \
  --region us-east-1
```

### Hourly ContextWindowOverflow for Feb 7 (spike start analysis)

```bash
env $(ada cred print --account 421629052180 --role ReadOnly --format env) \
  aws cloudwatch get-metric-statistics \
  --namespace "Toolkit" \
  --metric-name "QCLIMessageResponseError" \
  --dimensions Name=product,Value=CodewhispererForTerminal Name=failureReason,Value=ContextWindowOverflow \
  --start-time "2026-02-07T00:00:00Z" \
  --end-time "2026-02-08T00:00:00Z" \
  --period 3600 \
  --statistics Sum \
  --region us-east-1
```

---

## Summary Computation Script

Used to compute the daily breakdown table and period averages in the COE appendix.

```python
import json

overflow_raw = {
    '2026-02-01': 21953, '2026-02-02': 57397, '2026-02-03': 93658, '2026-02-04': 113735,
    '2026-02-05': 117888, '2026-02-06': 94932, '2026-02-07': 24608, '2026-02-08': 18555,
    '2026-02-09': 58185, '2026-02-10': 74841, '2026-02-11': 102972, '2026-02-12': 92224,
    '2026-02-13': 5694, '2026-02-14': 16333, '2026-02-15': 715, '2026-02-16': 2017,
    '2026-02-17': 297187, '2026-02-18': 13924, '2026-02-19': 25580, '2026-02-20': 37596,
    '2026-02-21': 95491, '2026-02-22': 25341, '2026-02-23': 31912, '2026-02-24': 135856,
}

validation_raw = {
    '2026-02-01': 848, '2026-02-02': 3431, '2026-02-03': 3195, '2026-02-04': 7789,
    '2026-02-05': 8952, '2026-02-06': 9464, '2026-02-07': 8208, '2026-02-08': 3972,
    '2026-02-09': 10357, '2026-02-10': 9970, '2026-02-11': 8979, '2026-02-12': 36046,
    '2026-02-13': 81456, '2026-02-14': 26894, '2026-02-15': 21351, '2026-02-16': 53996,
    '2026-02-17': 47896, '2026-02-18': 66873, '2026-02-19': 37213, '2026-02-20': 25032,
    '2026-02-21': 7091, '2026-02-22': 9458, '2026-02-23': 25129, '2026-02-24': 25388,
}

total_raw = {
    '2026-02-01': 1939383, '2026-02-02': 4786798, '2026-02-03': 5505411, '2026-02-04': 5620453,
    '2026-02-05': 5749764, '2026-02-06': 5370135, '2026-02-07': 2159643, '2026-02-08': 2193195,
    '2026-02-09': 5489587, '2026-02-10': 6271423, '2026-02-11': 6286435, '2026-02-12': 6447828,
    '2026-02-13': 5995360, '2026-02-14': 2585311, '2026-02-15': 2301022, '2026-02-16': 5316728,
    '2026-02-17': 6355400, '2026-02-18': 6555254, '2026-02-19': 6872838, '2026-02-20': 6942461,
    '2026-02-21': 2933382, '2026-02-22': 2986717, '2026-02-23': 6608391, '2026-02-24': 7756032,
}

baseline_overflow = sum(overflow_raw[f'2026-02-0{i}'] for i in range(1, 7))
baseline_validation = sum(validation_raw[f'2026-02-0{i}'] for i in range(1, 7))
baseline_total = sum(total_raw[f'2026-02-0{i}'] for i in range(1, 7))
baseline_days = 6

incident_dates = [f'2026-02-{d:02d}' for d in range(7, 19)]
incident_overflow = sum(overflow_raw[d] for d in incident_dates)
incident_validation = sum(validation_raw[d] for d in incident_dates)
incident_total = sum(total_raw[d] for d in incident_dates)
incident_days = 12

post_dates = [f'2026-02-{d:02d}' for d in range(19, 25)]
post_overflow = sum(overflow_raw[d] for d in post_dates)
post_validation = sum(validation_raw[d] for d in post_dates)
post_total = sum(total_raw[d] for d in post_dates)
post_days = 6

print('=== DAILY BREAKDOWN ===')
print(f'{"Date":<12} {"Overflow":>10} {"Validation":>12} {"Total Msgs":>12} {"Overflow%":>10} {"Valid%":>10}')
print('-' * 68)
for d in range(1, 25):
    date = f'2026-02-{d:02d}'
    o = overflow_raw[date]
    v = validation_raw[date]
    t = total_raw[date]
    print(f'{date:<12} {o:>10,} {v:>12,} {t:>12,} {o/t*100:>9.2f}% {v/t*100:>9.2f}%')

print()
print('=== PERIOD SUMMARY ===')
print(f'Pre-incident (Feb 1-6):  Overflow avg/day: {baseline_overflow/baseline_days:,.0f}  Validation avg/day: {baseline_validation/baseline_days:,.0f}  Total avg/day: {baseline_total/baseline_days:,.0f}')
print(f'Incident (Feb 7-18):     Overflow avg/day: {incident_overflow/incident_days:,.0f}  Validation avg/day: {incident_validation/incident_days:,.0f}  Total avg/day: {incident_total/incident_days:,.0f}')
print(f'Post-fix (Feb 19-24):    Overflow avg/day: {post_overflow/post_days:,.0f}  Validation avg/day: {post_validation/post_days:,.0f}  Total avg/day: {post_total/post_days:,.0f}')
print()
print(f'Peak day: Feb 17 - {297187:,} ContextWindowOverflow ({297187/6355400*100:.1f}% of requests)')
print(f'Peak day: Feb 13 - {81456:,} ValidationException ({81456/5995360*100:.1f}% of requests)')
print(f'Total incident overflow events: {incident_overflow:,}')
print(f'Total incident validation events: {incident_validation:,}')
```

---

## Tickets Fetched

| Ticket | Purpose |
|--------|---------|
| [P382946346](https://t.corp.amazon.com/P382946346) | Primary incident ticket — full correspondence (35 comments) and worklogs |
| [V2103376290](https://t.corp.amazon.com/V2103376290) | MaestroFailedCountCanaryTest alarm ticket — confirmed alarm routed to wrong resolver group |
| [P385355150](https://t.corp.amazon.com/P385355150) | Maestro deployment request — confirmed cross-team coordination and resolution |
| [P385427665](https://t.corp.amazon.com/P385427665) | Bedrock Anthropic 503 alarm — confirmed unrelated concurrent event |

---

## Runbook Docs Reviewed

| File | Purpose |
|------|---------|
| `docs/oncall/metrics_and_telemetry.md` | Identified Kibana dashboards, telemetry events, error codes, error classification table |
| `docs/oncall/cloudwatch_alarms_and_dashboard.md` | Identified CloudWatch account, namespace, dimensions, alarm definitions, metric pipeline |

### Key findings from runbook review

- CloudWatch account: `421629052180`, namespace: `Toolkit`, dimension: `product: CodewhispererForTerminal`
- `ContextWindowOverflow` classified as user error — excluded from `QCLI-SuccessRateDown` alarm
- `ValidationException` classified as system error — included in success rate but no dedicated alarm
- CloudWatch cannot break down by version or model — Kibana required for that
- `QCLIMessageResponseError` has `{product, failureReason}` dimensions — used for all error queries above
- Weekly Dashboard (wiki-based) referenced in ticket but not documented in runbooks
