"""Tests for the production-code duplication checker."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import shlex
import sys
import unittest
from pathlib import Path
from types import ModuleType
from typing import Any, Sequence

from helpers import FixtureTree, canned_jscpd_report, run_script
from generators import HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON

if HAS_HYPOTHESIS:
    from hypothesis import given, settings, strategies as st


SCRIPT = Path(__file__).resolve().parents[1] / "check-duplication.py"
SCRIPT_DIRECTORY = SCRIPT.parent
if str(SCRIPT_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIRECTORY))


def _load_checker() -> ModuleType:
    spec = importlib.util.spec_from_file_location(
        "code_quality_check_duplication_under_test", SCRIPT
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot import {SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


CHECKER = _load_checker()


def _config() -> dict[str, object]:
    return {
        "mode": "mild",
        "minLines": 10,
        "minTokens": 75,
        "format": ["typescript"],
        "path": ["src"],
        "include": ["src/**"],
        "ignore": ["**/tests/**", "**/*.test.*", "**/__tests__/**"],
    }


def _fixture_files(
    report: object,
    baseline: object = None,
) -> dict[str, str]:
    if baseline is None:
        baseline = {"clones": 0}
    return {
        ".jscpd.json": json.dumps(_config()),
        ".jscpd-baseline.json": json.dumps(baseline),
        "report.json": json.dumps(report),
        # A perimeter that resolves to no files is rejected before any comparison,
        # so the fixture tree needs one real production file for jscpd to have scanned.
        "src/perimeter-anchor.ts": "export const anchor = 1;\n",
    }


def _run_fixture(tree: FixtureTree, *extra_args: str):
    return run_script(
        SCRIPT,
        "--config",
        tree.path(".jscpd.json"),
        "--baseline",
        tree.path(".jscpd-baseline.json"),
        "--report",
        tree.path("report.json"),
        *extra_args,
        cwd=tree.root,
    )


def _invoke_main(arguments: Sequence[str]) -> tuple[int, str, str]:
    stdout = io.StringIO()
    stderr = io.StringIO()
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        exit_code = CHECKER.main(tuple(arguments))
    return exit_code, stdout.getvalue(), stderr.getvalue()


def _main_arguments(tree: FixtureTree) -> tuple[str, ...]:
    return (
        "--config",
        str(tree.path(".jscpd.json")),
        "--baseline",
        str(tree.path(".jscpd-baseline.json")),
        "--report",
        str(tree.path("report.json")),
    )


class DuplicationCheckerTests(unittest.TestCase):
    def test_report_at_baseline_exits_zero(self) -> None:
        report = canned_jscpd_report(
            [("src/one.ts", "src/two.ts"), ("src/three.ts", "src/four.ts")]
        )
        with FixtureTree(_fixture_files(report, {"clones": 2})) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("measured: duplication clones", result.stdout)
        self.assertNotIn("duplication:", result.stdout)
        self.assertEqual(result.stderr, "")

    def test_report_below_baseline_exits_one_and_asks_for_the_lower_count(self) -> None:
        report = canned_jscpd_report([("src/one.ts", "src/two.ts")])
        with FixtureTree(_fixture_files(report, {"clones": 3})) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("is below baseline 3", result.stdout)
        self.assertIn("lower the baseline to 1", result.stdout)

    def test_report_above_baseline_exits_one_and_reports_both_counts(self) -> None:
        report = canned_jscpd_report(
            [("src/one.ts", "src/two.ts"), ("src/three.ts", "src/four.ts")]
        )
        with FixtureTree(_fixture_files(report, {"clones": 1})) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("measured clone count 2", result.stdout)
        self.assertIn("baseline 1", result.stdout)
        self.assertEqual(result.stderr, "")

    def test_clone_touching_test_glob_does_not_count(self) -> None:
        report = canned_jscpd_report(
            [
                ("src/production.ts", "src/tests/production.test.ts"),
                ("src/tests/one.ts", "src/tests/two.ts"),
            ]
        )
        with FixtureTree(_fixture_files(report, {"clones": 0})) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("measured: duplication clones", result.stdout)
        self.assertNotIn("duplication:", result.stdout)

    def test_clone_outside_include_glob_does_not_count(self) -> None:
        report = canned_jscpd_report(
            [("src/production.ts", "docs/copied-example.ts")]
        )
        with FixtureTree(_fixture_files(report, {"clones": 0})) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_jscpd_nonzero_exit_is_operational_error_with_stderr(self) -> None:
        stub = "import sys\nprint('stub exploded', file=sys.stderr)\nraise SystemExit(7)\n"
        with FixtureTree(
            {
                ".jscpd.json": json.dumps(_config()),
                ".jscpd-baseline.json": json.dumps({"clones": 0}),
                "src/perimeter-anchor.ts": "export const anchor = 1;\n",
                "stub.py": stub,
            }
        ) as tree:
            command = f"{shlex.quote(sys.executable)} {shlex.quote(str(tree.path('stub.py')))}"
            result = run_script(
                SCRIPT,
                "--config",
                tree.path(".jscpd.json"),
                "--baseline",
                tree.path(".jscpd-baseline.json"),
                "--jscpd-cmd",
                command,
                cwd=tree.root,
            )

        self.assertEqual(result.returncode, 2)
        self.assertIn("jscpd exited with code 7", result.stderr)
        self.assertIn("stub exploded", result.stderr)

    def test_jscpd_garbage_report_is_operational_error_with_stderr(self) -> None:
        stub = """\
import pathlib
import sys
output = pathlib.Path(sys.argv[sys.argv.index('--output') + 1])
(output / 'jscpd-report.json').write_text('not-json', encoding='utf-8')
print('garbage clue', file=sys.stderr)
"""
        with FixtureTree(
            {
                ".jscpd.json": json.dumps(_config()),
                ".jscpd-baseline.json": json.dumps({"clones": 0}),
                "src/perimeter-anchor.ts": "export const anchor = 1;\n",
                "stub.py": stub,
            }
        ) as tree:
            command = f"{shlex.quote(sys.executable)} {shlex.quote(str(tree.path('stub.py')))}"
            result = run_script(
                SCRIPT,
                "--config",
                tree.path(".jscpd.json"),
                "--baseline",
                tree.path(".jscpd-baseline.json"),
                "--jscpd-cmd",
                command,
                cwd=tree.root,
            )

        self.assertEqual(result.returncode, 2)
        self.assertIn("no parseable clone report", result.stderr)
        self.assertIn("garbage clue", result.stderr)

    def test_report_count_mismatch_is_operational_error(self) -> None:
        report = canned_jscpd_report([("src/one.ts", "src/two.ts")])
        report["statistics"]["total"]["clones"] = 2  # type: ignore[index]
        with FixtureTree(_fixture_files(report)) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 2)
        self.assertIn("clone-count mismatch", result.stderr)

    def test_report_endpoint_without_name_is_operational_error(self) -> None:
        report = canned_jscpd_report([("src/one.ts", "src/two.ts")])
        report["duplicates"][0]["firstFile"] = {}  # type: ignore[index]
        with FixtureTree(_fixture_files(report)) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 2)
        self.assertIn("firstFile.name", result.stderr)

    def test_missing_baseline_exits_two_with_cause(self) -> None:
        report = canned_jscpd_report()
        with FixtureTree(
            {
                ".jscpd.json": json.dumps(_config()),
                "report.json": json.dumps(report),
            }
        ) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 2)
        self.assertIn("cannot read baseline", result.stderr)

    def test_non_json_baseline_exits_two_with_cause(self) -> None:
        with FixtureTree(
            _fixture_files(canned_jscpd_report(), {"clones": 0})
        ) as tree:
            tree.write_bytes(".jscpd-baseline.json", b"\xffnot-json")
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 2)
        self.assertIn("baseline", result.stderr)
        self.assertTrue(
            "cannot read" in result.stderr or "cannot parse" in result.stderr,
            result.stderr,
        )

    def test_wrong_json_type_baseline_exits_two_with_cause(self) -> None:
        with FixtureTree(_fixture_files(canned_jscpd_report(), [])) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 2)
        self.assertIn("must contain a JSON object", result.stderr)

    def test_negative_baseline_exits_two_with_cause(self) -> None:
        with FixtureTree(
            _fixture_files(canned_jscpd_report(), {"clones": -1})
        ) as tree:
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 2)
        self.assertIn("must be non-negative", result.stderr)

    def test_non_integer_and_boolean_baselines_exit_two(self) -> None:
        for clone_count in (1.5, True):
            with self.subTest(clone_count=clone_count):
                with FixtureTree(
                    _fixture_files(
                        canned_jscpd_report(), {"clones": clone_count}
                    )
                ) as tree:
                    result = _run_fixture(tree)
                self.assertEqual(result.returncode, 2)
                self.assertIn("non-negative integer", result.stderr)

    def test_missing_or_extra_baseline_fields_exit_two(self) -> None:
        for baseline in ({}, {"clones": 0, "note": "not allowed"}):
            with self.subTest(baseline=baseline):
                with FixtureTree(
                    _fixture_files(canned_jscpd_report(), baseline)
                ) as tree:
                    result = _run_fixture(tree)
                self.assertEqual(result.returncode, 2)
                self.assertIn("exactly the 'clones' field", result.stderr)

    def test_invalid_config_exits_two_with_cause(self) -> None:
        with FixtureTree(
            _fixture_files(canned_jscpd_report(), {"clones": 0})
        ) as tree:
            tree.write_text(".jscpd.json", "not-json")
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 2)
        self.assertIn("invalid duplication config", result.stderr)
        self.assertIn("cannot parse perimeter config", result.stderr)

    def test_include_outside_every_scan_root_exits_two(self) -> None:
        config = _config()
        config["path"] = ["src"]
        config["include"] = ["src/**", "other/**"]
        stub = """\
import pathlib
import sys
output = pathlib.Path(sys.argv[sys.argv.index('--output') + 1])
(output / 'jscpd-report.json').write_text('{}', encoding='utf-8')
"""
        with FixtureTree(
            {
                ".jscpd.json": json.dumps(config),
                ".jscpd-baseline.json": json.dumps({"clones": 0}),
                "src/perimeter-anchor.ts": "export const anchor = 1;\n",
                "other/thing.ts": "export const thing = 1;\n",
                "stub.py": stub,
            }
        ) as tree:
            command = f"{shlex.quote(sys.executable)} {shlex.quote(str(tree.path('stub.py')))}"
            result = run_script(
                SCRIPT,
                "--config",
                tree.path(".jscpd.json"),
                "--baseline",
                tree.path(".jscpd-baseline.json"),
                "--jscpd-cmd",
                command,
                cwd=tree.root,
            )

        self.assertEqual(result.returncode, 2)
        self.assertIn("lies outside every scan root", result.stderr)

    def test_missing_scan_root_key_exits_two_when_jscpd_would_run(self) -> None:
        stub = """\
import pathlib
import sys
output = pathlib.Path(sys.argv[sys.argv.index('--output') + 1])
(output / 'jscpd-report.json').write_text('{}', encoding='utf-8')
"""
        rootless = _config()
        rootless.pop("path")
        with FixtureTree(
            {
                ".jscpd.json": json.dumps(rootless),
                ".jscpd-baseline.json": json.dumps({"clones": 0}),
                "src/perimeter-anchor.ts": "export const anchor = 1;\n",
                "stub.py": stub,
            }
        ) as tree:
            command = f"{shlex.quote(sys.executable)} {shlex.quote(str(tree.path('stub.py')))}"
            result = run_script(
                SCRIPT,
                "--config",
                tree.path(".jscpd.json"),
                "--baseline",
                tree.path(".jscpd-baseline.json"),
                "--jscpd-cmd",
                command,
                cwd=tree.root,
            )

        self.assertEqual(result.returncode, 2)
        self.assertIn("requires a non-empty path list", result.stderr)

    def test_perimeter_matching_no_files_exits_two_instead_of_reporting_zero(self) -> None:
        with FixtureTree(
            _fixture_files(canned_jscpd_report(), {"clones": 0})
        ) as tree:
            tree.write_json(
                ".jscpd.json",
                {"include": ["packages/app/typo/**"], "ignore": []},
            )
            result = _run_fixture(tree)

        self.assertEqual(result.returncode, 2)
        self.assertIn("invalid duplication config", result.stderr)
        self.assertIn("matches no files", result.stderr)


if HAS_HYPOTHESIS:

    @st.composite
    def mixed_clone_endpoints(draw: Any) -> list[tuple[bool, bool]]:
        return draw(
            st.lists(
                st.tuples(st.booleans(), st.booleans()),
                min_size=0,
                max_size=30,
            )
        )


@unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)
class DuplicationCheckerPropertyTests(unittest.TestCase):
    if HAS_HYPOTHESIS:

        @settings(max_examples=100, deadline=None)
        @given(
            measured=st.integers(min_value=0, max_value=30),
            baseline=st.integers(min_value=0, max_value=30),
        )
        def test_duplication_exit_code_law(self, measured: int, baseline: int) -> None:
            pairs = [(f"src/a{index}.ts", f"src/b{index}.ts") for index in range(measured)]
            with FixtureTree(
                _fixture_files(canned_jscpd_report(pairs), {"clones": baseline})
            ) as tree:
                exit_code, stdout, stderr = _invoke_main(_main_arguments(tree))

            expected = 0 if measured == baseline else 1
            self.assertEqual(exit_code, expected, stderr)
            if measured != baseline:
                self.assertIn(str(measured), stdout)
                self.assertIn(str(baseline), stdout)

        @settings(max_examples=100, deadline=None)
        @given(endpoints=mixed_clone_endpoints())
        def test_test_glob_clones_never_count(
            self, endpoints: list[tuple[bool, bool]]
        ) -> None:
            pairs: list[tuple[str, str]] = []
            for index, (first_is_production, second_is_production) in enumerate(endpoints):
                first = (
                    f"src/production/first{index}.ts"
                    if first_is_production
                    else f"src/tests/first{index}.test.ts"
                )
                second = (
                    f"src/production/second{index}.ts"
                    if second_is_production
                    else f"src/tests/second{index}.test.ts"
                )
                pairs.append((first, second))
            expected_count = sum(first and second for first, second in endpoints)
            files = _fixture_files(
                canned_jscpd_report(pairs), {"clones": expected_count}
            )
            with FixtureTree(files) as tree:
                arguments = _main_arguments(tree)
                exit_code, _stdout, stderr = _invoke_main(arguments)
                self.assertEqual(exit_code, 0, stderr)
                if expected_count:
                    tree.write_json(
                        ".jscpd-baseline.json", {"clones": expected_count - 1}
                    )
                    exit_code, stdout, stderr = _invoke_main(arguments)
                    self.assertEqual(exit_code, 1, stderr)
                    self.assertIn(f"measured clone count {expected_count}", stdout)

        @settings(max_examples=100, deadline=None)
        @given(
            invalid_kind=st.sampled_from(
                (
                    "absent",
                    "non-json",
                    "wrong-type",
                    "missing-key",
                    "negative",
                    "non-integer",
                    "boolean",
                    "extra-key",
                )
            ),
            measured=st.integers(min_value=0, max_value=30),
            magnitude=st.integers(min_value=1, max_value=1_000_000),
            fraction=st.floats(
                min_value=0,
                max_value=1_000_000,
                allow_nan=False,
                allow_infinity=False,
                exclude_min=True,
            ),
        )
        def test_invalid_baseline_is_always_an_error(
            self,
            invalid_kind: str,
            measured: int,
            magnitude: int,
            fraction: float,
        ) -> None:
            pairs = [(f"src/a{index}.ts", f"src/b{index}.ts") for index in range(measured)]
            files = {
                ".jscpd.json": json.dumps(_config()),
                "report.json": json.dumps(canned_jscpd_report(pairs)),
            }
            with FixtureTree(files) as tree:
                baseline_path = tree.path(".jscpd-baseline.json")
                if invalid_kind == "non-json":
                    baseline_path.write_text(f"not-json-{magnitude}", encoding="utf-8")
                elif invalid_kind == "wrong-type":
                    baseline_path.write_text(json.dumps([magnitude]), encoding="utf-8")
                elif invalid_kind == "missing-key":
                    baseline_path.write_text(json.dumps({"count": magnitude}), encoding="utf-8")
                elif invalid_kind == "negative":
                    baseline_path.write_text(json.dumps({"clones": -magnitude}), encoding="utf-8")
                elif invalid_kind == "non-integer":
                    non_integer = fraction
                    if non_integer.is_integer():
                        non_integer += 0.5
                    baseline_path.write_text(
                        json.dumps({"clones": non_integer}), encoding="utf-8"
                    )
                elif invalid_kind == "boolean":
                    baseline_path.write_text(json.dumps({"clones": True}), encoding="utf-8")
                elif invalid_kind == "extra-key":
                    baseline_path.write_text(
                        json.dumps({"clones": 0, "extra": magnitude}), encoding="utf-8"
                    )
                exit_code, _stdout, stderr = _invoke_main(_main_arguments(tree))

            self.assertEqual(exit_code, 2)
            self.assertIn("baseline", stderr)

    else:

        def test_duplication_exit_code_law(self) -> None:
            self.fail("skip guard did not run")

        def test_test_glob_clones_never_count(self) -> None:
            self.fail("skip guard did not run")

        def test_invalid_baseline_is_always_an_error(self) -> None:
            self.fail("skip guard did not run")


if __name__ == "__main__":
    unittest.main()
