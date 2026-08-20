#!/usr/bin/env python3
"""Enforce the production-code jscpd clone count against a strict baseline."""

from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
import tempfile
from pathlib import Path
from typing import Sequence

from sa_lib import (
    EXIT_CLEAN,
    ErrorAccumulator,
    Measurement,
    Violation,
    emit_measurements,
    emit_results,
    load_production_code_perimeter,
    run_cli,
    write_measurements,
)


class DuplicationReportError(ValueError):
    """A jscpd report cannot provide a trustworthy clone list."""


def _parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare jscpd's filtered production clone count with its baseline."
    )
    parser.add_argument("--config", default=".jscpd.json")
    parser.add_argument("--baseline", default=".jscpd-baseline.json")
    parser.add_argument("--jscpd-cmd", default="bunx jscpd")
    parser.add_argument(
        "--report",
        help="consume a pre-produced jscpd JSON report instead of running jscpd",
    )
    parser.add_argument(
        "--measurements-out",
        help="write the gate readings as JSON so another gate's report can publish them",
    )
    return parser.parse_args(argv)


def _load_baseline(path: Path) -> int:
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise ValueError(f"cannot read baseline {path}: {error}") from error
    try:
        payload = json.loads(raw)
    except (json.JSONDecodeError, UnicodeError) as error:
        raise ValueError(f"cannot parse baseline {path} as JSON: {error}") from error

    if not isinstance(payload, dict):
        raise ValueError(f"baseline {path} must contain a JSON object")
    if set(payload) != {"clones"}:
        raise ValueError(
            f"baseline {path} must contain exactly the 'clones' field"
        )
    clone_count = payload["clones"]
    if isinstance(clone_count, bool) or not isinstance(clone_count, int):
        raise ValueError(f"baseline {path} clones must be a non-negative integer")
    if clone_count < 0:
        raise ValueError(f"baseline {path} clones must be non-negative")
    return clone_count


def _read_report_payload(path: Path) -> object:
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise DuplicationReportError(f"cannot read jscpd report {path}: {error}") from error
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, UnicodeError) as error:
        raise DuplicationReportError(
            f"cannot parse jscpd report {path} as JSON: {error}"
        ) from error


def _parse_report(payload: object) -> tuple[tuple[str, str], ...]:
    if not isinstance(payload, dict):
        raise DuplicationReportError("jscpd report must contain a JSON object")

    statistics = payload.get("statistics")
    if not isinstance(statistics, dict):
        raise DuplicationReportError("jscpd report is missing statistics")
    total = statistics.get("total")
    if not isinstance(total, dict):
        raise DuplicationReportError("jscpd report is missing statistics.total")
    reported_count = total.get("clones")
    if (
        isinstance(reported_count, bool)
        or not isinstance(reported_count, int)
        or reported_count < 0
    ):
        raise DuplicationReportError(
            "jscpd report statistics.total.clones must be a non-negative integer"
        )

    duplicates = payload.get("duplicates")
    if not isinstance(duplicates, list):
        raise DuplicationReportError("jscpd report duplicates must be an array")
    if len(duplicates) != reported_count:
        raise DuplicationReportError(
            "jscpd report clone-count mismatch: "
            f"statistics.total.clones={reported_count}, duplicates={len(duplicates)}"
        )

    clone_paths: list[tuple[str, str]] = []
    for index, duplicate in enumerate(duplicates):
        if not isinstance(duplicate, dict):
            raise DuplicationReportError(
                f"jscpd report duplicates[{index}] must be an object"
            )
        endpoints: list[str] = []
        for field_name in ("firstFile", "secondFile"):
            endpoint = duplicate.get(field_name)
            if not isinstance(endpoint, dict):
                raise DuplicationReportError(
                    f"jscpd report duplicates[{index}].{field_name} must be an object"
                )
            name = endpoint.get("name")
            if not isinstance(name, str) or not name.strip():
                raise DuplicationReportError(
                    f"jscpd report duplicates[{index}].{field_name}.name "
                    "must be a non-empty string"
                )
            endpoints.append(name)
        clone_paths.append((endpoints[0], endpoints[1]))
    return tuple(clone_paths)


def _stderr_text(stderr: str) -> str:
    return stderr.strip() or "<empty>"


def _run_jscpd(command_text: str, config_path: Path) -> tuple[tuple[str, str], ...]:
    try:
        command = shlex.split(command_text)
    except ValueError as error:
        raise DuplicationReportError(f"cannot parse jscpd command: {error}") from error
    if not command:
        raise DuplicationReportError("jscpd command cannot be empty")

    with tempfile.TemporaryDirectory(prefix="jscpd-report-") as temporary_directory:
        output_directory = Path(temporary_directory)
        argv = [
            *command,
            "--config",
            os.fspath(config_path),
            "--reporters",
            "json",
            "--output",
            os.fspath(output_directory),
        ]
        try:
            completed = subprocess.run(
                argv,
                text=True,
                capture_output=True,
                check=False,
            )
        except (OSError, UnicodeError, subprocess.SubprocessError) as error:
            raise DuplicationReportError(f"cannot execute jscpd: {error}") from error

        stderr = _stderr_text(completed.stderr)
        if completed.returncode != EXIT_CLEAN:
            raise DuplicationReportError(
                f"jscpd exited with code {completed.returncode}; stderr: {stderr}"
            )

        expected = output_directory / "jscpd-report.json"
        candidates = ([expected] if expected.is_file() else []) + [
            path
            for path in sorted(output_directory.rglob("*.json"))
            if path != expected
        ]
        if not candidates:
            raise DuplicationReportError(
                f"jscpd produced no JSON report; stderr: {stderr}"
            )

        failures: list[str] = []
        for candidate in candidates:
            try:
                return _parse_report(_read_report_payload(candidate))
            except DuplicationReportError as error:
                failures.append(f"{candidate.name}: {error}")
        raise DuplicationReportError(
            "jscpd produced no parseable clone report "
            f"({'; '.join(failures)}); stderr: {stderr}"
        )


def _load_injected_report(path: Path) -> tuple[tuple[str, str], ...]:
    return _parse_report(_read_report_payload(path))


def _validate_scan_roots(config_path: Path) -> None:
    """Check that jscpd's scan roots cover the perimeter it is meant to measure.

    jscpd walks `path` and only then applies `include`, so a root that misses an
    included tree reports zero clones there while the perimeter still claims it.
    """

    payload = json.loads(config_path.read_text(encoding="utf-8"))
    roots = payload.get("path")
    if not isinstance(roots, list) or not roots:
        raise DuplicationReportError(f"{config_path} requires a non-empty path list")
    normalised_roots = [str(root).strip("/") for root in roots if isinstance(root, str)]
    for pattern in payload.get("include", []):
        if not isinstance(pattern, str):
            continue
        static_prefix: list[str] = []
        for segment in pattern.strip("/").split("/"):
            if "*" in segment or "?" in segment or "[" in segment:
                break
            static_prefix.append(segment)
        prefix = "/".join(static_prefix)
        if not any(
            prefix == root or prefix.startswith(f"{root}/") or root.startswith(f"{prefix}/")
            for root in normalised_roots
        ):
            raise DuplicationReportError(
                f"{config_path} include pattern {pattern!r} lies outside every scan root, "
                "so jscpd would never look there"
            )


def main(argv: Sequence[str]) -> int:
    args = _parse_args(argv)
    config_path = Path(args.config)
    baseline_path = Path(args.baseline)
    errors = ErrorAccumulator()

    perimeter = None
    baseline = None
    try:
        perimeter = load_production_code_perimeter(config_path)
        if args.report is None:
            # Only jscpd reads `path`; an injected report was scanned elsewhere.
            _validate_scan_roots(config_path)
    except Exception as error:
        errors.add(error, context="invalid duplication config")
    try:
        baseline = _load_baseline(baseline_path)
    except Exception as error:
        errors.add(error, context="invalid duplication baseline")

    if errors.has_errors:
        return emit_results((), errors)

    assert perimeter is not None
    assert baseline is not None
    try:
        clone_paths = (
            _load_injected_report(Path(args.report))
            if args.report is not None
            else _run_jscpd(args.jscpd_cmd, config_path)
        )
    except Exception as error:
        errors.add(error, context="duplication measurement failed")
        return emit_results((), errors)

    filtered_count = sum(
        perimeter.matches(first_path) and perimeter.matches(second_path)
        for first_path, second_path in clone_paths
    )
    measurements = [
        Measurement(
            gate="duplication",
            subject="clones",
            measured=str(filtered_count),
            limit=str(baseline),
            headroom=str(baseline - filtered_count),
        )
    ]
    emit_measurements(measurements)
    if args.measurements_out:
        try:
            write_measurements(args.measurements_out, measurements)
        except OSError as error:
            errors.add(error, context="cannot write gate readings")
    violations: list[Violation] = []
    if filtered_count > baseline:
        violations.append(
            Violation(
                os.fspath(baseline_path),
                None,
                "duplication",
                f"measured clone count {filtered_count} exceeds baseline {baseline}",
            )
        )
    elif filtered_count < baseline:
        # Slack on a count is permission to add clones elsewhere, so a removed
        # clone has to be banked in the same change.
        violations.append(
            Violation(
                os.fspath(baseline_path),
                None,
                "duplication",
                f"measured clone count {filtered_count} is below baseline {baseline}; "
                f"lower the baseline to {filtered_count}",
            )
        )
    return emit_results(violations, errors)


if __name__ == "__main__":
    run_cli(main)
