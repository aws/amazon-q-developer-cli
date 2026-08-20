#!/usr/bin/env python3
"""Annotate pre-existing TypeScript lint debt with scoped ESLint disables."""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import stat
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence

from sa_lib import (
    EXIT_CLEAN,
    ErrorAccumulator,
    emit_results,
    format_lint_debt,
    run_cli,
)

DEFAULT_RULES = (
    "complexity",
    "max-depth",
    "max-params",
    "sonarjs/cognitive-complexity",
)
DEFAULT_ESLINT_COMMAND = "bunx eslint"
_INDENT_RE = re.compile(br"[ \t]*")
_WRITE_BITS = stat.S_IWUSR | stat.S_IWGRP | stat.S_IWOTH
_OPEN_PAREN = ord("(")
_CLOSE_PAREN = ord(")")
_HEAD_WINDOW = 40


@dataclass(frozen=True, slots=True)
class EslintRun:
    payload: Any
    returncode: int
    stderr: str


@dataclass(frozen=True, slots=True)
class AnnotationPlan:
    path: Path
    content: bytes


SiteMessages = dict[Path, dict[int, dict[str, str]]]


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Annotate Rule_Set ESLint errors with scoped LINT-DEBT comments."
    )
    parser.add_argument(
        "--package",
        required=True,
        help="TypeScript package directory to annotate",
    )
    parser.add_argument(
        "--eslint-cmd",
        default=DEFAULT_ESLINT_COMMAND,
        help=f"ESLint command prefix (default: {DEFAULT_ESLINT_COMMAND!r})",
    )
    parser.add_argument(
        "--rules",
        default=",".join(DEFAULT_RULES),
        help=(
            "Comma-separated ESLint Rule_Set identifiers "
            f"(default: {','.join(DEFAULT_RULES)})"
        ),
    )
    parser.add_argument(
        "--report",
        help="Pre-produced ESLint JSON report used instead of the initial ESLint run",
    )
    return parser


def _parse_rules(raw_rules: str, errors: ErrorAccumulator) -> tuple[str, ...]:
    if not isinstance(raw_rules, str):
        errors.add("--rules must be a comma-separated string")
        return ()
    pieces = raw_rules.split(",")
    if not pieces or any(not piece.strip() for piece in pieces):
        errors.add("--rules must contain only non-empty comma-separated rule identifiers")
        return ()

    rules: list[str] = []
    seen: set[str] = set()
    for piece in pieces:
        rule = piece.strip()
        try:
            format_lint_debt(rule, "configured Rule_Set violation")
        except (TypeError, ValueError) as error:
            errors.add(error, context=f"invalid --rules entry {rule!r}")
            continue
        if rule not in seen:
            seen.add(rule)
            rules.append(rule)
    return tuple(rules)


def _load_json_report(path: Path, errors: ErrorAccumulator) -> Any | None:
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        errors.add(error, context=f"cannot read ESLint report {path}")
        return None
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, UnicodeError) as error:
        errors.add(error, context=f"cannot parse ESLint report {path}")
        return None


def _run_eslint(
    command: str,
    package: Path,
    errors: ErrorAccumulator,
    *,
    phase: str,
) -> EslintRun | None:
    try:
        argv = shlex.split(command)
    except ValueError as error:
        errors.add(error, context=f"cannot parse {phase} ESLint command")
        return None
    if not argv:
        errors.add(f"{phase} ESLint command is empty")
        return None

    try:
        completed = subprocess.run(
            [*argv, ".", "--format", "json"],
            cwd=package,
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=True,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        errors.add(error, context=f"cannot execute {phase} ESLint command")
        return None

    try:
        payload = json.loads(completed.stdout)
    except (json.JSONDecodeError, UnicodeError) as error:
        detail = _diagnostic_suffix(completed.stderr)
        errors.add(
            f"unparseable JSON output ({error}){detail}",
            context=f"{phase} ESLint command",
        )
        return None
    return EslintRun(payload, completed.returncode, completed.stderr)


def _diagnostic_suffix(stderr: str) -> str:
    diagnostic = " ".join(stderr.split())
    if not diagnostic:
        return ""
    if len(diagnostic) > 1000:
        diagnostic = diagnostic[-1000:]
    return f"; stderr: {diagnostic}"


def _inside(candidate: Path, directory: Path) -> bool:
    try:
        candidate.relative_to(directory)
    except ValueError:
        return False
    return True


def _resolve_reported_path(
    raw_path: str,
    package: Path,
    invocation_root: Path,
    errors: ErrorAccumulator,
) -> Path | None:
    try:
        reported = Path(raw_path)
        if reported.is_absolute():
            candidates = (reported.resolve(strict=False),)
        else:
            candidates = (
                (invocation_root / reported).resolve(strict=False),
                (package / reported).resolve(strict=False),
            )
    except (OSError, RuntimeError, ValueError) as error:
        errors.add(error, context=f"cannot resolve ESLint path {raw_path!r}")
        return None

    eligible: list[Path] = []
    for candidate in candidates:
        if candidate not in eligible and _inside(candidate, package):
            eligible.append(candidate)
    if not eligible:
        errors.add(
            f"reported file is outside package {package}: {raw_path}",
            context="ESLint report",
        )
        return None
    for candidate in eligible:
        if candidate.exists():
            return candidate
    return eligible[0]


def _collect_violations(
    payload: Any,
    package: Path,
    rules: tuple[str, ...],
    errors: ErrorAccumulator,
) -> SiteMessages:
    grouped: SiteMessages = {}
    selected_rules = set(rules)
    invocation_root = Path.cwd().resolve()

    if not isinstance(payload, list):
        errors.add("ESLint report must contain a JSON array")
        return grouped

    for result_index, result in enumerate(payload):
        context = f"ESLint report result {result_index}"
        if not isinstance(result, dict):
            errors.add("result must be an object", context=context)
            continue
        raw_path = result.get("filePath")
        messages = result.get("messages")
        if not isinstance(raw_path, str) or not raw_path:
            errors.add("filePath must be a non-empty string", context=context)
            continue
        if not isinstance(messages, list):
            errors.add("messages must be an array", context=context)
            continue

        selected: list[tuple[int, str, str]] = []
        for message_index, message in enumerate(messages):
            message_context = f"{context} message {message_index}"
            if not isinstance(message, dict):
                errors.add("message must be an object", context=message_context)
                continue
            rule_id = message.get("ruleId")
            severity = message.get("severity")
            if rule_id is not None and not isinstance(rule_id, str):
                errors.add("ruleId must be a string or null", context=message_context)
                continue
            if (
                isinstance(severity, bool)
                or not isinstance(severity, int)
                or severity not in {0, 1, 2}
            ):
                errors.add("severity must be one of 0, 1, or 2", context=message_context)
                continue
            if severity != 2 or rule_id not in selected_rules:
                continue

            line = message.get("line")
            text = message.get("message")
            if isinstance(line, bool) or not isinstance(line, int) or line < 1:
                errors.add(
                    "selected Rule_Set violation requires a positive integer line",
                    context=message_context,
                )
                continue
            if not isinstance(text, str) or not text.strip():
                errors.add(
                    "selected Rule_Set violation requires a non-empty message",
                    context=message_context,
                )
                continue
            selected.append((line, rule_id, text))

        if not selected:
            continue
        target = _resolve_reported_path(raw_path, package, invocation_root, errors)
        if target is None:
            continue
        file_sites = grouped.setdefault(target, {})
        for line, rule_id, text in selected:
            site = file_sites.setdefault(line, {})
            site.setdefault(rule_id, text)

    return grouped


def _reason_from_message(message: str) -> str:
    printable = "".join(
        character if character.isprintable() and character not in "\r\n" else " "
        for character in message
    )
    detail = " ".join(printable.split())
    if not detail:
        detail = "Rule_Set threshold exceeded"
    return f"pre-existing at gate adoption; {detail}; refactor before extending"


def _line_ending(line: bytes, content: bytes) -> bytes:
    if line.endswith(b"\r\n"):
        return b"\r\n"
    if line.endswith(b"\n"):
        return b"\n"
    if line.endswith(b"\r"):
        return b"\r"
    first_ending = re.search(br"\r\n|\n|\r", content)
    return first_ending.group(0) if first_ending is not None else b"\n"


def _indentation(line: bytes) -> bytes:
    match = _INDENT_RE.match(line)
    return match.group(0) if match is not None else b""


def _head_anchor(lines: Sequence[bytes], line_number: int) -> int:
    """Line that opens the construct whose head ends at ``line_number``.

    ESLint reports a function at its arrow or keyword, which for a multi-line
    signature sits below the declaration, so annotating the reported line puts
    comments between the parameters. Balancing parentheses backwards recovers
    the declaration; anything further than the window, or an unbalanced head,
    keeps the reported line so the annotation stays adjacent to its violation.
    """
    depth = 0
    lowest = max(0, line_number - _HEAD_WINDOW)
    for index in range(line_number - 1, lowest - 1, -1):
        for char in reversed(lines[index]):
            if char == _CLOSE_PAREN:
                depth += 1
            elif char == _OPEN_PAREN:
                depth -= 1
                if depth <= 0:
                    return index + 1
        if depth <= 0:
            return line_number
    return line_number



def _build_plan(
    path: Path,
    sites: dict[int, dict[str, str]],
    rule_order: dict[str, int],
    errors: ErrorAccumulator,
) -> AnnotationPlan | None:
    try:
        metadata = path.stat()
    except OSError as error:
        errors.add(error, context=f"cannot inspect annotation target {path}")
        return None
    if not stat.S_ISREG(metadata.st_mode):
        errors.add("target is not a regular file", context=f"annotation target {path}")
        return None
    if metadata.st_mode & _WRITE_BITS == 0 or not os.access(path, os.W_OK):
        errors.add("target is not writable", context=f"annotation target {path}")
        return None
    try:
        original = path.read_bytes()
    except OSError as error:
        errors.add(error, context=f"cannot read annotation target {path}")
        return None

    lines = original.splitlines(keepends=True)
    for line_number in sites:
        if line_number > len(lines):
            errors.add(
                f"reported line {line_number} is outside file with {len(lines)} lines",
                context=f"annotation target {path}",
            )
    if errors.has_errors:
        return None

    output: list[bytes] = []
    before: dict[int, list[bytes]] = {}
    after: dict[int, list[bytes]] = {}
    for line_number, site in sorted(sites.items()):
        reported_line = lines[line_number - 1]
        ending = _line_ending(reported_line, original)
        ordered_rules = sorted(site, key=lambda rule: rule_order[rule])
        rules_text = ", ".join(ordered_rules)
        anchor = _head_anchor(lines, line_number)
        indentation = _indentation(lines[anchor - 1])
        block = [
            indentation
            + format_lint_debt(rule, _reason_from_message(site[rule])).encode("utf-8")
            + ending
            for rule in ordered_rules
        ]
        if anchor == line_number:
            directive = f"// eslint-disable-next-line {rules_text}"
        else:
            # The justification belongs above the declaration, so the span has to
            # reach the reported line; an enable keeps it off the rest of the file.
            directive = f"/* eslint-disable {rules_text} */"
            after.setdefault(line_number, []).append(
                _indentation(reported_line)
                + f"/* eslint-enable {rules_text} */".encode("utf-8")
                + ending
            )
        block.append(indentation + directive.encode("utf-8") + ending)
        before.setdefault(anchor, []).extend(block)

    for line_number, original_line in enumerate(lines, start=1):
        output.extend(before.get(line_number, ()))
        output.append(original_line)
        output.extend(after.get(line_number, ()))
    return AnnotationPlan(path, b"".join(output))


def _build_plans(
    grouped: SiteMessages,
    rules: tuple[str, ...],
    errors: ErrorAccumulator,
) -> list[AnnotationPlan]:
    rule_order = {rule: index for index, rule in enumerate(rules)}
    plans: list[AnnotationPlan] = []
    for path in sorted(grouped, key=lambda item: item.as_posix()):
        plan = _build_plan(path, grouped[path], rule_order, errors)
        if plan is not None:
            plans.append(plan)
    return plans


def _write_plans(plans: Sequence[AnnotationPlan], errors: ErrorAccumulator) -> None:
    for plan in plans:
        try:
            plan.path.write_bytes(plan.content)
        except OSError as error:
            errors.add(error, context=f"cannot write annotation target {plan.path}")


def _command_failed(
    run: EslintRun,
    errors: ErrorAccumulator,
    *,
    phase: str,
) -> None:
    errors.add(
        f"command exited {run.returncode}{_diagnostic_suffix(run.stderr)}",
        context=f"{phase} ESLint command",
    )


def main(argv: Sequence[str]) -> int:
    args = _parser().parse_args(argv)
    errors = ErrorAccumulator()
    rules = _parse_rules(args.rules, errors)

    try:
        package = Path(args.package).resolve(strict=False)
    except (OSError, RuntimeError, ValueError) as error:
        errors.add(error, context=f"cannot resolve package {args.package!r}")
        return emit_results((), errors)
    if not package.is_dir():
        errors.add("package is not a directory", context=str(package))
    if errors.has_errors:
        return emit_results((), errors)

    initial_run: EslintRun | None = None
    if args.report is not None:
        payload = _load_json_report(Path(args.report), errors)
    else:
        initial_run = _run_eslint(args.eslint_cmd, package, errors, phase="initial")
        payload = None if initial_run is None else initial_run.payload
    if payload is None or errors.has_errors:
        return emit_results((), errors)

    grouped = _collect_violations(payload, package, rules, errors)
    if initial_run is not None and initial_run.returncode != 0:
        expected_lint_exit = initial_run.returncode == 1 and bool(grouped)
        if not expected_lint_exit:
            _command_failed(initial_run, errors, phase="initial")
    if errors.has_errors:
        return emit_results((), errors)

    # A clean injected or live report is a strict no-op. In particular, do not
    # require JavaScript tooling merely to establish that an injected report is empty.
    if not grouped:
        return EXIT_CLEAN

    plans = _build_plans(grouped, rules, errors)
    if errors.has_errors:
        return emit_results((), errors)
    _write_plans(plans, errors)
    if errors.has_errors:
        return emit_results((), errors)

    verification = _run_eslint(args.eslint_cmd, package, errors, phase="verification")
    if verification is None:
        return emit_results((), errors)
    residual = _collect_violations(verification.payload, package, rules, errors)
    for path in sorted(residual, key=lambda item: item.as_posix()):
        for line in sorted(residual[path]):
            for rule in sorted(residual[path][line], key=rules.index):
                errors.add(
                    f"residual Rule_Set violation at {path}:{line}: {rule}",
                    context="post-annotation verification",
                )
    if verification.returncode != 0:
        _command_failed(verification, errors, phase="verification")
    return emit_results((), errors)


if __name__ == "__main__":
    run_cli(main)
