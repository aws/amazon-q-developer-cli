#!/usr/bin/env python3
"""Generate active-installation adoption and engine usage from KUTS metrics."""

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
ENGINES = ("v1", "v2", "v3", "unknown")
TERMINAL_QUERY_STATUSES = {"Complete", "Failed", "Cancelled", "Timeout", "Unknown"}

# Group by every supported cohort in one scan, then aggregate into report views locally.
QUERY = """\
fields @timestamp,
       kiro_cli_daily_heartbeat,
       kiro_cli_user_turns,
       version_full,
       release_channel,
       os_type,
       install_method,
       session_interface,
       agent_engine
| filter `kuts.forwarded` = "true"
  and (ispresent(kiro_cli_daily_heartbeat) or ispresent(kiro_cli_user_turns))
| stats sum(kiro_cli_daily_heartbeat) as heartbeats,
        sum(kiro_cli_user_turns) as turns
  by bin(24h) as day,
     version_full,
     release_channel,
     os_type,
     install_method,
     session_interface,
     agent_engine
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
        description=(
            "Generate Kiro CLI active-installation adoption and V1/V2/V3 engine usage "
            "from production KUTS metrics."
        ),
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


def parse_optional_integer(row, field):
    value = row.get(field)
    return parse_integer(value, field) if value not in (None, "") else 0


def dimension(row, field):
    return row.get(field) or "unknown"


def parse_query_results(response):
    status = response.get("status")
    if status and status != "Complete":
        raise ValueError(f"query results are not complete (status: {status})")

    heartbeats = {}
    turns = {}
    for raw_row in response.get("results", []):
        row = row_to_dict(raw_row)
        raw_day = row.get("day", "")
        try:
            day = date.fromisoformat(raw_day[:10])
        except ValueError as error:
            raise ValueError(f"invalid day value {raw_day!r}") from error

        heartbeat_count = parse_optional_integer(row, "heartbeats")
        if heartbeat_count:
            key = (
                day,
                dimension(row, "version_full"),
                dimension(row, "release_channel"),
                dimension(row, "os_type"),
                dimension(row, "install_method"),
            )
            heartbeats[key] = heartbeats.get(key, 0) + heartbeat_count

        turn_count = parse_optional_integer(row, "turns")
        if turn_count:
            agent_engine = dimension(row, "agent_engine")
            if agent_engine not in ENGINES:
                agent_engine = "unknown"
            key = (
                day,
                dimension(row, "version_full"),
                agent_engine,
                dimension(row, "session_interface"),
            )
            turns[key] = turns.get(key, 0) + turn_count

    return {"heartbeats": heartbeats, "turns": turns}


def format_bytes(value):
    amount = float(value)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB", "PiB"):
        if amount < 1024 or unit == "PiB":
            return f"{amount:.1f} {unit}"
        amount /= 1024
    raise AssertionError("unreachable")


def report_days(start_date, end_date):
    days = []
    current = start_date
    while current <= end_date:
        days.append(current)
        current += timedelta(days=1)
    return list(reversed(days))


def percentage(value, total):
    return f"{value / total * 100:.1f}%" if total else "n/a"


def heartbeat_cohorts_for_day(heartbeats, day):
    return {
        key[1:]: value
        for key, value in heartbeats.items()
        if key[0] == day
    }


def turn_cohorts_for_day(turns, day):
    return {
        key[1:]: value
        for key, value in turns.items()
        if key[0] == day
    }


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
    days = report_days(start_date, end_date)
    heartbeats = results["heartbeats"]
    turns = results["turns"]
    lines = [
        "# Kiro CLI Adoption and Engine Usage Report",
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
            "## Active Installation-Version Adoption",
            "",
            "| Date | Version | Channel | Active installations | Daily share |",
            "|------|---------|---------|---------------------:|------------:|",
        ]
    )
    for day in days:
        cohorts = heartbeat_cohorts_for_day(heartbeats, day)
        versions = {}
        for (version, channel, _os_type, _install_method), count in cohorts.items():
            key = (version, channel)
            versions[key] = versions.get(key, 0) + count
        total = sum(versions.values())
        if not versions:
            lines.append(f"| {day.isoformat()} | n/a | n/a | 0 | n/a |")
            continue
        for (version, channel), count in sorted(versions.items()):
            lines.append(
                f"| {day.isoformat()} | `{version}` | {channel} | "
                f"{count:,} | {percentage(count, total)} |"
            )

    lines.extend(
        [
            "",
            "## Installation Detail",
            "",
            "| Date | Version | Channel | OS | Install method | Active installations |",
            "|------|---------|---------|----|----------------|---------------------:|",
        ]
    )
    for day in days:
        cohorts = heartbeat_cohorts_for_day(heartbeats, day)
        if not cohorts:
            lines.append(f"| {day.isoformat()} | n/a | n/a | n/a | n/a | 0 |")
            continue
        for (version, channel, os_type, install_method), count in sorted(cohorts.items()):
            lines.append(
                f"| {day.isoformat()} | `{version}` | {channel} | {os_type} | "
                f"{install_method} | {count:,} |"
            )

    lines.extend(
        [
            "",
            "## Engine Usage",
            "",
            "| Date | V1 turn share | V2 turn share | V3 turn share | Unknown share | "
            "Modern share | Top-level turns |",
            "|------|--------------:|--------------:|--------------:|--------------:|"
            "-------------:|----------------:|",
        ]
    )
    for day in days:
        cohorts = turn_cohorts_for_day(turns, day)
        engine_turns = {engine: 0 for engine in ENGINES}
        for (_version, agent_engine, _session_interface), count in cohorts.items():
            engine_turns[agent_engine] += count
        total = sum(engine_turns.values())
        lines.append(
            f"| {day.isoformat()} | {percentage(engine_turns['v1'], total)} | "
            f"{percentage(engine_turns['v2'], total)} | "
            f"{percentage(engine_turns['v3'], total)} | "
            f"{percentage(engine_turns['unknown'], total)} | "
            f"{percentage(engine_turns['v2'] + engine_turns['v3'], total)} | "
            f"{total:,} |"
        )

    lines.extend(
        [
            "",
            "## Engine and Interface Detail",
            "",
            "| Date | Version | Engine | Session interface | Top-level turns |",
            "|------|---------|--------|-------------------|----------------:|",
        ]
    )
    for day in days:
        cohorts = turn_cohorts_for_day(turns, day)
        detail = {}
        for (version, agent_engine, session_interface), count in cohorts.items():
            key = (version, agent_engine, session_interface)
            detail[key] = detail.get(key, 0) + count
        if not detail:
            lines.append(f"| {day.isoformat()} | n/a | n/a | n/a | 0 |")
            continue
        for (version, agent_engine, session_interface), count in sorted(detail.items()):
            lines.append(
                f"| {day.isoformat()} | `{version}` | {agent_engine.upper()} | "
                f"{session_interface} | {count:,} |"
            )

    lines.extend(
        [
            "",
            "## Methodology",
            "",
            "- Active installations are daily sums of `kiro_cli_daily_heartbeat`, grouped by "
            "exact version, release channel, OS, and install method.",
            "- A heartbeat is one active installation-version day, not a unique person. An "
            "installation that runs two versions in one day contributes to both versions.",
            "- Multi-day totals are installation-version days. They are not weekly or monthly "
            "active-installation counts because installations repeat across days.",
            "- Engine and interface usage is the sum of completed top-level "
            "`kiro_cli_user_turns`. Turn share weights frequent users more heavily and is not "
            "user or installation adoption.",
            "- Current producers suppress subagent turn counters, so the query does not depend "
            "on the retired `is_subagent` attribute.",
            "- Missing reviewed dimensions are rendered as `unknown`. Expect a rollout-era "
            "unknown cohort from clients that predate the new metric contract.",
            "- `install_method=unknown` must not be interpreted as `installation_script`; the "
            "installer receipt is separate rollout work.",
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
