"""Standard-library tests for the lint-debt checker."""

from __future__ import annotations

import importlib.util
import io
import json
import os
import sys
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import ModuleType
from unittest import mock

TESTS_DIR = Path(__file__).resolve().parent
CODE_QUALITY_DIR = TESTS_DIR.parent
CHECKER_PATH = CODE_QUALITY_DIR / "check-lint-debt.py"
sys.path.insert(0, str(CODE_QUALITY_DIR))

import sa_lib  # noqa: E402
from helpers import FixtureTree, run_script  # noqa: E402


def _configure_tree(
    tree: FixtureTree,
    *,
    baseline: object = None,
    include: list[str] | None = None,
) -> None:
    tree.write_json(
        ".jscpd.json",
        {
            "include": include or ["src/**"],
            "ignore": ["**/*.test.*", "**/tests/**"],
        },
    )
    tree.write_json(".lint-debt-baseline.json", {} if baseline is None else baseline)


# The four-rule fixtures below carry one justified pair per rule.
ALL_FORMS_BASELINE = {
    "complexity": 1,
    "max-depth": 1,
    "max-params": 1,
    "sonarjs/cognitive-complexity": 1,
}


def _run(tree: FixtureTree, *args: str) -> object:
    return run_script(CHECKER_PATH, "--root", tree.root, *args, cwd=tree.root)


def _load_checker_module() -> ModuleType:
    module_name = "check_lint_debt_reporting_test"
    spec = importlib.util.spec_from_file_location(module_name, CHECKER_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load Debt_Checker module")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


class DebtCheckerMandatedTests(unittest.TestCase):
    def test_fully_justified_tree_must_sit_exactly_at_baseline(self) -> None:
        source = """// LINT-DEBT(complexity): existing complexity is tracked
// eslint-disable-next-line complexity -- generated description is not a rule
export const first = 1;
// LINT-DEBT(max-depth): existing nesting is tracked
export const second = 2; // eslint-disable-line max-depth -- trailing description
// LINT-DEBT(max-params): existing signature is tracked
/* eslint-disable max-params */
export const third = 3;
/* eslint-enable max-params */
// LINT-DEBT(sonarjs/cognitive-complexity): existing cognitive complexity is tracked
/* eslint-disable-next-line sonarjs/cognitive-complexity */
export const fourth = 4;
"""
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline=ALL_FORMS_BASELINE)
            tree.write_text("src/all-forms.ts", source)

            at_baseline = _run(tree)
            self.assertEqual(at_baseline.exit_code, sa_lib.EXIT_CLEAN)
            # Zero headroom must be visible on a green run, not discovered on the
            # first red one.
            self.assertIn(
                "measured: lint-debt complexity = 1 (limit 1, headroom 0)",
                at_baseline.stdout,
            )
            self.assertIn(
                "measured: lint-debt total = 4 (limit 4, headroom 0)",
                at_baseline.stdout,
            )
            self.assertNotIn("over-baseline", at_baseline.stdout)
            self.assertEqual(at_baseline.stderr, "")

            tree.write_json(
                ".lint-debt-baseline.json",
                {**ALL_FORMS_BASELINE, "complexity": 2},
            )
            below_baseline = _run(tree)
            # Unspent slack is permission to add debt elsewhere, so the removal
            # has to be banked in the same change.
            self.assertEqual(below_baseline.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn(
                "measured: lint-debt complexity = 1 (limit 2, headroom 1)",
                below_baseline.stdout,
            )
            self.assertIn("under-baseline", below_baseline.stdout)
            self.assertIn("lower the baseline to 1", below_baseline.stdout)
            self.assertEqual(below_baseline.stderr, "")

    def test_baseline_entry_for_a_cleared_rule_must_be_removed(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={"complexity": 1})
            tree.write_text("src/clean.ts", "export const first = 1;\n")

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("or remove the entry", result.stdout)

    def test_block_disable_with_description_at_baseline_exits_zero(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={"max-params": 1})
            tree.write_text(
                "src/block-description.ts",
                "// LINT-DEBT(max-params): existing signature is tracked\n"
                "/* eslint-disable max-params -- pre-existing */\n"
                "export const value = 1;\n"
                "/* eslint-enable max-params */\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_CLEAN)
            self.assertNotIn("over-baseline", result.stdout)
            self.assertEqual(result.stderr, "")

    def test_all_directive_forms_with_crlf_at_baseline_exit_zero(self) -> None:
        source = (
            "// LINT-DEBT(complexity): existing complexity is tracked\r\n"
            "// eslint-disable-next-line complexity -- generated description is not a rule\r\n"
            "export const first = 1;\r\n"
            "// LINT-DEBT(max-depth): existing nesting is tracked\r\n"
            "export const second = 2; // eslint-disable-line max-depth -- trailing description\r\n"
            "// LINT-DEBT(max-params): existing signature is tracked\r\n"
            "/* eslint-disable max-params */\r\n"
            "export const third = 3;\r\n"
            "/* eslint-enable max-params */\r\n"
            "// LINT-DEBT(sonarjs/cognitive-complexity): existing cognitive complexity is tracked\r\n"
            "/* eslint-disable-next-line sonarjs/cognitive-complexity */\r\n"
            "export const fourth = 4;\r\n"
        )
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline=ALL_FORMS_BASELINE)
            tree.write_bytes("src/all-forms-crlf.ts", source.encode("utf-8"))

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_CLEAN)
            self.assertNotIn("over-baseline", result.stdout)
            self.assertEqual(result.stderr, "")

    def test_jsx_block_form_pair_inside_jsx_children_is_justified(self) -> None:
        source = """export const panel = (
  <>
    {/* LINT-DEBT(complexity): pre-existing at gate adoption; tracked in JSX children */}
    {/* eslint-disable-next-line complexity */}
    {items.map((item) => render(item))}
  </>
);
"""
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={"complexity": 1})
            tree.write_text("src/jsx-form.tsx", source)

            result = _run(tree)
            self.assertEqual(result.exit_code, sa_lib.EXIT_CLEAN)
            self.assertNotIn("over-baseline", result.stdout)
            self.assertEqual(result.stderr, "")

    def test_unjustified_disable_exits_nonzero_and_names_file_and_line(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree)
            tree.write_text(
                "src/unjustified.ts",
                "export const first = 1;\n"
                "export const second = 2; // eslint-disable-line complexity\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("src/unjustified.ts:2: unjustified:", result.stdout)
            self.assertIn("complexity", result.stdout)

    def test_malformed_comment_exits_nonzero_and_names_file_and_line(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree)
            tree.write_text(
                "src/malformed.ts",
                "// LINT-DEBT(complexity) missing colon\n"
                "// eslint-disable-next-line complexity\n"
                "export const value = 1;\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("src/malformed.ts:1: malformed:", result.stdout)
            self.assertIn("src/malformed.ts:2: unjustified:", result.stdout)

    def test_count_above_baseline_prints_measured_count_and_baseline(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={"complexity": 1, "max-depth": 1})
            tree.write_text(
                "src/over.ts",
                "// LINT-DEBT(complexity): tracked complexity\n"
                "// LINT-DEBT(max-depth): tracked nesting\n"
                "// eslint-disable-next-line complexity, max-depth -- both are existing debt\n"
                "export const value = 1;\n"
                "// LINT-DEBT(complexity): a second tracked complexity site\n"
                "// eslint-disable-next-line complexity\n"
                "export const second = 2;\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("rule 'complexity' measured debt count 2", result.stdout)
            self.assertIn("baseline 1", result.stdout)
            # max-depth sits exactly at its own allowance, so the complexity overage
            # must not implicate it. Its reading is published either way.
            self.assertNotIn("rule 'max-depth'", result.stdout)
            self.assertIn(
                "measured: lint-debt max-depth = 1 (limit 1, headroom 0)", result.stdout
            )

    def test_paying_down_one_rule_does_not_fund_another(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            # Two pairs against a two-pair total allowance: a single scalar baseline
            # would accept this, so only per-rule accounting rejects the swap.
            _configure_tree(tree, baseline={"complexity": 2, "max-depth": 0})
            tree.write_text(
                "src/traded.ts",
                "// LINT-DEBT(complexity): tracked complexity\n"
                "// LINT-DEBT(max-depth): newly suppressed nesting\n"
                "// eslint-disable-next-line complexity, max-depth -- traded budget\n"
                "export const value = 1;\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("rule 'max-depth' measured debt count 1", result.stdout)
            self.assertIn("baseline 0", result.stdout)
            # The credit the trade tried to spend is itself a violation, so the
            # swap cannot net out to a green run.
            self.assertIn("under-baseline: rule 'complexity'", result.stdout)

    def test_missing_and_unparseable_baselines_exit_two_with_cause(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_json(
                ".jscpd.json",
                {"include": ["src/**"], "ignore": []},
            )
            tree.write_text("src/clean.ts", "export const clean = true;\n")

            missing = _run(tree)
            self.assertEqual(missing.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("cannot read debt baseline", missing.stderr)
            self.assertIn(".lint-debt-baseline.json", missing.stderr)

            tree.write_text(".lint-debt-baseline.json", "{not-json")
            unparseable = _run(tree)
            self.assertEqual(unparseable.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("cannot parse debt baseline", unparseable.stderr)
            self.assertIn("JSONDecodeError", unparseable.stderr)

    def test_unreadable_perimeter_file_records_error_continues_and_exits_two(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree)
            tree.write_text(
                "src/readable.ts",
                "// eslint-disable-next-line complexity\nexport const value = 1;\n",
            )
            broken = tree.path("src/unreadable.ts")
            broken.parent.mkdir(parents=True, exist_ok=True)
            os.symlink("missing-target.ts", broken)

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("src/readable.ts:1: unjustified:", result.stdout)
            self.assertIn("cannot read production file src/unreadable.ts", result.stderr)
            self.assertIn("FileNotFoundError", result.stderr)

    def test_non_text_file_in_the_perimeter_is_not_an_error(self) -> None:
        # The perimeter is include-minus-ignore over every file type, so a binary
        # artefact can land in it; it carries no directive ESLint could honour.
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={"complexity": 1})
            tree.write_text(
                "src/justified.ts",
                "// LINT-DEBT(complexity): tracked\n"
                "// eslint-disable-next-line complexity\n"
                "export const value = 1;\n",
            )
            binary = tree.path("src/artefact.bin")
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.write_bytes(b"\x00\x01\xff\xfe not text")

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_CLEAN, result.stderr)
            self.assertNotIn("cannot read production file", result.stderr)
            self.assertIn("measured: lint-debt complexity = 1", result.stdout)

    def test_stubbed_reporting_failure_still_exits_nonzero(self) -> None:
        checker = _load_checker_module()
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree)
            tree.write_text("src/clean.ts", "export const clean = true;\n")
            stderr = io.StringIO()

            with mock.patch.object(
                checker,
                "emit_results",
                side_effect=OSError("report sink unavailable"),
            ):
                with redirect_stderr(stderr):
                    with self.assertRaises(SystemExit) as raised:
                        sa_lib.run_cli(checker.main, ("--root", str(tree.root)))

            self.assertEqual(raised.exception.code, sa_lib.EXIT_ERROR)
            self.assertIn("report sink unavailable", stderr.getvalue())


class DebtCheckerEdgeCaseTests(unittest.TestCase):
    def test_baseline_shape_is_strict(self) -> None:
        invalid_payloads = (
            [],
            {"complexity": 0, "extra": True},
            {"complexity": -1},
            {"complexity": 1.5},
            {"complexity": True},
            {"complexity": "1"},
        )
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_json(
                ".jscpd.json",
                {"include": ["src/**"], "ignore": []},
            )
            tree.write_text("src/clean.ts", "export const clean = true;\n")

            for payload in invalid_payloads:
                with self.subTest(payload=payload):
                    tree.write_text(
                        ".lint-debt-baseline.json",
                        json.dumps(payload) + "\n",
                    )
                    result = _run(tree)
                    self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
                    self.assertIn("invalid debt baseline", result.stderr)
                    self.assertIn("non-negative integer", result.stderr)

    def test_baseline_rule_names_must_match_the_lint_debt_grammar(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_json(
                ".jscpd.json",
                {"include": ["src/**"], "ignore": []},
            )
            tree.write_text("src/clean.ts", "export const clean = true;\n")

            for rule in ("rule.with.dot", "", "has space"):
                with self.subTest(rule=rule):
                    tree.write_text(
                        ".lint-debt-baseline.json",
                        json.dumps({rule: 1}) + "\n",
                    )
                    result = _run(tree)
                    self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
                    self.assertIn("invalid debt baseline", result.stderr)
                    self.assertIn("outside the LINT-DEBT grammar", result.stderr)

    def test_empty_baseline_allows_no_debt_and_is_not_an_error(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={})
            tree.write_text("src/clean.ts", "export const clean = true;\n")

            clean = _run(tree)
            self.assertEqual(clean.exit_code, sa_lib.EXIT_CLEAN)
            self.assertNotIn("over-baseline", clean.stdout)

            tree.write_text(
                "src/debt.ts",
                "// LINT-DEBT(complexity): tracked complexity\n"
                "// eslint-disable-next-line complexity\n"
                "export const value = 1;\n",
            )
            with_debt = _run(tree)
            self.assertEqual(with_debt.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("rule 'complexity' measured debt count 1", with_debt.stdout)
            self.assertIn("baseline 0", with_debt.stdout)

    def test_bare_disable_is_unjustifiable(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree)
            tree.write_text("src/bare.ts", "/* eslint-disable */\nexport const value = 1;\n")

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("src/bare.ts:1: unjustifiable:", result.stdout)

    def test_unbounded_block_disable_is_a_violation(self) -> None:
        # A justified file-scope disable would count as one unit while exempting
        # the whole file, so only a region closed by eslint-enable is acceptable.
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={"complexity": 1})
            tree.write_text(
                "src/unbounded.ts",
                "// LINT-DEBT(complexity): tracked complexity\n"
                "/* eslint-disable complexity */\n"
                "export const value = 1;\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("src/unbounded.ts:2: unbounded:", result.stdout)
            self.assertIn("no matching", result.stdout)

    def test_block_disable_is_bounded_only_for_rules_the_enable_names(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={"complexity": 1, "max-depth": 1})
            tree.write_text(
                "src/partial.ts",
                "// LINT-DEBT(complexity): tracked complexity\n"
                "// LINT-DEBT(max-depth): tracked nesting\n"
                "/* eslint-disable complexity, max-depth */\n"
                "export const value = 1;\n"
                "/* eslint-enable complexity */\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("src/partial.ts:3: unbounded:", result.stdout)
            self.assertIn("'max-depth'", result.stdout)
            self.assertNotIn("'complexity' has no matching", result.stdout)

    def test_bare_eslint_enable_closes_every_rule(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={"complexity": 1, "max-depth": 1})
            tree.write_text(
                "src/bare-enable.ts",
                "// LINT-DEBT(complexity): tracked complexity\n"
                "// LINT-DEBT(max-depth): tracked nesting\n"
                "/* eslint-disable complexity, max-depth */\n"
                "export const value = 1;\n"
                "/* eslint-enable */\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_CLEAN, result.stdout)
            self.assertNotIn("unbounded", result.stdout)

    def test_rule_mismatch_is_malformed_and_leaves_directive_unjustified(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree)
            tree.write_text(
                "src/mismatch.ts",
                "// LINT-DEBT(max-depth): wrong attached rule\n"
                "// eslint-disable-next-line complexity\n"
                "export const value = 1;\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("src/mismatch.ts:1: malformed:", result.stdout)
            self.assertIn("not named by the attached directive", result.stdout)
            self.assertIn("src/mismatch.ts:2: unjustified:", result.stdout)

    def test_duplicate_comment_is_malformed_and_counts_pair_once(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree, baseline={"complexity": 1})
            tree.write_text(
                "src/duplicate.ts",
                "// LINT-DEBT(complexity): first justification\n"
                "// LINT-DEBT(complexity): duplicate justification\n"
                "// eslint-disable-next-line complexity\n"
                "export const value = 1;\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("src/duplicate.ts:2: malformed:", result.stdout)
            self.assertIn("duplicate LINT-DEBT justification", result.stdout)
            self.assertNotIn("over-baseline", result.stdout)

    def test_stray_lint_debt_block_is_reported(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            _configure_tree(tree)
            tree.write_text(
                "src/stray.ts",
                "// LINT-DEBT(complexity): no attached directive\n"
                "export const value = 1;\n",
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("src/stray.ts:1: stray:", result.stdout)

    def test_perimeter_matching_no_files_exits_two_instead_of_measuring_zero(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_json(".lint-debt-baseline.json", {"complexity": 3})
            tree.write_text(
                "packages/app/src/example.ts",
                "// eslint-disable-next-line complexity\nexport const value = 1;\n",
            )
            tree.write_json(
                ".jscpd.json",
                {"include": ["packages/app/typo/**"], "ignore": []},
            )

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("matches no files", result.stderr)
            # A count taken over an unresolved perimeter is not a reading; publishing
            # `measured: ... = 0` here would dress the exit-2 run as a passing one.
            self.assertNotIn("measured:", result.stdout)

    def test_one_bogus_include_suppresses_readings_and_their_file(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_json(".lint-debt-baseline.json", {"complexity": 1})
            tree.write_text(
                "src/example.ts",
                "// LINT-DEBT(complexity): tracked complexity\n"
                "// eslint-disable-next-line complexity\n"
                "export const value = 1;\n",
            )
            tree.write_json(
                ".jscpd.json",
                {"include": ["src/**", "no-such-tree/**"], "ignore": []},
            )

            result = _run(tree, "--measurements-out", "readings.json")

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertNotIn("measured:", result.stdout)
            self.assertFalse(tree.path("readings.json").exists())

    def test_missing_perimeter_config_exits_two_with_cause(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_json(".lint-debt-baseline.json", {})

            result = _run(tree)

            self.assertEqual(result.exit_code, sa_lib.EXIT_ERROR)
            self.assertIn("cannot load production-code perimeter", result.stderr)
            self.assertIn("cannot read perimeter config", result.stderr)


if __name__ == "__main__":
    unittest.main()
