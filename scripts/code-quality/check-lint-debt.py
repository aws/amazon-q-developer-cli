#!/usr/bin/env python3
"""Validate ESLint disable justifications and enforce the lint-debt baseline."""

from __future__ import annotations

import argparse
import json
import re
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence

from sa_lib import (
    EXIT_CLEAN,
    ErrorAccumulator,
    Measurement,
    PerimeterConfigError,
    Violation,
    emit_measurements,
    emit_results,
    load_production_code_perimeter,
    parse_lint_debt,
    run_cli,
    write_measurements,
)

_LINE_DIRECTIVE_RE = re.compile(
    r"//\s*eslint-disable-(?:next-line|line)(?=\s|$)(?P<body>.*)$"
)
_BLOCK_DIRECTIVE_RE = re.compile(
    r"/\*\s*eslint-disable(?P<next>-next-line)?(?=\s|\*/)(?P<body>.*?)\*/"
)
_BLOCK_ENABLE_RE = re.compile(r"/\*\s*eslint-enable(?=\s|\*/)(?P<body>.*?)\*/")
_LINT_DEBT_CANDIDATE_RE = re.compile(
    r"^\s*\{?\s*(?://|/\*)\s*(?:LINT[-_ ]?DEBT)\b",
    re.IGNORECASE,
)
_RULE_NAME_RE = re.compile(r"[A-Za-z0-9@/_-]+")


@dataclass(frozen=True, slots=True)
class DisableDirective:
    """One supported ESLint disable directive found in a source line."""

    line_index: int
    rules: tuple[str, ...]
    is_region_start: bool = False

    @property
    def line_number(self) -> int:
        return self.line_index + 1


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Validate production ESLint disables against the LINT-DEBT baseline."
    )
    parser.add_argument(
        "--root",
        default=".",
        help="repository root used to resolve relative inputs (default: current directory)",
    )
    parser.add_argument(
        "--baseline",
        default=".lint-debt-baseline.json",
        help="debt baseline JSON path (default: .lint-debt-baseline.json)",
    )
    parser.add_argument(
        "--jscpd-config",
        default=".jscpd.json",
        help="production-code perimeter config (default: .jscpd.json)",
    )
    parser.add_argument(
        "--measurements-out",
        help="write the gate readings as JSON so another gate's report can publish them",
    )
    return parser


def _resolve_from_root(root: Path, value: str) -> Path:
    path = Path(value)
    return path if path.is_absolute() else root / path


def _display_path(path: Path, root: Path) -> str:
    candidate = path if path.is_absolute() else root / path
    try:
        return candidate.relative_to(root).as_posix()
    except ValueError:
        return candidate.absolute().as_posix()


def _load_baseline(path: Path, errors: ErrorAccumulator) -> dict[str, int] | None:
    error_count = len(errors)
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        errors.add(error, context=f"cannot read debt baseline {path}")
        return None

    try:
        payload = json.loads(raw)
    except (json.JSONDecodeError, UnicodeError) as error:
        errors.add(error, context=f"cannot parse debt baseline {path}")
        return None

    if not isinstance(payload, dict):
        errors.add(
            ValueError(
                "expected an object mapping rule names to non-negative integers"
            ),
            context=f"invalid debt baseline {path}",
        )
        return None

    baseline: dict[str, int] = {}
    for rule, count in payload.items():
        # Keys share the LINT-DEBT rule grammar so a baseline entry can never name
        # something a justification is incapable of spelling.
        if not isinstance(rule, str) or _RULE_NAME_RE.fullmatch(rule) is None:
            errors.add(
                ValueError(f"rule name {rule!r} is outside the LINT-DEBT grammar"),
                context=f"invalid debt baseline {path}",
            )
            continue
        if isinstance(count, bool) or not isinstance(count, int) or count < 0:
            errors.add(
                ValueError(f"rule {rule!r} must map to a non-negative integer"),
                context=f"invalid debt baseline {path}",
            )
            continue
        baseline[rule] = count
    return baseline if len(errors) == error_count else None


def _extract_rules(body: str) -> tuple[str, ...]:
    rules_text = body.partition(" -- ")[0].strip()
    if not rules_text:
        return ()

    rules: list[str] = []
    seen: set[str] = set()
    for item in rules_text.split(","):
        rule = item.strip()
        if rule and rule not in seen:
            seen.add(rule)
            rules.append(rule)
    return tuple(rules)


def _find_directives(lines: Sequence[str]) -> list[DisableDirective]:
    directives: list[DisableDirective] = []
    for line_index, line in enumerate(lines):
        matches = [
            *((match, False) for match in _LINE_DIRECTIVE_RE.finditer(line)),
            *(
                (match, match.group("next") is None)
                for match in _BLOCK_DIRECTIVE_RE.finditer(line)
            ),
        ]
        for match, is_region_start in sorted(
            matches, key=lambda candidate: candidate[0].start()
        ):
            directives.append(
                DisableDirective(
                    line_index=line_index,
                    rules=_extract_rules(match.group("body")),
                    is_region_start=is_region_start,
                )
            )
    return directives


def _find_region_ends(lines: Sequence[str]) -> list[tuple[int, tuple[str, ...]]]:
    """Return each `eslint-enable` with the rules it re-enables (empty = all)."""

    ends: list[tuple[int, tuple[str, ...]]] = []
    for line_index, line in enumerate(lines):
        for match in _BLOCK_ENABLE_RE.finditer(line):
            ends.append((line_index, _extract_rules(match.group("body"))))
    return ends


def _is_lint_debt_candidate(line: str) -> bool:
    return _LINT_DEBT_CANDIDATE_RE.match(line) is not None


def _record_violation(
    violations: list[Violation],
    errors: ErrorAccumulator,
    *,
    path: str | None,
    line: int | None,
    kind: str,
    detail: str,
    seen: set[tuple[str, int | None, str, str]],
) -> None:
    location_is_valid = (
        isinstance(path, str)
        and bool(path)
        and isinstance(line, int)
        and not isinstance(line, bool)
        and line >= 1
    )
    safe_path = path if isinstance(path, str) and path else "<unknown>"
    safe_line = line if location_is_valid else None
    if not location_is_valid:
        errors.add(
            ValueError(f"cannot report location for {kind} violation"),
            context=safe_path,
        )

    key = (safe_path, safe_line, kind, detail)
    if key not in seen:
        seen.add(key)
        violations.append(Violation(safe_path, safe_line, kind, detail))


def _scan_file(
    path: Path,
    display_path: str,
    violations: list[Violation],
    errors: ErrorAccumulator,
    seen_violations: set[tuple[str, int | None, str, str]],
) -> Counter[str]:
    justified_pairs: Counter[str] = Counter()
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        # A file that is not UTF-8 text carries no directive ESLint could honour,
        # so it is outside this gate's subject rather than a broken repository.
        return justified_pairs
    except (OSError, UnicodeError) as error:
        errors.add(error, context=f"cannot read production file {display_path}")
        return justified_pairs

    lines = text.splitlines()
    directives = _find_directives(lines)
    region_ends = _find_region_ends(lines)
    candidates = {
        line_index
        for line_index, line in enumerate(lines)
        if _is_lint_debt_candidate(line)
    }
    attached_candidates: set[int] = set()

    for directive in directives:
        block: list[int] = []
        line_index = directive.line_index - 1
        while line_index >= 0 and line_index in candidates:
            block.append(line_index)
            line_index -= 1
        block.reverse()
        attached_candidates.update(block)

        if not directive.rules:
            _record_violation(
                violations,
                errors,
                path=display_path,
                line=directive.line_number,
                kind="unjustifiable",
                detail="ESLint disable directive names no rules",
                seen=seen_violations,
            )

        if directive.is_region_start:
            # A region left open suppresses to end-of-file, which one comment
            # cannot honestly justify; only a closed region is a single unit.
            for rule in directive.rules:
                if any(
                    end_index >= directive.line_index and (not end_rules or rule in end_rules)
                    for end_index, end_rules in region_ends
                ):
                    continue
                _record_violation(
                    violations,
                    errors,
                    path=display_path,
                    line=directive.line_number,
                    kind="unbounded",
                    detail=(
                        f"block eslint-disable for rule {rule!r} has no matching "
                        "eslint-enable, so it suppresses to end of file"
                    ),
                    seen=seen_violations,
                )

        matching_comments: dict[str, int] = {}
        directive_rule_set = set(directive.rules)
        for comment_index in block:
            parsed = parse_lint_debt(lines[comment_index])
            if parsed is None:
                _record_violation(
                    violations,
                    errors,
                    path=display_path,
                    line=comment_index + 1,
                    kind="malformed",
                    detail="LINT-DEBT comment does not match the required grammar",
                    seen=seen_violations,
                )
                continue

            rule, _reason = parsed
            if rule not in directive_rule_set:
                _record_violation(
                    violations,
                    errors,
                    path=display_path,
                    line=comment_index + 1,
                    kind="malformed",
                    detail=f"LINT-DEBT rule {rule!r} is not named by the attached directive",
                    seen=seen_violations,
                )
                continue
            if rule in matching_comments:
                _record_violation(
                    violations,
                    errors,
                    path=display_path,
                    line=comment_index + 1,
                    kind="malformed",
                    detail=f"duplicate LINT-DEBT justification for rule {rule!r}",
                    seen=seen_violations,
                )
                continue
            matching_comments[rule] = comment_index

        for rule in directive.rules:
            if rule not in matching_comments:
                _record_violation(
                    violations,
                    errors,
                    path=display_path,
                    line=directive.line_number,
                    kind="unjustified",
                    detail=f"ESLint disable for rule {rule!r} lacks a matching LINT-DEBT comment",
                    seen=seen_violations,
                )
            else:
                justified_pairs[rule] += 1

    for comment_index in sorted(candidates - attached_candidates):
        if parse_lint_debt(lines[comment_index]) is None:
            _record_violation(
                violations,
                errors,
                path=display_path,
                line=comment_index + 1,
                kind="malformed",
                detail="LINT-DEBT comment does not match the required grammar",
                seen=seen_violations,
            )
        _record_violation(
            violations,
            errors,
            path=display_path,
            line=comment_index + 1,
            kind="stray",
            detail="LINT-DEBT comment block is not immediately followed by a disable directive",
            seen=seen_violations,
        )

    return justified_pairs


def main(argv: Sequence[str]) -> int:
    args = _build_parser().parse_args(argv)
    errors = ErrorAccumulator()
    violations: list[Violation] = []
    seen_violations: set[tuple[str, int | None, str, str]] = set()
    measurements: list[Measurement] = []

    root = Path(args.root).expanduser().resolve(strict=False)
    if not root.is_dir():
        errors.add(ValueError("repository root is not a directory"), context=str(root))
        return emit_results(violations, errors)

    baseline_path = _resolve_from_root(root, args.baseline)
    config_path = _resolve_from_root(root, args.jscpd_config)
    baseline = _load_baseline(baseline_path, errors)

    perimeter = None
    try:
        perimeter = load_production_code_perimeter(config_path)
    except PerimeterConfigError as error:
        errors.add(error, context="cannot load production-code perimeter")

    debt_counts: Counter[str] = Counter()
    perimeter_resolved = False
    if perimeter is not None:
        try:
            production_files = tuple(perimeter.iter_files())
            perimeter_resolved = True
        except (OSError, RuntimeError) as error:
            errors.add(error, context="cannot enumerate production-code perimeter")
            production_files = ()

        for path in production_files:
            debt_counts.update(
                _scan_file(
                    path,
                    _display_path(path, root),
                    violations,
                    errors,
                    seen_violations,
                )
            )

    # Counts taken over an unresolved perimeter are not measurements of the
    # tree, so publishing them would dress an exit-2 run as a passing reading.
    if baseline is not None and perimeter_resolved:
        # A rule absent from the baseline is allowed zero, so adopting a rule and
        # suppressing it are separate reviewed acts rather than one silent one.
        for rule in sorted(set(baseline) | set(debt_counts)):
            measured = debt_counts[rule]
            allowed = baseline.get(rule, 0)
            measurements.append(
                Measurement(
                    gate="lint-debt",
                    subject=rule,
                    measured=str(measured),
                    limit=str(allowed),
                    headroom=str(allowed - measured),
                )
            )
            if measured > allowed:
                _record_violation(
                    violations,
                    errors,
                    path=_display_path(baseline_path, root),
                    line=1,
                    kind="over-baseline",
                    detail=(
                        f"rule {rule!r} measured debt count {measured} "
                        f"exceeds baseline {allowed}"
                    ),
                    seen=seen_violations,
                )
            elif measured < allowed:
                # Slack on a count is permission to add debt elsewhere, so a
                # removed suppression has to be banked in the same change.
                _record_violation(
                    violations,
                    errors,
                    path=_display_path(baseline_path, root),
                    line=1,
                    kind="under-baseline",
                    detail=(
                        f"rule {rule!r} measured debt count {measured} is below baseline "
                        f"{allowed}; lower the baseline to {measured}"
                        + (" or remove the entry" if measured == 0 else "")
                    ),
                    seen=seen_violations,
                )
        measurements.append(
            Measurement(
                gate="lint-debt",
                subject="total",
                measured=str(sum(debt_counts.values())),
                limit=str(sum(baseline.values())),
                headroom=str(sum(baseline.values()) - sum(debt_counts.values())),
            )
        )

    emit_measurements(measurements)
    if args.measurements_out and perimeter_resolved:
        try:
            write_measurements(_resolve_from_root(root, args.measurements_out), measurements)
        except OSError as error:
            errors.add(error, context="cannot write gate readings")
    return emit_results(violations, errors)


if __name__ == "__main__":
    run_cli(main)
