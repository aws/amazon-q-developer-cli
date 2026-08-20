#!/usr/bin/env python3
"""Enforce per-package line and function coverage floors from lcov streams."""

from __future__ import annotations

import json
import math
import re
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath
from typing import Any, Sequence

from sa_lib import (
    EXIT_CLEAN,
    ErrorAccumulator,
    Measurement,
    Violation,
    emit_measurements,
    emit_results,
    read_measurements,
    run_cli,
)

REPORT_MARKER = "<!-- code-quality-coverage-report -->"
USAGE = """usage: check-coverage.py --floors FILE --coverage-config FILE \\
  --package NAME [--lcov PATH:FILTER_KEY ...] [--suite-result NAME=EXIT_CODE ...] \\
  [--package NAME ...] --report-out FILE"""


@dataclass(frozen=True, slots=True)
class LcovInput:
    path: str
    filter_key: str


@dataclass(frozen=True, slots=True)
class SuiteResult:
    name: str
    exit_code: int


@dataclass(slots=True)
class PackageInput:
    name: str
    lcov_inputs: list[LcovInput] = field(default_factory=list)
    suite_results: list[SuiteResult] = field(default_factory=list)


@dataclass(slots=True)
class CliInput:
    floors_path: str | None = None
    coverage_config_path: str | None = None
    report_out: str | None = None
    gate_readings: list[str] = field(default_factory=list)
    measurement_out: str | None = None
    compare_to: str | None = None
    packages: list[PackageInput] = field(default_factory=list)
    help_requested: bool = False


@dataclass(frozen=True, slots=True)
class PackageFloors:
    lines: float
    functions: float
    min_lines: int
    min_functions: int


@dataclass(frozen=True, slots=True)
class CoverageFilters:
    runner_patterns: dict[str, tuple[re.Pattern[str], ...]]
    exclude_directories: frozenset[str]
    exclude_files: frozenset[str]
    exclude_patterns: tuple[re.Pattern[str], ...]

    def excludes(self, source: str, filter_key: str) -> bool:
        normalised = _normalise_source(source)
        if any(pattern.search(normalised) for pattern in self.runner_patterns[filter_key]):
            return True
        parts = tuple(part for part in normalised.split("/") if part)
        if any(part in self.exclude_directories for part in parts[:-1]):
            return True
        if parts and parts[-1] in self.exclude_files:
            return True
        return any(pattern.search(normalised) for pattern in self.exclude_patterns)


@dataclass(slots=True)
class CoverageTotals:
    lines: dict[tuple[str, int], float] = field(default_factory=dict)
    # Keyed by declaring line: two same-named functions in one file must not
    # collapse into one entry that counts as covered when either runs.
    functions: dict[tuple[str, int], float] = field(default_factory=dict)
    # Per-file (found, hit) from FNF/FNH, for generators that count a file's
    # functions without naming them; otherwise such files add no denominator.
    function_summaries: dict[str, tuple[int, int]] = field(default_factory=dict)

    def merge(self, other: "CoverageTotals") -> None:
        for key, hits in other.lines.items():
            self.lines[key] = max(self.lines.get(key, 0.0), hits)
        for key, hits in other.functions.items():
            self.functions[key] = max(self.functions.get(key, 0.0), hits)
        for source, (found, hit) in other.function_summaries.items():
            existing = self.function_summaries.get(source)
            self.function_summaries[source] = (
                (found, hit)
                if existing is None
                else (max(existing[0], found), max(existing[1], hit))
            )

    def measure(self) -> "CoverageMeasurement":
        line_total = len(self.lines)
        line_hit = sum(hits > 0 for hits in self.lines.values())
        function_total = len(self.functions)
        function_hit = sum(hits > 0 for hits in self.functions.values())
        # A summary is consulted only for files no stream described function by
        # function, so exact identities are never traded for bare counts.
        exact_sources = {source for source, _line in self.functions}
        for source, (found, hit) in self.function_summaries.items():
            if source in exact_sources:
                continue
            function_total += found
            function_hit += hit
        return CoverageMeasurement(
            lines=100.0 * line_hit / line_total if line_total else 0.0,
            functions=100.0 * function_hit / function_total if function_total else 0.0,
            line_total=line_total,
            function_total=function_total,
        )


@dataclass(frozen=True, slots=True)
class CoverageMeasurement:
    lines: float
    functions: float
    line_total: int
    function_total: int


@dataclass(slots=True)
class Evaluation:
    floors: dict[str, PackageFloors] = field(default_factory=dict)
    floors_valid: bool = False
    measurements: dict[str, CoverageMeasurement] = field(default_factory=dict)
    violations: list[Violation] = field(default_factory=list)
    failing_packages: set[str] = field(default_factory=set)
    suite_failed_packages: set[str] = field(default_factory=set)


def _option_value(
    argv: Sequence[str],
    index: int,
    option: str,
    inline_value: str | None,
    errors: ErrorAccumulator,
) -> tuple[str | None, int]:
    if inline_value is not None:
        if inline_value:
            return inline_value, index + 1
        errors.add(f"{option} requires a non-empty value", context="command line")
        return None, index + 1
    if index + 1 >= len(argv) or argv[index + 1].startswith("--"):
        errors.add(f"{option} requires a value", context="command line")
        return None, index + 1
    return argv[index + 1], index + 2


def _parse_cli(argv: Sequence[str], errors: ErrorAccumulator) -> CliInput:
    parsed = CliInput()
    current_package: PackageInput | None = None
    package_by_name: dict[str, PackageInput] = {}
    globals_seen: set[str] = set()
    index = 0

    while index < len(argv):
        token = argv[index]
        if token in {"-h", "--help"}:
            parsed.help_requested = True
            index += 1
            continue

        option, separator, inline = token.partition("=")
        inline_value = inline if separator else None
        if option not in {
            "--floors",
            "--coverage-config",
            "--report-out",
            "--gate-readings",
            "--measurement-out",
            "--compare-to",
            "--package",
            "--lcov",
            "--suite-result",
        }:
            errors.add(f"unknown argument {token!r}", context="command line")
            index += 1
            continue

        value, index = _option_value(argv, index, option, inline_value, errors)
        if value is None:
            continue

        if option == "--gate-readings":
            if not value.strip():
                errors.add("--gate-readings requires a path", context="command line")
                continue
            parsed.gate_readings.append(value)
            continue

        if option in {
            "--floors",
            "--coverage-config",
            "--report-out",
            "--measurement-out",
            "--compare-to",
        }:
            if option in globals_seen:
                errors.add(f"{option} may be provided only once", context="command line")
                continue
            globals_seen.add(option)
            if option == "--floors":
                parsed.floors_path = value
            elif option == "--coverage-config":
                parsed.coverage_config_path = value
            elif option == "--measurement-out":
                parsed.measurement_out = value
            elif option == "--compare-to":
                parsed.compare_to = value
            else:
                parsed.report_out = value
            continue

        if option == "--package":
            if not value.strip():
                errors.add("--package requires a non-empty name", context="command line")
                current_package = None
                continue
            if value in package_by_name:
                errors.add(f"duplicate --package group {value!r}", context="command line")
                current_package = package_by_name[value]
                continue
            current_package = PackageInput(value)
            parsed.packages.append(current_package)
            package_by_name[value] = current_package
            continue

        if current_package is None:
            errors.add(f"{option} must follow a --package group", context="command line")
            continue

        if option == "--lcov":
            if ":" not in value:
                errors.add(
                    f"--lcov value {value!r} must use PATH:FILTER_KEY syntax",
                    context=f"package {current_package.name}",
                )
                continue
            path, filter_key = value.rsplit(":", 1)
            if not path or not filter_key:
                errors.add(
                    f"--lcov value {value!r} must contain a path and filter key",
                    context=f"package {current_package.name}",
                )
                continue
            current_package.lcov_inputs.append(LcovInput(path, filter_key))
            continue

        name, equals, raw_exit_code = value.partition("=")
        if not equals or not name or re.fullmatch(r"[0-9]+", raw_exit_code) is None:
            errors.add(
                f"--suite-result value {value!r} must use NAME=NON_NEGATIVE_EXIT_CODE syntax",
                context=f"package {current_package.name}",
            )
            continue
        if name in {suite.name for suite in current_package.suite_results}:
            errors.add(
                f"duplicate suite result {name!r}",
                context=f"package {current_package.name}",
            )
            continue
        current_package.suite_results.append(SuiteResult(name, int(raw_exit_code)))

    for option, value in (
        ("--floors", parsed.floors_path),
        ("--coverage-config", parsed.coverage_config_path),
        ("--report-out", parsed.report_out),
    ):
        if value is None:
            errors.add(f"missing required argument {option}", context="command line")
    if not parsed.packages:
        errors.add("no packages were provided; at least one --package is required", context="command line")
    return parsed


def _reject_json_constant(value: str) -> None:
    raise ValueError(f"non-finite JSON number {value!r} is not permitted")


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key {key!r}")
        result[key] = value
    return result


def _load_json(path_text: str, label: str, errors: ErrorAccumulator) -> Any | None:
    path = Path(path_text)
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        errors.add(error, context=f"cannot read {label} {path}")
        return None
    try:
        return json.loads(
            raw,
            parse_constant=_reject_json_constant,
            object_pairs_hook=_reject_duplicate_keys,
        )
    except (json.JSONDecodeError, UnicodeError, ValueError) as error:
        errors.add(error, context=f"cannot parse {label} {path}")
        return None


def _is_nonnegative_number(value: object) -> bool:
    return (
        not isinstance(value, bool)
        and isinstance(value, (int, float))
        and math.isfinite(float(value))
        and value >= 0
    )


def _load_floors(
    path: str | None, errors: ErrorAccumulator
) -> tuple[dict[str, PackageFloors], bool]:
    if path is None:
        return {}, False
    error_count = len(errors)
    payload = _load_json(path, "floors file", errors)
    if payload is None:
        return {}, False
    if not isinstance(payload, dict):
        errors.add("top level must be a JSON object", context=f"floors file {path}")
        return {}, False

    floors: dict[str, PackageFloors] = {}
    for package, values in payload.items():
        if not isinstance(package, str) or not package.strip():
            errors.add("package names must be non-empty strings", context=f"floors file {path}")
            continue
        if not isinstance(values, dict):
            errors.add(
                "entry must be an object containing exactly 'lines' and 'functions'",
                context=f"floors file {path} package {package!r}",
            )
            continue
        keys = set(values)
        if keys != {"lines", "functions", "measured"}:
            errors.add(
                "entry must contain exactly 'lines', 'functions' and 'measured'",
                context=f"floors file {path} package {package!r}",
            )
            continue
        invalid_fields = [
            field_name
            for field_name in ("lines", "functions")
            if not _is_nonnegative_number(values[field_name])
        ]
        if invalid_fields:
            for field_name in invalid_fields:
                errors.add(
                    f"{field_name} must be a non-negative finite number",
                    context=f"floors file {path} package {package!r}",
                )
            continue
        measured = values["measured"]
        if not isinstance(measured, dict) or set(measured) != {"lines", "functions"}:
            errors.add(
                "measured must be an object containing exactly 'lines' and 'functions'",
                context=f"floors file {path} package {package!r}",
            )
            continue
        invalid_totals = [
            field_name
            for field_name in ("lines", "functions")
            if not isinstance(measured[field_name], int)
            or isinstance(measured[field_name], bool)
            or measured[field_name] < 0
        ]
        if invalid_totals:
            for field_name in invalid_totals:
                errors.add(
                    f"measured.{field_name} must be a non-negative integer",
                    context=f"floors file {path} package {package!r}",
                )
            continue
        floors[package] = PackageFloors(
            lines=float(values["lines"]),
            functions=float(values["functions"]),
            min_lines=int(measured["lines"]),
            min_functions=int(measured["functions"]),
        )
    return floors, len(errors) == error_count


def _extract_config_values(
    payload: dict[str, Any],
    field_name: str,
    item_key: str,
    path: str,
    errors: ErrorAccumulator,
) -> tuple[str, ...]:
    raw_values = payload.get(field_name)
    if not isinstance(raw_values, list):
        errors.add(f"{field_name} must be a list", context=f"coverage config {path}")
        return ()
    values: list[str] = []
    for index, item in enumerate(raw_values):
        value = item if isinstance(item, str) else item.get(item_key) if isinstance(item, dict) else None
        if not isinstance(value, str) or not value:
            errors.add(
                f"{field_name}[{index}] must provide a non-empty {item_key!r} string",
                context=f"coverage config {path}",
            )
            continue
        values.append(value)
    return tuple(values)


def _compile_patterns(
    patterns: Sequence[str], field_name: str, path: str, errors: ErrorAccumulator
) -> tuple[re.Pattern[str], ...]:
    compiled: list[re.Pattern[str]] = []
    for index, pattern in enumerate(patterns):
        if not isinstance(pattern, str) or not pattern:
            errors.add(
                f"{field_name}[{index}] must be a non-empty regex string",
                context=f"coverage config {path}",
            )
            continue
        try:
            compiled.append(re.compile(pattern))
        except re.error as error:
            errors.add(error, context=f"coverage config {path} {field_name}[{index}]")
    return tuple(compiled)


def _load_coverage_filters(
    path: str | None, errors: ErrorAccumulator
) -> tuple[CoverageFilters | None, bool]:
    if path is None:
        return None, False
    error_count = len(errors)
    payload = _load_json(path, "coverage config", errors)
    if payload is None:
        return None, False
    if not isinstance(payload, dict):
        errors.add("top level must be a JSON object", context=f"coverage config {path}")
        return None, False

    directories = _extract_config_values(
        payload, "excludeDirectories", "name", path, errors
    )
    files = _extract_config_values(payload, "excludeFiles", "name", path, errors)
    global_pattern_values = _extract_config_values(
        payload, "excludePatterns", "pattern", path, errors
    )

    raw_runner_filters = payload.get("lcovFilters")
    runner_patterns: dict[str, tuple[re.Pattern[str], ...]] = {}
    if not isinstance(raw_runner_filters, dict):
        errors.add("lcovFilters must be an object", context=f"coverage config {path}")
    else:
        for key, patterns in raw_runner_filters.items():
            if not isinstance(key, str) or not key:
                errors.add("lcovFilters keys must be non-empty strings", context=f"coverage config {path}")
                continue
            if not isinstance(patterns, list):
                errors.add(f"lcovFilters.{key} must be a list", context=f"coverage config {path}")
                continue
            runner_patterns[key] = _compile_patterns(
                patterns, f"lcovFilters.{key}", path, errors
            )

    filters = CoverageFilters(
        runner_patterns=runner_patterns,
        exclude_directories=frozenset(directories),
        exclude_files=frozenset(files),
        exclude_patterns=_compile_patterns(
            global_pattern_values, "excludePatterns", path, errors
        ),
    )
    return filters, len(errors) == error_count


def _normalise_source(source: str) -> str:
    normalised = source.replace("\\", "/")
    while normalised.startswith("./"):
        normalised = normalised[2:]
    return PurePosixPath(normalised).as_posix()


def _parse_positive_int(raw: str, what: str) -> int:
    if re.fullmatch(r"[0-9]+", raw) is None:
        raise ValueError(f"{what} must be a positive integer, got {raw!r}")
    value = int(raw)
    if value < 1:
        raise ValueError(f"{what} must be a positive integer, got {raw!r}")
    return value


def _parse_positive_int_or_zero(raw: str, what: str) -> int:
    if re.fullmatch(r"[0-9]+", raw) is None:
        raise ValueError(f"{what} must be a non-negative integer, got {raw!r}")
    return int(raw)


def _parse_hits(raw: str, what: str) -> float:
    try:
        value = float(raw)
    except ValueError as error:
        raise ValueError(f"{what} must be a non-negative number, got {raw!r}") from error
    if not math.isfinite(value) or value < 0:
        raise ValueError(f"{what} must be a non-negative finite number, got {raw!r}")
    return value


def _parse_lcov(
    text: str,
    input_path: str,
    filter_key: str,
    filters: CoverageFilters,
    errors: ErrorAccumulator,
) -> CoverageTotals:
    totals = CoverageTotals()
    current_source: str | None = None
    excluded = False
    # (source, function name) -> declaring line numbers seen in FN records, used to
    # attribute each FNDA hit to the line(s) that declare that name.
    function_lines: dict[tuple[str, str], list[int]] = {}
    # (source, function name) -> how many FNDA records have been consumed for it.
    function_cursors: dict[tuple[str, str], int] = {}
    # FNF/FNH are section totals, so they are held until the section closes.
    section_found: int | None = None
    section_hit: int | None = None
    section_summary_line = 0

    def record_error(line_number: int, message: str | BaseException) -> None:
        errors.add(message, context=f"lcov input {input_path}:{line_number}")

    def close_section(source: str) -> None:
        nonlocal section_found, section_hit
        if section_found is None and section_hit is None:
            return
        if section_found is None:
            record_error(
                section_summary_line, "FNH record has no corresponding FNF record"
            )
        elif (section_hit or 0) > section_found:
            record_error(
                section_summary_line,
                f"FNH {section_hit} exceeds FNF {section_found}",
            )
        else:
            existing = totals.function_summaries.get(source)
            found, hit = section_found, section_hit or 0
            totals.function_summaries[source] = (
                (found, hit)
                if existing is None
                else (max(existing[0], found), max(existing[1], hit))
            )
        section_found = section_hit = None

    for line_number, raw_line in enumerate(text.splitlines(), 1):
        line = raw_line.strip()
        if line.startswith("SF:"):
            if current_source is not None:
                record_error(line_number, "encountered SF before end_of_record")
                close_section(current_source)
            source = line[3:]
            if not source:
                record_error(line_number, "SF record has an empty source path")
                current_source = None
                excluded = False
                continue
            current_source = _normalise_source(source)
            excluded = filters.excludes(current_source, filter_key)
            continue

        if line == "end_of_record":
            if current_source is None:
                record_error(line_number, "end_of_record has no preceding SF record")
            else:
                close_section(current_source)
            current_source = None
            excluded = False
            # FNDA pairing restarts per section: a cursor left running pairs a
            # later section's hit with the wrong declaration, falsely covering it.
            function_cursors.clear()
            continue

        record_kind = next(
            (
                prefix
                for prefix in ("DA:", "FNDA:", "FNF:", "FNH:", "FN:")
                if line.startswith(prefix)
            ),
            None,
        )
        if record_kind is None:
            continue
        if current_source is None:
            record_error(line_number, f"{record_kind[:-1]} record has no preceding SF record")
            continue
        if excluded:
            continue

        payload = line[len(record_kind) :]
        try:
            if record_kind == "DA:":
                parts = payload.split(",")
                if len(parts) < 2:
                    raise ValueError("DA record must contain line and hit count")
                source_line = _parse_positive_int(parts[0], "DA line number")
                hits = _parse_hits(parts[1], "DA hit count")
                key = (current_source, source_line)
                totals.lines[key] = max(totals.lines.get(key, 0.0), hits)
            elif record_kind == "FN:":
                source_line_raw, comma, name = payload.partition(",")
                if not comma or not name:
                    raise ValueError("FN record must contain line and function name")
                source_line = _parse_positive_int(source_line_raw, "FN line number")
                totals.functions.setdefault((current_source, source_line), 0.0)
                # Appended per FN record, never deduped: the FNDA cursor consumes one
                # slot per record, and deduping desynchronises them on repeated sections.
                declared = function_lines.setdefault((current_source, name), [])
                declared.append(source_line)
            elif record_kind in {"FNF:", "FNH:"}:
                count = _parse_positive_int_or_zero(payload, f"{record_kind[:-1]} count")
                section_summary_line = line_number
                if record_kind == "FNF:":
                    if section_found is not None:
                        raise ValueError("section repeats its FNF record")
                    section_found = count
                else:
                    if section_hit is not None:
                        raise ValueError("section repeats its FNH record")
                    section_hit = count
            else:
                hits_raw, comma, name = payload.partition(",")
                if not comma or not name:
                    raise ValueError("FNDA record must contain hit count and function name")
                hits = _parse_hits(hits_raw, "FNDA hit count")
                declared = function_lines.get((current_source, name))
                if not declared:
                    # Without the declaring line the hit cannot be keyed, and
                    # dropping it would understate coverage, so fail closed.
                    raise ValueError(
                        f"FNDA record for {name!r} has no preceding FN record"
                    )
                # FNDA carries only a name, so a duplicated name resolves
                # positionally: the Nth hit belongs to the Nth declaration.
                cursor_key = (current_source, name)
                cursor = function_cursors.get(cursor_key, 0)
                if cursor >= len(declared):
                    raise ValueError(
                        f"FNDA records for {name!r} outnumber its FN records"
                    )
                function_cursors[cursor_key] = cursor + 1
                key = (current_source, declared[cursor])
                totals.functions[key] = max(totals.functions.get(key, 0.0), hits)
        except ValueError as error:
            record_error(line_number, error)

    if current_source is not None:
        record_error(max(1, len(text.splitlines())), "record is missing end_of_record")
        close_section(current_source)
    return totals


def _validate_package_sets(
    packages: Sequence[PackageInput],
    floors: dict[str, PackageFloors],
    floors_valid: bool,
    errors: ErrorAccumulator,
) -> None:
    if not floors_valid:
        return
    cli_names = {package.name for package in packages}
    floor_names = set(floors)
    missing_cli = sorted(floor_names - cli_names)
    missing_floors = sorted(cli_names - floor_names)
    if missing_cli:
        errors.add(
            f"floors packages missing CLI --package groups: {', '.join(missing_cli)}",
            context="package set mismatch",
        )
    if missing_floors:
        errors.add(
            f"CLI packages missing floors entries: {', '.join(missing_floors)}",
            context="package set mismatch",
        )


def _evaluate(cli: CliInput, errors: ErrorAccumulator) -> Evaluation:
    evaluation = Evaluation()
    evaluation.floors, evaluation.floors_valid = _load_floors(cli.floors_path, errors)
    filters, filters_valid = _load_coverage_filters(cli.coverage_config_path, errors)
    _validate_package_sets(cli.packages, evaluation.floors, evaluation.floors_valid, errors)

    for package in cli.packages:
        # Without a stream there is nothing to measure, so every floor is satisfied
        # and the package certifies itself. Absent coverage must not read as met.
        if not package.lcov_inputs:
            errors.add(
                "package declares no lcov streams, so its floors measure nothing",
                context=f"package {package.name}",
            )

    totals_by_package = {package.name: CoverageTotals() for package in cli.packages}
    if filters is not None and filters_valid:
        unknown_filter_keys = sorted(
            {
                stream.filter_key
                for package in cli.packages
                for stream in package.lcov_inputs
                if stream.filter_key not in filters.runner_patterns
            }
        )
        for key in unknown_filter_keys:
            errors.add(
                f"declared lcov filter key {key!r} is absent from lcovFilters",
                context=f"coverage config {cli.coverage_config_path}",
            )

        for package in cli.packages:
            suites_succeeded = all(result.exit_code == 0 for result in package.suite_results)
            for stream in package.lcov_inputs:
                if stream.filter_key not in filters.runner_patterns:
                    continue
                try:
                    text = Path(stream.path).read_text(encoding="utf-8")
                except (OSError, UnicodeError) as error:
                    if suites_succeeded:
                        errors.add(
                            error,
                            context=f"cannot read declared lcov input {stream.path} for package {package.name}",
                        )
                    continue
                parsed = _parse_lcov(
                    text,
                    stream.path,
                    stream.filter_key,
                    filters,
                    errors,
                )
                # Streams are merged, so an emptied one silently hands the figure
                # to the narrower survivors; absent data must not read as coverage.
                if suites_succeeded:
                    if not text.strip():
                        errors.add(
                            f"declared lcov input {stream.path} is empty",
                            context=f"package {package.name}",
                        )
                    elif not parsed.lines and not parsed.functions and not parsed.function_summaries:
                        errors.add(
                            f"declared lcov input {stream.path} yielded no records "
                            "inside the measured perimeter",
                            context=f"package {package.name}",
                        )
                totals_by_package[package.name].merge(parsed)

    evaluation.measurements = {
        package.name: totals_by_package[package.name].measure() for package in cli.packages
    }

    for package in cli.packages:
        for suite in package.suite_results:
            if suite.exit_code != 0:
                evaluation.suite_failed_packages.add(package.name)
                evaluation.violations.append(
                    Violation(
                        package.name,
                        None,
                        "suite-failure",
                        f"{suite.name} exited {suite.exit_code}",
                    )
                )

    if evaluation.floors_valid:
        for package in cli.packages:
            floors = evaluation.floors.get(package.name)
            if floors is None:
                continue
            measured = evaluation.measurements[package.name]
            # A percentage rises when measured code disappears, so the denominator
            # is floored too; a committed zero leaves the metric unfloored.
            for metric, measured_total, minimum in (
                ("lines", measured.line_total, floors.min_lines),
                ("functions", measured.function_total, floors.min_functions),
            ):
                if measured_total < minimum:
                    evaluation.failing_packages.add(package.name)
                    evaluation.violations.append(
                        Violation(
                            package.name,
                            None,
                            "coverage-denominator",
                            f"{metric} denominator {measured_total} is below the "
                            f"committed {minimum}, so the measured set shrank",
                        )
                    )
            for metric, measured_value, floor_value in (
                ("lines", measured.lines, floors.lines),
                ("functions", measured.functions, floors.functions),
            ):
                if measured_value < floor_value:
                    evaluation.failing_packages.add(package.name)
                    evaluation.violations.append(
                        Violation(
                            package.name,
                            None,
                            "coverage-floor",
                            f"{metric} {measured_value:.1f}% is below floor {floor_value:.1f}%",
                        )
                    )
    return evaluation


def _escape_table(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ")


def _denominator_cell(evaluation: Evaluation, package: str, metric: str) -> str:
    """Render a measured denominator beside its committed minimum, when one exists."""

    measured = evaluation.measurements[package]
    total = measured.line_total if metric == "lines" else measured.function_total
    floors = evaluation.floors.get(package)
    minimum = None
    if floors is not None:
        minimum = floors.min_lines if metric == "lines" else floors.min_functions
    if not minimum:
        return str(total)
    marker = "" if total >= minimum else " ❌"
    return f"{total} (min {minimum}){marker}"


def _code_path(value: str | None) -> str:
    return (value if value is not None else "<missing>").replace("`", "\\`")


def _coverage_cells(measured: CoverageMeasurement) -> tuple[str, str]:
    if measured.line_total == 0 and measured.function_total == 0:
        return "— (no measurable coverage data)", "—"
    lines = (
        f"{measured.lines:.1f}%"
        if measured.line_total
        else "— (no measurable coverage data)"
    )
    functions = (
        f"{measured.functions:.1f}%"
        if measured.function_total
        else "— (no measurable coverage data)"
    )
    return lines, functions


def _headroom_cell(measured: float, floor: float | None, total: int) -> str:
    if floor is None or total == 0:
        return "—"
    return f"{measured - floor:+.1f}"


def _write_measurement(
    path: str | None, evaluation: Evaluation, errors: ErrorAccumulator
) -> None:
    """Persist measured percentages so another run can be compared against them.

    Written whatever the verdict: a base-revision run exists only to be measured,
    and its floors are the head revision's, so it may legitimately fail them.
    """

    if path is None:
        return
    payload = {
        package: {
            "lines": measured.lines,
            "functions": measured.functions,
            "line_total": measured.line_total,
            "function_total": measured.function_total,
        }
        for package, measured in evaluation.measurements.items()
    }
    try:
        Path(path).write_text(json.dumps(payload), encoding="utf-8")
    except OSError as error:
        errors.add(error, context=f"cannot write measurement {path}")


def _read_comparison(
    path: str | None, errors: ErrorAccumulator
) -> dict[str, CoverageMeasurement] | None:
    """Load a prior run's percentages, or None when there is nothing to compare."""

    if path is None:
        return None
    try:
        payload = json.loads(Path(path).read_text(encoding="utf-8"))
    except FileNotFoundError:
        return None
    except (OSError, UnicodeError, ValueError) as error:
        errors.add(error, context=f"cannot read comparison {path}")
        return None
    if not isinstance(payload, dict):
        errors.add(f"comparison {path} must be a JSON object", context="comparison")
        return None
    if not payload:
        # A base run that measured nothing is not a comparison; saying so beats
        # marking every package "new".
        return None
    comparison: dict[str, CoverageMeasurement] = {}
    for package, entry in payload.items():
        if not isinstance(entry, dict):
            errors.add(f"comparison entry for {package} must be an object", context="comparison")
            continue
        try:
            comparison[package] = CoverageMeasurement(
                float(entry["lines"]),
                float(entry["functions"]),
                int(entry["line_total"]),
                int(entry["function_total"]),
            )
        except (KeyError, TypeError, ValueError) as error:
            errors.add(error, context=f"comparison entry for {package}")
    return comparison


def _comparison_cell(
    measured: CoverageMeasurement,
    baseline: CoverageMeasurement | None,
    metric: str,
) -> str:
    """Render movement against the base revision, not against the floor."""

    if baseline is None:
        return "new"
    total = measured.line_total if metric == "lines" else measured.function_total
    base_total = baseline.line_total if metric == "lines" else baseline.function_total
    if not total or not base_total:
        return "—"
    current = measured.lines if metric == "lines" else measured.functions
    previous = baseline.lines if metric == "lines" else baseline.functions
    return f"{current - previous:+.1f}"


def _render_gate_readings(
    paths: Sequence[str], errors: ErrorAccumulator
) -> list[str]:
    """Publish the other gates' readings so the comment audits all of them.

    A reading printed only to a job log leaves a reviewer unable to see a gate's
    margin without opening the workflow run.
    """

    if not paths:
        return []
    readings: list[Measurement] = []
    missing: list[str] = []
    for path in paths:
        try:
            readings.extend(read_measurements(path))
        except FileNotFoundError:
            missing.append(_code_path(path))
        except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as error:
            errors.add(error, context=f"cannot read gate readings {path}")
    lines = [""]
    if readings:
        lines.append("Other gates: " + " · ".join(
            f"{item.gate} {item.subject} {item.measured}/{item.limit}"
            for item in readings
        ))
    if missing:
        lines.append(
            "Readings not reported by: " + ", ".join(f"`{name}`" for name in missing)
        )
    return lines


def _render_report(
    cli: CliInput,
    evaluation: Evaluation,
    errors: ErrorAccumulator,
    comparison: dict[str, CoverageMeasurement] | None = None,
) -> str:
    ordered_packages = [package.name for package in cli.packages]
    ordered_packages.extend(
        package for package in sorted(evaluation.floors) if package not in ordered_packages
    )

    lines = [
        REPORT_MARKER,
        "## Code Quality — Coverage Report",
        "",
    ]
    if comparison is None:
        lines.extend(
            [
                "| Package | Lines | Floor | Δ | Functions | Floor | Δ | Status |",
                "|---|---|---|---|---|---|---|---|",
            ]
        )
    else:
        lines.extend(
            [
                "| Package | Lines | Floor | Δ floor | Δ base | Functions | Floor "
                "| Δ floor | Δ base | Status |",
                "|---|---|---|---|---|---|---|---|---|---|",
            ]
        )
    for package in ordered_packages:
        measured = evaluation.measurements.get(
            package, CoverageMeasurement(0.0, 0.0, 0, 0)
        )
        line_cell, function_cell = _coverage_cells(measured)
        floors = evaluation.floors.get(package)
        line_floor = f"{floors.lines:.1f}%" if floors is not None else "—"
        function_floor = f"{floors.functions:.1f}%" if floors is not None else "—"
        failed = (
            errors.has_errors
            or package in evaluation.failing_packages
            or package in evaluation.suite_failed_packages
            or floors is None
        )
        cells = [
            _escape_table(package),
            line_cell,
            line_floor,
            _headroom_cell(
                measured.lines,
                None if floors is None else floors.lines,
                measured.line_total,
            ),
        ]
        if comparison is not None:
            cells.append(_comparison_cell(measured, comparison.get(package), "lines"))
        cells.extend(
            (
                function_cell,
                function_floor,
                _headroom_cell(
                    measured.functions,
                    None if floors is None else floors.functions,
                    measured.function_total,
                ),
            )
        )
        if comparison is not None:
            cells.append(_comparison_cell(measured, comparison.get(package), "functions"))
        cells.append("❌" if failed else "✅")
        lines.append("| " + " | ".join(cells) + " |")
    lines.append("")
    lines.append(
        "Denominators: "
        + " · ".join(
            f"{package} {_denominator_cell(evaluation, package, 'lines')} lines,"
            f" {_denominator_cell(evaluation, package, 'functions')} functions"
            for package in ordered_packages
            if package in evaluation.measurements
        )
    )

    suites = [
        suite
        for package in cli.packages
        for suite in package.suite_results
    ]
    if suites:
        suite_text = " · ".join(
            f"{suite.name} ✅"
            if suite.exit_code == 0
            else f"{suite.name} ❌ (exit {suite.exit_code})"
            for suite in suites
        )
    else:
        suite_text = "none declared"
    lines.extend(
        [
            "",
            f"Suites: {suite_text}",
        ]
    )
    if cli.compare_to is not None and comparison is None:
        lines.append(
            f"Base-revision comparison unavailable (`{_code_path(cli.compare_to)}` "
            "was not produced), so movement against the base is not shown."
        )
    lines.extend(_render_gate_readings(cli.gate_readings, errors))
    lines.extend(
        [
            "Floors and denominators are committed in "
            f"`{_code_path(cli.floors_path)}`; Baseline_Direction fails any "
            "loosening of either that is not recorded in `.baseline-loosening.json`. "
            f"Exclusions: `{_code_path(cli.coverage_config_path)}`.",
        ]
    )
    if errors.has_errors:
        lines.extend(["", "Operational errors:"])
        lines.extend(f"- {error.replace(chr(10), ' ')}" for error in errors.errors)
    return "\n".join(lines) + "\n"


def _write_report(path_text: str | None, report: str, errors: ErrorAccumulator) -> None:
    if path_text is None:
        return
    path = Path(path_text)
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(report, encoding="utf-8")
    except (OSError, UnicodeError) as error:
        errors.add(error, context=f"cannot write coverage report {path}")


def _coverage_measurements(
    cli: CliInput, evaluation: Evaluation
) -> list[Measurement]:
    readings: list[Measurement] = []
    for package in cli.packages:
        floors = evaluation.floors.get(package.name)
        measured = evaluation.measurements.get(package.name)
        if floors is None or measured is None:
            continue
        for metric, value, floor in (
            ("lines", measured.lines, floors.lines),
            ("functions", measured.functions, floors.functions),
        ):
            readings.append(
                Measurement(
                    gate="coverage",
                    subject=f"{package.name} {metric}",
                    measured=f"{value:.1f}%",
                    limit=f"{floor:.1f}%",
                    headroom=f"{value - floor:+.1f}",
                )
            )
        # Published alongside the percentages because a percentage cannot show
        # that the set it was computed over shrank.
        for metric, total, minimum in (
            ("lines", measured.line_total, floors.min_lines),
            ("functions", measured.function_total, floors.min_functions),
        ):
            if not minimum:
                continue
            readings.append(
                Measurement(
                    gate="coverage",
                    subject=f"{package.name} {metric} denominator",
                    measured=str(total),
                    limit=str(minimum),
                    headroom=f"{total - minimum:+d}",
                )
            )
    return readings


def main(argv: Sequence[str]) -> int:
    errors = ErrorAccumulator()
    cli = _parse_cli(argv, errors)
    if cli.help_requested:
        print(USAGE)
        return EXIT_CLEAN

    evaluation = Evaluation()
    try:
        evaluation = _evaluate(cli, errors)
    except Exception as error:
        errors.add(error, context="coverage checker internal failure")

    _write_measurement(cli.measurement_out, evaluation, errors)
    comparison = _read_comparison(cli.compare_to, errors)
    try:
        report = _render_report(cli, evaluation, errors, comparison)
    except Exception as error:
        errors.add(error, context="coverage report rendering failure")
        report = (
            f"{REPORT_MARKER}\n## Code Quality — Coverage Report\n\n"
            "Operational errors:\n"
            + "\n".join(f"- {item}" for item in errors.errors)
            + "\n"
        )
    _write_report(cli.report_out, report, errors)

    emit_measurements(_coverage_measurements(cli, evaluation))
    return emit_results(evaluation.violations, errors)


if __name__ == "__main__":
    run_cli(main)
