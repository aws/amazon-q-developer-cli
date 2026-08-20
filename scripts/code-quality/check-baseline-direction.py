#!/usr/bin/env python3
"""Enforce that committed gate limits only tighten, relative to a base ref."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Callable, Iterator, Sequence

from sa_lib import (
    ErrorAccumulator,
    Violation,
    emit_results,
    run_cli,
)

DEFAULT_BASE_REF = "origin/main"
DEFAULT_FLOORS = ".coverage-floors.json"
DEFAULT_DEBT = ".lint-debt-baseline.json"
DEFAULT_DUPLICATION = ".jscpd-baseline.json"
DEFAULT_PERIMETER = ".jscpd.json"
DEFAULT_ACKNOWLEDGEMENTS = ".baseline-loosening.json"
DENOMINATOR_SEGMENT = "measured"
PERIMETER_INCLUDE_PREFIX = "include:"
PERIMETER_IGNORE_PREFIX = "ignore:"
PERIMETER_FORMAT_PREFIX = "format:"
PERIMETER_PATH_PREFIX = "path:"
# jscpd's own defaults, so omitting a field compares as the value jscpd will use
# and a behaviour-preserving deletion is not a loosening.
PERIMETER_THRESHOLDS = {"minLines": 5.0, "minTokens": 50.0}
PERIMETER_DEFAULT_MODE = "mild"
# Keys whose movement is judged by direction above. Any other key that decides what
# jscpd sees — maxLines, maxSize, skipLocal, a future option — narrows the measured
# set just as effectively, so anything outside these two sets needs a reviewed entry.
PERIMETER_RATCHETED_KEYS = frozenset(
    {"include", "ignore", "format", "minLines", "minTokens", "mode", "path"}
)
PERIMETER_INERT_KEYS = frozenset({"reporters", "output", "silent"})
# Ordered loosest last; an unknown value compares as unranked and is always a change.
PERIMETER_MODES = ("strict", "mild", "weak")


class BaselineComparisonError(ValueError):
    """A committed limit file cannot be compared against its base revision."""


def _parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Fail when a committed gate limit loosens without an acknowledgement."
    )
    parser.add_argument("--base-ref", default=DEFAULT_BASE_REF)
    parser.add_argument("--floors", default=DEFAULT_FLOORS)
    parser.add_argument("--debt-baseline", default=DEFAULT_DEBT)
    parser.add_argument("--duplication-baseline", default=DEFAULT_DUPLICATION)
    parser.add_argument("--perimeter", default=DEFAULT_PERIMETER)
    parser.add_argument("--acknowledgements", default=DEFAULT_ACKNOWLEDGEMENTS)
    parser.add_argument("--repository", default=".")
    return parser.parse_args(list(argv))


def _load_current(path: Path) -> dict[str, object] | None:
    if not path.exists():
        return None
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise BaselineComparisonError(f"cannot read committed limits {path}: {error}") from error
    if not isinstance(payload, dict):
        raise BaselineComparisonError(f"committed limits {path} must be a JSON object")
    return payload


def _load_base(repository: Path, base_ref: str, relative_path: str) -> dict[str, object] | None:
    """Return the base revision of a limit file, or None when it did not exist."""

    exists = subprocess.run(
        ("git", "-C", str(repository), "cat-file", "-e", f"{base_ref}:{relative_path}"),
        capture_output=True,
        text=True,
        check=False,
    )
    if exists.returncode != 0:
        # Distinguish a first appearance from an unusable ref: a bad ref cannot
        # be read at all, while an absent path leaves the rest of the tree readable.
        probe = subprocess.run(
            ("git", "-C", str(repository), "rev-parse", "--verify", f"{base_ref}^{{commit}}"),
            capture_output=True,
            text=True,
            check=False,
        )
        if probe.returncode != 0:
            raise BaselineComparisonError(
                f"cannot resolve base ref {base_ref}: {probe.stderr.strip() or probe.returncode}"
            )
        return None
    completed = subprocess.run(
        ("git", "-C", str(repository), "show", f"{base_ref}:{relative_path}"),
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise BaselineComparisonError(
            f"cannot read {relative_path} at {base_ref}: "
            f"{completed.stderr.strip() or completed.returncode}"
        )
    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise BaselineComparisonError(
            f"cannot parse {relative_path} at {base_ref}: {error}"
        ) from error
    if not isinstance(payload, dict):
        raise BaselineComparisonError(f"{relative_path} at {base_ref} must be a JSON object")
    return payload


def _numbers(payload: dict[str, object], prefix: str = "") -> Iterator[tuple[str, float]]:
    """Yield every numeric leaf as a dotted key."""

    for key, value in payload.items():
        dotted = f"{prefix}{key}"
        if isinstance(value, dict):
            yield from _numbers(value, prefix=f"{dotted}.")
        elif isinstance(value, bool):
            continue
        elif isinstance(value, (int, float)):
            yield dotted, float(value)


def _limits(payload: dict[str, object]) -> Iterator[tuple[str, float]]:
    """Yield the numeric leaves that state a requirement, skipping recorded sizes.

    A populated denominator must not make a package whose every floor is zero
    look capable of failing.
    """

    for key, value in _numbers(payload):
        if DENOMINATOR_SEGMENT in key.split(".")[:-1]:
            continue
        yield key, value


def _acknowledgement(
    acknowledgements: dict[str, object], relative_path: str, key: str
) -> dict[str, object] | None:
    entry = acknowledgements.get(relative_path)
    if not isinstance(entry, dict):
        return None
    record = entry.get(key)
    return record if isinstance(record, dict) else None


def _acknowledges(record: dict[str, object] | None, current_value: float | None) -> bool:
    """An acknowledgement covers exactly the value it names, and must say why.

    Binding it to the value keeps one reviewed exception from licensing every
    later movement of the same limit.
    """

    if record is None:
        return False
    reason = record.get("reason")
    if not isinstance(reason, str) or not reason.strip():
        return False
    if "value" not in record:
        return False
    value = record["value"]
    if current_value is None:
        # Retiring a limit is acknowledged by naming no value at all.
        return value is None
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return False
    return float(value) == current_value


def _compare(
    relative_path: str,
    base: dict[str, object],
    current: dict[str, object] | None,
    loosened: Callable[[float, float], bool],
    describe: str,
    implicit_base: float | None,
    acknowledgements: dict[str, object],
    violations: list[Violation],
) -> None:
    def record(detail: str) -> None:
        violations.append(Violation(relative_path, None, "baseline-direction", detail))

    if current is None:
        record("committed limits were deleted, which removes the gate for every key")
        return

    base_numbers = dict(_numbers(base))
    current_numbers = dict(_numbers(current))
    if implicit_base is not None:
        # A key absent from the base is a limit nobody reviewed; treating it as
        # zero makes adopting a limit and spending it two separate acts.
        for key in current_numbers:
            base_numbers.setdefault(key, implicit_base)

    visited: set[str] = set()
    for key, base_value in base_numbers.items():
        visited.add(key)
        current_value = current_numbers.get(key)
        acknowledged = _acknowledgement(acknowledgements, relative_path, key)
        covered = _acknowledges(acknowledged, current_value)
        if current_value is None:
            # Where an absent key already compares as zero, retiring one is the
            # tightest possible edit; only a floor loses real enforcement.
            if implicit_base is None:
                if not covered:
                    record(f"{key} was removed, which drops its limit of {base_value:g}")
            elif acknowledged is not None and not covered:
                # Left behind, the entry would still name the value a re-add
                # compares against zero, pre-authorising it outside any diff.
                record(
                    f"{key} was retired but keeps its {DEFAULT_ACKNOWLEDGEMENTS} entry; "
                    "remove the acknowledgement in this change"
                )
            continue
        if loosened(current_value, base_value):
            if covered:
                continue
            detail = (
                f"{key} {describe} from {base_value:g} to {current_value:g}; "
                f"record the value and the reason in {DEFAULT_ACKNOWLEDGEMENTS} to allow it"
            )
            if acknowledged is not None:
                detail = (
                    f"{key} {describe} from {base_value:g} to {current_value:g}; its "
                    f"{DEFAULT_ACKNOWLEDGEMENTS} entry does not name this value"
                )
            record(detail)
        elif acknowledged is not None and not covered:
            # An entry naming the committed value stays valid after merging, so it
            # cannot red unrelated PRs; one naming anything else outlived its movement.
            record(
                f"{key} carries a stale {DEFAULT_ACKNOWLEDGEMENTS} entry: the tree commits "
                f"{current_value:g}, so update or remove the acknowledgement"
            )

    if implicit_base is None:
        # Without this, an acknowledgement left over from a retirement survives a
        # re-adoption unseen, and the key returns at any value with no diff entry.
        for key, current_value in current_numbers.items():
            if key in visited:
                continue
            visited.add(key)
            acknowledged = _acknowledgement(acknowledgements, relative_path, key)
            if acknowledged is not None and not _acknowledges(acknowledged, current_value):
                record(
                    f"{key} carries a stale {DEFAULT_ACKNOWLEDGEMENTS} entry: the tree commits "
                    f"{current_value:g}, so update or remove the acknowledgement"
                )

    # An entry for a key in neither revision is never reviewed against the movement
    # it allows; only a floors entry naming null — a completed retirement — may stay.
    entries = acknowledgements.get(relative_path)
    if isinstance(entries, dict):
        for key in sorted(set(entries) - visited):
            entry = entries.get(key)
            if implicit_base is None and isinstance(entry, dict) and _acknowledges(entry, None):
                continue
            record(
                f"{key} has a {DEFAULT_ACKNOWLEDGEMENTS} entry but appears in neither "
                "the base nor the current limits; remove the acknowledgement in this change"
            )


def _pattern_set(payload: dict[str, object], field: str) -> frozenset[str]:
    """Read a pattern list, refusing a shape that would silently compare as empty."""

    if field not in payload:
        return frozenset()
    values = payload[field]
    if not isinstance(values, list):
        raise BaselineComparisonError(
            f"{field} must be a list of patterns, not {type(values).__name__}"
        )
    for item in values:
        if not isinstance(item, str):
            raise BaselineComparisonError(
                f"{field} entries must be strings, not {type(item).__name__}"
            )
    return frozenset(values)


def _threshold(payload: dict[str, object], field: str) -> float:
    """Resolve a sensitivity threshold, falling back to the value jscpd would use.

    jscpd compares these numerically in JavaScript, so a quoted number behaves as
    the number and must here too; a value it could not compare at all is an
    operational error rather than a silent default.
    """

    if field not in payload:
        return PERIMETER_THRESHOLDS[field]
    value = payload[field]
    if isinstance(value, bool) or not isinstance(value, (int, float, str)):
        raise BaselineComparisonError(
            f"{field} must be a number, not {type(value).__name__}"
        )
    try:
        return float(value)
    except ValueError as error:
        raise BaselineComparisonError(f"{field} is not a number: {value!r}") from error


def _mode(payload: dict[str, object]) -> str:
    value = payload.get("mode")
    return value if isinstance(value, str) else PERIMETER_DEFAULT_MODE


def _mode_rank(value: str) -> int:
    """Rank a jscpd mode by how little it counts; an unknown value sorts loosest."""

    if value in PERIMETER_MODES:
        return PERIMETER_MODES.index(value)
    return len(PERIMETER_MODES)


def _setting(payload: dict[str, object], field: str) -> str:
    """Render a setting for comparison and for the key that acknowledges it."""

    if field not in payload:
        return "absent"
    return json.dumps(payload[field], sort_keys=True, separators=(",", ":"))


def _perimeter_acknowledged(
    acknowledgements: dict[str, object], relative_path: str, key: str
) -> bool:
    entry = _acknowledgement(acknowledgements, relative_path, key)
    if entry is None:
        return False
    reason = entry.get("reason")
    return isinstance(reason, str) and bool(reason.strip())


def _compare_perimeter(
    relative_path: str,
    base: dict[str, object],
    current: dict[str, object] | None,
    acknowledgements: dict[str, object],
    violations: list[Violation],
) -> None:
    """The perimeter file has no numbers, but shrinking it lowers every count it
    feeds, which the exact-count gates then force to be banked as an improvement.
    """

    def record(detail: str) -> None:
        violations.append(Violation(relative_path, None, "baseline-direction", detail))

    if current is None:
        record("the perimeter config was deleted, which removes the gates it defines")
        return

    current_include = _pattern_set(current, "include")
    current_ignore = _pattern_set(current, "ignore")

    for pattern in sorted(_pattern_set(base, "include") - current_include):
        key = f"{PERIMETER_INCLUDE_PREFIX}{pattern}"
        if not _perimeter_acknowledged(acknowledgements, relative_path, key):
            record(
                f"include pattern {pattern!r} was removed, which narrows the measured "
                f"perimeter; record {key!r} with a reason in {DEFAULT_ACKNOWLEDGEMENTS} "
                "to allow it"
            )
    for pattern in sorted(current_ignore - _pattern_set(base, "ignore")):
        key = f"{PERIMETER_IGNORE_PREFIX}{pattern}"
        if not _perimeter_acknowledged(acknowledgements, relative_path, key):
            record(
                f"ignore pattern {pattern!r} was added, which narrows the measured "
                f"perimeter; record {key!r} with a reason in {DEFAULT_ACKNOWLEDGEMENTS} "
                "to allow it"
            )
    current_format = _pattern_set(current, "format")
    for entry in sorted(_pattern_set(base, "format") - current_format):
        key = f"{PERIMETER_FORMAT_PREFIX}{entry}"
        if not _perimeter_acknowledged(acknowledgements, relative_path, key):
            record(
                f"format {entry!r} was dropped, which stops that language being measured; "
                f"record {key!r} with a reason in {DEFAULT_ACKNOWLEDGEMENTS} to allow it"
            )
    for field in PERIMETER_THRESHOLDS:
        base_value = _threshold(base, field)
        current_value = _threshold(current, field)
        if current_value <= base_value:
            continue
        key = f"{field}:{current_value:g}"
        if not _perimeter_acknowledged(acknowledgements, relative_path, key):
            record(
                f"{field} rose from {base_value:g} to {current_value:g}, which makes fewer "
                f"duplicates count; record {key!r} with a reason in "
                f"{DEFAULT_ACKNOWLEDGEMENTS} to allow it"
            )
    base_mode, current_mode = _mode(base), _mode(current)
    if current_mode != base_mode and _mode_rank(current_mode) >= _mode_rank(base_mode):
        key = f"mode:{current_mode}"
        if not _perimeter_acknowledged(acknowledgements, relative_path, key):
            record(
                f"mode changed from {base_mode!r} to {current_mode!r}, which makes fewer "
                f"duplicates count; record {key!r} with a reason in "
                f"{DEFAULT_ACKNOWLEDGEMENTS} to allow it"
            )
    current_path = _pattern_set(current, "path")
    for root in sorted(_pattern_set(base, "path") - current_path):
        key = f"{PERIMETER_PATH_PREFIX}{root}"
        if not _perimeter_acknowledged(acknowledgements, relative_path, key):
            record(
                f"scan root {root!r} was removed, which stops that tree being scanned; "
                f"record {key!r} with a reason in {DEFAULT_ACKNOWLEDGEMENTS} to allow it"
            )
    for field in sorted((set(base) | set(current)) - PERIMETER_RATCHETED_KEYS):
        if field in PERIMETER_INERT_KEYS or base.get(field) == current.get(field):
            continue
        key = f"{field}:{_setting(current, field)}"
        if not _perimeter_acknowledged(acknowledgements, relative_path, key):
            record(
                f"{field} changed from {_setting(base, field)} to {_setting(current, field)}, "
                "which can change what jscpd measures; record "
                f"{key!r} with a reason in {DEFAULT_ACKNOWLEDGEMENTS} to allow it"
            )

    # Validity anchors to the committed perimeter, not the diff: a merged
    # acknowledgement stays green, one planted for a future widening turns stale.
    entries = acknowledgements.get(relative_path)
    if not isinstance(entries, dict):
        return
    for key in sorted(entries):
        if key.startswith(PERIMETER_INCLUDE_PREFIX):
            valid = key[len(PERIMETER_INCLUDE_PREFIX) :] not in current_include
        elif key.startswith(PERIMETER_IGNORE_PREFIX):
            valid = key[len(PERIMETER_IGNORE_PREFIX) :] in current_ignore
        elif key.startswith(PERIMETER_FORMAT_PREFIX):
            valid = key[len(PERIMETER_FORMAT_PREFIX) :] not in current_format
        elif key.startswith(PERIMETER_PATH_PREFIX):
            valid = key[len(PERIMETER_PATH_PREFIX) :] not in current_path
        elif key.startswith("mode:"):
            valid = key[len("mode:") :] == _mode(current)
        elif any(key.startswith(f"{field}:") for field in PERIMETER_THRESHOLDS):
            field, _, named = key.partition(":")
            valid = f"{_threshold(current, field):g}" == named
        else:
            field, separator, named = key.partition(":")
            valid = bool(separator) and _setting(current, field) == named
        if not valid or not _perimeter_acknowledged(acknowledgements, relative_path, key):
            record(
                f"{key} carries a {DEFAULT_ACKNOWLEDGEMENTS} entry that does not describe "
                "the committed perimeter; update or remove the acknowledgement"
            )


def _check_new_packages(
    relative_path: str,
    base: dict[str, object],
    current: dict[str, object] | None,
    violations: list[Violation],
) -> None:
    """A newly declared package must commit a limit that is capable of failing."""

    if current is None:
        return
    for package, values in current.items():
        if package in base or not isinstance(values, dict):
            continue
        # The report renders floors to one decimal place, so anything under
        # 0.05 displays as 0.0% and is a zero wearing a nonzero's clothes.
        if any(value >= 0.05 for _key, value in _limits({package: values})):
            continue
        violations.append(
            Violation(
                relative_path,
                None,
                "baseline-direction",
                f"{package} is new and every limit is zero, so it cannot fail",
            )
        )


def main(argv: Sequence[str]) -> int:
    args = _parse_args(argv)
    errors = ErrorAccumulator()
    violations: list[Violation] = []
    repository = Path(args.repository)

    comparisons = (
        # Floors are minimums, so a smaller number is weaker. A floor absent from
        # the base is not a loosening: any positive value is tighter than nothing.
        (args.floors, lambda current, base: current < base, "fell", None),
        # Debt and clone counts are ceilings, so a larger number is weaker.
        (args.debt_baseline, lambda current, base: current > base, "rose", 0.0),
        (args.duplication_baseline, lambda current, base: current > base, "rose", 0.0),
    )
    for relative_path, loosened, describe, implicit_base in comparisons:
        try:
            base = _load_base(repository, args.base_ref, relative_path)
            if base is None:
                continue
            current = _load_current(repository / relative_path)
            acknowledgements = _load_current(repository / args.acknowledgements) or {}
            _compare(
                relative_path,
                base,
                current,
                loosened,
                describe,
                implicit_base,
                acknowledgements,
                violations,
            )
            if relative_path == args.floors:
                _check_new_packages(relative_path, base, current, violations)
        except BaselineComparisonError as error:
            errors.add(error, context=f"baseline direction {relative_path}")

    try:
        base = _load_base(repository, args.base_ref, args.perimeter)
        if base is not None:
            _compare_perimeter(
                args.perimeter,
                base,
                _load_current(repository / args.perimeter),
                _load_current(repository / args.acknowledgements) or {},
                violations,
            )
    except BaselineComparisonError as error:
        errors.add(error, context=f"baseline direction {args.perimeter}")

    return emit_results(violations, errors)


if __name__ == "__main__":
    run_cli(main)
