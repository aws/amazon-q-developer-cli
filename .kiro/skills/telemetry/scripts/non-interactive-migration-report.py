#!/usr/bin/env python3
"""Non-Interactive Engine Migration Report

Queries KUTS production metrics for non-interactive sessions across all engines.
Reports: sessions started, user turns, success rate, and failure breakdown.

Usage:
    python3 non-interactive-migration-report.py             # last 7 days
    python3 non-interactive-migration-report.py --days 1    # last 1 day
    python3 non-interactive-migration-report.py --days 14   # last 2 weeks
"""

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timedelta, timezone

ACCOUNT = "615299732016"
LOG_GROUP = "/kuts/kiro-cli/metrics"
REGION = "us-east-1"
TERMINAL_STATUSES = {"Complete", "Failed", "Cancelled", "Timeout", "Unknown"}


def choose_profile():
    if os.environ.get("KUTS_AWS_PROFILE"):
        return os.environ["KUTS_AWS_PROFILE"]
    try:
        result = subprocess.run(
            ["aws", "configure", "list-profiles"],
            check=True, capture_output=True, text=True,
        )
        profiles = set(result.stdout.splitlines())
    except (FileNotFoundError, subprocess.CalledProcessError):
        profiles = set()
    for candidate in ("kuts_telemetry_prod_read-only", "kuts"):
        if candidate in profiles:
            return candidate
    raise RuntimeError("no KUTS AWS profile found; set KUTS_AWS_PROFILE")


def aws_cli(profile, *args):
    cmd = ["aws", "--profile", profile, "--region", REGION, "--no-cli-pager", "--output", "json", *args]
    result = subprocess.run(cmd, check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def run_query(profile, start_epoch, end_epoch, query_string, timeout=300):
    started = aws_cli(
        profile, "logs", "start-query",
        "--log-group-name", LOG_GROUP,
        "--start-time", str(start_epoch),
        "--end-time", str(end_epoch),
        "--query-string", query_string,
    )
    query_id = started["queryId"]
    deadline = time.monotonic() + timeout

    while time.monotonic() < deadline:
        time.sleep(5)
        response = aws_cli(profile, "logs", "get-query-results", "--query-id", query_id)
        status = response.get("status", "Unknown")
        if status in TERMINAL_STATUSES:
            if status != "Complete":
                raise RuntimeError(f"Query failed: {status}")
            return response["results"]

    raise RuntimeError(f"Query timed out after {timeout}s")


def parse_results(results):
    """Convert Logs Insights results into list of dicts."""
    rows = []
    for row in results:
        fields = {item["field"]: item.get("value", "") for item in row}
        rows.append(fields)
    return rows


def print_table(title, headers, rows):
    """Print a formatted table."""
    print(f"\n{'─' * 80}")
    print(f"  {title}")
    print(f"{'─' * 80}")

    col_widths = [len(h) for h in headers]
    for row in rows:
        for i, h in enumerate(headers):
            col_widths[i] = max(col_widths[i], len(str(row.get(h, ""))))

    header_line = "  ".join(h.rjust(col_widths[i]) for i, h in enumerate(headers))
    print(f"  {header_line}")
    print(f"  {'  '.join('─' * w for w in col_widths)}")

    for row in rows:
        line = "  ".join(str(row.get(h, "")).rjust(col_widths[i]) for i, h in enumerate(headers))
        print(f"  {line}")


def main():
    parser = argparse.ArgumentParser(description="Non-Interactive Engine Migration Report")
    parser.add_argument("--days", type=int, default=7, help="Number of days to query (default: 7)")
    args = parser.parse_args()

    engine_filter = '["v1", "v2", "v3"]'

    end_epoch = int(datetime.now(timezone.utc).timestamp())
    start_epoch = end_epoch - args.days * 86400

    start_str = datetime.fromtimestamp(start_epoch, tz=timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    end_str = datetime.fromtimestamp(end_epoch, tz=timezone.utc).strftime("%Y-%m-%d %H:%M UTC")

    print("=" * 80)
    print("  NON-INTERACTIVE ENGINE MIGRATION REPORT")
    print(f"  Period: last {args.days} day(s) ({start_str} → {end_str})")
    print("=" * 80)

    try:
        profile = choose_profile()
    except RuntimeError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    try:
        identity = aws_cli(profile, "sts", "get-caller-identity")
        if identity.get("Account") != ACCOUNT:
            print(f"Error: wrong account {identity.get('Account')}", file=sys.stderr)
            sys.exit(1)
    except (subprocess.CalledProcessError, RuntimeError):
        print(f"Error: credentials expired. Run: ada credentials update --profile {profile} --once", file=sys.stderr)
        sys.exit(1)

    print("\nQuerying KUTS... (this may take 30-60s per query)", file=sys.stderr)

    # 1. Sessions Started
    try:
        results = run_query(profile, start_epoch, end_epoch, f'''
fields agent_engine, user_id
| filter ispresent(kiro_cli_run_started_total)
| filter session_interface = "noninteractive_cli"
| filter agent_engine in {engine_filter}
| stats count(*) as sessions, count_distinct(user_id) as unique_users by agent_engine
| sort agent_engine asc''')
        rows = parse_results(results)
        print_table("Sessions Started", ["agent_engine", "sessions", "unique_users"], rows)
    except RuntimeError as e:
        print(f"  [sessions query failed: {e}]", file=sys.stderr)

    # 2. User Turns
    try:
        results = run_query(profile, start_epoch, end_epoch, f'''
fields agent_engine, user_id
| filter ispresent(kiro_cli_user_turns)
| filter session_interface = "noninteractive_cli"
| filter agent_engine in {engine_filter}
| stats count(*) as turns, count_distinct(user_id) as unique_users by agent_engine
| sort agent_engine asc''')
        rows = parse_results(results)
        print_table(
            "User Turns",
            ["agent_engine", "turns", "unique_users"],
            rows,
        )
    except RuntimeError as e:
        print(f"  [turns query failed: {e}]", file=sys.stderr)

    # 3. Success Rate
    try:
        results = run_query(profile, start_epoch, end_epoch, f'''
fields agent_engine, run_outcome
| filter ispresent(kiro_cli_run_outcome_total)
| filter session_interface = "noninteractive_cli"
| filter agent_engine in {engine_filter}
| stats count(*) as total,
        sum(run_outcome = "success") as successes,
        sum(run_outcome = "failure") as failures,
        sum(run_outcome = "crash") as crashes
  by agent_engine
| sort agent_engine asc''')
        rows = parse_results(results)
        for row in rows:
            total = int(row.get("total", 0))
            successes = int(row.get("successes", 0))
            row["success_rate"] = f"{successes * 100.0 / total:.1f}%" if total > 0 else "N/A"
        print_table("Success Rate", ["agent_engine", "total", "successes", "failures", "crashes", "success_rate"], rows)
    except RuntimeError as e:
        print(f"  [success rate query failed: {e}]", file=sys.stderr)

    # 4. Failures by version (top offenders)
    try:
        results = run_query(profile, start_epoch, end_epoch, f'''
fields agent_engine, version_full
| filter ispresent(kiro_cli_run_outcome_total)
| filter session_interface = "noninteractive_cli"
| filter agent_engine in {engine_filter}
| filter run_outcome = "failure"
| stats count(*) as failures, count_distinct(user_id) as users by agent_engine, version_full
| sort failures desc
| limit 15''')
        rows = parse_results(results)
        print_table("Top Failure Sources (by version)", ["agent_engine", "version_full", "failures", "users"], rows)
    except RuntimeError as e:
        print(f"  [failures query failed: {e}]", file=sys.stderr)

    print(f"\n{'─' * 80}")
    print("  NOTES")
    print(f"{'─' * 80}")
    print("  - V2 user_turns undercounts: the ACP subprocess emits turns under")
    print("    session_interface=external_acp. This is fixed once the graceful")
    print("    shutdown lands (subprocess gets time to flush telemetry).")
    print("  - V1 does not emit turn duration metrics for non-interactive.")
    print("  - Success rate (run_outcome) is the primary quality signal.")
    print("  - Failures by version helps identify outlier users/builds skewing rates.")
    print("")


if __name__ == "__main__":
    main()
