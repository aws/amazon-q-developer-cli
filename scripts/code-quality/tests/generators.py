"""Guarded Hypothesis strategies for static-analysis property tests.

Importing this module never requires Hypothesis. Property tests must use
``unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)``; the fallback
``given`` and ``settings`` decorators keep skipped tests importable in bare mode.
"""

from __future__ import annotations

import json
import string
import sys
from dataclasses import dataclass
from typing import Any, Literal, Sequence


def _identity_decorator(*_args: object, **_kwargs: object) -> Any:
    def decorate(function: Any) -> Any:
        return function

    return decorate


try:
    if sys.flags.no_user_site:
        raise ImportError("user site disabled")
    from hypothesis import given, settings
    from hypothesis import strategies as st
except ImportError:
    HAS_HYPOTHESIS = False
    given = _identity_decorator
    settings = _identity_decorator
    st = None
else:
    HAS_HYPOTHESIS = True

HYPOTHESIS_SKIP_REASON = "Hypothesis unavailable (or user site disabled with -s)"

PRODUCTION_INCLUDE_GLOBS = (
    "packages/tui/src/**",
    "packages/twinki/packages/*/src/**",
    "packages/terminal-harness/**",
)
TEST_IGNORE_GLOBS = (
    "**/__tests__/**",
    "**/*.test.*",
    "**/*.spec.*",
    "**/*.vitest.*",
    "**/tests/**",
    "**/test-utils/**",
    "**/test_fixtures/**",
)
RULE_SET = (
    "complexity",
    "max-depth",
    "max-params",
    "sonarjs/cognitive-complexity",
)

DisableDisposition = Literal["justified", "unjustified", "malformed"]


@dataclass(frozen=True, slots=True)
class LintDebtNearMiss:
    comment: str
    mutation: str


@dataclass(frozen=True, slots=True)
class DisableSite:
    path: str
    disposition: DisableDisposition
    rules: tuple[str, ...]
    comment_lines: tuple[int, ...]
    directive_line: int
    target_line: int


@dataclass(frozen=True, slots=True)
class SyntheticProductionTree:
    files: tuple[tuple[str, str], ...]
    include_globs: tuple[str, ...]
    ignore_globs: tuple[str, ...]
    sites: tuple[DisableSite, ...]

    @property
    def file_map(self) -> dict[str, str]:
        return dict(self.files)

    @property
    def justified_debt_count(self) -> int:
        return sum(len(site.rules) for site in self.sites if site.disposition == "justified")


@dataclass(frozen=True, slots=True)
class JscpdReportCase:
    report: dict[str, object]
    production_clone_count: int
    excluded_clone_count: int


@dataclass(frozen=True, slots=True)
class LcovStreamsCase:
    streams: tuple[str, ...]
    included_files: tuple[str, ...]
    excluded_files: tuple[str, ...]
    expected_line_hits: tuple[tuple[str, int, int], ...]
    expected_function_hits: tuple[tuple[str, str, int], ...]


@dataclass(frozen=True, slots=True)
class BaselineDocument:
    key: str
    value: int
    text: str


@dataclass(frozen=True, slots=True)
class FloorDocument:
    floors: tuple[tuple[str, float, float], ...]
    text: str


@dataclass(frozen=True, slots=True)
class FuzzedJsonDocument:
    text: str
    mutation: str


def _require_hypothesis() -> Any:
    if st is None:
        raise RuntimeError(HYPOTHESIS_SKIP_REASON)
    return st


def lint_debt_rule_ids() -> Any:
    """Generate non-empty identifiers accepted by the LINT-DEBT grammar."""

    strategies = _require_hypothesis()
    alphabet = string.ascii_letters + string.digits + "@/_-"
    return strategies.text(alphabet=alphabet, min_size=1, max_size=48)


def lint_debt_reasons() -> Any:
    """Generate non-empty single-line reasons without lossy leading whitespace."""

    strategies = _require_hypothesis()
    visible = strategies.characters(whitelist_categories=("L", "M", "N", "P", "S"))
    tail = strategies.one_of(visible, strategies.sampled_from((" ", "\t")))
    return strategies.builds(
        lambda first, rest: first + rest,
        visible,
        strategies.text(alphabet=tail, min_size=0, max_size=96),
    )


def valid_lint_debt_components() -> Any:
    """Generate ``(rule, reason)`` pairs in the parsed grammar domain."""

    strategies = _require_hypothesis()
    return strategies.tuples(lint_debt_rule_ids(), lint_debt_reasons())


def valid_lint_debt_comments() -> Any:
    """Generate canonical valid comments independently of the production formatter."""

    return valid_lint_debt_components().map(
        lambda parts: f"// LINT-DEBT({parts[0]}): {parts[1]}"
    )


def lint_debt_near_miss_cases() -> Any:
    """Generate one-edit-like malformed comments that the parser must reject."""

    strategies = _require_hypothesis()

    @strategies.composite
    def cases(draw: Any) -> LintDebtNearMiss:
        rule, reason = draw(valid_lint_debt_components())
        mutation = draw(
            strategies.sampled_from(
                (
                    "wrong-keyword",
                    "missing-open-parenthesis",
                    "missing-close-parenthesis",
                    "empty-rule",
                    "whitespace-rule",
                    "invalid-rule-character",
                    "missing-colon",
                    "empty-reason",
                    "whitespace-reason",
                    "block-comment",
                    "multiple-lines",
                )
            )
        )
        mutations = {
            "wrong-keyword": f"// LINT-DEBTT({rule}): {reason}",
            "missing-open-parenthesis": f"// LINT-DEBT{rule}): {reason}",
            "missing-close-parenthesis": f"// LINT-DEBT({rule}: {reason}",
            "empty-rule": f"// LINT-DEBT(): {reason}",
            "whitespace-rule": f"// LINT-DEBT(   ): {reason}",
            "invalid-rule-character": f"// LINT-DEBT({rule}.invalid): {reason}",
            "missing-colon": f"// LINT-DEBT({rule}) {reason}",
            "empty-reason": f"// LINT-DEBT({rule}):",
            "whitespace-reason": f"// LINT-DEBT({rule}):   \t",
            "block-comment": f"/* LINT-DEBT({rule}) {reason} */",
            "multiple-lines": f"// LINT-DEBT({rule}): {reason}\n// continuation",
        }
        return LintDebtNearMiss(mutations[mutation], mutation)

    return cases()


def lint_debt_near_miss_comments() -> Any:
    return lint_debt_near_miss_cases().map(lambda case: case.comment)


def production_paths() -> Any:
    """Generate paths that match one of the authoritative production include globs."""

    strategies = _require_hypothesis()
    segment = strategies.text(
        alphabet=string.ascii_lowercase + string.digits + "_-",
        min_size=1,
        max_size=12,
    ).filter(lambda value: value not in {"tests", "__tests__", "test-utils", "test_fixtures"})
    nesting = strategies.lists(segment, min_size=0, max_size=3)
    stem = strategies.text(
        alphabet=string.ascii_lowercase + string.digits + "_-",
        min_size=1,
        max_size=16,
    )
    extension = strategies.sampled_from((".ts", ".tsx", ".js"))

    @strategies.composite
    def paths(draw: Any) -> str:
        package = draw(strategies.sampled_from(("tui", "twinki", "terminal-harness")))
        directories = draw(nesting)
        filename = draw(stem) + draw(extension)
        if package == "tui":
            prefix = ["packages", "tui", "src"]
        elif package == "twinki":
            workspace = draw(segment)
            prefix = ["packages", "twinki", "packages", workspace, "src"]
        else:
            prefix = ["packages", "terminal-harness"]
        return "/".join((*prefix, *directories, filename))

    return paths()


def test_paths() -> Any:
    """Generate TypeScript paths planted under an authoritative test exclusion."""

    strategies = _require_hypothesis()
    stem = strategies.text(
        alphabet=string.ascii_lowercase + string.digits + "_-",
        min_size=1,
        max_size=16,
    )

    @strategies.composite
    def paths(draw: Any) -> str:
        name = draw(stem)
        layout = draw(strategies.integers(min_value=0, max_value=3))
        if layout == 0:
            return f"packages/tui/src/__tests__/{name}.ts"
        if layout == 1:
            return f"packages/tui/src/{name}.test.ts"
        if layout == 2:
            return f"packages/twinki/packages/core/src/tests/{name}.spec.ts"
        return f"packages/terminal-harness/tests/{name}.ts"

    return paths()


def disable_directive_forms() -> Any:
    return _require_hypothesis().sampled_from(
        ("line-next", "line-inline", "block-next", "block-scope")
    )


def _render_directive(form: str, rules: Sequence[str], index: int) -> tuple[str, str | None]:
    rule_list = ", ".join(rules)
    code = f"const generated{index} = input{index};"
    if form == "line-next":
        return f"// eslint-disable-next-line {rule_list}", code
    if form == "line-inline":
        return f"{code} // eslint-disable-line {rule_list}", None
    if form == "block-next":
        return f"/* eslint-disable-next-line {rule_list} */", code
    return f"/* eslint-disable {rule_list} */", f"{code}\n/* eslint-enable {rule_list} */"


def synthetic_production_trees(
    *,
    required_disposition: DisableDisposition | None = None,
    site_dispositions: Sequence[DisableDisposition] = (
        "justified",
        "unjustified",
        "malformed",
    ),
) -> Any:
    """Generate source mappings with tracked disable and comment line positions."""

    strategies = _require_hypothesis()
    known_dispositions = {"justified", "unjustified", "malformed"}
    if required_disposition not in {None, *known_dispositions}:
        raise ValueError(f"unsupported disposition: {required_disposition!r}")
    if not site_dispositions or any(item not in known_dispositions for item in site_dispositions):
        raise ValueError(f"unsupported site dispositions: {site_dispositions!r}")
    generated_dispositions = tuple(site_dispositions)

    @strategies.composite
    def trees(draw: Any) -> SyntheticProductionTree:
        paths = draw(strategies.lists(production_paths(), min_size=1, max_size=4, unique=True))
        files: list[tuple[str, str]] = []
        all_sites: list[DisableSite] = []
        site_index = 0
        for file_index, path in enumerate(paths):
            lines = [f"export const file{file_index} = true;"]
            site_count = draw(strategies.integers(min_value=1, max_value=3))
            for local_index in range(site_count):
                padding = draw(strategies.integers(min_value=0, max_value=3))
                lines.extend(f"const padding{site_index}_{offset} = {offset};" for offset in range(padding))
                disposition: DisableDisposition
                if file_index == 0 and local_index == 0 and required_disposition is not None:
                    disposition = required_disposition
                else:
                    disposition = draw(strategies.sampled_from(generated_dispositions))

                if disposition == "malformed":
                    rules = (draw(lint_debt_rule_ids()),)
                else:
                    rules = tuple(
                        draw(
                            strategies.lists(
                                lint_debt_rule_ids(), min_size=1, max_size=3, unique=True
                            )
                        )
                    )

                comment_lines: list[int] = []
                if disposition == "justified":
                    for rule in rules:
                        reason = draw(lint_debt_reasons())
                        comment_lines.append(len(lines) + 1)
                        lines.append(f"// LINT-DEBT({rule}): {reason}")
                elif disposition == "malformed":
                    comment_lines.append(len(lines) + 1)
                    if draw(strategies.booleans()):
                        lines.append(f"// LINT-DEBT({rules[0]}):    ")
                    else:
                        other_rule = draw(lint_debt_rule_ids().filter(lambda item: item != rules[0]))
                        lines.append(
                            f"// LINT-DEBT({other_rule}): justification names the wrong rule"
                        )

                directive_line = len(lines) + 1
                form = draw(disable_directive_forms())
                directive, following = _render_directive(form, rules, site_index)
                lines.append(directive)
                target_line = directive_line
                if following is not None:
                    target_line += 1
                    lines.extend(following.splitlines())
                all_sites.append(
                    DisableSite(
                        path=path,
                        disposition=disposition,
                        rules=rules,
                        comment_lines=tuple(comment_lines),
                        directive_line=directive_line,
                        target_line=target_line,
                    )
                )
                site_index += 1
            files.append((path, "\n".join(lines) + "\n"))

        return SyntheticProductionTree(
            files=tuple(files),
            include_globs=PRODUCTION_INCLUDE_GLOBS,
            ignore_globs=TEST_IGNORE_GLOBS,
            sites=tuple(all_sites),
        )

    return trees()


def fully_justified_production_trees() -> Any:
    """Generate trees in which every planted disable is justified."""

    return synthetic_production_trees(
        required_disposition="justified",
        site_dispositions=("justified",),
    )


def production_trees_with_unjustified_disables() -> Any:
    return synthetic_production_trees(
        required_disposition="unjustified",
        site_dispositions=("justified",),
    )


def production_trees_with_malformed_disables() -> Any:
    return synthetic_production_trees(
        required_disposition="malformed",
        site_dispositions=("justified",),
    )


def _clone(first: str, second: str, start: int, lines: int) -> dict[str, object]:
    return {
        "format": "typescript",
        "lines": lines,
        "firstFile": {"name": first, "start": start, "end": start + lines - 1},
        "secondFile": {"name": second, "start": start + 1, "end": start + lines},
    }


def jscpd_report_cases() -> Any:
    """Generate reports containing production clones and clones touching test paths."""

    strategies = _require_hypothesis()

    @strategies.composite
    def reports(draw: Any) -> JscpdReportCase:
        production_count = draw(strategies.integers(min_value=0, max_value=8))
        excluded_count = draw(strategies.integers(min_value=1, max_value=8))
        duplicates: list[dict[str, object]] = []
        for index in range(production_count):
            duplicates.append(
                _clone(
                    draw(production_paths()),
                    draw(production_paths()),
                    draw(strategies.integers(min_value=1, max_value=200)),
                    draw(strategies.integers(min_value=1, max_value=60)),
                )
            )
        for index in range(excluded_count):
            production = draw(production_paths())
            excluded = draw(test_paths())
            endpoints = (production, excluded) if index % 2 == 0 else (excluded, production)
            duplicates.append(
                _clone(
                    endpoints[0],
                    endpoints[1],
                    draw(strategies.integers(min_value=1, max_value=200)),
                    draw(strategies.integers(min_value=1, max_value=60)),
                )
            )
        report: dict[str, object] = {
            "statistics": {
                "total": {
                    "clones": len(duplicates),
                    "duplicatedLines": sum(int(clone["lines"]) for clone in duplicates),
                }
            },
            "duplicates": duplicates,
        }
        return JscpdReportCase(report, production_count, excluded_count)

    return reports()


def jscpd_json_reports() -> Any:
    return jscpd_report_cases().map(lambda case: case.report)


def _lcov_record(
    path: str,
    line_hits: Sequence[tuple[int, int]],
    function_hits: Sequence[tuple[int, str, int]],
) -> list[str]:
    rows = [f"SF:{path}"]
    rows.extend(f"FN:{line},{name}" for line, name, _hits in function_hits)
    rows.extend(f"FNDA:{hits},{name}" for _line, name, hits in function_hits)
    rows.append(f"FNF:{len(function_hits)}")
    rows.append(f"FNH:{sum(hits > 0 for _line, _name, hits in function_hits)}")
    rows.extend(f"DA:{line},{hits}" for line, hits in line_hits)
    rows.append(f"LF:{len(line_hits)}")
    rows.append(f"LH:{sum(hits > 0 for _line, hits in line_hits)}")
    rows.append("end_of_record")
    return rows


def lcov_stream_cases() -> Any:
    """Generate two overlapping streams plus records for a planted excluded file."""

    strategies = _require_hypothesis()

    @strategies.composite
    def cases(draw: Any) -> LcovStreamsCase:
        included = draw(production_paths())
        excluded = draw(test_paths())
        line_numbers = draw(
            strategies.lists(
                strategies.integers(min_value=1, max_value=100),
                min_size=2,
                max_size=8,
                unique=True,
            )
        )
        function_names = draw(
            strategies.lists(
                strategies.text(
                    alphabet=string.ascii_letters + string.digits + "_",
                    min_size=1,
                    max_size=16,
                ),
                min_size=1,
                max_size=5,
                unique=True,
            )
        )
        first_line_hits = [
            (line, draw(strategies.integers(min_value=0, max_value=20)))
            for line in line_numbers
        ]
        second_line_hits = [
            (line, draw(strategies.integers(min_value=0, max_value=20)))
            for line in reversed(line_numbers)
        ]
        first_function_hits = [
            (
                line_numbers[index % len(line_numbers)],
                name,
                draw(strategies.integers(min_value=0, max_value=20)),
            )
            for index, name in enumerate(function_names)
        ]
        second_function_hits = [
            (
                line_numbers[index % len(line_numbers)],
                name,
                draw(strategies.integers(min_value=0, max_value=20)),
            )
            for index, name in enumerate(reversed(function_names))
        ]
        excluded_lines = [(1, draw(strategies.integers(min_value=0, max_value=20)))]
        excluded_functions = [(1, "excluded", draw(strategies.integers(min_value=0, max_value=20)))]
        first_rows = ["TN:generated-first"]
        first_rows.extend(_lcov_record(included, first_line_hits, first_function_hits))
        first_rows.extend(_lcov_record(excluded, excluded_lines, excluded_functions))
        second_rows = ["TN:generated-second"]
        second_rows.extend(_lcov_record(included, second_line_hits, second_function_hits))
        second_rows.extend(_lcov_record(excluded, excluded_lines, excluded_functions))

        first_function_map = {name: hits for _line, name, hits in first_function_hits}
        second_function_map = {name: hits for _line, name, hits in second_function_hits}
        expected_lines = tuple(
            (included, line, max(dict(first_line_hits)[line], dict(second_line_hits)[line]))
            for line in sorted(line_numbers)
        )
        expected_functions = tuple(
            (included, name, max(first_function_map[name], second_function_map[name]))
            for name in sorted(function_names)
        )
        return LcovStreamsCase(
            streams=("\n".join(first_rows) + "\n", "\n".join(second_rows) + "\n"),
            included_files=(included,),
            excluded_files=(excluded,),
            expected_line_hits=expected_lines,
            expected_function_hits=expected_functions,
        )

    return cases()


def lcov_streams() -> Any:
    return lcov_stream_cases().map(lambda case: case.streams)


def valid_baseline_documents(keys: Sequence[str] = ("count", "clones")) -> Any:
    """Generate strict one-key debt or duplication baseline documents."""

    strategies = _require_hypothesis()
    if not keys:
        raise ValueError("at least one baseline key is required")
    return strategies.builds(
        lambda key, value: BaselineDocument(
            key=key,
            value=value,
            text=json.dumps({key: value}, separators=(",", ":")),
        ),
        strategies.sampled_from(tuple(keys)),
        strategies.integers(min_value=0, max_value=1_000_000),
    )


def fuzzed_invalid_baseline_documents(keys: Sequence[str] = ("count", "clones")) -> Any:
    """Generate malformed JSON and strict-shape/value baseline failures."""

    strategies = _require_hypothesis()
    if not keys:
        raise ValueError("at least one baseline key is required")

    @strategies.composite
    def documents(draw: Any) -> FuzzedJsonDocument:
        key = draw(strategies.sampled_from(tuple(keys)))
        value = draw(strategies.integers(min_value=0, max_value=1_000_000))
        mutation = draw(
            strategies.sampled_from(
                (
                    "truncated-json",
                    "trailing-garbage",
                    "wrong-root",
                    "missing-key",
                    "extra-key",
                    "negative",
                    "boolean",
                    "fractional",
                    "string",
                    "null",
                )
            )
        )
        payloads: dict[str, str] = {
            "truncated-json": f'{{"{key}":',
            "trailing-garbage": json.dumps({key: value}) + " trailing",
            "wrong-root": json.dumps([value]),
            "missing-key": json.dumps({"other": value}),
            "extra-key": json.dumps({key: value, "other": 0}),
            "negative": json.dumps({key: -max(1, value)}),
            "boolean": json.dumps({key: True}),
            "fractional": json.dumps({key: value + 0.5}),
            "string": json.dumps({key: str(value)}),
            "null": json.dumps({key: None}),
        }
        return FuzzedJsonDocument(payloads[mutation], mutation)

    return documents()


def valid_baseline_json(keys: Sequence[str] = ("count", "clones")) -> Any:
    return valid_baseline_documents(keys).map(lambda document: document.text)


def fuzzed_invalid_baseline_json(keys: Sequence[str] = ("count", "clones")) -> Any:
    return fuzzed_invalid_baseline_documents(keys).map(lambda document: document.text)


def valid_floor_documents() -> Any:
    """Generate strict per-package line/function floor JSON documents."""

    strategies = _require_hypothesis()
    packages = (
        "packages/tui",
        "packages/twinki",
        "packages/terminal-harness",
    )
    percentage = strategies.integers(min_value=0, max_value=1000).map(lambda value: value / 10)

    @strategies.composite
    def documents(draw: Any) -> FloorDocument:
        selected = draw(
            strategies.lists(
                strategies.sampled_from(packages), min_size=1, max_size=len(packages), unique=True
            )
        )
        floors: list[tuple[str, float, float]] = []
        payload: dict[str, dict[str, float]] = {}
        for package in selected:
            lines = draw(percentage)
            functions = draw(percentage)
            floors.append((package, lines, functions))
            payload[package] = {"lines": lines, "functions": functions, "measured": {"lines": 0, "functions": 0}}
        return FloorDocument(tuple(floors), json.dumps(payload, separators=(",", ":")))

    return documents()


def fuzzed_invalid_floor_documents() -> Any:
    """Generate malformed and strict-shape/value floor JSON failures."""

    strategies = _require_hypothesis()

    @strategies.composite
    def documents(draw: Any) -> FuzzedJsonDocument:
        package = draw(
            strategies.sampled_from(
                ("packages/tui", "packages/twinki", "packages/terminal-harness")
            )
        )
        mutation = draw(
            strategies.sampled_from(
                (
                    "truncated-json",
                    "wrong-root",
                    "empty-root",
                    "missing-lines",
                    "missing-functions",
                    "extra-field",
                    "negative-lines",
                    "negative-functions",
                    "boolean",
                    "string",
                    "null-package",
                )
            )
        )
        payloads: dict[str, str] = {
            "truncated-json": f'{{"{package}":',
            "wrong-root": json.dumps([]),
            "empty-root": json.dumps({}),
            "missing-lines": json.dumps({package: {"functions": 50.0}}),
            "missing-functions": json.dumps({package: {"lines": 50.0}}),
            "extra-field": json.dumps(
                {package: {"lines": 50.0, "functions": 50.0, "branches": 50.0}}
            ),
            "negative-lines": json.dumps({package: {"lines": -0.1, "functions": 50.0, "measured": {"lines": 0, "functions": 0}}}),
            "negative-functions": json.dumps({package: {"lines": 50.0, "functions": -0.1, "measured": {"lines": 0, "functions": 0}}}),
            "boolean": json.dumps({package: {"lines": True, "functions": 50.0, "measured": {"lines": 0, "functions": 0}}}),
            "string": json.dumps({package: {"lines": "50", "functions": 50.0, "measured": {"lines": 0, "functions": 0}}}),
            "null-package": json.dumps({package: None}),
        }
        return FuzzedJsonDocument(payloads[mutation], mutation)

    return documents()


def valid_floor_json() -> Any:
    return valid_floor_documents().map(lambda document: document.text)


def fuzzed_invalid_floor_json() -> Any:
    return fuzzed_invalid_floor_documents().map(lambda document: document.text)


__all__ = [
    "BaselineDocument",
    "DisableSite",
    "FloorDocument",
    "FuzzedJsonDocument",
    "HAS_HYPOTHESIS",
    "HYPOTHESIS_SKIP_REASON",
    "JscpdReportCase",
    "LcovStreamsCase",
    "LintDebtNearMiss",
    "PRODUCTION_INCLUDE_GLOBS",
    "RULE_SET",
    "SyntheticProductionTree",
    "TEST_IGNORE_GLOBS",
    "disable_directive_forms",
    "fully_justified_production_trees",
    "fuzzed_invalid_baseline_documents",
    "fuzzed_invalid_baseline_json",
    "fuzzed_invalid_floor_documents",
    "fuzzed_invalid_floor_json",
    "given",
    "jscpd_json_reports",
    "jscpd_report_cases",
    "lcov_stream_cases",
    "lcov_streams",
    "lint_debt_near_miss_cases",
    "lint_debt_near_miss_comments",
    "lint_debt_reasons",
    "lint_debt_rule_ids",
    "production_paths",
    "production_trees_with_malformed_disables",
    "production_trees_with_unjustified_disables",
    "settings",
    "synthetic_production_trees",
    "test_paths",
    "valid_baseline_documents",
    "valid_baseline_json",
    "valid_floor_documents",
    "valid_floor_json",
    "valid_lint_debt_comments",
    "valid_lint_debt_components",
]
