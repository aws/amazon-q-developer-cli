"""Tests for the TypeScript LINT-DEBT adoption annotator."""

from __future__ import annotations

import json
import shlex
import stat
import sys
import unittest
from collections import Counter
from pathlib import Path
from typing import Sequence

TESTS_DIR = Path(__file__).resolve().parent
CODE_QUALITY_DIR = TESTS_DIR.parent
ANNOTATOR = CODE_QUALITY_DIR / "annotate-lint-debt-ts.py"
DEBT_CHECKER = CODE_QUALITY_DIR / "check-lint-debt.py"
sys.path.insert(0, str(CODE_QUALITY_DIR))

import sa_lib  # noqa: E402
from helpers import FixtureTree, canned_eslint_message, canned_eslint_report, run_script  # noqa: E402
from generators import HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON  # noqa: E402

if HAS_HYPOTHESIS:
    from hypothesis import given, settings, strategies as st

RULES = (
    "complexity",
    "max-depth",
    "max-params",
    "sonarjs/cognitive-complexity",
)


def _command(*parts: str) -> str:
    return " ".join(shlex.quote(part) for part in parts)


CLEAN_ESLINT_COMMAND = _command(
    sys.executable,
    "-c",
    "import json; print(json.dumps([]))",
)
FAILING_ESLINT_COMMAND = _command(
    sys.executable,
    "-c",
    "import sys; print('eslint failed', file=sys.stderr); raise SystemExit(7)",
)


def _eslint_report(
    target: Path,
    sites: Sequence[tuple[int, Sequence[tuple[str, int, int]]]],
) -> list[dict[str, object]]:
    messages: list[dict[str, object]] = []
    for line, violations in sites:
        for rule, measured, threshold in violations:
            if rule == "complexity":
                message = (
                    f"Function has a complexity of {measured}. "
                    f"Maximum allowed is {threshold}."
                )
            elif rule == "max-depth":
                message = (
                    f"Blocks are nested too deeply ({measured}). "
                    f"Maximum allowed is {threshold}."
                )
            elif rule == "max-params":
                message = (
                    f"Function has too many parameters ({measured}). "
                    f"Maximum allowed is {threshold}."
                )
            else:
                message = (
                    "Refactor this function to reduce its Cognitive Complexity "
                    f"from {measured} to the {threshold} allowed."
                )
            messages.append(
                canned_eslint_message(
                    rule,
                    line=line,
                    message=message,
                    severity=2,
                )
            )
    return canned_eslint_report(str(target), messages)


def _run_with_report(
    tree: FixtureTree,
    package: str,
    payload: object,
    *,
    eslint_command: str = CLEAN_ESLINT_COMMAND,
    rules: str | None = None,
):
    report = tree.write_json("eslint-report.json", payload)
    arguments = [
        "--package",
        package,
        "--eslint-cmd",
        eslint_command,
        "--report",
        str(report),
    ]
    if rules is not None:
        arguments.extend(("--rules", rules))
    return run_script(ANNOTATOR, *arguments, cwd=tree.root)


def _ordered_rules(violations: Sequence[tuple[str, int, int]]) -> tuple[str, ...]:
    present = {rule for rule, _measured, _threshold in violations}
    return tuple(rule for rule in RULES if rule in present)


def _subsequence_positions(original: bytes, annotated: bytes) -> list[int]:
    original_lines = original.splitlines(keepends=True)
    annotated_lines = annotated.splitlines(keepends=True)
    positions: list[int] = []
    cursor = 0
    for original_line in original_lines:
        while cursor < len(annotated_lines) and annotated_lines[cursor] != original_line:
            cursor += 1
        if cursor == len(annotated_lines):
            raise AssertionError(f"original line missing or modified: {original_line!r}")
        positions.append(cursor)
        cursor += 1
    return positions


def _assert_annotated_sites(
    testcase: unittest.TestCase,
    original: bytes,
    annotated: bytes,
    sites: Sequence[tuple[int, Sequence[tuple[str, int, int]]]],
) -> None:
    original_lines = original.splitlines(keepends=True)
    output_lines = annotated.splitlines(keepends=True)
    positions = _subsequence_positions(original, annotated)
    for line, violations in sites:
        expected_rules = _ordered_rules(violations)
        source_position = positions[line - 1]
        target = original_lines[line - 1].rstrip(b"\r\n")
        expected_indentation = target[: len(target) - len(target.lstrip(b" \t"))]
        expected_prefix = expected_indentation.decode("utf-8")
        directive = output_lines[source_position - 1].decode("utf-8").rstrip("\r\n")
        testcase.assertEqual(
            directive,
            f"{expected_prefix}// eslint-disable-next-line {', '.join(expected_rules)}",
        )
        comment_start = source_position - 1 - len(expected_rules)
        comments = output_lines[comment_start : source_position - 1]
        testcase.assertEqual(len(comments), len(expected_rules))
        for inserted_line in (*comments, output_lines[source_position - 1]):
            content = inserted_line.rstrip(b"\r\n")
            indentation = content[: len(content) - len(content.lstrip(b" \t"))]
            testcase.assertEqual(indentation, expected_indentation)
        measured_by_rule = {rule: measured for rule, measured, _threshold in violations}
        threshold_by_rule = {rule: threshold for rule, _measured, threshold in violations}
        for expected_rule, comment_line in zip(expected_rules, comments):
            parsed = sa_lib.parse_lint_debt(comment_line.decode("utf-8").rstrip("\r\n"))
            testcase.assertIsNotNone(parsed)
            assert parsed is not None
            rule, reason = parsed
            testcase.assertEqual(rule, expected_rule)
            testcase.assertIn(str(measured_by_rule[rule]), reason)
            testcase.assertIn(str(threshold_by_rule[rule]), reason)


class AnnotatorUnitTests(unittest.TestCase):
    def test_cli_exposes_the_required_surface_and_defaults(self) -> None:
        result = run_script(ANNOTATOR, "--help", cwd=TESTS_DIR)

        self.assertEqual(result.exit_code, 0, result.stderr)
        for option in ("--package", "--eslint-cmd", "--rules", "--report"):
            self.assertIn(option, result.stdout)
        self.assertIn("bunx eslint", result.stdout)
        self.assertIn("sonarjs/cognitive-complexity", "".join(result.stdout.split()))

    def test_groups_rules_filters_messages_and_matches_indentation(self) -> None:
        source = (
            b"export function outer() {\n"
            b"  return calculate();\n"
            b"}\n"
        )
        sites = (
            (
                2,
                (
                    ("max-depth", 6, 4),
                    ("complexity", 24, 15),
                ),
            ),
        )
        with FixtureTree(parent=TESTS_DIR) as tree:
            target = tree.write_bytes("packages/app/src/example.ts", source)
            payload = _eslint_report(target, sites)
            messages = payload[0]["messages"]
            assert isinstance(messages, list)
            messages.extend(
                (
                    canned_eslint_message("max-params", line=2, severity=1),
                    canned_eslint_message("no-console", line=2, severity=2),
                )
            )

            result = _run_with_report(tree, "packages/app", payload)

            self.assertEqual(result.exit_code, 0, result.stderr)
            annotated = target.read_bytes()
            _assert_annotated_sites(self, source, annotated, sites)
            self.assertIn(
                b"  // eslint-disable-next-line complexity, max-depth\n",
                annotated,
            )
            self.assertNotIn(b"max-params", annotated)
            self.assertNotIn(b"no-console", annotated)

    def test_multi_line_head_anchors_above_the_declaration(self) -> None:
        source = (
            b"const render = (\n"
            b"  reason: string,\n"
            b"  row?: number\n"
            b"): boolean => {\n"
            b"  return true;\n"
            b"};\n"
        )
        with FixtureTree(parent=TESTS_DIR) as tree:
            target = tree.write_bytes("packages/app/src/example.ts", source)
            payload = canned_eslint_report(
                str(target),
                (canned_eslint_message("complexity", line=4),),
            )

            result = _run_with_report(tree, "packages/app", payload)

            self.assertEqual(result.exit_code, 0, result.stderr)
            annotated = target.read_text(encoding="utf-8").splitlines()
            self.assertTrue(annotated[0].startswith("// LINT-DEBT(complexity):"))
            self.assertEqual(annotated[1], "/* eslint-disable complexity */")
            self.assertEqual(annotated[2], "const render = (")
            self.assertEqual(annotated[5], "): boolean => {")
            self.assertEqual(annotated[6], "/* eslint-enable complexity */")

    def test_single_line_head_keeps_the_next_line_directive(self) -> None:
        source = b"function build(): number {\n  return 1;\n}\n"
        with FixtureTree(parent=TESTS_DIR) as tree:
            target = tree.write_bytes("packages/app/src/example.ts", source)
            payload = canned_eslint_report(
                str(target),
                (canned_eslint_message("complexity", line=1),),
            )

            result = _run_with_report(tree, "packages/app", payload)

            self.assertEqual(result.exit_code, 0, result.stderr)
            annotated = target.read_text(encoding="utf-8")
            self.assertIn("// eslint-disable-next-line complexity\n", annotated)
            self.assertNotIn("eslint-disable-line", annotated)
            self.assertNotIn("eslint-enable", annotated)

    def test_custom_rules_filter_is_honored(self) -> None:
        source = b"export const value = compute();\n"
        with FixtureTree(parent=TESTS_DIR) as tree:
            target = tree.write_bytes("packages/app/src/example.ts", source)
            payload = canned_eslint_report(
                str(target),
                (
                    canned_eslint_message("complexity", line=1),
                    canned_eslint_message(
                        "custom/rule",
                        line=1,
                        message="Measured value is 9; allowed value is 3.",
                    ),
                ),
            )

            result = _run_with_report(
                tree,
                "packages/app",
                payload,
                rules="custom/rule",
            )

            self.assertEqual(result.exit_code, 0, result.stderr)
            annotated = target.read_text(encoding="utf-8")
            self.assertIn("// LINT-DEBT(custom/rule):", annotated)
            self.assertIn("// eslint-disable-next-line custom/rule", annotated)
            self.assertNotIn("LINT-DEBT(complexity)", annotated)

    def test_stubbed_failing_initial_eslint_command_exits_two(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_text("packages/app/src/example.ts", "export const value = 1;\n")

            result = run_script(
                ANNOTATOR,
                "--package",
                "packages/app",
                "--eslint-cmd",
                FAILING_ESLINT_COMMAND,
                cwd=tree.root,
            )

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("initial ESLint command", result.stderr)
            self.assertIn("eslint failed", result.stderr)

    def test_unwritable_target_exits_two_without_modifying_it(self) -> None:
        source = b"export function difficult() { return 1; }\n"
        sites = ((1, (("complexity", 18, 15),)),)
        with FixtureTree(parent=TESTS_DIR) as tree:
            target = tree.write_bytes("packages/app/src/example.ts", source)
            payload = _eslint_report(target, sites)
            target.chmod(stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
            try:
                result = _run_with_report(tree, "packages/app", payload)
            finally:
                target.chmod(stat.S_IRUSR | stat.S_IWUSR)

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("not writable", result.stderr)
            self.assertEqual(target.read_bytes(), source)

    def test_empty_report_leaves_every_package_file_byte_identical(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_bytes("packages/app/src/a.ts", b"export const a = 1;\r\n")
            tree.write_bytes("packages/app/src/b.tsx", b"export const b = <div />;")
            tree.write_bytes("packages/app/assets/data.bin", b"\x00\xff\x01")
            package = tree.path("packages/app")
            before = {
                path.relative_to(package): path.read_bytes()
                for path in package.rglob("*")
                if path.is_file()
            }

            result = _run_with_report(
                tree,
                "packages/app",
                [],
                eslint_command=FAILING_ESLINT_COMMAND,
            )

            after = {
                path.relative_to(package): path.read_bytes()
                for path in package.rglob("*")
                if path.is_file()
            }
            self.assertEqual(result.exit_code, 0, result.stderr)
            self.assertEqual(after, before)

    def test_residual_rule_set_violation_names_the_site_and_exits_two(self) -> None:
        source = b"export function difficult() { return 1; }\n"
        initial_sites = ((1, (("complexity", 18, 15),)),)
        with FixtureTree(parent=TESTS_DIR) as tree:
            target = tree.write_bytes("packages/app/src/example.ts", source)
            initial = _eslint_report(target, initial_sites)
            residual = _eslint_report(target, ((3, (("complexity", 18, 15),)),))
            stub = tree.write_text(
                "residual-eslint.py",
                f"print({json.dumps(residual)!r})\n",
            )

            result = _run_with_report(
                tree,
                "packages/app",
                initial,
                eslint_command=_command(sys.executable, str(stub)),
            )

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("residual Rule_Set violation", result.stderr)
            self.assertIn(f"{target}:3", result.stderr)
            self.assertIn("complexity", result.stderr)

    def test_failing_post_annotation_eslint_command_exits_two(self) -> None:
        source = b"export function difficult() { return 1; }\n"
        sites = ((1, (("complexity", 18, 15),)),)
        with FixtureTree(parent=TESTS_DIR) as tree:
            target = tree.write_bytes("packages/app/src/example.ts", source)
            payload = _eslint_report(target, sites)
            stub = tree.write_text(
                "failing-verification.py",
                "import json\nprint(json.dumps([]))\nraise SystemExit(7)\n",
            )

            result = _run_with_report(
                tree,
                "packages/app",
                payload,
                eslint_command=_command(sys.executable, str(stub)),
            )

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("verification ESLint command", result.stderr)
            self.assertIn("exited 7", result.stderr)

    def test_reported_file_outside_package_is_rejected_without_writes(self) -> None:
        source = b"export function outside() { return 1; }\n"
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_text("packages/app/src/inside.ts", "export const inside = 1;\n")
            outside = tree.write_bytes("outside.ts", source)
            payload = _eslint_report(outside, ((1, (("complexity", 18, 15),)),))

            result = _run_with_report(tree, "packages/app", payload)

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("outside package", result.stderr)
            self.assertEqual(outside.read_bytes(), source)

    def test_report_line_outside_file_is_rejected_without_writes(self) -> None:
        source = b"export const value = 1;\n"
        with FixtureTree(parent=TESTS_DIR) as tree:
            target = tree.write_bytes("packages/app/src/example.ts", source)
            payload = _eslint_report(target, ((2, (("complexity", 18, 15),)),))

            result = _run_with_report(tree, "packages/app", payload)

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("outside file with 1 lines", result.stderr)
            self.assertEqual(target.read_bytes(), source)


if HAS_HYPOTHESIS:
    _source_statement = st.sampled_from(
        (
            "export const alpha = 1;",
            "const beta = alpha + 1;",
            "function gamma(value: number) { return value; }",
            "if (alpha) { console.log(alpha); }",
            "export default gamma;",
        )
    )

    @st.composite
    def annotation_cases(draw):
        line_count = draw(st.integers(min_value=1, max_value=8))
        statements = draw(st.lists(_source_statement, min_size=line_count, max_size=line_count))
        indentations = draw(
            st.lists(
                st.sampled_from(("", "  ", "    ", "\t")),
                min_size=line_count,
                max_size=line_count,
            )
        )
        ending = draw(st.sampled_from(("\n", "\r\n")))
        final_newline = draw(st.booleans())
        rendered = [indent + statement for indent, statement in zip(indentations, statements)]
        source_text = ending.join(rendered) + (ending if final_newline else "")
        selected_lines = draw(
            st.lists(
                st.integers(min_value=1, max_value=line_count),
                min_size=1,
                max_size=line_count,
                unique=True,
            )
        )
        sites: list[tuple[int, tuple[tuple[str, int, int], ...]]] = []
        for line in sorted(selected_lines):
            selected_rules = draw(
                st.lists(
                    st.sampled_from(RULES),
                    min_size=1,
                    max_size=len(RULES),
                    unique=True,
                )
            )
            violations: list[tuple[str, int, int]] = []
            for rule in selected_rules:
                threshold = draw(st.integers(min_value=1, max_value=30))
                measured = draw(st.integers(min_value=threshold + 1, max_value=threshold + 100))
                violations.append((rule, measured, threshold))
            sites.append((line, tuple(violations)))
        return source_text.encode("utf-8"), tuple(sites)

    @st.composite
    def source_trees(draw):
        line_count = draw(st.integers(min_value=1, max_value=12))
        statements = draw(st.lists(_source_statement, min_size=line_count, max_size=line_count))
        ending = draw(st.sampled_from(("\n", "\r\n")))
        final_newline = draw(st.booleans())
        source_text = ending.join(statements) + (ending if final_newline else "")
        return source_text.encode("utf-8")


@unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)
class AnnotatorPropertyTests(unittest.TestCase):
    if HAS_HYPOTHESIS:
        @settings(max_examples=100, deadline=None)
        @given(annotation_cases())
        def test_annotator_covers_every_reported_violation(self, case) -> None:
            source, sites = case
            with FixtureTree(parent=TESTS_DIR) as tree:
                target = tree.write_bytes("packages/app/src/example.ts", source)
                payload = _eslint_report(target, sites)

                result = _run_with_report(tree, "packages/app", payload)

                self.assertEqual(result.exit_code, 0, result.stderr)
                annotated = target.read_bytes()
                _assert_annotated_sites(self, source, annotated, sites)
                self.assertEqual(
                    annotated.count(b"// LINT-DEBT("),
                    sum(len(violations) for _line, violations in sites),
                )

        @unittest.skipUnless(
            DEBT_CHECKER.exists(),
            "the debt checker script is required for this composition test",
        )
        @settings(max_examples=100, deadline=None)
        @given(annotation_cases())
        def test_annotator_composes_with_debt_checker(self, case) -> None:
            source, sites = case
            # Counting every reported violation per rule is at or above the pairs the
            # annotator actually writes, since duplicate rules on one line collapse.
            debt_counts = Counter(
                rule for _line, violations in sites for rule, _measured, _threshold in violations
            )
            with FixtureTree(parent=TESTS_DIR) as tree:
                target = tree.write_bytes("packages/app/src/example.ts", source)
                payload = _eslint_report(target, sites)
                annotation_result = _run_with_report(tree, "packages/app", payload)
                self.assertEqual(annotation_result.exit_code, 0, annotation_result.stderr)
                config = tree.write_json(
                    ".jscpd.json",
                    {
                        "include": ["packages/app/src/**"],
                        "ignore": ["**/*.test.*", "**/tests/**"],
                    },
                )
                baseline = tree.write_json(".lint-debt-baseline.json", dict(debt_counts))

                checker_result = run_script(
                    DEBT_CHECKER,
                    "--root",
                    str(tree.root),
                    "--baseline",
                    str(baseline),
                    "--jscpd-config",
                    str(config),
                    cwd=tree.root,
                )

                self.assertEqual(checker_result.exit_code, 0, checker_result.stderr)

        @settings(max_examples=100, deadline=None)
        @given(annotation_cases())
        def test_annotator_edits_are_insertion_only(self, case) -> None:
            source, sites = case
            with FixtureTree(parent=TESTS_DIR) as tree:
                target = tree.write_bytes("packages/app/src/example.ts", source)
                payload = _eslint_report(target, sites)

                result = _run_with_report(tree, "packages/app", payload)

                self.assertEqual(result.exit_code, 0, result.stderr)
                positions = _subsequence_positions(source, target.read_bytes())
                self.assertEqual(len(positions), len(source.splitlines(keepends=True)))

        @settings(max_examples=100, deadline=None)
        @given(source_trees(), annotation_cases())
        def test_annotator_clean_input_and_annotated_rerun_are_no_ops(
            self,
            clean_source: bytes,
            annotation_case,
        ) -> None:
            annotated_source, sites = annotation_case
            with FixtureTree(parent=TESTS_DIR) as tree:
                clean_target = tree.write_bytes("packages/clean/src/example.ts", clean_source)
                clean_before = clean_target.read_bytes()
                clean_result = _run_with_report(
                    tree,
                    "packages/clean",
                    [],
                    eslint_command=FAILING_ESLINT_COMMAND,
                )
                self.assertEqual(clean_result.exit_code, 0, clean_result.stderr)
                self.assertEqual(clean_target.read_bytes(), clean_before)

                target = tree.write_bytes("packages/app/src/example.ts", annotated_source)
                payload = _eslint_report(target, sites)
                first_result = _run_with_report(tree, "packages/app", payload)
                self.assertEqual(first_result.exit_code, 0, first_result.stderr)
                after_first_run = target.read_bytes()

                second_result = _run_with_report(
                    tree,
                    "packages/app",
                    [],
                    eslint_command=FAILING_ESLINT_COMMAND,
                )
                self.assertEqual(second_result.exit_code, 0, second_result.stderr)
                self.assertEqual(target.read_bytes(), after_first_run)
    else:
        def test_annotator_covers_every_reported_violation(self) -> None:
            pass

        def test_annotator_composes_with_debt_checker(self) -> None:
            pass

        def test_annotator_edits_are_insertion_only(self) -> None:
            pass

        def test_annotator_clean_input_and_annotated_rerun_are_no_ops(self) -> None:
            pass


if __name__ == "__main__":
    unittest.main()
