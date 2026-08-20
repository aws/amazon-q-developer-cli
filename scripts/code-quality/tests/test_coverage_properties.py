"""Property tests for coverage computation, exit logic, and report persistence."""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path
from typing import Any, Sequence

TESTS_DIR = Path(__file__).resolve().parent
CODE_QUALITY_DIR = TESTS_DIR.parent
SCRIPT = CODE_QUALITY_DIR / "check-coverage.py"
sys.path.insert(0, str(CODE_QUALITY_DIR))

from helpers import FixtureTree, canned_lcov, run_script  # noqa: E402
from generators import (  # noqa: E402
    HAS_HYPOTHESIS,
    HYPOTHESIS_SKIP_REASON,
    given,
    settings,
)

if HAS_HYPOTHESIS:
    from hypothesis import strategies as st


def _config(filter_keys: Sequence[str]) -> dict[str, object]:
    return {
        "excludeDirectories": [{"name": "generated"}],
        "excludeFiles": [{"name": "ignored.ts"}],
        "excludePatterns": [{"pattern": r"\.test\."}],
        "lcovFilters": {key: [rf"runner-only-{index}/"] for index, key in enumerate(filter_keys)},
    }


def _package_group(
    name: str,
    lcov_inputs: Sequence[tuple[Path, str]],
    suite_results: Sequence[tuple[str, int]],
) -> list[str]:
    result = ["--package", name]
    for path, filter_key in lcov_inputs:
        result.extend(("--lcov", f"{path}:{filter_key}"))
    for suite, exit_code in suite_results:
        result.extend(("--suite-result", f"{suite}={exit_code}"))
    return result


def _invoke(
    tree: FixtureTree,
    floors: dict[str, dict[str, float]],
    config: dict[str, object],
    groups: Sequence[Sequence[str]],
    report_name: str,
):
    floors_path = tree.write_json(f"{report_name}-floors.json", floors)
    config_path = tree.write_json(f"{report_name}-config.json", config)
    report_path = tree.path(f"{report_name}.md")
    args = ["--floors", str(floors_path), "--coverage-config", str(config_path)]
    for group in groups:
        args.extend(group)
    args.extend(("--report-out", str(report_path)))
    return run_script(SCRIPT, *args, cwd=tree.root), report_path


def _row_metrics(report: str, package: str) -> tuple[float, float]:
    header = next(
        line for line in report.splitlines() if line.startswith("| Package |")
    )
    columns = [cell.strip() for cell in header.strip("|").split("|")]
    row = next(line for line in report.splitlines() if line.startswith(f"| {package} |"))
    cells = [cell.strip() for cell in row.strip("|").split("|")]

    def metric(name: str) -> float:
        # Located by header name: the table gains columns over time, and reading by
        # position silently starts measuring the wrong one.
        cell = cells[columns.index(name)]
        match = re.match(r"([0-9]+(?:\.[0-9]+)?)%", cell)
        return float(match.group(1)) if match is not None else 0.0

    return metric("Lines"), metric("Functions")


def _percentage(hits: dict[Any, int]) -> float:
    return 100.0 * sum(value > 0 for value in hits.values()) / len(hits) if hits else 0.0


IncludedFileCoverage = tuple[str, dict[int, int], dict[str, int]]
LcovStreamCase = tuple[
    tuple[IncludedFileCoverage, ...],
    dict[int, int],
    dict[str, int],
]


if HAS_HYPOTHESIS:

    @st.composite
    def lcov_stream_cases(draw):
        stream_count = draw(st.integers(min_value=1, max_value=3))
        file_count = draw(st.integers(min_value=2, max_value=3))
        file_sizes = sorted(
            draw(
                st.lists(
                    st.integers(min_value=2, max_value=8),
                    min_size=file_count,
                    max_size=file_count,
                    unique=True,
                )
            )
        )
        streams: list[LcovStreamCase] = []
        for _index in range(stream_count):
            included_files: list[IncludedFileCoverage] = []
            for file_index, size in enumerate(file_sizes):
                line_values = draw(
                    st.lists(
                        st.integers(min_value=0, max_value=5),
                        min_size=size,
                        max_size=size,
                    )
                )
                function_names = (
                    "shared",
                    *(f"file_{file_index}_function_{position}" for position in range(1, size)),
                )
                function_values = draw(
                    st.lists(
                        st.integers(min_value=0, max_value=5),
                        min_size=size,
                        max_size=size,
                    )
                )
                included_files.append(
                    (
                        f"pkg/src/file-{file_index}.ts",
                        dict(enumerate(line_values, start=1)),
                        dict(zip(function_names, function_values)),
                    )
                )
            excluded_lines = draw(
                st.dictionaries(
                    st.integers(min_value=1, max_value=5),
                    st.integers(min_value=0, max_value=5),
                    max_size=5,
                )
            )
            excluded_functions = draw(
                st.dictionaries(
                    st.sampled_from(("skipA", "skipB", "skipC")),
                    st.integers(min_value=0, max_value=5),
                    max_size=3,
                )
            )
            streams.append((tuple(included_files), excluded_lines, excluded_functions))
        return streams

    @st.composite
    def coverage_law_cases(draw):
        package_count = draw(st.integers(min_value=1, max_value=3))
        packages: list[dict[str, object]] = []
        for index in range(package_count):
            # A declared stream carrying no records is rejected as broken input, so
            # the valid domain for these laws starts at one record of each kind.
            line_hits = draw(st.lists(st.integers(0, 3), min_size=1, max_size=6))
            function_hits = draw(st.lists(st.integers(0, 3), min_size=1, max_size=5))
            packages.append(
                {
                    "name": f"pkg/{index}",
                    "lines": line_hits,
                    "functions": function_hits,
                    "line_floor": draw(st.integers(min_value=0, max_value=100)),
                    "function_floor": draw(st.integers(min_value=0, max_value=100)),
                    "suite_exit": draw(st.integers(min_value=0, max_value=2)),
                }
            )
        return packages

else:
    lcov_stream_cases = None
    coverage_law_cases = None

LCOV_STREAM_CASES = lcov_stream_cases() if HAS_HYPOTHESIS else None
COVERAGE_LAW_CASES = coverage_law_cases() if HAS_HYPOTHESIS else None


class CoverageProperties(unittest.TestCase):
    @unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)
    @settings(max_examples=100, deadline=None)
    @given(LCOV_STREAM_CASES)
    def test_coverage_matches_reference_and_exclusions_are_inert(
        self,
        streams: Sequence[LcovStreamCase],
    ) -> None:
        filter_keys = [f"runner{index}" for index in range(len(streams))]
        merged_lines: dict[tuple[str, int], int] = {}
        merged_functions: dict[tuple[str, str], int] = {}
        for included_files, _excluded_lines, _excluded_functions in streams:
            for source, lines, functions in included_files:
                for line, hits in lines.items():
                    key = (source, line)
                    merged_lines[key] = max(merged_lines.get(key, 0), hits)
                for function, hits in functions.items():
                    key = (source, function)
                    merged_functions[key] = max(merged_functions.get(key, 0), hits)
        expected_lines = round(_percentage(merged_lines), 1)
        expected_functions = round(_percentage(merged_functions), 1)

        with FixtureTree(parent=TESTS_DIR) as tree:
            baseline_inputs: list[tuple[Path, str]] = []
            augmented_inputs: list[tuple[Path, str]] = []
            suites: list[tuple[str, int]] = []
            for index, (included_files, excluded_lines, excluded_functions) in enumerate(streams):
                base = "".join(
                    canned_lcov(
                        source,
                        line_hits=tuple(sorted(lines.items())),
                        functions=tuple(
                            (position + 1, name, hits)
                            for position, (name, hits) in enumerate(sorted(functions.items()))
                        ),
                    )
                    for source, lines, functions in included_files
                )
                excluded_function_records = tuple(
                    (position + 1, name, hits)
                    for position, (name, hits) in enumerate(sorted(excluded_functions.items()))
                )
                excluded_record = canned_lcov(
                    "pkg/src/main.test.ts",
                    line_hits=tuple(sorted(excluded_lines.items())),
                    functions=excluded_function_records,
                )
                excluded_record += canned_lcov(
                    f"runner-only-{index}/skip.ts",
                    line_hits=tuple(sorted(excluded_lines.items())),
                    functions=excluded_function_records,
                )
                excluded_record += canned_lcov(
                    "pkg/src/generated/skip.ts",
                    line_hits=tuple(sorted(excluded_lines.items())),
                    functions=excluded_function_records,
                )
                excluded_record += canned_lcov(
                    "pkg/src/ignored.ts",
                    line_hits=tuple(sorted(excluded_lines.items())),
                    functions=excluded_function_records,
                )
                baseline_path = tree.write_text(f"base-{index}.info", base)
                augmented_path = tree.write_text(f"augmented-{index}.info", base + excluded_record)
                baseline_inputs.append((baseline_path, filter_keys[index]))
                augmented_inputs.append((augmented_path, filter_keys[index]))
                suites.append((f"suite-{index}", 0))

            floors = {"pkg": {"lines": 0.0, "functions": 0.0, "measured": {"lines": 0, "functions": 0}}}
            config = _config(filter_keys)
            baseline, baseline_report_path = _invoke(
                tree,
                floors,
                config,
                (_package_group("pkg", baseline_inputs, suites),),
                "baseline-report",
            )
            augmented, augmented_report_path = _invoke(
                tree,
                floors,
                config,
                (_package_group("pkg", augmented_inputs, suites),),
                "augmented-report",
            )

            self.assertEqual(baseline.exit_code, 0, baseline.stderr)
            self.assertEqual(augmented.exit_code, 0, augmented.stderr)
            baseline_metrics = _row_metrics(
                baseline_report_path.read_text(encoding="utf-8"), "pkg"
            )
            augmented_metrics = _row_metrics(
                augmented_report_path.read_text(encoding="utf-8"), "pkg"
            )
            self.assertEqual(baseline_metrics, (expected_lines, expected_functions))
            self.assertEqual(augmented_metrics, baseline_metrics)

    @unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)
    @settings(max_examples=100, deadline=None)
    @given(COVERAGE_LAW_CASES)
    def test_coverage_exit_code_law_with_suite_failure_override(
        self, packages: Sequence[dict[str, object]]
    ) -> None:
        filter_keys = [f"runner{index}" for index in range(len(packages))]
        floors: dict[str, dict[str, float]] = {}
        expected_failures: list[tuple[str, str, float, float]] = []
        suites_pass = True

        with FixtureTree(parent=TESTS_DIR) as tree:
            groups: list[list[str]] = []
            for index, package in enumerate(packages):
                name = str(package["name"])
                line_hits = list(package["lines"])
                function_hits = list(package["functions"])
                line_floor = float(package["line_floor"])
                function_floor = float(package["function_floor"])
                suite_exit = int(package["suite_exit"])
                measured_lines = 100.0 * sum(hit > 0 for hit in line_hits) / len(line_hits) if line_hits else 0.0
                measured_functions = (
                    100.0 * sum(hit > 0 for hit in function_hits) / len(function_hits)
                    if function_hits
                    else 0.0
                )
                floors[name] = {"lines": line_floor, "functions": function_floor, "measured": {"lines": 0, "functions": 0}}
                if measured_lines < line_floor:
                    expected_failures.append((name, "lines", measured_lines, line_floor))
                if measured_functions < function_floor:
                    expected_failures.append(
                        (name, "functions", measured_functions, function_floor)
                    )
                suites_pass = suites_pass and suite_exit == 0

                lcov_path = tree.write_text(
                    f"package-{index}.info",
                    canned_lcov(
                        f"{name}/src/main.ts",
                        line_hits=tuple(
                            (position + 1, hits) for position, hits in enumerate(line_hits)
                        ),
                        functions=tuple(
                            (position + 1, f"function{position}", hits)
                            for position, hits in enumerate(function_hits)
                        ),
                    ),
                )
                groups.append(
                    _package_group(
                        name,
                        ((lcov_path, filter_keys[index]),),
                        ((f"suite-{index}", suite_exit),),
                    )
                )

            result, _report_path = _invoke(
                tree, floors, _config(filter_keys), groups, "exit-law-report"
            )
            expected_clean = not expected_failures and suites_pass
            self.assertEqual(result.exit_code, 0 if expected_clean else 1, result.stderr)
            for name, metric, measured, floor in expected_failures:
                self.assertIn(name, result.stdout)
                self.assertIn(
                    f"{metric} {measured:.1f}% is below floor {floor:.1f}%",
                    result.stdout,
                )

    @unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)
    @settings(max_examples=100, deadline=None)
    @given(COVERAGE_LAW_CASES)
    def test_report_is_always_written_and_complete(
        self, packages: Sequence[dict[str, object]]
    ) -> None:
        filter_keys = [f"runner{index}" for index in range(len(packages))]
        floors: dict[str, dict[str, float]] = {}

        with FixtureTree(parent=TESTS_DIR) as tree:
            groups: list[list[str]] = []
            expected_rows: dict[str, tuple[float, float, float, float, int, int]] = {}
            for index, package in enumerate(packages):
                name = str(package["name"])
                line_hits = list(package["lines"])
                function_hits = list(package["functions"])
                line_floor = float(package["line_floor"])
                function_floor = float(package["function_floor"])
                suite_exit = int(package["suite_exit"])
                measured_lines = 100.0 * sum(hit > 0 for hit in line_hits) / len(line_hits) if line_hits else 0.0
                measured_functions = (
                    100.0 * sum(hit > 0 for hit in function_hits) / len(function_hits)
                    if function_hits
                    else 0.0
                )
                floors[name] = {"lines": line_floor, "functions": function_floor, "measured": {"lines": 0, "functions": 0}}
                expected_rows[name] = (
                    measured_lines,
                    measured_functions,
                    line_floor,
                    function_floor,
                    len(line_hits),
                    len(function_hits),
                )
                lcov_path = tree.write_text(
                    f"report-package-{index}.info",
                    canned_lcov(
                        f"{name}/src/main.ts",
                        line_hits=tuple(
                            (position + 1, hits) for position, hits in enumerate(line_hits)
                        ),
                        functions=tuple(
                            (position + 1, f"function{position}", hits)
                            for position, hits in enumerate(function_hits)
                        ),
                    ),
                )
                groups.append(
                    _package_group(
                        name,
                        ((lcov_path, filter_keys[index]),),
                        ((f"suite-{index}", suite_exit),),
                    )
                )

            result, report_path = _invoke(
                tree, floors, _config(filter_keys), groups, "always-report"
            )
            self.assertIn(result.exit_code, (0, 1))
            self.assertTrue(report_path.is_file())
            report = report_path.read_text(encoding="utf-8")
            self.assertIn("<!-- code-quality-coverage-report -->", report)
            self.assertIn("Suites:", report)
            self.assertIn("Floors and denominators are committed in", report)
            self.assertIn("Exclusions:", report)
            for name, (
                measured_lines,
                measured_functions,
                line_floor,
                function_floor,
                line_total,
                function_total,
            ) in expected_rows.items():
                row = next(
                    line for line in report.splitlines() if line.startswith(f"| {name} |")
                )
                self.assertIn(f"{line_floor:.1f}%", row)
                self.assertIn(f"{function_floor:.1f}%", row)
                if line_total:
                    self.assertIn(f"{measured_lines:.1f}%", row)
                else:
                    self.assertIn("no measurable coverage data", row)
                if function_total:
                    self.assertIn(f"{measured_functions:.1f}%", row)
                else:
                    self.assertIn("—", row)


if __name__ == "__main__":
    unittest.main()
