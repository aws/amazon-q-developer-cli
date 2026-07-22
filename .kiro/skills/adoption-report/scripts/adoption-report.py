#!/usr/bin/env python3
"""Generate daily V1/V2/V3 adoption from forwarded KUTS metrics."""

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import date, datetime, time as datetime_time, timedelta, timezone
from decimal import Decimal, InvalidOperation
from pathlib import Path

ACCOUNT = "615299732016"
DEFAULT_LOG_GROUP = "/kiro/metrics"
DEFAULT_REGION = "us-east-1"
DEFAULT_OUTPUT = "adoption-report.md"
ENGINES = ("v1", "v2", "v3")
TERMINAL_QUERY_STATUSES = {"Complete", "Failed", "Cancelled", "Timeout", "Unknown"}

QUERY = """\
fields @timestamp, engine, user_id, is_subagent, kiro_cli_user_turns
| filter ispresent(kiro_cli_user_turns)
  and `kuts.forwarded` = "true"
  and ispresent(user_id)
  and user_id != ""
  and engine in ["v1", "v2", "v3"]
  and is_subagent = "false"
| stats count_distinct(user_id) as users,
        sum(kiro_cli_user_turns) as turns
  by bin(24h) as day, engine
| sort day desc
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
    if end_date < start_date:
        raise ValueError("end date must be on or after start date")
    return start_date, end_date


def build_parser():
    parser = argparse.ArgumentParser(
        description="Generate V1/V2/V3 adoption from production KUTS user-turn metrics.",
        epilog=(
            "Dates are UTC and inclusive. A numeric range selects the last N complete UTC days. "
            "Keep ranges narrow because Logs Insights scans the shared metrics log group."
        ),
    )
    parser.add_argument("start", help="start date (YYYY-MM-DD) or number of complete UTC days")
    parser.add_argument("end", nargs="?", help="inclusive end date (YYYY-MM-DD)")
    parser.add_argument("-o", "--output", default=DEFAULT_OUTPUT, help="Markdown output path")
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


def query_metrics(profile, region, log_group, start_date, end_date, timeout_seconds):
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
        QUERY,
    )
    query_id = started.get("queryId")
    if not query_id:
        raise RuntimeError("CloudWatch Logs did not return a query ID")

    print(f"Started Logs Insights query {query_id}", file=sys.stderr)
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
                print(f"Query status: {status}", file=sys.stderr)
                last_status = status
            if status in TERMINAL_QUERY_STATUSES:
                if status != "Complete":
                    raise RuntimeError(f"Logs Insights query ended with status {status}")
                return response
            time.sleep(2)
    except BaseException:
        stop_query(profile, region, query_id)
        raise

    stop_query(profile, region, query_id)
    raise RuntimeError(f"Logs Insights query exceeded {timeout_seconds} seconds")


def row_to_dict(row):
    return {item["field"]: item.get("value", "") for item in row if "field" in item}


def parse_integer(value, field):
    try:
        number = Decimal(value)
    except InvalidOperation as error:
        raise ValueError(f"invalid {field} value {value!r}") from error
    if number != number.to_integral_value():
        raise ValueError(f"expected integral {field}, got {value!r}")
    return int(number)


def parse_query_results(response):
    status = response.get("status")
    if status and status != "Complete":
        raise ValueError(f"query results are not complete (status: {status})")

    parsed = {}
    for raw_row in response.get("results", []):
        row = row_to_dict(raw_row)
        engine = row.get("engine")
        if engine not in ENGINES:
            continue
        raw_day = row.get("day", "")
        try:
            day = date.fromisoformat(raw_day[:10])
        except ValueError as error:
            raise ValueError(f"invalid day value {raw_day!r}") from error
        key = (day, engine)
        users = parse_integer(row.get("users", ""), "users")
        turns = parse_integer(row.get("turns", ""), "turns")
        previous = parsed.get(key, {"users": 0, "turns": 0})
        parsed[key] = {
            "users": previous["users"] + users,
            "turns": previous["turns"] + turns,
        }
    return parsed


def format_bytes(value):
    amount = float(value)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB", "PiB"):
        if amount < 1024 or unit == "PiB":
            return f"{amount:.1f} {unit}"
        amount /= 1024
    raise AssertionError("unreachable")


def render_report(
    start_date,
    end_date,
    results,
    statistics=None,
    generated_at=None,
    region=DEFAULT_REGION,
    log_group=DEFAULT_LOG_GROUP,
):
    generated_at = generated_at or datetime.now(timezone.utc)
    lines = [
        "# Kiro CLI Engine Adoption Report",
        "",
        f"Generated: {generated_at.strftime('%Y-%m-%d %H:%M UTC')}",
        f"Window: {start_date.isoformat()} through {end_date.isoformat()} (UTC, inclusive)",
        f"Source: AWS account `{ACCOUNT}`, `{region}`, `{log_group}`",
    ]
    if statistics and statistics.get("bytesScanned") is not None:
        lines.append(
            "Query scan: "
            f"{format_bytes(statistics['bytesScanned'])}, "
            f"{int(statistics.get('recordsScanned', 0)):,} records"
        )

    lines.extend(
        [
            "",
            "## Agent Version Adoption",
            "",
            "| Date | V1 share | V2 share | V3 share | Modern share |",
            "|------|---------:|---------:|---------:|-------------:|",
        ]
    )
    days = []
    current = start_date
    while current <= end_date:
        days.append(current)
        current += timedelta(days=1)

    for day in reversed(days):
        users = {engine: results.get((day, engine), {}).get("users", 0) for engine in ENGINES}
        total = sum(users.values())
        shares = {
            engine: f"{users[engine] / total * 100:.1f}%" if total else "n/a"
            for engine in ENGINES
        }
        modern_share = f"{(users['v2'] + users['v3']) / total * 100:.1f}%" if total else "n/a"
        lines.append(
            f"| {day.isoformat()} | {shares['v1']} | {shares['v2']} | "
            f"{shares['v3']} | {modern_share} |"
        )

    lines.extend(
        [
            "",
            "## Daily Detail",
            "",
            "| Date | Engine | Distinct users | User turns |",
            "|------|--------|---------------:|-----------:|",
        ]
    )
    for day in reversed(days):
        for engine in ENGINES:
            values = results.get((day, engine), {"users": 0, "turns": 0})
            lines.append(
                f"| {day.isoformat()} | {engine.upper()} | "
                f"{values['users']:,} | {values['turns']:,} |"
            )

    lines.extend(
        [
            "",
            "## Methodology",
            "",
            "- Each version share is `version users / (V1 users + V2 users + V3 users)`. "
            "Modern share combines V2 and V3.",
            "- Users are CloudWatch `count_distinct(user_id)` estimates. A person using "
            "multiple engines in one day is counted once in each engine, so the percentage "
            "is an engine-user adoption proxy rather than a mutually exclusive user split.",
            "- Only forwarded top-level `kiro_cli_user_turns` records with a non-empty "
            "`user_id` are included. Logged-out or otherwise unidentified usage is excluded.",
            "- Missing engine buckets are rendered as zero. Treat unexpected zeros as possible "
            "telemetry gaps until the engine's rollout and emission path are confirmed.",
            "- V1 data is incomplete before the V1 KUTS metric rollout. Do not treat "
            "pre-rollout dates as complete V1-versus-modern adoption.",
            "",
        ]
    )
    return "\n".join(lines)


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        start_date, end_date = resolve_window(args.start, args.end)
        if args.timeout_seconds < 1:
            raise ValueError("--timeout-seconds must be at least 1")
        if args.query_only:
            print(QUERY, end="")
            return 0

        if args.input_json:
            response = json.loads(args.input_json.read_text())
        else:
            profile = choose_profile(args.profile, os.environ, list_profiles())
            verify_account(profile, args.region)
            print(
                f"Querying {start_date} through {end_date} with AWS profile {profile}",
                file=sys.stderr,
            )
            response = query_metrics(
                profile,
                args.region,
                args.log_group,
                start_date,
                end_date,
                args.timeout_seconds,
            )

        results = parse_query_results(response)
        report = render_report(
            start_date,
            end_date,
            results,
            response.get("statistics"),
            region=args.region,
            log_group=args.log_group,
        )
        output = Path(args.output)
        output.write_text(report)
        print(f"Report written to {output}", file=sys.stderr)
        return 0
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    sys.exit(main())
