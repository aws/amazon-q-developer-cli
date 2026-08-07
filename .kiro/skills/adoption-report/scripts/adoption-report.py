#!/usr/bin/env python3
"""Generate a source-aware Kiro CLI adoption report."""

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
SOURCES = ("both", "kuts", "toolkit")
TOOLKIT_METRIC = "codewhispererterminal_recordUserTurnCompletion"
TOOLKIT_CLI_SEGMENTS = ("int_v1", "int_v2", "ext_v1", "ext_v2")
TOOLKIT_SEGMENTS = (*TOOLKIT_CLI_SEGMENTS, "acp")
TOOLKIT_QUERY_SCRIPT = Path(__file__).resolve().parents[4] / "scripts" / "es-query.sh"
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


def build_toolkit_query():
    internal = {"wildcard": {"metadata.credentialStartUrl": "*amzn.awsapps.com*"}}
    app_type_exists = {"exists": {"field": "metadata.kirocli_appType"}}
    v2 = {"match": {"metadata.kirocli_appType": "V2"}}
    acp = {"match": {"metadata.kirocli_appType": "ACP"}}
    filters = {
        "int_v1": {"must": [internal], "must_not": [app_type_exists]},
        "int_v2": {"must": [internal, v2]},
        "ext_v1": {"must_not": [internal, app_type_exists]},
        "ext_v2": {"must": [v2], "must_not": [internal]},
        "acp": {"must": [acp]},
    }
    return {
        "size": 0,
        "query": {
            "bool": {
                "must": [
                    {"match_phrase": {"metadata.metricName": TOOLKIT_METRIC}},
                    {"match_phrase": {"product": "CodeWhisperer for Terminal"}},
                ]
            }
        },
        "aggs": {
            name: {
                "filter": {"bool": segment_filter},
                "aggs": {"installations": {"cardinality": {"field": "clientId"}}},
            }
            for name, segment_filter in filters.items()
        },
    }


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
            "Compare Kiro CLI adoption and usage from production KUTS and legacy "
            "Toolkit telemetry."
        ),
        epilog=(
            "Dates are UTC and inclusive. A numeric range selects the last N complete UTC days. "
            "Keep ranges narrow because Logs Insights scans the shared metrics log group."
        ),
    )
    parser.add_argument(
        "start", help="start date (YYYY-MM-DD) or number of complete UTC days"
    )
    parser.add_argument("end", nargs="?", help="inclusive end date (YYYY-MM-DD)")
    parser.add_argument(
        "-o", "--output", default=DEFAULT_OUTPUT, help="Markdown output path"
    )
    parser.add_argument(
        "--source",
        choices=SOURCES,
        help=(
            "data source to query (default: supplied offline sources, otherwise both)"
        ),
    )
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
        help="print the selected source queries without calling either service",
    )
    kuts_input = parser.add_mutually_exclusive_group()
    kuts_input.add_argument(
        "--input-json",
        dest="legacy_input_json",
        type=Path,
        help="legacy saved KUTS input (selects KUTS-only unless --source is set)",
    )
    kuts_input.add_argument(
        "--kuts-input-json",
        type=Path,
        help="render saved KUTS get-query-results JSON instead of querying AWS",
    )
    parser.add_argument(
        "--toolkit-input-json",
        type=Path,
        help="render saved Toolkit responses keyed by date instead of querying Elasticsearch",
    )
    parser.add_argument(
        "--toolkit-query-script",
        type=Path,
        default=TOOLKIT_QUERY_SCRIPT,
        help=argparse.SUPPRESS,
    )
    return parser


def effective_source(args):
    if args.source:
        return args.source
    has_kuts_input = bool(args.legacy_input_json or args.kuts_input_json)
    has_toolkit_input = bool(args.toolkit_input_json)
    if has_kuts_input and has_toolkit_input:
        return "both"
    if has_kuts_input:
        return "kuts"
    if has_toolkit_input:
        return "toolkit"
    return "both"


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
        raise RuntimeError(
            f"could not list AWS profiles: {error.stderr.strip()}"
        ) from error
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
    end_time = datetime.combine(
        end_date + timedelta(days=1), datetime_time.min, timezone.utc
    )
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
                    raise RuntimeError(
                        f"Logs Insights query ended with status {status}"
                    )
                return response
            time.sleep(2)
    except BaseException:
        stop_query(profile, region, query_id)
        raise

    stop_query(profile, region, query_id)
    raise RuntimeError(f"Logs Insights query exceeded {timeout_seconds} seconds")


def verify_toolkit_credentials(query_script, environ):
    cookie_file = query_script.parent / ".es-cookie"
    if environ.get("ES_COOKIE"):
        return
    try:
        has_cookie_file = cookie_file.is_file() and bool(
            cookie_file.read_text().strip()
        )
    except OSError as error:
        raise RuntimeError(f"could not read Toolkit cookie file: {error}") from error
    if not has_cookie_file:
        raise RuntimeError(
            "Toolkit credentials are unavailable; set ES_COOKIE or refresh scripts/.es-cookie"
        )


def run_toolkit_search(query_script, index, query):
    try:
        result = subprocess.run(
            [str(query_script), index, json.dumps(query, separators=(",", ":"))],
            check=True,
            capture_output=True,
            text=True,
            timeout=180,
        )
    except FileNotFoundError as error:
        raise RuntimeError(f"Toolkit query script not found: {query_script}") from error
    except subprocess.TimeoutExpired as error:
        raise RuntimeError(f"Toolkit query timed out for {index}") from error
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() or error.stdout.strip() or "unknown error"
        raise RuntimeError(f"Toolkit query failed for {index}: {detail}") from error
    try:
        response = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(
            f"Toolkit query returned invalid JSON for {index}"
        ) from error
    if "error" in response:
        raise RuntimeError(f"Toolkit query failed for {index}: {response['error']}")
    return response


def calendar_days(start_date, end_date):
    current = start_date
    while current <= end_date:
        yield current
        current += timedelta(days=1)


def query_toolkit(query_script, start_date, end_date):
    verify_toolkit_credentials(query_script, os.environ)
    query = build_toolkit_query()
    results = {}
    errors = {}
    for day in calendar_days(start_date, end_date):
        index = f"metrics-{day.isoformat()}"
        print(f"Querying Toolkit telemetry for {day.isoformat()}", file=sys.stderr)
        try:
            for attempt in range(3):
                try:
                    response = run_toolkit_search(query_script, index, query)
                    break
                except RuntimeError:
                    if attempt == 2:
                        raise
                    print(
                        f"Retrying Toolkit query for {day.isoformat()} "
                        f"({attempt + 2}/3)",
                        file=sys.stderr,
                    )
                    time.sleep(5)
            results[day] = parse_toolkit_response(response)
        except (RuntimeError, ValueError) as error:
            errors[day] = error
    return results, errors


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


def parse_toolkit_response(response):
    aggregations = response.get("aggregations")
    if not isinstance(aggregations, dict):
        raise ValueError("Toolkit response is missing aggregations")

    result = {}
    for segment in TOOLKIT_SEGMENTS:
        bucket = aggregations.get(segment)
        if not isinstance(bucket, dict):
            raise ValueError(f"Toolkit response is missing {segment} aggregation")
        installations = bucket.get("installations")
        if not isinstance(installations, dict) or "value" not in installations:
            raise ValueError(
                f"Toolkit response is missing {segment} installation cardinality"
            )
        result[segment] = {
            "installations": parse_integer(
                str(installations["value"]), f"{segment} installations"
            ),
            "turns": parse_integer(
                str(bucket.get("doc_count", "")), f"{segment} turns"
            ),
        }
    return result


def load_toolkit_results(path, start_date, end_date):
    payload = json.loads(path.read_text(encoding="utf-8"))
    if "aggregations" in payload:
        if start_date != end_date:
            raise ValueError(
                "a single Toolkit response can only be used for a one-day report"
            )
        return {start_date: parse_toolkit_response(payload)}

    responses = payload.get("responses", payload)
    if not isinstance(responses, dict):
        raise ValueError("Toolkit input must be a response or responses keyed by date")

    results = {}
    for raw_day, response in responses.items():
        day = parse_date(raw_day)
        if start_date <= day <= end_date:
            results[day] = parse_toolkit_response(response)
    if not results:
        raise ValueError("Toolkit input contains no responses in the requested window")
    return results


def format_bytes(value):
    amount = float(value)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB", "PiB"):
        if amount < 1024 or unit == "PiB":
            return f"{amount:.1f} {unit}"
        amount /= 1024
    raise AssertionError("unreachable")


def report_days(start_date, end_date):
    return list(reversed(list(calendar_days(start_date, end_date))))


def percentage(value, total):
    return f"{value / total * 100:.1f}%" if total else "n/a"


def heartbeat_cohorts_for_day(heartbeats, day):
    return {key[1:]: value for key, value in heartbeats.items() if key[0] == day}


def turn_cohorts_for_day(turns, day):
    return {key[1:]: value for key, value in turns.items() if key[0] == day}


def engine_turns_for_day(turns, day):
    totals = {engine: 0 for engine in ENGINES}
    for (_version, agent_engine, _session_interface), count in turn_cohorts_for_day(
        turns, day
    ).items():
        totals[agent_engine] += count
    return totals


def comparison_turns_for_day(turns, day):
    engine_totals = {engine: 0 for engine in ENGINES}
    acp = 0
    total = 0
    for (_version, agent_engine, session_interface), count in turn_cohorts_for_day(
        turns, day
    ).items():
        total += count
        if session_interface == "external_acp":
            acp += count
        else:
            engine_totals[agent_engine] += count
    return {
        "v1": engine_totals["v1"],
        "modern": engine_totals["v2"] + engine_totals["v3"],
        "acp": acp,
        "total": total,
    }


def toolkit_segment(results, day, segment):
    return results[day][segment]


def compact_error(error):
    detail = " ".join(str(error).split()).replace("|", "\\|")
    return detail if len(detail) <= 180 else f"{detail[:177]}..."


def gap_display(kuts, toolkit):
    gap = kuts - toolkit
    return f"{gap:+,} ({gap / toolkit * 100:+.1f}%)" if toolkit else "n/a"


def render_report(
    start_date,
    end_date,
    results,
    statistics=None,
    generated_at=None,
    region=DEFAULT_REGION,
    log_group=DEFAULT_LOG_GROUP,
    toolkit_results=None,
    source_errors=None,
    kuts_source=None,
    toolkit_source=None,
    toolkit_day_errors=None,
):
    generated_at = generated_at or datetime.now(timezone.utc)
    days = report_days(start_date, end_date)
    source_errors = source_errors or {}
    toolkit_day_errors = toolkit_day_errors or {}
    lines = [
        "# Kiro CLI Adoption Report",
        "",
        f"Generated: {generated_at.strftime('%Y-%m-%d %H:%M UTC')}",
        f"Window: {start_date.isoformat()} through {end_date.isoformat()} (UTC, inclusive)",
        "",
        "## Data Sources",
        "",
    ]
    if results is not None:
        source = kuts_source or f"AWS account `{ACCOUNT}`, `{region}`, `{log_group}`"
        if statistics and statistics.get("bytesScanned") is not None:
            source += (
                f"; scanned {format_bytes(statistics['bytesScanned'])}, "
                f"{int(statistics.get('recordsScanned', 0)):,} records"
            )
        lines.append(f"- **KUTS**: available from {source}.")
    elif "kuts" in source_errors:
        lines.append(
            f"- **KUTS**: unavailable ({compact_error(source_errors['kuts'])})."
        )
    if toolkit_results is not None:
        source = toolkit_source or "the legacy telemetry Elasticsearch daily indexes"
        if toolkit_day_errors:
            failures = ", ".join(
                f"`{day.isoformat()}` ({compact_error(error)})"
                for day, error in sorted(toolkit_day_errors.items())
            )
            lines.append(
                f"- **Toolkit**: partially available from {source}; "
                f"unavailable dates: {failures}."
            )
        else:
            lines.append(f"- **Toolkit**: available from {source}.")
    elif "toolkit" in source_errors:
        lines.append(
            f"- **Toolkit**: unavailable ({compact_error(source_errors['toolkit'])})."
        )

    if results is not None:
        heartbeats = results["heartbeats"]
        turns = results["turns"]
        lines.extend(
            [
                "",
                "## Active Installation-Version Adoption (KUTS)",
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
                "## Installation Detail (KUTS)",
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
            for (version, channel, os_type, install_method), count in sorted(
                cohorts.items()
            ):
                lines.append(
                    f"| {day.isoformat()} | `{version}` | {channel} | {os_type} | "
                    f"{install_method} | {count:,} |"
                )

        lines.extend(
            [
                "",
                "## Engine Usage (KUTS)",
                "",
                "| Date | V1 turn share | V2 turn share | V3 turn share | Unknown share | "
                "Modern share | Top-level turns |",
                "|------|--------------:|--------------:|--------------:|--------------:|"
                "-------------:|----------------:|",
            ]
        )
        for day in days:
            engine_turns = engine_turns_for_day(turns, day)
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
                "## Engine and Interface Detail (KUTS)",
                "",
                "| Date | Version | Engine | Session interface | Top-level turns |",
                "|------|---------|--------|-------------------|----------------:|",
            ]
        )
        for day in days:
            cohorts = turn_cohorts_for_day(turns, day)
            if not cohorts:
                lines.append(f"| {day.isoformat()} | n/a | n/a | n/a | 0 |")
                continue
            for (version, agent_engine, session_interface), count in sorted(
                cohorts.items()
            ):
                lines.append(
                    f"| {day.isoformat()} | `{version}` | {agent_engine.upper()} | "
                    f"{session_interface} | {count:,} |"
                )

    if toolkit_results is not None:
        lines.extend(
            [
                "",
                "## V1/V2 Active Installation Segments (Toolkit)",
                "",
                "| Date | Internal V2 segment share | External V2 segment share |",
                "|------|--------------------------:|--------------------------:|",
            ]
        )
        for day in days:
            if day not in toolkit_results:
                lines.append(f"| {day.isoformat()} | n/a | n/a |")
                continue
            int_v1 = toolkit_segment(toolkit_results, day, "int_v1")["installations"]
            int_v2 = toolkit_segment(toolkit_results, day, "int_v2")["installations"]
            ext_v1 = toolkit_segment(toolkit_results, day, "ext_v1")["installations"]
            ext_v2 = toolkit_segment(toolkit_results, day, "ext_v2")["installations"]
            lines.append(
                f"| {day.isoformat()} | {percentage(int_v2, int_v1 + int_v2)} | "
                f"{percentage(ext_v2, ext_v1 + ext_v2)} |"
            )

        lines.extend(
            [
                "",
                "## Detailed V1/V2/ACP Usage (Toolkit)",
                "",
                "| Date | Int V1 est. installs | Int V1 turns | Int V2 est. installs | "
                "Int V2 turns | Ext V1 est. installs | Ext V1 turns | "
                "Ext V2 est. installs | Ext V2 turns | ACP est. installs | ACP turns |",
                "|------|----------------:|-------------:|----------------:|-------------:|"
                "----------------:|-------------:|----------------:|-------------:|"
                "------------------:|----------:|",
            ]
        )
        for day in days:
            if day not in toolkit_results:
                lines.append(
                    f"| {day.isoformat()} | n/a | n/a | n/a | n/a | "
                    "n/a | n/a | n/a | n/a | n/a | n/a |"
                )
                continue
            values = [
                value
                for segment in TOOLKIT_SEGMENTS
                for value in (
                    toolkit_segment(toolkit_results, day, segment)["installations"],
                    toolkit_segment(toolkit_results, day, segment)["turns"],
                )
            ]
            lines.append(
                f"| {day.isoformat()} | "
                + " | ".join(f"{value:,}" for value in values)
                + " |"
            )

    if results is not None and toolkit_results is not None:
        lines.extend(
            [
                "",
                "## Daily Turn Comparison (KUTS vs Toolkit)",
                "",
                "| Date | Toolkit V1 | KUTS non-ACP V1 | V1 gap | Toolkit V2 app | "
                "KUTS non-ACP V2+V3 | V2 gap | Toolkit ACP | KUTS external ACP | "
                "ACP gap | Toolkit all | KUTS all | Total gap |",
                "|------|-----------:|----------------:|-------:|---------------:|"
                "-------------------:|-------:|------------:|------------------:|"
                "--------:|------------:|---------:|----------:|",
            ]
        )
        for day in days:
            kuts = comparison_turns_for_day(results["turns"], day)
            if day not in toolkit_results:
                lines.append(
                    f"| {day.isoformat()} | n/a | {kuts['v1']:,} | n/a | "
                    f"n/a | {kuts['modern']:,} | n/a | n/a | {kuts['acp']:,} | "
                    f"n/a | n/a | {kuts['total']:,} | n/a |"
                )
                continue
            toolkit_v1 = sum(
                toolkit_segment(toolkit_results, day, segment)["turns"]
                for segment in ("int_v1", "ext_v1")
            )
            toolkit_v2 = sum(
                toolkit_segment(toolkit_results, day, segment)["turns"]
                for segment in ("int_v2", "ext_v2")
            )
            toolkit_acp = toolkit_segment(toolkit_results, day, "acp")["turns"]
            toolkit_total = toolkit_v1 + toolkit_v2 + toolkit_acp
            lines.append(
                f"| {day.isoformat()} | {toolkit_v1:,} | {kuts['v1']:,} | "
                f"{gap_display(kuts['v1'], toolkit_v1)} | {toolkit_v2:,} | "
                f"{kuts['modern']:,} | {gap_display(kuts['modern'], toolkit_v2)} | "
                f"{toolkit_acp:,} | {kuts['acp']:,} | "
                f"{gap_display(kuts['acp'], toolkit_acp)} | {toolkit_total:,} | "
                f"{kuts['total']:,} | {gap_display(kuts['total'], toolkit_total)} |"
            )

    lines.extend(
        [
            "",
            "## Methodology",
            "",
        ]
    )
    if results is not None:
        lines.extend(
            [
                "- **KUTS active installations** are daily sums of "
                "`kiro_cli_daily_heartbeat`. A heartbeat is one installation-version day, "
                "not a unique person. Multi-day sums repeat installations.",
                "- **KUTS turns** are completed top-level `kiro_cli_user_turns`, grouped by "
                "`agent_engine` and `session_interface`. Turn share weights frequent users.",
                "- KUTS producers suppress subagent turn counters. Missing reviewed dimensions "
                "appear as `unknown`, including records from older clients.",
                "- `install_method=unknown` is unattributable and must not be relabeled as "
                "`installation_script`.",
            ]
        )
    if toolkit_results is not None:
        lines.extend(
            [
                "- **Toolkit installations** are approximate daily Elasticsearch cardinality "
                "estimates of `clientId` among completed-turn events. They are installations, "
                "not unique people.",
                "- Toolkit V2 requires `metadata.kirocli_appType=V2`, V1 lacks that field, "
                "and external ACP requires `metadata.kirocli_appType=ACP`. Internal requires "
                "`metadata.credentialStartUrl` matching `amzn.awsapps.com`.",
                "- Toolkit installation segments can overlap when one installation uses more "
                "than one engine in a day, so do not sum segment cardinalities as a unique total.",
            ]
        )
    if results is not None and toolkit_results is not None:
        lines.extend(
            [
                "- **Do not directly compare installation counts across sources.** KUTS counts "
                "heartbeat installation-version days; Toolkit estimates distinct `clientId` "
                "values observed on turn events.",
                "- Turn counts are the closest overlap, but producer coverage, classification, "
                "and rollout timing differ. Use gaps as migration-coverage signals, not proof "
                "that either source is wrong.",
                "- Toolkit V1/V2 app-type buckets are compared with KUTS non-ACP V1 and V2+V3 "
                "turns because Toolkit cannot isolate V3. Toolkit ACP is compared separately "
                "with KUTS `external_acp` turns.",
                "- `Toolkit all` includes V1, V2, and ACP turns. `KUTS all` includes every "
                "session interface and the `unknown` engine cohort. The signed gaps make "
                "source-coverage differences visible; they are not correction factors.",
            ]
        )
    lines.append("")
    return "\n".join(lines)


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        start_date, end_date = resolve_window(args.start, args.end)
        source = effective_source(args)
        if args.timeout_seconds < 1:
            raise ValueError("--timeout-seconds must be at least 1")
        if args.query_only:
            if source in ("both", "kuts"):
                print("# KUTS CloudWatch Logs Insights query")
                print(QUERY)
            if source in ("both", "toolkit"):
                print("# Toolkit Elasticsearch query")
                print(json.dumps(build_toolkit_query(), indent=2))
            return 0

        results = None
        statistics = None
        toolkit_results = None
        source_errors = {}
        kuts_source = None
        toolkit_source = None
        toolkit_day_errors = {}

        if source in ("both", "kuts"):
            try:
                kuts_input_json = args.legacy_input_json or args.kuts_input_json
                if kuts_input_json:
                    response = json.loads(kuts_input_json.read_text(encoding="utf-8"))
                    kuts_source = f"saved KUTS JSON `{kuts_input_json}`"
                else:
                    profile = choose_profile(args.profile, os.environ, list_profiles())
                    verify_account(profile, args.region)
                    print(
                        f"Querying KUTS for {start_date} through {end_date} "
                        f"with AWS profile {profile}",
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
                    kuts_source = (
                        f"AWS account `{ACCOUNT}`, `{args.region}`, `{args.log_group}`"
                    )
                results = parse_query_results(response)
                statistics = response.get("statistics")
            except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
                source_errors["kuts"] = error
                print(f"KUTS source unavailable: {error}", file=sys.stderr)

        if source in ("both", "toolkit"):
            try:
                if args.toolkit_input_json:
                    toolkit_results = load_toolkit_results(
                        args.toolkit_input_json, start_date, end_date
                    )
                    toolkit_day_errors = {
                        day: "saved response is missing"
                        for day in calendar_days(start_date, end_date)
                        if day not in toolkit_results
                    }
                    toolkit_source = f"saved Toolkit JSON `{args.toolkit_input_json}`"
                else:
                    toolkit_results, toolkit_day_errors = query_toolkit(
                        args.toolkit_query_script, start_date, end_date
                    )
                    if not toolkit_results:
                        details = "; ".join(
                            f"{day.isoformat()}: {compact_error(error)}"
                            for day, error in sorted(toolkit_day_errors.items())
                        )
                        raise RuntimeError(
                            f"Toolkit queries failed for every requested date ({details})"
                        )
                    toolkit_source = "the legacy telemetry Elasticsearch daily indexes"
            except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
                toolkit_results = None
                toolkit_day_errors = {}
                source_errors["toolkit"] = error
                print(f"Toolkit source unavailable: {error}", file=sys.stderr)

        if results is None and toolkit_results is None:
            details = "; ".join(
                f"{source}: {error}" for source, error in source_errors.items()
            )
            raise RuntimeError(f"no adoption data source was available ({details})")

        report = render_report(
            start_date,
            end_date,
            results,
            statistics,
            region=args.region,
            log_group=args.log_group,
            toolkit_results=toolkit_results,
            source_errors=source_errors,
            kuts_source=kuts_source,
            toolkit_source=toolkit_source,
            toolkit_day_errors=toolkit_day_errors,
        )
        output = Path(args.output)
        output.write_text(report, encoding="utf-8")
        print(f"Report written to {output}", file=sys.stderr)
        return 0
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    sys.exit(main())
