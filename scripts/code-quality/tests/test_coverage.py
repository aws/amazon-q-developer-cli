"""Standard-library tests for the per-package coverage checker."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Iterable, Sequence

TESTS_DIR = Path(__file__).resolve().parent
CODE_QUALITY_DIR = TESTS_DIR.parent
SCRIPT = CODE_QUALITY_DIR / "check-coverage.py"
sys.path.insert(0, str(CODE_QUALITY_DIR))

from helpers import FixtureTree, canned_lcov, run_script  # noqa: E402


def coverage_config(*filter_keys: str) -> dict[str, object]:
    return {
        "excludeDirectories": [{"name": "generated", "reason": "fixture"}],
        "excludeFiles": [{"name": "ignored.ts", "reason": "fixture"}],
        "excludePatterns": [{"pattern": r"\.test\.", "reason": "fixture"}],
        "lcovFilters": {key: [] for key in filter_keys},
    }


def package_args(
    name: str,
    *,
    lcov: Sequence[tuple[Path, str]] = (),
    suites: Sequence[tuple[str, int]] = (),
) -> list[str]:
    args = ["--package", name]
    for path, filter_key in lcov:
        args.extend(("--lcov", f"{path}:{filter_key}"))
    for suite_name, exit_code in suites:
        args.extend(("--suite-result", f"{suite_name}={exit_code}"))
    return args


def run_case(
    tree: FixtureTree,
    floors: object,
    config: object,
    groups: Iterable[Sequence[str]],
    *,
    extra_args: Sequence[str] = (),
):
    floors_path = tree.write_json("floors.json", floors)
    config_path = tree.write_json("coverage-config.json", config)
    report_path = tree.path("coverage-report.md")
    args = [
        "--floors",
        str(floors_path),
        "--coverage-config",
        str(config_path),
    ]
    for group in groups:
        args.extend(group)
    args.extend(extra_args)
    args.extend(("--report-out", str(report_path)))
    return run_script(SCRIPT, *args, cwd=tree.root), report_path


def expected_row(
    package: str,
    lines: str,
    line_floor: str,
    functions: str,
    function_floor: str,
    status: str,
) -> str:
    """Render an expected report row, deriving each Δ from the cells beside it.

    Keeps table layout in one place, so a column change does not touch every
    assertion in the file.
    """

    def delta(measured: str, floor: str) -> str:
        if not measured.endswith("%") or not floor.endswith("%"):
            return "—"
        return f"{float(measured[:-1]) - float(floor[:-1]):+.1f}"

    return (
        f"| {package} | {lines} | {line_floor} | {delta(lines, line_floor)} "
        f"| {functions} | {function_floor} | {delta(functions, function_floor)} "
        f"| {status} |"
    )


def assert_complete_report(
    test: unittest.TestCase,
    report_path: Path,
    packages: Sequence[str],
) -> str:
    test.assertTrue(report_path.is_file())
    report = report_path.read_text(encoding="utf-8")
    test.assertIn("<!-- code-quality-coverage-report -->", report)
    test.assertIn("## Code Quality — Coverage Report", report)
    test.assertIn(
        "| Package | Lines | Floor | Δ | Functions | Floor | Δ | Status |", report
    )
    test.assertIn("Suites:", report)
    test.assertIn("Denominators:", report)
    test.assertIn("Floors and denominators are committed in", report)
    test.assertIn("Exclusions:", report)
    for package in packages:
        test.assertIn(f"| {package} |", report)
    return report


class CoverageCheckerStdlibTests(unittest.TestCase):
    def test_empty_package_set_is_an_operational_error_and_writes_report(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            result, report_path = run_case(tree, {}, coverage_config(), ())

            self.assertEqual(result.exit_code, 2)
            self.assertIn("no packages were provided", result.stderr)
            report = assert_complete_report(self, report_path, ())
            self.assertIn("Operational errors:", report)
            self.assertIn("no packages were provided", report)

    def test_floors_and_cli_package_sets_must_match_both_ways(self) -> None:
        cases = (
            (
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                (package_args("pkg/b"),),
                "CLI packages missing floors entries: pkg/b",
            ),
            (
                {
                    "pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}},
                    "pkg/b": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}},
                },
                (package_args("pkg/a"),),
                "floors packages missing CLI --package groups: pkg/b",
            ),
        )
        for floors, groups, cause in cases:
            with self.subTest(cause=cause), FixtureTree(parent=TESTS_DIR) as tree:
                result, report_path = run_case(tree, floors, coverage_config(), groups)
                self.assertEqual(result.exit_code, 2)
                self.assertIn(cause, result.stderr)
                self.assertIn(cause, assert_complete_report(self, report_path, tuple(floors)))

    def test_all_floors_met_and_all_suites_passing_exits_zero(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1), (2, 0)),
                    functions=((1, "covered", 1), (2, "missed", 0)),
                ),
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 50, "functions": 50, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            # A passing run must publish its readings, or the margin is invisible
            # until the build reds.
            self.assertIn(
                "measured: coverage pkg/a lines = 50.0% (limit 50.0%, headroom +0.0)",
                result.stdout,
            )
            self.assertIn(
                "measured: coverage pkg/a functions = 50.0% (limit 50.0%, headroom +0.0)",
                result.stdout,
            )
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn(expected_row(
                    "pkg/a", "50.0%", "50.0%",
                    "50.0%", "50.0%", "✅"
                ), report)
            self.assertIn("Suites: unit ✅", report)
            self.assertNotIn("Operational errors:", report)

    def test_other_gates_readings_are_published_in_the_report(self) -> None:
        # The comment is the only surface a reviewer sees; readings that live only
        # in the job log leave a green run unauditable without opening CI.
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1),),
                    functions=((1, "covered", 1),),
                ),
            )
            readings = tree.write_json(
                "debt-readings.json",
                [
                    {
                        "gate": "lint-debt",
                        "subject": "complexity",
                        "measured": "83",
                        "limit": "83",
                        "headroom": "0",
                    },
                    {
                        "gate": "duplication",
                        "subject": "clones",
                        "measured": "29",
                        "limit": "29",
                        "headroom": "0",
                    },
                ],
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 50, "functions": 50, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
                extra_args=("--gate-readings", str(readings)),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = report_path.read_text(encoding="utf-8")
            self.assertIn("Other gates:", report)
            self.assertIn("lint-debt complexity 83/83", report)
            self.assertIn("duplication clones 29/29", report)

    def test_unreported_gate_readings_are_named_without_failing_coverage(self) -> None:
        # A crashed sibling gate must not turn the coverage verdict, but its
        # silence has to be visible rather than read as a clean sheet.
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1),),
                    functions=((1, "covered", 1),),
                ),
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 50, "functions": 50, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
                extra_args=("--gate-readings", str(tree.path("absent.json"))),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            self.assertIn("Readings not reported by:", report_path.read_text(encoding="utf-8"))

    def test_malformed_gate_readings_are_an_operational_error(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1),),
                    functions=((1, "covered", 1),),
                ),
            )
            readings = tree.write_json("bad-readings.json", {"gate": "lint-debt"})
            result, _report_path = run_case(
                tree,
                {"pkg/a": {"lines": 50, "functions": 50, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
                extra_args=("--gate-readings", str(readings)),
            )

            self.assertEqual(result.exit_code, 2, result.stdout)
            self.assertIn("must be a JSON array", result.stderr)

    def test_comparison_shows_movement_against_the_base_revision(self) -> None:
        # The floor Δ says how close the package is to failing; it cannot say
        # whether this change improved or regressed coverage.
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1), (2, 1), (3, 1), (4, 0)),
                    functions=((1, "covered", 1), (2, "missed", 0)),
                ),
            )
            baseline = tree.write_json(
                "base-measurement.json",
                {
                    "pkg/a": {
                        "lines": 50.0,
                        "functions": 25.0,
                        "line_total": 4,
                        "function_total": 2,
                    }
                },
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 50, "functions": 50, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
                extra_args=("--compare-to", str(baseline)),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = report_path.read_text(encoding="utf-8")
            self.assertIn("Δ floor | Δ base", report)
            # 75.0% measured against a 50.0% base is +25.0.
            self.assertIn("| +25.0 |", report)

    def test_package_absent_from_the_comparison_is_marked_new(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1),),
                    functions=((1, "covered", 1),),
                ),
            )
            baseline = tree.write_json(
                "base-measurement.json",
                {
                    "pkg/b": {
                        "lines": 80.0,
                        "functions": 80.0,
                        "line_total": 10,
                        "function_total": 4,
                    }
                },
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 50, "functions": 50, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
                extra_args=("--compare-to", str(baseline)),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            self.assertIn("| new |", report_path.read_text(encoding="utf-8"))

    def test_unavailable_comparison_drops_the_column_and_says_so(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1),),
                    functions=((1, "covered", 1),),
                ),
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 50, "functions": 50, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
                extra_args=("--compare-to", str(tree.path("absent.json"))),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = report_path.read_text(encoding="utf-8")
            self.assertIn("| Package | Lines | Floor | Δ | Functions | Floor | Δ | Status |", report)
            self.assertIn("comparison unavailable", report)

    def test_measurement_is_written_even_when_a_floor_fails(self) -> None:
        # The base revision is scored against the head revision's floors, so it may
        # legitimately fail them; its numbers still have to reach the head report.
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1), (2, 0)),
                    functions=((1, "covered", 1),),
                ),
            )
            measurement = tree.path("measurement.json")
            result, _report_path = run_case(
                tree,
                {"pkg/a": {"lines": 99, "functions": 99, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
                extra_args=("--measurement-out", str(measurement)),
            )

            self.assertEqual(result.exit_code, 1, result.stdout)
            self.assertTrue(measurement.is_file())
            self.assertIn('"lines": 50.0', measurement.read_text(encoding="utf-8"))

    def test_same_named_functions_at_different_lines_are_counted_separately(self) -> None:
        # Keyed by name, two same-named functions collapse into one entry covered
        # when the first runs; keyed by declaring line, the miss stays visible.
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                "SF:pkg/a/src/x.ts\n"
                "FN:10,handler\n"
                "FN:50,handler\n"
                "FNDA:7,handler\n"
                "FNDA:0,handler\n"
                "DA:10,7\n"
                "DA:50,0\n"
                "end_of_record\n",
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn(expected_row(
                    "pkg/a", "50.0%", "0.0%",
                    "50.0%", "0.0%", "✅"
                ), report)

    def test_source_repeated_across_two_sf_sections_is_measured_not_an_error(self) -> None:
        # FN declarations count per record to stay in step with the FNDA cursor;
        # deduped, a repeated source's second section reads as excess FNDA records.
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                "SF:pkg/a/src/x.ts\n"
                "FN:10,handler\n"
                "FNDA:7,handler\n"
                "DA:10,7\n"
                "end_of_record\n"
                "SF:pkg/a/src/x.ts\n"
                "FN:10,handler\n"
                "FNDA:3,handler\n"
                "DA:10,3\n"
                "end_of_record\n",
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn(expected_row(
                    "pkg/a", "100.0%", "0.0%",
                    "100.0%", "0.0%", "✅"
                ), report)

    def test_hits_do_not_pair_across_section_boundaries(self) -> None:
        # If the pairing cursor survived end_of_record, the second section's hit
        # would pair with the other declaration, covering a function that never ran.
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                "SF:pkg/a/src/w.ts\n"
                "FN:10,handler\n"
                "FN:50,handler\n"
                "FNDA:4,handler\n"
                "DA:10,4\n"
                "DA:50,0\n"
                "end_of_record\n"
                "SF:pkg/a/src/w.ts\n"
                "FN:10,handler\n"
                "FN:50,handler\n"
                "FNDA:6,handler\n"
                "DA:10,6\n"
                "DA:50,0\n"
                "end_of_record\n",
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn(expected_row(
                    "pkg/a", "50.0%", "0.0%",
                    "50.0%", "0.0%", "✅"
                ), report)

    def test_fnda_without_a_preceding_fn_is_an_operational_error(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                "SF:pkg/a/src/x.ts\nFNDA:1,orphan\nDA:1,1\nend_of_record\n",
            )
            result, _ = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 2)
            self.assertIn("no preceding FN record", result.stdout + result.stderr)

    def test_more_fnda_than_fn_records_for_one_name_is_an_operational_error(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                "SF:pkg/a/src/x.ts\n"
                "FN:10,handler\n"
                "FNDA:1,handler\n"
                "FNDA:1,handler\n"
                "DA:10,1\n"
                "end_of_record\n",
            )
            result, _ = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 2)
            self.assertIn("outnumber its FN records", result.stdout + result.stderr)

    def test_floor_violation_names_every_failing_package_with_values(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov_a = tree.write_text(
                "a.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1), (2, 0)),
                    functions=((1, "covered", 1),),
                ),
            )
            lcov_b = tree.write_text(
                "b.info",
                canned_lcov(
                    "pkg/b/src/main.ts",
                    line_hits=((1, 1),),
                    functions=((1, "missed", 0),),
                ),
            )
            result, report_path = run_case(
                tree,
                {
                    "pkg/a": {"lines": 75, "functions": 0, "measured": {"lines": 0, "functions": 0}},
                    "pkg/b": {"lines": 0, "functions": 50, "measured": {"lines": 0, "functions": 0}},
                },
                coverage_config("unit"),
                (
                    package_args("pkg/a", lcov=((lcov_a, "unit"),)),
                    package_args("pkg/b", lcov=((lcov_b, "unit"),)),
                ),
            )

            self.assertEqual(result.exit_code, 1, result.stderr)
            self.assertIn("pkg/a: coverage-floor: lines 50.0% is below floor 75.0%", result.stdout)
            self.assertIn("pkg/b: coverage-floor: functions 0.0% is below floor 50.0%", result.stdout)
            report = assert_complete_report(self, report_path, ("pkg/a", "pkg/b"))
            self.assertIn(expected_row(
                    "pkg/a", "50.0%", "75.0%",
                    "100.0%", "0.0%", "❌"
                ), report)
            self.assertIn(expected_row(
                    "pkg/b", "100.0%", "0.0%",
                    "0.0%", "50.0%", "❌"
                ), report)

    def test_nonzero_suite_result_overrides_satisfied_floors(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            lcov = tree.write_text(
                "lcov.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1),),
                    functions=((1, "covered", 1),),
                ),
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 100, "functions": 100, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((lcov, "unit"),), suites=(("unit", 7),)),),
            )

            self.assertEqual(result.exit_code, 1, result.stderr)
            self.assertIn("pkg/a: suite-failure: unit exited 7", result.stdout)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn("Suites: unit ❌ (exit 7)", report)
            self.assertIn(expected_row(
                    "pkg/a", "100.0%", "100.0%",
                    "100.0%", "100.0%", "❌"
                ), report)

    def test_missing_declared_lcov_after_success_is_exit_two_with_cause(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            missing = tree.path("missing.info")
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((missing, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 2)
            self.assertIn("cannot read declared lcov input", result.stderr)
            self.assertIn(str(missing), result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn("cannot read declared lcov input", report)
            self.assertIn("— (no measurable coverage data)", report)

    def test_missing_lcov_after_failed_suite_remains_a_suite_violation(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            missing = tree.path("missing.info")
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((missing, "unit"),), suites=(("unit", 1),)),),
            )

            self.assertEqual(result.exit_code, 1, result.stderr)
            self.assertNotIn("cannot read declared lcov input", result.stderr)
            self.assertIn("suite-failure", result.stdout)
            assert_complete_report(self, report_path, ("pkg/a",))

    def test_empty_declared_lcov_after_success_is_exit_two_with_cause(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            blank = tree.write_text("blank.info", "")
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((blank, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 2)
            self.assertIn("is empty", result.stderr)
            self.assertIn(str(blank), result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn("is empty", report)

    def test_empty_lcov_after_failed_suite_remains_a_suite_violation(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            blank = tree.write_text("blank.info", "")
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((blank, "unit"),), suites=(("unit", 1),)),),
            )

            self.assertEqual(result.exit_code, 1, result.stderr)
            self.assertNotIn("is empty", result.stderr)
            self.assertIn("suite-failure", result.stdout)
            assert_complete_report(self, report_path, ("pkg/a",))

    def test_fully_excluded_lcov_after_success_is_exit_two(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            excluded = tree.write_text("excluded.info", canned_lcov("pkg/src/main.test.ts"))
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((excluded, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 2)
            self.assertIn("yielded no records inside the measured perimeter", result.stderr)
            self.assertIn(str(excluded), result.stderr)
            assert_complete_report(self, report_path, ("pkg/a",))

    def test_blanking_one_merged_stream_cannot_score_as_coverage(self) -> None:
        floors = {"pkg/a": {"lines": 60, "functions": 40, "measured": {"lines": 0, "functions": 0}}}
        config = coverage_config("unit", "vitest")
        with FixtureTree(parent=TESTS_DIR) as tree:
            narrow = tree.write_text(
                "narrow.info",
                canned_lcov(
                    "pkg/src/narrow.ts",
                    line_hits=((1, 1),),
                    functions=((1, "narrow", 1),),
                ),
            )
            broad = tree.write_text(
                "broad.info",
                canned_lcov(
                    "pkg/src/broad.ts",
                    line_hits=((1, 1), (2, 0)),
                    functions=((1, "broad", 0),),
                ),
            )
            group = package_args(
                "pkg/a",
                lcov=((broad, "unit"), (narrow, "vitest")),
                suites=(("unit", 0), ("vitest", 0)),
            )

            honest, report_path = run_case(tree, floors, config, (group,))
            self.assertEqual(honest.exit_code, 0, honest.stderr)
            self.assertIn(
                expected_row(
                    "pkg/a", "66.7%", "60.0%",
                    "50.0%", "40.0%", "✅"
                ),
                report_path.read_text(encoding="utf-8"),
            )

            broad.write_text("", encoding="utf-8")
            blanked, report_path = run_case(tree, floors, config, (group,))

            self.assertEqual(blanked.exit_code, 2)
            self.assertIn("is empty", blanked.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            # The narrower survivor still measures 100%, so the guard rather than the
            # floor comparison is what stops an emptied stream scoring as a pass.
            self.assertIn(expected_row(
                    "pkg/a", "100.0%", "60.0%",
                    "100.0%", "40.0%", "❌"
                ), report)

    def test_dropping_records_from_a_stream_fails_on_the_denominator(self) -> None:
        floors = {
            "pkg/a": {
                "lines": 50.0,
                "functions": 40.0,
                "measured": {"lines": 3, "functions": 2},
            }
        }
        config = coverage_config("unit")
        with FixtureTree(parent=TESTS_DIR) as tree:
            stream = tree.write_text(
                "unit.info",
                canned_lcov(
                    "pkg/src/covered.ts",
                    line_hits=((1, 1),),
                    functions=((1, "covered", 1),),
                )
                + canned_lcov(
                    "pkg/src/partly.ts",
                    line_hits=((1, 1), (2, 0)),
                    functions=((1, "partly", 0),),
                ),
            )
            group = package_args(
                "pkg/a", lcov=((stream, "unit"),), suites=(("unit", 0),)
            )

            honest, _ = run_case(tree, floors, config, (group,))
            self.assertEqual(honest.exit_code, 0, honest.stderr)

            # Keeping only the fully covered file raises both percentages to 100%.
            stream.write_text(
                canned_lcov(
                    "pkg/src/covered.ts",
                    line_hits=((1, 1),),
                    functions=((1, "covered", 1),),
                ),
                encoding="utf-8",
            )
            trimmed, report_path = run_case(tree, floors, config, (group,))

            self.assertEqual(trimmed.exit_code, 1)
            self.assertIn(
                "pkg/a: coverage-denominator: lines denominator 1 is below the "
                "committed 3, so the measured set shrank",
                trimmed.stdout,
            )
            self.assertIn("functions denominator 1 is below the committed 2", trimmed.stdout)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn(expected_row(
                    "pkg/a", "100.0%", "50.0%",
                    "100.0%", "40.0%", "❌"
                ), report)

    def test_committed_denominator_of_zero_leaves_the_metric_unfloored(self) -> None:
        floors = {
            "pkg/a": {
                "lines": 0.0,
                "functions": 0.0,
                "measured": {"lines": 0, "functions": 0},
            }
        }
        config = coverage_config("unit")
        with FixtureTree(parent=TESTS_DIR) as tree:
            # Two lines measured against a committed minimum of zero: the guard is
            # skipped, where a minimum of 3 would fail it.
            stream = tree.write_text(
                "unit.info",
                canned_lcov(
                    "pkg/src/only.ts", line_hits=((1, 1), (2, 0)), functions=()
                ),
            )
            group = package_args(
                "pkg/a", lcov=((stream, "unit"),), suites=(("unit", 0),)
            )

            unfloored, _ = run_case(tree, floors, config, (group,))
            self.assertEqual(unfloored.exit_code, 0, unfloored.stderr)

            floored = {
                "pkg/a": {
                    "lines": 0.0,
                    "functions": 0.0,
                    "measured": {"lines": 3, "functions": 0},
                }
            }
            failing, _ = run_case(tree, floored, config, (group,))

            self.assertEqual(failing.exit_code, 1)
            self.assertIn("lines denominator 2 is below the committed 3", failing.stdout)

    def test_floors_entry_omitting_only_measured_is_an_error(self) -> None:
        config = coverage_config("unit")
        with FixtureTree(parent=TESTS_DIR) as tree:
            stream = tree.write_text(
                "unit.info",
                canned_lcov("pkg/src/only.ts", line_hits=((1, 1),), functions=()),
            )
            group = package_args(
                "pkg/a", lcov=((stream, "unit"),), suites=(("unit", 0),)
            )

            result, _ = run_case(
                tree, {"pkg/a": {"lines": 50.0, "functions": 50.0}}, config, (group,)
            )

            self.assertEqual(result.exit_code, 2)
            self.assertIn(
                "entry must contain exactly 'lines', 'functions' and 'measured'",
                result.stderr,
            )

    def test_malformed_measured_object_is_an_error(self) -> None:
        config = coverage_config("unit")
        cases = (
            ({"lines": 1}, "measured must be an object containing exactly 'lines' and 'functions'"),
            ({"lines": 1, "functions": "2"}, "measured.functions must be a non-negative integer"),
            ({"lines": 1, "functions": -1}, "measured.functions must be a non-negative integer"),
            ({"lines": 1.5, "functions": 2}, "measured.lines must be a non-negative integer"),
        )
        for measured, expected in cases:
            with self.subTest(measured=measured):
                with FixtureTree(parent=TESTS_DIR) as tree:
                    stream = tree.write_text(
                        "unit.info",
                        canned_lcov("pkg/src/only.ts", line_hits=((1, 1),), functions=()),
                    )
                    group = package_args(
                        "pkg/a", lcov=((stream, "unit"),), suites=(("unit", 0),)
                    )

                    result, _ = run_case(
                        tree,
                        {"pkg/a": {"lines": 0.0, "functions": 0.0, "measured": measured}},
                        config,
                        (group,),
                    )

                    self.assertEqual(result.exit_code, 2)
                    self.assertIn(expected, result.stderr)

    def test_invalid_floors_are_strict_errors_and_never_default(self) -> None:
        invalid_floors = (
            {"pkg/a": {"lines": -1, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
            {"pkg/a": {"lines": "90", "functions": 0, "measured": {"lines": 0, "functions": 0}}},
            {"pkg/a": {"lines": 90}},
            {"pkg/a": {"lines": 90, "functions": 80, "extra": 1}},
        )
        for floors in invalid_floors:
            with self.subTest(floors=floors), FixtureTree(parent=TESTS_DIR) as tree:
                result, report_path = run_case(
                    tree,
                    floors,
                    coverage_config(),
                    (package_args("pkg/a"),),
                )
                self.assertEqual(result.exit_code, 2)
                self.assertIn("floors file", result.stderr)
                report = assert_complete_report(self, report_path, ("pkg/a",))
                self.assertIn("Operational errors:", report)
                self.assertIn("floors file", report)

    def test_union_max_merge_and_all_exclusion_layers(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            streams = (
                (
                    (
                        "pkg/a/src/small.ts",
                        ((1, 1), (2, 0)),
                        ((1, "shared", 1), (2, "small-only", 0)),
                    ),
                    (
                        "pkg/a/src/large.ts",
                        ((1, 0), (2, 0), (3, 0), (4, 0), (5, 0)),
                        (
                            (1, "shared", 0),
                            (2, "large-two", 0),
                            (3, "large-three", 0),
                            (4, "large-four", 0),
                            (5, "large-five", 0),
                        ),
                    ),
                ),
                (
                    ("pkg/a/src/small.ts", ((2, 3),), ((2, "small-only", 2),)),
                    ("pkg/a/src/large.ts", ((2, 0),), ((2, "large-two", 0),)),
                ),
            )
            first = "".join(
                canned_lcov(source, line_hits=line_hits, functions=functions)
                for source, line_hits, functions in streams[0]
            )
            first += "".join(
                (
                    canned_lcov("runner-only/skip.ts", line_hits=((1, 0),), functions=((1, "skip", 0),)),
                    canned_lcov("pkg/a/src/generated/skip.ts", line_hits=((1, 0),), functions=((1, "skip", 0),)),
                    canned_lcov("pkg/a/src/ignored.ts", line_hits=((1, 0),), functions=((1, "skip", 0),)),
                    canned_lcov("pkg/a/src/main.test.ts", line_hits=((1, 0),), functions=((1, "skip", 0),)),
                )
            )
            second = "".join(
                canned_lcov(source, line_hits=line_hits, functions=functions)
                for source, line_hits, functions in streams[1]
            )

            expected_lines: dict[tuple[str, int], int] = {}
            expected_functions: dict[tuple[str, str], int] = {}
            for stream in streams:
                for source, line_hits, functions in stream:
                    for line, hits in line_hits:
                        key = (source, line)
                        expected_lines[key] = max(expected_lines.get(key, 0), hits)
                    for _line, function, hits in functions:
                        key = (source, function)
                        expected_functions[key] = max(expected_functions.get(key, 0), hits)
            expected_line_percentage = 100.0 * sum(
                hits > 0 for hits in expected_lines.values()
            ) / len(expected_lines)
            expected_function_percentage = 100.0 * sum(
                hits > 0 for hits in expected_functions.values()
            ) / len(expected_functions)
            per_file_average = 50.0
            self.assertNotEqual(round(expected_line_percentage, 1), per_file_average)
            self.assertNotEqual(round(expected_function_percentage, 1), per_file_average)
            first_path = tree.write_text("first.info", first)
            second_path = tree.write_text("second.info", second)
            config = coverage_config("first", "second")
            config["lcovFilters"]["first"] = ["runner-only/"]
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 28.5, "functions": 28.5, "measured": {"lines": 0, "functions": 0}}},
                config,
                (
                    package_args(
                        "pkg/a",
                        lcov=((first_path, "first"), (second_path, "second")),
                        suites=(("first-suite", 0), ("second-suite", 0)),
                    ),
                ),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn(
                expected_row(
                    "pkg/a", ""
                f"{expected_line_percentage:.1f}%", "28.5%",
                    ""
                f"{expected_function_percentage:.1f}%", "28.5%", "✅"
                ),
                report,
            )

    def test_exclude_patterns_are_regexes_and_do_not_match_by_wildcard(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            stream = tree.write_text(
                "streams.info",
                # A production file whose name merely contains the word, plus a real
                # test file. An unescaped `.test.` pattern matches both.
                canned_lcov("pkg/a/src/latest-config.ts", line_hits=((1, 1),), functions=())
                + canned_lcov("pkg/a/src/thing.test.ts", line_hits=((1, 0), (2, 0)), functions=()),
            )
            config = coverage_config("unit")
            config["excludePatterns"] = [
                {"pattern": r"\.test\.", "reason": "fixture"}
            ]
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                config,
                (package_args("pkg/a", lcov=((stream, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            # Only the production file counts, and it is fully covered. Were the dot
            # a wildcard both files would drop out and there would be no data at all.
            self.assertIn("| pkg/a | 100.0% | 0.0% |", report)

    def test_package_without_declared_streams_is_an_operational_error(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            result, report_path = run_case(
                tree,
                {"pkg/empty": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config(),
                (package_args("pkg/empty"),),
            )

            # Floors of 0.0 cannot be breached, so without this guard the package
            # renders a pass while measuring nothing at all.
            self.assertEqual(result.exit_code, 2)
            self.assertIn("declares no lcov streams", result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/empty",))
            self.assertIn(expected_row(
                    "pkg/empty", "— (no measurable coverage data)", "0.0%",
                    "—", "0.0%", "❌"
                ), report)

    def test_unmeasurable_metric_renders_a_dash_beside_a_measured_one(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            functions_only = tree.write_text(
                "functions.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=(),
                    functions=((1, "hit", 1),),
                    emit_function_records=False,
                ),
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 100, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (
                    package_args(
                        "pkg/a", lcov=((functions_only, "unit"),), suites=(("unit", 0),)
                    ),
                ),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn(
                expected_row(
                    "pkg/a", "— (no measurable coverage data)", "0.0%",
                    "100.0%", "100.0%", "✅"
                ),
                report,
            )

    def test_function_totals_come_from_summaries_when_no_records_name_them(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            summary_only = tree.write_text(
                "summary.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=((1, 1), (2, 0)),
                    functions=(
                        (1, "hit", 3),
                        (2, "alsoHit", 1),
                        (3, "cold", 0),
                        (4, "alsoCold", 0),
                    ),
                    emit_function_records=False,
                ),
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (package_args("pkg/a", lcov=((summary_only, "unit"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            # 2 of 4 functions hit, carried entirely by FNF/FNH.
            self.assertIn(expected_row(
                    "pkg/a", "50.0%", "0.0%",
                    "50.0%", "0.0%", "✅"
                ), report)

    def test_named_function_records_win_over_a_summary_for_the_same_file(self) -> None:
        source = "pkg/a/src/main.ts"
        with FixtureTree(parent=TESTS_DIR) as tree:
            exact = tree.write_text(
                "exact.info",
                canned_lcov(source, line_hits=((1, 1),), functions=((1, "only", 1),)),
            )
            # A wildly different denominator for the same file: if the summary were
            # added rather than ignored the result could not stay at 100%.
            summary = tree.write_text(
                "summary.info",
                canned_lcov(
                    source,
                    line_hits=((1, 1),),
                    functions=tuple((n, f"f{n}", 0) for n in range(1, 11)),
                    emit_function_records=False,
                ),
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit", "vitest"),
                (
                    package_args(
                        "pkg/a",
                        lcov=((exact, "unit"), (summary, "vitest")),
                        suites=(("unit", 0), ("vitest", 0)),
                    ),
                ),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn(expected_row(
                    "pkg/a", "100.0%", "0.0%",
                    "100.0%", "0.0%", "✅"
                ), report)

    def test_summary_only_stream_is_not_treated_as_contributing_nothing(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            functions_only = tree.write_text(
                "functions.info",
                canned_lcov(
                    "pkg/a/src/main.ts",
                    line_hits=(),
                    functions=((1, "hit", 1),),
                    emit_function_records=False,
                ),
            )
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 100, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("unit"),
                (
                    package_args(
                        "pkg/a", lcov=((functions_only, "unit"),), suites=(("unit", 0),)
                    ),
                ),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            self.assertNotIn("yielded no records", result.stderr)

    def test_real_bun_section_shape_measures_lines_and_functions(self) -> None:
        # Emitted verbatim by bun's lcov reporter: FNF/FNH totals and no FN records.
        bun_lcov = (
            "TN:\n"
            "SF:pkg/a/src/lib.ts\n"
            "FNF:2\n"
            "FNH:1\n"
            "DA:1,12\n"
            "DA:2,18\n"
            "DA:3,9\n"
            "DA:5,0\n"
            "DA:6,0\n"
            "DA:7,0\n"
            "LF:6\n"
            "LH:3\n"
            "end_of_record\n"
        )
        with FixtureTree(parent=TESTS_DIR) as tree:
            stream = tree.write_text("bun.info", bun_lcov)
            result, report_path = run_case(
                tree,
                {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                coverage_config("bun"),
                (package_args("pkg/a", lcov=((stream, "bun"),), suites=(("unit", 0),)),),
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            report = assert_complete_report(self, report_path, ("pkg/a",))
            self.assertIn(expected_row(
                    "pkg/a", "50.0%", "0.0%",
                    "50.0%", "0.0%", "✅"
                ), report)

    def test_malformed_function_summaries_are_operational_errors(self) -> None:
        cases = (
            ("FNH:1\n", "FNH record has no corresponding FNF record"),
            ("FNF:1\nFNH:2\n", "FNH 2 exceeds FNF 1"),
            ("FNF:1\nFNF:2\nFNH:1\n", "section repeats its FNF record"),
            ("FNF:1\nFNH:1\nFNH:1\n", "section repeats its FNH record"),
        )
        for records, expected in cases:
            with self.subTest(records=records), FixtureTree(parent=TESTS_DIR) as tree:
                stream = tree.write_text(
                    "broken.info",
                    "TN:\nSF:pkg/a/src/main.ts\n" + records + "DA:1,1\nend_of_record\n",
                )
                result, _report_path = run_case(
                    tree,
                    {"pkg/a": {"lines": 0, "functions": 0, "measured": {"lines": 0, "functions": 0}}},
                    coverage_config("unit"),
                    (
                        package_args(
                            "pkg/a", lcov=((stream, "unit"),), suites=(("unit", 0),)
                        ),
                    ),
                )

                self.assertEqual(result.exit_code, 2, result.stderr)
                self.assertIn(expected, result.stderr)


if __name__ == "__main__":
    unittest.main()
