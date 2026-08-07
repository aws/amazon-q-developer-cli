#!/usr/bin/env python3
"""Daily Kiro Crew ACP client traffic report: turns and users by client name and app type."""
import json
import subprocess
import sys
import time
from datetime import datetime, timedelta

SCRIPT = "scripts/es-query.sh"
METRIC = "codewhispererterminal_recordUserTurnCompletion"
PRODUCT = "CodeWhisperer for Terminal"

# Usage: python3 es-kiro-crew-report.py [DAYS|START_DATE END_DATE] [output.md]
# Examples:
#   python3 es-kiro-crew-report.py 7
#   python3 es-kiro-crew-report.py 2026-08-01 2026-08-07

args = sys.argv[1:]
if len(args) >= 2 and "-" in args[0]:
    START = args[0]
    END = args[1]
    OUT = args[2] if len(args) > 2 else "kiro-crew-adoption-report.md"
    start_dt = datetime.strptime(START, "%Y-%m-%d")
    end_dt = datetime.strptime(END, "%Y-%m-%d")
    dates = [(start_dt + timedelta(days=i)).strftime("%Y-%m-%d")
             for i in range((end_dt - start_dt).days + 1)]
else:
    DAYS = int(args[0]) if args else 7
    OUT = args[1] if len(args) > 1 else "kiro-crew-adoption-report.md"
    today = datetime.now()
    dates = [(today - timedelta(days=i)).strftime("%Y-%m-%d") for i in range(DAYS, -1, -1)]

KIRO_CREW_CLIENTS = ["meshclaw", "kirocrew"]


def es_query(index, body):
    for attempt in range(3):
        if attempt > 0:
            time.sleep(5)
        r = subprocess.run(
            [SCRIPT, index, json.dumps(body)],
            capture_output=True, text=True, timeout=120,
        )
        if r.returncode != 0 or not r.stdout.strip():
            print(f"  Attempt {attempt+1} failed for {index} (rc={r.returncode})", file=sys.stderr)
            continue
        try:
            return json.loads(r.stdout)
        except json.JSONDecodeError as e:
            print(f"  Attempt {attempt+1} parse error: {e}", file=sys.stderr)
            continue
    print(f"  FAILED all retries for {index}", file=sys.stderr)
    return None


def query_app_type_and_clients(index):
    """Get traffic breakdown by app type and ACP client name."""
    body = {
        "size": 0,
        "track_total_hits": True,
        "query": {
            "bool": {
                "must": [
                    {"match_phrase": {"product": PRODUCT}},
                    {"match_phrase": {"metadata.metricName": METRIC}},
                ]
            }
        },
        "aggs": {
            "total_users": {"cardinality": {"field": "clientId"}},
            "by_app_type": {
                "terms": {"field": "metadata.kirocli_appType", "size": 10, "missing": "(missing)"},
                "aggs": {
                    "users": {"cardinality": {"field": "clientId"}},
                    "by_client": {
                        "terms": {"field": "metadata.kirocli_acpClientName", "size": 30, "missing": "(none)"},
                        "aggs": {"users": {"cardinality": {"field": "clientId"}}},
                    },
                },
            },
        },
    }
    return es_query(index, body)


results = {}
for date in dates:
    index = f"metrics-{date}"
    print(f"Querying {date}...", file=sys.stderr)
    data = query_app_type_and_clients(index)
    if data:
        results[date] = data
    time.sleep(1.5)


def extract_day_data(data):
    """Parse ES response into structured day data."""
    if "hits" not in data or "aggregations" not in data:
        return {"total_turns": 0, "total_users": 0, "app_types": {}}
    total_turns = data["hits"]["total"]["value"]
    total_users = data["aggregations"]["total_users"]["value"]

    app_types = {}
    for bucket in data["aggregations"]["by_app_type"]["buckets"]:
        app_type = bucket["key"]
        clients = {}
        for cb in bucket["by_client"]["buckets"]:
            clients[cb["key"]] = {"turns": cb["doc_count"], "users": cb["users"]["value"]}
        app_types[app_type] = {
            "turns": bucket["doc_count"],
            "users": bucket["users"]["value"],
            "clients": clients,
        }

    return {"total_turns": total_turns, "total_users": total_users, "app_types": app_types}


lines = []
w = lines.append

w("# Kiro Crew ACP Client Adoption Report")
w("")
w(f"Generated: {datetime.now().strftime('%Y-%m-%d %H:%M')}")
w("")
w("This report tracks ACP client traffic using `recordUserTurnCompletion` telemetry events.")
w("One turn = one user prompt that received a complete agent response (may involve multiple LLM requests).")
w("")
w("## App Type Definitions")
w("")
w("| App Type | Description |")
w("|----------|-------------|")
w("| **V1** | Legacy V1 engine (no ACP client name) |")
w("| **V2** | Built-in kiro-tui (V2 engine, Twinki React frontend) |")
w("| **ACP** | External ACP clients connecting to the CLI |")
w("| **(missing)** | Pre-migration records lacking the `kirocli_appType` field |")
w("")

sorted_dates = sorted(dates)

# Table 1: Kiro Crew - By Turns
w("## Kiro Crew - By Turns")
w("")
w("| Date | ACP Total | meshclaw | kirocrew | Others |")
w("|------|----------:|---------:|---------:|-------:|")

for date in sorted_dates:
    if date not in results:
        w(f"| {date} | — | — | — | — |")
        continue
    day = extract_day_data(results[date])
    acp_data = day["app_types"].get("ACP", {"turns": 0, "clients": {}})
    acp_total = acp_data["turns"]
    clients = acp_data.get("clients", {})

    mesh = clients.get("meshclaw", {}).get("turns", 0)
    crew = clients.get("kirocrew", {}).get("turns", 0)
    others = acp_total - mesh - crew

    mesh_pct = mesh / acp_total * 100 if acp_total else 0
    crew_pct = crew / acp_total * 100 if acp_total else 0
    others_pct = others / acp_total * 100 if acp_total else 0

    w(f"| {date} | {acp_total:,} | {mesh:,} ({mesh_pct:.1f}%) | {crew:,} ({crew_pct:.1f}%) | {others:,} ({others_pct:.1f}%) |")

w("")

# Table 2: Kiro Crew - By Users
w("## Kiro Crew - By Users")
w("")
w("| Date | ACP Total | meshclaw | kirocrew | Others |")
w("|------|----------:|---------:|---------:|-------:|")

for date in sorted_dates:
    if date not in results:
        w(f"| {date} | — | — | — | — |")
        continue
    day = extract_day_data(results[date])
    acp_data = day["app_types"].get("ACP", {"users": 0, "clients": {}})
    acp_users = acp_data["users"]
    clients = acp_data.get("clients", {})

    mesh_u = clients.get("meshclaw", {}).get("users", 0)
    crew_u = clients.get("kirocrew", {}).get("users", 0)
    others_u = acp_users - mesh_u - crew_u

    mesh_pct = mesh_u / acp_users * 100 if acp_users else 0
    crew_pct = crew_u / acp_users * 100 if acp_users else 0
    others_pct = others_u / acp_users * 100 if acp_users else 0

    w(f"| {date} | {acp_users:,} | {mesh_u:,} ({mesh_pct:.1f}%) | {crew_u:,} ({crew_pct:.1f}%) | {others_u:,} ({others_pct:.1f}%) |")

w("")

# Table 3: CLI vs ACP - By Turns
w("## CLI vs ACP - By Turns")
w("")
w("| Date | Total | V1 | V2 (kiro-tui) | ACP (external) | (missing) |")
w("|------|------:|---:|--------------:|---------------:|----------:|")

for date in sorted_dates:
    if date not in results:
        w(f"| {date} | — | — | — | — | — |")
        continue
    day = extract_day_data(results[date])
    t = day["total_turns"]
    if not t:
        w(f"| {date} | 0 | — | — | — | — |")
        continue
    v1 = day["app_types"].get("V1", {}).get("turns", 0)
    v2 = day["app_types"].get("V2", {}).get("turns", 0)
    acp = day["app_types"].get("ACP", {}).get("turns", 0)
    missing = day["app_types"].get("(missing)", {}).get("turns", 0)
    w(f"| {date} | {t:,} | {v1:,} ({v1/t*100:.1f}%) | {v2:,} ({v2/t*100:.1f}%) | {acp:,} ({acp/t*100:.1f}%) | {missing:,} ({missing/t*100:.1f}%) |")

w("")

# Table 4: CLI vs ACP - By Users
w("## CLI vs ACP - By Users")
w("")
w("| Date | Total | V1 | V2 (kiro-tui) | ACP (external) | (missing) |")
w("|------|------:|---:|--------------:|---------------:|----------:|")

for date in sorted_dates:
    if date not in results:
        w(f"| {date} | — | — | — | — | — |")
        continue
    day = extract_day_data(results[date])
    tu = day["total_users"]
    if not tu:
        w(f"| {date} | 0 | — | — | — | — |")
        continue
    v1u = day["app_types"].get("V1", {}).get("users", 0)
    v2u = day["app_types"].get("V2", {}).get("users", 0)
    acpu = day["app_types"].get("ACP", {}).get("users", 0)
    missingu = day["app_types"].get("(missing)", {}).get("users", 0)

    v1_pct = v1u / tu * 100 if tu else 0
    v2_pct = v2u / tu * 100 if tu else 0
    acp_pct = acpu / tu * 100 if tu else 0
    missing_pct = missingu / tu * 100 if tu else 0

    w(f"| {date} | {tu:,} | {v1u:,} ({v1_pct:.1f}%) | {v2u:,} ({v2_pct:.1f}%) | {acpu:,} ({acp_pct:.1f}%) | {missingu:,} ({missing_pct:.1f}%) |")

w("")
w("## Methodology")
w("")
w("- Metric: `codewhispererterminal_recordUserTurnCompletion` — emitted once per completed user turn")
w("- One turn = one user prompt receiving a full agent response (may involve multiple LLM API calls)")
w("- Users = `cardinality(clientId)` — approximate unique installations (HyperLogLog, ~2% error)")
w("- App type from `metadata.kirocli_appType`: V1, V2, ACP, or missing (pre-migration)")
w("- ACP client name from `metadata.kirocli_acpClientName` set in ACP `InitializeRequest`")
w("- `kiro-tui` (built-in TUI) has app type `V2`, NOT `ACP` — it is not counted in ACP totals")
w("- Unnamed ACP = external clients that don't set their name in `InitializeRequest`")
w("")

with open(OUT, "w") as f:
    f.write("\n".join(lines) + "\n")

print(f"Report written to {OUT}", file=sys.stderr)
