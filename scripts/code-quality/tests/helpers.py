"""Standard-library fixtures shared by code-quality script tests."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from types import TracebackType
from typing import Any, Mapping, Sequence


class FixtureTree:
    """Build a temporary file tree and remove it when the context exits."""

    def __init__(
        self,
        files: Mapping[str, str | bytes] | None = None,
        *,
        parent: str | os.PathLike[str] | None = None,
    ) -> None:
        self._initial_files = dict(files or {})
        self._parent = None if parent is None else Path(parent)
        self._temporary_directory: tempfile.TemporaryDirectory[str] | None = None
        self._root: Path | None = None

    def __enter__(self) -> "FixtureTree":
        if self._parent is None:
            self._temporary_directory = tempfile.TemporaryDirectory()
        else:
            self._temporary_directory = tempfile.TemporaryDirectory(
                prefix="code-quality-test-",
                dir=self._parent,
            )
        self._root = Path(self._temporary_directory.name)
        for relative_path, content in self._initial_files.items():
            if isinstance(content, bytes):
                self.write_bytes(relative_path, content)
            else:
                self.write_text(relative_path, content)
        return self

    def __exit__(
        self,
        exception_type: type[BaseException] | None,
        exception: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        if self._temporary_directory is not None:
            self._temporary_directory.cleanup()
        self._temporary_directory = None
        self._root = None

    @property
    def root(self) -> Path:
        if self._root is None:
            raise RuntimeError("FixtureTree must be entered before use")
        return self._root

    def path(self, relative_path: str | os.PathLike[str]) -> Path:
        relative = Path(relative_path)
        if relative.is_absolute():
            raise ValueError("fixture paths must be relative")
        candidate = (self.root / relative).resolve(strict=False)
        try:
            candidate.relative_to(self.root.resolve())
        except ValueError as error:
            raise ValueError("fixture path cannot escape the temporary tree") from error
        return candidate

    def write_text(self, relative_path: str | os.PathLike[str], content: str) -> Path:
        target = self.path(relative_path)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
        return target

    def write_bytes(self, relative_path: str | os.PathLike[str], content: bytes) -> Path:
        target = self.path(relative_path)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
        return target

    def write_json(self, relative_path: str | os.PathLike[str], payload: Any) -> Path:
        return self.write_text(relative_path, json.dumps(payload, indent=2) + "\n")


def canned_eslint_message(
    rule_id: str = "complexity",
    *,
    line: int = 1,
    message: str = "Function has a complexity of 16. Maximum allowed is 15.",
    severity: int = 2,
) -> dict[str, object]:
    return {
        "ruleId": rule_id,
        "severity": severity,
        "line": line,
        "column": 1,
        "message": message,
    }


def canned_eslint_report(
    file_path: str = "packages/tui/src/example.ts",
    messages: Sequence[Mapping[str, object]] | None = None,
) -> list[dict[str, object]]:
    canned_messages = [
        dict(message)
        for message in (
            [canned_eslint_message()] if messages is None else messages
        )
    ]
    return [
        {
            "filePath": file_path,
            "messages": canned_messages,
            "errorCount": sum(message.get("severity") == 2 for message in canned_messages),
            "warningCount": sum(message.get("severity") == 1 for message in canned_messages),
        }
    ]


def canned_jscpd_report(
    clone_pairs: Sequence[tuple[str, str]] = (),
    *,
    lines_per_clone: int = 10,
) -> dict[str, object]:
    duplicates: list[dict[str, object]] = []
    for first_path, second_path in clone_pairs:
        duplicates.append(
            {
                "format": "typescript",
                "lines": lines_per_clone,
                "firstFile": {"name": first_path, "start": 1, "end": lines_per_clone},
                "secondFile": {"name": second_path, "start": 1, "end": lines_per_clone},
            }
        )
    return {
        "statistics": {
            "total": {
                "clones": len(duplicates),
                "duplicatedLines": len(duplicates) * lines_per_clone,
            }
        },
        "duplicates": duplicates,
    }


def canned_lcov(
    source: str = "packages/tui/src/example.ts",
    *,
    line_hits: Sequence[tuple[int, int]] = ((1, 1), (2, 0)),
    functions: Sequence[tuple[int, str, int]] = ((1, "example", 1),),
    emit_function_records: bool = True,
) -> str:
    """Render one lcov section.

    With ``emit_function_records`` disabled only the FNF/FNH totals are written,
    reproducing generators that report how many functions a file has without
    naming any of them.
    """

    lines = ["TN:", f"SF:{source}"]
    if emit_function_records:
        for line_number, name, _hits in functions:
            lines.append(f"FN:{line_number},{name}")
        for _line_number, name, hits in functions:
            lines.append(f"FNDA:{hits},{name}")
    lines.extend(
        [
            f"FNF:{len(functions)}",
            f"FNH:{sum(hits > 0 for _line, _name, hits in functions)}",
        ]
    )
    for line_number, hits in line_hits:
        lines.append(f"DA:{line_number},{hits}")
    lines.extend(
        [
            f"LF:{len(line_hits)}",
            f"LH:{sum(hits > 0 for _line, hits in line_hits)}",
            "end_of_record",
        ]
    )
    return "\n".join(lines) + "\n"


@dataclass(frozen=True, slots=True)
class ScriptResult:
    argv: tuple[str, ...]
    returncode: int
    stdout: str
    stderr: str

    @property
    def exit_code(self) -> int:
        return self.returncode


def run_script(
    script_path: str | os.PathLike[str],
    *args: str | os.PathLike[str],
    cwd: str | os.PathLike[str] | None = None,
    env: Mapping[str, str] | None = None,
    input_text: str | None = None,
    timeout: float = 10.0,
) -> ScriptResult:
    """Invoke a Python script by file path with this interpreter and capture output."""

    argv = (sys.executable, os.fspath(script_path), *(os.fspath(argument) for argument in args))
    child_env = None if env is None else {**os.environ, **env}
    completed = subprocess.run(
        argv,
        cwd=cwd,
        env=child_env,
        input=input_text,
        text=True,
        capture_output=True,
        check=False,
        timeout=timeout,
    )
    return ScriptResult(
        argv=argv,
        returncode=completed.returncode,
        stdout=completed.stdout,
        stderr=completed.stderr,
    )


__all__ = [
    "FixtureTree",
    "ScriptResult",
    "canned_eslint_message",
    "canned_eslint_report",
    "canned_jscpd_report",
    "canned_lcov",
    "run_script",
]
