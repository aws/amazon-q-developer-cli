#!/usr/bin/env python3
"""Daily V1 vs V2 report: internal vs external, unique users + total user turns."""
import json
import subprocess
import sys
from datetime import datetime, timedelta

SCRIPT = "scripts/es-query.sh"
METRIC = "codewhispererterminal_recordUserTurnCompletion"

# Usage: python3 es-v2-report.py [DAYS|START_DATE END_DATE] [output.md] [--min-version X.Y.Z]
# Examples:
#   python3 es-v2-report.py 10                                        # last 10 days
#   python3 es-v2-report.py 2026-03-27 2026-03-29                     # specific range
#   python3 es-v2-report.py 2026-04-02 2026-04-09 --min-version 1.29.3

# Parse --min-version flag
MIN_VERSION = None
filtered_args = []
i = 1
while i < len(sys.argv):
    if sys.argv[i] == "--min-version" and i + 1 < len(sys.argv):
        MIN_VERSION = sys.argv[i + 1]
        i += 2
    else:
        filtered_args.append(sys.argv[i])
        i += 1

if len(filtered_args) >= 2 and "-" in filtered_args[0]:
    START = filtered_args[0]
    END = filtered_args[1]
    OUT = filtered_args[2] if len(filtered_args) > 2 else "v2-adoption-report.md"
    start_dt = datetime.strptime(START, "%Y-%m-%d")
    end_dt = datetime.strptime(END, "%Y-%m-%d")
    num_days = (end_dt - start_dt).days
    dates = [(start_dt + timedelta(days=i)).strftime("%Y-%m-%d") for i in range(num_days + 1)]
else:
    DAYS = int(filtered_args[0]) if filtered_args else 10
    OUT = filtered_args[1] if len(filtered_args) > 1 else "v2-adoption-report.md"
    today = datetime.now()
    dates = [(today - timedelta(days=i)).strftime("%Y-%m-%d") for i in range(DAYS, -1, -1)]

def version_tuple(v):
    return tuple(int(x) for x in v.split("."))

def get_version_filter(min_ver):
    """Generate a terms filter for versions >= min_ver using known versions."""
    if not min_ver:
        return None
    min_t = version_tuple(min_ver)
    # Known released versions - update as new versions ship
    all_versions = [
        f"1.29.{p}" for p in range(0, 20)
    ] + [
        f"1.30.{p}" for p in range(0, 20)
    ] + [
        f"1.31.{p}" for p in range(0, 10)
    ]
    valid = [v for v in all_versions if version_tuple(v) >= min_t]
    print(f"Version filter: >= {min_ver} -> {valid[:10]}...", file=sys.stderr)
    return {"terms": {"productVersion": valid}}

VERSION_FILTER = get_version_filter(MIN_VERSION)

# Internal = any URL containing amzn.awsapps.com (with whitespace trimming handled via wildcard)
# V2 = has metadata.kirocli_appType, V1 = doesn't have it

SEGMENTS = {
    "int_v2": {
        "must": [
            {"match_phrase": {"metadata.metricName": METRIC}},
            {"match_phrase": {"product": "CodeWhisperer for Terminal"}},
            {"match": {"metadata.kirocli_appType": "V2"}},
            {"wildcard": {"metadata.credentialStartUrl": "*amzn.awsapps.com*"}},
        ],
    },
    "int_v1": {
        "must": [
            {"match_phrase": {"metadata.metricName": METRIC}},
            {"match_phrase": {"product": "CodeWhisperer for Terminal"}},
            {"wildcard": {"metadata.credentialStartUrl": "*amzn.awsapps.com*"}},
        ],
        "must_not": [{"exists": {"field": "metadata.kirocli_appType"}}],
    },
    "ext_v2": {
        "must": [
            {"match_phrase": {"metadata.metricName": METRIC}},
            {"match_phrase": {"product": "CodeWhisperer for Terminal"}},
            {"match": {"metadata.kirocli_appType": "V2"}},
        ],
        "must_not": [{"wildcard": {"metadata.credentialStartUrl": "*amzn.awsapps.com*"}}],
    },
    "ext_v1": {
        "must": [
            {"match_phrase": {"metadata.metricName": METRIC}},
            {"match_phrase": {"product": "CodeWhisperer for Terminal"}},
        ],
        "must_not": [
            {"exists": {"field": "metadata.kirocli_appType"}},
            {"wildcard": {"metadata.credentialStartUrl": "*amzn.awsapps.com*"}},
        ],
    },
}

import time

def query(index, seg):
    must = list(seg["must"])
    if VERSION_FILTER:
        must.append(VERSION_FILTER)
    body = {
        "size": 0,
        "track_total_hits": True,
        "query": {"bool": {"must": must, "must_not": seg.get("must_not", [])}},
        "aggs": {"users": {"cardinality": {"field": "clientId"}}},
    }
    for attempt in range(3):
        if attempt > 0:
            time.sleep(5)
        r = subprocess.run([SCRIPT, index, json.dumps(body)], capture_output=True, text=True, timeout=120)
        if r.returncode != 0 or not r.stdout.strip():
            print(f"  Attempt {attempt+1} failed for {index} (rc={r.returncode})", file=sys.stderr)
            continue
        try:
            d = json.loads(r.stdout)
            return d["aggregations"]["users"]["value"], d["hits"]["total"]["value"]
        except (json.JSONDecodeError, KeyError) as e:
            print(f"  Attempt {attempt+1} parse error for {index}: {e} resp={r.stdout[:200]}", file=sys.stderr)
            continue
    print(f"  FAILED all retries for {index}", file=sys.stderr)
    return 0, 0

results = {}
for date in dates:
    index = f"metrics-{date}"
    print(f"Querying {date}...", file=sys.stderr)
    results[date] = {}
    for seg_name, seg_query in SEGMENTS.items():
        users, turns = query(index, seg_query)
        results[date][seg_name] = (users, turns)
        time.sleep(1.5)

lines = []
w = lines.append

w("# V2 (kiro-tui) Adoption Report")
w("")
w(f"Generated: {datetime.now().strftime('%Y-%m-%d %H:%M')}")
w("")
w("This report tracks V2 (kiro-tui) adoption using the `recordUserTurnCompletion` telemetry event, which is emitted once per user turn after all LLM requests and tool-use follow-ups complete — making it a reliable proxy for active usage sessions.")
w("")
w("- **V1 vs V2**: V2 events are identified by the presence of `kirocli_appType: \"V2\"` (added in 1.28.x via the ACP telemetry commit); events without this field are counted as V1")
w("- **Internal vs External**: Internal users have a `credentialStartUrl` matching `amzn.awsapps.com`; all others (external SSO, Builder ID, unauthenticated) are counted as external")
if MIN_VERSION:
    w(f"- **Version filter**: Only versions ≥ `{MIN_VERSION}` (excluding nightlies)")
w("")

sorted_dates = sorted(dates, reverse=True)

# Table 1: High-level adoption %
w("## V2 User Adoption (%)")
w("")
w("| Date | Internal %V2 | External %V2 |")
w("|------|-------------|-------------|")

for date in sorted_dates:
    r = results[date]
    iv1u, _ = r["int_v1"]; iv2u, _ = r["int_v2"]
    ev1u, _ = r["ext_v1"]; ev2u, _ = r["ext_v2"]
    ipct = iv2u / (iv1u + iv2u) * 100 if (iv1u + iv2u) else 0
    epct = ev2u / (ev1u + ev2u) * 100 if (ev1u + ev2u) else 0
    w(f"| {date} | {ipct:.1f}% | {epct:.1f}% |")

w("")

# Table 2: Detailed numbers
w("## Detailed Numbers")
w("")
w("| Date | Int V1 Users | Int V1 Turns | Int V2 Users | Int V2 Turns | Ext V1 Users | Ext V1 Turns | Ext V2 Users | Ext V2 Turns |")
w("|------|-------------|-------------|-------------|-------------|-------------|-------------|-------------|-------------|")

for date in sorted_dates:
    r = results[date]
    iv1u, iv1t = r["int_v1"]; iv2u, iv2t = r["int_v2"]
    ev1u, ev1t = r["ext_v1"]; ev2u, ev2t = r["ext_v2"]
    w(f"| {date} | {iv1u:,} | {iv1t:,} | {iv2u:,} | {iv2t:,} | {ev1u:,} | {ev1t:,} | {ev2u:,} | {ev2t:,} |")

with open(OUT, "w") as f:
    f.write("\n".join(lines) + "\n")

print(f"Report written to {OUT}", file=sys.stderr)
