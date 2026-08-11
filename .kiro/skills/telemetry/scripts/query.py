#!/usr/bin/env python3
"""Run a CloudWatch Logs Insights query against the KUTS /kiro/metrics log group."""

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import date, datetime, time as datetime_time, timedelta, timezone

ACCOUNT = "615299732016"
DEFAULT_LOG_GROUP = "/kiro/metrics"
DEFAULT_REGION = "us-east-1"
TERMINAL_QUERY_STATUSES = {"Complete", "Failed", "Cancelled", "Timeout", "Unknown"}


def choose_profile(explicit, environ):
    if explicit:
        return explicit
    if environ.get("KUTS_AWS_PROFILE"):
        return environ["KUTS_AWS_PROFILE"]
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
    raise RuntimeError("no KUTS AWS profile found; pass --profile or set KUTS_AWS_PROFILE")


def run_aws(profile, region, *arguments):
    command = ["aws", "--profile", profile, "--region", region, "--no-cli-pager", *arguments, "--output", "json"]
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
        raise RuntimeError(f"AWS CLI returned invalid JSON: {result.stdout[:200]}") from error


def verify_account(profile, region):
    identity = run_aws(profile, region, "sts", "get-caller-identity")
    actual_account = identity.get("Account")
    if actual_account != ACCOUNT:
        raise RuntimeError(
            f"profile {profile!r} resolves to account {actual_account or 'unknown'}, "
            f"expected {ACCOUNT}"
        )


def format_bytes(value):
    amount = float(value)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB", "PiB"):
        if amount < 1024 or unit == "PiB":
            return f"{amount:.1f} {unit}"
        amount /= 1024
    raise AssertionError("unreachable")


def run_query(profile, region, log_group, start_epoch, end_epoch, query_string, timeout_seconds):
    started = run_aws(
        profile, region,
        "logs", "start-query",
        "--log-group-name", log_group,
        "--start-time", str(start_epoch),
        "--end-time", str(end_epoch),
        "--query-string", query_string,
    )
    query_id = started.get("queryId")
    if not query_id:
        raise RuntimeError("CloudWatch Logs did not return a query ID")

    print(f"Query: {query_id}", file=sys.stderr)
    deadline = time.monotonic() + timeout_seconds
    last_status = None
    try:
        while time.monotonic() < deadline:
            response = run_aws(profile, region, "logs", "get-query-results", "--query-id", query_id)
            status = response.get("status", "Unknown")
            if status != last_status:
                print(f"Status: {status}", file=sys.stderr)
                last_status = status
            if status in TERMINAL_QUERY_STATUSES:
                if status != "Complete":
                    raise RuntimeError(f"Query ended with status {status}")
                return response
            time.sleep(3)
    except BaseException:
        try:
            run_aws(profile, region, "logs", "stop-query", "--query-id", query_id)
        except RuntimeError:
            pass
        raise

    raise RuntimeError(f"Query exceeded {timeout_seconds} seconds")


def main():
    parser = argparse.ArgumentParser(description="Run a Logs Insights query against /kiro/metrics")
    parser.add_argument("query", help="Logs Insights query string")
    parser.add_argument("--days", type=int, help="Query last N complete UTC days")
    parser.add_argument("--start", help="Start date (YYYY-MM-DD)")
    parser.add_argument("--end", help="End date (YYYY-MM-DD)")
    parser.add_argument("--profile", help="AWS profile")
    parser.add_argument("--region", default=DEFAULT_REGION)
    parser.add_argument("--log-group", default=DEFAULT_LOG_GROUP)
    parser.add_argument("--timeout", type=int, default=300, help="Query timeout in seconds")
    parser.add_argument("--json", action="store_true", help="Output raw JSON response")
    args = parser.parse_args()

    today = datetime.now(timezone.utc).date()

    try:
        if args.days is not None:
            if args.days < 1:
                parser.error("--days must be at least 1")
            end_date = today - timedelta(days=1)
            start_date = end_date - timedelta(days=args.days - 1)
        elif args.start:
            start_date = date.fromisoformat(args.start)
            end_date = date.fromisoformat(args.end) if args.end else start_date
            if end_date >= today:
                end_date = today - timedelta(days=1)
            if end_date < start_date:
                parser.error(f"end date {end_date} is before start date {start_date}")
        else:
            end_date = today - timedelta(days=1)
            start_date = end_date
    except ValueError as error:
        parser.error(str(error))

    start_epoch = int(datetime.combine(start_date, datetime_time.min, timezone.utc).timestamp())
    end_epoch = int(datetime.combine(end_date + timedelta(days=1), datetime_time.min, timezone.utc).timestamp())

    print(f"Window: {start_date} to {end_date}", file=sys.stderr)

    try:
        profile = choose_profile(args.profile, os.environ)
        verify_account(profile, args.region)
        response = run_query(profile, args.region, args.log_group, start_epoch, end_epoch, args.query, args.timeout)

        if args.json:
            print(json.dumps(response, indent=2))
        else:
            results = response.get("results", [])
            if not results:
                print("No results.")
                return
            headers = [item["field"] for item in results[0] if not item["field"].startswith("@ptr")]
            print("\t".join(headers))
            for row in results:
                fields = {item["field"]: item.get("value", "") for item in row}
                print("\t".join(fields.get(h, "") for h in headers))

        stats = response.get("statistics", {})
        if stats:
            scanned = stats.get("bytesScanned", 0)
            records = int(stats.get("recordsScanned", 0))
            print(f"\nScanned: {format_bytes(scanned)}, {records:,} records", file=sys.stderr)

    except RuntimeError as error:
        print(f"Error: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
