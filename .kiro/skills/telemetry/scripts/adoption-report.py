#!/usr/bin/env python3
"""Generate Kiro CLI adoption report: users by engine and interface from KUTS metrics."""

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import date, datetime, time as datetime_time, timedelta, timezone
from pathlib import Path

ACCOUNT = "615299732016"
DEFAULT_LOG_GROUP = "/kiro/metrics"
DEFAULT_REGION = "us-east-1"
TERMINAL_QUERY_STATUSES = {"Complete", "Failed", "Cancelled", "Timeout", "Unknown"}

INTERNAL_DIRECTORY = "d-9067925563"

QUERY = """\
fields @timestamp, user_id, agent_engine, session_interface
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
| stats count_distinct(user_id) as users
  by bin(24h) as day, agent_engine, session_interface
| sort day asc
"""

QUERY_TOTAL_USERS = """\
fields @timestamp, user_id
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
| stats count_distinct(user_id) as users
  by bin(24h) as day
| sort day asc
"""

QUERY_INTERNAL_BY_ENGINE = """\
fields @timestamp, user_id, agent_engine
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
  and user_id like /d-9067925563/
| stats count_distinct(user_id) as users
  by bin(24h) as day, agent_engine
| sort day asc
"""

QUERY_TOTAL_BY_ENGINE = """\
fields @timestamp, user_id, agent_engine
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
| stats count_distinct(user_id) as users
  by bin(24h) as day, agent_engine
| sort day asc
"""


def parse_date(value):
    try:
        return date.fromisoformat(value)
    except ValueError as error:
        raise ValueError(f"invalid date {value!r}; expected YYYY-MM-DD") from error


def resolve_window(start, end=None, today=None):
    today = today or datetime.now(timezone.utc).date()
    if start.isdigit():
        if end is not None:
            raise ValueError("an end date cannot be used with a day count")
        days = int(start)
        if days < 1:
            raise ValueError("day count must be at least 1")
        end_date = today - timedelta(days=1)
        return end_date - timedelta(days=days - 1), end_date

    start_date = parse_date(start)
    end_date = parse_date(end) if end else start_date
    if end_date >= today:
        end_date = today - timedelta(days=1)
    if end_date < start_date:
        raise ValueError("end date must be on or after start date")
    return start_date, end_date


def build_parser():
    parser = argparse.ArgumentParser(
        description=(
            "Generate Kiro CLI adoption report with users by engine and interface "
            "from production KUTS metrics."
        ),
        epilog=(
            "Dates are UTC and inclusive. A numeric range selects the last N complete UTC days. "
            "The current (incomplete) day is always excluded."
        ),
    )
    parser.add_argument("start", help="start date (YYYY-MM-DD) or number of complete UTC days")
    parser.add_argument("end", nargs="?", help="inclusive end date (YYYY-MM-DD)")
    parser.add_argument("-o", "--output", help="Markdown output path (default: <run-date>-adoption-report.md)")
    parser.add_argument("--profile", help="AWS profile for the production KUTS account")
    parser.add_argument("--region", default=DEFAULT_REGION)
    parser.add_argument("--log-group", default=DEFAULT_LOG_GROUP)
    parser.add_argument(
        "--timeout-seconds",
        type=int,
        default=900,
        help="maximum time to wait for Logs Insights (default: 900)",
    )
    parser.add_argument(
        "--query-only",
        action="store_true",
        help="print the Logs Insights query without calling AWS",
    )
    parser.add_argument(
        "--input-json",
        type=Path,
        help="render saved aws logs get-query-results JSON instead of querying AWS",
    )
    return parser


def choose_profile(explicit, environ, available_profiles):
    if explicit:
        return explicit
    if environ.get("KUTS_AWS_PROFILE"):
        return environ["KUTS_AWS_PROFILE"]
    for candidate in ("kuts_telemetry_prod_read-only", "kuts"):
        if candidate in available_profiles:
            return candidate
    raise RuntimeError(
        "no KUTS AWS profile found; pass --profile or set KUTS_AWS_PROFILE"
    )


def list_profiles():
    try:
        result = subprocess.run(
            ["aws", "configure", "list-profiles"],
            check=True,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError as error:
        raise RuntimeError("AWS CLI is not installed") from error
    except subprocess.CalledProcessError as error:
        raise RuntimeError(f"could not list AWS profiles: {error.stderr.strip()}") from error
    return set(result.stdout.splitlines())


def run_aws(profile, region, *arguments):
    command = [
        "aws",
        "--profile",
        profile,
        "--region",
        region,
        "--no-cli-pager",
        *arguments,
        "--output",
        "json",
    ]
    try:
        result = subprocess.run(command, check=True, capture_output=True, text=True)
    except FileNotFoundError as error:
        raise RuntimeError("AWS CLI is not installed") from error
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() or error.stdout.strip()
        raise RuntimeError(f"AWS CLI failed: {detail}") from error
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("AWS CLI returned invalid JSON") from error


def verify_account(profile, region):
    identity = run_aws(profile, region, "sts", "get-caller-identity")
    actual_account = identity.get("Account")
    if actual_account != ACCOUNT:
        raise RuntimeError(
            f"AWS profile {profile!r} resolves to account {actual_account or 'unknown'}, "
            f"expected {ACCOUNT}"
        )


def stop_query(profile, region, query_id):
    try:
        run_aws(profile, region, "logs", "stop-query", "--query-id", query_id)
    except RuntimeError:
        pass


def run_logs_query(profile, region, log_group, start_date, end_date, query_string, label, timeout_seconds):
    start_time = datetime.combine(start_date, datetime_time.min, timezone.utc)
    end_time = datetime.combine(end_date + timedelta(days=1), datetime_time.min, timezone.utc)
    started = run_aws(
        profile,
        region,
        "logs",
        "start-query",
        "--log-group-name",
        log_group,
        "--start-time",
        str(int(start_time.timestamp())),
        "--end-time",
        str(int(end_time.timestamp())),
        "--query-string",
        query_string,
    )
    query_id = started.get("queryId")
    if not query_id:
        raise RuntimeError("CloudWatch Logs did not return a query ID")

    print(f"Started {label} query {query_id}", file=sys.stderr)
    deadline = time.monotonic() + timeout_seconds
    last_status = None
    try:
        while time.monotonic() < deadline:
            response = run_aws(
                profile,
                region,
                "logs",
                "get-query-results",
                "--query-id",
                query_id,
            )
            status = response.get("status", "Unknown")
            if status != last_status:
                print(f"  {label}: {status}", file=sys.stderr)
                last_status = status
            if status in TERMINAL_QUERY_STATUSES:
                if status != "Complete":
                    raise RuntimeError(f"{label} query ended with status {status}")
                return response
            time.sleep(3)
    except BaseException:
        stop_query(profile, region, query_id)
        raise

    stop_query(profile, region, query_id)
    raise RuntimeError(f"{label} query exceeded {timeout_seconds} seconds")


def query_metrics(profile, region, log_group, start_date, end_date, timeout_seconds):
    main_response = run_logs_query(
        profile, region, log_group, start_date, end_date, QUERY, "main", timeout_seconds
    )
    total_response = run_logs_query(
        profile, region, log_group, start_date, end_date, QUERY_TOTAL_USERS, "total", timeout_seconds
    )
    total_engine_response = run_logs_query(
        profile, region, log_group, start_date, end_date, QUERY_TOTAL_BY_ENGINE, "total-engine", timeout_seconds
    )
    internal_engine_response = run_logs_query(
        profile, region, log_group, start_date, end_date, QUERY_INTERNAL_BY_ENGINE, "internal-engine", timeout_seconds
    )
    return main_response, total_response, total_engine_response, internal_engine_response


def combine_statistics(*responses):
    """Sum bytesScanned and recordsScanned across all query responses."""
    total_bytes = 0.0
    total_records = 0.0
    for response in responses:
        stats = response.get("statistics") or {}
        total_bytes += stats.get("bytesScanned", 0) or 0
        total_records += stats.get("recordsScanned", 0) or 0
    return {"bytesScanned": total_bytes, "recordsScanned": total_records}


def row_to_dict(row):
    return {item["field"]: item.get("value", "") for item in row if "field" in item}


def parse_query_results(response, end_date):
    """Parse into: {day: {(engine, interface): {users}}}"""
    status = response.get("status")
    if status and status != "Complete":
        raise ValueError(f"query results are not complete (status: {status})")

    days = {}
    for raw_row in response.get("results", []):
        row = row_to_dict(raw_row)
        raw_day = row.get("day", "")
        try:
            day = date.fromisoformat(raw_day[:10])
        except ValueError as error:
            raise ValueError(f"invalid day value {raw_day!r}") from error

        if day > end_date:
            continue

        engine = row.get("agent_engine") or "unknown"
        interface = row.get("session_interface") or "unknown"
        users = int(float(row.get("users", "0")))

        if day not in days:
            days[day] = {}
        days[day][(engine, interface)] = {"users": users}

    return days


def parse_internal_results(response, end_date):
    """Parse internal query into: {day: {engine: users}}"""
    status = response.get("status")
    if status and status != "Complete":
        raise ValueError(f"internal query results are not complete (status: {status})")

    days = {}
    for raw_row in response.get("results", []):
        row = row_to_dict(raw_row)
        raw_day = row.get("day", "")
        try:
            day = date.fromisoformat(raw_day[:10])
        except ValueError as error:
            raise ValueError(f"invalid day value {raw_day!r}") from error

        if day > end_date:
            continue

        engine = row.get("agent_engine") or "unknown"
        users = int(float(row.get("users", "0")))

        if day not in days:
            days[day] = {}
        days[day][engine] = users

    return days


def parse_total_users(response, end_date):
    """Parse total distinct users query into: {day: users}"""
    status = response.get("status")
    if status and status != "Complete":
        raise ValueError(f"total users query results are not complete (status: {status})")

    days = {}
    for raw_row in response.get("results", []):
        row = row_to_dict(raw_row)
        raw_day = row.get("day", "")
        try:
            day = date.fromisoformat(raw_day[:10])
        except ValueError as error:
            raise ValueError(f"invalid day value {raw_day!r}") from error

        if day > end_date:
            continue

        users = int(float(row.get("users", "0")))
        days[day] = users

    return days


def format_bytes(value):
    amount = float(value)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB", "PiB"):
        if amount < 1024 or unit == "PiB":
            return f"{amount:.1f} {unit}"
        amount /= 1024
    raise AssertionError("unreachable")


def pct(value, total):
    if not total:
        return "—"
    return f"{value:,} ({value / total * 100:.1f}%)"


def render_report(
    start_date,
    end_date,
    days,
    total_users_by_day,
    total_engine_days,
    internal_engine_days,
    statistics=None,
    generated_at=None,
    region=DEFAULT_REGION,
    log_group=DEFAULT_LOG_GROUP,
):
    generated_at = generated_at or datetime.now(timezone.utc)
    sorted_days = sorted(days.keys())
    lines = [
        f"# Kiro CLI Adoption Report ({start_date.isoformat()} to {end_date.isoformat()})",
        "",
        f"Generated: {generated_at.strftime('%Y-%m-%d %H:%M UTC')}",
        f"Source: AWS account `{ACCOUNT}`, `{region}`, `{log_group}`",
    ]
    if statistics and statistics.get("bytesScanned") is not None:
        lines.append(
            f"Query scan: "
            f"{format_bytes(statistics['bytesScanned'])}, "
            f"{int(statistics.get('recordsScanned', 0)):,} records"
        )

    # Section 1: Users by Engine and Interface
    lines.extend([
        "",
        "## Users by Engine and Interface",
        "",
        "| Date | Total | V1 Classic | Headless | V2 TUI | V2 Non-Interact. | V2 ACP | V3 TUI | V3 Non-Interact. | V3 ACP | Other |",
        "|------|------:|----------:|----------:|----------:|----------:|----------:|----------:|----------:|----------:|----------:|",
    ])

    named_keys = {
        ("v1", "interactive_cli"),
        ("v1", "noninteractive_cli"),
        ("v2", "interactive_cli"),
        ("v2", "noninteractive_cli"),
        ("v2", "external_acp"),
        ("v3", "interactive_cli"),
        ("v3", "noninteractive_cli"),
        ("v3", "external_acp"),
    }

    for day in sorted_days:
        day_data = days[day]
        v1_ui = day_data.get(("v1", "interactive_cli"), {}).get("users", 0)
        v1_ni = day_data.get(("v1", "noninteractive_cli"), {}).get("users", 0)
        v2_ui = day_data.get(("v2", "interactive_cli"), {}).get("users", 0)
        v2_ni = day_data.get(("v2", "noninteractive_cli"), {}).get("users", 0)
        v2_acp = day_data.get(("v2", "external_acp"), {}).get("users", 0)
        v3_ui = day_data.get(("v3", "interactive_cli"), {}).get("users", 0)
        v3_ni = day_data.get(("v3", "noninteractive_cli"), {}).get("users", 0)
        v3_acp = day_data.get(("v3", "external_acp"), {}).get("users", 0)
        other = sum(
            v["users"] for k, v in day_data.items() if k not in named_keys
        )
        total = total_users_by_day.get(day, 0)
        lines.append(
            f"| {day.isoformat()} "
            f"| {total:,} "
            f"| {pct(v1_ui, total)} "
            f"| {pct(v1_ni, total)} "
            f"| {pct(v2_ui, total)} "
            f"| {pct(v2_ni, total)} "
            f"| {pct(v2_acp, total)} "
            f"| {pct(v3_ui, total)} "
            f"| {pct(v3_ni, total)} "
            f"| {pct(v3_acp, total)} "
            f"| {pct(other, total)} |"
        )

    # Section 2: Users by Engine and Internal/External
    lines.extend([
        "",
        "## Users by Engine and Internal/External",
        "",
        f"*Internal = Amazon corporate directory (`{INTERNAL_DIRECTORY}`). External = all other directories + Builder ID.*",
        "",
        "| Date | V1 Int | V1 Ext | V2 Int | V2 Ext | V3 Int | V3 Ext |",
        "|------|-------:|-------:|-------:|-------:|-------:|-------:|",
    ])

    for day in sorted_days:
        int_data = internal_engine_days.get(day, {})
        eng_data = total_engine_days.get(day, {})

        v1_total = eng_data.get("v1", 0)
        v2_total = eng_data.get("v2", 0)
        v3_total = eng_data.get("v3", 0)

        v1_int = int_data.get("v1", 0)
        v2_int = int_data.get("v2", 0)
        v3_int = int_data.get("v3", 0)

        v1_ext = max(0, v1_total - v1_int)
        v2_ext = max(0, v2_total - v2_int)
        v3_ext = max(0, v3_total - v3_int)

        lines.append(
            f"| {day.isoformat()} "
            f"| {v1_int:,} | {v1_ext:,} "
            f"| {v2_int:,} | {v2_ext:,} "
            f"| {v3_int:,} | {v3_ext:,} |"
        )

    # Methodology
    lines.extend([
        "",
        "## Methodology",
        "",
        "- **Metric**: `kiro_cli_user_turns` from KUTS-forwarded records in CloudWatch Logs.",
        "- **Turn**: One user prompt that received a complete agent response.",
        "- **Users**: `count_distinct(user_id)` — unique user identities per day.",
        "- **Total**: Deduplicated across all engines and interfaces.",
        "- **Coverage**: ~81% of turns have `user_id` (SSO/IdC users). ~19% missing (Builder ID) are not counted.",
        "- **Cost**: The report issues four separate Logs Insights queries over the same window; the scan line above sums all four.",
        "",
        "### Column Definitions",
        "",
        "| Column | Engine | Interface | Description |",
        "|--------|--------|-----------|-------------|",
        "| V1 Classic | V1 | interactive_cli | Legacy terminal UI |",
        "| Headless | V1 | noninteractive_cli | CI/scripted usage |",
        "| V2 TUI | V2 | interactive_cli | Twinki React terminal UI |",
        "| V2 Non-Interact. | V2 | noninteractive_cli | CI/scripted via V2 engine |",
        "| V2 ACP | V2 | external_acp | External ACP clients (meshclaw, kirocrew, etc.) |",
        "| V3 TUI | V3 | interactive_cli | KAS agent, Twinki React terminal UI |",
        "| V3 Non-Interact. | V3 | noninteractive_cli | CI/scripted via V3 engine |",
        "| V3 ACP | V3 | external_acp | External ACP clients via V3 (currently zero) |",
        "| Other | any | any | Unknown engine or any other engine/interface combination not above. Sums several per-bucket distinct counts, so it can exceed the deduplicated Total. |",
        "",
        "### Internal/External",
        "",
        f"- **Internal**: Users in Amazon corporate directory (`{INTERNAL_DIRECTORY}`)",
        "- **External**: All other IdC directories + Builder ID users with `user_id`",
        "",
        "### Notes",
        "",
        "- A user active across multiple interfaces is counted in each column independently (percentages may exceed 100%).",
        "- The current (incomplete) UTC day is always excluded.",
        "",
    ])
    return "\n".join(lines)


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        start_date, end_date = resolve_window(args.start, args.end)
        if args.timeout_seconds < 1:
            raise ValueError("--timeout-seconds must be at least 1")
        if args.query_only:
            for label, q in (
                ("main", QUERY),
                ("total", QUERY_TOTAL_USERS),
                ("total-engine", QUERY_TOTAL_BY_ENGINE),
                ("internal-engine", QUERY_INTERNAL_BY_ENGINE),
            ):
                print(f"-- {label} --")
                print(q)
            return 0

        if args.input_json:
            main_response = json.loads(args.input_json.read_text())
            total_response = {"status": "Complete", "results": []}
            total_engine_response = {"status": "Complete", "results": []}
            internal_engine_response = {"status": "Complete", "results": []}
            print(
                "Warning: --input-json only provides the main query data. "
                "Total and Internal/External sections will be incomplete.",
                file=sys.stderr,
            )
        else:
            profile = choose_profile(args.profile, os.environ, list_profiles())
            verify_account(profile, args.region)
            print(
                f"Querying {start_date} through {end_date} with AWS profile {profile}",
                file=sys.stderr,
            )
            main_response, total_response, total_engine_response, internal_engine_response = query_metrics(
                profile,
                args.region,
                args.log_group,
                start_date,
                end_date,
                args.timeout_seconds,
            )

        days = parse_query_results(main_response, end_date)
        total_users_by_day = parse_total_users(total_response, end_date)
        total_engine_days = parse_internal_results(total_engine_response, end_date)
        internal_engine_days = parse_internal_results(internal_engine_response, end_date)
        combined_stats = combine_statistics(
            main_response, total_response, total_engine_response, internal_engine_response
        )
        report = render_report(
            start_date,
            end_date,
            days,
            total_users_by_day,
            total_engine_days,
            internal_engine_days,
            combined_stats,
            region=args.region,
            log_group=args.log_group,
        )

        run_date = datetime.now(timezone.utc).strftime("%Y-%m-%d")
        output = Path(args.output) if args.output else Path(f"{run_date}-adoption-report.md")
        output.write_text(report)
        print(f"Report written to {output}", file=sys.stderr)
        return 0
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    sys.exit(main())
