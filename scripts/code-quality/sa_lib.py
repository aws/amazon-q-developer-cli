"""Shared, dependency-free primitives for the code-quality scripts."""

from __future__ import annotations

import fnmatch
import glob
import json
import os
import re
import sys
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path, PurePosixPath
from typing import Callable, Iterable, Iterator, NoReturn, Sequence, TextIO

EXIT_CLEAN = 0
EXIT_VIOLATIONS = 1
EXIT_ERROR = 2

# Exposed contract: this pattern defines what counts as a justified suppression
# repo-wide, so any change to it changes what the gates accept.
LINT_DEBT_RE = re.compile(r"^\s*//\s*LINT-DEBT\(([A-Za-z0-9@/_-]+)\):\s*(\S.*)$")

# Block-comment form, required inside JSX children where a `//` line renders as
# visible text; the braces are optional so a plain `/* ... */` is also accepted.
LINT_DEBT_BLOCK_RE = re.compile(
    r"^\s*\{?\s*/\*\s*LINT-DEBT\(([A-Za-z0-9@/_-]+)\):\s*(\S(?:.*\S)?)\s*\*/\s*\}?\s*$"
)


class PerimeterConfigError(ValueError):
    """The production-code perimeter configuration could not be loaded."""


def parse_lint_debt(comment: str) -> tuple[str, str] | None:
    """Return ``(rule, reason)`` for a canonical LINT-DEBT comment."""

    if not isinstance(comment, str):
        raise TypeError("comment must be a string")
    if "\n" in comment or "\r" in comment:
        return None
    match = LINT_DEBT_RE.fullmatch(comment)
    if match is None:
        match = LINT_DEBT_BLOCK_RE.fullmatch(comment)
    if match is None:
        return None
    return match.group(1), match.group(2)


def format_lint_debt(rule: str, reason: str) -> str:
    """Format a LINT-DEBT comment, rejecting values outside the grammar."""

    if not isinstance(rule, str) or not isinstance(reason, str):
        raise TypeError("rule and reason must be strings")
    comment = f"// LINT-DEBT({rule}): {reason}"
    if parse_lint_debt(comment) != (rule, reason):
        raise ValueError("rule or reason does not satisfy the LINT-DEBT grammar")
    return comment


@dataclass(frozen=True, slots=True)
class Violation:
    """A uniformly rendered policy violation."""

    path: str
    line: int | None
    kind: str
    detail: str

    def __post_init__(self) -> None:
        path = os.fspath(self.path)
        if not isinstance(path, str) or not path:
            raise ValueError("violation path must be a non-empty string")
        if (
            self.line is not None
            and (
                isinstance(self.line, bool)
                or not isinstance(self.line, int)
                or self.line < 1
            )
        ):
            raise ValueError("violation line must be a positive integer or None")
        if not isinstance(self.kind, str) or not self.kind:
            raise ValueError("violation kind must be a non-empty string")
        if not isinstance(self.detail, str) or not self.detail:
            raise ValueError("violation detail must be a non-empty string")
        object.__setattr__(self, "path", path)

    def render(self) -> str:
        location = self.path if self.line is None else f"{self.path}:{self.line}"
        return f"{location}: {self.kind}: {self.detail}"

    def __str__(self) -> str:
        return self.render()


@dataclass(frozen=True, slots=True)
class Measurement:
    """One numeric gate reading, rendered identically by every checker.

    Emitted whether or not the gate passes: a reading published only on failure
    leaves nobody able to see the margin until it is already gone.
    """

    gate: str
    subject: str
    measured: str
    limit: str
    headroom: str

    def __post_init__(self) -> None:
        for name in ("gate", "measured", "limit", "headroom"):
            value = getattr(self, name)
            if not isinstance(value, str) or not value:
                raise ValueError(f"measurement {name} must be a non-empty string")
        if not isinstance(self.subject, str):
            raise ValueError("measurement subject must be a string")

    def render(self) -> str:
        subject = f" {self.subject}" if self.subject else ""
        return (
            f"measured: {self.gate}{subject} = {self.measured} "
            f"(limit {self.limit}, headroom {self.headroom})"
        )

    def __str__(self) -> str:
        return self.render()


def emit_measurements(
    measurements: Iterable[Measurement],
    *,
    stdout: TextIO | None = None,
) -> None:
    """Publish gate readings so a passing run is auditable, not merely green."""

    stream = sys.stdout if stdout is None else stdout
    for measurement in measurements:
        print(measurement.render(), file=stream)


def write_measurements(path: str | Path, measurements: Iterable[Measurement]) -> None:
    """Persist gate readings so another gate's report can publish them.

    A reading printed only to a job log is invisible on the pull request, which
    leaves a reviewer unable to audit a green run without leaving the page.
    """

    payload = [
        {
            "gate": item.gate,
            "subject": item.subject,
            "measured": item.measured,
            "limit": item.limit,
            "headroom": item.headroom,
        }
        for item in measurements
    ]
    Path(path).write_text(json.dumps(payload), encoding="utf-8")


def read_measurements(path: str | Path) -> list[Measurement]:
    """Load gate readings written by another checker."""

    payload = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(payload, list):
        raise ValueError(f"gate readings {os.fspath(path)} must be a JSON array")
    readings: list[Measurement] = []
    for entry in payload:
        if not isinstance(entry, dict):
            raise ValueError(f"gate readings {os.fspath(path)} must contain objects")
        try:
            readings.append(
                Measurement(
                    gate=entry["gate"],
                    subject=entry["subject"],
                    measured=entry["measured"],
                    limit=entry["limit"],
                    headroom=entry["headroom"],
                )
            )
        except KeyError as error:
            raise ValueError(
                f"gate readings {os.fspath(path)} entry is missing {error.args[0]!r}"
            ) from error
    return readings


def resolve_exit_code(has_errors: bool, has_violations: bool) -> int:
    """Resolve the shared 2-over-1-over-0 process exit-code contract."""

    if has_errors:
        return EXIT_ERROR
    if has_violations:
        return EXIT_VIOLATIONS
    return EXIT_CLEAN


def _describe_cause(cause: BaseException | str) -> str:
    if isinstance(cause, BaseException):
        message = str(cause)
        return f"{type(cause).__name__}: {message}" if message else type(cause).__name__
    message = str(cause)
    return message if message else "unknown operational error"


@dataclass(slots=True)
class ErrorAccumulator:
    """Collect operational errors while allowing a checker to continue safely."""

    _errors: list[str] = field(default_factory=list, init=False, repr=False)

    def add(self, cause: BaseException | str, *, context: str | None = None) -> None:
        message = _describe_cause(cause)
        if context:
            message = f"{context}: {message}"
        self._errors.append(message)

    @property
    def errors(self) -> tuple[str, ...]:
        return tuple(self._errors)

    @property
    def has_errors(self) -> bool:
        return bool(self._errors)

    def __bool__(self) -> bool:
        return self.has_errors

    def __len__(self) -> int:
        return len(self._errors)

    def emit(self, stream: TextIO | None = None) -> None:
        target = sys.stderr if stream is None else stream
        for error in self._errors:
            print(f"error: {error}", file=target)

    def exit_code(self, has_violations: bool = False) -> int:
        return resolve_exit_code(self.has_errors, has_violations)


def emit_results(
    violations: Iterable[Violation],
    errors: ErrorAccumulator,
    *,
    stdout: TextIO | None = None,
    stderr: TextIO | None = None,
) -> int:
    """Emit violations to stdout, diagnostics to stderr, and return the exit code."""

    findings = tuple(violations)
    violation_stream = sys.stdout if stdout is None else stdout
    for finding in findings:
        print(finding.render(), file=violation_stream)
    errors.emit(stderr)
    return resolve_exit_code(errors.has_errors, bool(findings))


def _exception_chain(error: BaseException) -> str:
    descriptions: list[str] = []
    seen: set[int] = set()
    current: BaseException | None = error
    while current is not None and id(current) not in seen:
        seen.add(id(current))
        descriptions.append(_describe_cause(current))
        if current.__cause__ is not None:
            current = current.__cause__
        elif current.__context__ is not None and not current.__suppress_context__:
            current = current.__context__
        else:
            current = None
    return "; caused by ".join(descriptions)


def run_cli(main: Callable[[Sequence[str]], int], argv: Sequence[str] | None = None) -> NoReturn:
    """Run a script main function and convert every unhandled exception to exit 2."""

    try:
        exit_code = main(tuple(sys.argv[1:] if argv is None else argv))
        if isinstance(exit_code, bool) or exit_code not in {
            EXIT_CLEAN,
            EXIT_VIOLATIONS,
            EXIT_ERROR,
        }:
            raise ValueError(f"main returned unsupported exit code: {exit_code!r}")
    except Exception as error:
        try:
            print(f"error: unhandled exception: {_exception_chain(error)}", file=sys.stderr)
        except Exception:
            # Reporting failure must not turn an operational error into success.
            pass
        raise SystemExit(EXIT_ERROR) from error
    raise SystemExit(exit_code)


def _normalise_glob(pattern: str, *, field_name: str) -> str:
    if not isinstance(pattern, str) or not pattern.strip():
        raise PerimeterConfigError(f"{field_name} entries must be non-empty strings")
    normalised = pattern.replace("\\", "/")
    while normalised.startswith("./"):
        normalised = normalised[2:]
    while "//" in normalised:
        normalised = normalised.replace("//", "/")
    if normalised.startswith("/"):
        raise PerimeterConfigError(f"{field_name} glob must be relative: {pattern!r}")
    parts = PurePosixPath(normalised).parts
    if ".." in parts:
        raise PerimeterConfigError(f"{field_name} glob cannot escape the repository: {pattern!r}")
    normalised = "/".join(part for part in parts if part not in {"", "."})
    if not normalised:
        raise PerimeterConfigError(f"{field_name} entries must be non-empty strings")
    return normalised


@lru_cache(maxsize=32_768)
def _glob_matches(relative_path: str, pattern: str) -> bool:
    """Match slash-delimited paths; ``**`` consumes zero or more path segments."""

    path_parts = tuple(part for part in relative_path.split("/") if part)
    pattern_parts = tuple(part for part in pattern.split("/") if part)

    @lru_cache(maxsize=None)
    def match(path_index: int, pattern_index: int) -> bool:
        if pattern_index == len(pattern_parts):
            return path_index == len(path_parts)
        pattern_part = pattern_parts[pattern_index]
        if pattern_part == "**":
            return match(path_index, pattern_index + 1) or (
                path_index < len(path_parts) and match(path_index + 1, pattern_index)
            )
        return (
            path_index < len(path_parts)
            and fnmatch.fnmatchcase(path_parts[path_index], pattern_part)
            and match(path_index + 1, pattern_index + 1)
        )

    return match(0, 0)


@dataclass(frozen=True, slots=True)
class ProductionCodePerimeter:
    """Authoritative include-minus-ignore perimeter rooted beside `.jscpd.json`."""

    root: Path
    include_globs: tuple[str, ...]
    ignore_globs: tuple[str, ...]

    def _relative_path(self, path: str | os.PathLike[str]) -> str | None:
        candidate = Path(path)
        if candidate.is_absolute():
            try:
                candidate = candidate.resolve(strict=False).relative_to(self.root)
            except ValueError:
                return None
            raw = candidate.as_posix()
        else:
            raw = os.fspath(path).replace("\\", "/")
        while raw.startswith("./"):
            raw = raw[2:]
        parts = PurePosixPath(raw).parts
        if not parts or ".." in parts:
            return None
        return "/".join(part for part in parts if part not in {"", "."})

    def _is_ignored(self, relative_path: str) -> bool:
        return any(_glob_matches(relative_path, pattern) for pattern in self.ignore_globs)

    def matches(self, path: str | os.PathLike[str]) -> bool:
        """Return whether a relative or rooted absolute path is production code."""

        relative_path = self._relative_path(path)
        if relative_path is None:
            return False
        included = any(_glob_matches(relative_path, pattern) for pattern in self.include_globs)
        return included and not self._is_ignored(relative_path)

    def iter_files(self) -> Iterator[Path]:
        """Yield matching files deterministically without traversing ignored trees."""

        starts: set[Path] = set()
        for pattern in self.include_globs:
            prefix: list[str] = []
            for part in pattern.split("/"):
                if part == "**" or glob.has_magic(part):
                    break
                prefix.append(part)
            start = self.root.joinpath(*prefix)
            if start.exists():
                starts.add(start)

        minimal_starts: list[Path] = []
        for start in sorted(starts, key=lambda item: (len(item.parts), item.as_posix())):
            if any(start == existing or existing in start.parents for existing in minimal_starts):
                continue
            minimal_starts.append(start)

        found: set[Path] = set()
        for start in minimal_starts:
            if start.is_file():
                if self.matches(start):
                    found.add(start)
                continue

            def raise_walk_error(error: OSError) -> None:
                raise error

            for directory, directory_names, file_names in os.walk(
                start, followlinks=False, onerror=raise_walk_error
            ):
                directory_path = Path(directory)
                kept_directories: list[str] = []
                for name in sorted(directory_names):
                    child = directory_path / name
                    relative_child = self._relative_path(child)
                    if relative_child is not None and not self._is_ignored(relative_child):
                        kept_directories.append(name)
                directory_names[:] = kept_directories
                for name in sorted(file_names):
                    candidate = directory_path / name
                    if self.matches(candidate):
                        found.add(candidate)

        yield from sorted(found, key=lambda item: item.as_posix())


def load_production_code_perimeter(
    config_path: str | os.PathLike[str],
) -> ProductionCodePerimeter:
    """Load and validate a `.jscpd.json` perimeter, including that it matches files.

    Resolving the globs against the filesystem here means a perimeter that
    selects nothing is an error rather than a reading of zero.
    """

    path = Path(config_path)
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise PerimeterConfigError(f"cannot read perimeter config {path}: {error}") from error
    try:
        payload = json.loads(raw)
    except (json.JSONDecodeError, UnicodeError) as error:
        raise PerimeterConfigError(f"cannot parse perimeter config {path}: {error}") from error
    if not isinstance(payload, dict):
        raise PerimeterConfigError(f"perimeter config {path} must contain a JSON object")

    include = payload.get("include")
    ignore = payload.get("ignore")
    if not isinstance(include, list) or not include:
        raise PerimeterConfigError(f"perimeter config {path} requires a non-empty include list")
    if not isinstance(ignore, list):
        raise PerimeterConfigError(f"perimeter config {path} requires an ignore list")

    include_globs = tuple(_normalise_glob(item, field_name="include") for item in include)
    ignore_globs = tuple(_normalise_glob(item, field_name="ignore") for item in ignore)
    perimeter = ProductionCodePerimeter(
        root=path.parent.resolve(),
        include_globs=include_globs,
        ignore_globs=ignore_globs,
    )
    try:
        matched = tuple(perimeter.iter_files())
    except OSError as error:
        raise PerimeterConfigError(f"cannot enumerate perimeter under {perimeter.root}: {error}") from error
    # Every include pattern is checked separately: one that matches nothing makes
    # its whole tree invisible to the gates while the others keep the total non-zero.
    for pattern in include_globs:
        if any(
            _glob_matches(relative, pattern)
            for relative in (perimeter._relative_path(candidate) for candidate in matched)
            if relative is not None
        ):
            continue
        raise PerimeterConfigError(
            f"perimeter config {path} include pattern {pattern!r} matches no files "
            f"under {perimeter.root}"
        )
    return perimeter


__all__ = [
    "EXIT_CLEAN",
    "EXIT_ERROR",
    "EXIT_VIOLATIONS",
    "ErrorAccumulator",
    "LINT_DEBT_RE",
    "Measurement",
    "PerimeterConfigError",
    "ProductionCodePerimeter",
    "Violation",
    "emit_measurements",
    "emit_results",
    "format_lint_debt",
    "load_production_code_perimeter",
    "parse_lint_debt",
    "resolve_exit_code",
    "run_cli",
]
