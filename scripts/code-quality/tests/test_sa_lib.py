"""Tests for the shared static-analysis library and test scaffolding."""

from __future__ import annotations

import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
CODE_QUALITY_DIR = TESTS_DIR.parent
sys.path.insert(0, str(CODE_QUALITY_DIR))

import sa_lib  # noqa: E402
from helpers import (  # noqa: E402
    FixtureTree,
    canned_eslint_report,
    canned_jscpd_report,
    canned_lcov,
    run_script,
)


class LintDebtGrammarTests(unittest.TestCase):
    def test_canonical_pattern_is_stable(self) -> None:
        self.assertEqual(
            sa_lib.LINT_DEBT_RE.pattern,
            r"^\s*//\s*LINT-DEBT\(([A-Za-z0-9@/_-]+)\):\s*(\S.*)$",
        )

    def test_parse_and_format_round_trip(self) -> None:
        comment = sa_lib.format_lint_debt(
            "sonarjs/cognitive-complexity",
            "pre-existing complexity; refactor before extending",
        )
        self.assertEqual(
            comment,
            "// LINT-DEBT(sonarjs/cognitive-complexity): "
            "pre-existing complexity; refactor before extending",
        )
        self.assertEqual(
            sa_lib.parse_lint_debt(f"  {comment}"),
            ("sonarjs/cognitive-complexity", "pre-existing complexity; refactor before extending"),
        )

    def test_malformed_comments_are_rejected(self) -> None:
        malformed = (
            "// LINT-DEBT: reason",
            "// LINT-DEBT(): reason",
            "// LINT-DEBT(complexity):   ",
            "/* LINT-DEBT(complexity): reason",
            "// lint-debt(complexity): reason",
            "// LINT-DEBT(rule.with.dot): reason",
            "\n// LINT-DEBT(complexity): reason",
        )
        for comment in malformed:
            with self.subTest(comment=comment):
                self.assertIsNone(sa_lib.parse_lint_debt(comment))

    def test_block_and_jsx_forms_are_accepted(self) -> None:
        for comment in (
            "/* LINT-DEBT(complexity): reason */",
            "{/* LINT-DEBT(sonarjs/cognitive-complexity): reason with spaces */}",
            "  {/* LINT-DEBT(max-depth): indented JSX child form */}  ",
        ):
            with self.subTest(comment=comment):
                parsed = sa_lib.parse_lint_debt(comment)
                self.assertIsNotNone(parsed)
                rule, reason = parsed
                self.assertNotIn("*/", reason)

    def test_formatter_rejects_invalid_fields(self) -> None:
        for rule, reason in (
            ("", "reason"),
            ("rule.with.dot", "reason"),
            ("complexity", ""),
            ("complexity", " leading whitespace"),
            ("complexity", "two\nlines"),
        ):
            with self.subTest(rule=rule, reason=reason):
                with self.assertRaises(ValueError):
                    sa_lib.format_lint_debt(rule, reason)


class ResultContractTests(unittest.TestCase):
    def test_violation_rendering_with_and_without_line(self) -> None:
        located = sa_lib.Violation("packages/app/src/main.ts", 7, "unjustified", "missing debt")
        unlocated = sa_lib.Violation(".jscpd.json", None, "config", "missing include")
        self.assertEqual(
            located.render(),
            "packages/app/src/main.ts:7: unjustified: missing debt",
        )
        self.assertEqual(str(unlocated), ".jscpd.json: config: missing include")

    def test_exit_code_precedence(self) -> None:
        self.assertEqual(sa_lib.resolve_exit_code(False, False), sa_lib.EXIT_CLEAN)
        self.assertEqual(sa_lib.resolve_exit_code(False, True), sa_lib.EXIT_VIOLATIONS)
        self.assertEqual(sa_lib.resolve_exit_code(True, False), sa_lib.EXIT_ERROR)
        self.assertEqual(sa_lib.resolve_exit_code(True, True), sa_lib.EXIT_ERROR)

    def test_emit_results_separates_channels_and_preserves_error_precedence(self) -> None:
        errors = sa_lib.ErrorAccumulator()
        errors.add(ValueError("invalid JSON"), context=".jscpd.json")
        stdout = io.StringIO()
        stderr = io.StringIO()
        exit_code = sa_lib.emit_results(
            [sa_lib.Violation("src/main.ts", 3, "malformed", "bad comment")],
            errors,
            stdout=stdout,
            stderr=stderr,
        )
        self.assertEqual(exit_code, sa_lib.EXIT_ERROR)
        self.assertEqual(stdout.getvalue(), "src/main.ts:3: malformed: bad comment\n")
        self.assertEqual(stderr.getvalue(), "error: .jscpd.json: ValueError: invalid JSON\n")

    def test_unhandled_exception_handler_prints_cause_and_exits_two(self) -> None:
        def exploding_main(_argv: object) -> int:
            try:
                raise ValueError("root cause")
            except ValueError as cause:
                raise RuntimeError("outer failure") from cause

        stderr = io.StringIO()
        with redirect_stderr(stderr):
            with self.assertRaises(SystemExit) as raised:
                sa_lib.run_cli(exploding_main, ())
        self.assertEqual(raised.exception.code, sa_lib.EXIT_ERROR)
        self.assertIn("RuntimeError: outer failure", stderr.getvalue())
        self.assertIn("ValueError: root cause", stderr.getvalue())


class ProductionCodePerimeterTests(unittest.TestCase):
    def test_loader_matches_and_enumerates_include_minus_ignore(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            config = tree.write_json(
                ".jscpd.json",
                {
                    "include": ["packages/app/src/**", "packages/*/lib/*.ts"],
                    "ignore": ["**/tests/**", "**/*.test.*", "**/dist/**"],
                },
            )
            expected = {
                tree.write_text("packages/app/src/main.ts", "export const main = 1;\n"),
                tree.write_text("packages/tool/lib/index.ts", "export const tool = 1;\n"),
            }
            tree.write_text("packages/app/src/main.test.ts", "test('x', () => {});\n")
            tree.write_text("packages/app/src/tests/helper.ts", "export const helper = 1;\n")
            tree.write_text("packages/app/dist/generated.ts", "export const generated = 1;\n")
            tree.write_text("README.md", "outside\n")

            perimeter = sa_lib.load_production_code_perimeter(config)

            self.assertEqual(set(perimeter.iter_files()), expected)
            self.assertTrue(perimeter.matches("packages/app/src/main.ts"))
            self.assertTrue(perimeter.matches(tree.root / "packages/tool/lib/index.ts"))
            self.assertTrue(perimeter.matches(r"packages\app\src\main.ts"))
            self.assertFalse(perimeter.matches("packages/app/src/main.test.ts"))
            self.assertFalse(perimeter.matches("packages/app/src/tests/helper.ts"))
            self.assertFalse(perimeter.matches("../outside.ts"))

    def test_missing_and_unparseable_configs_take_the_exit_two_path(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            invalid = tree.write_text("invalid.json", "{not-json")
            for config in (tree.root / "missing.json", invalid):
                with self.subTest(config=config.name):
                    stderr = io.StringIO()

                    def loading_main(_argv: object) -> int:
                        sa_lib.load_production_code_perimeter(config)
                        return sa_lib.EXIT_CLEAN

                    with redirect_stderr(stderr):
                        with self.assertRaises(SystemExit) as raised:
                            sa_lib.run_cli(loading_main, ())
                    self.assertEqual(raised.exception.code, sa_lib.EXIT_ERROR)
                    self.assertIn("perimeter config", stderr.getvalue())

    def test_invalid_config_shape_is_rejected(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            missing_ignore = tree.write_json(".jscpd.json", {"include": ["src/**"]})
            with self.assertRaises(sa_lib.PerimeterConfigError):
                sa_lib.load_production_code_perimeter(missing_ignore)


class TestScaffoldingTests(unittest.TestCase):
    def test_canned_reports_have_expected_shapes(self) -> None:
        eslint = canned_eslint_report()
        jscpd = canned_jscpd_report((("src/a.ts", "src/b.ts"),))
        lcov = canned_lcov()

        self.assertEqual(eslint[0]["messages"][0]["ruleId"], "complexity")
        self.assertEqual(jscpd["statistics"]["total"]["clones"], 1)
        self.assertEqual(len(jscpd["duplicates"]), 1)
        self.assertIn("SF:packages/tui/src/example.ts", lcov)
        self.assertIn("FNDA:1,example", lcov)
        self.assertTrue(lcov.endswith("end_of_record\n"))

    def test_subprocess_runner_uses_current_interpreter_and_captures_streams(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            script = tree.write_text(
                "probe.py",
                "import json, sys\n"
                "print(json.dumps({'executable': sys.executable, 'arg': sys.argv[1]}))\n"
                "print('diagnostic', file=sys.stderr)\n"
                "raise SystemExit(7)\n",
            )
            result = run_script(script, "value", cwd=tree.root)

            self.assertEqual(result.exit_code, 7)
            self.assertEqual(json.loads(result.stdout)["executable"], sys.executable)
            self.assertEqual(json.loads(result.stdout)["arg"], "value")
            self.assertEqual(result.stderr, "diagnostic\n")
            self.assertEqual(result.argv[0], sys.executable)


class MeasurementTests(unittest.TestCase):
    def test_render_states_measured_limit_and_headroom(self) -> None:
        reading = sa_lib.Measurement(
            gate="lint-debt",
            subject="complexity",
            measured="82",
            limit="82",
            headroom="0",
        )

        self.assertEqual(
            reading.render(),
            "measured: lint-debt complexity = 82 (limit 82, headroom 0)",
        )
        self.assertEqual(str(reading), reading.render())

    def test_subject_may_be_empty_but_the_rest_may_not(self) -> None:
        self.assertEqual(
            sa_lib.Measurement(
                gate="duplication", subject="", measured="29", limit="29", headroom="0"
            ).render(),
            "measured: duplication = 29 (limit 29, headroom 0)",
        )
        for field in ("gate", "measured", "limit", "headroom"):
            with self.subTest(field=field):
                kwargs = {
                    "gate": "g",
                    "subject": "s",
                    "measured": "1",
                    "limit": "1",
                    "headroom": "0",
                }
                kwargs[field] = ""
                with self.assertRaises(ValueError):
                    sa_lib.Measurement(**kwargs)

    def test_emit_writes_every_reading_to_the_given_stream(self) -> None:
        stream = io.StringIO()
        readings = [
            sa_lib.Measurement(
                gate="coverage",
                subject=f"pkg/{index} lines",
                measured="90.0%",
                limit="88.0%",
                headroom="+2.0",
            )
            for index in range(3)
        ]

        sa_lib.emit_measurements(readings, stdout=stream)

        self.assertEqual(stream.getvalue().count("measured: coverage"), 3)
        self.assertIn("pkg/2 lines = 90.0% (limit 88.0%, headroom +2.0)", stream.getvalue())

    def test_persisted_readings_survive_a_round_trip(self) -> None:
        readings = [
            sa_lib.Measurement(
                gate="lint-debt",
                subject="complexity",
                measured="83",
                limit="83",
                headroom="0",
            )
        ]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "readings.json"

            sa_lib.write_measurements(path, readings)

            self.assertEqual(sa_lib.read_measurements(path), readings)

    def test_readings_missing_a_field_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "readings.json"
            path.write_text('[{"gate": "lint-debt", "subject": "complexity"}]', encoding="utf-8")

            with self.assertRaises(ValueError) as caught:
                sa_lib.read_measurements(path)

            self.assertIn("missing 'measured'", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
