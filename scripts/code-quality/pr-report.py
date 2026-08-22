#!/usr/bin/env python3
"""Build independently updateable sections in the shared PR quality report."""

from __future__ import annotations

import argparse
import html
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence

REPORT_MARKER = "<!-- code-quality-coverage-report -->"
SECTION_MARKER = re.compile(
    r"<!-- quality-metrics-section:([a-z0-9-]+):(start|end) -->"
)
SECTION_ORDER = ("code-quality", "acp-integration", "visual-stories")
MAX_REPORT_BYTES = 60_000
MAX_RENDERED_CAPABILITY_GAPS = 100
MAX_CAPABILITY_GAP_DETAIL_BYTES = 20_000


@dataclass(frozen=True)
class CoverageMetric:
    label: str
    covered: int
    total: int

    @property
    def percent(self) -> float:
        return 100.0 * self.covered / self.total if self.total else 0.0


@dataclass(frozen=True)
class CapabilityGap:
    id: str
    area: str
    description: str


@dataclass(frozen=True)
class CapabilityCoverage:
    description: str
    denominator: str
    covered: int
    total: int
    areas: tuple[CoverageMetric, ...]
    uncovered: tuple[CapabilityGap, ...]

    @property
    def percent(self) -> float:
        return 100.0 * self.covered / self.total if self.total else 0.0


def _section_marker(section: str, edge: str) -> str:
    if re.fullmatch(r"[a-z0-9-]+", section) is None:
        raise ValueError(f"invalid report section name {section!r}")
    return f"<!-- quality-metrics-section:{section}:{edge} -->"


def _without_report_marker(content: str) -> str:
    return "\n".join(
        line for line in content.splitlines() if line.strip() != REPORT_MARKER
    ).strip()


def _section_ranges(content: str) -> dict[str, tuple[int, int]]:
    ranges: dict[str, tuple[int, int]] = {}
    open_section: tuple[str, int] | None = None
    for match in SECTION_MARKER.finditer(content):
        section, edge = match.groups()
        if edge == "start":
            if open_section is not None:
                raise ValueError("quality report sections cannot be nested")
            if section in ranges:
                raise ValueError(f"duplicate quality report section {section!r}")
            open_section = (section, match.start())
            continue
        if open_section is None or open_section[0] != section:
            raise ValueError(f"unmatched quality report section end for {section!r}")
        ranges[section] = (open_section[1], match.end())
        open_section = None
    if open_section is not None:
        raise ValueError(f"unclosed quality report section {open_section[0]!r}")
    return ranges


def _wrap_section(section: str, content: str) -> str:
    return (
        f"{_section_marker(section, 'start')}\n"
        f"{content.strip()}\n"
        f"{_section_marker(section, 'end')}"
    )


def merge_report_section(current: str, section: str, content: str) -> str:
    """Replace one section while preserving every independently owned section."""

    clean_content = _without_report_marker(content)
    if not clean_content:
        raise ValueError("quality report section content cannot be empty")
    if SECTION_MARKER.search(clean_content):
        raise ValueError("quality report section content cannot contain control markers")

    clean_current = _without_report_marker(current)
    ranges = _section_ranges(clean_current)
    replacement = _wrap_section(section, clean_content)

    if section in ranges:
        start, end = ranges[section]
        merged = clean_current[:start] + replacement + clean_current[end:]
    elif ranges:
        if section in SECTION_ORDER:
            following = next(
                (
                    ranges[name][0]
                    for name in SECTION_ORDER[SECTION_ORDER.index(section) + 1 :]
                    if name in ranges
                ),
                None,
            )
        else:
            following = None
        if following is None:
            merged = f"{clean_current.rstrip()}\n\n{replacement}"
        else:
            merged = (
                f"{clean_current[:following].rstrip()}\n\n"
                f"{replacement}\n\n{clean_current[following:].lstrip()}"
            )
    elif clean_current and section != "code-quality":
        merged = (
            f"{_wrap_section('code-quality', clean_current)}\n\n{replacement}"
        )
    else:
        merged = replacement

    _section_ranges(merged)
    report = f"{REPORT_MARKER}\n\n{merged.strip()}\n"
    report_size = len(report.encode("utf-8"))
    if report_size > MAX_REPORT_BYTES:
        raise ValueError(
            f"merged quality report is {report_size} bytes; "
            f"limit is {MAX_REPORT_BYTES}"
        )
    return report


def _read_count(payload: dict[str, Any], field: str, source: Path) -> int:
    value = payload.get(field)
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ValueError(f"{source}: {field} must be a non-negative integer")
    return value


def load_visual_metrics(path: Path) -> tuple[CoverageMetric, ...]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read visual coverage {path}: {error}") from error
    if not isinstance(payload, dict):
        raise ValueError(f"{path}: visual coverage must be a JSON object")

    fields = (
        ("Story coverage", "coveredStories", "totalStories"),
        ("Variant execution", "executedVariants", "totalVariants"),
        ("Semantic variants", "verifiedVariants", "totalVariants"),
        ("Declared visual states", "coveredVisualStates", "totalVisualStates"),
        ("Component reachability", "coveredComponents", "totalComponents"),
    )
    metrics: list[CoverageMetric] = []
    for label, covered_field, total_field in fields:
        covered = _read_count(payload, covered_field, path)
        total = _read_count(payload, total_field, path)
        if covered > total:
            raise ValueError(
                f"{path}: {covered_field} cannot exceed {total_field}"
            )
        metrics.append(CoverageMetric(label, covered, total))
    return tuple(metrics)


def _read_text(
    payload: dict[str, Any],
    field: str,
    source: Path,
    *,
    max_length: int,
) -> str:
    value = payload.get(field)
    if not isinstance(value, str) or not value:
        raise ValueError(f"{source}: {field} must be a non-empty string")
    if len(value) > max_length:
        raise ValueError(f"{source}: {field} exceeds {max_length} characters")
    if any(ord(character) < 32 or ord(character) == 127 for character in value):
        raise ValueError(f"{source}: {field} contains control characters")
    return value


def _read_identifier(payload: dict[str, Any], field: str, source: Path) -> str:
    value = _read_text(payload, field, source, max_length=128)
    if re.fullmatch(r"[A-Za-z0-9._-]+", value) is None:
        raise ValueError(f"{source}: {field} must be a stable identifier")
    return value


def _escape_markdown(value: str) -> str:
    escaped = html.escape(value, quote=False).replace("\\", "\\\\")
    for character in ("`", "*", "_", "[", "]", "|"):
        escaped = escaped.replace(character, f"\\{character}")
    return escaped.replace("@", "&#64;")


def load_capability_coverage(path: Path) -> CapabilityCoverage:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read capability coverage {path}: {error}") from error
    if not isinstance(payload, dict) or payload.get("schemaVersion") != 1:
        raise ValueError(f"{path}: capability coverage must use schema version 1")
    if payload.get("suite") != "acp-integration":
        raise ValueError(f"{path}: unexpected capability coverage suite")

    description = _read_text(payload, "description", path, max_length=300)
    denominator = _read_text(payload, "denominator", path, max_length=1000)
    covered = _read_count(payload, "covered", path)
    total = _read_count(payload, "total", path)
    if covered > total:
        raise ValueError(f"{path}: covered cannot exceed total")

    area_values = payload.get("areas")
    if not isinstance(area_values, list) or not area_values:
        raise ValueError(f"{path}: areas must be a non-empty array")
    areas: list[CoverageMetric] = []
    area_names: set[str] = set()
    for index, value in enumerate(area_values):
        if not isinstance(value, dict):
            raise ValueError(f"{path}: area {index} must be an object")
        area = _read_text(value, "area", path, max_length=80)
        area_covered = _read_count(value, "covered", path)
        area_total = _read_count(value, "total", path)
        if area in area_names:
            raise ValueError(f"{path}: duplicate area {area!r}")
        if area_covered > area_total:
            raise ValueError(f"{path}: covered cannot exceed total for area {area!r}")
        area_names.add(area)
        areas.append(CoverageMetric(area, area_covered, area_total))
    if sum(area.covered for area in areas) != covered:
        raise ValueError(f"{path}: area covered counts do not match report total")
    if sum(area.total for area in areas) != total:
        raise ValueError(f"{path}: area totals do not match report total")

    uncovered_values = payload.get("uncovered")
    if not isinstance(uncovered_values, list):
        raise ValueError(f"{path}: uncovered must be an array")
    uncovered: list[CapabilityGap] = []
    uncovered_ids: set[str] = set()
    for index, value in enumerate(uncovered_values):
        if not isinstance(value, dict):
            raise ValueError(f"{path}: uncovered capability {index} must be an object")
        capability_id = _read_identifier(value, "id", path)
        area = _read_text(value, "area", path, max_length=80)
        description_value = _read_text(
            value, "description", path, max_length=300
        )
        if capability_id in uncovered_ids:
            raise ValueError(
                f"{path}: duplicate uncovered capability {capability_id!r}"
            )
        if area not in area_names:
            raise ValueError(
                f"{path}: uncovered capability {capability_id!r} has unknown area"
            )
        uncovered_ids.add(capability_id)
        uncovered.append(CapabilityGap(capability_id, area, description_value))
    if len(uncovered) != total - covered:
        raise ValueError(f"{path}: uncovered capabilities do not match report total")
    uncovered_by_area: dict[str, int] = {}
    for capability in uncovered:
        uncovered_by_area[capability.area] = (
            uncovered_by_area.get(capability.area, 0) + 1
        )
    for area in areas:
        if uncovered_by_area.get(area.label, 0) != area.total - area.covered:
            raise ValueError(
                f"{path}: uncovered capabilities do not match area {area.label!r}"
            )

    return CapabilityCoverage(
        description,
        denominator,
        covered,
        total,
        tuple(areas),
        tuple(uncovered),
    )


def _coverage_cell(metric: CoverageMetric) -> str:
    return f"{metric.percent:.1f}% ({metric.covered}/{metric.total})"


def _render_capability_gaps(
    capabilities: tuple[CapabilityGap, ...],
) -> list[str]:
    lines: list[str] = []
    used_bytes = 0
    for capability in capabilities:
        line = (
            f"- `{capability.id}` ({_escape_markdown(capability.area)}): "
            f"{_escape_markdown(capability.description)}"
        )
        line_bytes = len((line + "\n").encode("utf-8"))
        if (
            len(lines) >= MAX_RENDERED_CAPABILITY_GAPS
            or used_bytes + line_bytes > MAX_CAPABILITY_GAP_DETAIL_BYTES
        ):
            break
        lines.append(line)
        used_bytes += line_bytes
    omitted = len(capabilities) - len(lines)
    if omitted:
        lines.append(
            f"- _{omitted} additional uncovered capabilities omitted to keep "
            "the PR comment within GitHub's size limit._"
        )
    return lines


def render_acp_integration_section(
    report: CapabilityCoverage | None,
    run_url: str | None,
    run_result: str | None = None,
    evidence_error: str | None = None,
) -> str:
    if report is None:
        result = run_result or "unavailable"
        lines = [
            "## ACP Integration — Capability Coverage",
            "",
            "Metrics unavailable or ACP Integration was not selected for this "
            f"revision. RC workflow result: **{result}**.",
        ]
        if evidence_error:
            lines.append(f"Evidence error: {evidence_error}.")
        if run_url:
            lines.extend(["", f"[Open RC Certification]({run_url})"])
        return "\n".join(lines) + "\n"

    lines = [
        "## ACP Integration — Capability Coverage",
        "",
        f"**{report.percent:.1f}% ({report.covered}/{report.total})** supported "
        "ACP, KAS, and workflow capabilities have passing Linux integration-test "
        "evidence.",
        "",
        f"_{_escape_markdown(report.description)}_",
        "",
        f"RC workflow result: **{run_result or 'unknown'}**.",
        "",
        "| Area | Coverage | Uncovered |",
        "|---|---:|---:|",
    ]
    for area in report.areas:
        lines.append(
            f"| {_escape_markdown(area.label)} | {_coverage_cell(area)} "
            f"| {area.total - area.covered} |"
        )
    lines.extend(
        [
            "",
            "<details>",
            f"<summary>{len(report.uncovered)} uncovered capabilities</summary>",
            "",
            f"Coverage denominator: {_escape_markdown(report.denominator)}",
            "",
            *_render_capability_gaps(report.uncovered),
            "",
            "</details>",
            "",
            "This is manifest-backed capability coverage, not source-code line "
            "coverage. No base delta is claimed because RC does not generate "
            "exact-base ACP evidence.",
        ]
    )
    if run_url:
        lines.extend(["", f"[Open RC Certification artifacts]({run_url})"])
    return "\n".join(lines) + "\n"


def render_visual_stories_section(
    head: tuple[CoverageMetric, ...] | None,
    base: tuple[CoverageMetric, ...] | None,
    run_url: str | None,
    run_result: str | None = None,
    evidence_error: str | None = None,
) -> str:
    if head is None:
        result = run_result or "unavailable"
        lines = [
            "## Visual Stories — Coverage",
            "",
            f"Metrics unavailable or not selected for this revision. RC workflow result: **{result}**.",
        ]
        if evidence_error:
            lines.append(f"Evidence error: {evidence_error}.")
        if run_url:
            lines.extend(["", f"[Open RC Certification]({run_url})"])
        return "\n".join(lines) + "\n"

    base_by_label = {metric.label: metric for metric in base or ()}
    lines = [
        "## Visual Stories — Coverage",
        "",
        f"RC workflow result: **{run_result or 'unknown'}**.",
        "",
        "| Metric | Base | Head | Δ base | Policy |",
        "|---|---:|---:|---:|---|",
    ]
    for metric in head:
        baseline = base_by_label.get(metric.label)
        base_cell = _coverage_cell(baseline) if baseline else "n/a"
        delta = f"{metric.percent - baseline.percent:+.1f} pp" if baseline else "n/a"
        lines.append(
            f"| {metric.label} | {base_cell} | {_coverage_cell(metric)} "
            f"| {delta} | Advisory |"
        )
    lines.extend(
        [
            "",
            "Percentage floors are not enforced yet. RC still fails on declared "
            "capture and semantic assertion errors.",
        ]
    )
    if base is None:
        lines.append(
            "The exact PR-base capture was unavailable, so base deltas are not shown."
        )
    if run_url:
        lines.extend(["", f"[Open RC Certification artifacts]({run_url})"])
    return "\n".join(lines) + "\n"


def _write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)

    merge = commands.add_parser("merge")
    merge.add_argument("--current", type=Path)
    merge.add_argument("--section", required=True)
    merge.add_argument("--content", required=True, type=Path)
    merge.add_argument("--out", required=True, type=Path)

    visual = commands.add_parser("visual")
    visual.add_argument("--head", type=Path)
    visual.add_argument("--base", type=Path)
    visual.add_argument("--run-result")
    visual.add_argument("--run-url")
    visual.add_argument("--out", required=True, type=Path)

    acp = commands.add_parser("acp")
    acp.add_argument("--report", type=Path)
    acp.add_argument("--run-result")
    acp.add_argument("--run-url")
    acp.add_argument("--out", required=True, type=Path)
    return parser


def main(argv: Sequence[str]) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "merge":
            current = (
                args.current.read_text(encoding="utf-8")
                if args.current and args.current.exists()
                else ""
            )
            content = args.content.read_text(encoding="utf-8")
            _write(args.out, merge_report_section(current, args.section, content))
        elif args.command == "visual":
            head = None
            head_error = None
            if args.head and args.head.exists():
                try:
                    head = load_visual_metrics(args.head)
                except ValueError as error:
                    head_error = "head coverage artifact is invalid"
                    print(f"warning: {error}", file=sys.stderr)
            base = None
            if args.base and args.base.exists():
                try:
                    base = load_visual_metrics(args.base)
                except ValueError as error:
                    print(f"warning: {error}", file=sys.stderr)
            _write(
                args.out,
                render_visual_stories_section(
                    head,
                    base,
                    args.run_url,
                    args.run_result,
                    head_error,
                ),
            )
        else:
            report = None
            report_error = None
            if args.report and args.report.exists():
                try:
                    report = load_capability_coverage(args.report)
                except ValueError as error:
                    report_error = "ACP capability coverage artifact is invalid"
                    print(f"warning: {error}", file=sys.stderr)
            _write(
                args.out,
                render_acp_integration_section(
                    report,
                    args.run_url,
                    args.run_result,
                    report_error,
                ),
            )
    except (OSError, UnicodeError, ValueError) as error:
        print(error, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
